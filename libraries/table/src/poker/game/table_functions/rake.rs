use candid::CandidType;
use currency::Currency;
use errors::{game_error::GameError, trace_err, traced_error::TracedError};
use serde::{Deserialize, Serialize};

use crate::poker::game::{table_functions::table::SmallBlind, types::GameType};

/// # Poker Rake System
///
/// This module implements a dynamic rake calculation system for poker games that supports
/// multiple currencies with different decimal places (ICP, ETH, USDC, BTC, etc.).
///
/// ## Rake Structure
/// The rake system is based on:
/// - A percentage of the pot (specified in millipercents, e.g., 4500 = 4.5%)
/// - Different caps based on number of players (2-3 players vs 4+ players)
/// - Stakes-based tiers that adjust rake percentages and caps
///
/// ## Stake Levels and Rake Configuration
/// Rake configurations are defined for different stake levels:
///
/// ### No Limit Games:
/// - Micro Stakes (ICP 0.01 - ICP 0.10): 4.5% rake
/// - Low Stakes (ICP 0.25 - ICP 0.50): 4.0% rake
/// - Mid Stakes (ICP 1 - ICP 2): 3.5% rake
/// - High Stakes (ICP 3 - ICP 4): 3.0% rake
/// - Higher Stakes (ICP 5+): 2.5% rake
///
/// ### Fixed Limit Games:
/// Similar structure but with slightly different ranges and caps.
///
/// Configuration for a specific rake tier.
/// All amounts are represented in 8 decimal places (ICP standard)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, CandidType)]
struct RakeConfig {
    pub min_small_blind: u64,
    pub max_small_blind: u64,
    pub percentage_millipercent: u64, // e.g., 4500 for 4.5%
    pub cap_2_3_players_min: u64,
    pub cap_2_3_players_max: u64,
    pub cap_4_plus_players_min: u64,
    pub cap_4_plus_players_max: u64,
}

/// Represents the active rake configuration for a table
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, CandidType)]
pub struct Rake {
    /// Rake percentage in millipercents (e.g., 4500 = 4.5%)
    pub percentage_millipercent: u64,
    /// Maximum rake for tables with 2-3 players
    pub cap_2_3_players: u64,
    /// Maximum rake for tables with 4 or more players
    pub cap_4_plus_players: u64,
}

impl Rake {
    /// Creates a new Rake configuration based on small blind amount and game type.
    ///
    /// # Arguments
    /// * `small_blind` - The small blind amount in the currency's native units
    /// * `game_type` - The type of poker game (NoLimit, PotLimit, etc.)
    /// * `currency` - The currency being used (affects decimal scaling)
    ///
    /// # Returns
    /// * `Result<Self, TracedError<GameError>>` - The rake configuration or an error
    pub fn new(
        small_blind: SmallBlind,
        game_type: &GameType,
        currency: &Currency,
    ) -> Result<Self, TracedError<GameError>> {
        let mut scaled_small_blind = if currency.decimals() > 8 {
            scale_amount(small_blind.0, currency.decimals(), 8)
        } else {
            // For currencies with less decimals, scale up carefully
            let scaled = scale_amount(small_blind.0, currency.decimals(), 8);
            if scaled == u64::MAX {
                return Err(trace_err!(TracedError::new(
                    GameError::CouldNotCalculateRake
                )));
            }
            scaled
        };

        if scaled_small_blind < 1_000_000 {
            scaled_small_blind = 1_000_000; // Minimum small blind is ICP 0.01
        }

        let rake = match game_type {
            GameType::NoLimit(_) | GameType::PotLimit(_) | GameType::PotLimitOmaha4(_) | GameType::PotLimitOmaha5(_) => get_no_limit_config(scaled_small_blind),
            GameType::FixedLimit(_, _) | GameType::SpreadLimit(_, _) => {
                get_fixed_limit_configs(scaled_small_blind)
            }
        };

        match rake {
            Some(rake) => {
                let small_blind_decimals = currency.decimals();
                let rake = Rake {
                    percentage_millipercent: rake.percentage_millipercent,
                    cap_2_3_players: scale_amount(rake.cap_2_3_players, 8, small_blind_decimals),
                    cap_4_plus_players: scale_amount(
                        rake.cap_4_plus_players,
                        8,
                        small_blind_decimals,
                    ),
                };

                Ok(rake)
            }
            None => Err(trace_err!(TracedError::new(
                GameError::CouldNotCalculateRake
            ))),
        }
    }

    /// Calculates the rake amount for a given pot and number of players
    ///
    /// # Arguments
    /// * `pot` - The total pot amount in currency units
    /// * `num_players` - Number of players in the hand
    ///
    /// # Returns
    /// * `u64` - The calculated rake amount, capped appropriately
    pub fn calculate_rake(&self, pot: u64, num_players: u8) -> u64 {
        // Calculate the raw rake using millipercent (divide by 100,000 to get the percentage)
        let raw_rake = (pot * self.percentage_millipercent) / 100_000;

        // Interpolate the cap based on the number of players
        if num_players <= 3 {
            raw_rake.min(self.cap_2_3_players)
        } else {
            raw_rake.min(self.cap_4_plus_players)
        }
    }
}

pub fn get_no_limit_config(small_blind: u64) -> Option<Rake> {
    // All amounts are in micro-units (1 ICP = 100_000_000 units)
    let no_limit_rake_configs = vec![
        // $0.0001/$0.0002 to $0.0099/$0.0198 - Micro Stakes (4.5%)
        RakeConfig {
            min_small_blind: 10_000,           // $0.0001
            max_small_blind: 999_999,          // $0.00999...
            percentage_millipercent: 4500,     // 4.5%
            cap_2_3_players_min: 500_000,      // $0.005
            cap_2_3_players_max: 2_000_000,    // $0.02
            cap_4_plus_players_min: 1_000_000, // $0.01
            cap_4_plus_players_max: 5_000_000, // $0.05
        },
        // $0.01/$0.02 to $0.10/$0.25 - Mini Stakes (4.5%)
        RakeConfig {
            min_small_blind: 1_000_000,         // $0.01
            max_small_blind: 24_999_999,        // $0.24
            percentage_millipercent: 4500,      // 4.5%
            cap_2_3_players_min: 5_000_000,     // $0.05
            cap_2_3_players_max: 20_000_000,    // $0.20
            cap_4_plus_players_min: 10_000_000, // $0.10
            cap_4_plus_players_max: 50_000_000, // $0.50
        },
        // $0.25/$0.50 to $0.50/$1 - Low Stakes (4.0%)
        RakeConfig {
            min_small_blind: 25_000_000,         // $0.25
            max_small_blind: 99_999_999,         // $0.99
            percentage_millipercent: 4000,       // 4.0%
            cap_2_3_players_min: 30_000_000,     // $0.30
            cap_2_3_players_max: 50_000_000,     // $0.50
            cap_4_plus_players_min: 75_000_000,  // $0.75
            cap_4_plus_players_max: 100_000_000, // $1.00
        },
        // $1/$2 to $2/$4 - Mid Stakes (3.5%)
        RakeConfig {
            min_small_blind: 100_000_000,        // $1.00
            max_small_blind: 299_999_999,        // $2.99
            percentage_millipercent: 3500,       // 3.5%
            cap_2_3_players_min: 75_000_000,     // $0.75
            cap_2_3_players_max: 100_000_000,    // $1.00
            cap_4_plus_players_min: 150_000_000, // $1.50
            cap_4_plus_players_max: 200_000_000, // $2.00
        },
        // $3/$6 to $4/$8 - High Stakes (3.0%)
        RakeConfig {
            min_small_blind: 300_000_000,        // $3.00
            max_small_blind: 499_999_999,        // $4.99
            percentage_millipercent: 3000,       // 3.0%
            cap_2_3_players_min: 150_000_000,    // $1.50
            cap_2_3_players_max: 200_000_000,    // $2.00
            cap_4_plus_players_min: 250_000_000, // $2.50
            cap_4_plus_players_max: 300_000_000, // $3.00
        },
        // $5/$10 and higher - Higher Stakes (2.5%)
        RakeConfig {
            min_small_blind: 500_000_000,           // $5.00
            max_small_blind: u64::MAX,              // Arbitrary high value
            percentage_millipercent: 2500,          // 2.5%
            cap_2_3_players_min: 250_000_000,       // $2.50
            cap_2_3_players_max: 15_000_000_000,    // $150.00 (adjust as needed)
            cap_4_plus_players_min: 400_000_000,    // $4.00
            cap_4_plus_players_max: 30_000_000_000, // $300.00 (adjust as needed)
        },
    ];

    no_limit_rake_configs
        .iter()
        .find(|cfg| small_blind >= cfg.min_small_blind && small_blind <= cfg.max_small_blind)
        .map(|cfg| Rake {
            percentage_millipercent: cfg.percentage_millipercent,
            cap_2_3_players: interpolate_u64(
                small_blind,
                cfg.min_small_blind,
                cfg.max_small_blind,
                cfg.cap_2_3_players_min,
                cfg.cap_2_3_players_max,
            ),
            cap_4_plus_players: interpolate_u64(
                small_blind,
                cfg.min_small_blind,
                cfg.max_small_blind,
                cfg.cap_4_plus_players_min,
                cfg.cap_4_plus_players_max,
            ),
        })
}

pub fn get_fixed_limit_configs(small_blind: u64) -> Option<Rake> {
    // All amounts are in micro-units (1 dollar = 100_000_000 units)
    let fixed_limit_rake_configs = vec![
        // Micro Stakes (4.5%)
        RakeConfig {
            min_small_blind: 10_000,            // $0.0001
            max_small_blind: 2_999_999,         // $0.0299
            percentage_millipercent: 4500,      // 4.5%
            cap_2_3_players_min: 50_000,        // 5x min small blind
            cap_2_3_players_max: 15_000_000,    // 5x max small blind
            cap_4_plus_players_min: 100_000,    // 10x min small blind
            cap_4_plus_players_max: 30_000_000, // 10x max small blind
        },
        // Mini Stakes (4.5%)
        RakeConfig {
            min_small_blind: 3_000_000,          // ICP 0.03
            max_small_blind: 24_999_999,         // ICP 0.24
            percentage_millipercent: 4500,       // 4.5%
            cap_2_3_players_min: 15_000_000,     // 5x min small blind
            cap_2_3_players_max: 125_000_000,    // 5x max small blind
            cap_4_plus_players_min: 30_000_000,  // 10x min small blind
            cap_4_plus_players_max: 250_000_000, // 10x max small blind
        },
        // Low Stakes (4.0%)
        RakeConfig {
            min_small_blind: 25_000_000,           // ICP 0.25
            max_small_blind: 99_999_999,           // ICP 0.99
            percentage_millipercent: 4000,         // 4.0%
            cap_2_3_players_min: 125_000_000,      // 5x min small blind
            cap_2_3_players_max: 500_000_000,      // 5x max small blind
            cap_4_plus_players_min: 250_000_000,   // 10x min small blind
            cap_4_plus_players_max: 1_000_000_000, // 10x max small blind
        },
        // Mid Stakes (3.5%)
        RakeConfig {
            min_small_blind: 100_000_000,          // ICP 1.00
            max_small_blind: 299_999_999,          // ICP 2.99
            percentage_millipercent: 3500,         // 3.5%
            cap_2_3_players_min: 500_000_000,      // 5x min small blind
            cap_2_3_players_max: 1_500_000_000,    // 5x max small blind
            cap_4_plus_players_min: 1_000_000_000, // 10x min small blind
            cap_4_plus_players_max: 3_000_000_000, // 10x max small blind
        },
        // High Stakes (3.0%)
        RakeConfig {
            min_small_blind: 300_000_000,          // ICP 3.00
            max_small_blind: 499_999_999,          // ICP 4.99
            percentage_millipercent: 3000,         // 3.0%
            cap_2_3_players_min: 1_500_000_000,    // 5x min small blind
            cap_2_3_players_max: 2_500_000_000,    // 5x max small blind
            cap_4_plus_players_min: 3_000_000_000, // 10x min small blind
            cap_4_plus_players_max: 5_000_000_000, // 10x max small blind
        },
        // Higher Stakes (2.5%)
        RakeConfig {
            min_small_blind: 500_000_000,           // ICP 5.00
            max_small_blind: u64::MAX,              // Arbitrary high value
            percentage_millipercent: 2500,          // 2.5%
            cap_2_3_players_min: 2_500_000_000,     // 5x min small blind
            cap_2_3_players_max: 5_000_000_000,     // Reasonable cap for high stakes
            cap_4_plus_players_min: 5_000_000_000,  // 10x min small blind
            cap_4_plus_players_max: 10_000_000_000, // Reasonable cap for high stakes
        },
    ];

    fixed_limit_rake_configs
        .iter()
        .find(|cfg| small_blind >= cfg.min_small_blind && small_blind <= cfg.max_small_blind)
        .map(|cfg| Rake {
            percentage_millipercent: cfg.percentage_millipercent,
            cap_2_3_players: interpolate_u64(
                small_blind,
                cfg.min_small_blind,
                cfg.max_small_blind,
                cfg.cap_2_3_players_min,
                cfg.cap_2_3_players_max,
            ),
            cap_4_plus_players: interpolate_u64(
                small_blind,
                cfg.min_small_blind,
                cfg.max_small_blind,
                cfg.cap_4_plus_players_min,
                cfg.cap_4_plus_players_max,
            ),
        })
}

/// Linear interpolation between two points
///
/// # Arguments
/// * `x` - Current value
/// * `x_min` - Minimum input range
/// * `x_max` - Maximum input range
/// * `y_min` - Minimum output range
/// * `y_max` - Maximum output range
///
/// # Returns
/// * `u64` - Interpolated value
pub fn interpolate_u64(x: u64, x_min: u64, x_max: u64, y_min: u64, y_max: u64) -> u64 {
    if x_max == x_min {
        y_min // Avoid division by zero; return minimum cap
    } else {
        y_min + ((x - x_min) * (y_max - y_min)) / (x_max - x_min)
    }
}

/// Scales an amount between different decimal precisions
///
/// # Arguments
/// * `amount` - The amount to scale
/// * `from_decimals` - Current decimal precision
/// * `to_decimals` - Target decimal precision
///
/// # Returns
/// * `u64` - The scaled amount
fn scale_amount(amount: u64, from_decimals: u8, to_decimals: u8) -> u64 {
    match from_decimals.cmp(&to_decimals) {
        std::cmp::Ordering::Greater => {
            // When scaling down, divide first to avoid overflow
            let scale = 10u64.pow((from_decimals - to_decimals) as u32);
            amount / scale
        }
        std::cmp::Ordering::Less => {
            // When scaling up, check for potential overflow
            let scale = 10u64.pow((to_decimals - from_decimals) as u32);
            amount.checked_mul(scale).unwrap_or(u64::MAX)
        }
        std::cmp::Ordering::Equal => amount,
    }
}
