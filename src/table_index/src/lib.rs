use authentication::validate_caller;
use candid::{Nat, Principal};
use canister_functions::{
    create_canister_wrapper,
    cycle::{check_and_top_up_canister, monitor_and_top_up_canisters, top_up_canister},
    install_wasm_code, rake_constants,
    rake_stats::{GlobalRakeStats, RakeStats, TableRakeStats},
    stop_and_delete_canister,
};
use currency::{state::TransactionState, types::currency_manager::CurrencyManager, Currency};
use errors::{
    canister_management_error::CanisterManagementError, table_error::TableError,
    table_index_error::TableIndexError,
};
use futures::future::join_all;
use ic_cdk::management_canister::{canister_status, CanisterStatusArgs};
use intercanister_call_wrappers::table_index::get_rake_stats;
use lazy_static::lazy_static;
use std::{cmp::Ordering, collections::HashMap, sync::Mutex};
use table::poker::game::{
    table_functions::{
        rake::Rake,
        table::{SmallBlind, TableConfig, TableId, TableType},
        types::CurrencyType,
    },
    types::{GameType, PublicTable},
};
use table::table_canister::{
    clear_table, create_table_wrapper, get_table_wrapper, is_game_ongoing_wrapper, join_table,
    return_all_cycles_to_index,
};
use table_index::{PrivateTableIndex, PublicTableIndex};
use table_index_types::filter::FilterOptions;
use user::user::{UsersCanisterId, WalletPrincipalId};
use utils::{get_canister_state, is_table_full};

mod memory;
pub mod table_index;
pub mod utils;

const MINIMUM_CYCLE_THRESHOLD: u128 = 2_000_000_000_000;

async fn handle_cycle_check() -> Result<(), TableIndexError> {
    let id = ic_cdk::api::canister_self();
    let cycle_dispenser_canister_id =
        if id == Principal::from_text("zbspl-ziaaa-aaaam-qbe2q-cai").unwrap() {
            *CYCLE_DISPENSER_CANISTER_PROD
        } else if id == Principal::from_text("e4yx7-lqaaa-aaaah-qdslq-cai").unwrap() {
            *CYCLE_DISPENSER_CANISTER_TEST
        } else if id == Principal::from_text("tqzl2-p7777-77776-aaaaa-cai").unwrap() {
            *CYCLE_DISPENSER_CANISTER_DEV
        } else {
            return Ok(());
        };
    check_and_top_up_canister(id, cycle_dispenser_canister_id, MINIMUM_CYCLE_THRESHOLD).await?;
    Ok(())
}

// Define a global instance of GameState wrapped in a Mutex for safe concurrent access.
lazy_static! {
    static ref PUBLIC_TABLE_INDEX_STATE: Mutex<PublicTableIndex> =
        Mutex::new(PublicTableIndex::new());
    static ref PRIVATE_TABLE_INDEX_STATE: Mutex<PrivateTableIndex> =
        Mutex::new(PrivateTableIndex::new());
    static ref TABLE_PLAYER_COUNTS: Mutex<HashMap<TableId, usize>> = Mutex::new(HashMap::new());
    static ref CYCLE_DISPENSER_CANISTER_PROD: Principal =
        Principal::from_text("zuv6g-yaaaa-aaaam-qbeza-cai").unwrap();
    static ref CYCLE_DISPENSER_CANISTER_TEST: Principal =
        Principal::from_text("ev34d-5yaaa-aaaah-qdska-cai").unwrap();
    static ref CYCLE_DISPENSER_CANISTER_DEV: Principal =
        Principal::from_text("tz2ag-zx777-77776-aaabq-cai").unwrap();
    static ref CONTROLLER_PRINCIPALS: Vec<Principal> = vec![
        Principal::from_text("py2cj-ei3dt-3ber7-nvxdl-56xvh-qkhop-7x7fz-nph7j-7cuya-3gyxr-cqe")
            .unwrap(),
        Principal::from_text("km7qz-4bai4-e5ptx-hgrck-z3web-ameqg-ksxcf-u7wbr-t5fna-i7bqp-hqe")
            .unwrap(),
        Principal::from_text("uyxh5-bi3za-gxbfs-op3gj-ere73-a6jhv-5jky3-zawef-b5r2s-k26un-sae")
            .unwrap(),
        Principal::from_text("w3kjy-pitqg-dvab7-tb57q-63gnd-di4vo-loiiy-s6zm2-gqcmw-ixliz-aae").unwrap(),
    ];
    static ref TABLE_CANISTER_WASM: &'static [u8] =
        include_bytes!("../../../target/wasm32-unknown-unknown/release/table_canister.wasm");
    static ref ENABLE_RAKE: bool = true;
    static ref CURRENCY_MANAGER: Mutex<CurrencyManager> = Mutex::new(CurrencyManager::new());
    static ref RAKE_WALLET_PRINCIPAL_ID: Principal =
        Principal::from_text(rake_constants::RAKE_WALLET_ADDRESS_PRINCIPAL).unwrap();
    static ref RAKE_WALLET_ACCOUNT_ID: String = rake_constants::RAKE_WALLET_ACCOUNT_ID.to_string();
    static ref TRANSACTION_STATE: Mutex<TransactionState> = Mutex::new(TransactionState::new());
    static ref TABLE_CANISTER_POOL: Mutex<Vec<Principal>> = Mutex::new(Vec::new());
    static ref POOL_SIZE_TARGET: usize = 5; // Keep 5 ready canisters
    static ref POOL_REFILL_THRESHOLD: usize = 2; // Refill when below 2
    static ref POOL_MAX_SIZE: usize = 10; // Maximum pool size
}

#[ic_cdk::init]
fn init() {
    let id = ic_cdk::api::canister_self();
    ic_cdk::println!("Table index canister {id} initialized");
}

#[ic_cdk::query]
fn ping() -> String {
    "Ok".to_string()
}

#[ic_cdk::query]
fn get_account_number() -> Result<Option<String>, TableIndexError> {
    let canister_state = get_canister_state();
    Ok(Some(canister_state.account_identifier.to_string()))
}

#[ic_cdk::query]
fn get_pool_status() -> (usize, usize, usize) {
    let pool_size = TABLE_CANISTER_POOL
        .lock()
        .map(|p| p.len())
        .unwrap_or(0);
    (*POOL_SIZE_TARGET, *POOL_REFILL_THRESHOLD, pool_size)
}

#[ic_cdk::update]
async fn maintain_canister_pool() -> Result<(), TableIndexError> {
    handle_cycle_check().await?;
    
    let pool_size = {
        TABLE_CANISTER_POOL
            .lock()
            .map_err(|_| TableIndexError::LockError)?
            .len()
    };

    if pool_size < *POOL_REFILL_THRESHOLD {
        ic_cdk::println!("Pool size {} is below threshold {}, refilling...", pool_size, *POOL_REFILL_THRESHOLD);
        
        let controllers = CONTROLLER_PRINCIPALS.clone();
        let wasm_module = TABLE_CANISTER_WASM.to_vec();
        
        // Calculate how many we need to create
        let needed = (*POOL_SIZE_TARGET - pool_size).min(*POOL_MAX_SIZE - pool_size);
        
        if needed == 0 {
            return Ok(());
        }
        
        // Create multiple canisters in parallel
        let mut futures = Vec::new();
        
        for _ in 0..needed {
            let controllers_clone = controllers.clone();
            let wasm_clone = wasm_module.clone();
            futures.push(async move {
                match create_canister_wrapper(controllers_clone, None).await {
                    Ok(canister) => {
                        match install_wasm_code(canister, wasm_clone).await {
                            Ok(_) => {
                                ic_cdk::println!("Successfully pre-installed WASM in canister {}", canister);
                                Ok(canister)
                            }
                            Err(e) => {
                                ic_cdk::println!("Failed to install WASM in canister {}: {:?}", canister, e);
                                Err(TableIndexError::CanisterCallError(format!("Install failed: {:?}", e)))
                            }
                        }
                    }
                    Err(e) => {
                        ic_cdk::println!("Failed to create canister: {:?}", e);
                        Err(TableIndexError::CanisterCallError(format!("Create failed: {:?}", e)))
                    }
                }
            });
        }
        
        // Wait for all to complete
        let results = join_all(futures).await;
        
        let mut pool = TABLE_CANISTER_POOL
            .lock()
            .map_err(|_| TableIndexError::LockError)?;
            
        let mut success_count = 0;
        for result in results {
            match result {
                Ok(canister) => {
                    pool.push(canister);
                    success_count += 1;
                }
                Err(e) => {
                    ic_cdk::println!("Pool refill error: {:?}", e);
                }
            }
        }
        
        ic_cdk::println!("Pool refilled: {} new canisters added (target: {})", success_count, needed);
    }
    
    Ok(())
}

#[ic_cdk::update]
async fn create_table(
    config: TableConfig,
    wallet_principal_id: Option<WalletPrincipalId>,
) -> Result<PublicTable, TableIndexError> {
    handle_cycle_check().await?;
    
    // Get canister from pool OR create new (fallback)
    let table_canister_principal = {
        let mut pool = TABLE_CANISTER_POOL
            .lock()
            .map_err(|_| TableIndexError::LockError)?;
            
        if let Some(canister) = pool.pop() {
            ic_cdk::println!("Using pre-installed canister from pool: {}", canister);
            // Use pre-installed canister (FAST!)
            drop(pool); // Release lock before async operations
            canister
        } else {
            ic_cdk::println!("Pool empty, creating new canister...");
            // Fallback: create new (slow, but works)
            drop(pool); // Release lock before async operations
            let controllers = CONTROLLER_PRINCIPALS.clone();
            let wasm_module = TABLE_CANISTER_WASM.to_vec();
            let canister = create_canister_wrapper(controllers, None).await?;
            install_wasm_code(canister, wasm_module).await?;
            canister
        }
    };
    
    // Trigger pool refill in background (don't wait for it)
    let pool_refill_future = maintain_canister_pool();
    ic_cdk::futures::spawn(async {
        if let Err(e) = pool_refill_future.await {
            ic_cdk::println!("Background pool refill failed: {:?}", e);
        }
    });
    
    let table_canister_principal = TableId(table_canister_principal);
    let raw_bytes = ic_cdk::management_canister::raw_rand().await;
    let raw_bytes = raw_bytes.map_err(|e| {
        TableIndexError::CanisterCallError(format!("Failed to generate random bytes: {:?}", e))
    })?;

    let config = if *ENABLE_RAKE {
        TableConfig {
            enable_rake: Some(true),
            table_type: Some(TableType::Cash),
            ..config
        }
    } else {
        TableConfig {
            enable_rake: Some(false),
            table_type: Some(TableType::Cash),
            ..config
        }
    };

    ic_cdk::println!("Table config: {:?}", config);

    if config.is_shared_rake.is_some() {
        let manager = CURRENCY_MANAGER
            .lock()
            .map_err(|_| TableIndexError::LockError)?
            .clone();
        let mut transaction_state = {
            TRANSACTION_STATE
                .lock()
                .map_err(|_| TableIndexError::LockError)?
                .clone()
        };
        if config.currency_type == CurrencyType::Real(Currency::BTC) {
            manager
                .deposit(
                    &mut transaction_state,
                    &currency::Currency::BTC,
                    wallet_principal_id
                        .ok_or(TableIndexError::InvalidRequest(
                            "Wallet principal id is required".to_string(),
                        ))?
                        .0,
                    50000,
                )
                .await?;
            manager
                .withdraw(&currency::Currency::BTC, *RAKE_WALLET_PRINCIPAL_ID, 50000)
                .await?;
        } else {
            manager
                .deposit(
                    &mut transaction_state,
                    &currency::Currency::ICP,
                    wallet_principal_id
                        .ok_or(TableIndexError::InvalidRequest(
                            "Wallet principal id is required".to_string(),
                        ))?
                        .0,
                    1e8 as u64,
                )
                .await?;
            manager
                .withdraw(
                    &currency::Currency::ICP,
                    *RAKE_WALLET_PRINCIPAL_ID,
                    1e8 as u64,
                )
                .await?;
        }
        *TRANSACTION_STATE
            .lock()
            .map_err(|_| TableIndexError::LockError)? = transaction_state;
    }

    let table = create_table_wrapper(table_canister_principal, config.clone(), raw_bytes)
        .await
        .map_err(|e| {
            TableIndexError::CanisterCallError(format!("Failed to create table wrapper: {:?}", e))
        })?;

    if config.is_private.unwrap_or(false) {
        PRIVATE_TABLE_INDEX_STATE
            .lock()
            .map_err(|_| TableIndexError::LockError)?
            .tables
            .insert(table.id, config);
    } else {
        PUBLIC_TABLE_INDEX_STATE
            .lock()
            .map_err(|_| TableIndexError::LockError)?
            .tables
            .insert(table.id, config);
    }
    Ok(table)
}

#[ic_cdk::update]
async fn update_table_player_count(table_id: TableId, count: usize) -> Result<(), TableIndexError> {
    handle_cycle_check().await?;

    TABLE_PLAYER_COUNTS
        .lock()
        .map_err(|_| TableIndexError::LockError)?
        .insert(table_id, count);
    Ok(())
}

#[ic_cdk::update]
async fn get_tables(
    filter_options: Option<FilterOptions>,
    page_number: u16,
    page_size: u16,
) -> Result<Vec<(TableId, TableConfig)>, TableIndexError> {
    handle_cycle_check().await?;

    // Get initial table states
    let mut tables: Vec<(TableId, TableConfig)> = PUBLIC_TABLE_INDEX_STATE
        .lock()
        .map_err(|_| TableIndexError::LockError)?
        .tables
        .iter()
        .map(|(id, table_config)| (*id, table_config.clone()))
        .collect();

    // Apply additional filtering if provided
    if let Some(filter_options) = filter_options {
        tables = filter_options.filter_tables(tables);
    }

    // Get player counts from cache
    let player_counts = TABLE_PLAYER_COUNTS
        .lock()
        .map_err(|_| TableIndexError::LockError)?;

    // Sort by player count
    tables.sort_by(|a, b| {
        let count_a = player_counts.get(&a.0).unwrap_or(&0);
        let count_b = player_counts.get(&b.0).unwrap_or(&0);
        count_b.cmp(count_a) // Sort in descending order
    });

    // Apply pagination
    let start = page_number as usize * page_size as usize;
    let end = tables.len().min(start + page_size as usize);

    Ok(tables[start..end].to_vec())
}

#[ic_cdk::query]
async fn get_all_public_tables() -> Result<Vec<(TableId, TableConfig)>, TableIndexError> {
    let public_table_index_state = PUBLIC_TABLE_INDEX_STATE
        .lock()
        .map_err(|_| TableIndexError::LockError)?
        .tables
        .clone();
    Ok(public_table_index_state.into_iter().collect())
}

#[ic_cdk::query]
async fn get_all_table_principals() -> Result<Vec<TableId>, TableIndexError> {
    let public_table_index_state = PUBLIC_TABLE_INDEX_STATE
        .lock()
        .map_err(|_| TableIndexError::LockError)?
        .tables
        .clone();
    let private_table_index_state = PRIVATE_TABLE_INDEX_STATE
        .lock()
        .map_err(|_| TableIndexError::LockError)?
        .tables
        .clone();
    let all_tables = public_table_index_state
        .iter()
        .chain(private_table_index_state.iter())
        .map(|(id, _table_config)| *id)
        .collect();

    Ok(all_tables)
}

#[ic_cdk::query]
fn get_private_tables() -> Result<Vec<TableId>, TableIndexError> {
    let private_table_index_state = PRIVATE_TABLE_INDEX_STATE
        .lock()
        .map_err(|_| TableIndexError::LockError)?
        .tables
        .clone();
    Ok(private_table_index_state.keys().copied().collect())
}

#[ic_cdk::update]
async fn remove_table_from_indexes(table_principal: TableId) -> Result<(), TableIndexError> {
    handle_cycle_check().await?;
    let controllers = (*CONTROLLER_PRINCIPALS).clone();
    validate_caller(controllers);
    {
        PUBLIC_TABLE_INDEX_STATE
            .lock()
            .map_err(|_| TableIndexError::LockError)?
            .tables
            .remove(&table_principal);
        PRIVATE_TABLE_INDEX_STATE
            .lock()
            .map_err(|_| TableIndexError::LockError)?
            .tables
            .remove(&table_principal);
    }
    Ok(())
}

#[ic_cdk::update]
async fn purge_dud_tables() -> Result<(), TableIndexError> {
    handle_cycle_check().await?;
    let public_index_tables = {
        PUBLIC_TABLE_INDEX_STATE
            .lock()
            .map_err(|_| TableIndexError::LockError)?
            .clone()
            .tables
    };
    {
        let mut principals_to_delete = Vec::new();

        for (principal, table) in public_index_tables {
            if table.seats == 0 || principal == Principal::anonymous() {
                principals_to_delete.push(principal);
            } else {
                match get_table_wrapper(principal).await {
                    Ok(_) => continue,                              // Table exists, no action needed
                    Err(_) => principals_to_delete.push(principal), // Table does not exist, mark for deletion
                }
            }
        }
        for (principal, table) in PRIVATE_TABLE_INDEX_STATE
            .lock()
            .map_err(|_| TableError::LockError)?
            .tables
            .clone()
        {
            if table.seats == 0 || principal == Principal::anonymous() {
                principals_to_delete.push(principal);
            }
        }
        for principal in principals_to_delete {
            PUBLIC_TABLE_INDEX_STATE
                .lock()
                .map_err(|_| TableError::LockError)?
                .tables
                .remove(&principal);
            PRIVATE_TABLE_INDEX_STATE
                .lock()
                .map_err(|_| TableError::LockError)?
                .tables
                .remove(&principal);
        }
    }
    Ok(())
}

#[ic_cdk::update]
async fn delete_table_by_id(table_principal: TableId) -> Result<(), TableIndexError> {
    handle_cycle_check().await?;
    let controllers = (*CONTROLLER_PRINCIPALS).clone();
    validate_caller(controllers);
    delete_table(table_principal).await
}

#[ic_cdk::update]
async fn top_up_table_canister(
    canister_id: Principal,
    amount: u128,
) -> Result<(), TableIndexError> {
    handle_cycle_check().await?;
    top_up_canister(canister_id, amount).await?;
    Ok(())
}

#[ic_cdk::update]
async fn monitor_and_top_up_table_canisters() -> Result<(), TableIndexError> {
    handle_cycle_check().await?;
    let public_table_index_state = PUBLIC_TABLE_INDEX_STATE
        .lock()
        .map_err(|_| TableIndexError::LockError)?
        .tables
        .clone();
    let private_table_index_state = PRIVATE_TABLE_INDEX_STATE
        .lock()
        .map_err(|_| TableIndexError::LockError)?
        .tables
        .clone();

    let all_tables: Vec<Principal> = public_table_index_state
        .iter()
        .chain(private_table_index_state.iter())
        .map(|(id, _table_config)| id.0)
        .collect();

    monitor_and_top_up_canisters(all_tables).await?;
    Ok(())
}

async fn delete_table(table_principal: TableId) -> Result<(), TableIndexError> {
    handle_cycle_check().await?;
    {
        PUBLIC_TABLE_INDEX_STATE
            .lock()
            .map_err(|_| TableIndexError::LockError)?
            .tables
            .remove(&table_principal);
        PRIVATE_TABLE_INDEX_STATE
            .lock()
            .map_err(|_| TableIndexError::LockError)?
            .tables
            .remove(&table_principal);
    }
    let res = is_game_ongoing_wrapper(table_principal).await?;
    if res {
        return Err(TableIndexError::InvalidRequest(
            "Cannot delete table with ongoing game".to_string(),
        ));
    } else {
        return_all_cycles_to_index(table_principal).await?;
        stop_and_delete_canister(table_principal.0).await?;
    }
    Ok(())
}

#[ic_cdk::update]
async fn delete_all_tables() -> Result<Vec<Result<(), TableIndexError>>, TableIndexError> {
    handle_cycle_check().await?;
    let controllers = (*CONTROLLER_PRINCIPALS).clone();
    validate_caller(controllers);

    // Get all table principals
    let public_tables = PUBLIC_TABLE_INDEX_STATE
        .lock()
        .map_err(|_| TableIndexError::LockError)?
        .tables
        .keys()
        .copied()
        .collect::<Vec<TableId>>();

    let private_tables = PRIVATE_TABLE_INDEX_STATE
        .lock()
        .map_err(|_| TableIndexError::LockError)?
        .tables
        .keys()
        .copied()
        .collect::<Vec<TableId>>();

    // Combine all tables
    let all_tables: Vec<TableId> = public_tables
        .into_iter()
        .chain(private_tables.into_iter())
        .collect();

    // Create futures for all delete operations
    let delete_futures: Vec<_> = all_tables
        .into_iter()
        .map(|table_principal| async move {
            // First check if game is ongoing
            let is_game_ongoing = is_game_ongoing_wrapper(table_principal).await?;

            if is_game_ongoing {
                return Err(TableIndexError::InvalidRequest(format!(
                    "Cannot delete table {} with ongoing game",
                    table_principal.0.to_text()
                )));
            }

            clear_table(table_principal).await?;

            // Return cycles to index
            return_all_cycles_to_index(table_principal).await?;

            // Delete the canister
            stop_and_delete_canister(table_principal.0).await?;

            // Remove from indexes
            {
                PUBLIC_TABLE_INDEX_STATE
                    .lock()
                    .map_err(|_| TableIndexError::LockError)?
                    .tables
                    .remove(&table_principal);

                PRIVATE_TABLE_INDEX_STATE
                    .lock()
                    .map_err(|_| TableIndexError::LockError)?
                    .tables
                    .remove(&table_principal);
            }

            Ok(())
        })
        .collect();

    // Execute all delete operations in parallel
    let results = join_all(delete_futures).await;

    Ok(results)
}

const CYCLES_TOP_UP_AMOUNT: u128 = 750_000_000_000;

#[ic_cdk::update]
async fn request_cycles() -> Result<(), CanisterManagementError> {
    let cycles = ic_cdk::api::canister_cycle_balance();
    let caller = TableId(ic_cdk::api::msg_caller());
    if cycles < CYCLES_TOP_UP_AMOUNT {
        return Err(CanisterManagementError::InsufficientCycles);
    }

    transfer_cycles(CYCLES_TOP_UP_AMOUNT, caller).await
}

async fn transfer_cycles(
    cycles_amount: u128,
    caller: TableId,
) -> Result<(), CanisterManagementError> {
    let public_table_index_state = PUBLIC_TABLE_INDEX_STATE
        .lock()
        .map_err(|_| CanisterManagementError::LockError)?
        .tables
        .clone();
    let private_table_index_state = PRIVATE_TABLE_INDEX_STATE
        .lock()
        .map_err(|_| CanisterManagementError::LockError)?
        .tables
        .clone();

    if !public_table_index_state.contains_key(&caller)
        && !private_table_index_state.contains_key(&caller)
    {
        return Err(CanisterManagementError::Transfer(format!(
            "Caller is not a valid destination: {}",
            caller.0.to_text()
        )));
    }

    top_up_canister(caller.0, cycles_amount).await?;
    Ok(())
}

#[ic_cdk::query]
async fn get_rake(
    small_blind: SmallBlind,
    currency: Currency,
    game_type: GameType,
) -> Option<Rake> {
    match Rake::new(small_blind, &game_type, &currency) {
        Ok(rake) => Some(rake),
        Err(e) => {
            ic_cdk::println!("Failed to get rake: {:?}", e);
            None
        }
    }
}

const BATCH_SIZE: usize = 10; // Adjust based on your needs

#[ic_cdk::update]
async fn get_all_rake_stats() -> Result<GlobalRakeStats, TableIndexError> {
    handle_cycle_check().await?;

    // Get all table principals
    let all_tables = get_all_table_principals().await?;
    let mut global_stats = GlobalRakeStats {
        total_rake_collected: 0,
        total_rake_shared: 0,
        table_stats: Vec::new(),
    };

    // Process tables in batches
    for tables_chunk in all_tables.chunks(BATCH_SIZE) {
        let stats_futures: Vec<_> = tables_chunk
            .iter()
            .map(|&table_id| async move {
                let stats = get_rake_stats(table_id).await?;

                Ok::<(TableId, RakeStats), TableIndexError>((table_id, stats))
            })
            .collect();

        // Process batch
        let batch_results = join_all(stats_futures).await;

        // Aggregate results from this batch
        for result in batch_results {
            match result {
                Ok((table_id, stats)) => {
                    global_stats.total_rake_collected += stats.total_rake_collected;
                    global_stats.total_rake_shared += stats.total_rake_shared;
                    global_stats.table_stats.push(TableRakeStats {
                        table_id,
                        total_rake_collected: stats.total_rake_collected,
                        total_rake_shared: stats.total_rake_shared,
                    });
                }
                Err(e) => {
                    ic_cdk::println!("Error getting rake stats: {:?}", e);
                    continue;
                }
            }
        }
    }

    Ok(global_stats)
}

#[ic_cdk::query]
fn get_rake_wallet_info() -> (Principal, String) {
    (*RAKE_WALLET_PRINCIPAL_ID, (*RAKE_WALLET_ACCOUNT_ID).clone())
}

#[ic_cdk::update]
async fn quick_join_table(
    user_principal: UsersCanisterId,
    wallet_principal_id: WalletPrincipalId,
    amount: u64,
    currency: CurrencyType,
) -> Result<PublicTable, TableIndexError> {
    handle_cycle_check().await?;

    // Get all public tables
    let public_tables = {
        PUBLIC_TABLE_INDEX_STATE
            .lock()
            .map_err(|_| TableIndexError::LockError)?
            .tables
            .clone()
    };

    // Get player counts for sorting
    let player_counts = TABLE_PLAYER_COUNTS
        .lock()
        .map_err(|_| TableIndexError::LockError)?
        .clone();

    // Filter tables by currency and not full
    let mut available_tables: Vec<(TableId, usize)> = Vec::new();

    for (id, table_config) in public_tables.iter() {
        // Skip tables that don't match the currency
        if table_config.currency_type != currency {
            continue;
        }

        // Skip paused tables
        if table_config.is_paused.unwrap_or(false) {
            continue;
        }

        // Check if table is full
        let is_full = match is_table_full(table_config, id).await {
            Ok(is_full) => is_full,
            Err(_) => continue, // Skip tables we can't check
        };

        if !is_full {
            // Get player count (default to 0 if not found)
            let count = player_counts.get(id).cloned().unwrap_or(0);
            available_tables.push((*id, count));
        }
    }

    if available_tables.is_empty() {
        return Err(TableIndexError::NoAvailableTables(format!(
            "No available tables for currency: {:?}",
            currency
        )));
    }

    // Sort tables by player count, prioritizing tables with players but not full
    available_tables.sort_by(|a, b| {
        let count_a = a.1;
        let count_b = b.1;

        // First prioritize tables with players (but not too many)
        match (count_a, count_b) {
            // If both tables have a good number of players (1-3), compare them directly
            (1..=3, 1..=3) => count_b.cmp(&count_a),

            // Prioritize tables with 1-3 players
            (1..=3, _) => Ordering::Less,
            (_, 1..=3) => Ordering::Greater,

            // If neither has 1-3 players, prioritize any table with players
            (0, 0) => Ordering::Equal,
            (0, _) => Ordering::Greater,
            (_, 0) => Ordering::Less,

            // Default to more players over fewer
            _ => count_b.cmp(&count_a),
        }
    });

    // Join the best available table
    let table_to_join = available_tables[0].0;

    let table = join_table(
        table_to_join,
        user_principal,
        wallet_principal_id,
        None,
        amount,
        false,
    )
    .await?;

    Ok(table)
}

#[ic_cdk::update]
async fn upgrade_all_table_canisters(
) -> Result<Vec<(TableId, CanisterManagementError)>, TableIndexError> {
    // Validate caller permissions
    let caller = ic_cdk::api::msg_caller();
    if !CONTROLLER_PRINCIPALS.contains(&caller) {
        return Err(TableIndexError::InvalidRequest(
            "Unauthorized: caller is not a controller".to_string(),
        ));
    }

    handle_cycle_check().await?;

    const BATCH_SIZE: usize = 30; // Process 30 tables at a time

    let tables: Vec<TableId> = {
        let public_tables = PUBLIC_TABLE_INDEX_STATE
            .lock()
            .map_err(|_| TableIndexError::LockError)?
            .tables
            .keys()
            .copied()
            .collect::<Vec<TableId>>();

        let private_tables = PRIVATE_TABLE_INDEX_STATE
            .lock()
            .map_err(|_| TableIndexError::LockError)?
            .tables
            .keys()
            .copied()
            .collect::<Vec<TableId>>();

        public_tables
            .into_iter()
            .chain(private_tables.into_iter())
            .collect()
    };

    let wasm_module = TABLE_CANISTER_WASM.to_vec();
    let mut failed_upgrades = Vec::new();

    // Process tables in batches
    for chunk in tables.chunks(BATCH_SIZE) {
        let futures: Vec<_> = chunk
            .iter()
            .map(|&table_canister| {
                let wasm_clone = wasm_module.clone();
                async move {
                    match canister_functions::upgrade_wasm_code(table_canister.0, wasm_clone).await
                    {
                        Ok(_) => {
                            ic_cdk::println!(
                                "Successfully upgraded table canister {}",
                                table_canister.0.to_text()
                            );
                            Ok(table_canister)
                        }
                        Err(e) => {
                            ic_cdk::println!(
                                "Failed to upgrade table canister {}: {:?}",
                                table_canister.0.to_text(),
                                e
                            );
                            Err((table_canister, e))
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
async fn upgrade_table_canister(table_principal: Principal) -> Result<(), TableIndexError> {
    // Validate caller permissions
    let caller = ic_cdk::api::msg_caller();
    if !CONTROLLER_PRINCIPALS.contains(&caller) {
        return Err(TableIndexError::InvalidRequest(
            "Unauthorized: caller is not a controller".to_string(),
        ));
    }

    handle_cycle_check().await?;

    let wasm_module = TABLE_CANISTER_WASM.to_vec();
    canister_functions::upgrade_wasm_code(table_principal, wasm_module).await?;
    Ok(())
}

#[ic_cdk::update]
async fn withdraw_rake(rake_amount: u64) -> Result<(), TableError> {
    let currency_manager = {
        let currency_manager = CURRENCY_MANAGER.lock().map_err(|_| TableError::LockError)?;
        currency_manager.clone()
    };

    currency_manager
        .withdraw_rake(&Currency::ICP, *RAKE_WALLET_PRINCIPAL_ID, rake_amount)
        .await?;

    Ok(())
}

#[ic_cdk::update]
async fn get_canister_status_formatted() -> Result<String, TableIndexError> {
    // Validate caller is a controller
    let controllers = (*CONTROLLER_PRINCIPALS).clone();
    validate_caller(controllers);

    handle_cycle_check().await?;

    // Call the management canister to get status
    let canister_status_arg = CanisterStatusArgs {
        canister_id: ic_cdk::api::canister_self(),
    };

    let status_response = canister_status(&canister_status_arg).await.map_err(|e| {
        TableIndexError::CanisterCallError(format!("Failed to get canister status: {:?}", e))
    })?;

    // Format the status into a readable string
    let formatted_status = format!(
        "📊 Canister Status Report
════════════════════════════════════════════════════════════════
🆔 Canister ID: {}
🔄 Status: {:?}
💾 Memory Size: {} bytes ({:.2} MB)
⚡ Cycles: {} ({} B cycles)
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
        status_response.cycles.clone() / Nat::from(1_000_000_000_u64), // Convert to T cycles
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
async fn get_table_player_counts() -> Result<Vec<String>, TableIndexError> {
    handle_cycle_check().await?;
    let player_counts = TABLE_PLAYER_COUNTS
        .lock()
        .map_err(|_| TableIndexError::LockError)?;
    let player_counts = player_counts.clone();
    let mut tables = Vec::new();
    for (table_id, count) in player_counts.iter() {
        let table = get_table_wrapper(*table_id).await.map_err(|e| {
            TableIndexError::CanisterCallError(format!(
                "Failed to get table wrapper for {}: {:?}",
                table_id.0.to_text(),
                e
            ))
        })?;
        tables.push(format!(
            "Table ID: {}, Count: {}, Actual Players: {}, Config: {:?}",
            table_id.0.to_text(),
            count,
            table.users.len(),
            table.config
        ));
    }
    Ok(tables)
}

#[ic_cdk::update]
async fn update_table_player_counts() -> Result<(), TableIndexError> {
    validate_caller(CONTROLLER_PRINCIPALS.clone());
    handle_cycle_check().await?;
    // Get all table principals
    let public_tables = PUBLIC_TABLE_INDEX_STATE
        .lock()
        .map_err(|_| TableIndexError::LockError)?
        .tables
        .keys()
        .copied()
        .collect::<Vec<TableId>>();

    let private_tables = PRIVATE_TABLE_INDEX_STATE
        .lock()
        .map_err(|_| TableIndexError::LockError)?
        .tables
        .keys()
        .copied()
        .collect::<Vec<TableId>>();

    // Combine all tables
    let all_tables: Vec<TableId> = public_tables
        .into_iter()
        .chain(private_tables.into_iter())
        .collect();

    let mut player_counts = TABLE_PLAYER_COUNTS
        .lock()
        .map_err(|_| TableIndexError::LockError)?;

    for table_id in all_tables {
        let table = get_table_wrapper(table_id).await.map_err(|e| {
            TableIndexError::CanisterCallError(format!(
                "Failed to get table wrapper for {}: {:?}",
                table_id.0.to_text(),
                e
            ))
        })?;

        // Update player count
        player_counts.insert(table_id, table.users.len());
    }

    Ok(())
}

ic_cdk::export_candid!();
