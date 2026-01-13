use crate::poker::core::card::{Card, Suit, Value};
use crate::poker::core::deck::Deck;
use std::ops::{Index, Range, RangeFrom, RangeFull, RangeTo};

use candid::CandidType;
use serde::{Deserialize, Serialize};

/// `FlatDeck` is a deck of cards that allows easy
/// indexing into the cards. It does not provide
/// contains methods.
#[derive(Debug, Clone, PartialEq, CandidType, Serialize, Deserialize)]
pub struct FlatDeck {
    /// Card storage.
    cards: Vec<Card>,
}

impl FlatDeck {
    /// How many cards are there in the deck ?
    pub fn len(&self) -> usize {
        self.cards.len()
    }
    /// Have all cards been dealt ?
    /// This probably won't be used as it's unlikely
    /// that someone will deal all 52 cards from a deck.
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    /// Get a slice of all cards in the deck
    pub fn cards(&self) -> &[Card] {
        &self.cards
    }

    /// Randomly shuffle the flat deck.
    /// This will ensure the there's no order to the deck.
    pub fn shuffle(&mut self, rand_bytes: Vec<u8>) {
        // Perform the Fisher-Yates shuffle using the random bytes
        let mut n = self.cards.len();
        for i in 0..n - 1 {
            let rand_index = (rand_bytes[i % rand_bytes.len()] as usize) % n;
            self.cards.swap(i, rand_index);
            // Adjust n to prevent the same bytes from influencing too many swaps
            n = n.saturating_sub(1);
        }
    }

    /// Deal a card if there is one there to deal.
    /// None if the deck is empty
    pub fn deal(&mut self) -> Option<Card> {
        self.cards.pop()
    }
}

impl Index<usize> for FlatDeck {
    type Output = Card;
    fn index(&self, index: usize) -> &Card {
        &self.cards[index]
    }
}
impl Index<Range<usize>> for FlatDeck {
    type Output = [Card];
    fn index(&self, index: Range<usize>) -> &[Card] {
        &self.cards[index]
    }
}
impl Index<RangeTo<usize>> for FlatDeck {
    type Output = [Card];
    fn index(&self, index: RangeTo<usize>) -> &[Card] {
        &self.cards[index]
    }
}
impl Index<RangeFrom<usize>> for FlatDeck {
    type Output = [Card];
    fn index(&self, index: RangeFrom<usize>) -> &[Card] {
        &self.cards[index]
    }
}
impl Index<RangeFull> for FlatDeck {
    type Output = [Card];
    fn index(&self, index: RangeFull) -> &[Card] {
        &self.cards[index]
    }
}

impl From<Vec<Card>> for FlatDeck {
    fn from(value: Vec<Card>) -> Self {
        Self { cards: value }
    }
}

/// Allow creating a flat deck from a Deck
impl From<Deck> for FlatDeck {
    /// Flatten this deck, consuming it to produce a `FlatDeck` that's
    /// easier to get random access to.
    fn from(value: Deck) -> Self {
        // We sort the cards so that the same input
        // cards always result in the same starting flat deck
        let mut cards: Vec<Card> = value.into_iter().collect();
        cards.sort();

        Self { cards }
    }
}

impl FlatDeck {
    /// Create a new shuffled deck from random bytes.
    ///
    /// Generates cards in deterministic sorted order (matching Card's Ord implementation)
    /// to ensure verification can recreate the exact same initial deck state.
    /// This avoids the non-deterministic iteration order of HashSet.
    pub fn new(bytes: Vec<u8>) -> Self {
        // Generate cards deterministically in sorted order
        // This matches the pattern used in Deck::default() but produces sorted order directly
        let mut cards = Vec::with_capacity(52);
        for v in &Value::values() {
            for s in &Suit::suits() {
                cards.push(Card {
                    value: *v,
                    suit: *s,
                });
            }
        }
        // Cards are now in sorted order (TwoSpade, TwoClub, ..., AceDiamond)
        let mut fdeck = Self { cards };
        fdeck.shuffle(bytes);

        fdeck
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poker::core::card::{Suit, Value};

    #[test]
    fn test_deck_from() {
        let fd: FlatDeck = Deck::default().into();
        assert_eq!(52, fd.len());
    }

    #[test]
    fn test_deck_new() {
        let fd: FlatDeck = FlatDeck::new(vec![1, 2, 3, 4, 5]);
        assert_eq!(52, fd.len());
    }

    #[test]
    fn test_from_vec() {
        let c = Card {
            value: Value::Nine,
            suit: Suit::Heart,
        };
        let v = vec![c];

        let mut flat_deck: FlatDeck = v.into();

        assert_eq!(1, flat_deck.len());
        assert_eq!(c, flat_deck.deal().unwrap());
    }
}
