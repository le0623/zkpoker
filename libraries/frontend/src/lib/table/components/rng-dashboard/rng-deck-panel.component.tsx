import { useMemo, useState, useEffect, useRef } from "react";
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
  const [revealedShowdownPlayers, setRevealedShowdownPlayers] = useState<
    Set<string>
  >(new Set());
  const lastRevealOrderRef = useRef<string>("");

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

    // 4. After game ends, reveal showdown cards progressively
    // (See showdown reveal logic below - handled separately)

    return revealed;
  }, [user, table, cardProvenance, rngData.round_id]);

  // Showdown reveal logic
  const showdownData = useMemo<{
    isShowdown: boolean;
    revealOrder: string[];
    showdownPlayerCards: Map<string, Set<number>>;
  } | null>(() => {
    if (!table || !gameFinished || !cardProvenance.size) return null;

    const firstSortedUsers = table.sorted_users?.[0];
    if (!firstSortedUsers || firstSortedUsers.length === 0) return null;

    // Check if showdown happened (2+ players remained)
    const showdownPlayersCount = firstSortedUsers.length;
    const isShowdown = showdownPlayersCount >= 2;

    // If only 1 player remained, no showdown - don't reveal cards
    if (!isShowdown) {
      return {
        isShowdown: false,
        revealOrder: [],
        showdownPlayerCards: new Map(),
      };
    }

    // Determine reveal order
    let firstToRevealPrincipal: string | null = null;

    // Find last aggressor (last Bet/Raise on River)
    let riverStageIndex = -1;
    for (let i = table.action_logs.length - 1; i >= 0; i--) {
      const log = table.action_logs[i];
      if ("Stage" in log.action_type) {
        const stage = Object.keys(log.action_type.Stage.stage)[0];
        if (stage === "River") {
          riverStageIndex = i;
          break;
        }
      }
    }

    // Look for last Bet or Raise after River stage started
    if (riverStageIndex >= 0) {
      for (let i = table.action_logs.length - 1; i > riverStageIndex; i--) {
        const log = table.action_logs[i];
        if (log.user_principal[0]) {
          if ("Bet" in log.action_type || "Raise" in log.action_type) {
            firstToRevealPrincipal = log.user_principal[0].toText();
            break;
          }
        }
      }
    }

    // If no bets on River, use player left of dealer
    if (!firstToRevealPrincipal && table.dealer_position !== undefined) {
      const seats = table.seats || [];
      const dealerPos = Number(table.dealer_position);
      // Find next occupied seat after dealer
      for (let offset = 1; offset < seats.length; offset++) {
        const seatIndex = (dealerPos + offset) % seats.length;
        const seat = seats[seatIndex];
        if ("Occupied" in seat) {
          // Check if this player is in showdown
          const principalText = seat.Occupied.toText();
          if (firstSortedUsers.some((uc) => uc.id.toText() === principalText)) {
            firstToRevealPrincipal = principalText;
            break;
          }
        }
      }
    }

    // Build reveal order (clockwise from first to reveal)
    const revealOrder: string[] = [];
    const showdownPrincipals = new Set(
      firstSortedUsers.map((uc) => uc.id.toText())
    );

    if (firstToRevealPrincipal && table.seats) {
      // Find starting position
      let startIndex = -1;
      for (let i = 0; i < table.seats.length; i++) {
        const seat = table.seats[i];
        if (
          "Occupied" in seat &&
          seat.Occupied.toText() === firstToRevealPrincipal
        ) {
          startIndex = i;
          break;
        }
      }

      // Collect in clockwise order
      if (startIndex >= 0) {
        for (let offset = 0; offset < table.seats.length; offset++) {
          const seatIndex = (startIndex + offset) % table.seats.length;
          const seat = table.seats[seatIndex];
          if ("Occupied" in seat) {
            const principalText = seat.Occupied.toText();
            if (showdownPrincipals.has(principalText)) {
              revealOrder.push(principalText);
            }
          }
        }
      }
    }

    // Map showdown players to their card positions
    const showdownPlayerCards = new Map<string, Set<number>>();
    for (const userCards of firstSortedUsers) {
      const principalText = userCards.id.toText();
      const cardPositions = new Set<number>();

      // Get player's cards from user_table_data
      const userData = table.user_table_data.find(
        ([principal]) => principal.toText() === principalText
      )?.[1];

      if (userData?.cards) {
        userData.cards.forEach((card) => {
          const prov = Array.from(cardProvenance.values()).find(
            (p) => p.round_id === rngData.round_id && cardMatches(p.card, card)
          );
          if (prov) {
            cardPositions.add(prov.shuffled_position);
          }
        });
      }

      if (cardPositions.size > 0) {
        showdownPlayerCards.set(principalText, cardPositions);
      }
    }

    return {
      isShowdown: true,
      revealOrder,
      showdownPlayerCards,
    };
  }, [table, gameFinished, cardProvenance, rngData.round_id]);

  // Progressively reveal showdown players
  useEffect(() => {
    if (!showdownData?.isShowdown || !showdownData.revealOrder.length) {
      // Reset if no showdown
      setRevealedShowdownPlayers(new Set());
      lastRevealOrderRef.current = "";
      return;
    }

    const currentRevealOrder = showdownData.revealOrder;
    const orderKey = currentRevealOrder.join(",");

    // Only start reveal if order changed
    if (orderKey === lastRevealOrderRef.current) {
      return;
    }

    lastRevealOrderRef.current = orderKey;

    // Reset and start progressive reveal
    setRevealedShowdownPlayers(new Set());

    // Progressive reveal: reveal one player every 1.5 seconds
    const timeouts: NodeJS.Timeout[] = [];
    for (let i = 0; i < currentRevealOrder.length; i++) {
      const principal = currentRevealOrder[i];
      const timeout = setTimeout(
        () => {
          setRevealedShowdownPlayers((prev) => {
            const next = new Set(prev);
            next.add(principal);
            return next;
          });
        },
        (i + 1) * 1500
      ); // 1.5 second delay between each reveal (start after 1.5s)
      timeouts.push(timeout);
    }

    return () => {
      timeouts.forEach((timeout) => clearTimeout(timeout));
    };
  }, [showdownData]);

  // Update revealedCards to include showdown cards
  const allRevealedCards = useMemo(() => {
    const revealed = new Set(revealedCards);

    // SECURITY: Only reveal showdown cards if current user is in the showdown
    // Folded players should not see showdown cards
    const currentUserPrincipal = zkpUser?.principal_id?.toText();
    const isCurrentUserInShowdown = Boolean(
      currentUserPrincipal &&
      showdownData?.isShowdown &&
      showdownData?.revealOrder &&
      showdownData.revealOrder.length > 0 &&
      showdownData.revealOrder.includes(currentUserPrincipal)
    );

    // Add showdown players' cards that have been revealed (only if user is in showdown)
    if (
      isCurrentUserInShowdown &&
      showdownData?.isShowdown &&
      showdownData.showdownPlayerCards
    ) {
      for (const [
        principal,
        cardPositions,
      ] of showdownData.showdownPlayerCards.entries()) {
        if (revealedShowdownPlayers.has(principal)) {
          cardPositions.forEach((pos: number) => revealed.add(pos));
        }
      }
    }

    return revealed;
  }, [
    revealedCards,
    showdownData,
    revealedShowdownPlayers,
    zkpUser?.principal_id,
  ]);

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
    const isRevealed =
      allRevealedCards.has(position) &&
      (!gameFinished || !isDummyCard(provenance.card));
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

  // Check if all showdown cards have been revealed (if showdown happened)
  const allCardsRevealed = useMemo(() => {
    if (!gameFinished) return false;
    if (!showdownData?.isShowdown) {
      // If no showdown (only 1 player), all cards are "revealed" (none to reveal)
      return true;
    }
    // Check if all showdown players have been revealed
    return (
      showdownData.revealOrder.length > 0 &&
      revealedShowdownPlayers.size >= showdownData.revealOrder.length
    );
  }, [gameFinished, showdownData, revealedShowdownPlayers.size]);

  const revealedCount = allRevealedCards.size;

  if (loading) {
    return (
      <section className="rng-panel">
        <h3 className="panel-title">🃏 Deck Transparency</h3>
        <div className="loading-state">Loading deck data...</div>
      </section>
    );
  }

  return (
    <section className="rng-panel">
      <h3 className="panel-title">
        {allCardsRevealed
          ? "🃏 Full Shuffled Deck (52 cards)"
          : `🃏 Deck Transparency (${revealedCount} cards revealed)`}
      </h3>

      <div className="deck-info-banner">
        {gameFinished ? (
          showdownData?.isShowdown ? (
            allCardsRevealed ? (
              <p className="info-hint">
                ✅ Showdown completed. All cards are now visible. Click any card
                to verify its cryptographic hash and toggle between front/back
                view.
              </p>
            ) : (
              <p className="info-hint">
                🔄 Showdown in progress. Cards are being revealed in order (
                {revealedShowdownPlayers.size}/{showdownData.revealOrder.length}{" "}
                players revealed).
              </p>
            )
          ) : (
            <p className="info-hint">
              ✅ Game ended. Only one player remained, so no showdown occurred.
              Community cards and your cards are visible. Click any card to
              verify its cryptographic hash.
            </p>
          )
        ) : (
          <p className="info-hint">
            🔒 During gameplay: You can see your hole cards (marked with{" "}
            <strong style={{ color: "#3b82f6" }}>H</strong>) and community cards
            (marked with <strong style={{ color: "#f59e0b" }}>C</strong>). All
            52 card hashes are visible. Other cards show as face-down with their
            hash. Click revealed cards to verify their integrity.
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
              <strong>🔍 Verification:</strong> Click any card to independently
              calculate its hash from the card value, suit, position, and round
              ID. A ✓ badge means the hash matches (deck integrity confirmed).
            </p>
            <p className="info-hint" style={{ marginTop: "0.5rem" }}>
              <strong>📋 Card Badges:</strong> Cards are marked with badges:{" "}
              <strong style={{ color: "#3b82f6" }}>H</strong> = Hole card (your
              private cards), <strong style={{ color: "#f59e0b" }}>C</strong> =
              Community card (shared cards on the table).
            </p>
          </div>
        )}
      </div>
    </section>
  );
}
