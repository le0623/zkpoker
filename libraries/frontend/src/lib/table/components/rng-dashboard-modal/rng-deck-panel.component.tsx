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
      const isDummy = isDummyCard(provenance.card);

      // ✅ FIX: Skip verification for dummy cards (hidden cards during gameplay)
      // Dummy cards have hashes calculated from real cards, so verification will always fail
      if (!hasBeenVerified && !isFlipped && !isDummy) {
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
          'relative cursor-pointer flex',
          {
            'opacity-100': isRevealed,
            'opacity-50': !isRevealed,
          }
        )}
        onClick={handleCardClick}
      >
        <div className="absolute -top-3.5 left-0 bg-black/55 text-xs px-1 rounded-t-md">#{position}</div>
        {!isRevealed ? (
          <div className="flex flex-1 items-center">
            <CardComponent className='w-18!' />
            <div className="absolute bottom-0 left-0 text-xs bg-black/85 rounded-b-md py-1 w-full" title={provenance.card_hash}>{shortenHash(provenance.card_hash, 4, 4)}</div>
          </div>
        ) : isFlipped ? (
          <div className="flex flex-1 items-center">
            <CardComponent className='w-18!' />
            <div className="absolute bottom-0 left-0 text-xs bg-black/85 rounded-b-md py-1 w-full" title={provenance.card_hash}>{shortenHash(provenance.card_hash, 4, 4)}</div>
          </div>
        ) : (
          <div className="flex flex-1 items-center">
            <CardComponent card={provenance.card} className='w-18!' />
            {calculatedHashes.has(position) && (
              <div className="absolute bottom-0 left-0 text-xs bg-black/85 rounded-b-md py-1 w-full" title={calculatedHashes.get(position)}>
                {shortenHash(calculatedHashes.get(position)!, 4, 4)}
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
          <div className="text-blue-500 bg-blue-500/30 absolute top-2 right-2 text-xs font-bold rounded-full w-5 h-5 flex items-center justify-center -mt-1 -mr-1">H</div>
        )}
        {isCommunityCard && !isHoleCard && (
          <div className="text-orange-500 bg-orange-500/30 absolute top-2 right-2 text-xs font-bold rounded-full w-5 h-5 flex items-center justify-center -mt-1 -mr-1">C</div>
        )}
        
        {hashVerified !== undefined &&
          calculatedHashes.has(position) &&
          !isFlipped && (
            <div className="absolute top-0 right-0 w-full h-full flex items-center justify-center">
              <div
                className={classNames(
                  'rounded-full w-8 h-8 flex bg-black/10 items-center justify-center text-lg font-bold',
                  hashVerified ? 'text-green-500' : 'text-red-500'
                )}
              >
                {hashVerified ? '✓' : '✗'}
              </div>
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

    // ✅ FIX: Always try deal order calculation to find missing cards
    // This ensures all players can see all other players' cards, even if dealt_to is missing
    // Only run if we have the necessary data to calculate
    if (
      groups.holeCards.length > 0 &&
      activePlayingPlayers.length > 0 &&
      groups.remainingCards.length >= groups.holeCards.length
    ) {
      const totalActivePlayers = activePlayingPlayers.length + 1;
      const numHoleCards = groups.holeCards.length;
      const userHolePositions = groups.holeCards
        .map((c) => c.position)
        .sort((a, b) => b - a);

      const remainingPositionsSet = new Set(
        groups.remainingCards.map((c) => c.position)
      );

      // Track which positions are already assigned to other players
      const alreadyAssignedToOthers = new Set(
        groups.otherPlayersCards.map((c) => c.position)
      );

      const cardsByPlayerIndex = new Map<
        number,
        Array<{ position: number; provenance: CardProvenance }>
      >();

      // ✅ FIX: Calculate cards for each round (each hole card)
      // Ensure we find exactly numHoleCards for each player
      for (let round = 0; round < numHoleCards; round++) {
        const userPosition = userHolePositions[round]!;

        // For each other player, try to find their card in this round
        for (let playerOffset = 1; playerOffset < totalActivePlayers; playerOffset++) {
          const playerIndex = playerOffset - 1;

          // Skip if this player already has enough cards for this round
          const playerCards = cardsByPlayerIndex.get(playerIndex) || [];
          if (playerCards.length > round) {
            continue; // Already found a card for this player in this round
          }

          // Calculate expected position based on deal order
          // Cards are dealt in order: player 0, player 1, player 2, etc.
          // So if user is at position X in round R, player at offset O should be at X + O
          const expectedOffset = playerOffset;

          // Try multiple offset strategies to find the card
          const offsets = [
            expectedOffset,        // Primary: exact deal order offset
            -expectedOffset,       // Fallback: reverse direction
            expectedOffset + 1,    // Nearby positions
            -(expectedOffset + 1),
            expectedOffset - 1,
            -(expectedOffset - 1),
          ];

          let foundCard = false;
          for (const offset of offsets) {
            const calculatedPosition = userPosition + offset;

            if (
              calculatedPosition >= 0 &&
              calculatedPosition < 52 &&
              remainingPositionsSet.has(calculatedPosition) &&
              !assignedPositions.has(calculatedPosition) &&
              !alreadyAssignedToOthers.has(calculatedPosition)
            ) {
              // Check if this position is already assigned to this player
              const alreadyAssignedToThisPlayer = playerCards.some(
                (c) => c.position === calculatedPosition
              );

              if (!alreadyAssignedToThisPlayer) {
                if (!cardsByPlayerIndex.has(playerIndex)) {
                  cardsByPlayerIndex.set(playerIndex, []);
                }

                const card = groups.remainingCards.find(
                  (c) => c.position === calculatedPosition
                );

                if (card) {
                  cardsByPlayerIndex.get(playerIndex)!.push(card);
                  assignedPositions.add(calculatedPosition);
                  foundCard = true;
                  break;
                }
              }
            }
          }

          // ✅ FIX: If we didn't find a card with the expected offset, try broader search
          // Search in a wider range to ensure we find cards for all players
          if (!foundCard) {
            // Try searching in a wider range around the expected position
            for (let searchRange = 1; searchRange <= 10 && !foundCard; searchRange++) {
              for (const direction of [1, -1]) {
                // Try positions around the expected offset
                const searchPosition = userPosition + (expectedOffset * direction) + (searchRange * direction);

                if (
                  searchPosition >= 0 &&
                  searchPosition < 52 &&
                  remainingPositionsSet.has(searchPosition) &&
                  !assignedPositions.has(searchPosition) &&
                  !alreadyAssignedToOthers.has(searchPosition)
                ) {
                  const playerCards = cardsByPlayerIndex.get(playerIndex) || [];
                  const alreadyAssignedToThisPlayer = playerCards.some(
                    (c) => c.position === searchPosition
                  );

                  if (!alreadyAssignedToThisPlayer) {
                    if (!cardsByPlayerIndex.has(playerIndex)) {
                      cardsByPlayerIndex.set(playerIndex, []);
                    }

                    const card = groups.remainingCards.find(
                      (c) => c.position === searchPosition
                    );

                    if (card) {
                      cardsByPlayerIndex.get(playerIndex)!.push(card);
                      assignedPositions.add(searchPosition);
                      foundCard = true;
                      break;
                    }
                  }
                }
              }
            }
          }

          // ✅ FIX: Last resort - search all remaining positions if still not found
          // This ensures we find a card for every player in every round
          if (!foundCard) {
            for (const { position, provenance } of groups.remainingCards) {
              if (
                !assignedPositions.has(position) &&
                !alreadyAssignedToOthers.has(position)
              ) {
                const playerCards = cardsByPlayerIndex.get(playerIndex) || [];
                const alreadyAssignedToThisPlayer = playerCards.some(
                  (c) => c.position === position
                );

                if (!alreadyAssignedToThisPlayer) {
                  if (!cardsByPlayerIndex.has(playerIndex)) {
                    cardsByPlayerIndex.set(playerIndex, []);
                  }

                  cardsByPlayerIndex.get(playerIndex)!.push({ position, provenance });
                  assignedPositions.add(position);
                  foundCard = true;
                  break;
                }
              }
            }
          }
        }
      }

      // ✅ FIX: Add calculated cards to otherPlayersCards, ensuring we have the right count
      // Only add cards if we found the correct number for each player
      cardsByPlayerIndex.forEach((cards, playerIndex) => {
        if (playerIndex < activePlayingPlayers.length) {
          // ✅ FIX: Only add if we found the correct number of cards (numHoleCards)
          // This ensures consistency - all players should see the same number of cards per other player
          if (cards.length === numHoleCards) {
            cards.forEach(({ position, provenance }) => {
              // Only add if not already in otherPlayersCards
              const alreadyExists = groups.otherPlayersCards.some(
                (c) => c.position === position
              );
              if (!alreadyExists) {
                groups.otherPlayersCards.push({ position, provenance });
              }
            });
          } else if (cards.length > 0) {
            // If we found some cards but not all, still add them (partial is better than nothing)
            // But log a warning for debugging
            console.warn(
              `Found ${cards.length} cards for player ${playerIndex}, expected ${numHoleCards}`
            );
            cards.forEach(({ position, provenance }) => {
              const alreadyExists = groups.otherPlayersCards.some(
                (c) => c.position === position
              );
              if (!alreadyExists) {
                groups.otherPlayersCards.push({ position, provenance });
              }
            });
          }
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
      <div className={classNames('mb-8', className)}>
        <div className="flex items-baseline gap-3 mb-5 pb-2 border-b border-slate-200/50 dark:border-slate-700/50">
          <h4 className="type-button-2 font-bold text-base">{title}</h4>
          {cards.length > 0 && (
            <span className="type-tiny opacity-60 font-mono bg-slate-100 dark:bg-slate-800 px-2 py-0.5 rounded">
              {cards.length} {cards.length === 1 ? 'card' : 'cards'}
            </span>
          )}
        </div>
        {cards.length === 0 ? (
          <div className="type-tiny opacity-50 py-6 text-center border-2 border-dashed border-slate-200 dark:border-slate-700 rounded-lg bg-slate-50/50 dark:bg-slate-800/20">
            No cards yet
          </div>
        ) : (
          <div className="flex flex-wrap gap-4">
            {cards.map(({ position, provenance }) => (
              <CardItem
                key={position}
                position={position}
                provenance={provenance}
              />
            ))}
          </div>
        )}
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

          // ✅ FIX: Calculate cards for each round (each hole card) for each player
          // This ensures we find exactly numHoleCards for each player
          for (let round = 0; round < numHoleCards; round++) {
            const userPosition = userHolePositions[round]!;

            for (let dealOrderIndex = 0; dealOrderIndex < allActiveSeats.length; dealOrderIndex++) {
              if (dealOrderIndex === userSeatIndexInDealOrder) continue;

              const seatInfo = allActiveSeats[dealOrderIndex]!;
              const dealOrderOffset = dealOrderIndex - userSeatIndexInDealOrder;

              // Skip if this player already has enough cards for this round
              const playerCards = calculatedCardsBySeatIndex.get(seatInfo.seatIndex) || [];
              if (playerCards.length > round) {
                continue; // Already found a card for this player in this round
              }

              // ✅ FIX: Try multiple offset strategies to find cards for all players
              const offsets = [
                dealOrderOffset,
                -dealOrderOffset,
                dealOrderOffset + 1,
                -(dealOrderOffset + 1),
                dealOrderOffset - 1,
                -(dealOrderOffset - 1),
              ];

              let foundCard = false;
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
                    foundCard = true;
                    break;
                  }
                }
              }

              // ✅ FIX: If standard offsets didn't work, try searching nearby positions
              if (!foundCard) {
                for (let searchOffset = 1; searchOffset <= 10 && !foundCard; searchOffset++) {
                  for (const direction of [1, -1]) {
                    const searchPosition = userPosition + (dealOrderOffset * direction) + (searchOffset * direction);

                    if (
                      searchPosition >= 0 &&
                      searchPosition < 52 &&
                      remainingPositionsSet.has(searchPosition) &&
                      !assignedSet.has(searchPosition)
                    ) {
                      if (!calculatedCardsBySeatIndex.has(seatInfo.seatIndex)) {
                        calculatedCardsBySeatIndex.set(seatInfo.seatIndex, []);
                      }

                      const card = cardsWithoutPrincipal.find(
                        (c) => c.position === searchPosition
                      );

                      if (card) {
                        calculatedCardsBySeatIndex.get(seatInfo.seatIndex)!.push(card);
                        assignedSet.add(searchPosition);
                        foundCard = true;
                        break;
                      }
                    }
                  }
                }
              }

              // ✅ FIX: Last resort - search all remaining positions if still not found
              // This ensures we find a card for every player in every round
              if (!foundCard) {
                for (const { position, provenance } of cardsWithoutPrincipal) {
                  if (!assignedSet.has(position)) {
                    if (!calculatedCardsBySeatIndex.has(seatInfo.seatIndex)) {
                      calculatedCardsBySeatIndex.set(seatInfo.seatIndex, []);
                    }
                    calculatedCardsBySeatIndex.get(seatInfo.seatIndex)!.push({ position, provenance });
                    assignedSet.add(position);
                    foundCard = true;
                    break;
                  }
                }
              }
            }
          }

          // ✅ FIX: Add calculated players' cards, ensuring consistency
          // Only add if we found the correct number of cards (numHoleCards) for consistency
          calculatedCardsBySeatIndex.forEach((playerCards, seatIndex) => {
            const seatInfo = allActiveSeats.find((s) => s.seatIndex === seatIndex);
            if (seatInfo && seatInfo.principal !== currentUserPrincipal) {
              const player = activePlayingPlayers.find(
                (p) => p.principalText === seatInfo.principal
              );

              if (player && playerCards.length > 0) {
                // ✅ FIX: Only add if we found the correct number of cards
                // This ensures all players see the same number of cards per other player
                if (playerCards.length === numHoleCards) {
                  playerCards.forEach(({ position, provenance }) => {
                    // Check if already in cardsByPlayer to avoid duplicates
                    const existingCards = cardsByPlayer.get(player.principalText) || [];
                    const alreadyExists = existingCards.some(
                      (c) => c.position === position
                    );

                    if (!alreadyExists) {
                      if (!cardsByPlayer.has(player.principalText)) {
                        cardsByPlayer.set(player.principalText, []);
                      }
                      cardsByPlayer.get(player.principalText)!.push({
                        position,
                        provenance,
                        seatNumber: seatInfo.seatIndex,
                      });
                    }
                  });
                } else if (playerCards.length > 0) {
                  // If we found some cards but not all, still add them (partial is better than nothing)
                  console.warn(
                    `Found ${playerCards.length} cards for player ${seatInfo.principal}, expected ${numHoleCards}`
                  );
                  playerCards.forEach(({ position, provenance }) => {
                    const existingCards = cardsByPlayer.get(player.principalText) || [];
                    const alreadyExists = existingCards.some(
                      (c) => c.position === position
                    );

                    if (!alreadyExists) {
                      if (!cardsByPlayer.has(player.principalText)) {
                        cardsByPlayer.set(player.principalText, []);
                      }
                      cardsByPlayer.get(player.principalText)!.push({
                        position,
                        provenance,
                        seatNumber: seatInfo.seatIndex,
                      });
                    }
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
      <div className="mb-8">
        <div className="flex items-baseline gap-3 mb-4 pb-2 border-b border-slate-200/50 dark:border-slate-700/50">
          <h4 className="type-button-2 font-bold text-base">Other Players Cards</h4>
          {cardsByPlayer.size > 0 && (
            <span className="type-tiny opacity-60 font-mono bg-slate-100 dark:bg-slate-800 px-2 py-0.5 rounded">
              {cardsByPlayer.size} {cardsByPlayer.size === 1 ? 'player' : 'players'}
            </span>
          )}
        </div>
        <div className="space-y-6">
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
                    <div key={principalText} className="space-y-4 pb-4 border-b border-slate-100 dark:border-slate-800 last:border-0 last:pb-0">
                      <div className="flex items-center gap-3">
                        <span className="type-button-3  bg-slate-100 dark:bg-slate-800/80 px-3 py-1 rounded text-sm">
                          Seat {displaySeatNumber}
                        </span>
                        {username && (
                          <>
                            <span className="opacity-30">•</span>
                            <span className="type-button-3 font-medium text-sm">{username}</span>
                          </>
                        )}
                      </div>
                      <div className="flex flex-wrap gap-2 pl-2">
                        {playerCards.map(
                          ({
                            position,
                            provenance,
                            seatNumber: cardSeatNumber,
                          }) => (
                            <Interactable
                              key={position}
                              className="relative cursor-pointer flex opacity-50"
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
                              <div className="absolute -top-3.5 left-0 bg-black/55 text-xs px-1 rounded-t-md">#{position}</div>
                              <div className="flex flex-1 items-center">
                                <CardComponent className='w-18!' />
                                <div className="absolute bottom-0 left-0 text-xs bg-black/85 rounded-b-md py-1 w-full" title={provenance.card_hash}>
                                  {shortenHash(provenance.card_hash, 4, 4)}
                                </div>
                              </div>
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
      <div className="material rounded-xl p-6 mb-5 bg-transparent">
        <h3 className="type-top mb-2 flex items-center gap-2 font-semibold tracking-wide">
          🃏 Deck Transparency
        </h3>
        <div className="type-tiny opacity-70">Loading deck data...</div>
      </div>
    );
  }

  return (
    <div className="material rounded-xl p-6 mb-5 bg-transparent">
      <h3 className="type-top mb-2 flex items-center gap-2 font-semibold tracking-wide">
        🃏 Deck Transparency
      </h3>
      <p className="type-tiny opacity-70 mb-6">
        {allCardsRevealed
          ? 'All 52 cards are visible'
          : `${revealedCount} of 52 cards revealed`}
      </p>

      <div className="material rounded-lg p-5 mb-6 bg-blue-50/30 dark:bg-blue-950/10 border border-blue-200/50 dark:border-blue-800/30">
        {gameFinished ? (
          showdownData?.isShowdown ? (
            allCardsRevealed ? (
              <div className="space-y-2">
                <p className="type-tiny font-medium text-green-700 dark:text-green-400">
                  ✅ Showdown completed. All cards are now visible.
                </p>
                <p className="type-tiny opacity-80">
                  Click any card to verify its cryptographic hash and toggle between front/back view.
                </p>
              </div>
            ) : (
              <div className="space-y-2">
                <p className="type-tiny font-medium text-blue-700 dark:text-blue-400">
                  🔄 Showdown in progress
                </p>
                <p className="type-tiny opacity-80">
                  Cards are being revealed in order ({revealedShowdownPlayers.size}/{showdownData.revealOrder.length} players revealed).
                </p>
              </div>
            )
          ) : (
            <div className="space-y-2">
              <p className="type-tiny font-medium text-green-700 dark:text-green-400">
                ✅ Game ended
              </p>
              <p className="type-tiny opacity-80">
                Only one player remained, so no showdown occurred. Community cards and your cards are visible. Click any card to verify its cryptographic hash.
              </p>
            </div>
          )
        ) : (
          <div className="space-y-2">
            <p className="type-tiny font-medium text-slate-700 dark:text-slate-300">
              🔒 During gameplay
            </p>
            <p className="type-tiny opacity-80">
              You can see your hole cards (marked with{' '}
              <span className="inline-flex items-center justify-center w-6 h-6 bg-blue-600 text-white text-[10px] font-bold rounded-full mx-1 shadow-md">H</span>) and community cards
              (marked with <span className="inline-flex items-center justify-center w-6 h-6 bg-orange-600 text-white text-[10px] font-bold rounded-full mx-1 shadow-md">C</span>). All
              52 card hashes are visible. Other cards show as face-down with their
              hash. Click revealed cards to verify their integrity.
            </p>
          </div>
        )}
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
        <div className="space-y-8 col-span-1 lg:col-span-2">
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

        <div className="material rounded-lg px-4 py-2 h-full overflow-y-auto" ref={hashListContainerRef}>
          <div className="flex items-center gap-2 mb-2 pb-2 border-b border-slate-200/50 dark:border-slate-700/50">
            <h4 className="font-bold text-base">Card Hashes</h4>
            <span className="type-tiny opacity-60 font-mono bg-slate-100 dark:bg-slate-800 px-2 py-0.5 rounded">52</span>
          </div>
          <div>
            {Array.from({ length: 52 }, (_, i) => {
              const prov = cardProvenance.get(i);
              const hash = prov?.card_hash || "…";
              const highlighted = selectedCardPosition === i;
              const revealed = prov && allRevealedCards.has(i) && (!gameFinished || !isDummyCard(prov.card));
              return (
                <div
                  key={i}
                  ref={el => {
                    if (el) hashListRefs.current.set(i, el)
                    else hashListRefs.current.delete(i)
                  }}
                  className={classNames(
                    "flex items-center gap-2 rounded cursor-pointer transition-all",
                    highlighted
                      ? "bg-blue-100 dark:bg-blue-900/30 border-2 border-blue-500 dark:border-blue-500 shadow"
                      : "hover:bg-slate-100 dark:hover:bg-slate-800/40 border border-transparent",
                    revealed ? "opacity-100" : "opacity-50"
                  )}
                  onClick={() => setSelectedCardPosition(i)}
                >
                  <span className="font-mono text-xs font-bold w-6 text-right text-slate-600 dark:text-slate-300">#{i}</span>
                  <span className="font-mono text-xs flex-1 text-slate-900 dark:text-slate-100">{shortenHash(hash, 12, 12)}</span>
                  {highlighted && <span className="text-yellow-500 animate-pulse text-base">⚡</span>}
                  {!highlighted && revealed && <span className="text-green-500 text-xs">●</span>}
                </div>
              )
            })}
          </div>
        </div>
      </div>

      <div className="mt-8 pt-6 border-t border-slate-200/50 dark:border-slate-700/50">
        <div className="space-y-4">
          {allCardsRevealed ? (
            <>
              <div className="flex items-start gap-3">
                <span className="text-lg">✅</span>
                <div>
                  <p className="type-tiny font-semibold mb-1">Full Transparency</p>
                  <p className="type-tiny opacity-80">
                    All 52 cards are visible. Click any card to flip between front and back. Verify hashes to ensure deck integrity.
                  </p>
                </div>
              </div>
              <div className="space-y-3 pl-8">
                <div>
                  <p className="type-tiny font-semibold mb-1.5">🔍 Verification</p>
                  <p className="type-tiny opacity-80">
                    Click any card to independently calculate its hash from the card value, suit, position, and round ID. A ✓ badge means the hash matches (deck integrity confirmed).
                  </p>
                </div>
                <div>
                  <p className="type-tiny font-semibold mb-1.5">📋 Card Badges</p>
                  <div className="flex flex-wrap items-center gap-2 type-tiny opacity-80">
                    <span className="inline-flex items-center gap-1.5">
                      <span className="inline-flex items-center justify-center w-6 h-6 bg-blue-600 text-white text-[10px] font-bold rounded-full shadow-md">HOLE</span>
                      <span>= Hole card (your private cards)</span>
                    </span>
                    <span className="inline-flex items-center gap-1.5">
                      <span className="inline-flex items-center justify-center w-6 h-6 bg-orange-600 text-white text-[10px] font-bold rounded-full shadow-md">COMM</span>
                      <span>= Community card</span>
                    </span>
                    <span className="inline-flex items-center gap-1.5">
                      <span className="inline-flex items-center justify-center px-2 py-0.5 bg-slate-900 text-white text-[10px] font-mono rounded shadow-md">#N</span>
                      <span>= Position in deck</span>
                    </span>
                  </div>
                </div>
              </div>
            </>
          ) : (
            <div className="flex items-start gap-3">
              <span className="text-lg">🔒</span>
              <div>
                <p className="type-tiny font-semibold mb-1">Partial Visibility</p>
                <p className="type-tiny opacity-80">
                  You can see {revealedCount} cards (your hole cards + community cards). All hashes are visible for verification. Full deck revealed after game ends.
                </p>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
});
RngDeckPanelComponent.displayName = 'RngDeckPanelComponent';
