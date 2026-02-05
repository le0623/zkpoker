import classNames from 'classnames';
import { memo, useEffect, useMemo, useRef, useState } from 'react';

import type { _SERVICE } from '@declarations/table_canister/table_canister.did';
import { ButtonComponent, Interactable } from '@zk-game-dao/ui';

import { useUser } from '@lib/user';
import { CardComponent } from '../card/card.component';
import { useTable } from '../../context/table.context';
import { calculateCardHash } from '../../utils/card-hash';
import { getAllSeatAssignments } from '../../utils/seat-lookup';
import type { Card, CardProvenance, RngMetadata } from '../../types/rng.types';
import { shortenHash } from '../../types/rng.types';

export const RngDeckPanelComponent = memo<{
  rngData: RngMetadata;
  tableActor: _SERVICE;
  roundId?: bigint;
}>(({ rngData, tableActor, roundId }) => {
  const { table, user } = useTable();
  const { user: zkpUser } = useUser();

  const seatAssignments = useMemo(() => {
    if (!table) return new Map<string, number>();
    return getAllSeatAssignments(table);
  }, [table]);
  const currentUserPrincipal = useMemo(
    () => zkpUser?.principal_id?.toText(),
    [zkpUser]
  );

  const activePlayingPlayers = useMemo(() => {
    if (!table?.seats) return [];

    return table.seats
      .map((seat, index) => {
        // Only include players with Occupied status (actively at table)
        if ("Occupied" in seat) {
          const principal = seat.Occupied;
          const principalText = principal.toText();

          // Exclude current user - they already see their own cards
          if (principalText === currentUserPrincipal) {
            return null;
          }

          const userData = table.user_table_data.find(
            ([id]) => id.toText() === principalText
          )?.[1];

          if (userData) {
            const playerAction = userData.player_action;
            if (
              ("SittingOut" in playerAction) ||
              ("Joining" in playerAction)
            ) {
              return null;
            }
          }

          return {
            principal,
            principalText,
            seatNumber: index,
          };
        }
        return null;
      })
      .filter((player): player is NonNullable<typeof player> => player !== null);
  }, [table?.seats, table?.user_table_data, currentUserPrincipal]);

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

  const [selectedCardPosition, setSelectedCardPosition] = useState<
    number | null
  >(null);

  const hashListRefs = useRef<Map<number, HTMLDivElement>>(new Map());
  const hashListContainerRef = useRef<HTMLDivElement>(null);

  const gameFinished = useMemo(() => {
    if (!table) return false;
    const firstSortedUsers = table.sorted_users?.[0];
    const hasWinners =
      firstSortedUsers !== undefined && firstSortedUsers.length > 0;
    return hasWinners;
  }, [table]);

  useEffect(() => {
    async function loadCardProvenance() {
      setLoading(true);
      try {
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

  const cardMatches = (c1: Card, c2: Card): boolean => {
    const c1Value = Object.keys(c1.value)[0];
    const c1Suit = Object.keys(c1.suit)[0];
    const c2Value = Object.keys(c2.value)[0];
    const c2Suit = Object.keys(c2.suit)[0];
    return c1Value === c2Value && c1Suit === c2Suit;
  };

  const revealedCards = useMemo(() => {
    const revealed = new Set<number>();

    if (user?.data?.cards) {
      user.data.cards.forEach((card) => {
        const prov = Array.from(cardProvenance.values()).find(
          (p) => p.round_id === rngData.round_id && cardMatches(p.card, card)
        );
        if (prov) revealed.add(prov.shuffled_position);
      });
    }

    if (table?.community_cards) {
      table.community_cards.forEach((card) => {
        const prov = Array.from(cardProvenance.values()).find(
          (p) => p.round_id === rngData.round_id && cardMatches(p.card, card)
        );
        if (prov) revealed.add(prov.shuffled_position);
      });
    }

    if (table?.deal_stage) {
      const stage = Object.keys(table.deal_stage)[0];
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

    }

    return revealed;
  }, [user, table, cardProvenance, rngData.round_id]);

  const showdownData = useMemo<{
    isShowdown: boolean;
    revealOrder: string[];
    showdownPlayerCards: Map<string, Set<number>>;
  } | null>(() => {
    if (!table || !gameFinished || !cardProvenance.size) return null;

    const firstSortedUsers = table.sorted_users?.[0];
    if (!firstSortedUsers || firstSortedUsers.length === 0) return null;

    const showdownPlayersCount = firstSortedUsers.length;
    const isShowdown = showdownPlayersCount >= 2;

    if (!isShowdown) {
      return {
        isShowdown: false,
        revealOrder: [],
        showdownPlayerCards: new Map(),
      };
    }

    let firstToRevealPrincipal: string | null = null;

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

    if (!firstToRevealPrincipal && table.dealer_position !== undefined) {
      const seats = table.seats || [];
      const dealerPos = Number(table.dealer_position);
      for (let offset = 1; offset < seats.length; offset++) {
        const seatIndex = (dealerPos + offset) % seats.length;
        const seat = seats[seatIndex];
        if ("Occupied" in seat) {
          const principalText = seat.Occupied.toText();
          if (firstSortedUsers.some((uc) => uc.id.toText() === principalText)) {
            firstToRevealPrincipal = principalText;
            break;
          }
        }
      }
    }

    const revealOrder: string[] = [];
    const showdownPrincipals = new Set(
      firstSortedUsers.map((uc) => uc.id.toText())
    );

    if (firstToRevealPrincipal && table.seats) {
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

    const showdownPlayerCards = new Map<string, Set<number>>();
    for (const userCards of firstSortedUsers) {
      const principalText = userCards.id.toText();
      const cardPositions = new Set<number>();

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

  useEffect(() => {
    if (!showdownData?.isShowdown || !showdownData.revealOrder.length) {
      setRevealedShowdownPlayers(new Set());
      lastRevealOrderRef.current = "";
      return;
    }

    const currentRevealOrder = showdownData.revealOrder;
    const orderKey = currentRevealOrder.join(",");

    if (orderKey === lastRevealOrderRef.current) {
      return;
    }

    lastRevealOrderRef.current = orderKey;

    setRevealedShowdownPlayers(new Set(currentRevealOrder));
  }, [showdownData]);

  const allRevealedCards = useMemo(() => {
    const revealed = new Set(revealedCards);

    const currentUserPrincipal = zkpUser?.principal_id?.toText();
    const isCurrentUserInShowdown = Boolean(
      currentUserPrincipal &&
      showdownData?.isShowdown &&
      showdownData?.revealOrder &&
      showdownData.revealOrder.length > 0 &&
      showdownData.revealOrder.includes(currentUserPrincipal)
    );

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

  const cardPositionToPrincipal = useMemo(() => {
    const positionToPrincipal = new Map<number, string>();
    if (!table || cardProvenance.size === 0 || !rngData?.round_id) {
      return positionToPrincipal;
    }

    cardProvenance.forEach((prov) => {
      if (prov.dealt_to.length > 0 && prov.dealt_to[0]) {
        const principalText = prov.dealt_to[0].toText();
        positionToPrincipal.set(prov.shuffled_position, principalText);
      }
    });
    return positionToPrincipal;
  }, [cardProvenance, rngData?.round_id]);

  const isDummyCard = (card: Card): boolean => {
    const valueKey = Object.keys(card.value)[0];
    const suitKey = Object.keys(card.suit)[0];
    return valueKey === "Two" && suitKey === "Spade";
  };

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

    const isHoleCard = useMemo(() => {
      if (
        provenance.dealt_to.length > 0 &&
        provenance.dealt_to[0] &&
        provenance.dealt_at_stage &&
        Object.keys(provenance.dealt_at_stage).length > 0 &&
        currentUserPrincipal
      ) {
        const dealtToPrincipal = provenance.dealt_to[0].toText();
        const stage = Object.keys(provenance.dealt_at_stage)[0];
        if (dealtToPrincipal === currentUserPrincipal && stage === "Opening") {
          return true;
        }
      }

      return holeCardPositions.has(position);
    }, [
      provenance.dealt_to,
      provenance.dealt_at_stage,
      currentUserPrincipal,
      position,
      holeCardPositions,
    ]);

    const isCommunityCard = useMemo(() => {
      if (
        provenance.dealt_at_stage &&
        Object.keys(provenance.dealt_at_stage).length > 0
      ) {
        const stage = Object.keys(provenance.dealt_at_stage)[0];
        const isCommunityStage =
          stage === "Flop" || stage === "Turn" || stage === "River";
        if (isCommunityStage && provenance.dealt_to.length === 0) {
          return true;
        }
      }

      return communityCardPositions.has(position);
    }, [
      provenance.dealt_to,
      provenance.dealt_at_stage,
      position,
      communityCardPositions,
    ]);

    const isCurrentUserCard = useMemo(() => {
      if (!currentUserPrincipal) return false;

      if (provenance.dealt_to.length > 0 && provenance.dealt_to[0]) {
        return provenance.dealt_to[0].toText() === currentUserPrincipal;
      }

      const principalText = cardPositionToPrincipal.get(position);
      return principalText === currentUserPrincipal;
    }, [
      provenance.dealt_to,
      currentUserPrincipal,
      cardPositionToPrincipal,
      position,
    ]);


    const handleCardClick = async () => {
      if (!isRevealed) {
        setSelectedCardPosition(position);
        setTimeout(() => {
          const hashElement = hashListRefs.current.get(position);
          if (hashElement && hashListContainerRef.current) {
            hashElement.scrollIntoView({
              behavior: "smooth",
              block: "center",
            });
          }
        }, 100);
        return;
      }

      setSelectedCardPosition(position);

      setTimeout(() => {
        const hashElement = hashListRefs.current.get(position);
        if (hashElement && hashListContainerRef.current) {
          hashElement.scrollIntoView({
            behavior: "smooth",
            block: "center",
          });
        }
      }, 100);

      const hasBeenVerified = calculatedHashes.has(position);

      if (!hasBeenVerified && !isFlipped) {
        setVerifyingCard(position);
        try {
          const calculatedHash = await calculateCardHash(
            provenance.card,
            rngData.round_id,
            position
          );

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
        return;
      }

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
      <Interactable
        className={classNames(
          'relative cursor-pointer',
          {
            'opacity-100': isRevealed,
            'opacity-50': !isRevealed,
          }
        )}
        onClick={handleCardClick}
      >
        {!isRevealed ? (
          <div className="relative">
            <CardComponent size="small" />
            <div className="material absolute bottom-0 left-0 right-0 text-xs px-1 py-0.5 rounded-b" title={provenance.card_hash}>
              {shortenHash(provenance.card_hash, 6, 4)}
            </div>
          </div>
        ) : isFlipped ? (
          <div className="relative">
            <CardComponent size="small" />
            <div className="material absolute bottom-0 left-0 right-0 text-xs px-1 py-0.5 rounded-b" title={provenance.card_hash}>
              {shortenHash(provenance.card_hash, 6, 4)}
            </div>
          </div>
        ) : (
          <div className="relative">
            <CardComponent card={provenance.card} size="small" />
            {calculatedHashes.has(position) && (
              <div
                className="material absolute bottom-0 left-0 right-0 text-xs px-1 py-0.5 rounded-b"
                title={calculatedHashes.get(position)}
              >
                {shortenHash(calculatedHashes.get(position)!, 6, 4)}
              </div>
            )}
            {verifyingCard === position && (
              <div className="material absolute inset-0 flex items-center justify-center rounded text-xs">
                Calculating...
              </div>
            )}
          </div>
        )}

        {isHoleCard && (
          <div className="material absolute top-0 right-0 text-blue-500 text-xs font-bold rounded-full w-5 h-5 flex items-center justify-center -mt-1 -mr-1">
            H
          </div>
        )}
        {isCommunityCard && !isHoleCard && (
          <div className="material absolute top-0 right-0 text-orange-500 text-xs font-bold rounded-full w-5 h-5 flex items-center justify-center -mt-1 -mr-1">
            C
          </div>
        )}

        <div className="material absolute bottom-0 left-0 text-xs px-1 rounded-tr">#{position}</div>

        {hashVerified !== undefined &&
          calculatedHashes.has(position) &&
          !isFlipped && (
            <div
              className={classNames(
                'material absolute top-0 left-0 rounded-full w-5 h-5 flex items-center justify-center text-xs font-bold -mt-1 -ml-1',
                hashVerified ? 'text-green-500' : 'text-red-500'
              )}
            >
              {hashVerified ? '✓' : '✗'}
            </div>
          )}
      </Interactable>
    );
  };

  const allCardsRevealed = useMemo(() => {
    if (!gameFinished) return false;
    if (!showdownData?.isShowdown) {
      return true;
    }
    return (
      showdownData.revealOrder.length > 0 &&
      revealedShowdownPlayers.size >= showdownData.revealOrder.length
    );
  }, [gameFinished, showdownData, revealedShowdownPlayers.size]);

  const revealedCount = allRevealedCards.size;

  const groupedCards = useMemo(() => {
    const groups: {
      holeCards: Array<{ position: number; provenance: CardProvenance }>;
      flopCards: Array<{ position: number; provenance: CardProvenance }>;
      turnCards: Array<{ position: number; provenance: CardProvenance }>;
      riverCards: Array<{ position: number; provenance: CardProvenance }>;
      otherPlayersCards: Array<{
        position: number;
        provenance: CardProvenance;
      }>;
      remainingCards: Array<{ position: number; provenance: CardProvenance }>;
    } = {
      holeCards: [],
      flopCards: [],
      turnCards: [],
      riverCards: [],
      otherPlayersCards: [],
      remainingCards: [],
    };

    const currentRoundId = rngData?.round_id;
    const allCards: Array<{ position: number; provenance: CardProvenance }> =
      [];
    for (let position = 0; position < 52; position++) {
      const provenance = cardProvenance.get(position);
      if (
        provenance &&
        (!currentRoundId || provenance.round_id === currentRoundId)
      ) {
        allCards.push({ position, provenance });
      }
    }

    if (allCards.length === 0 && cardProvenance.size > 0) {
      Array.from(cardProvenance.entries()).forEach(([position, provenance]) => {
        if (!currentRoundId || provenance.round_id === currentRoundId) {
          allCards.push({ position, provenance });
        }
      });
    }

    for (const { position, provenance } of allCards) {
      if (holeCardPositions.has(position)) {
        groups.holeCards.push({ position, provenance });
        continue;
      }
    }

    const communityCardsByStage: {
      Flop: Array<{ position: number; provenance: CardProvenance }>;
      Turn: Array<{ position: number; provenance: CardProvenance }>;
      River: Array<{ position: number; provenance: CardProvenance }>;
    } = {
      Flop: [],
      Turn: [],
      River: [],
    };

    for (const { position, provenance } of allCards) {
      if (groups.holeCards.some((c) => c.position === position)) continue;

      if (
        provenance.dealt_at_stage &&
        Object.keys(provenance.dealt_at_stage).length > 0
      ) {
        const stage = Object.keys(provenance.dealt_at_stage)[0] as
          | "Flop"
          | "Turn"
          | "River";
        if (stage === "Flop" || stage === "Turn" || stage === "River") {
          communityCardsByStage[stage].push({ position, provenance });
          continue;
        }
      }

      if (communityCardPositions.has(position)) {
        if (table?.community_cards) {
          const cardIndex = table.community_cards.findIndex((card) => {
            const prov = Array.from(cardProvenance.values()).find(
              (p) =>
                p.round_id === rngData.round_id && cardMatches(p.card, card)
            );
            return prov?.shuffled_position === position;
          });

          if (cardIndex >= 0 && cardIndex < 3) {
            communityCardsByStage.Flop.push({ position, provenance });
            continue;
          } else if (cardIndex === 3) {
            communityCardsByStage.Turn.push({ position, provenance });
            continue;
          } else if (cardIndex === 4) {
            communityCardsByStage.River.push({ position, provenance });
            continue;
          }
        }
      }
    }

    groups.flopCards = communityCardsByStage.Flop.slice(0, 3).sort(
      (a, b) => a.position - b.position
    );
    groups.turnCards = communityCardsByStage.Turn.slice(0, 1).sort(
      (a, b) => a.position - b.position
    );
    groups.riverCards = communityCardsByStage.River.slice(0, 1).sort(
      (a, b) => a.position - b.position
    );

    const assignedPositions = new Set([
      ...groups.holeCards.map((c) => c.position),
      ...groups.flopCards.map((c) => c.position),
      ...groups.turnCards.map((c) => c.position),
      ...groups.riverCards.map((c) => c.position),
    ]);

    // Step 4: Identify other players' hole cards from dealt_to and dealt_at_stage
    // Only include cards for actively playing players (from activePlayingPlayers)
    for (const { position, provenance } of allCards) {
      if (assignedPositions.has(position)) continue;

      if (
        provenance.dealt_to.length > 0 &&
        provenance.dealt_to[0] &&
        provenance.dealt_at_stage &&
        Object.keys(provenance.dealt_at_stage).length > 0
      ) {
        const stage = Object.keys(provenance.dealt_at_stage)[0];

        if (stage === "Opening") {
          const dealtToPrincipal = provenance.dealt_to[0].toText();

          if (currentUserPrincipal && dealtToPrincipal === currentUserPrincipal) {
            continue;
          }

          const isActivePlayer = activePlayingPlayers.some(
            (p) => p.principalText === dealtToPrincipal
          );

          if (isActivePlayer) {
            groups.otherPlayersCards.push({ position, provenance });
            assignedPositions.add(position);
          }
        }
      }
    }
    groups.otherPlayersCards.sort((a, b) => a.position - b.position);

    for (const { position, provenance } of allCards) {
      if (!assignedPositions.has(position)) {
        groups.remainingCards.push({ position, provenance });
      }
    }

    groups.remainingCards.sort((a, b) => b.position - a.position);

    // Try deal order calculation if:
    // 1. No other players' cards found via dealt_to, AND
    // 2. Current user has hole cards (so we can calculate deal order), AND
    // 3. There are active players, AND
    // 4. There are enough remaining cards for all players
    // This handles cases where dealt_to might not be set yet or for players who joined mid-game
    if (
      groups.otherPlayersCards.length === 0 &&
      groups.holeCards.length > 0 &&
      activePlayingPlayers.length > 0 &&
      groups.remainingCards.length >= groups.holeCards.length * activePlayingPlayers.length
    ) {
      const totalActivePlayers = activePlayingPlayers.length + 1;
      const numHoleCards = groups.holeCards.length;
      const userHolePositions = groups.holeCards
        .map((c) => c.position)
        .sort((a, b) => b - a);

      const remainingPositionsSet = new Set(
        groups.remainingCards.map((c) => c.position)
      );

      const cardsByPlayerIndex = new Map<
        number,
        Array<{ position: number; provenance: CardProvenance }>
      >();

      for (let round = 0; round < numHoleCards; round++) {
        const userPosition = userHolePositions[round]!;

        for (let playerOffset = 1; playerOffset < totalActivePlayers; playerOffset++) {
          const playerIndex = playerOffset - 1;

          const offsets = [playerOffset, -playerOffset];

          for (const offset of offsets) {
            const calculatedPosition = userPosition + offset;

            if (
              calculatedPosition >= 0 &&
              calculatedPosition < 52 &&
              remainingPositionsSet.has(calculatedPosition) &&
              !assignedPositions.has(calculatedPosition)
            ) {
              if (!cardsByPlayerIndex.has(playerIndex)) {
                cardsByPlayerIndex.set(playerIndex, []);
              }

              const card = groups.remainingCards.find(
                (c) => c.position === calculatedPosition
              );

              if (card) {
                cardsByPlayerIndex.get(playerIndex)!.push(card);
                assignedPositions.add(calculatedPosition);
                break;
              }
            }
          }
        }
      }

      cardsByPlayerIndex.forEach((cards, playerIndex) => {
        if (
          cards.length === numHoleCards &&
          playerIndex < activePlayingPlayers.length
        ) {
          cards.forEach(({ position, provenance }) => {
            groups.otherPlayersCards.push({ position, provenance });
          });
        }
      });

      if (groups.otherPlayersCards.length > 0) {
        groups.otherPlayersCards.sort((a, b) => a.position - b.position);
      }
    }

    groups.remainingCards = groups.remainingCards.filter(
      (c) => !assignedPositions.has(c.position)
    );
    groups.remainingCards.sort((a, b) => a.position - b.position);

    return groups;
  }, [
    cardProvenance,
    holeCardPositions,
    communityCardPositions,
    table?.community_cards,
    rngData.round_id,
    cardMatches,
    currentUserPrincipal,
    cardPositionToPrincipal,
    activePlayingPlayers, // Added: dependency for filtering other players
  ]);

  const renderCardGroup = (
    title: string,
    cards: Array<{ position: number; provenance: CardProvenance }>,
    showIfEmpty = false,
    className = ""
  ) => {
    if (cards.length === 0 && !showIfEmpty) return null;

    return (
      <div className={classNames('mb-4', className)}>
        <h4 className="type-button-2 mb-2">{title}</h4>
        <div
          className={classNames(
            'grid gap-2',
            className ? 'grid-cols-2' : 'grid-cols-4'
          )}
        >
          {cards.length === 0 ? (
            <div className="type-tiny opacity-70 col-span-full">No cards yet</div>
          ) : (
            cards.map(({ position, provenance }) => (
              <CardItem
                key={position}
                position={position}
                provenance={provenance}
              />
            ))
          )}
        </div>
      </div>
    );
  };

  const renderOtherPlayersCards = (
    cards: Array<{ position: number; provenance: CardProvenance }>
  ) => {
    const cardsByPlayer = new Map<
      string,
      Array<{
        position: number;
        provenance: CardProvenance;
        seatNumber: number;
      }>
    >();

    const cardsWithPrincipal = cards.filter(
      (c) => c.provenance.dealt_to.length > 0 && c.provenance.dealt_to[0]
    );
    const cardsWithoutPrincipal = cards.filter(
      (c) => !(c.provenance.dealt_to.length > 0 && c.provenance.dealt_to[0])
    );

    cardsWithPrincipal.forEach(({ position, provenance }) => {
      const principalText = provenance.dealt_to[0]!.toText();
      const player = activePlayingPlayers.find(
        (p) => p.principalText === principalText
      );

      if (player) {
        if (!cardsByPlayer.has(principalText)) {
          cardsByPlayer.set(principalText, []);
        }
        cardsByPlayer.get(principalText)!.push({
          position,
          provenance,
          seatNumber: player.seatNumber,
        });
      }
    });

    if (cardsWithoutPrincipal.length > 0 && activePlayingPlayers.length > 0) {
      const groupedCardsResult = groupedCards;
      const userHoleCards = groupedCardsResult.holeCards;

      if (userHoleCards.length > 0 && table?.seats && currentUserPrincipal) {
        const numHoleCards = userHoleCards.length;
        const userHolePositions = userHoleCards
          .map((c) => c.position)
          .sort((a, b) => b - a);

        // Build allActiveSeats matching the same filtering logic as activePlayingPlayers
        // This ensures deal order calculation matches actual players who received cards
        const allActiveSeats: Array<{ seatIndex: number; principal: string }> = [];
        table.seats.forEach((seat, seatIndex) => {
          if ("Occupied" in seat) {
            const principal = seat.Occupied;
            const principalText = principal.toText();

            // Check if player is sitting out or joining (not actively playing)
            const userData = table.user_table_data.find(
              ([id]) => id.toText() === principalText
            )?.[1];

            if (userData) {
              const playerAction = userData.player_action;
              // Exclude players who are sitting out or joining
              if (
                ("SittingOut" in playerAction) ||
                ("Joining" in playerAction)
              ) {
                return; // Skip this player
              }
            }

            allActiveSeats.push({
              seatIndex,
              principal: principalText,
            });
          }
        });

        const userSeatIndexInDealOrder = allActiveSeats.findIndex(
          (s) => s.principal === currentUserPrincipal
        );

        if (userSeatIndexInDealOrder >= 0) {
          const calculatedCardsBySeatIndex = new Map<
            number,
            Array<{ position: number; provenance: CardProvenance }>
          >();

          const remainingPositionsSet = new Set(
            cardsWithoutPrincipal.map((c) => c.position)
          );
          const assignedSet = new Set<number>();

          for (let round = 0; round < numHoleCards; round++) {
            const userPosition = userHolePositions[round]!;

            for (let dealOrderIndex = 0; dealOrderIndex < allActiveSeats.length; dealOrderIndex++) {
              if (dealOrderIndex === userSeatIndexInDealOrder) continue;

              const seatInfo = allActiveSeats[dealOrderIndex]!;
              const dealOrderOffset = dealOrderIndex - userSeatIndexInDealOrder;
              const offsets = [dealOrderOffset, -dealOrderOffset];

              for (const offset of offsets) {
                const calculatedPosition = userPosition + offset;

                if (
                  calculatedPosition >= 0 &&
                  calculatedPosition < 52 &&
                  remainingPositionsSet.has(calculatedPosition) &&
                  !assignedSet.has(calculatedPosition)
                ) {
                  if (!calculatedCardsBySeatIndex.has(seatInfo.seatIndex)) {
                    calculatedCardsBySeatIndex.set(seatInfo.seatIndex, []);
                  }

                  const card = cardsWithoutPrincipal.find(
                    (c) => c.position === calculatedPosition
                  );

                  if (card) {
                    calculatedCardsBySeatIndex.get(seatInfo.seatIndex)!.push(card);
                    assignedSet.add(calculatedPosition);
                    break;
                  }
                }
              }
            }
          }

          calculatedCardsBySeatIndex.forEach((playerCards, seatIndex) => {
            if (playerCards.length === numHoleCards) {
              const seatInfo = allActiveSeats.find((s) => s.seatIndex === seatIndex);
              if (seatInfo && seatInfo.principal !== currentUserPrincipal) {
                const player = activePlayingPlayers.find(
                  (p) => p.principalText === seatInfo.principal
                );

                if (player) {
                  playerCards.forEach(({ position, provenance }) => {
                    if (!cardsByPlayer.has(player.principalText)) {
                      cardsByPlayer.set(player.principalText, []);
                    }
                    cardsByPlayer.get(player.principalText)!.push({
                      position,
                      provenance,
                      seatNumber: seatInfo.seatIndex,
                    });
                  });
                }
              }
            }
          });
        }
      }
    }

    const getUsername = (principalText: string): string | undefined => {
      if (!table?.users?.users) return undefined;
      const user = table.users.users.find(
        ([id]) => id.toText() === principalText
      )?.[1];
      return user?.user_name;
    };

    return (
      <div className="mb-4">
        <h4 className="type-button-2 mb-2">Other Players Cards</h4>
        <div className="space-y-4">
          {cardsByPlayer.size === 0 ? (
            <div className="type-tiny opacity-70">No cards yet</div>
          ) : (
            Array.from(cardsByPlayer.entries())
              .sort((a, b) => (a[1][0]?.seatNumber || 0) - (b[1][0]?.seatNumber || 0))
              .map(
                ([principalText, playerCards]) => {
                  const seatNumber = playerCards[0]?.seatNumber;
                  const displaySeatNumber = seatNumber !== undefined ? seatNumber + 1 : "?";
                  const username = getUsername(principalText);

                  return (
                    <div key={principalText} className="space-y-2">
                      <div className="flex items-center gap-2 type-button-3">
                        <span>
                          Seat {displaySeatNumber}
                        </span>
                        {username && (
                          <>
                            <span>-</span>
                            <span>{username}</span>
                          </>
                        )}
                      </div>
                      <div className="grid grid-cols-4 gap-2">
                        {playerCards.map(
                          ({
                            position,
                            provenance,
                            seatNumber: cardSeatNumber,
                          }) => (
                            <Interactable
                              key={position}
                              className="relative cursor-pointer"
                              onClick={() => {
                                setSelectedCardPosition(position);
                                setTimeout(() => {
                                  const hashElement =
                                    hashListRefs.current.get(position);
                                  if (hashElement && hashListContainerRef.current) {
                                    hashElement.scrollIntoView({
                                      behavior: "smooth",
                                      block: "center",
                                    });
                                  }
                                }, 100);
                              }}
                            >
                              <div className="relative">
                                <CardComponent size="small" />
                                <div
                                  className="material absolute bottom-0 left-0 right-0 text-xs px-1 py-0.5 rounded-b"
                                  title={provenance.card_hash}
                                >
                                  {shortenHash(provenance.card_hash, 6, 4)}
                                </div>
                              </div>
                              <div className="material absolute bottom-0 left-0 text-xs px-1 rounded-tr">#{position}</div>
                            </Interactable>
                          )
                        )}
                      </div>
                    </div>
                  );
                }
              )
          )}
        </div>
      </div>
    );
  };

  if (loading) {
    return (
      <div className="material rounded-lg p-4 mb-4">
        <h3 className="type-top mb-4">🃏 Deck Transparency</h3>
        <div className="type-tiny opacity-70">Loading deck data...</div>
      </div>
    );
  }

  return (
    <div className="material rounded-lg p-4 mb-4">
      <h3 className="type-top mb-4">
        {allCardsRevealed
          ? '🃏 Full Shuffled Deck (52 cards)'
          : `🃏 Deck Transparency (${revealedCount} cards revealed)`}
      </h3>

      <div className="material rounded p-3 mb-4">
        {gameFinished ? (
          showdownData?.isShowdown ? (
            allCardsRevealed ? (
              <p className="type-tiny opacity-70">
                ✅ Showdown completed. All cards are now visible. Click any card
                to verify its cryptographic hash and toggle between front/back
                view.
              </p>
            ) : (
              <p className="type-tiny opacity-70">
                🔄 Showdown in progress. Cards are being revealed in order (
                {revealedShowdownPlayers.size}/{showdownData.revealOrder.length}{' '}
                players revealed).
              </p>
            )
          ) : (
            <p className="type-tiny opacity-70">
              ✅ Game ended. Only one player remained, so no showdown occurred.
              Community cards and your cards are visible. Click any card to
              verify its cryptographic hash.
            </p>
          )
        ) : (
          <p className="type-tiny opacity-70">
            🔒 During gameplay: You can see your hole cards (marked with{' '}
            <strong className="text-blue-500">H</strong>) and community cards
            (marked with <strong className="text-orange-500">C</strong>). All
            52 card hashes are visible. Other cards show as face-down with their
            hash. Click revealed cards to verify their integrity.
          </p>
        )}
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <div className="space-y-4">
          {renderCardGroup(
            "Hole Cards",
            groupedCards.holeCards,
            true,
            "hole-cards-group"
          )}
          {renderCardGroup("Flop", groupedCards.flopCards, false)}
          {renderCardGroup("Turn", groupedCards.turnCards, false)}
          {renderCardGroup("River", groupedCards.riverCards, false)}
          {renderOtherPlayersCards(groupedCards.otherPlayersCards)}
        </div>

        <div className="material rounded p-4 max-h-[600px] overflow-y-auto" ref={hashListContainerRef}>
          <h4 className="type-button-2 mb-4">Card Hash List (52 cards)</h4>
          <div className="space-y-1">
            {Array.from({ length: 52 }, (_, index) => {
              const provenance = cardProvenance.get(index);
              const hash = provenance?.card_hash || "Loading...";
              const isHighlighted = selectedCardPosition === index;
              const isRevealed =
                provenance &&
                allRevealedCards.has(index) &&
                (!gameFinished || !isDummyCard(provenance.card));

              return (
                <div
                  key={index}
                  ref={(el) => {
                    if (el) {
                      hashListRefs.current.set(index, el);
                    } else {
                      hashListRefs.current.delete(index);
                    }
                  }}
                  className={classNames(
                    'material flex items-center gap-2 p-2 rounded cursor-pointer',
                    {
                      'opacity-100': isRevealed,
                      'opacity-50': !isRevealed,
                    }
                  )}
                  onClick={() => {
                    setSelectedCardPosition(index);
                  }}
                >
                  <span className="type-tiny font-mono">#{index}</span>
                  <span className="type-tiny font-mono flex-1">{shortenHash(hash, 8, 6)}</span>
                  {isHighlighted && <span className="text-yellow-500">⚡</span>}
                </div>
              );
            })}
          </div>
        </div>
      </div>

      <div className="mt-4 space-y-2">
        <p className="type-tiny opacity-70">
          {allCardsRevealed ? (
            <>
              <strong>✅ Full Transparency:</strong> All 52 cards are visible.
              Click any card to flip between front and back. Verify hashes to
              ensure deck integrity.
            </>
          ) : (
            <>
              <strong>🔒 Partial Visibility:</strong> You can see{' '}
              {revealedCount} cards (your hole cards + community cards). All
              hashes are visible for verification. Full deck revealed after game
              ends.
            </>
          )}
        </p>

        {allCardsRevealed && (
          <div className="space-y-2">
            <p className="type-tiny opacity-70">
              <strong>🔍 Verification:</strong> Click any card to independently
              calculate its hash from the card value, suit, position, and round
              ID. A ✓ badge means the hash matches (deck integrity confirmed).
            </p>
            <p className="type-tiny opacity-70">
              <strong>📋 Card Badges:</strong> Cards are marked with badges:{' '}
              <strong className="text-blue-500">H</strong> = Hole card (your
              private cards), <strong className="text-orange-500">C</strong> =
              Community card (shared cards on the table),{' '}
              <strong className="text-indigo-500">#N</strong> = Seat number
              (which player received the card).
            </p>
          </div>
        )}
      </div>
    </div>
  );
});
RngDeckPanelComponent.displayName = 'RngDeckPanelComponent';
