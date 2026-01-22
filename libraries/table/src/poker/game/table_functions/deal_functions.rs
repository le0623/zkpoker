use errors::{game_error::GameError, trace_err, traced_error::TracedError};

use crate::poker::core::Card;
use crate::poker::game::types::GameType;
use user::user::WalletPrincipalId;

use super::{
    action_log::ActionType,
    table::Table,
    types::{DealStage, PlayerAction, SeatStatus},
};

impl Table {
    /// Helper function to update card provenance when a card is dealt
    /// Finds the provenance record matching the card and updates dealt_to and dealt_at_stage
    fn update_card_provenance(
        &mut self,
        card: Card,
        dealt_to: Option<WalletPrincipalId>,
        dealt_at_stage: DealStage,
    ) {
        let round_id = self.round_ticker;
        let mut found = false;

        #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
        let mut total_checked = 0;
        #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
        let mut round_match_count = 0;

        // Find the provenance record matching this card for the current round
        // Since each card is unique in a deck, we can match by card value/suit
        for (_, provenance) in self.card_provenance.iter_mut() {
            #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
            {
                total_checked += 1;
            }
            if provenance.round_id == round_id {
                #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
                {
                    round_match_count += 1;
                }
                if provenance.card.value == card.value
                    && provenance.card.suit == card.suit
                    && provenance.dealt_to.is_none()
                // Only update if not already dealt
                {
                    provenance.dealt_to = dealt_to;
                    provenance.dealt_at_stage = Some(dealt_at_stage);
                    found = true;
                    break; // Each card is unique, so we found it
                }
            }
        }

        // Debug: Log if card wasn't found (shouldn't happen in normal flow)
        #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
        if !found {
            ic_cdk::println!(
                "⚠️ Warning: Could not find provenance for card {:?}:{:?} in round {} (checked {} records, {} matched round)",
                card.value,
                card.suit,
                round_id,
                total_checked,
                round_match_count
            );
            // If no records matched the round, there might be a round_id mismatch
            if round_match_count == 0 {
                ic_cdk::println!(
                    "⚠️ No provenance records found for round {}! Total provenance records: {}",
                    round_id,
                    self.card_provenance.len()
                );
            }
        }
        let _ = found; // Suppress unused variable warning in non-wasm builds
    }

    /// Deals the cards for the current stage
    ///
    /// # Parameters
    ///
    /// - `is_cycling_to_showdown`: Whether the game is cycling to the showdown stage
    ///
    /// # Errors
    ///
    /// - [`GameError::PlayerNotFound`] if a player is not found
    /// - [`GameError::NoCardsLeft`] if there are no cards left in the deck
    /// - [`GameError::Other`] if the user table data cannot be retrieved
    pub fn deal_cards(
        &mut self,
        is_cycling_to_showdown: bool,
    ) -> Result<(), TracedError<GameError>> {
        self.clean_up_side_pots()
            .map_err(|e| trace_err!(e, "Failed to clean up side pots in deal_cards"))?;
        self.log_action(
            None,
            ActionType::Stage {
                stage: self.deal_stage,
            },
        );
        match self.deal_stage {
            DealStage::Opening => {
                self.deal_opening_cards()
                    .map_err(|e| trace_err!(e, "Failed to deal opening cards."))?;
                return Ok(());
            }
            DealStage::Flop => {
                self.deal_flop_cards()
                    .map_err(|e| trace_err!(e, "Failed to deal flop cards."))?;
            }
            DealStage::Turn => {
                self.deal_turn_card()
                    .map_err(|e| trace_err!(e, "Failed to deal turn cards."))?;
            }
            DealStage::River => {
                self.deal_river_card()
                    .map_err(|e| trace_err!(e, "Failed to deal river cards."))?;
            }
            _ => {}
        }
        if self.is_side_pot_active && !is_cycling_to_showdown {
            self.get_side_pot_mut()
                .map_err(|e| trace_err!(e, "Failed to get side pot to confirm it in deal_cards."))?
                .confirm_pot();
        }

        // Prepare for the next stage
        self.prepare_user_actions(is_cycling_to_showdown)
            .map_err(|e| trace_err!(e, "Failed to prepare user actions in deal_cards."))?;
        self.highest_bet = 0;
        self.highest_bet_in_pot = 0;
        self.last_raise = 0;

        Ok(())
    }

    /// Deals the opening cards to all players ( 2 for Hold'em, 4 for PLO4, 5 for PLO5)
    ///
    /// # Errors
    ///
    /// - [`GameError::NoCardsLeft`] if there are no cards left in the deck
    /// - [`GameError::Other`] if the user table data cannot be retrieved
    fn deal_opening_cards(&mut self) -> Result<(), TracedError<GameError>> {
        let num_hole_cards = match self.config.game_type {
            GameType::PotLimitOmaha4(_) => 4,
            GameType::PotLimitOmaha5(_) => 5,
            _ => 2,
        };

        // Collect occupied seat principals first to avoid borrowing conflicts
        let occupied_principals: Vec<WalletPrincipalId> = self
            .seats
            .iter()
            .filter_map(|seat| {
                if let SeatStatus::Occupied(principal) = seat {
                    Some(*principal)
                } else {
                    None
                }
            })
            .collect();

        for _ in 0..num_hole_cards {
            for user_principal in &occupied_principals {
                // Check if player is sitting out
                let is_sitting_out = self
                    .user_table_data
                    .get(user_principal)
                    .map(|data| data.player_action == PlayerAction::SittingOut)
                    .unwrap_or(true);

                if !is_sitting_out {
                    let card = self
                        .deck
                        .deal()
                        .ok_or_else(|| trace_err!(TracedError::new(GameError::NoCardsLeft)))?;
                    // Update card provenance: this card was dealt to this player at Opening stage
                    self.update_card_provenance(card, Some(*user_principal), DealStage::Opening);
                    // Now add card to user's hand
                    if let Some(user_table_data) = self.user_table_data.get_mut(user_principal) {
                        user_table_data.cards.push(card);
                    }
                }
            }
        }
        self.deal_stage = DealStage::Flop;
        Ok(())
    }

    /// Deals the flop cards (the first three community cards)
    ///
    /// # Errors
    ///
    /// - [`GameError::NoCardsLeft`] if there are no cards left in the deck
    fn deal_flop_cards(&mut self) -> Result<(), TracedError<GameError>> {
        self.burn_card()
            .map_err(|e| trace_err!(e, "Failed to burn card in deal_flop_cards."))?;
        for _ in 0..3 {
            self.deal_card()
                .map_err(|e| trace_err!(e, "Failed to deal card in deal_flop_cards."))?;
        }
        self.deal_stage = DealStage::Turn;
        Ok(())
    }

    /// Deals the turn card (the fourth community card)
    ///
    /// # Errors
    ///
    /// - [`GameError::NoCardsLeft`] if there are no cards left in the deck
    fn deal_turn_card(&mut self) -> Result<(), TracedError<GameError>> {
        self.burn_and_deal()
            .map_err(|e| trace_err!(e, "Failed to burn and deal in deal_turn_card."))?;
        self.deal_stage = DealStage::River;
        Ok(())
    }

    /// Deals the river card (the fifth and final community card)
    ///
    /// # Errors
    ///
    /// - [`GameError::NoCardsLeft`] if there are no cards left in the deck
    fn deal_river_card(&mut self) -> Result<(), TracedError<GameError>> {
        self.burn_and_deal()
            .map_err(|e| trace_err!(e, "Failed to burn and deal in deal_river_card."))?;
        self.deal_stage = DealStage::Showdown;
        Ok(())
    }

    /// Discards a card from the deck and deals a card to the community cards
    ///
    /// # Errors
    ///
    /// - [`GameError::NoCardsLeft`] if there are no cards left in the deck
    fn burn_and_deal(&mut self) -> Result<(), TracedError<GameError>> {
        self.burn_card()
            .map_err(|e| trace_err!(e, "Failed to burn card in burn_and_deal."))?;
        self.deal_card()
            .map_err(|e| trace_err!(e, "Failed to deal card in burn_and_deal."))?;
        Ok(())
    }

    /// Deals a card from the deck to the community cards
    ///
    /// # Errors
    ///
    /// - [`GameError::NoCardsLeft`] if there are no cards left in the deck
    fn deal_card(&mut self) -> Result<(), TracedError<GameError>> {
        let card = self
            .deck
            .deal()
            .ok_or_else(|| trace_err!(TracedError::new(GameError::NoCardsLeft)))?;
        // Update card provenance: community cards have no owner (None) and are dealt at current stage
        self.update_card_provenance(card, None, self.deal_stage);
        self.community_cards.push(card);
        Ok(())
    }

    /// Discards a card from the deck
    ///
    /// # Errors
    ///
    /// - [`GameError::NoCardsLeft`] if there are no cards left in the deck
    fn burn_card(&mut self) -> Result<(), TracedError<GameError>> {
        self.deck
            .deal()
            .ok_or_else(|| trace_err!(TracedError::new(GameError::NoCardsLeft)))?;
        Ok(())
    }
}
