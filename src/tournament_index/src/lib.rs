use authentication::validate_caller;
use candid::{Nat, Principal};
use canister_functions::{
    create_canister_wrapper, cycle::check_and_top_up_canister, install_wasm_code,
};
use currency::{
    rake_constants::RAKE_WALLET_ADDRESS_PRINCIPAL,
    types::{
        canister_wallets::icrc1_token_wallet::GenericICRC1TokenWallet,
        currency::{CKTokenSymbol, Token},
        currency_manager::CurrencyManager,
    },
    utils::get_canister_state,
    Currency,
};
use errors::{
    canister_management_error::CanisterManagementError,
    tournament_index_error::TournamentIndexError,
};
use ic_cdk::management_canister::{canister_status, CanisterStatusArgs};
use intercanister_call_wrappers::{tournament_canister::{
    create_tournament_wrapper, ensure_principal_is_controller,
    return_all_cycles_to_tournament_index_wrapper, user_join_tournament,
}, users_index::get_user_wrapper_index};
use lazy_static::lazy_static;
use memory::TABLE_CANISTER_POOL;
use std::{collections::HashMap, sync::Mutex};
use table::poker::game::table_functions::{
    table::{TableConfig, TableId},
    types::CurrencyType,
};
use table::poker::game::types::GameType::NoLimit;
use tournament_index::{create_spin_go_tournament, TournamentIndex};
use tournaments::tournaments::{
    blind_level::BlindLevel,
    tournament_type::TournamentType,
    types::{
        get_blind_level_at_time, NewTournament, NewTournamentSpeedType, TournamentData,
        TournamentId, TournamentState,
    },
};
use user::user::{UsersCanisterId, WalletPrincipalId};

pub mod cycle;
pub mod memory;
pub mod tournament_index;

const MINIMUM_CYCLE_THRESHOLD: u128 = 6_000_000_000_000;
// const MULTI_TABLE_TOURNAMENT_CYCLE_START_AMOUNT: u128 = 5_000_000_000_000;
const SINGLE_TABLE_TOURNAMENT_CYCLE_START_AMOUNT: u128 = 3_000_000_000_000;

// TODO update principals here to the correct tournament index principals
async fn handle_cycle_check() -> Result<(), TournamentIndexError> {
    ic_cdk::println!("%%%%%%%%%%%% Checking and topping up canister");
    let id = ic_cdk::api::canister_self();
    let cycle_dispenser_canister_id =
        if id == Principal::from_text("zocwf-5qaaa-aaaam-qdfaq-cai").unwrap() {
            *CYCLE_DISPENSER_CANISTER_PROD
        } else if id == Principal::from_text("u2qna-fiaaa-aaaag-at3ea-cai").unwrap() {
            *CYCLE_DISPENSER_CANISTER_TEST
        } else if id == Principal::from_text("t63gs-up777-77776-aaaba-cai").unwrap() {
            *CYCLE_DISPENSER_CANISTER_DEV
        } else {
            return Ok(());
        };

    ic_cdk::println!(
        "%%%%%%%%%%%% Checking and topping up canister from cycle dispenser: {}",
        cycle_dispenser_canister_id
    );
    check_and_top_up_canister(id, cycle_dispenser_canister_id, MINIMUM_CYCLE_THRESHOLD).await?;
    Ok(())
}

fn get_user_index() -> Result<Principal, TournamentIndexError> {
    let id = ic_cdk::api::canister_self();
    if id == Principal::from_text("zocwf-5qaaa-aaaam-qdfaq-cai").unwrap() {
        Ok(*USER_INDEX_CANISTER_PROD)
    } else if id == Principal::from_text("u2qna-fiaaa-aaaag-at3ea-cai").unwrap() {
        Ok(*USER_INDEX_CANISTER_TEST)
    } else if id == Principal::from_text("t63gs-up777-77776-aaaba-cai").unwrap() {
        Ok(*USER_INDEX_CANISTER_DEV)
    } else if id == Principal::from_text("ufxgi-4p777-77774-qaadq-cai").unwrap() {
        Ok(*USER_INDEX_CANISTER_LOCAL)
    } else {
        Err(TournamentIndexError::Other(
            "Tournament index canister is not a user index canister".to_string(),
        ))
    }
}

// Define a global instance of GameState wrapped in a Mutex for safe concurrent access.
lazy_static! {
    static ref STATE: Mutex<TournamentIndex> = Mutex::new(TournamentIndex::new());
    static ref CURRENCY_MANAGER: Mutex<CurrencyManager> = Mutex::new(CurrencyManager::new());
    static ref CYCLE_DISPENSER_CANISTER_PROD: Principal =
        Principal::from_text("zuv6g-yaaaa-aaaam-qbeza-cai").unwrap();
    static ref CYCLE_DISPENSER_CANISTER_TEST: Principal =
        Principal::from_text("ev34d-5yaaa-aaaah-qdska-cai").unwrap();
    static ref CYCLE_DISPENSER_CANISTER_DEV: Principal =
        Principal::from_text("tz2ag-zx777-77776-aaabq-cai").unwrap();

    static ref USER_INDEX_CANISTER_PROD: Principal =
        Principal::from_text("lvq5c-nyaaa-aaaam-qdswa-cai").unwrap();
    static ref USER_INDEX_CANISTER_TEST: Principal =
        Principal::from_text("m3tym-daaaa-aaaah-qqbsq-cai").unwrap();
    static ref USER_INDEX_CANISTER_DEV: Principal =
        Principal::from_text("txyno-ch777-77776-aaaaq-cai").unwrap();
    static ref USER_INDEX_CANISTER_LOCAL: Principal =
        Principal::from_text("vizcg-th777-77774-qaaea-cai").unwrap();

    static ref CONTROLLER_PRINCIPALS: Vec<Principal> = vec![
        Principal::from_text("km7qz-4bai4-e5ptx-hgrck-z3web-ameqg-ksxcf-u7wbr-t5fna-i7bqp-hqe")
            .unwrap(),
        Principal::from_text("uyxh5-bi3za-gxbfs-op3gj-ere73-a6jhv-5jky3-zawef-b5r2s-k26un-sae")
            .unwrap(),
        Principal::from_text("w3kjy-pitqg-dvab7-tb57q-63gnd-di4vo-loiiy-s6zm2-gqcmw-ixliz-aae").unwrap(),
        Principal::from_text("tcuxo-b5b4t-vxwo7-mwgxi-vb4ig-zuux4-jmvru-fjocv-4uxuz-7yo4v-hqe").unwrap(),
    ];
    static ref TOURNAMENT_CANISTER_WASM: &'static [u8] =
        include_bytes!("../../../target/wasm32-unknown-unknown/release/tournament_canister.wasm");

    static ref MIN_TOURNAMENT_LIQUIDITY: Mutex<HashMap<Currency, u128>> = {
        let mut min_tournament_liquidity = HashMap::new();
        min_tournament_liquidity.insert(
            Currency::ICP,
            100 * 1e8 as u128, // Minimum liquidity for ICP tournaments
        );
        min_tournament_liquidity.insert(
            Currency::CKETHToken(currency::types::currency::CKTokenSymbol::ETH),
            1 * 1e18 as u128, // Minimum liquidity for ETH tournaments
        );
        min_tournament_liquidity.insert(
            Currency::CKETHToken(currency::types::currency::CKTokenSymbol::USDC),
            1000 * 1e6 as u128, // Minimum liquidity for USDC tournaments
        );
        min_tournament_liquidity.insert(
            Currency::CKETHToken(currency::types::currency::CKTokenSymbol::USDT),
            1000 * 1e6 as u128, // Minimum liquidity for USDT tournaments
        );
        Mutex::new(min_tournament_liquidity)
    };
    static ref ENABLE_RAKE: bool = false;
}

#[ic_cdk::init]
fn init() {
    let id = ic_cdk::api::canister_self();
    ic_cdk::println!("Tournament index canister {id} initialized");
}

#[ic_cdk::query]
fn ping() -> String {
    "Ok".to_string()
}

#[ic_cdk::query]
fn get_account_number() -> Result<Option<String>, TournamentIndexError> {
    let canister_state = get_canister_state();
    Ok(Some(canister_state.account_identifier.to_string()))
}

#[ic_cdk::update]
async fn create_tournament(
    new_tournament: NewTournament,
    table_config: TableConfig,
) -> Result<TournamentId, TournamentIndexError> {
    let mut new_tournament = new_tournament;
    let mut table_config = table_config;
    let caller = ic_cdk::api::msg_caller();
    let user_index = get_user_index()?;
    match get_user_wrapper_index(user_index, WalletPrincipalId(caller)).await {
        Ok(user) => {
            if new_tournament.guaranteed_prize_pool.is_some() && !user.admin_role.unwrap_or(user::admin::AdminRole::Moderator).can_promote_to_moderator() {
                return Err(TournamentIndexError::NotAuthorized(
                    "Only admins can create tournaments with guaranteed prize pools".to_string(),
                ));
            }
        },
        Err(e) => {
            if new_tournament.guaranteed_prize_pool.is_some() {
                ic_cdk::println!("Error getting user: {:?}", e);
                return Err(TournamentIndexError::CanisterCallFailed(format!("{:?}", e)));
            }
        }
    };
    let tournament_canister = {
        handle_cycle_check().await?;

        // Create new tournament canister
        if new_tournament.start_time <= ic_cdk::api::time()
            && !matches!(new_tournament.tournament_type, TournamentType::SitAndGo(_))
        {
            return Err(TournamentIndexError::InvalidTournamentConfig(
                "Start time must be in the future".to_string(),
            ));
        }
        if let TournamentType::SitAndGo(_) | TournamentType::SpinAndGo(_, _) =
            new_tournament.tournament_type
        {
            new_tournament.start_time = 0;
        }
        if let TournamentType::Freeroll(_) = &new_tournament.tournament_type {
            if new_tournament.buy_in != 0 {
                return Err(TournamentIndexError::InvalidTournamentConfig(
                    "Freeroll tournaments must have a buy-in of 0".to_string(),
                ));
            }
        }
        let tournament_canister = TournamentId(create_tournament_canister().await?);

        // Create tournament info
        let (tournament, prize_pool) =
            if let TournamentType::SpinAndGo(_, _) = new_tournament.tournament_type {
                TournamentData::new_spin_and_go(
                    tournament_canister,
                    new_tournament,
                    table_config.clone(),
                )
                .await?
            } else {
                (
                    TournamentData::new(tournament_canister, new_tournament, table_config.clone())?,
                    0,
                )
            };
        if matches!(tournament.tournament_type, TournamentType::SpinAndGo(_, _)) {
            let currency_manager = {
                CURRENCY_MANAGER
                    .lock()
                    .map_err(|_| TournamentIndexError::LockError)?
                    .clone()
            };
            match tournament.currency {
                CurrencyType::Real(currency) => {
                    let balance = currency_manager
                        .get_balance(&currency, ic_cdk::api::canister_self())
                        .await
                        .map_err(|e| {
                            TournamentIndexError::CanisterCallFailed(format!("{:?}", e))
                        })?;
                    if balance < prize_pool as u128 {
                        return Err(TournamentIndexError::InsufficientLiquidity);
                    } else if prize_pool > 3 * tournament.buy_in {
                        currency_manager
                            .withdraw(
                                &currency,
                                tournament_canister.0,
                                prize_pool - 3 * tournament.buy_in,
                            )
                            .await
                            .map_err(|e| {
                                TournamentIndexError::CanisterCallFailed(format!("{:?}", e))
                            })?;
                    }
                }
                CurrencyType::Fake => {}
            }
        }

        // Validate tournament configuration
        tournament.validate()?;

        table_config.game_type =
            NoLimit(tournament.speed_type.get_params().blind_levels[0].small_blind);

        let tournament =
            create_tournament_wrapper(tournament_canister, tournament, table_config, prize_pool)
                .await?;

        // Store tournament info
        let mut state = STATE.lock().map_err(|_| TournamentIndexError::LockError)?;
        state
            .tournaments
            .insert(tournament_canister, tournament.clone());
        state.active_tournaments.push(tournament_canister);
        state.delete_all_tournaments_older_than_a_week();
        tournament_canister
    };

    Ok(tournament_canister)
}

#[ic_cdk::update]
async fn update_tournament_state(
    tournament_id: TournamentId,
    new_state: TournamentState,
) -> Result<(), TournamentIndexError> {
    handle_cycle_check().await?;
    let mut state = STATE.lock().map_err(|_| TournamentIndexError::LockError)?;
    let valid_callers: Vec<Principal> = state.tournaments.values().map(|t| t.id.0).collect();
    validate_caller(valid_callers);

    let tournament = state
        .tournaments
        .get_mut(&tournament_id)
        .ok_or(TournamentIndexError::TournamentNotFound)?;

    tournament.state = new_state.clone();

    // Handle state transitions
    match new_state {
        TournamentState::Completed | TournamentState::Cancelled => {
            state.active_tournaments.retain(|&id| id != tournament_id);
            state.completed_tournaments.push(tournament_id);
        }
        _ => {}
    }

    Ok(())
}

#[ic_cdk::update]
async fn delete_tournament(tournament_id: TournamentId) -> Result<(), TournamentIndexError> {
    handle_cycle_check().await?;
    let valid_callers = CONTROLLER_PRINCIPALS.clone();
    validate_caller(valid_callers);

    if let Err(e) = return_all_cycles_to_tournament_index_wrapper(tournament_id)
        .await
        .map_err(|e| TournamentIndexError::CanisterCallFailed(format!("{:?}", e)))
    {
        ic_cdk::println!("Error returning cycles to tournament index: {:?}", e);
    }

    let mut state = STATE.lock().map_err(|_| TournamentIndexError::LockError)?;
    state.tournaments.remove(&tournament_id);
    state.active_tournaments.retain(|&id| id != tournament_id);
    state
        .completed_tournaments
        .retain(|&id| id != tournament_id);
    Ok(())
}

#[ic_cdk::query]
fn get_player_tournaments(player: WalletPrincipalId) -> Vec<TournamentData> {
    let state = STATE.lock().unwrap();
    state
        .tournaments
        .values()
        .filter(|t| t.current_players.contains_key(&player))
        .cloned()
        .collect()
}

#[ic_cdk::update]
async fn join_spin_and_go_tournament(
    buy_in: u64,
    user_principal: UsersCanisterId,
    user_wallet_principal_id: WalletPrincipalId,
) -> Result<(), TournamentIndexError> {
    let pool = {
        let mut state = STATE.lock().map_err(|_| TournamentIndexError::LockError)?;
        let user_pool = state
            .spin_go_pools
            .get_mut(&buy_in)
            .ok_or(TournamentIndexError::PoolNotFound)?;
        if user_pool.contains(&(user_principal, user_wallet_principal_id)) {
            return Err(TournamentIndexError::FailedToAddToUserPool(
                "User already in pool".to_string(),
            ));
        }
        user_pool.push((user_principal, user_wallet_principal_id));
        user_pool.clone()
    };

    if pool.len() == 3 {
        let spin_and_go = create_spin_go_tournament(buy_in).await?;
        for (user_principal, user_wallet_principal_id) in pool {
            user_join_tournament(spin_and_go, user_principal, user_wallet_principal_id).await?;
        }
    }

    Ok(())
}

#[ic_cdk::update]
fn leave_spin_and_go_tournament(
    buy_in: u64,
    user_principal: Principal,
    user_wallet_principal_id: Principal,
) -> Result<(), TournamentIndexError> {
    let mut state = STATE.lock().map_err(|_| TournamentIndexError::LockError)?;
    let user_pool = state
        .spin_go_pools
        .get_mut(&buy_in)
        .ok_or(TournamentIndexError::PoolNotFound)?;
    user_pool.retain(|(p, w)| *p != user_principal || *w != user_wallet_principal_id);
    Ok(())
}

#[ic_cdk::query]
fn get_blind_level_at_timestamp(
    speed_type: NewTournamentSpeedType,
    timestamp: u64,
    tournament_start: u64,
    starting_chips: u64,
) -> Option<BlindLevel> {
    get_blind_level_at_time(speed_type, timestamp, tournament_start, starting_chips)
}

async fn create_tournament_canister() -> Result<Principal, TournamentIndexError> {
    handle_cycle_check().await?;
    let controllers = CONTROLLER_PRINCIPALS.clone();
    let wasm_module = TOURNAMENT_CANISTER_WASM.to_vec();
    let cycle_amount = SINGLE_TABLE_TOURNAMENT_CYCLE_START_AMOUNT;
    let table_canister_principal = create_canister_wrapper(controllers, Some(cycle_amount)).await?;
    install_wasm_code(table_canister_principal, wasm_module).await?;

    Ok(table_canister_principal)
}

#[ic_cdk::update]
fn purge_table_pool() {
    let valid_callers = CONTROLLER_PRINCIPALS.clone();
    validate_caller(valid_callers);

    TABLE_CANISTER_POOL.with(|pool| {
        let pool = pool.borrow_mut();
        loop {
            if pool.pop().is_none() {
                break;
            }
        }
    });
}

#[ic_cdk::update]
fn add_to_pool(principal: TableId) -> Result<(), TournamentIndexError> {
    ic_cdk::println!("Adding to pool: {:?}", principal.0.to_text());
    {
        let tournament_index = STATE.lock().map_err(|_| TournamentIndexError::LockError)?;
        let valid_callers: Vec<Principal> = tournament_index
            .tournaments
            .values()
            .map(|t| t.id.0)
            .collect();
        validate_caller(valid_callers);
    }
    TABLE_CANISTER_POOL
        .with(|pool| pool.borrow_mut().push(&principal.0))
        .map_err(|e| TournamentIndexError::FailedToAddToTablePool(format!("{:?}", e)))
}

#[ic_cdk::update]
async fn get_and_remove_from_pool() -> Result<Option<TableId>, TournamentIndexError> {
    ic_cdk::println!("Getting and removing from pool");
    {
        let tournament_index = STATE.lock().map_err(|_| TournamentIndexError::LockError)?;
        let valid_callers: Vec<Principal> = tournament_index
            .tournaments
            .values()
            .map(|t| t.id.0)
            .collect();

        validate_caller(valid_callers);
    }

    let res = TABLE_CANISTER_POOL.with(|pool| {
        let pool = pool.borrow_mut();
        if !pool.is_empty() {
            pool.pop()
        } else {
            None
        }
    });

    // If we got a canister, set the caller as its controller
    if let Some(canister_id) = res {
        ensure_principal_is_controller(canister_id, ic_cdk::api::msg_caller())
            .await
            .map_err(|e| {
                ic_cdk::println!("Error setting caller as controller: {:?}", e);
                TournamentIndexError::CanisterCallFailed(format!("{:?}", e))
            })?;
        Ok(Some(TableId(canister_id)))
    } else {
        Ok(None)
    }
}

#[ic_cdk::query]
fn get_pool() -> Vec<Principal> {
    TABLE_CANISTER_POOL.with(|pool| {
        let pool = pool.borrow();
        pool.iter().collect::<Vec<Principal>>()
    })
}

#[ic_cdk::update]
async fn clear_pool() -> Result<(), TournamentIndexError> {
    TABLE_CANISTER_POOL.with(|pool| {
        let pool = pool.borrow_mut();
        while pool.pop().is_some() {}
    });
    Ok(())
}

#[ic_cdk::update]
async fn get_icp_balance() -> Result<u64, TournamentIndexError> {
    let currency_manager = {
        CURRENCY_MANAGER
            .lock()
            .map_err(|_| TournamentIndexError::LockError)?
            .clone()
    };
    let balance = currency_manager
        .get_balance(&Currency::ICP, ic_cdk::api::canister_self())
        .await
        .map_err(|e| TournamentIndexError::CanisterCallFailed(format!("{:?}", e)))?;
    Ok(balance as u64)
}

#[ic_cdk::update]
async fn withdraw_icp(principal: Principal, amount: u64) -> Result<(), TournamentIndexError> {
    validate_caller(CONTROLLER_PRINCIPALS.to_vec());

    let currency_manager = {
        CURRENCY_MANAGER
            .lock()
            .map_err(|_| TournamentIndexError::LockError)?
            .clone()
    };
    currency_manager
        .withdraw(&Currency::ICP, principal, amount)
        .await
        .map_err(|e| TournamentIndexError::CanisterCallFailed(format!("{:?}", e)))
}

#[ic_cdk::update]
async fn get_ckbtc_balance() -> Result<u64, TournamentIndexError> {
    let currency_manager = {
        CURRENCY_MANAGER
            .lock()
            .map_err(|_| TournamentIndexError::LockError)?
            .clone()
    };
    let balance = currency_manager
        .get_balance(&Currency::BTC, ic_cdk::api::canister_self())
        .await
        .map_err(|e| TournamentIndexError::CanisterCallFailed(format!("{:?}", e)))?;
    Ok(balance as u64)
}

#[ic_cdk::update]
async fn withdraw_ckbtc(principal: Principal, amount: u64) -> Result<(), TournamentIndexError> {
    validate_caller(CONTROLLER_PRINCIPALS.to_vec());

    let currency_manager = {
        CURRENCY_MANAGER
            .lock()
            .map_err(|_| TournamentIndexError::LockError)?
            .clone()
    };
    currency_manager
        .withdraw(&Currency::BTC, principal, amount)
        .await
        .map_err(|e| TournamentIndexError::CanisterCallFailed(format!("{:?}", e)))
}

#[ic_cdk::update]
async fn get_ckusdc_balance() -> Result<u64, TournamentIndexError> {
    let currency_manager = {
        CURRENCY_MANAGER
            .lock()
            .map_err(|_| TournamentIndexError::LockError)?
            .clone()
    };
    let balance = currency_manager
        .get_balance(
            &Currency::CKETHToken(CKTokenSymbol::USDC),
            ic_cdk::api::canister_self(),
        )
        .await
        .map_err(|e| TournamentIndexError::CanisterCallFailed(format!("{:?}", e)))?;
    Ok(balance as u64)
}

#[ic_cdk::update]
async fn withdraw_ckusdc(principal: Principal, amount: u64) -> Result<(), TournamentIndexError> {
    validate_caller(CONTROLLER_PRINCIPALS.to_vec());

    let currency_manager = {
        CURRENCY_MANAGER
            .lock()
            .map_err(|_| TournamentIndexError::LockError)?
            .clone()
    };
    currency_manager
        .withdraw(
            &Currency::CKETHToken(CKTokenSymbol::USDC),
            principal,
            amount,
        )
        .await
        .map_err(|e| TournamentIndexError::CanisterCallFailed(format!("{:?}", e)))
}

#[ic_cdk::update]
async fn get_ckusdt_balance() -> Result<u64, TournamentIndexError> {
    let currency_manager = {
        CURRENCY_MANAGER
            .lock()
            .map_err(|_| TournamentIndexError::LockError)?
            .clone()
    };
    let balance = currency_manager
        .get_balance(
            &Currency::CKETHToken(CKTokenSymbol::USDT),
            ic_cdk::api::canister_self(),
        )
        .await
        .map_err(|e| TournamentIndexError::CanisterCallFailed(format!("{:?}", e)))?;
    Ok(balance as u64)
}

#[ic_cdk::update]
async fn withdraw_ckusdt(principal: Principal, amount: u64) -> Result<(), TournamentIndexError> {
    validate_caller(CONTROLLER_PRINCIPALS.to_vec());

    let currency_manager = {
        CURRENCY_MANAGER
            .lock()
            .map_err(|_| TournamentIndexError::LockError)?
            .clone()
    };
    currency_manager
        .withdraw(
            &Currency::CKETHToken(CKTokenSymbol::USDT),
            principal,
            amount,
        )
        .await
        .map_err(|e| TournamentIndexError::CanisterCallFailed(format!("{:?}", e)))
}

#[ic_cdk::update]
async fn get_cketh_balance() -> Result<u64, TournamentIndexError> {
    let currency_manager = {
        CURRENCY_MANAGER
            .lock()
            .map_err(|_| TournamentIndexError::LockError)?
            .clone()
    };
    let balance = currency_manager
        .get_balance(
            &Currency::CKETHToken(CKTokenSymbol::ETH),
            ic_cdk::api::canister_self(),
        )
        .await
        .map_err(|e| TournamentIndexError::CanisterCallFailed(format!("{:?}", e)))?;
    Ok(balance as u64)
}

#[ic_cdk::update]
async fn withdraw_cketh(principal: Principal, amount: u64) -> Result<(), TournamentIndexError> {
    validate_caller(CONTROLLER_PRINCIPALS.to_vec());

    let currency_manager = {
        CURRENCY_MANAGER
            .lock()
            .map_err(|_| TournamentIndexError::LockError)?
            .clone()
    };
    currency_manager
        .withdraw(&Currency::CKETHToken(CKTokenSymbol::ETH), principal, amount)
        .await
        .map_err(|e| TournamentIndexError::CanisterCallFailed(format!("{:?}", e)))
}

#[ic_cdk::update]
async fn request_withdrawal(currency: Currency, amount: u64) -> Result<(), TournamentIndexError> {
    ic_cdk::println!(
        "Requesting withdrawal of {} {}",
        amount,
        currency.to_string()
    );
    let active_tournaments = {
        STATE
            .lock()
            .map_err(|_| TournamentIndexError::LockError)?
            .active_tournaments
            .clone()
    };
    let valid_callers = active_tournaments
        .iter()
        .map(|t| t.0)
        .collect::<Vec<Principal>>();
    validate_caller(valid_callers);

    let currency_manager = {
        CURRENCY_MANAGER
            .lock()
            .map_err(|_| TournamentIndexError::LockError)?
            .clone()
    };
    let fee = currency_manager.get_fee(&currency).await.map_err(|e| {
        TournamentIndexError::CanisterCallFailed(format!("Failed to get fee: {:?}", e))
    })?;
    currency_manager
        .withdraw(&currency, ic_cdk::api::msg_caller(), amount + fee as u64)
        .await
        .map_err(|e| {
            TournamentIndexError::CanisterCallFailed(format!("Failed to withdraw: {:?}", e))
        })
}

#[ic_cdk::update]
async fn register_token(ledger_id: Principal) -> Result<Currency, TournamentIndexError> {
    // Verify the token implements ICRC-1
    let wallet = GenericICRC1TokenWallet::new(ledger_id).await.map_err(|e| {
        TournamentIndexError::CanisterCallFailed(format!("Failed to create token wallet: {:?}", e))
    })?;

    // Create Token struct
    let token = Token::from_string(ledger_id, &wallet.metadata.symbol, wallet.metadata.decimals);
    let currency = Currency::GenericICRC1(token);

    // Add to CurrencyManager
    let mut currency_manager = {
        CURRENCY_MANAGER
            .lock()
            .map_err(|_| TournamentIndexError::LockError)?
            .clone()
    };
    currency_manager.add_currency(currency).await.map_err(|e| {
        TournamentIndexError::CanisterCallFailed(format!(
            "Failed to add token to CurrencyManager: {:?}",
            e
        ))
    })?;
    *CURRENCY_MANAGER
        .lock()
        .map_err(|_| TournamentIndexError::LockError)? = currency_manager;

    // Return as Currency
    Ok(currency)
}

#[ic_cdk::query]
fn get_all_tournaments() -> Vec<TournamentData> {
    let state = STATE.lock().unwrap();
    state.tournaments.values().cloned().collect()
}

#[ic_cdk::query]
fn get_active_tournaments(filter_type: Option<u8>) -> Vec<TournamentData> {
    let state = STATE.lock().unwrap();
    state
        .active_tournaments
        .iter()
        .filter_map(|id| state.tournaments.get(id))
        .filter(|tournament| {
            if let Some(filter) = filter_type {
                tournament.tournament_type.get_type_id() == filter
            } else {
                true
            }
        })
        .cloned()
        .collect()
}

#[ic_cdk::query]
fn get_completed_tournaments() -> Vec<TournamentData> {
    let state = STATE.lock().unwrap();
    state
        .completed_tournaments
        .iter()
        .filter_map(|id| state.tournaments.get(id))
        .cloned()
        .collect()
}

#[ic_cdk::update]
async fn upgrade_all_tournament_canisters(
) -> Result<Vec<(TournamentId, CanisterManagementError)>, TournamentIndexError> {
    // Validate caller permissions
    let caller = ic_cdk::api::msg_caller();
    if !CONTROLLER_PRINCIPALS.contains(&caller) {
        return Err(TournamentIndexError::NotAuthorized(
            "Caller is not authorized to upgrade canisters".to_string(),
        ));
    }

    handle_cycle_check().await?;

    const BATCH_SIZE: usize = 30; // Process 30 tournaments at a time

    let tournaments: Vec<TournamentId> = {
        let state = STATE.lock().map_err(|_| TournamentIndexError::LockError)?;
        state.tournaments.keys().copied().collect()
    };

    let wasm_module = TOURNAMENT_CANISTER_WASM.to_vec();
    let mut failed_upgrades = Vec::new();

    // Process tournaments in batches
    for chunk in tournaments.chunks(BATCH_SIZE) {
        let futures: Vec<_> = chunk
            .iter()
            .map(|&tournament_canister| {
                let wasm_clone = wasm_module.clone();
                async move {
                    match canister_functions::upgrade_wasm_code(tournament_canister.0, wasm_clone)
                        .await
                    {
                        Ok(_) => {
                            ic_cdk::println!(
                                "Successfully upgraded tournament canister {}",
                                tournament_canister.0.to_text()
                            );
                            Ok(tournament_canister)
                        }
                        Err(e) => {
                            ic_cdk::println!(
                                "Failed to upgrade tournament canister {}: {:?}",
                                tournament_canister.0.to_text(),
                                e
                            );
                            Err((tournament_canister, e))
                        }
                    }
                }
            })
            .collect();

        // Execute batch of upgrades
        let batch_results = futures::future::join_all(futures).await;
        for result in batch_results {
            if let Err((canister_id, error)) = result {
                failed_upgrades.push((canister_id, error));
            }
        }
    }

    Ok(failed_upgrades)
}

#[ic_cdk::update]
async fn upgrade_tournament_canister(
    tournament_canister: Principal,
) -> Result<(), TournamentIndexError> {
    // Validate caller permissions
    let caller = ic_cdk::api::msg_caller();
    if !CONTROLLER_PRINCIPALS.contains(&caller) {
        return Err(TournamentIndexError::NotAuthorized(
            "Caller is not authorized to upgrade canister".to_string(),
        ));
    }

    handle_cycle_check().await?;

    let wasm_module = TOURNAMENT_CANISTER_WASM.to_vec();
    canister_functions::upgrade_wasm_code(tournament_canister, wasm_module).await?;

    Ok(())
}

#[ic_cdk::update]
async fn check_tournament_liquidity() -> Result<(), TournamentIndexError> {
    handle_cycle_check().await?;
    validate_caller(CONTROLLER_PRINCIPALS.clone());

    let (min_tournament_liquidity, currency_manager) = {
        let min_tournament_liquidity = MIN_TOURNAMENT_LIQUIDITY
            .lock()
            .map_err(|_| TournamentIndexError::LockError)?;
        let currency_manager = CURRENCY_MANAGER
            .lock()
            .map_err(|_| TournamentIndexError::LockError)?;
        (min_tournament_liquidity.clone(), currency_manager.clone())
    };

    for (currency, min_liquidity) in min_tournament_liquidity.iter() {
        let balance = currency_manager
            .get_balance(currency, ic_cdk::api::canister_self())
            .await
            .map_err(|e| TournamentIndexError::CanisterCallFailed(format!("{:?}", e)))?;

        if balance > *min_liquidity {
            currency_manager
                .withdraw(
                    currency,
                    Principal::from_text(RAKE_WALLET_ADDRESS_PRINCIPAL).unwrap(),
                    (balance - min_liquidity) as u64,
                )
                .await
                .map_err(|e| TournamentIndexError::CanisterCallFailed(format!("{:?}", e)))?;
        }
    }

    Ok(())
}

#[ic_cdk::update]
async fn get_canister_status_formatted() -> Result<String, TournamentIndexError> {
    // Validate caller is a controller
    let controllers = (*CONTROLLER_PRINCIPALS).clone();
    validate_caller(controllers);

    handle_cycle_check().await?;

    // Call the management canister to get status
    let canister_status_arg = CanisterStatusArgs {
        canister_id: ic_cdk::api::canister_self(),
    };

    let status_response = canister_status(&canister_status_arg).await.map_err(|e| {
        TournamentIndexError::CanisterCallError(format!("Failed to get canister status: {:?}", e))
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
