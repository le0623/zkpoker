use candid::Principal;
use user::user::{User, UserBalance, UsersCanisterId};

use crate::poker::game::types::GameType;

use super::table::TableConfig;

pub mod action_logs;

pub mod all_in_tests_basic;

pub mod all_in_tests_side_pot;

pub mod ante_tests;

pub mod betting_order;

pub mod fixed_limit_tests;

pub mod general_tests;

pub mod no_limit_tests;

pub mod plo_tests;

pub mod pot_distribution_tests;

pub mod pot_limit;

pub mod rake;

pub mod spread_limit_tests;

pub mod turn_tests;

pub fn create_user(canister_id: Principal, balance: u64) -> User {
    User::new(
        user::user::WalletPrincipalId(canister_id),
        UsersCanisterId(Principal::anonymous()),
        format!("User{}", canister_id),
        UserBalance(balance),
        None,
        None,
        None,
        None,
        None,
    )
}

pub fn get_table_config(game_type: GameType, seats: u8) -> TableConfig {
    TableConfig::new(
        "Test Table".to_string(),
        game_type,
        seats,
        30,
        0,
        0,
        0,
        0,
        0,
        super::types::CurrencyType::Real(currency::Currency::ICP),
        Some(false),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
}
