use std::{collections::HashMap, sync::Mutex};

use authentication::validate_caller;
use candid::{Nat, Principal};
use canister_functions::{rake_constants, rake_stats::RakeStats};
use chat::{ChatHistory, ChatMessage, ChatMessageType};
use currency::{state::TransactionState, types::currency_manager::CurrencyManager, Currency};
use errors::{chat_error::ChatError, game_error::GameError, table_error::TableError};
use ic_cdk::management_canister::{canister_status, CanisterStatusArgs, DepositCyclesArgs};
use intercanister_call_wrappers::{
    log_store::log_actions_wrapper,
    users_canister::{
        add_users_active_table, get_user_wrapper, get_users_canister_principal_by_id_wrapper,
        remove_users_active_table,
    },
};
use lazy_static::lazy_static;
use table::{
    poker::{
        core::{Card, FlatDeck, Rank, Suit, Value},
        game::{
            table_functions::{
                action_log::ActionType,
                ante::AnteType,
                table::{BigBlind, SmallBlind, Table, TableConfig, TableId, TableType},
                types::{BetType, CurrencyType, DealStage, Notification, PlayerAction, SeatStatus},
            },
            types::{CardProvenance, PublicTable, QueueItem, RngMetadata, RngStats, TableStatus},
            utils::rank_hand,
        },
    },
    table_canister::{kick_player_wrapper, leave_table_wrapper, start_new_betting_round_wrapper},
    types::ReturnResult,
    utils::is_table_game_ongoing,
};
use tournaments::tournaments::types::UserTournamentAction;
use user::user::{User, UsersCanisterId, WalletPrincipalId, REFERRAL_PERIOD};
use utils::{
    get_user_index_principal, handle_cycle_check, handle_last_user_leaving,
    handle_table_validity_check, update_player_count_tournament, update_table_player_count,
};

use crate::utils::reshuffle_bytes_hash;

mod memory;
pub mod utils;

// Define a global instance of GameState wrapped in a Mutex for safe concurrent access.
lazy_static! {
    static ref TABLE: Mutex<Option<Table>> = Mutex::new(None);
    static ref BACKEND_PRINCIPAL: Mutex<Option<Principal>> = Mutex::new(None);
    static ref TRANSACTION_STATE: Mutex<TransactionState> = Mutex::new(TransactionState::new());
    static ref CURRENCY_MANAGER: Mutex<Option<CurrencyManager>> = Mutex::new(None);
    static ref RAKE_WALLET_ACCOUNT_ID: String = rake_constants::RAKE_WALLET_ACCOUNT_ID.to_string();
    static ref RAKE_WALLET_ADDRESS_PRINCIPAL: Principal =
        Principal::from_text(rake_constants::RAKE_WALLET_ADDRESS_PRINCIPAL).unwrap();
    static ref RAKE_STATS: Mutex<RakeStats> = Mutex::new(RakeStats::new());
    static ref CHAT_HISTORY: Mutex<ChatHistory> = Mutex::new(ChatHistory::new(1000));
    static ref CONTROLLER_PRINCIPALS: Vec<Principal> = vec![
        Principal::from_text("py2cj-ei3dt-3ber7-nvxdl-56xvh-qkhop-7x7fz-nph7j-7cuya-3gyxr-cqe")
            .unwrap(),
        Principal::from_text("km7qz-4bai4-e5ptx-hgrck-z3web-ameqg-ksxcf-u7wbr-t5fna-i7bqp-hqe")
            .unwrap(),
        Principal::from_text("uyxh5-bi3za-gxbfs-op3gj-ere73-a6jhv-5jky3-zawef-b5r2s-k26un-sae")
            .unwrap(),
        Principal::from_text("w3kjy-pitqg-dvab7-tb57q-63gnd-di4vo-loiiy-s6zm2-gqcmw-ixliz-aae")
            .unwrap(),
    ];
}

#[ic_cdk::init]
fn init() {
    let principal = ic_cdk::api::canister_self();
    ic_cdk::println!("Table canister {} initialized", principal);
}

#[ic_cdk::update]
async fn create_table(config: TableConfig, bytes: Vec<u8>) -> Result<PublicTable, TableError> {
    let table = {
        let mut backend_principal = BACKEND_PRINCIPAL
            .lock()
            .map_err(|_| TableError::LockError)?;

        if let Some(backend_principal) = *backend_principal {
            validate_caller(vec![backend_principal]);
        }
        *backend_principal = Some(ic_cdk::api::msg_caller());

        let mut table_state = TABLE.lock().map_err(|_| TableError::LockError)?;

        let table = Table::new(TableId(ic_cdk::api::canister_self()), config.clone(), bytes);

        *table_state = Some(table.clone());
        table.clone()
    };

    let currency_manager = match &config.currency_type {
        CurrencyType::Real(currency) => {
            let mut currency_manager = CurrencyManager::new();
            currency_manager.add_currency(*currency).await?;
            Some(currency_manager)
        }
        CurrencyType::Fake => None,
    };

    *CURRENCY_MANAGER.lock().map_err(|_| TableError::LockError)? = currency_manager;
    Ok(table.into())
}

#[ic_cdk::query]
fn ping() -> String {
    "Ok".to_string()
}

#[ic_cdk::query]
fn get_table() -> Result<PublicTable, TableError> {
    let mut table = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table = table.as_mut().ok_or(TableError::TableNotFound)?;
    let caller = WalletPrincipalId(ic_cdk::api::msg_caller());

    if table.deal_stage != DealStage::Showdown {
        table.hide_cards(caller).map_err(|e| e.into_inner())?;
    }
    Ok(table.into())
}

#[ic_cdk::query]
fn is_table_full() -> Result<bool, TableError> {
    let table = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table = table.as_ref().ok_or(TableError::TableNotFound)?;
    Ok(table.is_full())
}

#[ic_cdk::query]
fn is_game_ongoing() -> Result<bool, TableError> {
    let table = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table = table.as_ref().ok_or(TableError::TableNotFound)?;
    Ok(is_table_game_ongoing(table))
}

// ============================================================================
// RNG Transparency Query Endpoints
// ============================================================================

/// Get RNG metadata for a specific round
#[ic_cdk::query]
fn get_rng_metadata(round_id: u64) -> Result<RngMetadata, TableError> {
    let table = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table = table.as_ref().ok_or(TableError::TableNotFound)?;

    let mut rng_metadata = table
        .rng_history
        .iter()
        .find(|rng| rng.round_id == round_id)
        .cloned()
        .ok_or_else(|| {
            TableError::InvalidRequest(format!("RNG metadata not found for round {}", round_id))
        })?;

    // 🔒 SECURITY: Hide sensitive data during gameplay to prevent deck reconstruction
    // Only reveal after game ends for verification
    let game_ended = table.sorted_users.is_some();

    if !game_ended && rng_metadata.round_id == table.round_ticker {
        rng_metadata.time_seed = 0; // Hide - allows deck reconstruction
        rng_metadata.timestamp_ns = 0; // Hide - same as time_seed
                                       // ✅ Keep raw_random_bytes - visible from start for transparency
                                       // ✅ Keep deck_hash - commitment proof
                                       // ✅ shuffled_deck already empty during game
    }

    Ok(rng_metadata)
}

/// Get RNG metadata for the current round
#[ic_cdk::query]
fn get_current_rng_metadata() -> Result<RngMetadata, TableError> {
    let table = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table = table.as_ref().ok_or(TableError::TableNotFound)?;

    let mut rng_metadata = table
        .rng_history
        .last()
        .cloned()
        .ok_or_else(|| TableError::InvalidRequest("No RNG history available".to_string()))?;

    // 🔒 SECURITY: Hide sensitive data during gameplay to prevent deck reconstruction
    // Only reveal after game ends for verification
    let game_ended = table.sorted_users.is_some();

    if !game_ended {
        rng_metadata.time_seed = 0; // Hide - allows deck reconstruction
        rng_metadata.timestamp_ns = 0; // Hide - same as time_seed
                                       // ✅ Keep raw_random_bytes - visible from start for transparency
                                       // ✅ Keep deck_hash - commitment proof
                                       // ✅ shuffled_deck already empty during game
    }

    Ok(rng_metadata)
}

/// Get all RNG history (paginated to avoid large responses)
#[ic_cdk::query]
fn get_rng_history(
    limit: Option<u64>,
    offset: Option<u64>,
) -> Result<Vec<RngMetadata>, TableError> {
    let table = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table = table.as_ref().ok_or(TableError::TableNotFound)?;

    let limit = limit.unwrap_or(10).min(50) as usize; // Max 50 records
    let offset = offset.unwrap_or(0) as usize;

    // 🔒 SECURITY: Hide time_seed for current round if game is ongoing
    let game_ended = table.sorted_users.is_some();
    let current_round = table.round_ticker;

    let history: Vec<RngMetadata> = table
        .rng_history
        .iter()
        .skip(offset)
        .take(limit)
        .map(|rng| {
            let mut rng_clone = rng.clone();
            // Hide sensitive data for current ongoing round
            if !game_ended && rng.round_id == current_round {
                rng_clone.time_seed = 0;
                rng_clone.timestamp_ns = 0;
                // ✅ Keep raw_random_bytes - visible from start for transparency
                rng_clone.shuffled_deck = Vec::new();
            }
            rng_clone
        })
        .collect();

    Ok(history)
}

/// Get the total number of rounds with RNG data
#[ic_cdk::query]
fn get_rng_history_count() -> Result<u64, TableError> {
    let table = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table = table.as_ref().ok_or(TableError::TableNotFound)?;

    Ok(table.rng_history.len() as u64)
}

/// Get card provenance by card hash
#[ic_cdk::query]
fn get_card_provenance(card_hash: String) -> Result<CardProvenance, TableError> {
    let table = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table = table.as_ref().ok_or(TableError::TableNotFound)?;

    let provenance = table
        .card_provenance
        .get(&card_hash)
        .cloned()
        .ok_or_else(|| {
            TableError::InvalidRequest(format!("Card provenance not found for hash {}", card_hash))
        })?;

    // 🔒 SECURITY: Hide card value during gameplay
    let game_ended = table.sorted_users.is_some();

    if !game_ended {
        // During gameplay: return ONLY hash and position
        // Hide the actual card value to prevent cheating
        let dummy_card = Card {
            value: Value::Two,
            suit: Suit::Spade,
        };

        Ok(CardProvenance {
            round_id: provenance.round_id,
            card: dummy_card,
            original_position: provenance.original_position,
            shuffled_position: provenance.shuffled_position,
            card_hash: provenance.card_hash,
            dealt_to: provenance.dealt_to,
            dealt_at_stage: provenance.dealt_at_stage,
        })
    } else {
        // After game: reveal actual card
        Ok(provenance)
    }
}

/// Get card provenance for a specific round (or current round if None)
/// Players can only see their own cards and community cards during gameplay
#[ic_cdk::query]
fn get_card_provenance_by_round_id(
    round_id: Option<u64>,
) -> Result<Vec<CardProvenance>, TableError> {
    let table = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table = table.as_ref().ok_or(TableError::TableNotFound)?;

    let target_round = round_id.unwrap_or(table.round_ticker);
    let caller = WalletPrincipalId(ic_cdk::api::msg_caller());

    // 🔒 SECURITY: Check if game has ended
    let game_ended = table.sorted_users.is_some();

    // Get caller's cards (if they're playing)
    let caller_cards: Vec<Card> = table
        .user_table_data
        .get(&caller)
        .map(|user_data| user_data.cards.clone())
        .unwrap_or_default();

    // Get community cards
    let community_cards = table.community_cards.clone();

    let mut provenance: Vec<CardProvenance> = table
        .card_provenance
        .values()
        .filter(|prov| prov.round_id == target_round)
        .map(|prov| {
            if game_ended {
                // After game: reveal all cards
                return prov.clone();
            }

            // During gameplay: check if this card should be visible to caller
            let card_matches = |c1: &Card, c2: &Card| -> bool {
                format!("{:?}:{:?}", c1.value, c1.suit) == format!("{:?}:{:?}", c2.value, c2.suit)
            };

            let is_callers_card = caller_cards.iter().any(|c| card_matches(c, &prov.card));
            let is_community_card = community_cards.iter().any(|c| card_matches(c, &prov.card));

            if is_callers_card || is_community_card {
                // Reveal this card (it's theirs or community)
                prov.clone()
            } else {
                // Hide this card (belongs to another player or not dealt yet)
                let dummy_card = Card {
                    value: Value::Two,
                    suit: Suit::Spade,
                };

                CardProvenance {
                    round_id: prov.round_id,
                    card: dummy_card,
                    original_position: prov.original_position,
                    shuffled_position: prov.shuffled_position,
                    card_hash: prov.card_hash.clone(),
                    dealt_to: prov.dealt_to.clone(),
                    dealt_at_stage: prov.dealt_at_stage.clone(),
                }
            }
        })
        .collect();

    // Sort by shuffled_position to ensure consistent order (0-51)
    // Frontend displays cards by shuffled_position, so sorting here makes the response predictable
    provenance.sort_by_key(|p| p.shuffled_position);

    Ok(provenance)
}

/// Get card provenance history for recent rounds (for historical transparency)
/// Returns data from the last N rounds (default 10, max 50)
/// This is for viewing historical game data, not current game state
#[ic_cdk::query]
fn get_all_card_provenance(limit: Option<u64>) -> Result<Vec<CardProvenance>, TableError> {
    let table = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table = table.as_ref().ok_or(TableError::TableNotFound)?;

    let limit = limit.unwrap_or(10).min(50); // Default 10, max 50
    let total_rounds = table.rng_history.len();
    let start_idx = if total_rounds > limit as usize {
        total_rounds - limit as usize
    } else {
        0
    };

    // Collect round IDs from recent history
    let recent_round_ids: Vec<u64> = table
        .rng_history
        .iter()
        .skip(start_idx)
        .map(|rng| rng.round_id)
        .collect();

    // Get all provenance for these rounds
    let provenance: Vec<CardProvenance> = table
        .card_provenance
        .values()
        .filter(|prov| recent_round_ids.contains(&prov.round_id))
        .cloned()
        .collect();

    Ok(provenance)
}

/// Verify the shuffle for a specific round by recalculating the hash
///
/// 🔒 SECURITY: Verification is only allowed after the game ends to prevent
/// time_seed exposure during gameplay, which would allow deck reconstruction.
#[ic_cdk::query]
fn verify_shuffle(round_id: u64) -> Result<bool, TableError> {
    let table = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table = table.as_ref().ok_or(TableError::TableNotFound)?;

    let rng_metadata = table
        .rng_history
        .iter()
        .find(|rng| rng.round_id == round_id)
        .ok_or_else(|| {
            TableError::InvalidRequest(format!("RNG metadata not found for round {}", round_id))
        })?;

    // 🔒 SECURITY: Check if game has ended before allowing verification
    // This prevents time_seed exposure during gameplay
    let game_ended = table.sorted_users.is_some();
    let is_current_round = rng_metadata.round_id == table.round_ticker;

    if !game_ended && is_current_round {
        return Err(TableError::InvalidRequest(
            "Verification is only available after the game ends".to_string(),
        ));
    }

    // Recreate the deck from the RNG metadata
    let mut shuffled_bytes = rng_metadata.raw_random_bytes.clone();
    reshuffle_bytes_hash(&mut shuffled_bytes, rng_metadata.time_seed);
    let deck = FlatDeck::new(shuffled_bytes);

    // Calculate hash and compare
    let calculated_hash = calculate_deck_hash(&deck);
    let matches = calculated_hash == rng_metadata.deck_hash;

    Ok(matches)
}

/// Get RNG statistics summary
#[ic_cdk::query]
fn get_rng_stats() -> Result<RngStats, TableError> {
    let table = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table = table.as_ref().ok_or(TableError::TableNotFound)?;

    Ok(RngStats {
        total_rounds: table.rng_history.len() as u64,
        total_cards_tracked: table.card_provenance.len() as u64,
        oldest_round_id: table.rng_history.first().map(|r| r.round_id),
        latest_round_id: table.rng_history.last().map(|r| r.round_id),
        current_round_ticker: table.round_ticker,
    })
}

// ============================================================================

#[allow(dependency_on_unit_never_type_fallback)]
#[ic_cdk::update]
async fn join_table(
    users_canister_principal: UsersCanisterId,
    user_id: WalletPrincipalId,
    seat_index: Option<u64>, // javascript can't send u8
    deposit_amount: u64,
    player_sitting_out: bool,
) -> Result<PublicTable, TableError> {
    handle_cycle_check().await;
    if deposit_amount == 0 {
        return Err(TableError::InvalidRequest(
            "Deposit amount cannot be 0".to_string(),
        ));
    }

    let table = {
        let table = TABLE.lock().map_err(|_| TableError::LockError)?;
        table.as_ref().ok_or(TableError::TableNotFound)?.clone()
    };

    if table.big_blind.0 > deposit_amount {
        return Err(TableError::InsufficientFunds);
    }

    let seat_index = match seat_index {
        Some(seat_index) => seat_index as u8,
        None => table
            .get_free_seat_index()
            .ok_or(TableError::InvalidRequest("Table is full.".to_string()))?,
    };

    {
        let user = get_user_wrapper(users_canister_principal, user_id).await?;
        if !user.can_play() {
            return Err(TableError::UserBanned);
        }
        if let Some(require_proof_of_humanity) = table.config.require_proof_of_humanity {
            if require_proof_of_humanity && !user.is_verified.unwrap_or(false) {
                return Err(TableError::UserNotVerified);
            }
        }
    }

    match table.config.currency_type {
        CurrencyType::Real(currency) => {
            let currency_manager = {
                let currency_manager =
                    CURRENCY_MANAGER.lock().map_err(|_| TableError::LockError)?;
                currency_manager
                    .as_ref()
                    .ok_or(TableError::StateNotInitialized)?
                    .clone()
            };

            currency_manager
                .validate_allowance(&currency, user_id.0, deposit_amount)
                .await?;
        }
        CurrencyType::Fake => {}
    }

    let user = add_users_active_table(
        users_canister_principal,
        user_id,
        TableId(ic_cdk::api::canister_self()),
    )
    .await;
    let mut user = user?;
    user.balance.0 = deposit_amount;

    if table.users.users.contains_key(&user_id) || table.is_user_in_table(user_id) {
        return Err(TableError::UserAlreadyInGame);
    }

    let (ret, mut table_state) = {
        let mut table_state = TABLE.lock().map_err(|_| TableError::LockError)?;
        let table_state = table_state.as_mut().ok_or(TableError::TableNotFound)?;
        let ret = table_state.add_user(user.clone(), seat_index, player_sitting_out);
        (ret.map_err(|e| e.into_inner()), table_state.clone())
    };

    if let Err(e) = ret {
        ic_cdk::println!("Error adding user to table: {:?}", e);
        remove_users_active_table(
            users_canister_principal,
            user_id,
            TableId(ic_cdk::api::canister_self()),
        )
        .await?;
        return Err(e.into());
    }

    match table.config.currency_type {
        CurrencyType::Real(currency) => {
            let mut transaction_state = TRANSACTION_STATE
                .lock()
                .map_err(|_| TableError::LockError)?
                .clone();
            let currency_manager = {
                let currency_manager =
                    CURRENCY_MANAGER.lock().map_err(|_| TableError::LockError)?;
                currency_manager
                    .as_ref()
                    .ok_or(TableError::StateNotInitialized)?
                    .clone()
            };
            match currency_manager
                .deposit(&mut transaction_state, &currency, user_id.0, deposit_amount)
                .await
            {
                Ok(_) => {
                    *TRANSACTION_STATE
                        .lock()
                        .map_err(|_| TableError::LockError)? = transaction_state;
                }
                Err(e) => {
                    ic_cdk::println!("Error depositing: {:?}", e);
                    remove_users_active_table(
                        users_canister_principal,
                        user_id,
                        TableId(ic_cdk::api::canister_self()),
                    )
                    .await?;

                    let mut table = TABLE.lock().map_err(|_| TableError::LockError)?;
                    let table = table.as_mut().ok_or(TableError::TableNotFound)?;
                    table
                        .remove_user(user_id, ActionType::Leave)
                        .map_err(|e| e.into_inner())?;
                    return Err(e.into());
                }
            }
        }
        CurrencyType::Fake => {}
    }

    let is_paused = table_state.config.is_paused.unwrap_or(false);

    if table_state.number_of_players() >= 2 && !table_state.is_game_ongoing() && !is_paused {
        if let Err(e) = start_new_betting_round_wrapper(TableId(ic_cdk::api::canister_self())).await
        {
            ic_cdk::println!("Error starting new betting round: {:?}", e);
            if matches!(e, TableError::CanisterCallError(_)) {
                return Err(e);
            }
        }
    }
    let caller = WalletPrincipalId(ic_cdk::api::msg_caller());
    table_state.hide_cards(caller).map_err(|e| e.into_inner())?;
    let res = match table_state.config.table_type {
        Some(TableType::Cash) => update_table_player_count(table_state.users.len()),
        _ => update_player_count_tournament(UserTournamentAction::Join(user_id)),
    };
    if let Err(e) = res {
        ic_cdk::println!("Error updating table player count: {:?}", e);
    }

    Ok(table_state.into())
}

#[ic_cdk::update]
async fn kick_player(
    users_canister_principal: UsersCanisterId,
    user_id: WalletPrincipalId,
    balance: u64,
) -> Result<PublicTable, TableError> {
    handle_cycle_check().await;

    let mut table = {
        let mut table = TABLE.lock().map_err(|_| TableError::LockError)?;
        let table = table.as_mut().ok_or(TableError::TableNotFound)?;

        if table.has_user_left(user_id) {
            return Err(TableError::InvalidRequest(
                "User has already left the table".to_string(),
            ));
        }

        let backend_principal = BACKEND_PRINCIPAL
            .lock()
            .map_err(|_| TableError::LockError)?
            .ok_or(TableError::CanisterCallError(
                "Backend principal not found.".to_string(),
            ))?;

        validate_caller(vec![backend_principal, table.id.0]);

        if is_table_game_ongoing(table) {
            if table.is_players_turn(user_id) {
                if let Err(e) = table.user_fold(user_id, false) {
                    ic_cdk::println!("Error folding user: {:?}", e);
                }
            } else if let Err(e) = table.user_pre_fold(user_id) {
                ic_cdk::println!("Error pre-folding user: {:?}", e);
            }
        }

        table.clone()
    };

    match &table.config.currency_type {
        CurrencyType::Real(currency) => {
            ic_cdk::println!("Balance: {}", balance);

            let fee = if table.config.currency_type == CurrencyType::Real(currency::Currency::BTC) {
                10
            } else {
                ic_ledger_types::DEFAULT_FEE.e8s()
            };
            if balance > fee {
                ic_cdk::println!("Withdrawing: {}", balance);
                let currency_manager = {
                    let currency_manager =
                        CURRENCY_MANAGER.lock().map_err(|_| TableError::LockError)?;
                    currency_manager
                        .as_ref()
                        .ok_or(TableError::StateNotInitialized)?
                        .clone()
                };

                currency_manager
                    .withdraw(currency, user_id.0, balance)
                    .await?;
            }
        }
        CurrencyType::Fake => {}
    }

    ic_cdk::futures::spawn(async move {
        if let Err(e) = remove_users_active_table(
            users_canister_principal,
            user_id,
            TableId(ic_cdk::api::canister_self()),
        )
        .await
        {
            ic_cdk::println!("Error removing active table: {}", e);
        }
    });

    {
        let mut table_state = TABLE.lock().map_err(|_| TableError::LockError)?;
        let table_state = table_state.as_mut().ok_or(TableError::TableNotFound)?;
        table_state
            .remove_user(user_id, ActionType::Leave)
            .map_err(|e| e.into_inner())?;
    }

    let caller = WalletPrincipalId(ic_cdk::api::msg_caller());
    table.hide_cards(caller).map_err(|e| e.into_inner())?;
    let res = match table.config.table_type {
        Some(TableType::Cash) => update_table_player_count(table.users.len()),
        _ => update_player_count_tournament(UserTournamentAction::Join(user_id)),
    };
    if let Err(e) = res {
        ic_cdk::println!("Error updating table player count: {:?}", e);
    }

    if table.users.users.is_empty() {
        handle_last_user_leaving().await?;
    }

    Ok(table.into())
}

#[ic_cdk::update]
async fn leave_table(
    users_canister_id: UsersCanisterId,
    user_id: WalletPrincipalId,
) -> Result<PublicTable, TableError> {
    handle_cycle_check().await;

    ic_cdk::futures::spawn(async move {
        for _ in 0..3 {
            match remove_users_active_table(
                users_canister_id,
                user_id,
                TableId(ic_cdk::api::canister_self()),
            )
            .await
            {
                Ok(_) => {
                    ic_cdk::println!("User {} removed from active table", user_id.0.to_text());
                    break;
                }
                Err(e) => {
                    ic_cdk::println!("Error removing active table: {}", e);
                }
            }
        }
    });

    let (mut table, balance) = {
        let mut table = TABLE.lock().map_err(|_| TableError::LockError)?;
        let table = table.as_mut().ok_or(TableError::TableNotFound)?;

        if table.has_user_left(user_id) {
            return Err(TableError::InvalidRequest(
                "User has already left the table".to_string(),
            ));
        }

        let backend_principal = BACKEND_PRINCIPAL
            .lock()
            .map_err(|_| TableError::LockError)?
            .ok_or(TableError::CanisterCallError(
                "Backend principal not found.".to_string(),
            ))?;

        let user = table.users.get(&user_id).ok_or(TableError::UserNotFound)?;
        validate_caller(vec![
            user_id.0,
            user.principal_id.0,
            backend_principal,
            table.id.0,
        ]);

        if is_table_game_ongoing(table) {
            if table.is_players_turn(user_id) {
                table
                    .user_fold(user_id, false)
                    .map_err(|e| e.into_inner())?;
            } else {
                table.user_pre_fold(user_id).map_err(|e| e.into_inner())?;
            }
        }

        let balance = table
            .users
            .get(&user_id)
            .map(|user| user.balance)
            .unwrap_or_default();

        (table.clone(), balance)
    };

    match &table.config.currency_type {
        CurrencyType::Real(currency) => {
            ic_cdk::println!("Balance: {}", balance.0);

            let fee = if table.config.currency_type == CurrencyType::Real(currency::Currency::BTC) {
                10
            } else {
                ic_ledger_types::DEFAULT_FEE.e8s()
            };
            if balance.0 > fee {
                ic_cdk::println!("Withdrawing: {}", balance.0);
                let currency_manager = {
                    let currency_manager =
                        CURRENCY_MANAGER.lock().map_err(|_| TableError::LockError)?;
                    currency_manager
                        .as_ref()
                        .ok_or(TableError::StateNotInitialized)?
                        .clone()
                };

                currency_manager
                    .withdraw(currency, user_id.0, balance.0)
                    .await?;
            }
        }
        CurrencyType::Fake => {}
    }

    {
        let mut table_state = TABLE.lock().map_err(|_| TableError::LockError)?;
        let table_state = table_state.as_mut().ok_or(TableError::TableNotFound)?;
        table_state
            .remove_user(user_id, ActionType::Leave)
            .map_err(|e| e.into_inner())?;
    }

    let caller = WalletPrincipalId(ic_cdk::api::msg_caller());
    table.hide_cards(caller).map_err(|e| e.into_inner())?;
    let res = match table.config.table_type {
        Some(TableType::Cash) => update_table_player_count(table.users.len()),
        _ => update_player_count_tournament(UserTournamentAction::Leave(user_id)),
    };
    if let Err(e) = res {
        ic_cdk::println!("Error updating table player count: {:?}", e);
    }

    if table.users.users.is_empty() {
        handle_last_user_leaving().await?;
    }

    Ok(table.into())
}

#[ic_cdk::update]
async fn leave_table_for_table_balancing(
    users_canister_id: UsersCanisterId,
    user_id: WalletPrincipalId,
    table_to_move_to_id: TableId,
) -> Result<PublicTable, TableError> {
    handle_cycle_check().await;

    let mut table = {
        let mut table = TABLE.lock().map_err(|_| TableError::LockError)?;
        let table = table.as_mut().ok_or(TableError::TableNotFound)?;

        if table.has_user_left(user_id) {
            return Err(TableError::InvalidRequest(
                "User has already left the table".to_string(),
            ));
        }

        let backend_principal = BACKEND_PRINCIPAL
            .lock()
            .map_err(|_| TableError::LockError)?
            .ok_or(TableError::CanisterCallError(
                "Backend principal not found.".to_string(),
            ))?;

        let user = table.users.get(&user_id).ok_or(TableError::UserNotFound)?;
        validate_caller(vec![
            user_id.0,
            user.principal_id.0,
            backend_principal,
            table.id.0,
        ]);

        table.clone()
    };

    ic_cdk::futures::spawn(async move {
        if let Err(e) = remove_users_active_table(
            users_canister_id,
            user_id,
            TableId(ic_cdk::api::canister_self()),
        )
        .await
        {
            ic_cdk::println!("Error removing active table: {}", e);
        }
    });

    {
        let mut table_state = TABLE.lock().map_err(|_| TableError::LockError)?;
        let table_state = table_state.as_mut().ok_or(TableError::TableNotFound)?;
        table_state
            .remove_user_for_table_balancing(users_canister_id, user_id, table_to_move_to_id)
            .map_err(|e| e.into_inner())?;
    }

    let caller = WalletPrincipalId(ic_cdk::api::msg_caller());
    table.hide_cards(caller).map_err(|e| e.into_inner())?;
    let res = match table.config.table_type {
        Some(TableType::Cash) => update_table_player_count(table.users.len()),
        _ => update_player_count_tournament(UserTournamentAction::Leave(user_id)),
    };
    if let Err(e) = res {
        ic_cdk::println!("Error updating table player count: {:?}", e);
    }

    Ok(table.into())
}

#[ic_cdk::update]
async fn withdraw_from_table(user_id: WalletPrincipalId, amount: u64) -> Result<(), TableError> {
    handle_cycle_check().await;
    handle_table_validity_check()?;
    if amount == 0 {
        return Err(TableError::InvalidRequest(
            "Withdraw amount cannot be 0".to_string(),
        ));
    }

    let table = {
        let mut table = TABLE.lock().map_err(|_| TableError::LockError)?;
        let table = table.as_mut().ok_or(TableError::TableNotFound)?;
        let user = table.users.get(&user_id).ok_or(TableError::UserNotFound)?;

        let backend_principal = BACKEND_PRINCIPAL
            .lock()
            .map_err(|_| TableError::LockError)?
            .ok_or(TableError::CanisterCallError(
                "Backend principal not found.".to_string(),
            ))?;
        validate_caller(vec![user_id.0, user.principal_id.0, backend_principal]);

        let user_table_data = table
            .get_user_table_data(user_id)
            .map_err(|e| e.into_inner())?;
        if is_table_game_ongoing(table) && user_table_data.player_action != PlayerAction::Folded {
            ic_cdk::println!("Error withdrawing from table: Game is ongoing");
            return Err(GameError::ActionNotAllowed {
                reason: "Game is ongoing. You need to fold to withdraw from an ongoing game."
                    .to_string(),
            }
            .into());
        }

        let user = table
            .users
            .get_mut(&user_id)
            .ok_or(TableError::UserNotFound)?;
        if user.balance.0 < amount {
            return Err(GameError::InsufficientFunds.into());
        }
        user.withdraw(amount);
        table.clone()
    };

    match table.config.currency_type {
        CurrencyType::Real(currency) => {
            let currency_manager = {
                let currency_manager =
                    CURRENCY_MANAGER.lock().map_err(|_| TableError::LockError)?;
                currency_manager
                    .as_ref()
                    .ok_or(TableError::StateNotInitialized)?
                    .clone()
            };

            currency_manager
                .withdraw(&currency, user_id.0, amount)
                .await?;
        }
        CurrencyType::Fake => {}
    }
    Ok(())
}

#[ic_cdk::update]
async fn deposit_to_table(
    users_canister_id: UsersCanisterId,
    user_id: WalletPrincipalId,
    amount: u64,
    is_queued: bool,
) -> Result<ReturnResult, TableError> {
    handle_cycle_check().await;
    if amount == 0 {
        return Err(TableError::InvalidRequest(
            "Deposit amount cannot be 0".to_string(),
        ));
    }

    {
        let table = {
            let table = TABLE.lock().map_err(|_| TableError::LockError)?;
            table.as_ref().ok_or(TableError::TableNotFound)?.clone()
        };
        match table.config.currency_type {
            CurrencyType::Real(currency) => {
                let currency_manager = {
                    let currency_manager =
                        CURRENCY_MANAGER.lock().map_err(|_| TableError::LockError)?;
                    currency_manager
                        .as_ref()
                        .ok_or(TableError::StateNotInitialized)?
                        .clone()
                };

                if let Err(e) = currency_manager
                    .validate_allowance(&currency, user_id.0, amount)
                    .await
                {
                    return Err(e.into());
                }
            }
            CurrencyType::Fake => {}
        }
    }

    let table = {
        let mut table = TABLE.lock().map_err(|_| TableError::LockError)?;
        let table = table.as_mut().ok_or(TableError::TableNotFound)?;

        if table.is_game_ongoing()
            && !is_queued
            && table
                .get_user_table_data(user_id)
                .map_err(|e| e.into_inner())?
                .player_action
                != PlayerAction::Folded
        {
            table.append_to_queue(QueueItem::Deposit(user_id, users_canister_id, amount));
            return Ok(ReturnResult::DepositQueued);
        }

        let user = table
            .users
            .get_mut(&user_id)
            .ok_or(TableError::UserNotFound)?;
        user.deposit(amount);
        table.clone()
    };

    match table.config.currency_type {
        CurrencyType::Real(currency) => {
            let currency_manager = {
                let currency_manager =
                    CURRENCY_MANAGER.lock().map_err(|_| TableError::LockError)?;
                currency_manager
                    .as_ref()
                    .ok_or(TableError::StateNotInitialized)?
                    .clone()
            };

            let mut transaction_state = {
                let transaction_state = TRANSACTION_STATE
                    .lock()
                    .map_err(|_| TableError::LockError)?;
                transaction_state.clone()
            };

            match currency_manager
                .deposit(&mut transaction_state, &currency, user_id.0, amount)
                .await
            {
                Ok(_) => {
                    *TRANSACTION_STATE
                        .lock()
                        .map_err(|_| TableError::LockError)? = transaction_state;
                }
                Err(e) => {
                    ic_cdk::println!("Error depositing: {:?}", e);
                    let mut table = TABLE.lock().map_err(|_| TableError::LockError)?;
                    let table = table.as_mut().ok_or(TableError::TableNotFound)?;
                    let user = table
                        .users
                        .get_mut(&user_id)
                        .ok_or(TableError::UserNotFound)?;
                    user.withdraw(amount);
                    return Err(e.into());
                }
            }
        }
        CurrencyType::Fake => {}
    }
    Ok(ReturnResult::DepositSuccessful)
}

#[ic_cdk::update]
async fn set_player_action(
    user_principal: WalletPrincipalId,
    player_action: PlayerAction,
) -> Result<(), TableError> {
    handle_cycle_check().await;
    let mut table_state = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table_state = table_state.as_mut().ok_or(TableError::TableNotFound)?;
    let user = table_state
        .users
        .get(&user_principal)
        .ok_or(TableError::UserNotFound)?;
    let backend_principal = BACKEND_PRINCIPAL
        .lock()
        .map_err(|_| TableError::LockError)?
        .ok_or(TableError::CanisterCallError(
            "Backend principal not found.".to_string(),
        ))?;
    validate_caller(vec![
        user_principal.0,
        user.principal_id.0,
        backend_principal,
    ]);

    table_state
        .set_player_action(user_principal, player_action)
        .map_err(|e| e.into_inner())?;
    Ok(())
}

// #[ic_cdk::update]
// fn set_auto_check_fold(user_principal: WalletPrincipalId, enabled: bool) -> Result<(), TableError> {
//     handle_cycle_check().await;
//     let mut table_state = TABLE.lock().map_err(|_| TableError::LockError)?;
//     let table_state = table_state.as_mut().ok_or(TableError::TableNotFound)?;
//     let user = table_state
//         .users
//         .get(&user_principal)
//         .ok_or(TableError::UserNotFound)?;
//     let backend_principal = BACKEND_PRINCIPAL
//         .lock()
//         .map_err(|_| TableError::LockError)?
//         .ok_or(TableError::CanisterCallError(
//             "Backend principal not found.".to_string(),
//         ))?;
//     validate_caller(vec![user_principal.0, user.principal_id.0, backend_principal]);

//     if table_state.is_players_turn(user_principal) {
//         if table_state.is_users_current_total_bet_equal_to_highest_bet(user_principal) {
//             table_state
//                 .user_check(user_principal, false)
//                 .map_err(|e| e.into_inner())?;
//             if table_state.is_everyone_auto_check_fold(user_principal) {
//                 table_state.calculate_pots().map_err(|e| e.into_inner())?;
//                 table_state
//                     .cycle_to_showdown()
//                     .map_err(|e| e.into_inner())?;
//             } else {
//                 table_state
//                     .set_user_auto_check_fold(user_principal, enabled)
//                     .map_err(|e| e.into_inner())?;
//             }
//         } else {
//             table_state
//                 .user_fold(user_principal, false)
//                 .map_err(|e| e.into_inner())?;
//         }
//     } else {
//         table_state
//             .set_user_auto_check_fold(user_principal, enabled)
//             .map_err(|e| e.into_inner())?;
//     }

//     Ok(())
// }

#[ic_cdk::update]
async fn player_sitting_out(user_principal: WalletPrincipalId) -> Result<(), TableError> {
    handle_cycle_check().await;
    let mut table_state = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table_state = table_state.as_mut().ok_or(TableError::TableNotFound)?;
    let user = table_state
        .users
        .get(&user_principal)
        .ok_or(TableError::UserNotFound)?;
    let backend_principal = BACKEND_PRINCIPAL
        .lock()
        .map_err(|_| TableError::LockError)?
        .ok_or(TableError::CanisterCallError(
            "Backend principal not found.".to_string(),
        ))?;
    validate_caller(vec![
        user_principal.0,
        user.principal_id.0,
        backend_principal,
    ]);

    if table_state.users.get(&user_principal).is_none() {
        return Err(GameError::ActionNotAllowed {
            reason: "User not in table".to_string(),
        }
        .into());
    }

    table_state
        .user_sitting_out(user_principal, false)
        .map_err(|e| e.into_inner())?;

    Ok(())
}

#[ic_cdk::update]
async fn player_sitting_in(
    users_canister_id: UsersCanisterId,
    user_id: WalletPrincipalId,
    auto_start: bool,
) -> Result<(), TableError> {
    handle_cycle_check().await;
    let mut table_state = {
        let mut table_state = TABLE.lock().map_err(|_| TableError::LockError)?;
        let table_state = table_state.as_mut().ok_or(TableError::TableNotFound)?;
        let user_table_data = table_state
            .get_user_table_data_mut(user_id)
            .map_err(|e| e.into_inner())?;
        user_table_data.seated_out_turns = 0;
        let user = table_state
            .users
            .get(&user_id)
            .ok_or(TableError::UserNotFound)?;
        let backend_principal = BACKEND_PRINCIPAL
            .lock()
            .map_err(|_| TableError::LockError)?
            .ok_or(TableError::CanisterCallError(
                "Backend principal not found.".to_string(),
            ))?;
        validate_caller(vec![user_id.0, user.principal_id.0, backend_principal]);
        table_state.clone()
    };

    match table_state.users.get(&user_id) {
        Some(user) => {
            if user.balance.0 < table_state.big_blind.0 {
                table_state
                    .kick_user(user_id, "Insufficient Funds".to_string())
                    .map_err(|e| e.into_inner())?;
                remove_users_active_table(
                    users_canister_id,
                    user_id,
                    TableId(ic_cdk::api::canister_self()),
                )
                .await?;

                *TABLE.lock().map_err(|_| TableError::LockError)? = Some(table_state);
                return Err(GameError::InsufficientFunds.into());
            }
        }
        None => {
            ic_cdk::println!("User not in table");
            return Err(GameError::ActionNotAllowed {
                reason: "User not in table".to_string(),
            }
            .into());
        }
    }

    let user_table_data = table_state
        .get_user_table_data(user_id)
        .map_err(|e| e.into_inner())?;
    if user_table_data.player_action != PlayerAction::SittingOut {
        return Err(GameError::ActionNotAllowed {
            reason: "User not sitting out".to_string(),
        }
        .into());
    }

    if table_state.is_game_ongoing() {
        let is_game_paused = table_state
            .get_playing_users()
            .map_err(|e| e.into_inner())?
            < 2;
        table_state.append_to_queue(QueueItem::SittingIn(user_id, is_game_paused));
        *TABLE.lock().map_err(|_| TableError::LockError)? = Some(table_state);
        return Ok(());
    } else {
        table_state
            .set_player_action(user_id, PlayerAction::None)
            .map_err(|e| e.into_inner())?;
        *TABLE.lock().map_err(|_| TableError::LockError)? = Some(table_state.clone());
    }

    if table_state.number_of_players() >= 2
        && table_state.number_of_active_players() >= 2
        && !table_state.is_game_ongoing()
        && auto_start
    {
        if let Err(e) = start_new_betting_round_wrapper(TableId(ic_cdk::api::canister_self())).await
        {
            ic_cdk::println!("Error starting new betting round: {:?}", e);
            if matches!(e, TableError::CanisterCallError(_)) {
                return Err(e);
            }
        }
    }

    Ok(())
}

#[ic_cdk::update]
async fn place_bet(user_principal: WalletPrincipalId, bet_type: BetType) -> Result<(), TableError> {
    handle_cycle_check().await;

    let mut table_state = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table_state = table_state.as_mut().ok_or(TableError::TableNotFound)?;
    let user = table_state
        .users
        .get(&user_principal)
        .ok_or(TableError::UserNotFound)?;
    let backend_principal = BACKEND_PRINCIPAL
        .lock()
        .map_err(|_| TableError::LockError)?
        .ok_or(TableError::CanisterCallError(
            "Backend principal not found.".to_string(),
        ))?;
    validate_caller(vec![
        user_principal.0,
        user.principal_id.0,
        backend_principal,
    ]);

    if table_state
        .get_player_at_seat(table_state.current_player_index)
        .map_err(|e| e.into_inner())?
        != user_principal
    {
        return Err(GameError::ActionNotAllowed {
            reason: "Not your turn".to_string(),
        }
        .into());
    }

    table_state
        .bet(user_principal, bet_type)
        .map_err(|e| e.into_inner())?;
    Ok(())
}

#[ic_cdk::update]
async fn fold(user_principal: WalletPrincipalId, is_pre_fold: bool) -> Result<(), TableError> {
    handle_cycle_check().await;

    let mut table_state = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table_state = table_state.as_mut().ok_or(TableError::TableNotFound)?;
    let user = table_state
        .users
        .get(&user_principal)
        .ok_or(TableError::UserNotFound)?;
    let backend_principal = BACKEND_PRINCIPAL
        .lock()
        .map_err(|_| TableError::LockError)?
        .ok_or(TableError::CanisterCallError(
            "Backend principal not found.".to_string(),
        ))?;
    validate_caller(vec![
        user_principal.0,
        user.principal_id.0,
        backend_principal,
    ]);

    if is_pre_fold {
        table_state
            .user_pre_fold(user_principal)
            .map_err(|e| e.into_inner())?;
    } else {
        table_state
            .user_fold(user_principal, false)
            .map_err(|e| e.into_inner())?;
    }

    Ok(())
}

#[ic_cdk::update]
async fn check(user_principal: WalletPrincipalId) -> Result<(), TableError> {
    handle_cycle_check().await;

    let mut table_state = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table_state = table_state.as_mut().ok_or(TableError::TableNotFound)?;
    let user = table_state
        .users
        .get(&user_principal)
        .ok_or(TableError::UserNotFound)?;
    let backend_principal = BACKEND_PRINCIPAL
        .lock()
        .map_err(|_| TableError::LockError)?
        .ok_or(TableError::CanisterCallError(
            "Backend principal not found.".to_string(),
        ))?;
    validate_caller(vec![
        user_principal.0,
        user.principal_id.0,
        backend_principal,
    ]);

    if let SeatStatus::Occupied(current_player) =
        table_state.seats[table_state.current_player_index]
    {
        if current_player != user_principal {
            return Ok(());
        }
    } else {
        return Err(TableError::UserNotFound);
    }

    if table_state
        .get_player_at_seat(table_state.current_player_index)
        .map_err(|e| e.into_inner())?
        != user_principal
    {
        return Err(GameError::ActionNotAllowed {
            reason: "Not your turn".to_string(),
        }
        .into());
    }

    table_state
        .user_check(user_principal, false)
        .map_err(|e| e.into_inner())?;
    Ok(())
}

#[ic_cdk::update]
async fn handle_timer_expiration(user_id: WalletPrincipalId) -> Result<(), TableError> {
    handle_cycle_check().await;
    let mut table_state = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table_state = table_state.as_mut().ok_or(TableError::TableNotFound)?;
    let backend_principal = BACKEND_PRINCIPAL
        .lock()
        .map_err(|_| TableError::LockError)?
        .ok_or(TableError::CanisterCallError(
            "Backend principal not found.".to_string(),
        ))?;
    let mut valid_callers = (*CONTROLLER_PRINCIPALS).clone();
    valid_callers.push(backend_principal);
    valid_callers.push(table_state.id.0);
    validate_caller(valid_callers);

    let res = if table_state.is_users_current_total_bet_equal_to_highest_bet(user_id) {
        table_state
            .user_check(user_id, true)
            .map_err(|e| e.into_inner())
    } else {
        table_state.force_fold(user_id).map_err(|e| e.into_inner())
    };

    res?;

    Ok(())
}

#[ic_cdk::update]
async fn start_new_betting_round() -> Result<(), TableError> {
    handle_cycle_check().await;

    let raw_bytes = ic_cdk::management_canister::raw_rand().await;
    let raw_bytes = raw_bytes.map_err(|e| {
        TableError::CanisterCallError(format!("Failed to generate random bytes: {:?}", e))
    })?;

    // Store original bytes BEFORE reshuffling for transparency
    let original_random_bytes = raw_bytes.clone();

    // Get current time in nanoseconds as seed
    let time_seed = ic_cdk::api::time();

    // Reshuffle the bytes using time as seed
    let mut shuffled_bytes = raw_bytes;
    reshuffle_bytes_hash(&mut shuffled_bytes, time_seed);

    // Create deck to capture the shuffle result
    let deck = FlatDeck::new(shuffled_bytes.clone());

    let (kicked_players, action_logs, table_id, seated_out_kicked_players, users) = {
        let mut table_state = TABLE.lock().map_err(|_| TableError::LockError)?;
        let table_state = table_state.as_mut().ok_or(TableError::TableNotFound)?;
        let backend_principal = BACKEND_PRINCIPAL
            .lock()
            .map_err(|_| TableError::LockError)?
            .ok_or(TableError::CanisterCallError(
                "Backend principal not found.".to_string(),
            ))?;

        validate_caller(vec![backend_principal, table_state.id.0]);

        if table_state.config.is_paused.unwrap_or(false) {
            ic_cdk::println!("Game is paused.");
            return Err(GameError::ActionNotAllowed {
                reason: "Game is paused".to_string(),
            }
            .into());
        }

        if table_state.is_game_ongoing() {
            return Err(GameError::ActionNotAllowed {
                reason: "Game is ongoing".to_string(),
            }
            .into());
        } else if table_state.config.is_paused.unwrap_or(false) {
            table_state.status = TableStatus::Paused;
            return Err(GameError::ActionNotAllowed {
                reason: "Game is paused".to_string(),
            }
            .into());
        }

        let action_logs = table_state.action_logs.clone();

        // ✅ Call start_betting_round FIRST (before storing RNG data)
        // This ensures we only store RNG data if the round successfully starts
        let (kicked_players, seated_out_kicked_players) =
            match table_state.start_betting_round(shuffled_bytes) {
                Ok(kicked_players) => kicked_players,
                Err(e) => {
                    ic_cdk::println!("Error starting betting round: {:?}", e);
                    return Err(e.into_inner().into());
                }
            };

        // 🆕 CREATE RNG METADATA for transparency (only after successful round start)
        // round_ticker has already been incremented in start_betting_round, so use it directly
        let round_id = table_state.round_ticker;
        let deck_hash = calculate_deck_hash(&deck);

        // 🔒 SECURITY: Store empty shuffled_deck initially (commit phase)
        // Full deck will be revealed only after the game ends (reveal phase)
        let rng_metadata = RngMetadata {
            round_id,
            raw_random_bytes: original_random_bytes,
            time_seed,
            timestamp_ns: time_seed,
            deck_hash,
            ic_transaction_id: None, // Can be enhanced later with actual transaction ID
            shuffled_deck: Vec::new(), // ✅ Empty until game ends
        };

        // 🆕 GENERATE CARD PROVENANCE
        let card_provenance = generate_card_provenance(&deck, round_id);

        // 🆕 STORE RNG DATA IN TABLE
        table_state.rng_history.push(rng_metadata);
        table_state.card_provenance.extend(card_provenance);

        ic_cdk::println!(
            "✅ RNG captured for round {}: {} cards, hash: {}",
            round_id,
            table_state.card_provenance.len(),
            table_state
                .rng_history
                .last()
                .map(|r| r.deck_hash.clone())
                .unwrap_or_default()
        );

        (
            kicked_players,
            action_logs,
            table_state.id,
            seated_out_kicked_players,
            table_state.users.clone(),
        )
    };

    let mut total_kicked_players = kicked_players.clone();
    total_kicked_players.extend(seated_out_kicked_players.clone());
    let mut total_kicked_users = Vec::new();
    for (user_principal, balance) in total_kicked_players {
        let user = match users.get(&user_principal) {
            Some(user) => user,
            None => {
                ic_cdk::println!("User not found in table");
                continue;
            }
        };
        total_kicked_users.push((user.clone(), balance));
    }

    ic_cdk::futures::spawn(async move {
        for (user, balance) in total_kicked_users {
            match kick_player_wrapper(
                TableId(ic_cdk::api::canister_self()),
                user.users_canister_id,
                user.principal_id,
                balance,
            )
            .await
            {
                Ok(_) => {
                    ic_cdk::println!("Kicked player: {}", user.principal_id.0.to_text());
                }
                Err(e) => {
                    ic_cdk::println!("Error kicking player: {:?}", e);
                }
            }
        }
    });

    ic_cdk::futures::spawn(async move {
        let backend_principal = match BACKEND_PRINCIPAL.lock() {
            Ok(guard) => match *guard {
                Some(principal) => principal,
                None => {
                    ic_cdk::println!("Error storing logs: Backend principal not found");
                    return;
                }
            },
            Err(_) => {
                ic_cdk::println!("Error storing logs: Failed to acquire lock");
                return;
            }
        };

        if backend_principal == Principal::from_text("zbspl-ziaaa-aaaam-qbe2q-cai").unwrap() {
            // Check if the backend principal is the prod deployment or dev deployment
            if let Err(e) = log_actions_wrapper(
                Principal::from_text("ztuys-vyaaa-aaaam-qbezq-cai").unwrap(),
                table_id,
                action_logs,
            )
            .await
            {
                ic_cdk::println!("Error storing logs: {:?}", e);
            }
        } else if backend_principal == Principal::from_text("e4yx7-lqaaa-aaaah-qdslq-cai").unwrap()
        {
            if let Err(e) = log_actions_wrapper(
                Principal::from_text("es22x-qaaaa-aaaah-qdskq-cai").unwrap(),
                table_id,
                action_logs,
            )
            .await
            {
                ic_cdk::println!("Error storing logs: {:?}", e);
            }
        } else if backend_principal == Principal::from_text("by6od-j4aaa-aaaaa-qaadq-cai").unwrap()
        {
            if let Err(e) = log_actions_wrapper(
                Principal::from_text("bw4dl-smaaa-aaaaa-qaacq-cai").unwrap(),
                table_id,
                action_logs,
            )
            .await
            {
                ic_cdk::println!("Error storing logs: {:?}", e);
            }
        }
    });

    Ok(())
}

#[ic_cdk::update]
async fn withdraw_rake(rake_amount: u64) -> Result<(), TableError> {
    handle_cycle_check().await;
    handle_table_validity_check()?;

    let table = {
        let mut table = TABLE.lock().map_err(|_| TableError::LockError)?;
        let table = table.as_mut().ok_or(TableError::TableNotFound)?;

        table.clone()
    };

    let mut rake_amount = rake_amount;

    let user_index = {
        let backend_principal = BACKEND_PRINCIPAL
            .lock()
            .map_err(|_| TableError::LockError)?
            .ok_or(TableError::CanisterCallError(
                "Backend principal not found.".to_string(),
            ))?;
        get_user_index_principal(backend_principal)
    };

    match table.config.currency_type {
        CurrencyType::Real(currency) => {
            let currency_manager = {
                let currency_manager =
                    CURRENCY_MANAGER.lock().map_err(|_| TableError::LockError)?;
                currency_manager
                    .as_ref()
                    .ok_or(TableError::StateNotInitialized)?
                    .clone()
            };

            {
                let is_shared = table.config.is_shared_rake.is_some();
                // Update rake stats
                let mut rake_stats = RAKE_STATS.lock().map_err(|_| TableError::LockError)?;
                rake_stats.add_rake(rake_amount, is_shared);
            }

            let mut house_rake = rake_amount / 2;
            rake_amount -= house_rake;
            let mut referrers: HashMap<WalletPrincipalId, User> = HashMap::new();

            // For each player at the table, check if they were referred
            for user in table.users.users.values() {
                if let Some(referrer_principal) = user.referrer {
                    // Get referrer's canister and check if referral is still active
                    let referrer_canister_id = match get_users_canister_principal_by_id_wrapper(
                        user_index,
                        referrer_principal,
                    )
                    .await
                    {
                        Ok(canister_id) => canister_id,
                        Err(_) => continue,
                    };
                    let referrer = match referrers.get(&referrer_principal) {
                        Some(referrer) => referrer.clone(),
                        None => {
                            let referrer =
                                get_user_wrapper(referrer_canister_id, referrer_principal).await?;
                            referrers.insert(referrer_principal, referrer.clone());
                            referrer
                        }
                    };

                    // Check if referral is active
                    let is_active = user.referral_start_date.unwrap_or(0) + REFERRAL_PERIOD
                        > ic_cdk::api::time();

                    if is_active {
                        // Get referrer's rake percentage based on tier
                        let rake_percentage = referrer.get_referral_rake_percentage();

                        // Calculate referrer's share
                        let player_share = house_rake / table.users.users.len() as u64;
                        let referrer_amount = player_share * rake_percentage as u64 / 100;

                        if referrer_amount > 0 && referrer_amount < house_rake {
                            // Transfer rake share to referrer
                            if let Err(e) = currency_manager
                                .withdraw(&currency, referrer_principal.0, referrer_amount)
                                .await
                            {
                                ic_cdk::println!("Error distributing referral rake: {:?}", e);
                            } else {
                                house_rake = house_rake.saturating_sub(referrer_amount);
                                if house_rake == 0 {
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            let fee = if currency == Currency::BTC {
                10
            } else {
                ic_ledger_types::DEFAULT_FEE.e8s()
            };

            if let Some((rake_share_principal, _rake_share_account_id)) =
                table.config.is_shared_rake
            {
                if let Err(e) = currency_manager
                    .withdraw_rake(&currency, *RAKE_WALLET_ADDRESS_PRINCIPAL, house_rake - fee)
                    .await
                {
                    ic_cdk::println!("Error withdrawing rake: {:?}", e);
                }
                if let Err(e) = currency_manager
                    .withdraw(&currency, rake_share_principal, rake_amount - fee)
                    .await
                {
                    ic_cdk::println!("Error withdrawing rake: {:?}", e);
                }
            } else if let Err(e) = currency_manager
                .withdraw_rake(
                    &currency,
                    *RAKE_WALLET_ADDRESS_PRINCIPAL,
                    rake_amount + house_rake - fee,
                )
                .await
            {
                ic_cdk::println!("Error withdrawing rake: {:?}", e);
            }
        }
        CurrencyType::Fake => {}
    }
    Ok(())
}

#[ic_cdk::query]
fn get_rake_stats() -> Result<RakeStats, TableError> {
    let rake_stats = RAKE_STATS.lock().map_err(|_| TableError::LockError)?;
    Ok(rake_stats.clone())
}

#[ic_cdk::query]
fn rank_cards(hand: Vec<Card>) -> Result<Rank, TableError> {
    Ok(rank_hand(hand))
}

#[ic_cdk::query]
fn get_notifications() -> Result<Vec<Notification>, TableError> {
    let table_state = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table_state = table_state.as_ref().ok_or(TableError::TableNotFound)?;

    Ok(table_state.notifications.notifications.clone())
}

#[ic_cdk::update]
async fn return_all_cycles_to_index() -> Result<(), TableError> {
    // TODO: We are losing cycles on this we need to efficiently find a way to return all cycles to the index canister
    // Get the current balance of cycles in the canister
    {
        let mut table_state = TABLE.lock().map_err(|_| TableError::LockError)?;
        let table_state = table_state.as_mut().ok_or(TableError::TableNotFound)?;
        let backend_principal = BACKEND_PRINCIPAL
            .lock()
            .map_err(|_| TableError::LockError)?
            .ok_or(TableError::CanisterCallError(
                "Backend principal not found.".to_string(),
            ))?;

        validate_caller(vec![backend_principal, table_state.id.0]);
    }
    let all_cycles = ic_cdk::api::canister_cycle_balance().saturating_sub(35_000_000_000);
    if all_cycles == 0 {
        return Err(TableError::CanisterCallError(
            "No cycles available to send".to_string(),
        ));
    }

    let _ = transfer_cycles_to_table_index(all_cycles).await;
    Ok(())
}

#[ic_cdk::update]
async fn return_cycles_to_index(cycles_amount: u128) -> Result<(), TableError> {
    {
        let mut table_state = TABLE.lock().map_err(|_| TableError::LockError)?;
        let table_state = table_state.as_mut().ok_or(TableError::TableNotFound)?;
        let backend_principal = BACKEND_PRINCIPAL
            .lock()
            .map_err(|_| TableError::LockError)?
            .ok_or(TableError::CanisterCallError(
                "Backend principal not found.".to_string(),
            ))?;

        validate_caller(vec![backend_principal, table_state.id.0]);
    }
    transfer_cycles_to_table_index(cycles_amount).await
}

async fn transfer_cycles_to_table_index(cycles_amount: u128) -> Result<(), TableError> {
    let backend_principal = BACKEND_PRINCIPAL
        .lock()
        .map_err(|_| TableError::LockError)?
        .ok_or(TableError::CanisterCallError(
            "Backend principal not found.".to_string(),
        ))?;

    // Transfer all cycles to the index canister
    let res = ic_cdk::management_canister::deposit_cycles(
        &DepositCyclesArgs {
            canister_id: backend_principal,
        },
        cycles_amount,
    )
    .await;

    match res {
        Ok(_) => Ok(()),
        Err(e) => Err(TableError::CanisterCallError(format!(
            "Failed to send cycles: {:?}",
            e
        ))),
    }
}

#[ic_cdk::update]
async fn update_blinds(
    small_blind: SmallBlind,
    big_blind: BigBlind,
    ante: AnteType,
) -> Result<(), TableError> {
    handle_cycle_check().await;

    let mut table = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table = table.as_mut().ok_or(TableError::TableNotFound)?;
    let backend_principal = BACKEND_PRINCIPAL
        .lock()
        .map_err(|_| TableError::LockError)?
        .ok_or(TableError::CanisterCallError(
            "Backend principal not found.".to_string(),
        ))?;

    validate_caller(vec![backend_principal, table.id.0]);

    let ante = if ante != AnteType::None {
        Some(ante)
    } else {
        None
    };

    // Only allow blind updates between hands
    if table.is_game_ongoing() {
        table.append_to_queue(QueueItem::UpdateBlinds(small_blind, big_blind, ante));
        return Ok(());
    }

    // Update the blinds
    table.small_blind = small_blind;
    table.big_blind = big_blind;
    table.config.ante_type = ante;

    Ok(())
}

#[ic_cdk::query]
async fn get_free_seat_index() -> Result<Option<u8>, TableError> {
    handle_cycle_check().await;

    let table = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table = table.as_ref().ok_or(TableError::TableNotFound)?;

    Ok(table.get_free_seat_index())
}

#[ic_cdk::update]
async fn clear_table() -> Result<(), TableError> {
    handle_cycle_check().await;
    {
        let mut table_state = TABLE.lock().map_err(|_| TableError::LockError)?;
        let table_state = table_state.as_mut().ok_or(TableError::TableNotFound)?;
        let backend_principal = BACKEND_PRINCIPAL
            .lock()
            .map_err(|_| TableError::LockError)?
            .ok_or(TableError::CanisterCallError(
                "Backend principal not found.".to_string(),
            ))?;

        validate_caller(vec![backend_principal, table_state.id.0]);
    }

    let users_to_remove = {
        let table = TABLE.lock().map_err(|_| TableError::LockError)?;
        let table = table.as_ref().ok_or(TableError::TableNotFound)?;

        // Collect all users and their wallet principals into a Vec
        table
            .users
            .users
            .iter()
            .map(|(principal, user)| (*principal, user.users_canister_id))
            .collect::<Vec<(WalletPrincipalId, UsersCanisterId)>>()
    };

    // Create futures for all leave_table operations
    let leave_futures =
        users_to_remove
            .into_iter()
            .map(|(user_id, users_canister_id)| async move {
                // TODO: Handle this properly
                match leave_table_wrapper(
                    TableId(ic_cdk::api::canister_self()),
                    users_canister_id,
                    user_id,
                )
                .await
                {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        ic_cdk::println!(
                            "Error removing user {}: {:?}",
                            users_canister_id.0.to_text(),
                            e
                        );
                        Err(e)
                    }
                }
            });

    // Execute all leave_table operations in parallel
    let results = futures::future::join_all(leave_futures).await;

    // Check for any errors
    for result in results {
        if let Err(e) = result {
            ic_cdk::println!("Error during clear_table: {:?}", e);
        }
    }

    Ok(())
}

#[ic_cdk::query]
fn get_seat_index(user: WalletPrincipalId) -> Result<Option<u8>, TableError> {
    let table = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table = table.as_ref().ok_or(TableError::TableNotFound)?;

    Ok(table.get_seat_index(user))
}

#[ic_cdk::update]
async fn pause_table() -> Result<(), TableError> {
    handle_cycle_check().await;

    let mut table = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table = table.as_mut().ok_or(TableError::TableNotFound)?;
    let backend_principal = BACKEND_PRINCIPAL
        .lock()
        .map_err(|_| TableError::LockError)?
        .ok_or(TableError::CanisterCallError(
            "Backend principal not found.".to_string(),
        ))?;

    validate_caller(vec![backend_principal, table.id.0]);

    table.config.is_paused = Some(true);
    Ok(())
}

#[ic_cdk::update]
async fn pause_table_for_addon(duration: u64) -> Result<(), TableError> {
    handle_cycle_check().await;

    let mut table = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table = table.as_mut().ok_or(TableError::TableNotFound)?;
    if table.config.is_paused.unwrap_or(false) {
        return Ok(());
    }
    let backend_principal = BACKEND_PRINCIPAL
        .lock()
        .map_err(|_| TableError::LockError)?
        .ok_or(TableError::CanisterCallError(
            "Backend principal not found.".to_string(),
        ))?;

    validate_caller(vec![backend_principal, table.id.0]);

    ic_cdk::println!("Pausing table");

    if table.config.table_type.is_none()
        || table.config.table_type.clone().unwrap() == TableType::Cash
    {
        return Err(TableError::InvalidRequest(
            "Table is not a tournament table".to_string(),
        ));
    }

    table.append_to_queue(QueueItem::PauseTableForAddon(duration));
    Ok(())
}

#[ic_cdk::update]
async fn resume_table() -> Result<(), TableError> {
    handle_cycle_check().await;
    ic_cdk::println!("Resuming table");
    let is_paused = {
        let mut table = TABLE.lock().map_err(|_| TableError::LockError)?;
        let table = table.as_mut().ok_or(TableError::TableNotFound)?;
        if !table.config.is_paused.unwrap_or(false) {
            return Ok(());
        }
        let backend_principal = BACKEND_PRINCIPAL
            .lock()
            .map_err(|_| TableError::LockError)?
            .ok_or(TableError::CanisterCallError(
                "Backend principal not found.".to_string(),
            ))?;

        validate_caller(vec![backend_principal, table.id.0]);

        table.config.is_paused = Some(false);
        false
    };

    if !is_paused {
        if let Err(e) = start_new_betting_round_wrapper(TableId(ic_cdk::api::canister_self())).await
        {
            ic_cdk::println!("Error resuming table: {:?}", e);
            return Ok(());
        }
    }
    Ok(())
}

#[ic_cdk::update]
async fn set_as_final_table() -> Result<(), TableError> {
    handle_cycle_check().await;
    let mut table = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table = table.as_mut().ok_or(TableError::TableNotFound)?;
    let backend_principal = BACKEND_PRINCIPAL
        .lock()
        .map_err(|_| TableError::LockError)?
        .ok_or(TableError::CanisterCallError(
            "Backend principal not found.".to_string(),
        ))?;

    validate_caller(vec![backend_principal, table.id.0]);

    if let Some(TableType::Tournament {
        tournament_id: _,
        is_final_table: _,
    }) = table.config.table_type
    {
        table.config.table_type = Some(TableType::Tournament {
            tournament_id: ic_cdk::api::msg_caller(),
            is_final_table: true,
        });
    } else {
        return Err(TableError::InvalidRequest(
            "Table is not a tournament table".to_string(),
        ));
    }
    Ok(())
}

#[ic_cdk::query]
async fn get_rake_wallet_account_id() -> String {
    RAKE_WALLET_ACCOUNT_ID.clone()
}

#[ic_cdk::query]
async fn get_rake_wallet_principal() -> Principal {
    *RAKE_WALLET_ADDRESS_PRINCIPAL
}

#[ic_cdk::update]
async fn get_players_on_table() -> Result<Vec<WalletPrincipalId>, TableError> {
    let table = TABLE.lock().map_err(|_| TableError::LockError)?;
    let table = table.as_ref().ok_or(TableError::TableNotFound)?;

    let mut players = Vec::new();
    table.seats.iter().for_each(|seat| match seat {
        SeatStatus::Occupied(user_id) => {
            if !players.contains(user_id) {
                players.push(*user_id)
            }
        }
        SeatStatus::QueuedForNextRound(user_id, _, _) => {
            if !players.contains(user_id) {
                players.push(*user_id)
            }
        }
        SeatStatus::Reserved {
            principal: user_id,
            timestamp: _,
        } => {
            if !players.contains(user_id) {
                players.push(*user_id)
            }
        }
        _ => {}
    });
    Ok(players)
}

// Chat functions

#[ic_cdk::update]
async fn send_chat_message(
    user_principal: WalletPrincipalId,
    content: String,
    message_type: ChatMessageType,
    recipient: Option<Principal>,
) -> Result<u64, ChatError> {
    handle_cycle_check().await;

    // Validate the user is in the table
    let table_state = TABLE
        .lock()
        .map_err(|_| ChatError::LockError("Failed to acquire table lock".to_string()))?;
    let table_state = table_state
        .as_ref()
        .ok_or(ChatError::InternalError("Table not found".to_string()))?;

    let user = table_state
        .users
        .get(&user_principal)
        .ok_or(ChatError::UserNotInTable(user_principal.0))?;

    // Validate caller
    let backend_principal = BACKEND_PRINCIPAL
        .lock()
        .map_err(|_| ChatError::LockError("Failed to acquire backend principal lock".to_string()))?
        .ok_or(ChatError::InternalError(
            "Backend principal not found".to_string(),
        ))?;

    validate_caller(vec![
        user_principal.0,
        user.principal_id.0,
        backend_principal,
    ]);

    // Check message length
    if content.len() > 2000 {
        return Err(ChatError::MessageTooLong {
            current_size: content.len(),
            max_size: 2000,
        });
    }

    // Get user display name
    let sender_name = user.user_name.clone();

    // Add the message to chat history
    let mut chat_history = CHAT_HISTORY
        .lock()
        .map_err(|_| ChatError::LockError("Failed to acquire chat history lock".to_string()))?;
    let msg_id = chat_history.add_message(
        user_principal.0,
        sender_name,
        content,
        message_type,
        recipient,
    );

    Ok(msg_id)
}

#[ic_cdk::update]
async fn edit_chat_message(
    user_principal: WalletPrincipalId,
    message_id: u64,
    new_content: String,
) -> Result<(), ChatError> {
    handle_cycle_check().await;

    // Validate the user is in the table
    let table_state = TABLE
        .lock()
        .map_err(|_| ChatError::LockError("Failed to acquire table lock".to_string()))?;
    let table_state = table_state
        .as_ref()
        .ok_or(ChatError::InternalError("Table not found".to_string()))?;

    let user = table_state
        .users
        .get(&user_principal)
        .ok_or(ChatError::UserNotInTable(user_principal.0))?;

    // Validate caller
    let backend_principal = BACKEND_PRINCIPAL
        .lock()
        .map_err(|_| ChatError::LockError("Failed to acquire backend principal lock".to_string()))?
        .ok_or(ChatError::InternalError(
            "Backend principal not found".to_string(),
        ))?;

    validate_caller(vec![
        user_principal.0,
        user.principal_id.0,
        backend_principal,
    ]);

    // Check message length
    if new_content.len() > 2000 {
        return Err(ChatError::MessageTooLong {
            current_size: new_content.len(),
            max_size: 2000,
        });
    }

    // Edit the message
    let mut chat_history = CHAT_HISTORY
        .lock()
        .map_err(|_| ChatError::LockError("Failed to acquire chat history lock".to_string()))?;
    chat_history.edit_message(message_id, new_content, user_principal.0)?;

    Ok(())
}

#[ic_cdk::query]
fn get_recent_chat_messages(
    from_message_id: Option<u64>,
    page_size: usize,
) -> Result<Vec<ChatMessage>, ChatError> {
    let chat_history = CHAT_HISTORY
        .lock()
        .map_err(|_| ChatError::LockError("Failed to acquire chat history lock".to_string()))?;
    Ok(chat_history.get_messages(from_message_id, page_size))
}

#[ic_cdk::query]
fn get_chat_messages_for_user(
    user_principal: WalletPrincipalId,
) -> Result<Vec<ChatMessage>, ChatError> {
    let chat_history = CHAT_HISTORY
        .lock()
        .map_err(|_| ChatError::LockError("Failed to acquire chat history lock".to_string()))?;
    Ok(chat_history.get_messages_for_user(user_principal.0))
}

#[ic_cdk::update]
async fn clear_chat_history() -> Result<(), ChatError> {
    handle_cycle_check().await;

    // Only allow the backend to clear chat history
    let backend_principal = BACKEND_PRINCIPAL
        .lock()
        .map_err(|_| ChatError::LockError("Failed to acquire backend principal lock".to_string()))?
        .ok_or(ChatError::InternalError(
            "Backend principal not found".to_string(),
        ))?;

    validate_caller(vec![backend_principal]);

    let mut chat_history = CHAT_HISTORY
        .lock()
        .map_err(|_| ChatError::LockError("Failed to acquire chat history lock".to_string()))?;
    chat_history.clear();

    Ok(())
}

#[ic_cdk::update]
async fn get_canister_status_formatted() -> Result<String, TableError> {
    // Validate caller is a controller
    let controllers = (*CONTROLLER_PRINCIPALS).clone();
    validate_caller(controllers);

    handle_cycle_check().await;

    // Call the management canister to get status
    let canister_status_arg = CanisterStatusArgs {
        canister_id: ic_cdk::api::canister_self(),
    };

    let status_response = canister_status(&canister_status_arg).await.map_err(|e| {
        TableError::CanisterCallError(format!("Failed to get canister status: {:?}", e))
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

// ============================================================================
// RNG Transparency Helper Functions
// ============================================================================

/// Calculate SHA-256 hash of the deck for cryptographic verification
///
/// Uses SHA-256 for cryptographic security and transparency.
/// This ensures hashes can be independently verified by users.
fn calculate_deck_hash(deck: &FlatDeck) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();

    // Hash each card in the deck deterministically
    for card in deck.cards() {
        // Serialize card as value:suit bytes
        let card_bytes = format!("{:?}:{:?}", card.value, card.suit);
        hasher.update(card_bytes.as_bytes());
    }

    // Convert to hex string (64 chars for SHA-256)
    let result = hasher.finalize();
    format!("{:x}", result)
}

/// Generate card provenance for all cards in the deck
///
/// Uses SHA-256 for each card hash to ensure cryptographic verification.
/// Hash format: SHA-256(round_id || card || shuffled_position)
fn generate_card_provenance(deck: &FlatDeck, round_id: u64) -> HashMap<String, CardProvenance> {
    use sha2::{Digest, Sha256};

    let mut provenance_map = HashMap::new();

    for (shuffled_position, card) in deck.cards().iter().enumerate() {
        // Calculate unique SHA-256 hash for this card
        let mut hasher = Sha256::new();

        // Hash round_id (8 bytes, little-endian)
        hasher.update(round_id.to_le_bytes());

        // Hash card (deterministic serialization)
        let card_bytes = format!("{:?}:{:?}", card.value, card.suit);
        hasher.update(card_bytes.as_bytes());

        // Hash position (1 byte)
        hasher.update(&[shuffled_position as u8]);

        // Finalize to hex string
        let result = hasher.finalize();
        let card_hash = format!("{:x}", result);

        // Create provenance record
        let provenance = CardProvenance {
            round_id,
            card: *card,
            original_position: shuffled_position as u8,
            shuffled_position: shuffled_position as u8,
            card_hash: card_hash.clone(),
            dealt_to: None,
            dealt_at_stage: None,
        };

        provenance_map.insert(card_hash, provenance);
    }

    provenance_map
}

ic_cdk::export_candid!();
