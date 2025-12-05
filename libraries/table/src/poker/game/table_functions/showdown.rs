use std::collections::HashMap;

use errors::{game_error::GameError, trace_err, traced_error::TracedError};
use user::user::WalletPrincipalId;

use crate::poker::{
    core::{Card, Hand, Rank, Rankable, CardIter},
    game::{table_functions::table::Pot, types::{GameType, UserCards}},
};

use super::{
    action_log::ActionType,
    rake::Rake,
    table::Table,
    types::{CurrencyType, PlayerAction, SeatStatus},
};

type RankedHand = (WalletPrincipalId, Hand, Rank, Vec<Card>);

impl Table {
    /// Compares the hands of the players to determine the winner
    ///
    /// # Errors
    ///
    /// - [`GameError::Other`] if the user table data cannot be retrieved
    /// - [`GameError::PlayerNotFound`] if retrieving a player fails
    pub fn showdown(&mut self) -> Result<(), TracedError<GameError>> {
        let ranked_hands = self
            .get_ranked_hands()
            .map_err(|e| trace_err!(e, "Failed to get ranked hands in showdown."))?;

        let mut winners_total_amount: HashMap<WalletPrincipalId, u64> = HashMap::new();
        self.log_action(
            None,
            ActionType::Stage {
                stage: self.deal_stage,
            },
        );

        self.rotate_dealer()
            .map_err(|e| trace_err!(e, "Failed to rotate dealer in showdown."))?;

        #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
        {
            ic_cdk::println!("Clearing turn timer in showdown.");
            self.clear_turn_timer();
        }

        self.confirm_side_pots();

        // Distribute the side pots
        for pot in self.side_pots.clone().iter_mut() {
            let inner_ranked_hands = ranked_hands.clone();
            let inner_ranked_hands = inner_ranked_hands
                .iter()
                .filter(|(user_principal, _, _, _)| pot.user_principals.contains(user_principal))
                .collect::<Vec<_>>();

            let mut individual_shares: HashMap<WalletPrincipalId, u64> = HashMap::new();

            if let Some(enable_rake) = self.config.enable_rake {
                if enable_rake {
                    if let CurrencyType::Real(currency) = &self.config.currency_type {
                        let rake = self
                            .rake_config
                            .clone()
                            .unwrap_or(
                                Rake::new(self.small_blind, &self.config.game_type, currency)
                                    .map_err(|e| {
                                        trace_err!(
                                            e,
                                            "Failed to intialise rake in showdown, side pot."
                                        )
                                    })?,
                            )
                            .calculate_rake(pot.confirmed_pot, self.number_of_players() as u8);
                        pot.confirmed_pot = pot.confirmed_pot.saturating_sub(rake);
                        let rake_total = self.rake_total.unwrap_or(0);
                        self.rake_total = Some(rake_total + rake);
                    }
                }
            }

            let inner_rank_hands_clone = inner_ranked_hands.clone();

            if let Some((_, _, first_rank, _)) = inner_ranked_hands.first() {
                let tied_users: Vec<WalletPrincipalId> = inner_ranked_hands
                    .iter()
                    .filter(|(_, _, rank, _)| rank == first_rank)
                    .map(|(user_principal, _, _, _)| *user_principal)
                    .collect();

                if !tied_users.is_empty() {
                    let individual_share = pot.confirmed_pot / tied_users.len() as u64;

                    for user in &tied_users {
                        self.users
                            .get_mut(user)
                            .ok_or_else(|| trace_err!(TracedError::new(GameError::PlayerNotFound)))?
                            .deposit(individual_share);
                        *winners_total_amount.entry(*user).or_insert(0) += individual_share;
                        individual_shares.insert(*user, individual_share);
                    }
                }
            }

            let mut log_hands = Vec::new();
            for (user_principal, _, _, cards) in inner_rank_hands_clone.iter() {
                let user = self
                    .users
                    .get(user_principal)
                    .ok_or_else(|| trace_err!(TracedError::new(GameError::PlayerNotFound)))?;
                let amount_won = *individual_shares.get(user_principal).unwrap_or(&0);
                log_hands.push((user.user_name.clone(), cards.clone(), amount_won));
            }

            if self.all_players_folded() {
                // If all players except one folded remove all cards from log hands.
                // This is for ensuring correct mucking of cards in the frontend.
                log_hands = log_hands
                    .iter()
                    .map(|(user_name, _, amount_won)| (user_name.clone(), vec![], *amount_won))
                    .collect();
            }

            self.log_action(
                None,
                ActionType::PlayersHandsRankedSidePot { hands: log_hands },
            );
        }

        let mut individual_shares: HashMap<WalletPrincipalId, u64> = HashMap::new();

        if let Some(enable_rake) = self.config.enable_rake {
            if enable_rake {
                if let CurrencyType::Real(currency) = &self.config.currency_type {
                    let rake =
                        self.rake_config
                            .clone()
                            .unwrap_or(
                                Rake::new(self.small_blind, &self.config.game_type, currency)
                                    .map_err(|e| {
                                        trace_err!(
                                            e,
                                            "Failed to initialise rake in showdown, main pot."
                                        )
                                    })?,
                            )
                            .calculate_rake(self.pot.0, self.number_of_players() as u8);
                    self.pot = Pot(self.pot.0.saturating_sub(rake));
                    let rake_total = self.rake_total.unwrap_or(0);
                    self.rake_total = Some(rake_total + rake);
                }
            }
        }

        // Distribute the main pot
        if let Some((_, _, first_rank, _)) = ranked_hands.first() {
            let tied_users: Vec<WalletPrincipalId> = ranked_hands
                .iter()
                .filter(|(_, _, rank, _)| rank == first_rank)
                .map(|(user_principal, _, _, _)| *user_principal)
                .collect();

            if !tied_users.is_empty() {
                let mut winners = Vec::new();
                let individual_share = self.pot.0 / tied_users.len() as u64;

                for user in &tied_users {
                    let user = {
                        let user = self.users.get_mut(user).ok_or_else(|| {
                            trace_err!(TracedError::new(GameError::PlayerNotFound))
                        })?;
                        user.deposit(individual_share);
                        user.clone()
                    };

                    *winners_total_amount.entry(user.principal_id).or_insert(0) += individual_share;
                    individual_shares.insert(user.principal_id, individual_share);

                    winners.push(user.clone());
                }
                // Handle ties according to your game's rules
                self.winners = Some(winners.clone());
                self.set_sorted_users(winners_total_amount)
                    .map_err(|e| trace_err!(e, "Failed to set sorted users."))?;
                self.pot = Pot(0);
                self.side_pots.clear();

                #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
                self.start_next_turn_timer(self.config.auto_start_timer.into());
                return Ok(());
            }
        }

        let mut log_hands = Vec::new();
        for (user_principal, _, _, cards) in ranked_hands.iter() {
            let user = self
                .users
                .get(user_principal)
                .ok_or_else(|| trace_err!(TracedError::new(GameError::PlayerNotFound)))?;
            let amount_won = *individual_shares.get(user_principal).unwrap_or(&0);
            log_hands.push((user.user_name.clone(), cards.clone(), amount_won));
        }

        if self.all_players_folded() {
            // If all players except one folded remove all cards from log hands.
            // This is for ensuring correct mucking of cards in the frontend.
            log_hands = log_hands
                .iter()
                .map(|(user_name, _, amount_won)| (user_name.clone(), vec![], *amount_won))
                .collect();
        }

        self.log_action(
            None,
            ActionType::PlayersHandsRankedMainPot { hands: log_hands },
        );

        self.set_sorted_users(winners_total_amount)
            .map_err(|e| trace_err!(e, "Failed to set sorted users."))?;
        self.pot = Pot(0);
        self.side_pots.clear();

        #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
        self.start_next_turn_timer(self.config.auto_start_timer.into());
        Ok(())
    }

    /// Checks if the side pots have been confirmed and if not, confirms them
    fn confirm_side_pots(&mut self) {
        for pot in self.side_pots.iter_mut() {
            if pot.pot > pot.confirmed_pot {
                pot.confirm_pot();
            }
        }
    }

    /// Sets the sorted users sorted by their rank
    ///
    /// # Parameters
    ///
    /// - `winners_total_amount`: The total amount won by each winner
    ///
    /// # Errors
    ///
    /// - [`GameError::Other`] if the user table data cannot be retrieved
    /// - [`GameError::PlayerNotFound`] if retrieving a player fails
    fn set_sorted_users(
        &mut self,
        winners_total_amount: HashMap<WalletPrincipalId, u64>,
    ) -> Result<(), TracedError<GameError>> {
        let ranked_hands = self
            .get_ranked_hands()
            .map_err(|e| trace_err!(e, "Failed to ger ranked hands in set sorted users."))?;
        self.sorted_users = Some(
            ranked_hands
                .iter()
                .map(|(user_principal, hand, rank, _)| {
                    let amount_won = *winners_total_amount.get(user_principal).unwrap_or(&0);
                    UserCards::new(*user_principal, hand.clone(), *rank, amount_won)
                })
                .collect(),
        );
        Ok(())
    }

    /// Gets the ranked hands of the players sorted by each player's rank
    ///
    /// # Errors
    ///
    /// - [`GameError::Other`] if the user table data cannot be retrieved
    /// - [`GameError::PlayerNotFound`] if retrieving a player fails
    fn get_ranked_hands(&mut self) -> Result<Vec<RankedHand>, TracedError<GameError>> {
        let mut ranked_hands: Vec<RankedHand> = Vec::new();
        let is_omaha = matches!(
            self.config.game_type,
            GameType::PotLimitOmaha4(_) | GameType::PotLimitOmaha5(_)
        );

        for user_principal in self.seats.iter() {
            if let SeatStatus::Occupied(user_principal) = user_principal {
                // Clone the cards first to avoid borrow checker issues
                let hole_cards = {
                    let user_table_data =
                        self.user_table_data
                            .get(user_principal)
                            .ok_or_else(|| {
                                trace_err!(TracedError::new(GameError::Other(
                                    "Could not get users table data".to_string(),
                                )))
                            })?;
                    
                    if user_table_data.player_action == PlayerAction::Folded
                        || user_table_data.player_action == PlayerAction::SittingOut
                        || user_table_data.player_action == PlayerAction::Joining
                    {
                        continue;
                    }
                    
                    user_table_data.cards.clone()
                };

                // Get principal_id first, then release the borrow
                let principal_id = {
                    let user = self
                        .users
                        .get(user_principal)
                        .ok_or_else(|| trace_err!(TracedError::new(GameError::PlayerNotFound)))?;
                    user.principal_id
                };
                
                let (best_hand, best_rank, all_cards) = if is_omaha {
                    // Omaha: Must use exactly 2 hole cards + exactly 3 community cards
                    // Clone community cards to avoid borrow issues
                    let community_cards = self.community_cards.clone();
                    self.get_best_omaha_hand(
                        &hole_cards,
                        &community_cards,
                    )?
                } else {
                    // Hold'em: Use all cards and find best 5-card hand
                    let mut all_cards = hole_cards.clone();
                    all_cards.extend(self.community_cards.clone());
                    let hand = Hand::new_with_cards(all_cards.clone());
                    let rank = hand.rank();
                    (hand, rank, all_cards)
                };

                ranked_hands.push((principal_id, best_hand, best_rank, all_cards));
            }
        }

        // Sort ranked hands by their rank
        ranked_hands.sort_by(|a, b| b.2.cmp(&a.2)); // Descending order
        Ok(ranked_hands)
    }

    /// Finds the best 5-card hand for Omaha (exactly 2 hole + exactly 3 community)
    ///
    /// # Parameters
    ///
    /// - `hole_cards`: The player's hole cards (4 or 5 cards)
    /// - `community_cards`: The 5 community cards
    ///
    /// # Returns
    ///
    /// A tuple of (best Hand, best Rank, all cards used)
    fn get_best_omaha_hand(
        &self,
        hole_cards: &[Card],
        community_cards: &[Card],
    ) -> Result<(Hand, Rank, Vec<Card>), TracedError<GameError>> {
        if hole_cards.len() < 2 {
            return Err(trace_err!(TracedError::new(GameError::Other(
                "Not enough hole cards for Omaha".to_string(),
            ))));
        }
        if community_cards.len() < 3 {
            return Err(trace_err!(TracedError::new(GameError::Other(
                "Not enough community cards for Omaha".to_string(),
            ))));
        }

        let mut best_rank = Rank::HighCard(0);
        let mut best_hand = Hand::default();
        let mut _best_cards = Vec::new();

        // Generate all combinations of 2 hole cards
        for hole_combo in CardIter::new(hole_cards, 2) {
            // Generate all combinations of 3 community cards
            for community_combo in CardIter::new(community_cards, 3) {
                // Combine to make a 5-card hand
                let mut five_card_hand: Vec<Card> = hole_combo.clone();
                five_card_hand.extend(community_combo.clone());
                
                let hand = Hand::new_with_cards(five_card_hand.clone());
                let rank = hand.rank_five(); // Use rank_five for exactly 5 cards (faster)

                // Keep track of the best hand
                if rank > best_rank {
                    best_rank = rank;
                    best_hand = hand;
                    _best_cards = five_card_hand;
                }
            }
        }

        // Create all_cards for logging (all hole + all community)
        let mut all_cards = hole_cards.to_vec();
        all_cards.extend(community_cards.to_vec());

        Ok((best_hand, best_rank, all_cards))
    }
}
