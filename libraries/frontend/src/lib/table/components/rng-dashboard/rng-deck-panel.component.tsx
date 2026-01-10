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
}

export function RngDeckPanel({ rngData, tableActor }: RngDeckPanelProps) {
  const { table, user } = useTable();
  const { user: zkpUser } = useUser();
  const [cardHashes, setCardHashes] = useState<Map<number, string>>(new Map());
  const [loadingHashes, setLoadingHashes] = useState(true);

  // Fetch card hashes for all 52 cards
  useEffect(() => {
    async function loadCardHashes() {
      setLoadingHashes(true);
      try {
        // 🔒 SECURITY: If shuffled_deck is empty, the game is still ongoing
        // We'll fetch card hashes from the backend's card_provenance instead
        if (rngData.shuffled_deck.length === 0) {
          // Game is ongoing - fetch card provenance for hashes
          const provenanceResult = await tableActor.get_all_card_provenance();
          console.log("provenanceResult", provenanceResult);
          if ("Ok" in provenanceResult) {
            const hashes = new Map<number, string>();
            provenanceResult.Ok.forEach((prov) => {
              hashes.set(prov.shuffled_position, prov.card_hash);
            });
            setCardHashes(hashes);
          }
        } else {
          // Game has ended - calculate hashes from revealed deck
          const hashes = new Map<number, string>();
          const hashPromises = rngData.shuffled_deck.map((card, position) =>
            calculateCardHash(card, rngData.round_id, position).then(
              (hash) => ({
                position,
                hash,
              })
            )
          );
          const results = await Promise.all(hashPromises);
          results.forEach(({ position, hash }) => {
            hashes.set(position, hash);
          });
          setCardHashes(hashes);
        }
      } catch (err) {
        console.error("Failed to calculate card hashes:", err);
      } finally {
        setLoadingHashes(false);
      }
    }

    if (rngData) {
      loadCardHashes();
    }
  }, [rngData.round_id, rngData.shuffled_deck.length, tableActor]);

  // Check if game has finished
  const gameFinished = useMemo(() => {
    if (!table) return false;
    // Game is finished if there are winners (sorted_users has entries)
    const firstSortedUsers = table.sorted_users?.[0];
    const hasWinners =
      firstSortedUsers !== undefined && firstSortedUsers.length > 0;
    return hasWinners;
  }, [table]);

  // Check if we're in Showdown (all active player cards should be revealed)
  const isShowdown = useMemo(() => {
    if (!table) return false;
    return "Showdown" in table.deal_stage;
  }, [table]);

  // Get revealed cards during gameplay
  const revealedCards = useMemo(() => {
    // 🔒 SECURITY: If deck is empty (game ongoing) or game finished, return empty
    if (!table || gameFinished || rngData.shuffled_deck.length === 0) return [];

    const revealed: {
      card: Card;
      position: number;
      type: "hole" | "community";
    }[] = [];
    const usedPositions = new Set<number>();

    // Helper to find card position in shuffled deck (avoid duplicates)
    const findCardPosition = (card: Card, startFrom = 0): number => {
      for (let i = startFrom; i < rngData.shuffled_deck.length; i++) {
        if (usedPositions.has(i)) continue;
        const deckCard = rngData.shuffled_deck[i];
        if (
          Object.keys(deckCard.value)[0] === Object.keys(card.value)[0] &&
          Object.keys(deckCard.suit)[0] === Object.keys(card.suit)[0]
        ) {
          return i;
        }
      }
      return -1;
    };

    // Add user's hole cards (if they're in the game)
    if (user?.data?.cards && zkpUser) {
      user.data.cards.forEach((card) => {
        const position = findCardPosition(card);
        if (position >= 0) {
          usedPositions.add(position);
          revealed.push({ card, position, type: "hole" });
        }
      });
    }

    // In Showdown, add all active players' hole cards
    if (isShowdown && table.user_table_data) {
      table.user_table_data.forEach(([principal, userData]) => {
        // Skip if it's the current user (already added above)
        if (zkpUser && principal.compareTo(zkpUser.principal_id) === "eq")
          return;

        // Skip folded or sitting out players
        if (
          "Folded" in userData.player_action ||
          "SittingOut" in userData.player_action
        )
          return;

        // Add their cards if available
        if (userData.cards) {
          userData.cards.forEach((card) => {
            const position = findCardPosition(card);
            if (position >= 0) {
              usedPositions.add(position);
              revealed.push({ card, position, type: "hole" });
            }
          });
        }
      });
    }

    // Add community cards
    if (table.community_cards) {
      table.community_cards.forEach((card) => {
        const position = findCardPosition(card);
        if (position >= 0) {
          usedPositions.add(position);
          revealed.push({ card, position, type: "community" });
        }
      });
    }

    return revealed;
  }, [table, user, zkpUser, rngData.shuffled_deck, gameFinished, isShowdown]);

  // Function to check if a card at position is revealed
  const isCardRevealed = (
    position: number
  ): { revealed: boolean; type?: "hole" | "community" } => {
    if (gameFinished) {
      return { revealed: true }; // All cards revealed after game
    }
    const revealed = revealedCards.find((r) => r.position === position);
    return revealed
      ? { revealed: true, type: revealed.type }
      : { revealed: false };
  };

  const allCardsRevealed = gameFinished && rngData.shuffled_deck.length > 0;
  const deckIsHidden = rngData.shuffled_deck.length === 0;

  return (
    <section className="rng-panel">
      <h3 className="panel-title">
        {allCardsRevealed
          ? "🃏 Full Shuffled Deck (52 cards)"
          : deckIsHidden
            ? "🔒 Deck Hashes (52 cards - deck hidden until game ends)"
            : `🃏 Deck with Card Hashes (${revealedCards.length} cards revealed)`}
      </h3>

      <div className="deck-info-banner">
        {allCardsRevealed ? (
          <p className="info-hint">
            ✅ Game ended. All cards are now visible. Verify that each card
            matches its hash.
          </p>
        ) : deckIsHidden ? (
          <p className="info-hint">
            🔒 <strong>Security:</strong> The deck is hidden during gameplay to
            ensure fairness. You can see the cryptographic hash of each card
            position. After the game ends, the full deck will be revealed for
            verification.
          </p>
        ) : (
          <p className="info-hint">
            🔒 All 52 card hashes are shown below. Revealed cards show actual
            values. Hidden cards show only their hash. After game ends, verify
            all cards match their hashes.
          </p>
        )}
      </div>

      {deckIsHidden && (
        <div className="deck-grid">
          {/* Show 52 card hash placeholders when deck is hidden */}
          {Array.from({ length: 52 }, (_, index) => {
            const cardHash = cardHashes.get(index);
            return (
              <div
                key={index}
                className="deck-card-item-wrapper"
                title={`Card Hash: ${cardHash || "Loading..."}`}
              >
                <div className="deck-card-hash-overlay">
                  {cardHash ? shortenHash(cardHash, 6, 4) : "..."}
                </div>
                <CardComponent size="microscopic" />
                <div className="card-position">#{index}</div>
              </div>
            );
          })}
        </div>
      )}

      {!deckIsHidden && (
        <div className="deck-grid">
          {rngData.shuffled_deck.map((card, index) => {
            const cardStatus = isCardRevealed(index);
            const isRevealed = cardStatus.revealed || allCardsRevealed;
            const cardHash = cardHashes.get(index);
            const displayHash = cardHash ? shortenHash(cardHash, 6, 4) : "...";

            return (
              <div
                key={index}
                className={`deck-card-item-wrapper ${
                  cardStatus.type === "hole" ? "hole-card" : ""
                } ${cardStatus.type === "community" ? "community-card" : ""}`}
                title={cardHash ? `Hash: ${cardHash}` : "Calculating hash..."}
              >
                {/* Show card with hash overlay */}
                {isRevealed ? (
                  <>
                    <CardComponent
                      card={card}
                      size="microscopic"
                      hash={cardHash}
                    />
                    {cardHash && (
                      <div className="card-hash-overlay" title={cardHash}>
                        {displayHash}
                      </div>
                    )}
                  </>
                ) : (
                  <>
                    <CardComponent size="microscopic" />
                    <div className="deck-card-hash-overlay">
                      {loadingHashes ? "..." : displayHash}
                    </div>
                  </>
                )}
                <div className="card-position">
                  <span>#{index}</span>
                  {cardStatus.type && (
                    <span className="card-type-badge">
                      {cardStatus.type === "hole" ? "H" : "C"}
                    </span>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}

      <div className="panel-footer">
        <p className="info-hint">
          {allCardsRevealed ? (
            <>
              <strong>✅ Full Transparency:</strong> All 52 cards are visible.
              Each card shows both its value and hash. Verify that each card's
              hash matches its committed hash. Any mismatch would indicate
              tampering.
            </>
          ) : (
            <>
              <strong>🔒 Hash Commitment:</strong> All 52 card hashes are
              visible in deck order. Revealed cards (hole cards marked "H",
              community cards marked "C") show both value and hash. Hidden cards
              show only their hash. After the game ends, verify all cards match
              their hashes.
            </>
          )}
        </p>

        {!allCardsRevealed && revealedCards.length > 0 && (
          <div className="revealed-summary">
            <div className="summary-item">
              <span className="summary-label">Hole Cards Revealed:</span>
              <span className="summary-value">
                {revealedCards.filter((r) => r.type === "hole").length}
              </span>
            </div>
            <div className="summary-item">
              <span className="summary-label">Community Cards Revealed:</span>
              <span className="summary-value">
                {revealedCards.filter((r) => r.type === "community").length}
              </span>
            </div>
            <div className="summary-item">
              <span className="summary-label">Total Hashes Visible:</span>
              <span className="summary-value">52</span>
            </div>
            <div className="summary-item">
              <span className="summary-label">Cards Hidden:</span>
              <span className="summary-value">{52 - revealedCards.length}</span>
            </div>
          </div>
        )}

        {allCardsRevealed && (
          <div className="verification-reminder">
            <p className="info-hint">
              <strong>🔍 Verification:</strong> You can now verify each card's
              hash matches the committed hash. Hover over any card to see its
              full hash. All hashes were committed before any cards were dealt.
            </p>
          </div>
        )}
      </div>
    </section>
  );
}
