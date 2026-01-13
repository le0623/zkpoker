import { useMemo, useState, useEffect } from "react";
import type { RngMetadata, Card, CardProvenance } from "../../types/rng.types";
import type { _SERVICE } from "@declarations/table_canister/table_canister.did";
import { useTable } from "../../context/table.context";
import { useUser } from "@lib/user";
import { calculateCardHash } from "../../utils/card-hash";
import { shortenHash } from "../../types/rng.types";
import { CardComponent } from "../card/card.component";

interface RngDeckPanelProps {
  rngData: RngMetadata;
  tableActor: _SERVICE;
  roundId?: bigint;
}

export function RngDeckPanel({
  rngData,
  tableActor,
  roundId,
}: RngDeckPanelProps) {
  const { table, user } = useTable();
  const { user: zkpUser } = useUser();

  // New state management as per plan
  const [cardProvenance, setCardProvenance] = useState<
    Map<number, CardProvenance>
  >(new Map());
  const [flippedCards, setFlippedCards] = useState<Set<number>>(new Set());
  const [verifiedHashes, setVerifiedHashes] = useState<Map<number, boolean>>(
    new Map()
  );
  const [calculatedHashes, setCalculatedHashes] = useState<Map<number, string>>(
    new Map()
  );
  const [verifyingCard, setVerifyingCard] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);

  // Check if game has finished
  const gameFinished = useMemo(() => {
    if (!table) return false;
    const firstSortedUsers = table.sorted_users?.[0];
    const hasWinners =
      firstSortedUsers !== undefined && firstSortedUsers.length > 0;
    return hasWinners;
  }, [table]);

  // Fetch card provenance using the new API
  useEffect(() => {
    async function loadCardProvenance() {
      setLoading(true);
      try {
        // Use get_card_provenance_by_round_id (None = current round)
        const result = await tableActor.get_card_provenance_by_round_id(
          roundId ? [roundId] : []
        );
        if ("Ok" in result) {
          const provenanceMap = new Map<number, CardProvenance>();
          result.Ok.forEach((prov) => {
            provenanceMap.set(prov.shuffled_position, prov);
          });
          setCardProvenance(provenanceMap);
        }
      } catch (err) {
        console.error("Failed to load card provenance:", err);
      } finally {
        setLoading(false);
      }
    }

    loadCardProvenance();
  }, [roundId, tableActor, rngData.round_id]);

  // Card matching helper
  const cardMatches = (c1: Card, c2: Card): boolean => {
    const c1Value = Object.keys(c1.value)[0];
    const c1Suit = Object.keys(c1.suit)[0];
    const c2Value = Object.keys(c2.value)[0];
    const c2Suit = Object.keys(c2.suit)[0];
    return c1Value === c2Value && c1Suit === c2Suit;
  };

  // Get revealed cards with game sync (plan section 2.3)
  const revealedCards = useMemo(() => {
    const revealed = new Set<number>();

    // 1. User's hole cards (always visible)
    if (user?.data?.cards) {
      user.data.cards.forEach((card) => {
        const prov = Array.from(cardProvenance.values()).find(
          (p) => p.round_id === rngData.round_id && cardMatches(p.card, card)
        );
        if (prov) revealed.add(prov.shuffled_position);
      });
    }

    // 2. Community cards (sync with table state AND deal stage)
    if (table?.community_cards) {
      table.community_cards.forEach((card) => {
        const prov = Array.from(cardProvenance.values()).find(
          (p) => p.round_id === rngData.round_id && cardMatches(p.card, card)
        );
        if (prov) revealed.add(prov.shuffled_position);
      });
    }

    // 3. Double-check with deal_stage for robustness
    if (table?.deal_stage) {
      const stage = Object.keys(table.deal_stage)[0];
      // PreFlop: 0, Flop: 3, Turn: 4, River: 5
      const expectedCount =
        stage === "PreFlop"
          ? 0
          : stage === "Flop"
            ? 3
            : stage === "Turn"
              ? 4
              : stage === "River"
                ? 5
                : 0;

      // Validate sync
      if (table.community_cards.length !== expectedCount) {
        console.warn("Community cards out of sync with deal stage!");
      }
    }

    // 4. After game ends, reveal all
    if (gameFinished) {
      Array.from(cardProvenance.keys()).forEach((pos) => revealed.add(pos));
    }

    return revealed;
  }, [user, table, cardProvenance, gameFinished, rngData.round_id]);

  // Identify card types (hole cards, community cards)
  const holeCardPositions = useMemo(() => {
    const positions = new Set<number>();
    if (user?.data?.cards) {
      user.data.cards.forEach((card) => {
        const prov = Array.from(cardProvenance.values()).find(
          (p) => p.round_id === rngData.round_id && cardMatches(p.card, card)
        );
        if (prov) positions.add(prov.shuffled_position);
      });
    }
    return positions;
  }, [user, cardProvenance, rngData.round_id]);

  const communityCardPositions = useMemo(() => {
    const positions = new Set<number>();
    if (table?.community_cards) {
      table.community_cards.forEach((card) => {
        const prov = Array.from(cardProvenance.values()).find(
          (p) => p.round_id === rngData.round_id && cardMatches(p.card, card)
        );
        if (prov) positions.add(prov.shuffled_position);
      });
    }
    return positions;
  }, [table, cardProvenance, rngData.round_id]);

  // Check if a card is dummy (Two of Spades = hidden)
  const isDummyCard = (card: Card): boolean => {
    const valueKey = Object.keys(card.value)[0];
    const suitKey = Object.keys(card.suit)[0];
    return valueKey === "Two" && suitKey === "Spade";
  };

  // Interactive card component (plan section 2.4)
  const CardItem = ({
    position,
    provenance,
  }: {
    position: number;
    provenance: CardProvenance;
  }) => {
    const isRevealed = gameFinished
      ? revealedCards.has(position)
      : revealedCards.has(position) && !isDummyCard(provenance.card);
    const isFlipped = flippedCards.has(position);
    const hashVerified = verifiedHashes.get(position);
    const isHoleCard = holeCardPositions.has(position);
    const isCommunityCard = communityCardPositions.has(position);

    const handleCardClick = async () => {
      if (!isRevealed) return; // Only clickable when revealed

      const hasBeenVerified = calculatedHashes.has(position);

      // If card hasn't been verified yet, calculate hash on first click
      if (!hasBeenVerified && !isFlipped) {
        setVerifyingCard(position);
        try {
          // Calculate hash from visible card data
          const calculatedHash = await calculateCardHash(
            provenance.card,
            rngData.round_id,
            position
          );

          // Store the calculated hash and comparison result
          setCalculatedHashes((prev) =>
            new Map(prev).set(position, calculatedHash)
          );
          const matches = calculatedHash === provenance.card_hash;
          setVerifiedHashes((prev) => new Map(prev).set(position, matches));
        } catch (err) {
          console.error("Hash verification failed:", err);
          setVerifiedHashes((prev) => new Map(prev).set(position, false));
        } finally {
          setVerifyingCard(null);
        }
        return; // Don't toggle on first verification
      }

      // Toggle between verified front and card back
      setFlippedCards((prev) => {
        const next = new Set(prev);
        if (next.has(position)) {
          next.delete(position);
        } else {
          next.add(position);
        }
        return next;
      });
    };

    return (
      <div
        className={`card-item-wrapper ${isRevealed ? "revealed" : "hidden"} ${isHoleCard ? "hole-card" : ""} ${isCommunityCard ? "community-card" : ""}`}
        onClick={handleCardClick}
        style={{ cursor: isRevealed ? "pointer" : "default" }}
      >
        {/* Card display */}
        {!isRevealed ? (
          // Unrevealed card: always show back with hash overlay
          <div className="card-back">
            <CardComponent size="small" />
            <div className="hash-overlay" title={provenance.card_hash}>
              {shortenHash(provenance.card_hash, 6, 4)}
            </div>
          </div>
        ) : isFlipped ? (
          // Revealed card: back view (after toggle)
          <div className="card-back">
            <CardComponent size="small" />
            <div className="hash-overlay" title={provenance.card_hash}>
              {shortenHash(provenance.card_hash, 6, 4)}
            </div>
          </div>
        ) : (
          // Revealed card: front view (verified or unverified)
          <div className="card-display">
            <CardComponent card={provenance.card} size="small" />
            {calculatedHashes.has(position) && (
              <div
                className="hash-overlay hash-overlay-front"
                title={calculatedHashes.get(position)}
              >
                {shortenHash(calculatedHashes.get(position)!, 6, 4)}
              </div>
            )}
            {verifyingCard === position && (
              <div className="hash-overlay hash-overlay-loading">
                Calculating...
              </div>
            )}
          </div>
        )}

        {/* Card type badge */}
        {isHoleCard && <div className="card-type-badge">H</div>}
        {isCommunityCard && !isHoleCard && (
          <div className="card-type-badge">C</div>
        )}

        {/* Position label */}
        <div className="card-position">#{position}</div>

        {/* Verification badge (only show when verified) */}
        {hashVerified !== undefined &&
          calculatedHashes.has(position) &&
          !isFlipped && (
            <div
              className={`hash-badge ${hashVerified ? "match" : "mismatch"}`}
            >
              {hashVerified ? "✓" : "✗"}
            </div>
          )}
      </div>
    );
  };

  if (loading) {
    return (
      <section className="rng-panel">
        <h3 className="panel-title">🃏 Deck Transparency</h3>
        <div className="loading-state">Loading deck data...</div>
      </section>
    );
  }

  const allCardsRevealed = gameFinished;
  const revealedCount = revealedCards.size;

  return (
    <section className="rng-panel">
      <h3 className="panel-title">
        {allCardsRevealed
          ? "🃏 Full Shuffled Deck (52 cards)"
          : `🃏 Deck Transparency (${revealedCount} cards revealed)`}
      </h3>

      <div className="deck-info-banner">
        {allCardsRevealed ? (
          <p className="info-hint">
            ✅ Game ended. All cards are now visible. Click any card to toggle
            between front/back view. Use "Verify Hash" button to independently
            verify each card's cryptographic hash.
          </p>
        ) : (
          <p className="info-hint">
            🔒 During gameplay: You can see your hole cards and community cards.
            All 52 card hashes are visible. Other cards show as face-down with
            their hash. Click "Verify Hash" on revealed cards to check their
            integrity.
          </p>
        )}
      </div>

      {/* Display all 52 cards in shuffled_position order (0-51) */}
      {/* Cards are displayed by their shuffled_position, which represents the actual deal order */}
      <div className="deck-grid">
        {Array.from({ length: 52 }, (_, index) => {
          const provenance = cardProvenance.get(index);
          if (!provenance) {
            return (
              <div key={index} className="card-item-wrapper hidden">
                <div className="card-back">
                  <CardComponent size="small" />
                  <div className="hash-overlay">Loading...</div>
                </div>
                <div className="card-position">#{index}</div>
              </div>
            );
          }
          return (
            <CardItem key={index} position={index} provenance={provenance} />
          );
        })}
      </div>

      <div className="panel-footer">
        <p className="info-hint">
          {allCardsRevealed ? (
            <>
              <strong>✅ Full Transparency:</strong> All 52 cards are visible.
              Click any card to flip between front and back. Verify hashes to
              ensure deck integrity.
            </>
          ) : (
            <>
              <strong>🔒 Partial Visibility:</strong> You can see{" "}
              {revealedCount} cards (your hole cards + community cards). All
              hashes are visible for verification. Full deck revealed after game
              ends.
            </>
          )}
        </p>

        {allCardsRevealed && (
          <div className="verification-reminder">
            <p className="info-hint">
              <strong>🔍 Verification:</strong> Click "Verify Hash" on any card
              to independently calculate its hash from the card value, suit,
              position, and round ID. A ✓ badge means the hash matches (deck
              integrity confirmed).
            </p>
          </div>
        )}
      </div>
    </section>
  );
}
