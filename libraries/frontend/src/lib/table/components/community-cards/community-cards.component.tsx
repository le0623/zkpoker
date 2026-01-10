import classNames from "classnames";
import { motion } from "framer-motion";
import { forwardRef, memo, useEffect, useMemo, useRef, useState } from "react";

import { useSound } from "@/src/context/sound.context";
import { Card } from "@declarations/table_canister/table_canister.did";

import { CardComponent } from "../card/card.component";
import { IsSameCard, IsSameHand } from "../../../utils/compare";
import { useCardHashes } from "../../hooks/use-card-hash";
import { useTable } from "../../context/table.context";

const CardIndent = forwardRef<HTMLDivElement, { className?: string }>(
  ({ className }, ref) => (
    <div
      ref={ref}
      className={classNames(
        "w-[58px] aspect-card shadow-card-slot rounded-[6px]",
        className,
      )}
    />
  ),
);
CardIndent.displayName = "CardIndent";

type Slot = {
  card?: Card;
  offset: { x: number; y: number; zIndex?: number[] | number };
  index: number;
  hash?: string | null;
};

const AnimatedSlot = memo<Slot>(({ card, index, offset, hash }) => {
  const { play } = useSound();
  return (
    <motion.div
      key={index}
      className={classNames("absolute")}
      title={`${index}`}
      initial={false}
      animate={{
        ...offset,
        zIndex: offset.zIndex ?? index,
      }}
      transition={{
        ease: "easeInOut",
        duration: 0.5,
        delay: card ? 0.5 : 0,
      }}
      onAnimationComplete={() => {
        if (!card) return;
        play("deal-card");
      }}
    >
      <CardComponent card={card} hash={hash || undefined} key={index} />
    </motion.div>
  );
},
  (prevProps, nextProps) =>
    IsSameCard(prevProps.card, nextProps.card) &&
    prevProps.index === nextProps.index &&
    prevProps.offset.x === nextProps.offset.x &&
    prevProps.offset.y === nextProps.offset.y &&
    prevProps.offset.zIndex === nextProps.offset.zIndex &&
    prevProps.hash === nextProps.hash
);
AnimatedSlot.displayName = "AnimatedSlot";

export const CommunityCardsComponent = memo<{ community_cards: Card[] }>(
  ({ community_cards }) => {
    const { table } = useTable();
    const roundId = table?.round_ticker;
    
    // Calculate hashes for community cards
    const cardHashes = useCardHashes(community_cards, roundId);
    
    const stackIndent = useRef<HTMLDivElement>(null);
    const hiddenIndent = useRef<HTMLDivElement>(null);
    const slot0Indent = useRef<HTMLDivElement>(null);
    const slot1Indent = useRef<HTMLDivElement>(null);
    const slot2Indent = useRef<HTMLDivElement>(null);
    const slot3Indent = useRef<HTMLDivElement>(null);
    const slot4Indent = useRef<HTMLDivElement>(null);

    const [indents, setIndents] = useState<(HTMLDivElement | null)[]>(
      new Array(6).fill(null),
    );

    useEffect(() => {
      setIndents([
        hiddenIndent.current,
        slot0Indent.current,
        slot1Indent.current,
        slot2Indent.current,
        slot3Indent.current,
        slot4Indent.current,
      ]);
    }, [
      hiddenIndent.current,
      slot0Indent.current,
      slot1Indent.current,
      slot2Indent.current,
      slot3Indent.current,
      slot4Indent.current,
    ]);

    const slots = useMemo((): Slot[] => {
      const stack = stackIndent.current;

      let deck: Slot[] = Array.from({ length: 20 }, (_, i) => {
        return {
          card: undefined,
          index: i,
          offset: {
            x: 0,
            y: i * -1,
          },
        };
      });

      if (indents.some((v) => !v)) return [];

      if (community_cards.length === 0) return deck;

      const burntCards: Slot[] = [];
      const playedCards: Slot[] = [];

      // Guarding against community_cards having more than 5
      community_cards.slice(0, 5).forEach((card, i) => {
        burntCards.push({
          ...deck.pop()!,
          offset: {
            // x: 0,
            x:
              (hiddenIndent.current?.offsetLeft ?? 0) -
              (stack?.offsetLeft ?? 0),
            y: i * -2,
            zIndex: 20 + i,
          },
        });

        const indent = indents[i + 1];

        playedCards.push({
          ...deck.pop()!,
          card,
          hash: cardHashes[i] || undefined,
          offset: {
            x: -(stack?.offsetLeft ?? 0) + (indent?.offsetLeft ?? 0),
            y: -(stack?.offsetTop ?? 0) + (indent?.offsetTop ?? 0),
            zIndex: [20 - i, 60 - i],
          },
        });
      });

      if (community_cards.length > 2)
        deck = deck.map((v, i) => ({
          ...v,
          offset: { ...v.offset, y: i * -2 },
        }));

      return [...deck, ...burntCards, ...playedCards].sort(
        (a, b) => a.index - b.index,
      );
    }, [
      stackIndent.current,
      community_cards,
      cardHashes,
      hiddenIndent.current,
      slot0Indent.current,
      slot1Indent.current,
      slot2Indent.current,
      slot3Indent.current,
      slot4Indent.current,
    ]);

    return (
      <div className="flex flex-col lg:flex-row gap-2 lg:gap-0.5 items-center lg:items-end relative mt-4">
        <div className="flex flex-row gap-0.5 items-end">
          <CardIndent ref={stackIndent} />

          {slots.map((slot) => (
            <AnimatedSlot key={slot.index} {...slot} />
          ))}
          <CardIndent className="mr-2 ml-1" ref={hiddenIndent} />
        </div>

        <div className="flex flex-row gap-0.5 items-end">
          <CardIndent ref={slot0Indent} />
          <CardIndent ref={slot1Indent} />
          <CardIndent ref={slot2Indent} />
          <CardIndent ref={slot3Indent} />
          <CardIndent ref={slot4Indent} />
        </div>
      </div>
    );
  },
  (prevProps, nextProps) => IsSameHand(prevProps.community_cards, nextProps.community_cards)
);
CommunityCardsComponent.displayName = "CommunityCardsComponent";
