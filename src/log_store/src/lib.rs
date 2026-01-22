use std::cell::RefCell;

use authentication::validate_caller;
use candid::{Nat, Principal};
use errors::log_store_error::LogStoreError;
use ic_cdk::management_canister::{canister_status, CanisterStatusArgs};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableBTreeMap,
};
use serde_cbor::{from_slice, to_vec};
use table::poker::game::table_functions::action_log::ActionLog;

type Memory = VirtualMemory<DefaultMemoryImpl>;

thread_local! {
    // The memory manager is used for simulating multiple memories. Given a `MemoryId` it can
    // return a memory that can be used by stable structures.
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));

    static MAP: RefCell<StableBTreeMap<Principal, Vec<u8>, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(0))),
        )
    );

    static CONTROLLER_PRINCIPALS: Vec<Principal> = vec![
        Principal::from_text("py2cj-ei3dt-3ber7-nvxdl-56xvh-qkhop-7x7fz-nph7j-7cuya-3gyxr-cqe").unwrap(),
        Principal::from_text("uyxh5-bi3za-gxbfs-op3gj-ere73-a6jhv-5jky3-zawef-b5r2s-k26un-sae").unwrap(),
        Principal::from_text("w3kjy-pitqg-dvab7-tb57q-63gnd-di4vo-loiiy-s6zm2-gqcmw-ixliz-aae").unwrap(),
        Principal::from_text("tcuxo-b5b4t-vxwo7-mwgxi-vb4ig-zuux4-jmvru-fjocv-4uxuz-7yo4v-hqe").unwrap(),
        Principal::from_text("2hbym-ivof6-l2yyh-zgu62-fqgf3-nvtlj-edso2-ebahf-p5j5c-2p2eb-lae").unwrap(),
    ];
}

fn serialize_with_length_prefix(action_log: &ActionLog) -> Result<Vec<u8>, LogStoreError> {
    let serialized_data =
        to_vec(action_log).map_err(|e| LogStoreError::SerializationError(e.to_string()))?;
    let length = serialized_data.len() as u32; // Ensure the length fits within a u32
    let mut length_prefix = length.to_be_bytes().to_vec(); // Big-endian byte order for consistency

    length_prefix.extend(serialized_data);
    Ok(length_prefix)
}

fn deserialize_from_bytes(
    data: &[u8],
    start_timestamp: Option<u64>,
    end_timestamp: Option<u64>,
) -> Result<Vec<ActionLog>, LogStoreError> {
    let mut index = 0;
    let mut logs = Vec::new();

    while index < data.len() {
        let bytes = data[index..index + 4].try_into().map_err(|_| {
            LogStoreError::DeserializationError("Invalid length prefix".to_string())
        })?;
        let length = u32::from_be_bytes(bytes) as usize;
        index += 4; // Move past the length prefix
        let log = from_slice::<ActionLog>(&data[index..index + length]).map_err(|_| {
            LogStoreError::DeserializationError("Failed to deserialize ActionLog".to_string())
        })?;
        logs.push(log.clone());
        if let Some(start_timestamp) = start_timestamp {
            if log.timestamp < start_timestamp {
                // Skip logs before start_timestamp
                continue;
            }
        }
        if let Some(end_timestamp) = end_timestamp {
            if log.timestamp > end_timestamp {
                // Since logs are in chronological order, we can stop here
                break;
            }
        }
        index += length; // Move to the next log entry
    }

    Ok(logs)
}

#[ic_cdk::init]
fn init() {
    let principal = ic_cdk::api::canister_self();
    ic_cdk::println!("Log store canister {} initialized", principal.to_text());
}

#[ic_cdk::update]
fn log_action(table_principal: Principal, action_log: ActionLog) -> Result<(), LogStoreError> {
    MAP.with(|p| {
        let mut map = p.borrow_mut();
        let mut logs = map.get(&table_principal).unwrap_or_default();
        let action_log_bytes = serialize_with_length_prefix(&action_log)?;

        logs.extend(action_log_bytes);
        map.insert(table_principal, logs);
        Ok(())
    })
}

#[ic_cdk::update]
fn log_actions(
    table_principal: Principal,
    action_logs: Vec<ActionLog>,
) -> Result<(), LogStoreError> {
    MAP.with(|p| {
        let mut map = p.borrow_mut();
        let mut logs = map.get(&table_principal).unwrap_or_default();

        for action_log in action_logs {
            let action_log_bytes = serialize_with_length_prefix(&action_log)?;
            logs.extend(action_log_bytes);
        }

        map.insert(table_principal, logs);
        Ok(())
    })
}

#[ic_cdk::update]
fn clear_logs(table_principal: Principal) -> Result<(), LogStoreError> {
    MAP.with(|p| {
        let mut map = p.borrow_mut();
        map.insert(table_principal, Vec::new());
        Ok(())
    })
}

#[ic_cdk::update]
fn clear_all_logs() -> Result<(), LogStoreError> {
    MAP.with(|p| {
        let mut map = p.borrow_mut();
        map.clear_new();
        Ok(())
    })
}

#[ic_cdk::update]
fn clear_logs_before(table_principal: Principal, timestamp: u64) -> Result<(), LogStoreError> {
    MAP.with(|p| {
        let mut map = p.borrow_mut();
        let logs = map.get(&table_principal).unwrap_or_default();
        let mut index = 0;
        let mut new_logs = Vec::new();

        while index < logs.len() {
            let bytes = logs[index..index + 4].try_into().map_err(|_| {
                LogStoreError::DeserializationError("Invalid length prefix".to_string())
            })?;
            let length = u32::from_be_bytes(bytes) as usize;
            index += 4; // Move past the length prefix
            let log = from_slice::<ActionLog>(&logs[index..index + length]).map_err(|_| {
                LogStoreError::DeserializationError("Failed to deserialize ActionLog".to_string())
            })?;
            if log.timestamp >= timestamp {
                let action_log_bytes = serialize_with_length_prefix(&log)?;
                new_logs.extend(action_log_bytes);
            } else {
                break; // Logs are in chronological order, so we can stop here
            }
            index += length; // Move to the next log entry
        }

        map.insert(table_principal, new_logs);
        Ok(())
    })
}

#[ic_cdk::query]
fn get_action_logs(
    table_principal: Principal,
    start_timestamp: u64,
    end_timestamp: u64,
    offset: Option<u32>,
    limit: Option<u32>,
) -> Result<Vec<ActionLog>, LogStoreError> {
    MAP.with(|p| {
        let map = p.borrow();
        let logs = map.get(&table_principal).unwrap_or_default();
        let result_logs =
            deserialize_from_bytes(&logs, Some(start_timestamp), Some(end_timestamp))?;
        // Apply offset and limit
        let offset = offset.unwrap_or(0) as usize;
        let limit = limit.unwrap_or(result_logs.len() as u32) as usize;

        let paginated_logs = result_logs.into_iter().skip(offset).take(limit).collect();

        Ok(paginated_logs)
    })
}

#[ic_cdk::update]
async fn get_canister_status_formatted() -> Result<String, LogStoreError> {
    // Validate caller is a controller
    let controllers = CONTROLLER_PRINCIPALS.with(|c| c.clone());
    validate_caller(controllers);

    // Call the management canister to get status
    let canister_status_arg = CanisterStatusArgs {
        canister_id: ic_cdk::api::canister_self(),
    };

    let status_response = canister_status(&canister_status_arg).await.map_err(|e| {
        LogStoreError::CanisterCallError(format!("Failed to get canister status: {:?}", e))
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
