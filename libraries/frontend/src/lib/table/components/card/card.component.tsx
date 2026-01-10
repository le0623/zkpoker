import classNames from 'classnames';
import { AnimatePresence, motion } from 'framer-motion';
import { CSSProperties, memo, useMemo } from 'react';

import { CardColor } from '@/src/models/table-color.model';
import { Card } from '@declarations/table_canister/table_canister.did';

import { useTableVisuals } from '../../context/table-visuals.context';
import { IsSameCard } from '../../../utils/compare';
import isEqual from 'lodash/isEqual';
import { CardHashBadge } from './card-hash-badge.component';

const SuitsMapping: Record<string, { symbol: string; className: string }> = {
  Spade: { symbol: "♠", className: "text-black" },
  Diamond: { symbol: "♦", className: "text-red-500" },
  Club: { symbol: "♣", className: "text-black" },
  Heart: { symbol: "♥", className: "text-red-500" },
};

const ValuesMapping: Record<string, string> = {
  Two: "2",
  Three: "3",
  Four: "4",
  Five: "5",
  Six: "6",
  Seven: "7",
  Eight: "8",
  Nine: "9",
  Ten: "10",
  Jack: "J",
  Queen: "Q",
  King: "K",
  Ace: "A",
};

type Props = {
  card?: Card;
  className?: string;
  size?:
  | "microscopic"
  | "between-microscopic-and-small"
  | "small"
  | "medium"
  | "large";
  floating?: boolean;
  style?: CSSProperties;
  hash?: string; // Optional card hash for transparency
};

const FlipDuration = 0.2;

const ActualCard = memo<Pick<Props, "floating" | "card" | "size">>(
  ({ size = "small", card, floating }) => {
    const isLargeText = useMemo(() => true, []);

    const { className: suiteClassName, symbol } = useMemo(
      () =>
        card
          ? SuitsMapping[Object.keys(card.suit)[0]]
          : ({} as Partial<{ symbol: string; className: string }>),
      [card?.suit],
    );

    const valueSymbol = useMemo(
      () => (card ? ValuesMapping[Object.keys(card.value)[0]] : undefined),
      [card?.value],
    );

    const { cardColor } = useTableVisuals();

    const color = useMemo(() => {
      switch (cardColor) {
        case CardColor.Green:
          return "green";
        case CardColor.Yellow:
          return "yellow";
        case CardColor.Purple:
          return "purple";
        case CardColor.Red:
        default:
          return "red";
      }
    }, [cardColor]);

    return (
      <motion.div
        variants={{
          hidden: {
            scaleX: [1, 0],
            transition: { ease: "linear", duration: FlipDuration / 2 },
          },
          visible: {
            scaleX: [0, 1],
            transition: {
              ease: "linear",
              duration: FlipDuration / 2,
              delay: FlipDuration / 2,
            },
          },
        }}
        initial="hidden"
        animate="visible"
        exit="hidden"
        className={classNames(
          "absolute inset-0 bg-[#fffff6] overflow-hidden leading-snug",
          card
            ? [
              suiteClassName,
              "overflow-hidden flex flex-col font-song-myung font-normal ",
            ]
            : [
              "bg-center bg-contain",
              {
                "bg-[#fffff6]": color === "red",
                "bg-[#fffCF1]": color === "green",
              },
            ],
          {
            "rounded-[5px] text-[13px] py-[3px] px-[6px]":
              size === "microscopic",
            "rounded-[4px] text-[18px]":
              size === "between-microscopic-and-small",
            "text-[17px]": size === "small",
            "text-[24px]": size === "medium",
            "rounded-[6px]": size === "small" || size === "medium",
            "py-1 px-2":
              size === "between-microscopic-and-small" ||
              size === "small" ||
              size === "medium",
            "rounded-[10px] text-[32px] px-3 py-1": size === "large",
          },
          !floating
            ? {
              "shadow-card-laying":
                size === "microscopic" ||
                size === "small" ||
                size === "between-microscopic-and-small" ||
                size === "medium",
              "shadow-card-floating": size === "large",
            }
            : "shadow-outer-regular-wide",
        )}
      >
        {!card && (
          <div className="flex justify-center items-center left-[5%] top-[5%] w-[90%] h-[90%] absolute ">
            <img src={`/nfts/card-backs/${color}-card.jpg`} />
          </div>
        )}
        <div
          className={classNames(
            "flex flex-col text-center mr-auto",
            isLargeText
              ? {
                "leading-[1.1]": isLargeText,
                "text-[24px]":
                  (isLargeText && size === "large") ||
                  size === "medium" ||
                  size === "small" ||
                  size === "between-microscopic-and-small",
                "text-[17px]": isLargeText && size === "microscopic",
              }
              : "",
          )}
        >
          {valueSymbol}
          <br />
          {symbol}
        </div>
      </motion.div>
    );
  },
  (prevProps, nextProps) =>
    IsSameCard(prevProps.card, nextProps.card) &&
    prevProps.size === nextProps.size &&
    prevProps.floating === nextProps.floating,
);
ActualCard.displayName = "ActualCard";

export const CardComponent = memo<Props>(
  ({ className, style, size = "small", hash, ...props }) => {
    return (
      <div
        className={classNames("aspect-card", className, {
          relative: !className || className?.indexOf("absolute") === -1,
          "w-[34px]": size === "microscopic",
          "w-[48px]": size === "between-microscopic-and-small",
          "w-[58px]": size === "small",
          "w-[80px]": size === "medium",
          "w-24": size === "large",
        })}
        style={style}
      >
        <AnimatePresence initial={false}>
          {props.card && <ActualCard key="face-up" {...props} size={size} />}
          {!props.card && <ActualCard key="face-down" {...props} size={size} />}
        </AnimatePresence>
      </div>
    );
  },
  (prevProps, nextProps) =>
    prevProps.size === nextProps.size &&
    prevProps.floating === nextProps.floating &&
    prevProps.className === nextProps.className &&
    prevProps.hash === nextProps.hash &&
    isEqual(prevProps.style, nextProps.style) &&
    IsSameCard(prevProps.card, nextProps.card)
);
CardComponent.displayName = "CardComponent";
