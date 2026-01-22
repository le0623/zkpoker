use authentication::validate_caller;
// use authentication::validate_caller;
use candid::{Nat, Principal};
use canister_functions::{
    create_canister_wrapper,
    cycle::{
        check_and_top_up_canister, get_cycle_balances, monitor_and_top_up_canisters,
        top_up_canister,
    },
    install_wasm_code, stop_and_delete_canister, upgrade_wasm_code,
};
use currency::types::currency_manager::CurrencyManager;
use errors::{canister_management_error::CanisterManagementError, user_error::UserError};
use ic_cdk::management_canister::{canister_status, CanisterStatusArgs};
use ic_ledger_types::{AccountIdentifier, Subaccount, DEFAULT_SUBACCOUNT};
use intercanister_call_wrappers::users_canister::{
    create_user_wrapper, get_user_by_username_wrapper, get_user_wrapper, update_user_wrapper
};
use lazy_static::lazy_static;
use user::user::{User, UserAvatar, UsersCanisterId, WalletPrincipalId};
use user_index::{get_position_in_leaderboard, UserIndex};

use std::sync::Mutex;

mod memory;
pub mod reset_xp_utils;
pub mod user_index;

const MINIMUM_CYCLE_THRESHOLD: u128 = 2_000_000_000_000;

pub struct CanisterState {
    pub owner: Principal,
    pub default_subaccount: Subaccount,
    pub account_identifier: AccountIdentifier,
}

async fn handle_cycle_check() -> Result<(), UserError> {
    let id = ic_cdk::api::canister_self();
    let cycle_dispenser_canister_id =
        if id == Principal::from_text("lvq5c-nyaaa-aaaam-qdswa-cai").unwrap() {
            *CYCLE_DISPENSER_CANISTER_PROD
        } else if id == Principal::from_text("m3tym-daaaa-aaaah-qqbsq-cai").unwrap() {
            *CYCLE_DISPENSER_CANISTER_TEST
        } else if id == Principal::from_text("txyno-ch777-77776-aaaaq-cai").unwrap() {
            *CYCLE_DISPENSER_CANISTER_DEV
        } else {
            return Ok(());
        };
    check_and_top_up_canister(id, cycle_dispenser_canister_id, MINIMUM_CYCLE_THRESHOLD).await?;
    Ok(())
}

pub type PlayerId = u64;
pub type TableId = u64;

// Define a global instance of GameState wrapped in a Mutex for safe concurrent access.
lazy_static! {
    static ref USER_INDEX_STATE: Mutex<UserIndex> = Mutex::new(UserIndex::new());
    static ref CANISTER_STATE: Mutex<Option<CanisterState>> = Mutex::new(None);
    static ref SUPPORT_US_WALLET: Principal =
        Principal::from_text("amwxf-a2rkd-b42qc-jwbst-oy3co-d5ues-jgfcp-khbg4-zdxoa-n66ja-2ae")
            .unwrap();
    static ref CYCLE_DISPENSER_CANISTER_PROD: Principal =
        Principal::from_text("zuv6g-yaaaa-aaaam-qbeza-cai").unwrap();
    static ref CYCLE_DISPENSER_CANISTER_TEST: Principal =
        Principal::from_text("ev34d-5yaaa-aaaah-qdska-cai").unwrap();
    static ref CYCLE_DISPENSER_CANISTER_DEV: Principal =
        Principal::from_text("tz2ag-zx777-77776-aaabq-cai").unwrap();
    static ref CONTROLLER_PRINCIPALS: Vec<Principal> = vec![
        Principal::from_text("km7qz-4bai4-e5ptx-hgrck-z3web-ameqg-ksxcf-u7wbr-t5fna-i7bqp-hqe")
            .unwrap(),
        Principal::from_text("uyxh5-bi3za-gxbfs-op3gj-ere73-a6jhv-5jky3-zawef-b5r2s-k26un-sae")
            .unwrap(),
        Principal::from_text("w3kjy-pitqg-dvab7-tb57q-63gnd-di4vo-loiiy-s6zm2-gqcmw-ixliz-aae").unwrap(),
        Principal::from_text("tcuxo-b5b4t-vxwo7-mwgxi-vb4ig-zuux4-jmvru-fjocv-4uxuz-7yo4v-hqe").unwrap(),
        Principal::from_text("2hbym-ivof6-l2yyh-zgu62-fqgf3-nvtlj-edso2-ebahf-p5j5c-2p2eb-lae").unwrap(),
    ];
    static ref USER_CANISTER_WASM: &'static [u8] =
        include_bytes!("../../../target/wasm32-unknown-unknown/release/users_canister.wasm");
    static ref LEADERBOARD_CACHE: Mutex<Option<Vec<(WalletPrincipalId, u64)>>> = Mutex::new(None);
    static ref LEADERBOARD_CACHE_TIMESTAMP: Mutex<Option<u64>> = Mutex::new(None);
    static ref PURE_POKER_LEADERBOARD_CACHE: Mutex<Option<Vec<(WalletPrincipalId, u64)>>> =
        Mutex::new(None);
    static ref PURE_POKER_LEADERBOARD_CACHE_TIMESTAMP: Mutex<Option<u64>> = Mutex::new(None);
    static ref VERIFIED_LEADERBOARD_CACHE: Mutex<Option<Vec<(WalletPrincipalId, u64)>>> =
        Mutex::new(None);
    static ref VERIFIED_LEADERBOARD_CACHE_TIMESTAMP: Mutex<Option<u64>> = Mutex::new(None);
    static ref VERIFIED_PURE_POKER_LEADERBOARD_CACHE: Mutex<Option<Vec<(WalletPrincipalId, u64)>>> =
        Mutex::new(None);
    static ref VERIFIED_PURE_POKER_LEADERBOARD_CACHE_TIMESTAMP: Mutex<Option<u64>> =
        Mutex::new(None);
    static ref CURRENCY_MANAGER: Mutex<CurrencyManager> = Mutex::new(CurrencyManager::new());
}

fn get_canister_state() -> CanisterState {
    let owner_principal = ic_cdk::api::canister_self();

    let account_identifier = AccountIdentifier::new(&owner_principal, &DEFAULT_SUBACCOUNT);
    CanisterState {
        owner: owner_principal,
        default_subaccount: DEFAULT_SUBACCOUNT,
        account_identifier,
    }
}

#[ic_cdk::init]
fn init() {
    let id = ic_cdk::api::canister_self();
    ic_cdk::println!("Users index canister {id} initialized");
    let canister_state = get_canister_state();
    let mut canister_state_mutex = match CANISTER_STATE.lock() {
        Ok(canister_state) => canister_state,
        Err(_) => {
            ic_cdk::println!("Failed to acquire lock");
            return;
        }
    };
    *canister_state_mutex = Some(canister_state);
}

#[ic_cdk::query]
fn get_account_number() -> Result<Option<String>, UserError> {
    let canister_state = get_canister_state();
    Ok(Some(canister_state.account_identifier.to_string()))
}

#[ic_cdk::query]
fn ping() -> String {
    "Ok".to_string()
}

#[ic_cdk::update]
async fn create_user(
    user_name: String,
    address: Option<String>,
    principal_id: WalletPrincipalId,
    avatar: Option<UserAvatar>,
    referrer: Option<WalletPrincipalId>,
) -> Result<User, UserError> {
    handle_cycle_check().await?;
    if user_name.is_empty() {
        return Err(UserError::InvalidRequest(
            "User name cannot be empty".to_string(),
        ));
    }

    let mut user_index_state = {
        USER_INDEX_STATE
            .lock()
            .map_err(|_| UserError::LockError)?
            .clone()
    };

    if user_index_state.does_user_name_exist(&user_name).await {
        return Err(UserError::UserAlreadyExists);
    }

    // Check if user already exists
    if user_index_state
        .user_to_canister
        .contains_key(&principal_id)
    {
        return Err(UserError::UserAlreadyExists);
    }

    // Find an available canister or create a new one
    let user_canister = match user_index_state.get_available_canister() {
        Some(canister_id) => canister_id,
        None => {
            // No available canister, create a new one
            let controller_principals = CONTROLLER_PRINCIPALS.clone();
            let wasm_module = USER_CANISTER_WASM.to_vec();
            let new_canister = create_canister_wrapper(controller_principals, Some(3_000_000_000_000)).await?;
            install_wasm_code(new_canister, wasm_module).await?;

            let new_canister = UsersCanisterId(new_canister);
            // Initialize the count for this new canister
            user_index_state.canister_user_count.insert(new_canister, 0);

            new_canister
        }
    };

    let res = create_user_wrapper(
        user_canister,
        user_name,
        address,
        principal_id,
        avatar,
        referrer,
    )
    .await;

    let mut user_index_state = USER_INDEX_STATE.lock().map_err(|_| UserError::LockError)?;

    // Handle the result and update the index
    match res {
        Ok((user, player_count)) => {
            user_index_state.add_user(principal_id, user_canister)?;

            // Mark the canister as full if needed
            if let Some(count) = user_index_state.canister_user_count.get_mut(&user_canister) {
                *count = player_count;
            } else {
                user_index_state
                    .canister_user_count
                    .insert(user_canister, player_count);
            }

            Ok(user)
        }
        Err(e) => Err(e),
    }
}

#[ic_cdk::update]
async fn get_users_canister_principal_by_id(
    user_id: WalletPrincipalId,
) -> Result<UsersCanisterId, UserError> {
    let user_index_state = USER_INDEX_STATE.lock().map_err(|_| UserError::LockError)?;

    match user_index_state.get_users_canister_principal(user_id) {
        Some(user_id) => Ok(*user_id),
        None => Err(UserError::UserNotFound),
    }
}

#[ic_cdk::update]
async fn update_user(
    user_canister_principal_id: UsersCanisterId,
    user_name: Option<String>,
    balance: Option<u64>,
    address: Option<String>,
    principal_id: WalletPrincipalId,
    wallet_principal_id: Option<String>,
    avatar: Option<UserAvatar>,
) -> Result<User, UserError> {
    validate_caller(vec![principal_id.0]);
    handle_cycle_check().await?;

    let res = update_user_wrapper(
        user_canister_principal_id,
        user_name,
        balance,
        address,
        principal_id,
        wallet_principal_id,
        avatar,
    )
    .await;

    res
}

#[ic_cdk::update]
async fn get_user(user_id: WalletPrincipalId) -> Result<User, UserError> {
    handle_cycle_check().await?;
    let user_canister_principal_id = {
        let user_index_state = USER_INDEX_STATE.lock().map_err(|_| UserError::LockError)?;
        *user_index_state
            .get_users_canister_principal(user_id)
            .ok_or(UserError::UserNotFound)?
    };
    get_user_wrapper(user_canister_principal_id, user_id).await
}

#[ic_cdk::update]
async fn top_up_user_canister(canister_id: Principal, amount: u128) -> Result<(), UserError> {
    handle_cycle_check().await?;
    top_up_canister(canister_id, amount).await?;
    Ok(())
}

#[ic_cdk::update]
async fn monitor_and_top_up_user_canisters() -> Result<(), UserError> {
    handle_cycle_check().await?;
    let user_canisters = {
        let user_index_state = USER_INDEX_STATE.lock().map_err(|_| UserError::LockError)?;
        user_index_state
            .get_user_canisters()
            .iter()
            .map(|c| c.0)
            .collect()
    };

    monitor_and_top_up_canisters(user_canisters).await?;
    Ok(())
}

#[ic_cdk::query]
fn get_user_canisters() -> Result<Vec<WalletPrincipalId>, UserError> {
    let user_index_state = USER_INDEX_STATE.lock().map_err(|_| UserError::LockError)?;
    Ok(user_index_state.get_user_canisters())
}

#[ic_cdk::query]
fn get_number_of_registered_users() -> Result<usize, UserError> {
    let user_index_state = USER_INDEX_STATE.lock().map_err(|_| UserError::LockError)?;
    Ok(user_index_state.user_count())
}

#[ic_cdk::query]
async fn get_user_canisters_cycles() -> Result<Vec<(Principal, Nat)>, UserError> {
    let user_canisters = {
        let user_index_state = USER_INDEX_STATE.lock().map_err(|_| UserError::LockError)?;
        user_index_state
            .get_user_canisters()
            .iter()
            .map(|c| c.0)
            .collect::<Vec<_>>()
    };

    let balances = get_cycle_balances(user_canisters).await;
    Ok(balances)
}

#[ic_cdk::query]
async fn get_user_canister_cycles(user_canister: Principal) -> Result<Nat, UserError> {
    let balances = get_cycle_balances(vec![user_canister]).await;
    Ok(balances[0].1.clone())
}

const CYCLES_TOP_UP_AMOUNT: u128 = 750_000_000_000;

#[ic_cdk::update]
async fn request_cycles() -> Result<(), UserError> {
    let cycles = ic_cdk::api::canister_cycle_balance();
    let caller = ic_cdk::api::msg_caller();
    if cycles < CYCLES_TOP_UP_AMOUNT {
        return Err(UserError::ManagementCanisterError(
            CanisterManagementError::InsufficientCycles,
        ));
    }

    transfer_cycles(CYCLES_TOP_UP_AMOUNT, caller).await
}

async fn transfer_cycles(cycles_amount: u128, caller: Principal) -> Result<(), UserError> {
    {
        let user_index_state = USER_INDEX_STATE.lock().map_err(|_| UserError::LockError)?;
        if !user_index_state
            .canister_user_count
            .contains_key(&UsersCanisterId(caller))
        {
            return Err(UserError::ManagementCanisterError(
                CanisterManagementError::Transfer(format!(
                    "Caller is not a valid destination: {}",
                    caller
                )),
            ));
        }
    }

    top_up_canister(caller, cycles_amount).await?;
    Ok(())
}

fn safely_get_leaderboard_page(
    leaderboard: &[(WalletPrincipalId, u64)],
    page: u64,
    page_size: u64,
) -> Result<Vec<(WalletPrincipalId, u64)>, UserError> {
    let start = page as usize * page_size as usize;
    let end = start + page_size as usize;
    if start >= leaderboard.len() {
        return Ok(vec![]);
    }
    if end > leaderboard.len() {
        return Ok(leaderboard[start..].to_vec());
    }
    Ok(leaderboard[start..end].to_vec())
}

#[ic_cdk::update]
async fn get_experience_points_leaderboard(
    page: u64,
    page_size: u64,
) -> Result<Vec<(WalletPrincipalId, u64)>, UserError> {
    {
        if let Some(leaderboard_cache_timestamp) = *LEADERBOARD_CACHE_TIMESTAMP
            .lock()
            .map_err(|_| UserError::LockError)?
        {
            if ic_cdk::api::time() - leaderboard_cache_timestamp < 3_600_000_000_000 {
                // 1 hour
                let leaderboard_cache =
                    LEADERBOARD_CACHE.lock().map_err(|_| UserError::LockError)?;
                if let Some(leaderboard) = &*leaderboard_cache {
                    return safely_get_leaderboard_page(leaderboard, page, page_size);
                }
            }
        }
    }

    let user_index_state = USER_INDEX_STATE
        .lock()
        .map_err(|_| UserError::LockError)?
        .clone();
    let leaderboard = user_index_state.get_experience_points_leaderboard().await?;

    let mut leaderboard_cache = LEADERBOARD_CACHE.lock().map_err(|_| UserError::LockError)?;
    let mut leaderboard_cache_timestamp = LEADERBOARD_CACHE_TIMESTAMP
        .lock()
        .map_err(|_| UserError::LockError)?;
    leaderboard_cache.replace(leaderboard.clone());
    leaderboard_cache_timestamp.replace(ic_cdk::api::time());

    safely_get_leaderboard_page(&leaderboard, page, page_size)
}

#[ic_cdk::update]
async fn get_verified_experience_points_leaderboard(
    page: u64,
    page_size: u64,
) -> Result<Vec<(WalletPrincipalId, u64)>, UserError> {
    {
        if let Some(leaderboard_cache_timestamp) = *VERIFIED_LEADERBOARD_CACHE_TIMESTAMP
            .lock()
            .map_err(|_| UserError::LockError)?
        {
            if ic_cdk::api::time() - leaderboard_cache_timestamp < 3_600_000_000_000 {
                // 1 hour
                let leaderboard_cache = VERIFIED_LEADERBOARD_CACHE
                    .lock()
                    .map_err(|_| UserError::LockError)?;
                if let Some(leaderboard) = &*leaderboard_cache {
                    return safely_get_leaderboard_page(leaderboard, page, page_size);
                }
            }
        }
    }

    let user_index_state = USER_INDEX_STATE
        .lock()
        .map_err(|_| UserError::LockError)?
        .clone();
    let leaderboard = user_index_state
        .get_verified_experience_points_leaderboard()
        .await?;

    let mut leaderboard_cache = VERIFIED_LEADERBOARD_CACHE
        .lock()
        .map_err(|_| UserError::LockError)?;
    let mut leaderboard_cache_timestamp = VERIFIED_LEADERBOARD_CACHE_TIMESTAMP
        .lock()
        .map_err(|_| UserError::LockError)?;
    leaderboard_cache.replace(leaderboard.clone());
    leaderboard_cache_timestamp.replace(ic_cdk::api::time());

    safely_get_leaderboard_page(&leaderboard, page, page_size)
}

#[ic_cdk::update]
async fn get_verified_experience_points_leaderboard_length() -> Result<usize, UserError> {
    {
        if let Some(leaderboard_cache_timestamp) = *VERIFIED_LEADERBOARD_CACHE_TIMESTAMP
            .lock()
            .map_err(|_| UserError::LockError)?
        {
            if ic_cdk::api::time() - leaderboard_cache_timestamp < 3_600_000_000_000 {
                // 1 hour
                let leaderboard_cache = VERIFIED_LEADERBOARD_CACHE
                    .lock()
                    .map_err(|_| UserError::LockError)?;
                if let Some(leaderboard) = &*leaderboard_cache {
                    return Ok(leaderboard.len());
                }
            }
        }
    }

    let user_index_state = USER_INDEX_STATE
        .lock()
        .map_err(|_| UserError::LockError)?
        .clone();
    let leaderboard = user_index_state
        .get_verified_experience_points_leaderboard()
        .await?;

    let mut leaderboard_cache = VERIFIED_LEADERBOARD_CACHE
        .lock()
        .map_err(|_| UserError::LockError)?;
    let mut leaderboard_cache_timestamp = VERIFIED_LEADERBOARD_CACHE_TIMESTAMP
        .lock()
        .map_err(|_| UserError::LockError)?;
    leaderboard_cache.replace(leaderboard.clone());
    leaderboard_cache_timestamp.replace(ic_cdk::api::time());
    Ok(leaderboard.len())
}

#[ic_cdk::update]
async fn get_pure_poker_experience_points(
    page: u64,
    page_size: u64,
) -> Result<Vec<(WalletPrincipalId, u64)>, UserError> {
    {
        if let Some(leaderboard_cache_timestamp) = *PURE_POKER_LEADERBOARD_CACHE_TIMESTAMP
            .lock()
            .map_err(|_| UserError::LockError)?
        {
            if ic_cdk::api::time() - leaderboard_cache_timestamp < 3_600_000_000_000 {
                // 1 hour
                let leaderboard_cache = PURE_POKER_LEADERBOARD_CACHE
                    .lock()
                    .map_err(|_| UserError::LockError)?;
                if let Some(leaderboard) = &*leaderboard_cache {
                    return safely_get_leaderboard_page(leaderboard, page, page_size);
                }
            }
        }
    }

    let user_index_state = USER_INDEX_STATE
        .lock()
        .map_err(|_| UserError::LockError)?
        .clone();
    let leaderboard = user_index_state
        .get_pure_poker_experience_points_leaderboard()
        .await?;

    let mut leaderboard_cache = PURE_POKER_LEADERBOARD_CACHE
        .lock()
        .map_err(|_| UserError::LockError)?;
    let mut leaderboard_cache_timestamp = PURE_POKER_LEADERBOARD_CACHE_TIMESTAMP
        .lock()
        .map_err(|_| UserError::LockError)?;
    leaderboard_cache.replace(leaderboard.clone());
    leaderboard_cache_timestamp.replace(ic_cdk::api::time());

    safely_get_leaderboard_page(&leaderboard, page, page_size)
}

#[ic_cdk::update]
async fn get_verified_pure_poker_experience_points(
    page: u64,
    page_size: u64,
) -> Result<Vec<(WalletPrincipalId, u64)>, UserError> {
    {
        if let Some(leaderboard_cache_timestamp) = *VERIFIED_PURE_POKER_LEADERBOARD_CACHE_TIMESTAMP
            .lock()
            .map_err(|_| UserError::LockError)?
        {
            if ic_cdk::api::time() - leaderboard_cache_timestamp < 3_600_000_000_000 {
                // 1 hour
                let leaderboard_cache = VERIFIED_PURE_POKER_LEADERBOARD_CACHE
                    .lock()
                    .map_err(|_| UserError::LockError)?;
                if let Some(leaderboard) = &*leaderboard_cache {
                    return safely_get_leaderboard_page(leaderboard, page, page_size);
                }
            }
        }
    }

    let user_index_state = USER_INDEX_STATE
        .lock()
        .map_err(|_| UserError::LockError)?
        .clone();
    let leaderboard = user_index_state
        .get_verified_pure_poker_experience_points_leaderboard()
        .await?;

    let mut leaderboard_cache = VERIFIED_PURE_POKER_LEADERBOARD_CACHE
        .lock()
        .map_err(|_| UserError::LockError)?;
    let mut leaderboard_cache_timestamp = VERIFIED_PURE_POKER_LEADERBOARD_CACHE_TIMESTAMP
        .lock()
        .map_err(|_| UserError::LockError)?;
    leaderboard_cache.replace(leaderboard.clone());
    leaderboard_cache_timestamp.replace(ic_cdk::api::time());

    safely_get_leaderboard_page(&leaderboard, page, page_size)
}

#[ic_cdk::query]
fn get_leaderboard_length() -> Result<usize, UserError> {
    let user_index_state = USER_INDEX_STATE.lock().map_err(|_| UserError::LockError)?;
    Ok(user_index_state.user_to_canister.len())
}

#[ic_cdk::update]
async fn upgrade_all_user_canisters(
) -> Result<Vec<(UsersCanisterId, CanisterManagementError)>, UserError> {
    // Validate caller permissions
    let caller = ic_cdk::api::msg_caller();
    if !CONTROLLER_PRINCIPALS.contains(&caller) {
        return Err(UserError::AuthorizationError);
    }

    handle_cycle_check().await?;

    const BATCH_SIZE: usize = 30; // Process 30 users at a time, same as in get_experience_points_leaderboard

    let users: Vec<UsersCanisterId> = {
        let user_index_state = USER_INDEX_STATE.lock().map_err(|_| UserError::LockError)?;
        user_index_state.get_users_canisters()
    };

    let wasm_module = USER_CANISTER_WASM.to_vec();
    let mut failed_upgrades = Vec::new();

    // Process users in batches
    for chunk in users.chunks(BATCH_SIZE) {
        let futures: Vec<_> = chunk
            .iter()
            .map(|&user_canister| {
                let wasm_clone = wasm_module.clone();
                async move {
                    match upgrade_wasm_code(user_canister.0, wasm_clone).await {
                        Ok(_) => {
                            ic_cdk::println!(
                                "Successfully upgraded canister {}",
                                user_canister.0.to_text()
                            );
                            Ok(user_canister)
                        }
                        Err(e) => {
                            ic_cdk::println!(
                                "Failed to upgrade canister {}: {:?}",
                                user_canister.0.to_text(),
                                e
                            );
                            Err((user_canister, e))
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
async fn upgrade_user_canister(user_canister: Principal) -> Result<(), UserError> {
    // Validate caller permissions
    let caller = ic_cdk::api::msg_caller();
    if !CONTROLLER_PRINCIPALS.contains(&caller) {
        return Err(UserError::AuthorizationError);
    }

    handle_cycle_check().await?;

    let wasm_module = USER_CANISTER_WASM.to_vec();
    upgrade_wasm_code(user_canister, wasm_module).await?;
    Ok(())
}

#[ic_cdk::update]
async fn get_experience_points_position(
    user_principal: Principal,
) -> Result<Option<u64>, UserError> {
    handle_cycle_check().await?;
    get_position_in_leaderboard(user_principal, false).await
}

#[ic_cdk::update]
async fn get_pure_poker_position(user_principal: Principal) -> Result<Option<u64>, UserError> {
    handle_cycle_check().await?;
    get_position_in_leaderboard(user_principal, true).await
}

#[ic_cdk::update]
async fn delete_users_canister(user_canister: UsersCanisterId) -> Result<(), UserError> {
    // Validate caller permissions
    let caller = ic_cdk::api::msg_caller();
    if !CONTROLLER_PRINCIPALS.contains(&caller) {
        return Err(UserError::AuthorizationError);
    }

    {
        let mut user_index_state = USER_INDEX_STATE.lock().map_err(|_| UserError::LockError)?;

        for (user_id, canister_id) in user_index_state.user_to_canister.clone().iter() {
            if *canister_id == user_canister {
                user_index_state.user_to_canister.remove(user_id);
            }
        }

        user_index_state.canister_user_count.remove(&user_canister);
    }

    handle_cycle_check().await?;

    // Delete the canister
    stop_and_delete_canister(user_canister.0)
        .await
        .map_err(|e| {
            UserError::ManagementCanisterError(CanisterManagementError::DeleteCanisterError(
                format!(
                    "Failed to delete canister {}: {:?}",
                    user_canister.0.to_text(),
                    e
                ),
            ))
        })?;

    Ok(())
}

#[ic_cdk::update]
async fn get_user_by_username(user_name: String) -> Result<(WalletPrincipalId, UsersCanisterId), UserError> {
    let users_canister_ids = {
        let user_index_state = USER_INDEX_STATE.lock().map_err(|_| UserError::LockError)?;
        user_index_state.get_users_canisters()
    };

    for user_canister in users_canister_ids {
        let user = get_user_by_username_wrapper(user_canister, user_name.clone()).await;
        match user {
            Ok(user) => {
                if user.user_name == user_name {
                    return Ok((user.principal_id, user_canister));
                }
            }
            Err(_) => continue, // Ignore errors and continue searching
        }
    }

    Err(UserError::UserNotFound)
}

#[ic_cdk::update]
async fn get_canister_status_formatted() -> Result<String, UserError> {
    // Validate caller is a controller
    let controllers = (*CONTROLLER_PRINCIPALS).clone();
    validate_caller(controllers);

    handle_cycle_check().await?;

    // Call the management canister to get status
    let canister_status_arg = CanisterStatusArgs {
        canister_id: ic_cdk::api::canister_self(),
    };

    let status_response = canister_status(&canister_status_arg).await.map_err(|e| {
        UserError::CanisterCallFailed(format!("Failed to get canister status: {:?}", e))
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

#[ic_cdk::update]
async fn get_ckbtc_balance() -> Result<u64, UserError> {
    let currency_manager = {
        CURRENCY_MANAGER
            .lock()
            .map_err(|_| UserError::LockError)?
            .clone()
    };
    let balance = currency_manager
        .get_balance(&currency::Currency::BTC, ic_cdk::api::canister_self())
        .await
        .map_err(|e| UserError::CanisterCallFailed(format!("{:?}", e)))?;
    Ok(balance as u64)
}

ic_cdk::export_candid!();
