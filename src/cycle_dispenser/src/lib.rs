use authentication::validate_caller;
use candid::{Nat, Principal};
use canister_functions::cycle::top_up_canister;
use errors::canister_management_error::CanisterManagementError;
use ic_cdk::management_canister::{canister_status, CanisterStatusArgs};
use lazy_static::lazy_static;

pub type PlayerId = u64;
pub type TableId = u64;

const CYCLES_TOP_UP_AMOUNT: u128 = 30_000_000_000_000;

lazy_static! {
    static ref VALID_CALLERS: Vec<Principal> = vec![
        *TOURNAMENT_INDEX_PROD,
        *TABLE_INDEX_PROD,
        *USERS_INDEX_PROD,
        *TOURNAMENT_INDEX_TEST,
        *TABLE_INDEX_TEST,
        *USERS_INDEX_TEST,
        *TABLE_INDEX_DEV,
        *TOURNAMENT_INDEX_DEV,
        *USERS_INDEX_DEV,
        *CLANS_INDEX_PROD,
        *CLANS_INDEX_TEST,
        *CLANS_INDEX_DEV,
    ];
    static ref TOURNAMENT_INDEX_PROD: Principal =
        Principal::from_text("zocwf-5qaaa-aaaam-qdfaq-cai").unwrap();
    static ref TABLE_INDEX_PROD: Principal =
        Principal::from_text("zbspl-ziaaa-aaaam-qbe2q-cai").unwrap();
    static ref USERS_INDEX_PROD: Principal =
        Principal::from_text("lvq5c-nyaaa-aaaam-qdswa-cai").unwrap();
    static ref TOURNAMENT_INDEX_TEST: Principal =
        Principal::from_text("u2qna-fiaaa-aaaag-at3ea-cai").unwrap();
    static ref TABLE_INDEX_TEST: Principal =
        Principal::from_text("e4yx7-lqaaa-aaaah-qdslq-cai").unwrap();
    static ref USERS_INDEX_TEST: Principal =
        Principal::from_text("m3tym-daaaa-aaaah-qqbsq-cai").unwrap();
    static ref TABLE_INDEX_DEV: Principal =
        Principal::from_text("tqzl2-p7777-77776-aaaaa-cai").unwrap();
    static ref TOURNAMENT_INDEX_DEV: Principal =
        Principal::from_text("t63gs-up777-77776-aaaba-cai").unwrap();
    static ref USERS_INDEX_DEV: Principal =
        Principal::from_text("txyno-ch777-77776-aaaaq-cai").unwrap();
    static ref CLANS_INDEX_PROD: Principal =
        Principal::from_text("xh2e2-uiaaa-aaaam-qdxiq-cai").unwrap();
    static ref CLANS_INDEX_TEST: Principal =
        Principal::from_text("434tn-2qaaa-aaaah-qqc7q-cai").unwrap();
    static ref CLANS_INDEX_DEV: Principal =
        Principal::from_text("tm5rl-y7777-77776-aaaca-cai").unwrap();

    static ref CONTROLLER_PRINCIPALS: Vec<Principal> = vec![
        Principal::from_text("py2cj-ei3dt-3ber7-nvxdl-56xvh-qkhop-7x7fz-nph7j-7cuya-3gyxr-cqe")
            .unwrap(),
        Principal::from_text("uyxh5-bi3za-gxbfs-op3gj-ere73-a6jhv-5jky3-zawef-b5r2s-k26un-sae")
            .unwrap(),
        Principal::from_text("km7qz-4bai4-e5ptx-hgrck-z3web-ameqg-ksxcf-u7wbr-t5fna-i7bqp-hqe")
            .unwrap(),
        Principal::from_text("w3kjy-pitqg-dvab7-tb57q-63gnd-di4vo-loiiy-s6zm2-gqcmw-ixliz-aae").unwrap(),
        Principal::from_text("tcuxo-b5b4t-vxwo7-mwgxi-vb4ig-zuux4-jmvru-fjocv-4uxuz-7yo4v-hqe").unwrap(),
        Principal::from_text("2hbym-ivof6-l2yyh-zgu62-fqgf3-nvtlj-edso2-ebahf-p5j5c-2p2eb-lae").unwrap(),
    ];
}

#[ic_cdk::init]
fn init() {
    let principal = ic_cdk::api::canister_self();
    ic_cdk::println!(
        "Cycle dispenser canister {} initialized",
        principal.to_text()
    );
}

#[ic_cdk::update]
async fn request_cycles() -> Result<(), CanisterManagementError> {
    let cycles = ic_cdk::api::canister_cycle_balance();
    let caller = ic_cdk::api::msg_caller();

    if cycles < CYCLES_TOP_UP_AMOUNT {
        ic_cdk::println!(
            "Not enough cycles in the canister. Current balance: {}",
            cycles
        );
        return Err(CanisterManagementError::InsufficientCycles);
    }

    transfer_cycles(CYCLES_TOP_UP_AMOUNT, caller).await
}

async fn transfer_cycles(
    cycles_amount: u128,
    caller: Principal,
) -> Result<(), CanisterManagementError> {
    let destination = if VALID_CALLERS.contains(&caller) {
        caller
    } else {
        return Err(CanisterManagementError::Transfer(format!(
            "Caller is not a valid destination: {}",
            caller
        )));
    };

    top_up_canister(destination, cycles_amount).await
}

#[ic_cdk::update]
async fn get_canister_status_formatted() -> Result<String, CanisterManagementError> {
    // Validate caller is a controller
    let controllers = (*CONTROLLER_PRINCIPALS).clone();
    validate_caller(controllers);

    // Call the management canister to get status
    let canister_status_arg = CanisterStatusArgs {
        canister_id: ic_cdk::api::canister_self(),
    };

    let status_response = canister_status(&canister_status_arg).await.map_err(|e| {
        CanisterManagementError::CanisterCallError(format!(
            "Failed to get canister status: {:?}",
            e
        ))
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
