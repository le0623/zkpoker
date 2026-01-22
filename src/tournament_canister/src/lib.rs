use authentication::validate_caller;
use candid::{Nat, Principal};
use canister_functions::{cycle::top_up_canister, rake_constants::RAKE_WALLET_ADDRESS_PRINCIPAL};
use currency::{state::TransactionState, types::currency_manager::CurrencyManager, Currency};
use errors::{
    canister_management_error::CanisterManagementError, tournament_error::TournamentError,
};
use ic_cdk::management_canister::{canister_status, CanisterStatusArgs};
use intercanister_call_wrappers::{
    tournament_canister::{
        add_to_table_pool_wrapper, distribute_winnings_wrapper, ensure_principal_is_controller,
        handle_cancelled_tournament_wrapper, return_all_cycles_to_tournament_index_wrapper,
        user_leave_tournament_wrapper,
    },
    tournament_index::request_withdrawal_wrapper,
    users_canister::get_user_wrapper,
};
use lazy_static::lazy_static;
use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};
use table::{
    poker::game::{
        table_functions::{
            table::{TableConfig, TableId, TableType},
            types::CurrencyType,
        },
        types::PublicTable,
    },
    table_canister::{clear_table, get_table_wrapper, join_table, leave_table_wrapper},
};
use table_balancing::{check_and_balance_tables, move_player_to_table};
use tournaments::tournaments::{
    table_balancing::get_balance_interval,
    tournament_type::{TournamentSizeType, TournamentType},
    types::{
        TournamentData, TournamentId, TournamentState, UserTournamentAction, UserTournamentData,
    },
    utils::calculate_rake,
};
use user::user::{UsersCanisterId, WalletPrincipalId};
use utils::{
    add_to_tournament_prize_pool, handle_addon, handle_cycle_check_async, handle_invalid_join,
    handle_lost_user_rebuy_availability, handle_rebuy, handle_reentry, handle_refund,
    handle_tournament_deposit, transfer_cycles_to_tournament_index, update_live_leaderboard,
    update_tournament_state, LEADERBOARD_UPDATE_INTERVAL,
};

pub mod heartbeat;
pub mod memory;
pub mod table_balancing;
pub mod utils;

// Define a global instance of GameState wrapped in a Mutex for safe concurrent access.
lazy_static! {
    static ref LAST_HEARTBEAT: AtomicU64 = AtomicU64::new(0);

    static ref TOURNAMENT: Mutex<Option<TournamentData>> = Mutex::new(None);
    static ref TOURNAMENT_INDEX: Mutex<Option<Principal>> = Mutex::new(None);

    static ref LEADERBOARD: Mutex<Vec<WalletPrincipalId>> = Mutex::new(Vec::new());
    static ref LIVE_LEADERBOARD: Mutex<Vec<(WalletPrincipalId, u64)>> = Mutex::new(Vec::new());
    static ref LAST_LEADERBOARD_UPDATE: AtomicU64 = AtomicU64::new(0);

    static ref PRIZE_POOL: AtomicU64 = AtomicU64::new(0); // New atomic for prize pool
    static ref RAKE_AMOUNT: AtomicU64 = AtomicU64::new(0); // New atomic for prize pool

    static ref TOURNAMENT_START_TIME: AtomicU64 = AtomicU64::new(u64::MAX);

    static ref LAST_BALANCE_TIMESTAMP: AtomicU64 = AtomicU64::new(0);
    static ref TRANSACTION_STATE: Mutex<TransactionState> = Mutex::new(TransactionState::new());
    static ref CKUSDC_LEDGER_CANISTER_ID: Principal = Principal::from_text("xevnm-gaaaa-aaaar-qafnq-cai").unwrap();

    static ref CONTROLLER_PRINCIPALS: Vec<Principal> = vec![
        Principal::from_text("km7qz-4bai4-e5ptx-hgrck-z3web-ameqg-ksxcf-u7wbr-t5fna-i7bqp-hqe").unwrap(),
        Principal::from_text("uyxh5-bi3za-gxbfs-op3gj-ere73-a6jhv-5jky3-zawef-b5r2s-k26un-sae").unwrap(),
        Principal::from_text("w3kjy-pitqg-dvab7-tb57q-63gnd-di4vo-loiiy-s6zm2-gqcmw-ixliz-aae").unwrap(),
        Principal::from_text("tcuxo-b5b4t-vxwo7-mwgxi-vb4ig-zuux4-jmvru-fjocv-4uxuz-7yo4v-hqe").unwrap(),
        Principal::from_text("2hbym-ivof6-l2yyh-zgu62-fqgf3-nvtlj-edso2-ebahf-p5j5c-2p2eb-lae").unwrap(),
    ];
    static ref TABLE_CANISTER_WASM: &'static [u8] = include_bytes!("../../../target/wasm32-unknown-unknown/release/table_canister.wasm");

    static ref CURRENCY_MANAGER: Mutex<CurrencyManager> = Mutex::new(CurrencyManager::new());

    static ref DEPOSITORS: Mutex<Vec<(WalletPrincipalId, u64)>> = Mutex::new(Vec::new());
}

#[ic_cdk::init]
fn init() {
    let principal = ic_cdk::api::canister_self();
    ic_cdk::println!("Tournament canister {} initialized", principal);
}

#[ic_cdk::update]
async fn create_tournament(
    config: TournamentData,
    table_config: TableConfig,
    prize_pool: u64,
) -> Result<TournamentData, TournamentError> {
    let mut table_config = table_config;
    config.validate()?;
    match &config.tournament_type {
        TournamentType::BuyIn(buy_in_type) => match buy_in_type {
            TournamentSizeType::SingleTable(_) => {
                table_config.table_type = Some(TableType::Tournament {
                    tournament_id: ic_cdk::api::canister_self(),
                    is_final_table: true,
                });
            }
            TournamentSizeType::MultiTable(_, _) => {
                table_config.table_type = Some(TableType::Tournament {
                    tournament_id: ic_cdk::api::canister_self(),
                    is_final_table: false,
                });
            }
        },
        TournamentType::SitAndGo(buy_in_type) => match buy_in_type {
            TournamentSizeType::SingleTable(_) => {
                table_config.table_type = Some(TableType::Tournament {
                    tournament_id: ic_cdk::api::canister_self(),
                    is_final_table: true,
                });
            }
            _ => {
                return Err(TournamentError::Other(
                    "Unsupported SitAndGo tournament type".to_string(),
                ))
            }
        },
        TournamentType::SpinAndGo(buy_in_type, _) => match buy_in_type {
            TournamentSizeType::SingleTable(_) => {
                table_config.table_type = Some(TableType::Tournament {
                    tournament_id: ic_cdk::api::canister_self(),
                    is_final_table: true,
                });
            }
            _ => {
                return Err(TournamentError::Other(
                    "Unsupported SpinAndGo tournament type".to_string(),
                ))
            }
        },
        TournamentType::Freeroll(buy_in_type) => match buy_in_type {
            TournamentSizeType::SingleTable(_) => {
                table_config.table_type = Some(TableType::Tournament {
                    tournament_id: ic_cdk::api::canister_self(),
                    is_final_table: true,
                });
            }
            TournamentSizeType::MultiTable(_, _) => {
                table_config.table_type = Some(TableType::Tournament {
                    tournament_id: ic_cdk::api::canister_self(),
                    is_final_table: false,
                });
            }
        },
    }

    table_config.is_paused = Some(true);

    {
        let mut tournament_index = TOURNAMENT_INDEX
            .lock()
            .map_err(|_| TournamentError::LockError)?;
        if let Some(tournament_index) = *tournament_index {
            if tournament_index != ic_cdk::api::msg_caller() {
                return Err(TournamentError::InvalidState(
                    "Tournament already started".to_string(),
                ));
            }
        }
        *tournament_index = Some(ic_cdk::api::msg_caller());

        let mut tournament_state = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;

        *tournament_state = Some(config.clone());
        tournament_state
            .as_mut()
            .ok_or(TournamentError::TournamentNotFound)?
            .table_config = table_config.clone();

        TOURNAMENT_START_TIME.store(config.start_time, Ordering::Relaxed);
        PRIZE_POOL.store(prize_pool, Ordering::Relaxed);
        LAST_BALANCE_TIMESTAMP.store(ic_cdk::api::time(), Ordering::Relaxed);
    }

    let mut currency_manager = {
        CURRENCY_MANAGER
            .lock()
            .map_err(|_| TournamentError::LockError)?
            .clone()
    };
    if let CurrencyType::Real(currency) = config.currency {
        currency_manager.add_currency(currency).await.map_err(|e| {
            TournamentError::InvalidConfiguration(format!(
                "Failed to add tournament currency {:?}",
                e
            ))
        })?;
    }
    *CURRENCY_MANAGER
        .lock()
        .map_err(|_| TournamentError::LockError)? = currency_manager;
    Ok(config)
}

#[ic_cdk::query]
fn get_tournament() -> Result<TournamentData, TournamentError> {
    let tournament_state = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
    match tournament_state.as_ref() {
        Some(tournament_state) => Ok(tournament_state.clone()),
        None => Err(TournamentError::TournamentNotFound),
    }
}

#[ic_cdk::update]
async fn cancel_tournament() -> Result<(), TournamentError> {
    let mut valid_callers = CONTROLLER_PRINCIPALS.clone();
    {
        let mut tournament = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
        let tournament = tournament
            .as_mut()
            .ok_or(TournamentError::TournamentNotFound)?;
        valid_callers.push(tournament.id.0);
    }
    validate_caller(valid_callers);

    update_tournament_state(TournamentState::Cancelled).await?;
    let id = TournamentId(ic_cdk::api::canister_self());
    ic_cdk::futures::spawn(async move {
        match handle_cancelled_tournament_wrapper(id).await {
            Ok(_) => {}
            Err(e) => ic_cdk::println!("Error handling cancelled tournament: {:?}", e),
        }
    });
    Err(TournamentError::Other(
        "Not enough players to start tournament".to_string(),
    ))
}

#[ic_cdk::query]
fn ping() -> String {
    "Ok".to_string()
}

#[ic_cdk::update]
async fn user_join_tournament(
    users_canister_principal: UsersCanisterId,
    user_id: WalletPrincipalId,
) -> Result<(), TournamentError> {
    handle_cycle_check_async().await;

    let tournament_state = {
        let tournament_state = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
        let tournament_state = tournament_state.as_ref();
        match tournament_state {
            Some(tournament_state) => tournament_state,
            None => return Err(TournamentError::TournamentNotFound),
        }
        .clone()
    };
    if tournament_state.require_proof_of_humanity {
        let user = get_user_wrapper(users_canister_principal, user_id).await?;
        if !user.is_verified.unwrap_or(false) {
            return Err(TournamentError::UserNotVerified);
        }
    }

    {
        let user = get_user_wrapper(users_canister_principal, user_id).await?;
        if !user.can_play() {
            return Err(TournamentError::UserBanned);
        }
    }

    let tournament_state: TournamentData = {
        let mut tournament_state = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
        let tournament_state = tournament_state.as_mut();
        let tournament_state = match tournament_state {
            Some(tournament_state) => tournament_state,
            None => return Err(TournamentError::TournamentNotFound),
        };

        if tournament_state.is_full() {
            return Err(TournamentError::TournamentFull);
        }

        if tournament_state.state != TournamentState::Registration
            && tournament_state.state != TournamentState::LateRegistration
        {
            return Err(TournamentError::RegistrationClosed);
        }

        if tournament_state.current_players.contains_key(&user_id) {
            return Err(TournamentError::AlreadyRegistered);
        }

        tournament_state.current_players.insert(
            user_id,
            UserTournamentData::new(
                users_canister_principal,
                tournament_state.starting_chips,
                tournament_state.current_players.len() as u32,
            ),
        );

        tournament_state.clone()
    };

    let currency_type = match tournament_state.currency {
        CurrencyType::Real(currency) => currency.to_string(),
        CurrencyType::Fake => "Fake".to_string(),
    };

    if !matches!(
        tournament_state.tournament_type,
        TournamentType::Freeroll(_)
    ) {
        if let Err(e) =
            handle_tournament_deposit(tournament_state.currency, tournament_state.buy_in, user_id)
                .await
        {
            handle_invalid_join(
                user_id,
                currency_type.clone(),
                false,
                TournamentError::CanisterCallError(format!("{:?}", e)),
            )?;
        }
    }

    if tournament_state.state != TournamentState::Registration {
        let mut table_id = TableId::default();
        for table in tournament_state.tables {
            if table.1.players.len() < tournament_state.table_config.seats as usize {
                table_id = table.0;
                break;
            }
        }
        if table_id == Principal::anonymous() {
            return Err(TournamentError::TableNotFound);
        }
        let res = join_table(
            table_id,
            users_canister_principal,
            user_id,
            None,
            tournament_state.starting_chips,
            true,
        )
        .await;

        match res {
            Ok(_) => (),
            Err(e) => {
                ic_cdk::println!("Error joining table: {:?}", e);
                handle_invalid_join(
                    user_id,
                    currency_type,
                    true,
                    TournamentError::CanisterCallError(format!("{:?}", e)),
                )?;
            }
        }
    }

    if !matches!(
        tournament_state.tournament_type,
        TournamentType::Freeroll(_)
    ) {
        add_to_tournament_prize_pool(tournament_state.buy_in, false)?;
    }

    Ok(())
}

#[ic_cdk::update]
async fn handle_cancelled_tournament() -> Result<(), TournamentError> {
    handle_cycle_check_async().await;
    let mut valid_callers = CONTROLLER_PRINCIPALS.clone();
    {
        let mut tournament = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
        let tournament = tournament
            .as_mut()
            .ok_or(TournamentError::TournamentNotFound)?;
        valid_callers.push(tournament.id.0);
    }
    validate_caller(valid_callers);

    let tournament = {
        let mut tournament = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
        let tournament = tournament
            .as_mut()
            .ok_or(TournamentError::TournamentNotFound)?;

        tournament.clone()
    };
    update_tournament_state(TournamentState::Cancelled).await?;

    for (user_principal, tournament_data) in tournament.current_players.iter() {
        let user = match get_user_wrapper(tournament_data.users_canister_principal, *user_principal)
            .await
        {
            Ok(user) => user,
            Err(e) => {
                ic_cdk::println!("Error getting user: {:?}", e);
                continue;
            }
        };
        let table_id = tournament
            .tables
            .iter()
            .find(|(_, table)| table.players.contains(user_principal))
            .map(|(table_id, _)| *table_id);
        if let Some(table_id) = table_id {
            match user_leave_tournament_wrapper(
                TournamentId(ic_cdk::api::canister_self()),
                user.users_canister_id,
                *user_principal,
                table_id,
            )
            .await
            {
                Ok(_) => (),
                Err(e) => ic_cdk::println!("Error leaving table: {:?}", e),
            }
        }
    }

    let depositors = {
        DEPOSITORS
            .lock()
            .map_err(|_| TournamentError::LockError)?
            .clone()
    };
    let currency_manager = {
        CURRENCY_MANAGER
            .lock()
            .map_err(|_| TournamentError::LockError)?
            .clone()
    };
    let currency = match tournament.currency {
        CurrencyType::Real(currency) => currency,
        _ => {
            return Err(TournamentError::InvalidConfiguration(
                "Invalid currency type".to_string(),
            ))
        }
    };
    // let depositors = depositors.as_ref();
    for (wallet_id, amount) in depositors.iter() {
        if let Err(e) = currency_manager
            .withdraw(&currency, wallet_id.0, *amount)
            .await
        {
            ic_cdk::println!("Error refunding user: {:?}", e);
        }
    }

    ic_cdk::futures::spawn(async move {
        if let Err(e) = return_all_cycles_to_tournament_index_wrapper(TournamentId(
            ic_cdk::api::canister_self(),
        ))
        .await
        {
            ic_cdk::println!("Error returning cycles to tournament index: {:?}", e);
        }
    });
    Ok(())
}

#[ic_cdk::update]
async fn user_leave_tournament(
    users_canister_id: UsersCanisterId,
    user_id: WalletPrincipalId,
) -> Result<(), TournamentError> {
    handle_cycle_check_async().await;
    let tournament = {
        let mut tournament_state = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
        let tournament_state = tournament_state.as_mut();

        if let Some(tournament_state) = tournament_state {
            validate_caller(vec![tournament_state.id.0, users_canister_id.0, user_id.0]);
            tournament_state.current_players.remove(&user_id);
            tournament_state.clone()
        } else {
            return Err(TournamentError::TournamentNotFound);
        }
    };

    let mut table_id = TableId::default();
    for table in tournament.tables {
        if table.1.players.contains(&user_id) {
            table_id = table.0;
            break;
        }
    }
    if table_id != Principal::anonymous() {
        let _ = leave_table_wrapper(table_id, users_canister_id, user_id).await?;
    }

    if !matches!(tournament.tournament_type, TournamentType::Freeroll(_)) {
        match tournament.currency {
            CurrencyType::Real(currency) => {
                let currency_type = currency.to_string();
                let (prize_pool, rake_amount) = calculate_rake(tournament.buy_in)?;

                if let Err(e) = handle_refund(user_id, tournament.buy_in, currency_type) {
                    ic_cdk::println!("Error refunding user: {:?}", e);
                }
                PRIZE_POOL.fetch_sub(prize_pool, Ordering::SeqCst);
                RAKE_AMOUNT.fetch_sub(rake_amount, Ordering::SeqCst);
            }
            CurrencyType::Fake => (),
        }
    }

    Ok(())
}

#[ic_cdk::update]
async fn user_reentry_into_tournament(
    users_canister_id: UsersCanisterId,
    user_id: WalletPrincipalId,
    table_id: TableId,
) -> Result<(), TournamentError> {
    handle_cycle_check_async().await;
    let tournament_state = {
        let tournament = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
        tournament
            .as_ref()
            .ok_or(TournamentError::TournamentNotFound)?
            .clone()
    };

    let currency_type = match tournament_state.currency {
        CurrencyType::Real(currency) => currency.to_string(),
        CurrencyType::Fake => "Fake".to_string(),
    };

    {
        if !tournament_state.all_players.contains_key(&user_id) {
            return Err(TournamentError::NotRegistered);
        } else if tournament_state.current_players.contains_key(&user_id) {
            return Err(TournamentError::AlreadyRegistered);
        }
    }

    match tournament_state.tournament_type.clone() {
        TournamentType::BuyIn(buy_in_type) => {
            handle_reentry(
                buy_in_type,
                users_canister_id,
                user_id,
                table_id,
                &tournament_state,
                currency_type,
            )
            .await?
        }
        TournamentType::SitAndGo(buy_in_type) => {
            handle_reentry(
                buy_in_type,
                users_canister_id,
                user_id,
                table_id,
                &tournament_state,
                currency_type,
            )
            .await?
        }
        TournamentType::Freeroll(buy_in_type) => {
            handle_reentry(
                buy_in_type,
                users_canister_id,
                user_id,
                table_id,
                &tournament_state,
                currency_type,
            )
            .await?
        }
        _ => {
            return Err(TournamentError::Other(
                "Unsupported tournament type".to_string(),
            ))
        }
    };

    {
        let mut tournament_state = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
        let tournament_state = tournament_state
            .as_mut()
            .ok_or(TournamentError::TournamentNotFound)?;
        let tournament_data = {
            let tournament_data = tournament_state.get_user_tournament_data_mut(&user_id)?;
            tournament_data.reentries += 1;
            tournament_data.clone()
        };

        tournament_state
            .current_players
            .insert(user_id, tournament_data.clone());

        // Remove user from leaderboard
        let mut leaderboard = LEADERBOARD.lock().map_err(|_| TournamentError::LockError)?;
        leaderboard.retain(|&x| x != user_id);
    }

    Ok(())
}

#[ic_cdk::update]
async fn user_rebuy_into_tournament(
    users_canister_id: UsersCanisterId,
    user_id: WalletPrincipalId,
    table_id: TableId,
) -> Result<(), TournamentError> {
    handle_cycle_check_async().await;
    let tournament_state = {
        let tournament = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
        tournament
            .as_ref()
            .ok_or(TournamentError::TournamentNotFound)?
            .clone()
    };

    let currency_type = match tournament_state.currency {
        CurrencyType::Real(currency) => currency.to_string(),
        CurrencyType::Fake => "Fake".to_string(),
    };

    if !tournament_state.current_players.contains_key(&user_id) {
        return Err(TournamentError::NotRegistered);
    } else if tournament_state.get_user_tournament_data(&user_id)?.chips
        >= tournament_state.get_current_blinds().0
    {
        return Err(TournamentError::RebuyNotAllowed(
            "User has enough chips to continue".to_string(),
        ));
    }

    match tournament_state.tournament_type.clone() {
        TournamentType::BuyIn(buy_in_type) => {
            handle_rebuy(
                buy_in_type,
                users_canister_id,
                user_id,
                table_id,
                &tournament_state,
                currency_type,
            )
            .await?
        }
        TournamentType::SitAndGo(buy_in_type) => {
            handle_rebuy(
                buy_in_type,
                users_canister_id,
                user_id,
                table_id,
                &tournament_state,
                currency_type,
            )
            .await?
        }
        TournamentType::Freeroll(buy_in_type) => {
            handle_rebuy(
                buy_in_type,
                users_canister_id,
                user_id,
                table_id,
                &tournament_state,
                currency_type,
            )
            .await?
        }
        _ => {
            return Err(TournamentError::Other(
                "Unsupported tournament type".to_string(),
            ))
        }
    };

    {
        let mut tournament_state = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
        let tournament_state = tournament_state
            .as_mut()
            .ok_or(TournamentError::TournamentNotFound)?;
        tournament_state
            .get_user_tournament_data_mut(&user_id)?
            .rebuys += 1;
    }

    Ok(())
}

#[ic_cdk::update]
async fn user_refill_chips(
    users_canister_id: UsersCanisterId,
    table_id: TableId,
    user_id: WalletPrincipalId,
) -> Result<(), TournamentError> {
    handle_cycle_check_async().await;
    let tournament = {
        let tournament = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
        tournament
            .as_ref()
            .ok_or(TournamentError::TournamentNotFound)?
            .clone()
    };

    let currency_type = match tournament.currency {
        CurrencyType::Real(currency) => currency.to_string(),
        CurrencyType::Fake => "Fake".to_string(),
    };

    match tournament.tournament_type.clone() {
        TournamentType::BuyIn(buy_in_type) => {
            handle_addon(
                &buy_in_type,
                users_canister_id,
                user_id,
                table_id,
                currency_type,
                &tournament,
            )
            .await?
        }
        TournamentType::SitAndGo(buy_in_type) => {
            handle_addon(
                &buy_in_type,
                users_canister_id,
                user_id,
                table_id,
                currency_type,
                &tournament,
            )
            .await?
        }
        TournamentType::Freeroll(buy_in_type) => {
            handle_addon(
                &buy_in_type,
                users_canister_id,
                user_id,
                table_id,
                currency_type,
                &tournament,
            )
            .await?
        }
        _ => {
            return Err(TournamentError::Other(
                "Unsupported tournament type".to_string(),
            ))
        }
    };

    let mut tournament = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
    let tournament = tournament
        .as_mut()
        .ok_or(TournamentError::TournamentNotFound)?;
    tournament.get_user_tournament_data_mut(&user_id)?.addons += 1;
    Ok(())
}

#[ic_cdk::update]
async fn distribute_winnings(table: PublicTable) -> Result<(), TournamentError> {
    handle_cycle_check_async().await;
    let mut valid_callers = CONTROLLER_PRINCIPALS.clone();
    {
        let mut tournament = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
        let tournament = tournament
            .as_mut()
            .ok_or(TournamentError::TournamentNotFound)?;
        valid_callers.push(tournament.id.0);
    }
    validate_caller(valid_callers);

    let tournament = {
        let mut tournament = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
        let tournament = tournament
            .as_mut()
            .ok_or(TournamentError::TournamentNotFound)?;

        if tournament.state != TournamentState::Completed {
            return Err(TournamentError::InvalidState(
                "Tournament not completed".to_string(),
            ));
        }

        tournament.clone()
    };
    update_tournament_state(TournamentState::Completed).await?;

    let total_prize = PRIZE_POOL.load(Ordering::SeqCst);
    ic_cdk::println!(
        "Distributing winnings for tournament {} with total prize: {}",
        tournament.id.0.to_text(),
        total_prize
    );
    let positions: Vec<WalletPrincipalId> = {
        let mut leaderboard = LEADERBOARD.lock().map_err(|_| TournamentError::LockError)?;

        // Get remaining players sorted by balance
        let mut active_players: Vec<_> = table
            .users
            .users
            .iter()
            .map(|(principal, user)| (*principal, user.balance))
            .collect();

        // Sort by balance in descending order
        active_players.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));

        // Combine active players with eliminated players
        let positions = active_players
            .into_iter()
            .map(|(principal, _)| principal)
            .collect::<Vec<_>>();

        for position in positions {
            leaderboard.push(position);
        }

        leaderboard.iter().rev().copied().collect()
    };

    {
        let mut tournament = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
        let tournament = tournament
            .as_mut()
            .ok_or(TournamentError::TournamentNotFound)?;

        if tournament.state != TournamentState::Completed {
            return Err(TournamentError::InvalidState(
                "Tournament not completed".to_string(),
            ));
        }

        let mut positions = positions.iter().map(|p| (*p, 0)).collect::<Vec<_>>();
        for (i, payout) in tournament.payout_structure.payouts.iter().enumerate() {
            let prize_amount = (total_prize * payout.percentage as u64) / 100;
            positions[i].1 = prize_amount;
        }

        tournament.sorted_users = Some(positions);
    }

    if let CurrencyType::Real(currency) = tournament.currency {
        let currency_manager = {
            CURRENCY_MANAGER
                .lock()
                .map_err(|_| TournamentError::LockError)?
                .clone()
        };

        // Distribute according to payout structure
        for (position, payout) in tournament.payout_structure.payouts.iter().enumerate() {
            ic_cdk::println!(
                "Distributing {}% to position {}",
                payout.percentage,
                position + 1
            );
            if position < positions.len() {
                let user_id = positions[position];

                let prize_amount = (total_prize * payout.percentage as u64) / 100;

                currency_manager
                    .withdraw(&currency, user_id.0, prize_amount)
                    .await
                    .map_err(|e| TournamentError::CanisterCallError(format!("{:?}", e)))?;

                ic_cdk::println!(
                    "Distributed {} to user {}",
                    prize_amount,
                    user_id.0.to_text()
                );
            }
        }

        let rake = RAKE_AMOUNT.load(Ordering::SeqCst);
        let tournament_index = match TOURNAMENT_INDEX.lock() {
            Ok(tournament_index) => match tournament_index.as_ref() {
                Some(tournament_index) => *tournament_index,
                None => {
                    return Err(TournamentError::InvalidState(
                        "Tournament index not found".to_string(),
                    ))
                }
            },
            Err(e) => {
                ic_cdk::println!("Error getting tournament index: {:?}", e);
                return Err(TournamentError::LockError);
            }
        };
        ic_cdk::println!("Rake: {}", rake);
        if rake > 0 {
            match tournament.currency {
                CurrencyType::Real(Currency::ICP)
                | CurrencyType::Fake
                | CurrencyType::Real(Currency::BTC) => {
                    currency_manager
                        .withdraw(&currency, tournament_index, rake)
                        .await
                        .map_err(|e| TournamentError::CanisterCallError(format!("{:?}", e)))?;
                    ic_cdk::println!("Distributed {} to tournament index", rake);
                    let balance = currency_manager
                        .get_balance(&currency, ic_cdk::api::canister_self())
                        .await
                        .map_err(|e| TournamentError::CanisterCallError(format!("{:?}", e)))?;
                    ic_cdk::println!("Balance: {}", balance);
                    if balance > 0 {
                        currency_manager
                            .withdraw(&currency, tournament_index, balance as u64)
                            .await
                            .map_err(|e| TournamentError::CanisterCallError(format!("{:?}", e)))?;
                        ic_cdk::println!("Distributed {} to tournament index", balance);
                    }
                }
                _ => {
                    let rake_wallet = Principal::from_text(RAKE_WALLET_ADDRESS_PRINCIPAL).unwrap();
                    currency_manager
                        .withdraw(&currency, rake_wallet, rake)
                        .await
                        .map_err(|e| TournamentError::CanisterCallError(format!("{:?}", e)))?;
                    ic_cdk::println!("Distributed {} to tournament index", rake);
                    let balance = currency_manager
                        .get_balance(&currency, ic_cdk::api::canister_self())
                        .await
                        .map_err(|e| TournamentError::CanisterCallError(format!("{:?}", e)))?;
                    ic_cdk::println!("Balance: {}", balance);
                    if balance > 0 {
                        currency_manager
                            .withdraw(&currency, rake_wallet, balance as u64)
                            .await
                            .map_err(|e| TournamentError::CanisterCallError(format!("{:?}", e)))?;
                        ic_cdk::println!("Distributed {} to tournament index", balance);
                    }
                }
            }
        }
    }
    ic_cdk::futures::spawn(async move {
        if let Err(e) = return_all_cycles_to_tournament_index_wrapper(TournamentId(
            ic_cdk::api::canister_self(),
        ))
        .await
        {
            ic_cdk::println!("Error returning cycles to tournament index: {:?}", e);
        }
    });
    Ok(())
}

#[ic_cdk::update]
async fn handle_tournament_end() -> Result<(), TournamentError> {
    let mut valid_callers = CONTROLLER_PRINCIPALS.clone();
    {
        let mut tournament = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
        let tournament = tournament
            .as_mut()
            .ok_or(TournamentError::TournamentNotFound)?;

        tournament.calculate_payouts()?;

        ic_cdk::println!("Payout structure:");
        for (position, payout) in tournament.payout_structure.payouts.iter().enumerate() {
            ic_cdk::println!(
                "Position: {}, Percentage: {}",
                position + 1,
                payout.percentage
            );
        }

        ic_cdk::println!("---------------- Tournament end all players contains:");
        for (player, data) in &tournament.all_players {
            ic_cdk::println!("Player: {:?}, data: {:?}", player.0.to_text(), data);
        }

        ic_cdk::println!("---------------- Tournament end current players contains:");
        for (player, data) in &tournament.current_players {
            ic_cdk::println!("Player: {:?}, data: {:?}", player.0.to_text(), data);
        }
        if let Some(guaranteed_prize_pool) = tournament.guaranteed_prize_pool {
            ic_cdk::println!("Guaranteed prize pool: {}", guaranteed_prize_pool);
            let prize_pool = PRIZE_POOL.load(Ordering::SeqCst);
            let tournament_index = {
                TOURNAMENT_INDEX
                    .lock()
                    .map_err(|_| TournamentError::LockError)?
                    .clone()
                    .ok_or(TournamentError::InvalidState(
                        "Tournament index not found".to_string(),
                    ))?
            };
            if prize_pool < guaranteed_prize_pool && tournament.currency != CurrencyType::Fake {
                ic_cdk::println!(
                    "Prize pool {} is less than guaranteed prize pool {}",
                    prize_pool,
                    guaranteed_prize_pool
                );
                let currency = match tournament.currency {
                    CurrencyType::Real(currency) => currency,
                    CurrencyType::Fake => {
                        return Err(TournamentError::InvalidConfiguration(
                            "Fake currency cannot have guaranteed prize pool".to_string(),
                        ))
                    }
                };
                request_withdrawal_wrapper(
                    tournament_index,
                    currency,
                    guaranteed_prize_pool - prize_pool,
                )
                .await
                .map_err(|e| TournamentError::CanisterCallError(format!("{:?}", e)))?;

                PRIZE_POOL.store(guaranteed_prize_pool, Ordering::SeqCst);
            }
        }
        valid_callers.push(tournament.id.0);
    }
    validate_caller(valid_callers);

    let table = {
        let mut tournament = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
        let tournament = tournament
            .as_mut()
            .ok_or(TournamentError::TournamentNotFound)?;

        *tournament
            .tables
            .iter()
            .next()
            .ok_or(TournamentError::TableNotFound)?
            .0
    };
    update_tournament_state(TournamentState::Completed).await?;
    let table = get_table_wrapper(table).await?;

    ic_cdk::futures::spawn(async move {
        if let Err(e) =
            distribute_winnings_wrapper(TournamentId(ic_cdk::api::canister_self()), table).await
        {
            ic_cdk::println!("Error distributing winnings: {:?}", e);
        }
    });

    let tournament = {
        // Delete all tables
        let tournament = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
        tournament
            .as_ref()
            .ok_or(TournamentError::TournamentNotFound)?
            .clone()
    };

    for (table_id, _) in tournament.tables.iter() {
        let table_id = *table_id;
        ic_cdk::println!("Deleting table: {:?}", table_id.0.to_text());
        ic_cdk::futures::spawn(async move {
            if let Err(e) = clear_table(table_id).await {
                ic_cdk::println!("Error clearing table: {:?}", e);
            }
            let tournament_index_id = match TOURNAMENT_INDEX.lock() {
                Ok(tournament_index) => match tournament_index.as_ref() {
                    Some(tournament_index) => *tournament_index,
                    None => return,
                },
                Err(e) => {
                    ic_cdk::println!("Error getting tournament index: {:?}", e);
                    return;
                }
            };
            if let Err(e) = ensure_principal_is_controller(table_id.0, tournament_index_id).await {
                ic_cdk::println!("Error ensuring principal is controller: {:?}", e);
            } else if let Err(e) = add_to_table_pool_wrapper(tournament_index_id, table_id).await {
                ic_cdk::println!("Error adding table to table pool: {:?}", e);
            }
        });
    }

    Ok(())
}

#[ic_cdk::update]
async fn deposit_prize_pool(
    amount: u64,
    wallet_principal_id: WalletPrincipalId,
    is_admin: bool,
) -> Result<(), TournamentError> {
    handle_cycle_check_async().await;
    let tournament = {
        let tournament = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
        let tournament = tournament
            .as_ref()
            .ok_or(TournamentError::TournamentNotFound)?;

        tournament.clone()
    };

    if let Err(e) =
        handle_tournament_deposit(tournament.currency, amount, wallet_principal_id).await
    {
        return Err(TournamentError::CanisterCallError(format!("{:?}", e)));
    }

    add_to_tournament_prize_pool(amount, is_admin)?;

    DEPOSITORS
        .lock()
        .map_err(|_| TournamentError::LockError)?
        .push((wallet_principal_id, amount));

    Ok(())
}

#[ic_cdk::update]
async fn return_all_cycles_to_tournament_index() -> Result<(), TournamentError> {
    let mut valid_callers = CONTROLLER_PRINCIPALS.clone();
    {
        let mut tournament = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
        let tournament = tournament
            .as_mut()
            .ok_or(TournamentError::TournamentNotFound)?;
        valid_callers.push(tournament.id.0);
    }
    validate_caller(valid_callers);

    // TODO: We are losing cycles on this we need to efficiently find a way to return all cycles to the index canister
    // Get the current balance of cycles in the canister
    let all_cycles = ic_cdk::api::canister_cycle_balance().saturating_sub(100_000_000_000);
    if all_cycles == 0 {
        return Err(TournamentError::CanisterCallError(
            "No cycles available to send".to_string(),
        ));
    }

    ic_cdk::println!("Returning all cycles to tournament index: {}", all_cycles);

    let _ = transfer_cycles_to_tournament_index(all_cycles).await;
    Ok(())
}

#[ic_cdk::query]
async fn get_total_prize_pool() -> u64 {
    PRIZE_POOL.load(Ordering::SeqCst)
}

#[ic_cdk::update]
async fn handle_user_losing(
    user_principal: WalletPrincipalId,
    table_id: TableId,
) -> Result<(), TournamentError> {
    ic_cdk::println!(
        "Handling user losing: {:?} on table: {:?}",
        user_principal.0.to_text(),
        table_id.0.to_text()
    );
    handle_cycle_check_async().await;

    let tournament = {
        let tournament = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
        let tournament = tournament
            .as_ref()
            .ok_or(TournamentError::TournamentNotFound)?
            .clone();
        let valid_callers = vec![table_id.0, tournament.id.0];
        validate_caller(valid_callers);
        tournament
    };

    // Check if rebuy is possible
    match &tournament.tournament_type {
        TournamentType::BuyIn(buy_in_type) => {
            handle_lost_user_rebuy_availability(buy_in_type, user_principal, table_id, &tournament)
                .await?;
        }
        TournamentType::SitAndGo(buy_in_type) => {
            handle_lost_user_rebuy_availability(buy_in_type, user_principal, table_id, &tournament)
                .await?;
        }
        TournamentType::Freeroll(buy_in_type) => {
            handle_lost_user_rebuy_availability(buy_in_type, user_principal, table_id, &tournament)
                .await?;
        }
        _ => {
            return Err(TournamentError::Other(
                "Unsupported tournament type".to_string(),
            ))
        }
    }

    if let Err(e) = check_and_balance_tables(true).await {
        ic_cdk::println!("Error balancing tables: {:?}", e);
    }

    Ok(())
}

#[ic_cdk::query]
fn get_leaderboard() -> Result<Vec<(WalletPrincipalId, u64)>, TournamentError> {
    let leaderboard = LEADERBOARD.lock().unwrap();
    let mut total_players = {
        let tournament = TOURNAMENT.lock().unwrap();
        let tournament = tournament
            .as_ref()
            .ok_or(TournamentError::TournamentNotFound)?
            .clone();
        let mut all_players = HashSet::new();
        for player in tournament.all_players.keys() {
            all_players.insert(*player);
        }
        for player in tournament.current_players.keys() {
            all_players.insert(*player);
        }
        for player in leaderboard.iter() {
            all_players.insert(*player);
        }
        all_players.len()
    };
    let mut ranked_leaderboard = Vec::new();
    for player in leaderboard.iter() {
        ranked_leaderboard.push((*player, total_players as u64));
        total_players -= 1;
    }
    Ok(ranked_leaderboard.iter().rev().cloned().collect())
}

#[ic_cdk::update]
async fn get_live_leaderboard() -> Result<Vec<(WalletPrincipalId, u64)>, TournamentError> {
    let current_time = ic_cdk::api::time();
    let last_update = LAST_LEADERBOARD_UPDATE.load(Ordering::Relaxed);

    if last_update == 0 {
        if let Err(e) = update_live_leaderboard().await {
            ic_cdk::println!("Error updating live leaderboard: {:?}", e);
        }
    } else if current_time > last_update + LEADERBOARD_UPDATE_INTERVAL {
        ic_cdk::futures::spawn(async {
            if let Err(e) = update_live_leaderboard().await {
                ic_cdk::println!("Error updating live leaderboard: {:?}", e);
            }
        });
    }

    // Return the current leaderboard data
    let live_leaderboard = LIVE_LEADERBOARD
        .lock()
        .map_err(|_| TournamentError::LockError)?;
    Ok(live_leaderboard.clone())
}

#[ic_cdk::update]
async fn update_player_count_tournament(
    table_id: TableId,
    user_action: UserTournamentAction,
) -> Result<(), TournamentError> {
    let mut tournament = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
    let tournament = tournament
        .as_mut()
        .ok_or(TournamentError::TournamentNotFound)?;
    validate_caller(vec![table_id.0, tournament.id.0]);
    match user_action {
        UserTournamentAction::Join(uid) => {
            tournament
                .tables
                .get_mut(&table_id)
                .ok_or(TournamentError::TableNotFound)?
                .players
                .insert(uid);
        }
        UserTournamentAction::Leave(uid) => {
            tournament
                .tables
                .get_mut(&table_id)
                .ok_or(TournamentError::TableNotFound)?
                .players
                .retain(|&x| x != uid);
        }
    }
    Ok(())
}

#[ic_cdk::query]
fn get_balance_time_interval() -> Result<u64, TournamentError> {
    let tournament = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
    let tournament = tournament
        .as_ref()
        .ok_or(TournamentError::TournamentNotFound)?;
    Ok(get_balance_interval(&tournament.speed_type))
}

#[ic_cdk::query]
fn get_last_balance_timestamp() -> u64 {
    LAST_BALANCE_TIMESTAMP.load(Ordering::SeqCst)
}

#[ic_cdk::update]
async fn move_player_from_to_table(
    from_table: TableId,
    to_table: TableId,
) -> Result<(), TournamentError> {
    let mut tournament = {
        let tournament = TOURNAMENT.lock().map_err(|_| TournamentError::LockError)?;
        tournament
            .as_ref()
            .ok_or(TournamentError::TournamentNotFound)?
            .clone()
    };

    {
        let mut valid_callers = CONTROLLER_PRINCIPALS.clone();
        valid_callers.push(tournament.id.0);
        valid_callers.push(from_table.0);
        validate_caller(valid_callers);
    }

    let tournament_index = {
        let tournament_index = TOURNAMENT_INDEX
            .lock()
            .map_err(|_| TournamentError::LockError)?;
        *tournament_index
            .as_ref()
            .ok_or(TournamentError::TournamentNotFound)?
    };

    let table = get_table_wrapper(from_table).await?;

    // Get Big blind UID as thats the one that usually gets moved.
    let big_blind_principal = table.get_big_blind_user_principal();

    if let Some(player) = big_blind_principal {
        // Move the player
        move_player_to_table(player, from_table, to_table, &mut tournament).await?;

        // Record the move time
        tournament.record_table_move(to_table)?;
    }

    for (table_id, table_info) in tournament.tables.clone().iter() {
        if table_info.players.is_empty() {
            if let Err(e) = ensure_principal_is_controller(table_id.0, tournament_index).await {
                ic_cdk::println!("Error ensuring principal is controller: {:?}", e);
            } else if let Err(e) = add_to_table_pool_wrapper(tournament_index, *table_id).await {
                ic_cdk::println!("Error adding table to table pool: {:?}", e);
            }
            tournament.tables.remove(table_id);
        }
    }

    *TOURNAMENT.lock().map_err(|_| TournamentError::LockError)? = Some(tournament);
    Ok(())
}

const CYCLES_TOP_UP_AMOUNT: u128 = 750_000_000_000;

#[ic_cdk::update]
async fn request_cycles() -> Result<(), CanisterManagementError> {
    handle_cycle_check_async().await;

    let cycles = ic_cdk::api::canister_cycle_balance();
    let caller = TableId(ic_cdk::api::msg_caller());
    ic_cdk::println!(
        "%%%%%%%%%%% Tournament Canister: Requesting cycles: {} from caller: {}",
        cycles,
        caller.0.to_text()
    );
    if cycles < CYCLES_TOP_UP_AMOUNT {
        return Err(CanisterManagementError::InsufficientCycles);
    }

    transfer_cycles(CYCLES_TOP_UP_AMOUNT, caller).await
}

async fn transfer_cycles(
    cycles_amount: u128,
    caller: TableId,
) -> Result<(), CanisterManagementError> {
    {
        let tournament = TOURNAMENT
            .lock()
            .map_err(|_| CanisterManagementError::LockError)?;
        let tournament = tournament
            .as_ref()
            .ok_or(TournamentError::TournamentNotFound)
            .map_err(|e| CanisterManagementError::ManagementCanisterError(format!("{:?}", e)))?;
        if !tournament.tables.contains_key(&caller) {
            return Err(CanisterManagementError::Transfer(format!(
                "Caller is not a valid destination: {}",
                caller.0.to_text()
            )));
        }
    }

    top_up_canister(caller.0, cycles_amount).await?;
    Ok(())
}

#[ic_cdk::update]
async fn get_canister_status_formatted() -> Result<String, TournamentError> {
    // Validate caller is a controller
    let controllers = (*CONTROLLER_PRINCIPALS).clone();
    validate_caller(controllers);

    handle_cycle_check_async().await;

    // Call the management canister to get status
    let canister_status_arg = CanisterStatusArgs {
        canister_id: ic_cdk::api::canister_self(),
    };

    let status_response = canister_status(&canister_status_arg).await.map_err(|e| {
        TournamentError::CanisterCallError(format!("Failed to get canister status: {:?}", e))
    })?;

    // Format the status into a readable string
    let formatted_status = format!(
        "📊 Canister Status Report
════════════════════════════════════════════════════════════════
🆔 Canister ID: {}
🔄 Status: {:?}
💾 Memory Size: {} bytes ({:.2} MB)
⚡ Cycles: {} ({:.2} T cycles)
🎛️  Controllers: {}
📈 Compute Allocation: {}
🧠 Memory Allocation: {} bytes
🧊 Freezing Threshold: {}
📊 Reserved Cycles Limit: {}
════════════════════════════════════════════════════════════════",
        ic_cdk::api::canister_self().to_text(),
        status_response.status,
        status_response.memory_size,
        status_response.memory_size.clone() / Nat::from(1_048_576_u64), // Convert to MB
        status_response.cycles,
        status_response.cycles.clone() / Nat::from(1_000_000_000_000_u64), // Convert to T cycles
        status_response
            .settings
            .controllers
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(", "),
        status_response.settings.compute_allocation,
        status_response.settings.memory_allocation,
        status_response.settings.freezing_threshold,
        status_response.settings.reserved_cycles_limit
    );

    ic_cdk::println!("{}", formatted_status);
    Ok(formatted_status)
}

ic_cdk::export_candid!();
