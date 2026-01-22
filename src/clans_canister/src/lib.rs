use std::collections::HashMap;

use authentication::validate_caller;
use candid::{CandidType, Nat, Principal};
use canister_functions::{create_canister_wrapper, install_wasm_code};
use clan::{
    environment::ClanEnvironmentSettings,
    member::{ClanMember, MemberStatus},
    subscriptions::{
        ClanRole, SubscriptionBenefits, SubscriptionRequirements, SubscriptionTier,
        SubscriptionTierId,
    },
    tags::ClanTag,
    treasury::ClanTreasury,
    Clan, ClanEvent, ClanId, ClanInvitation, ClanPrivacy, ClanStats, CreateClanRequest,
    JoinRequest,
};
use currency::{state::TransactionState, types::currency_manager::CurrencyManager, Currency};
use errors::clan_error::ClanError;
use ic_cdk::management_canister::{canister_status, CanisterStatusArgs, DepositCyclesArgs};
use intercanister_call_wrappers::{tournament_canister::create_tournament_wrapper, users_canister::get_user_wrapper};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use table::{poker::game::{table_functions::{table::{TableConfig, TableId}, types::CurrencyType}, types::PublicTable}, table_canister::create_table_wrapper};
use tournaments::tournaments::types::{NewTournament, TournamentData, TournamentId};
use user::user::{UsersCanisterId, WalletPrincipalId};
use utils::handle_cycle_check;

mod memory;
mod utils;

#[derive(Debug, Clone, PartialEq, CandidType, Serialize, Deserialize)]
pub struct ClanEvents {
    pub events: Vec<ClanEvent>,
}

impl ClanEvents {
    pub fn new() -> Self {
        ClanEvents { events: Vec::new() }
    }

    pub fn add_event(&mut self, event: ClanEvent) {
        self.events.push(event);
    }

    pub fn get_events(&self, limit: Option<usize>) -> Vec<ClanEvent> {
        let limit = limit.unwrap_or(100);
        if self.events.len() <= limit {
            self.events.clone()
        } else {
            self.events[self.events.len() - limit..].to_vec()
        }
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }
}

// Define global instances wrapped in Mutex for safe concurrent access
lazy_static! {
    static ref CLAN: Mutex<Option<Clan>> = Mutex::new(None);
    static ref BACKEND_PRINCIPAL: Mutex<Option<Principal>> = Mutex::new(None);
    static ref TRANSACTION_STATE: Mutex<TransactionState> = Mutex::new(TransactionState::new());
    static ref CURRENCY_MANAGER: Mutex<CurrencyManager> = Mutex::new(CurrencyManager::new());
    static ref CLAN_EVENTS: Mutex<ClanEvents> = Mutex::new(ClanEvents::new());
    static ref CONTROLLER_PRINCIPALS: Vec<Principal> = vec![
        Principal::from_text("py2cj-ei3dt-3ber7-nvxdl-56xvh-qkhop-7x7fz-nph7j-7cuya-3gyxr-cqe")
            .unwrap(),
        Principal::from_text("km7qz-4bai4-e5ptx-hgrck-z3web-ameqg-ksxcf-u7wbr-t5fna-i7bqp-hqe")
            .unwrap(),
        Principal::from_text("uyxh5-bi3za-gxbfs-op3gj-ere73-a6jhv-5jky3-zawef-b5r2s-k26un-sae")
            .unwrap(),
        Principal::from_text("w3kjy-pitqg-dvab7-tb57q-63gnd-di4vo-loiiy-s6zm2-gqcmw-ixliz-aae").unwrap(),
        Principal::from_text("tcuxo-b5b4t-vxwo7-mwgxi-vb4ig-zuux4-jmvru-fjocv-4uxuz-7yo4v-hqe").unwrap(),
        Principal::from_text("2hbym-ivof6-l2yyh-zgu62-fqgf3-nvtlj-edso2-ebahf-p5j5c-2p2eb-lae").unwrap(),
    ];

    static ref TABLE_CANISTER_WASM: &'static [u8] =
        include_bytes!("../../../target/wasm32-unknown-unknown/release/table_canister.wasm");
    static ref TOURNAMENT_CANISTER_WASM: &'static [u8] =
        include_bytes!("../../../target/wasm32-unknown-unknown/release/tournament_canister.wasm");
}

use std::sync::Mutex;

#[ic_cdk::init]
fn init() {
    let principal = ic_cdk::api::canister_self();
    ic_cdk::println!("Clan canister {} initialized", principal);
}

#[ic_cdk::update]
async fn create_clan(
    request: CreateClanRequest,
    creator: WalletPrincipalId,
    creator_canister_id: UsersCanisterId,
) -> Result<Clan, ClanError> {
    let mut backend_principal = BACKEND_PRINCIPAL.lock().map_err(|_| ClanError::LockError)?;

    if let Some(backend_principal) = *backend_principal {
        validate_caller(vec![backend_principal]);
    }
    *backend_principal = Some(ic_cdk::api::msg_caller());

    let creator_user = get_user_wrapper(creator_canister_id, creator).await?;

    if !creator_user.can_play() {
        return Err(ClanError::UserBanned);
    }

    // Create the clan
    let clan_id = ClanId(ic_cdk::api::canister_self());
    let mut clan = Clan::new(
        clan_id,
        request.name,
        request.description,
        vec![], // Will be set below
        creator,
        request.privacy,
        request.supported_currency,
        request.joining_fee,
    )?;

    // Set optional fields
    clan.avatar = request.avatar;
    clan.website = request.website;
    clan.discord = request.discord;
    clan.twitter = request.twitter;
    clan.require_proof_of_humanity = request.require_proof_of_humanity;

    if let Some(limit) = request.member_limit {
        clan.member_limit = limit;
    }

    // Initialize currency manager for real currency
    {
        let mut currency_manager = CURRENCY_MANAGER.lock().map_err(|_| ClanError::LockError)?;
        currency_manager
            .add_currency(clan.supported_currency.clone())
            .await?;
    }

    // Store the clan
    *CLAN.lock().map_err(|_| ClanError::LockError)? = Some(clan.clone());

    // Log creation event
    let event = ClanEvent::SettingsUpdated {
        by: creator,
        timestamp: ic_cdk::api::time(),
    };
    CLAN_EVENTS
        .lock()
        .map_err(|_| ClanError::LockError)?
        .add_event(event);

    Ok(clan)
}

#[ic_cdk::query]
fn ping() -> String {
    "Ok".to_string()
}

#[ic_cdk::query]
fn get_clan() -> Result<Clan, ClanError> {
    let clan = CLAN.lock().map_err(|_| ClanError::LockError)?;
    let clan = clan.as_ref().ok_or(ClanError::ClanNotFound)?;
    Ok(clan.clone())
}

#[ic_cdk::query]
fn get_clan_member(member_id: WalletPrincipalId) -> Result<ClanMember, ClanError> {
    let clan = CLAN.lock().map_err(|_| ClanError::LockError)?;
    let clan = clan.as_ref().ok_or(ClanError::ClanNotFound)?;

    clan.members
        .get(&member_id)
        .cloned()
        .ok_or(ClanError::MemberNotFound)
}

#[ic_cdk::query]
fn get_clan_members() -> Result<Vec<ClanMember>, ClanError> {
    let clan = CLAN.lock().map_err(|_| ClanError::LockError)?;
    let clan = clan.as_ref().ok_or(ClanError::ClanNotFound)?;
    Ok(clan.members.values().cloned().collect())
}

#[ic_cdk::query]
fn get_clan_events(limit: Option<usize>) -> Result<Vec<ClanEvent>, ClanError> {
    let events = CLAN_EVENTS.lock().map_err(|_| ClanError::LockError)?;
    let limit = limit.unwrap_or(100);

    if events.len() <= limit {
        Ok(events.events.clone())
    } else {
        Ok(events.events[events.len() - limit..].to_vec())
    }
}

#[ic_cdk::update]
async fn join_clan(
    users_canister_principal: UsersCanisterId,
    user_id: WalletPrincipalId,
    joining_fee_paid: u64,
) -> Result<(), ClanError> {
    handle_cycle_check();

    let clan = {
        let clan = CLAN.lock().map_err(|_| ClanError::LockError)?;
        clan.as_ref().ok_or(ClanError::ClanNotFound)?.clone()
    };

    // Get user and validate
    let user = get_user_wrapper(users_canister_principal, user_id).await?;
    if !user.can_play() {
        return Err(ClanError::UserBanned);
    }

    // Check if user meets requirements
    clan.meets_joining_requirements(&user)?;

    // Check if already a member
    if clan.is_member(&user_id) {
        return Err(ClanError::UserAlreadyMember);
    }

    // Validate joining fee for real currency
    match clan.supported_currency {
        Currency::ICP | Currency::BTC => {
            let currency_manager = {
                let currency_manager = CURRENCY_MANAGER.lock().map_err(|_| ClanError::LockError)?;
                currency_manager.clone()
            };

            currency_manager
                .validate_allowance(&clan.supported_currency, user_id.0, joining_fee_paid)
                .await?;

            // Process payment if required
            if clan.joining_fee > 0 {
                let mut transaction_state = TRANSACTION_STATE
                    .lock()
                    .map_err(|_| ClanError::LockError)?
                    .clone();

                currency_manager
                    .deposit(
                        &mut transaction_state,
                        &clan.supported_currency,
                        user_id.0,
                        joining_fee_paid,
                    )
                    .await?;

                *TRANSACTION_STATE.lock().map_err(|_| ClanError::LockError)? = transaction_state;
            }
        }
        _ => {} // No payment validation for fake currencies
    }

    // Add member to clan
    {
        let mut clan_state = CLAN.lock().map_err(|_| ClanError::LockError)?;
        let clan_state = clan_state.as_mut().ok_or(ClanError::ClanNotFound)?;

        if clan.joining_fee > 0 {
            clan_state.join_with_fee(user_id, joining_fee_paid)?;
        } else {
            clan_state.add_member(user_id, ClanRole::Member)?;
        }
    }

    // Log event
    let event = ClanEvent::MemberJoined {
        member: user_id,
        joining_fee_paid,
        timestamp: ic_cdk::api::time(),
    };
    CLAN_EVENTS
        .lock()
        .map_err(|_| ClanError::LockError)?
        .add_event(event);

    Ok(())
}

#[ic_cdk::update]
async fn leave_clan(user_id: WalletPrincipalId) -> Result<(), ClanError> {
    handle_cycle_check();

    let removed_member = {
        let mut clan_state = CLAN.lock().map_err(|_| ClanError::LockError)?;
        let clan_state = clan_state.as_mut().ok_or(ClanError::ClanNotFound)?;

        // Validate caller
        let backend_principal = BACKEND_PRINCIPAL
            .lock()
            .map_err(|_| ClanError::LockError)?
            .ok_or(ClanError::CanisterCallError(
                "Backend principal not found".to_string(),
            ))?;

        validate_caller(vec![user_id.0, backend_principal]);

        clan_state.remove_member(&user_id)?
    };

    // Refund any subscription balance if applicable
    if removed_member.subscription_expires_at.is_some() && removed_member.is_subscription_active() {
        // In a real implementation, calculate and refund unused subscription time
        ic_cdk::println!("Member {} left with active subscription", user_id.0);
    }

    // Log event
    let event = ClanEvent::MemberLeft {
        member: user_id,
        timestamp: ic_cdk::api::time(),
    };
    CLAN_EVENTS
        .lock()
        .map_err(|_| ClanError::LockError)?
        .add_event(event);

    Ok(())
}

#[ic_cdk::update]
async fn kick_member(
    user_id: WalletPrincipalId,
    kicked_by: WalletPrincipalId,
) -> Result<(), ClanError> {
    handle_cycle_check();

    let mut clan_state = CLAN.lock().map_err(|_| ClanError::LockError)?;
    let clan_state = clan_state.as_mut().ok_or(ClanError::ClanNotFound)?;

    // Validate caller permissions
    let kicker = clan_state
        .members
        .get(&kicked_by)
        .ok_or(ClanError::MemberNotFound)?;

    if !kicker.can_moderate() {
        return Err(ClanError::InsufficientPermissions);
    }

    // Cannot kick owner or same/higher role
    let target = clan_state
        .members
        .get(&user_id)
        .ok_or(ClanError::MemberNotFound)?;

    if target.role == ClanRole::Owner {
        return Err(ClanError::CannotRemoveOwner);
    }

    clan_state.remove_member(&user_id)?;

    // Log event
    let event = ClanEvent::MemberLeft {
        member: user_id,
        timestamp: ic_cdk::api::time(),
    };
    CLAN_EVENTS
        .lock()
        .map_err(|_| ClanError::LockError)?
        .add_event(event);

    Ok(())
}

#[ic_cdk::update]
async fn update_member_role(
    user_id: WalletPrincipalId,
    new_role: ClanRole,
    updated_by: WalletPrincipalId,
) -> Result<(), ClanError> {
    handle_cycle_check();

    {
        let mut clan_state = CLAN.lock().map_err(|_| ClanError::LockError)?;
        let clan_state = clan_state.as_mut().ok_or(ClanError::ClanNotFound)?;

        // Validate caller
        let backend_principal = BACKEND_PRINCIPAL
            .lock()
            .map_err(|_| ClanError::LockError)?
            .ok_or(ClanError::CanisterCallError(
                "Backend principal not found".to_string(),
            ))?;

        validate_caller(vec![updated_by.0, backend_principal]);

        clan_state.update_member_role(&user_id, new_role.clone(), &updated_by)?;
    }

    // Log event
    let event = ClanEvent::MemberPromoted {
        member: user_id,
        new_role,
        by: updated_by,
        timestamp: ic_cdk::api::time(),
    };
    CLAN_EVENTS
        .lock()
        .map_err(|_| ClanError::LockError)?
        .add_event(event);

    Ok(())
}

#[ic_cdk::update]
async fn suspend_member(
    user_id: WalletPrincipalId,
    suspended_by: WalletPrincipalId,
    until: Option<u64>,
) -> Result<(), ClanError> {
    handle_cycle_check();

    {
        let mut clan_state = CLAN.lock().map_err(|_| ClanError::LockError)?;
        let clan_state = clan_state.as_mut().ok_or(ClanError::ClanNotFound)?;

        // Validate permissions
        let suspender = clan_state
            .members
            .get(&suspended_by)
            .ok_or(ClanError::MemberNotFound)?;

        if !suspender.can_moderate() {
            return Err(ClanError::InsufficientPermissions);
        }

        // Update member status
        if let Some(member) = clan_state.members.get_mut(&user_id) {
            member.status = MemberStatus::Suspended { until };
        } else {
            return Err(ClanError::MemberNotFound);
        }
    }

    // Log event
    let event = ClanEvent::MemberSuspended {
        member: user_id,
        by: suspended_by,
        until,
        timestamp: ic_cdk::api::time(),
    };
    CLAN_EVENTS
        .lock()
        .map_err(|_| ClanError::LockError)?
        .add_event(event);

    Ok(())
}

#[ic_cdk::update]
async fn upgrade_subscription(
    user_id: WalletPrincipalId,
    new_tier: SubscriptionTierId,
    paid_amount: u64,
    months: u32,
) -> Result<(), ClanError> {
    handle_cycle_check();

    let clan_currency = {
        let clan = CLAN.lock().map_err(|_| ClanError::LockError)?;
        let clan = clan.as_ref().ok_or(ClanError::ClanNotFound)?;
        clan.supported_currency.clone()
    };

    // Process payment for real currencies
    match clan_currency {
        Currency::ICP | Currency::BTC => {
            let currency_manager = {
                let currency_manager = CURRENCY_MANAGER.lock().map_err(|_| ClanError::LockError)?;
                currency_manager.clone()
            };

            let mut transaction_state = TRANSACTION_STATE
                .lock()
                .map_err(|_| ClanError::LockError)?
                .clone();

            currency_manager
                .deposit(
                    &mut transaction_state,
                    &clan_currency,
                    user_id.0,
                    paid_amount,
                )
                .await?;

            *TRANSACTION_STATE.lock().map_err(|_| ClanError::LockError)? = transaction_state;
        }
        _ => {} // No payment processing for fake currencies
    }

    // Upgrade subscription
    {
        let mut clan_state = CLAN.lock().map_err(|_| ClanError::LockError)?;
        let clan_state = clan_state.as_mut().ok_or(ClanError::ClanNotFound)?;

        clan_state.upgrade_subscription(&user_id, &new_tier, paid_amount, months)?;
    }

    Ok(())
}

#[ic_cdk::update]
async fn update_clan_settings(
    settings: ClanUpdateRequest,
    updated_by: WalletPrincipalId,
) -> Result<(), ClanError> {
    handle_cycle_check();

    {
        let mut clan_state = CLAN.lock().map_err(|_| ClanError::LockError)?;
        let clan_state = clan_state.as_mut().ok_or(ClanError::ClanNotFound)?;

        // Validate permissions
        let updater = clan_state
            .members
            .get(&updated_by)
            .ok_or(ClanError::MemberNotFound)?;

        if !updater.is_admin_or_higher() {
            return Err(ClanError::InsufficientPermissions);
        }

        // Update settings
        if let Some(name) = settings.name {
            if name.is_empty() || name.len() > 50 {
                return Err(ClanError::InvalidClanName);
            }
            clan_state.name = name;
        }

        if let Some(description) = settings.description {
            if description.len() > 500 {
                return Err(ClanError::InvalidDescription);
            }
            clan_state.description = description;
        }

        if let Some(privacy) = settings.privacy {
            clan_state.privacy = privacy;
        }

        if let Some(joining_fee) = settings.joining_fee {
            clan_state.update_joining_fee(joining_fee, &updated_by)?;
        }

        if let Some(require_poh) = settings.require_proof_of_humanity {
            clan_state.require_proof_of_humanity = require_poh;
        }

        if let Some(member_limit) = settings.member_limit {
            clan_state.member_limit = member_limit;
        }

        if let Some(avatar) = settings.avatar {
            clan_state.avatar = Some(avatar);
        }

        if let Some(website) = settings.website {
            clan_state.website = Some(website);
        }

        if let Some(discord) = settings.discord {
            clan_state.discord = Some(discord);
        }

        if let Some(twitter) = settings.twitter {
            clan_state.twitter = Some(twitter);
        }

        if let Some(tags) = settings.tags {
            clan_state.tags = tags.into_iter().collect();
        }

        if let Some(env_settings) = settings.environment_settings {
            clan_state.environment_settings = env_settings;
        }
    }

    // Log event
    let event = ClanEvent::SettingsUpdated {
        by: updated_by,
        timestamp: ic_cdk::api::time(),
    };
    CLAN_EVENTS
        .lock()
        .map_err(|_| ClanError::LockError)?
        .add_event(event);

    Ok(())
}

#[ic_cdk::update]
async fn create_clan_table(
    config: TableConfig,
    created_by: WalletPrincipalId,
) -> Result<PublicTable, ClanError> {
    handle_cycle_check();

    let clan = {
        let clan = CLAN.lock().map_err(|_| ClanError::LockError)?;
        clan.as_ref().ok_or(ClanError::ClanNotFound)?.clone()
    };

    // Check if member has permission to create tables
    if !clan.has_tier_access(&created_by, "can_create_tables")? {
        return Err(ClanError::InsufficientPermissions);
    }

    // Validate currency matches clan's supported currency
    match config.currency_type {
        CurrencyType::Real(currency) => {
            if currency != clan.supported_currency {
                return Err(ClanError::InvalidCurrency);
            }
        }
        CurrencyType::Fake => {
            // Allow fake currency for practice/fun games
        }
    }

    // Create the table canister
    let controllers = CONTROLLER_PRINCIPALS.clone();
    let wasm_module = TABLE_CANISTER_WASM.to_vec();
    let table_canister_principal = create_canister_wrapper(controllers, None).await
        .map_err(|e| ClanError::CanisterCallError(format!("Failed to create table canister: {:?}", e)))?;
    
    install_wasm_code(table_canister_principal, wasm_module).await
        .map_err(|e| ClanError::CanisterCallError(format!("Failed to install table WASM: {:?}", e)))?;

    let table_id = TableId(table_canister_principal);

    // Generate random bytes for table creation
    let raw_bytes = ic_cdk::management_canister::raw_rand().await
        .map_err(|e| ClanError::CanisterCallError(format!("Failed to generate random bytes: {:?}", e)))?;

    // Create the table with clan-specific configuration
    let mut clan_config = config.clone();
    clan_config.is_private = Some(true); // Clan tables are private by default
    
    let mut config = config;

    // Set table name prefix if configured
    if let Some(prefix) = &clan.environment_settings.table_name_prefix {
        config.name = format!("{} - {}", prefix, config.name);
    }

    let table = create_table_wrapper(table_id, clan_config, raw_bytes).await
        .map_err(|e| ClanError::CanisterCallError(format!("Failed to create table: {:?}", e)))?;

    // Add table to clan's active tables
    {
        let mut clan_state = CLAN.lock().map_err(|_| ClanError::LockError)?;
        let clan_state = clan_state.as_mut().ok_or(ClanError::ClanNotFound)?;
        clan_state.active_tables.push(table_id);
    }

    // Log event
    let event = ClanEvent::SettingsUpdated {
        by: created_by,
        timestamp: ic_cdk::api::time(),
    };
    CLAN_EVENTS
        .lock()
        .map_err(|_| ClanError::LockError)?
        .add_event(event);

    Ok(table)
}

/// Create a tournament for the clan
#[ic_cdk::update]
async fn create_clan_tournament(
    new_tournament: NewTournament,
    table_config: TableConfig,
    created_by: WalletPrincipalId,
) -> Result<TournamentData, ClanError> {
    handle_cycle_check();

    let clan = {
        let clan = CLAN.lock().map_err(|_| ClanError::LockError)?;
        clan.as_ref().ok_or(ClanError::ClanNotFound)?.clone()
    };

    // Check if member has permission to create tournaments
    if !clan.has_tier_access(&created_by, "can_create_tournaments")? {
        return Err(ClanError::InsufficientPermissions);
    }

    // Validate currency matches clan's supported currency
    match table_config.currency_type {
        CurrencyType::Real(currency) => {
            if currency != clan.supported_currency {
                return Err(ClanError::InvalidCurrency);
            }
        }
        CurrencyType::Fake => {
            // Allow fake currency for practice tournaments
        }
    }

    // Validate tournament start time
    if new_tournament.start_time <= ic_cdk::api::time() {
        return Err(ClanError::InvalidRequest(
            "Tournament start time must be in the future".to_string(),
        ));
    }

    // Create the tournament canister
    let controllers = CONTROLLER_PRINCIPALS.clone();
    let wasm_module = TOURNAMENT_CANISTER_WASM.to_vec();
    let cycle_amount = 3_000_000_000_000; // 3T cycles for tournament
    
    let tournament_canister_principal = create_canister_wrapper(controllers, Some(cycle_amount)).await
        .map_err(|e| ClanError::CanisterCallError(format!("Failed to create tournament canister: {:?}", e)))?;
    
    install_wasm_code(tournament_canister_principal, wasm_module).await
        .map_err(|e| ClanError::CanisterCallError(format!("Failed to install tournament WASM: {:?}", e)))?;

    let tournament_id = TournamentId(tournament_canister_principal);

    // Create tournament data
    let tournament_data = TournamentData::new(tournament_id, new_tournament, table_config.clone())
        .map_err(|e| ClanError::InvalidRequest(format!("Invalid tournament configuration: {:?}", e)))?;

    // Create the tournament
    let tournament = create_tournament_wrapper(tournament_id, tournament_data.clone(), table_config, 0).await
        .map_err(|e| ClanError::CanisterCallError(format!("Failed to create tournament: {:?}", e)))?;

    // Add tournament to clan's hosted tournaments
    {
        let mut clan_state = CLAN.lock().map_err(|_| ClanError::LockError)?;
        let clan_state = clan_state.as_mut().ok_or(ClanError::ClanNotFound)?;
        clan_state.hosted_tournaments.push(tournament_id);
        clan_state.stats.total_tournaments_hosted += 1;
    }

    // Log event
    let event = ClanEvent::TournamentHosted {
        tournament_id: created_by, // Using created_by as placeholder - you might want to change this
        timestamp: ic_cdk::api::time(),
    };
    CLAN_EVENTS
        .lock()
        .map_err(|_| ClanError::LockError)?
        .add_event(event);

    Ok(tournament)
}

/// Get all active tables for the clan
#[ic_cdk::query]
fn get_clan_tables() -> Result<Vec<TableId>, ClanError> {
    let clan = CLAN.lock().map_err(|_| ClanError::LockError)?;
    let clan = clan.as_ref().ok_or(ClanError::ClanNotFound)?;
    Ok(clan.active_tables.clone())
}

/// Get all tournaments hosted by the clan
#[ic_cdk::query]
fn get_clan_tournaments() -> Result<Vec<TournamentId>, ClanError> {
    let clan = CLAN.lock().map_err(|_| ClanError::LockError)?;
    let clan = clan.as_ref().ok_or(ClanError::ClanNotFound)?;
    Ok(clan.hosted_tournaments.clone())
}

/// Remove a table from the clan (admin+ only)
#[ic_cdk::update]
async fn remove_clan_table(
    table_id: TableId,
    removed_by: WalletPrincipalId,
) -> Result<(), ClanError> {
    handle_cycle_check();

    {
        let mut clan_state = CLAN.lock().map_err(|_| ClanError::LockError)?;
        let clan_state = clan_state.as_mut().ok_or(ClanError::ClanNotFound)?;

        // Validate permissions
        let remover = clan_state
            .members
            .get(&removed_by)
            .ok_or(ClanError::MemberNotFound)?;

        if !remover.is_admin_or_higher() {
            return Err(ClanError::InsufficientPermissions);
        }

        // Remove from active tables
        clan_state.active_tables.retain(|&id| id != table_id);
    }

    Ok(())
}

/// Remove a tournament from the clan (admin+ only)
#[ic_cdk::update]
async fn remove_clan_tournament(
    tournament_id: TournamentId,
    removed_by: WalletPrincipalId,
) -> Result<(), ClanError> {
    handle_cycle_check();

    {
        let mut clan_state = CLAN.lock().map_err(|_| ClanError::LockError)?;
        let clan_state = clan_state.as_mut().ok_or(ClanError::ClanNotFound)?;

        // Validate permissions
        let remover = clan_state
            .members
            .get(&removed_by)
            .ok_or(ClanError::MemberNotFound)?;

        if !remover.is_admin_or_higher() {
            return Err(ClanError::InsufficientPermissions);
        }

        // Remove from hosted tournaments
        clan_state.hosted_tournaments.retain(|&id| id != tournament_id);
    }

    // TODO: Optionally stop and delete the tournament canister here

    Ok(())
}

/// Check if a member can access a specific table based on their subscription tier
#[ic_cdk::query]
fn can_member_access_table(
    member_id: WalletPrincipalId,
    table_stakes: u64,
) -> Result<bool, ClanError> {
    let clan = CLAN.lock().map_err(|_| ClanError::LockError)?;
    let clan = clan.as_ref().ok_or(ClanError::ClanNotFound)?;

    ic_cdk::println!(
        "Checking access for member {:?} with table stakes {}",
        member_id, table_stakes
    );
    // Check if member exists
    if !clan.is_member(&member_id) {
        return Ok(false);
    }

    ic_cdk::println!("Member {:?} exists in the clan", member_id);

    // Check table stakes access based on subscription tier
    clan.can_access_table_stakes(&member_id, table_stakes)
}

/// Check if a member can participate in tournaments
#[ic_cdk::query]
fn can_member_access_benefits(member_id: WalletPrincipalId, benefits: String) -> Result<bool, ClanError> {
    let clan = CLAN.lock().map_err(|_| ClanError::LockError)?;
    let clan = clan.as_ref().ok_or(ClanError::ClanNotFound)?;

    // Check if member exists
    if !clan.is_member(&member_id) {
        return Ok(false);
    }

    // Check tournament access based on subscription tier
    clan.has_tier_access(&member_id, &benefits)
}

/// Update clan table/tournament statistics when games complete
#[ic_cdk::update]
async fn update_clan_game_stats(
    table_id: Option<TableId>,
    tournament_id: Option<TournamentId>,
    revenue_generated: u64,
) -> Result<(), ClanError> {
    handle_cycle_check();
    let mut clan_state = CLAN.lock().map_err(|_| ClanError::LockError)?;
    let clan_state = clan_state.as_mut().ok_or(ClanError::ClanNotFound)?;
    
    let mut valid_callers = CONTROLLER_PRINCIPALS.clone();
    let table_ids = clan_state.active_tables.iter().map(|id| id.0).collect::<Vec<_>>();
    let tournament_ids = clan_state.hosted_tournaments.iter().map(|id| id.0).collect::<Vec<_>>();
    valid_callers.extend_from_slice(&table_ids);
    valid_callers.extend_from_slice(&tournament_ids);
    validate_caller(valid_callers);        

    if table_id.is_some() {
        clan_state.stats.total_games_played += 1;
    }
    if tournament_id.is_some() {
        clan_state.stats.total_tournaments_hosted += 1;
    }

    clan_state.treasury.balance += revenue_generated;
    clan_state.treasury.total_revenue_generated += revenue_generated;
    clan_state.stats.total_revenue_generated += revenue_generated;
    clan_state.stats.last_activity = ic_cdk::api::time();

    // Log revenue event
    if revenue_generated > 0 {
        let event = ClanEvent::RevenueGenerated {
            amount: revenue_generated,
            timestamp: ic_cdk::api::time(),
        };
        CLAN_EVENTS
            .lock()
            .map_err(|_| ClanError::LockError)?
            .add_event(event);
    }

    Ok(())
}

#[ic_cdk::update]
async fn update_subscription_tier(
    tier: SubscriptionTier,
    created_by: WalletPrincipalId,
) -> Result<(), ClanError> {
    handle_cycle_check();

    {
        let mut clan_state = CLAN.lock().map_err(|_| ClanError::LockError)?;
        let clan_state = clan_state.as_mut().ok_or(ClanError::ClanNotFound)?;

        clan_state.update_subscription_tier(tier, &created_by)?;
    }

    Ok(())
}

#[ic_cdk::update]
async fn create_custom_subscription_tier(
    id: SubscriptionTierId,
    name: String,
    requirements: SubscriptionRequirements,
    benefits: SubscriptionBenefits,
    is_active: bool,
    tier_order: u32,
    creator: WalletPrincipalId,
) -> Result<SubscriptionTier, ClanError> {
    handle_cycle_check();

    let mut clan_state = CLAN.lock().map_err(|_| ClanError::LockError)?;
    let clan_state = clan_state.as_mut().ok_or(ClanError::ClanNotFound)?;

    clan_state.create_custom_subscription_tier(
        id,
        name,
        requirements,
        benefits,
        is_active,
        tier_order,
        &creator,
    )
}

#[ic_cdk::update]
async fn remove_subscription_tier(
    tier_id: SubscriptionTierId,
    removed_by: WalletPrincipalId,
) -> Result<(), ClanError> {
    handle_cycle_check();

    {
        let mut clan_state = CLAN.lock().map_err(|_| ClanError::LockError)?;
        let clan_state = clan_state.as_mut().ok_or(ClanError::ClanNotFound)?;

        clan_state.remove_subscription_tier(&tier_id, &removed_by)?;
    }

    Ok(())
}

#[ic_cdk::query]
fn get_subscription_tiers() -> Result<Vec<SubscriptionTier>, ClanError> {
    let clan = CLAN.lock().map_err(|_| ClanError::LockError)?;
    let clan = clan.as_ref().ok_or(ClanError::ClanNotFound)?;
    Ok(clan.subscription_tiers.values().cloned().collect())
}

#[ic_cdk::update]
async fn distribute_rewards(
    distribution: HashMap<WalletPrincipalId, u64>,
    distributed_by: WalletPrincipalId,
) -> Result<(), ClanError> {
    handle_cycle_check();

    let clan_currency = {
        let clan = CLAN.lock().map_err(|_| ClanError::LockError)?;
        let clan = clan.as_ref().ok_or(ClanError::ClanNotFound)?;
        clan.supported_currency.clone()
    };

    // Validate and distribute rewards
    {
        let mut clan_state = CLAN.lock().map_err(|_| ClanError::LockError)?;
        let clan_state = clan_state.as_mut().ok_or(ClanError::ClanNotFound)?;

        // Validate permissions
        let distributor = clan_state
            .members
            .get(&distributed_by)
            .ok_or(ClanError::MemberNotFound)?;

        if !distributor.is_admin_or_higher() {
            return Err(ClanError::InsufficientPermissions);
        }

        clan_state.distribute_rewards(distribution.clone())?;
    }

    // Process actual withdrawals for real currencies
    match clan_currency {
        Currency::ICP | Currency::BTC => {
            let currency_manager = {
                let currency_manager = CURRENCY_MANAGER.lock().map_err(|_| ClanError::LockError)?;
                currency_manager.clone()
            };

            for (recipient, amount) in distribution {
                if amount > 0 {
                    currency_manager
                        .withdraw(&clan_currency, recipient.0, amount)
                        .await?;

                    // Log individual reward
                    let event = ClanEvent::RewardDistributed {
                        amount,
                        to: recipient,
                        timestamp: ic_cdk::api::time(),
                    };
                    CLAN_EVENTS
                        .lock()
                        .map_err(|_| ClanError::LockError)?
                        .add_event(event);
                }
            }
        }
        _ => {} // No actual withdrawal for fake currencies
    }

    Ok(())
}

#[ic_cdk::query]
fn get_clan_treasury() -> Result<ClanTreasury, ClanError> {
    let clan = CLAN.lock().map_err(|_| ClanError::LockError)?;
    let clan = clan.as_ref().ok_or(ClanError::ClanNotFound)?;
    Ok(clan.treasury.clone())
}

#[ic_cdk::query]
fn get_clan_statistics() -> Result<ClanStats, ClanError> {
    let clan = CLAN.lock().map_err(|_| ClanError::LockError)?;
    let clan = clan.as_ref().ok_or(ClanError::ClanNotFound)?;
    Ok(clan.stats.clone())
}

#[ic_cdk::query]
fn get_leaderboard(
    leaderboard_type: LeaderboardType,
) -> Result<Vec<(WalletPrincipalId, u64)>, ClanError> {
    let clan = CLAN.lock().map_err(|_| ClanError::LockError)?;
    let clan = clan.as_ref().ok_or(ClanError::ClanNotFound)?;

    let leaderboard = match leaderboard_type {
        LeaderboardType::Contribution => clan.get_contribution_leaderboard(),
        LeaderboardType::GamesPlayed => clan.get_games_leaderboard(),
        LeaderboardType::Winnings => clan.get_winnings_leaderboard(),
    };

    Ok(leaderboard.into_iter().map(|(p, v)| (*p, v)).collect())
}

#[ic_cdk::update]
async fn request_cycles() -> Result<(), ClanError> {
    let cycles = ic_cdk::api::canister_cycle_balance();
    let caller = ic_cdk::api::msg_caller();

    const CYCLES_TOP_UP_AMOUNT: u128 = 750_000_000_000;

    ic_cdk::println!(
        "Clan canister: Requesting cycles: {} from caller: {}",
        cycles,
        caller.to_text()
    );

    if cycles < CYCLES_TOP_UP_AMOUNT {
        return Err(ClanError::ManagementCanisterError(
            errors::canister_management_error::CanisterManagementError::InsufficientCycles,
        ));
    }

    let backend_principal = BACKEND_PRINCIPAL
        .lock()
        .map_err(|_| ClanError::LockError)?
        .ok_or(ClanError::CanisterCallError(
            "Backend principal not found".to_string(),
        ))?;

    if caller != backend_principal {
        return Err(ClanError::ManagementCanisterError(
            errors::canister_management_error::CanisterManagementError::Transfer(format!(
                "Caller is not the clan index: {}",
                caller
            )),
        ));
    }

    transfer_cycles(CYCLES_TOP_UP_AMOUNT, caller).await
}

async fn transfer_cycles(cycles_amount: u128, caller: Principal) -> Result<(), ClanError> {
    let res = ic_cdk::management_canister::deposit_cycles(
        &DepositCyclesArgs {
            canister_id: caller,
        },
        cycles_amount,
    )
    .await;

    match res {
        Ok(_) => Ok(()),
        Err(e) => Err(ClanError::CanisterCallError(format!(
            "Failed to send cycles: {:?}",
            e
        ))),
    }
}

#[ic_cdk::update]
async fn get_canister_status_formatted() -> Result<String, ClanError> {
    // Validate caller is a controller
    let controllers = (*CONTROLLER_PRINCIPALS).clone();
    validate_caller(controllers);

    handle_cycle_check();

    // Call the management canister to get status
    let canister_status_arg = CanisterStatusArgs {
        canister_id: ic_cdk::api::canister_self(),
    };

    let status_response = canister_status(&canister_status_arg).await.map_err(|e| {
        ClanError::CanisterCallError(format!("Failed to get canister status: {:?}", e))
    })?;

    // Format the status into a readable string
    let formatted_status = format!(
        "📊 Clan Canister Status Report
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

// Helper types and structs

#[derive(Debug, Clone, candid::CandidType, serde::Deserialize)]
pub struct ClanUpdateRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub privacy: Option<ClanPrivacy>,
    pub joining_fee: Option<u64>,
    pub require_proof_of_humanity: Option<bool>,
    pub member_limit: Option<u32>,
    pub avatar: Option<user::user::UserAvatar>,
    pub website: Option<String>,
    pub discord: Option<String>,
    pub twitter: Option<String>,
    pub tags: Option<Vec<ClanTag>>,
    pub environment_settings: Option<ClanEnvironmentSettings>,
}

#[derive(Debug, Clone, candid::CandidType, serde::Deserialize)]
pub enum LeaderboardType {
    Contribution,
    GamesPlayed,
    Winnings,
}

// Additional clan management functions

#[ic_cdk::update]
async fn send_clan_invitation(
    invitee: WalletPrincipalId,
    invited_by: WalletPrincipalId,
    message: Option<String>,
    expires_in_hours: Option<u64>,
) -> Result<ClanInvitation, ClanError> {
    handle_cycle_check();

    let (clan_id, clan_name) = {
        let clan = CLAN.lock().map_err(|_| ClanError::LockError)?;
        let clan = clan.as_ref().ok_or(ClanError::ClanNotFound)?;

        // Validate inviter permissions
        // let inviter = clan.members.get(&invited_by)
        //     .ok_or(ClanError::MemberNotFound)?;

        if !clan.has_tier_access(&invited_by, "can_invite_members")? {
            return Err(ClanError::InsufficientPermissions);
        }

        // Check if already a member
        if clan.is_member(&invitee) {
            return Err(ClanError::UserAlreadyMember);
        }

        (clan.id, clan.name.clone())
    };

    let now = ic_cdk::api::time();
    let expires_at = expires_in_hours.map(|hours| now + (hours * 60 * 60 * 1_000_000_000));

    let invitation = ClanInvitation {
        clan_id,
        clan_name,
        clan_tag: "CLAN".to_string(), // You might want to add a tag field to Clan
        invited_by: invited_by,
        invited_at: now,
        expires_at,
        message,
    };

    // Store invitation in clan
    {
        let mut clan_state = CLAN.lock().map_err(|_| ClanError::LockError)?;
        let clan_state = clan_state.as_mut().ok_or(ClanError::ClanNotFound)?;
        clan_state.invited_users.insert(invitee, now);
    }

    Ok(invitation)
}

#[ic_cdk::update]
async fn accept_clan_invitation(
    user_id: WalletPrincipalId,
    users_canister_principal: UsersCanisterId,
) -> Result<(), ClanError> {
    handle_cycle_check();

    // Check if user was invited
    let was_invited = {
        let clan = CLAN.lock().map_err(|_| ClanError::LockError)?;
        let clan = clan.as_ref().ok_or(ClanError::ClanNotFound)?;
        clan.invited_users.contains_key(&user_id)
    };

    if !was_invited {
        return Err(ClanError::InviteOnlyAccess);
    }

    // Join the clan (no fee for invited members)
    join_clan(users_canister_principal, user_id, 0).await?;

    // Remove from invited users
    {
        let mut clan_state = CLAN.lock().map_err(|_| ClanError::LockError)?;
        let clan_state = clan_state.as_mut().ok_or(ClanError::ClanNotFound)?;
        clan_state.invited_users.remove(&user_id);
    }

    Ok(())
}

#[ic_cdk::update]
async fn submit_join_request(
    user_id: WalletPrincipalId,
    users_canister_principal: UsersCanisterId,
    message: Option<String>,
    referred_by: Option<WalletPrincipalId>,
) -> Result<(), ClanError> {
    handle_cycle_check();

    let clan = {
        let clan = CLAN.lock().map_err(|_| ClanError::LockError)?;
        clan.as_ref().ok_or(ClanError::ClanNotFound)?.clone()
    };

    // Check if clan accepts applications
    if clan.privacy != ClanPrivacy::Application {
        return Err(ClanError::InvalidRequest(
            "Clan does not accept applications".to_string(),
        ));
    }

    // Get user and validate
    let user = get_user_wrapper(users_canister_principal, user_id).await?;
    if !user.can_play() {
        return Err(ClanError::UserBanned);
    }

    // Check if user meets requirements
    clan.meets_joining_requirements(&user)?;

    // Check if already a member or has pending request
    if clan.is_member(&user_id) {
        return Err(ClanError::UserAlreadyMember);
    }

    let has_pending_request = clan
        .pending_requests
        .iter()
        .any(|req| req.applicant == user_id);

    if has_pending_request {
        return Err(ClanError::InvalidRequest(
            "You already have a pending join request".to_string(),
        ));
    }

    // Create join request
    let request = JoinRequest {
        applicant: user_id,
        message,
        requested_at: ic_cdk::api::time(),
        referred_by,
    };

    // Add to pending requests
    {
        let mut clan_state = CLAN.lock().map_err(|_| ClanError::LockError)?;
        let clan_state = clan_state.as_mut().ok_or(ClanError::ClanNotFound)?;
        clan_state.pending_requests.push(request);
    }

    Ok(())
}

#[ic_cdk::update]
async fn approve_join_request(
    applicant: WalletPrincipalId,
    approved_by: WalletPrincipalId,
    users_canister_principal: UsersCanisterId,
) -> Result<(), ClanError> {
    handle_cycle_check();

    // Find and remove the request
    let mut clan_state = CLAN.lock().map_err(|_| ClanError::LockError)?;
    let clan_state = clan_state.as_mut().ok_or(ClanError::ClanNotFound)?;

    // Validate approver permissions
    let approver = clan_state
        .members
        .get(&approved_by)
        .ok_or(ClanError::MemberNotFound)?;

    if !approver.can_moderate() {
        return Err(ClanError::InsufficientPermissions);
    }

    // Find and remove the request
    let request_index = clan_state
        .pending_requests
        .iter()
        .position(|req| req.applicant == applicant)
        .ok_or(ClanError::JoinRequestNotFound)?;

    clan_state.pending_requests.remove(request_index);

    // Add member to clan (no fee for approved applications)
    join_clan(users_canister_principal, applicant, 0).await?;

    Ok(())
}

#[ic_cdk::update]
async fn reject_join_request(
    applicant: Principal,
    rejected_by: WalletPrincipalId,
) -> Result<(), ClanError> {
    handle_cycle_check();

    {
        let mut clan_state = CLAN.lock().map_err(|_| ClanError::LockError)?;
        let clan_state = clan_state.as_mut().ok_or(ClanError::ClanNotFound)?;

        // Validate rejector permissions
        let rejector = clan_state
            .members
            .get(&rejected_by)
            .ok_or(ClanError::MemberNotFound)?;

        if !rejector.can_moderate() {
            return Err(ClanError::InsufficientPermissions);
        }

        // Find and remove the request
        let request_index = clan_state
            .pending_requests
            .iter()
            .position(|req| req.applicant == applicant)
            .ok_or(ClanError::JoinRequestNotFound)?;

        clan_state.pending_requests.remove(request_index);
    }

    Ok(())
}

#[ic_cdk::query]
fn get_pending_requests() -> Result<Vec<JoinRequest>, ClanError> {
    let clan = CLAN.lock().map_err(|_| ClanError::LockError)?;
    let clan = clan.as_ref().ok_or(ClanError::ClanNotFound)?;
    Ok(clan.pending_requests.clone())
}

#[ic_cdk::query]
fn get_invited_users() -> Result<HashMap<WalletPrincipalId, u64>, ClanError> {
    let clan = CLAN.lock().map_err(|_| ClanError::LockError)?;
    let clan = clan.as_ref().ok_or(ClanError::ClanNotFound)?;
    Ok(clan.invited_users.clone())
}

#[ic_cdk::update]
async fn update_member_stats(
    member_id: WalletPrincipalId,
    games_played_delta: u64,
    tournaments_won_delta: u64,
    winnings_delta: u64,
    xp_delta: u64,
) -> Result<(), ClanError> {
    handle_cycle_check();

    // Validate caller is backend
    let backend_principal = BACKEND_PRINCIPAL
        .lock()
        .map_err(|_| ClanError::LockError)?
        .ok_or(ClanError::CanisterCallError(
            "Backend principal not found".to_string(),
        ))?;

    validate_caller(vec![backend_principal]);

    {
        let mut clan_state = CLAN.lock().map_err(|_| ClanError::LockError)?;
        let clan_state = clan_state.as_mut().ok_or(ClanError::ClanNotFound)?;

        if let Some(member) = clan_state.members.get_mut(&member_id) {
            member.games_played += games_played_delta;
            member.tournaments_won += tournaments_won_delta;
            member.total_winnings += winnings_delta;
            member.xp += xp_delta;
            member.last_active = ic_cdk::api::time();

            // Update contribution points based on activity
            let contribution_points =
                games_played_delta * 10 + tournaments_won_delta * 100 + xp_delta / 10;
            member.contribution_points += contribution_points;
        }

        // Update clan stats
        clan_state.stats.total_games_played += games_played_delta;
        clan_state.stats.last_activity = ic_cdk::api::time();
    }

    Ok(())
}

#[ic_cdk::update]
async fn add_clan_revenue(amount: u64, source: RevenueSource) -> Result<(), ClanError> {
    handle_cycle_check();

    // Validate caller is backend
    let backend_principal = BACKEND_PRINCIPAL
        .lock()
        .map_err(|_| ClanError::LockError)?
        .ok_or(ClanError::CanisterCallError(
            "Backend principal not found".to_string(),
        ))?;

    validate_caller(vec![backend_principal]);

    {
        let mut clan_state = CLAN.lock().map_err(|_| ClanError::LockError)?;
        let clan_state = clan_state.as_mut().ok_or(ClanError::ClanNotFound)?;

        // Add to treasury (50% clan share for MVP)
        let clan_share = amount / 2;
        clan_state.treasury.balance += clan_share;
        clan_state.treasury.total_revenue_generated += clan_share;

        // Update stats
        clan_state.stats.total_revenue_generated += clan_share;
        clan_state.stats.last_activity = ic_cdk::api::time();

        match source {
            RevenueSource::Tournament => {
                clan_state.stats.total_tournaments_hosted += 1;
            }
            RevenueSource::Table => {
                // Table revenue tracking
            }
            RevenueSource::Subscription => {
                clan_state.treasury.total_subscription_revenue += clan_share;
            }
        }
    }

    // Log event
    let event = ClanEvent::RevenueGenerated {
        amount,
        timestamp: ic_cdk::api::time(),
    };
    CLAN_EVENTS
        .lock()
        .map_err(|_| ClanError::LockError)?
        .add_event(event);

    Ok(())
}

#[ic_cdk::update]
async fn process_subscription_renewals() -> Result<Vec<(WalletPrincipalId, String)>, ClanError> {
    handle_cycle_check();

    // Validate caller is backend
    let backend_principal = BACKEND_PRINCIPAL
        .lock()
        .map_err(|_| ClanError::LockError)?
        .ok_or(ClanError::CanisterCallError(
            "Backend principal not found".to_string(),
        ))?;

    validate_caller(vec![backend_principal]);

    let renewal_results = {
        let mut clan_state = CLAN.lock().map_err(|_| ClanError::LockError)?;
        let clan_state = clan_state.as_mut().ok_or(ClanError::ClanNotFound)?;

        clan_state.process_subscription_renewals()
    };

    Ok(renewal_results)
}

#[ic_cdk::query]
fn has_tier_access(
    member_id: WalletPrincipalId,
    required_benefit: String,
) -> Result<bool, ClanError> {
    let clan = CLAN.lock().map_err(|_| ClanError::LockError)?;
    let clan = clan.as_ref().ok_or(ClanError::ClanNotFound)?;

    clan.has_tier_access(&member_id, &required_benefit)
}

#[ic_cdk::query]
fn can_access_table_stakes(
    member_id: WalletPrincipalId,
    table_stakes: u64,
) -> Result<bool, ClanError> {
    let clan = CLAN.lock().map_err(|_| ClanError::LockError)?;
    let clan = clan.as_ref().ok_or(ClanError::ClanNotFound)?;

    clan.can_access_table_stakes(&member_id, table_stakes)
}

#[derive(Debug, Clone, candid::CandidType, serde::Deserialize)]
pub enum RevenueSource {
    Tournament,
    Table,
    Subscription,
}

ic_cdk::export_candid!();
