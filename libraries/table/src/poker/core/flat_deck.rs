use crate::poker::core::card::{Card, Suit, Value};
use crate::poker::core::deck::Deck;
use std::ops::{Index, Range, RangeFrom, RangeFull, RangeTo};

use candid::CandidType;
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
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

    /// Generate an unbiased random index in range [0, upper) using rejection sampling.
    ///
    /// This eliminates modulo bias by rejecting values that would create unfair distribution.
    /// For example, if upper=3 and we have 256 possibilities, 256%3=1, so values 0-254
    /// map evenly (85, 85, 85) but 255-256 would create bias. We reject those.
    fn unbiased_index(rng: &mut ChaCha20Rng, upper: usize) -> usize {
        assert!(upper > 0, "upper must be positive");

        let max = 256usize;
        let limit = max - (max % upper);

        loop {
            let byte = (rng.next_u32() & 0xFF) as usize; // Get one byte (0-255)

            if byte < limit {
                return byte % upper;
            }
            // Reject and try again if byte >= limit (eliminates modulo bias)
        }
    }

    /// Fisher-Yates shuffle with cryptographically unbiased random selection.
    ///
    /// Uses ChaCha20 PRNG seeded from the provided random bytes, ensuring:
    /// 1. Unlimited deterministic randomness for rejection sampling
    /// 2. No modulo bias (critical for provably fair poker)
    /// 3. Same input bytes always produce same shuffle (verifiable)
    /// 4. Cryptographically secure randomness distribution
    ///
    /// The input `rand_bytes` should come from IC VRF (via raw_rand) for on-chain transparency.
    pub fn shuffle(&mut self, rand_bytes: Vec<u8>) {
        let n = self.cards.len();
        if n <= 1 {
            return;
        }

        // Seed ChaCha20 PRNG with the provided random bytes
        // This gives us unlimited deterministic random bytes for rejection sampling
        let mut seed = [0u8; 32];
        let copy_len = rand_bytes.len().min(32);
        seed[..copy_len].copy_from_slice(&rand_bytes[..copy_len]);

        let mut rng = ChaCha20Rng::from_seed(seed);

        // Fisher-Yates shuffle with unbiased random selection
        // For each position i, pick uniformly from remaining cards [i, n)
        for i in 0..(n - 1) {
            let remaining = n - i;
            let offset = Self::unbiased_index(&mut rng, remaining);
            let j = i + offset;

            self.cards.swap(i, j);
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
