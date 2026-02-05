import classNames from 'classnames';
import { AnimatePresence, motion } from 'framer-motion';
import { memo, PropsWithChildren, useEffect, useMemo, useState } from 'react';

import {
  Card, CurrencyType, PlayerAction, UserTableData
} from '@declarations/table_canister/table_canister.did';
import { User } from '@declarations/users_index/users_index.did';
import { CardComponent } from '@lib/table/components/card/card.component';
import { useTableUIContext } from '@lib/table/context/table-ui.context';
import { useTable } from '@lib/table/context/table.context';
import { useCardHashes } from '@lib/table/hooks/use-card-hash';
import { AvatarComponent } from '@lib/ui/avatar/avatar.component';
import { CurrencyComponent, IsSameCurrencyType } from '@zk-game-dao/currency';
import { Interactable, ScreenAvoidingElement, UnwrapOptional } from '@zk-game-dao/ui';

import { IsSameAvatar, IsSameHand, IsSamePlayerAction } from '../../../lib/utils/compare';

const PlayerActionComponent = memo<{ action: PlayerAction; onClose(): void }>(
  ({ action, onClose }) => {
    const { currencyType: currency } = useTable();

    useEffect(() => {
      if ("AllIn" in action)
        return;
      const timeout = setTimeout(() => onClose(), 3000);
      return () => clearTimeout(timeout);
    }, [action]);

    const { label, cls } = useMemo(() => {
      if (!action || "None" in action) return {};
      if ("Checked" in action)
        return { label: "Checked", cls: "text-black bg-neutral-200" };
      if ("Called" in action)
        return { label: "Called", cls: "text-white bg-orange-500" };
      if ("Bet" in action)
        return {
          label: <>Bet <CurrencyComponent currencyType={currency} currencyValue={action.Bet} /></>,
          cls: "text-white bg-black",
        };
      if ("Raised" in action)
        return {
          label: <>Raised to <CurrencyComponent currencyType={currency} currencyValue={action.Raised} /></>,
          cls: "text-white bg-black",
        };
      if ("Folded" in action)
        return { label: "Folded", cls: "text-white bg-red-500" };
      if ("AllIn" in action)
        return { label: "All in", cls: "text-white bg-black" };
      if ("SittingOut" in action) return { label: "Sitting out" };
      if ("Joining" in action) return { label: "Joining" };
      return {
        label: `Uknown user action "${Object.keys(action)[0]}"`,
        cls: "",
      };
    }, [action, currency]);

    if (!label) return null;

    return (
      <motion.div
        className={classNames(
          "material type-button-2 px-2 h-6 rounded-sm flex justify-center items-center whitespace-nowrap",
          cls,
        )}
      >
        {label}
      </motion.div>
    );
  },
);
PlayerActionComponent.displayName = "PlayerActionComponent";

const SlideIn = memo<
  PropsWithChildren<{
    direction: "up" | "down";
  }>
>(({ children, direction }) => (
  <motion.div
    variants={{
      hidden: {
        [direction === "up" ? "marginBottom" : "marginTop"]: 0,
        opacity: 0,
        height: 0,
      },
      visible: {
        [direction === "up" ? "marginBottom" : "marginTop"]: 2,
        opacity: 1,
        height: "auto",
      },
    }}
    initial="hidden"
    animate="visible"
    exit="hidden"
  >
    {children}
  </motion.div>
));
SlideIn.displayName = "SlideIn";

const Trapezoid = memo<{
  width?: number;
  height?: number;
  slope?: number;
  radius?: number;
  fill?: string;
  className?: string;
  children?: React.ReactNode;
  shouldBlink?: boolean;
}>(({
  width = 160,
  height = 60,
  slope = 10,
  radius = 8,
  fill = "#353535", // Tailwind gray-500
  className,
  children,
  shouldBlink = false,
}) => {
  const w = width;
  const h = height;
  const p = slope;
  const r = radius;

  // Path for a rounded trapezoid (top narrower than bottom)
  const d = `
    M ${p + r},${h}
    H ${w - p - r}
    Q ${w - p},${h} ${w - p},${h - r}
    L ${w},${r}
    Q ${w},0 ${w - r},0
    H ${r}
    Q 0,0 0,${r}
    L ${p},${h - r}
    Q ${p},${h} ${p + r},${h}
    Z
  `;

  // Neon color for blink (green neon)
  const neonStroke = "#39ff14";
  const neonGlow1 = "#39ff14";
  const neonGlow2 = "#aaff77";

  return (
    <div
      className={classNames('relative flex flex-col justify-center items-start', className)}
      style={{ width: w, height: h, minWidth: w, minHeight: h, overflow: "visible" }}
    >
      <svg
        width="100%"
        height="100%"
        viewBox={`0 0 ${w} ${h}`}
        className='absolute top-0 left-0 w-full h-full pointer-events-none overflow-visible'
        preserveAspectRatio="none"
      >
        {/* SVG filter for neon glow */}
        <defs>
          <filter id="neon-glow" filterUnits="userSpaceOnUse">
            <feDropShadow dx="0" dy="0" stdDeviation="2" floodColor={neonGlow1} floodOpacity="0.8" />
            <feDropShadow dx="0" dy="0" stdDeviation="2" floodColor={neonGlow2} floodOpacity="0.5" />
            <feDropShadow dx="0" dy="0" stdDeviation="8" floodColor={neonGlow1} floodOpacity="0.5" />
          </filter>
        </defs>
        <path d={d} fill={fill} />
        {shouldBlink && (
          <motion.path
            d={d}
            fill="none"
            stroke={neonStroke}
            strokeWidth="5"
            strokeLinejoin="round"
            strokeLinecap="round"
            filter="url(#neon-glow)"
            style={{
              // If not using animate, always visible neon
              // But here we want a "blinking" neon
              // Animate opacity for a flashing neon effect
            }}
            animate={{
              opacity: [0, 1, 0],
              // optional: scale could help exaggerate the glow if desired
              // scale: [1, 1.02, 1]
            }}
            transition={{
              duration: 1.8,
              repeat: Infinity,
              ease: 'easeInOut',
            }}
          />
        )}
      </svg>
      <div
        className="z-10 w-full h-full absolute top-0 left-0 flex flex-col justify-center items-center"
        style={{
          pointerEvents: "auto",
        }}
      >
        {children}
      </div>
    </div>
  );
});

Trapezoid.displayName = "Trapezoid";

export type PlayerTagProps = {
  turnProgress?: number;
  isSelf?: boolean;
  onClick?(): void;
  direction?: "up" | "down";
  cards?: (Card | undefined)[];
  isQueued?: boolean;
  currencyType: CurrencyType;
  isDealer?: boolean;
} & Partial<
  Pick<User, "user_name" | "avatar" | 'balance' | 'is_verified'> &
  Pick<UserTableData, "player_action" | "current_total_bet">
>;

export const PlayerTag = memo<PlayerTagProps>(
  ({
    player_action,
    user_name,
    direction = "up",
    avatar,
    is_verified,
    turnProgress,
    onClick,
    cards,
    isSelf,
    isQueued = false,
    current_total_bet,
    currencyType: currency,
    isDealer,
    balance
  }) => {
    const [playerAction, setPlayerAction] = useState(player_action);
    const { animatePots } = useTableUIContext();
    const { table } = useTable();
    const roundId = table?.round_ticker;

    // Calculate hashes for visible cards (only for isSelf cards)
    const cardHashes = useCardHashes(isSelf && cards ? cards : [], roundId);

    useEffect(() => {
      if (!player_action) return;
      setPlayerAction({ ...player_action });
    }, [player_action]);

    const potCls = useMemo(() => {
      if (animatePots || !playerAction) return;
      if ("Folded" in playerAction) return "bg-red-500  text-white";
      if ("AllIn" in playerAction || "Bet" in playerAction || "Raised" in playerAction)
        return "bg-black text-white";
      if ("Called" in playerAction) return "bg-orange-500  text-white";
      if ("Checked" in playerAction) return "bg-neutral-200 text-black";
      return "";
    }, [animatePots, playerAction]);

    const potActionText = useMemo(() => {
      if (!playerAction) return;
      if ("Folded" in playerAction) return "Folded";
      if ("AllIn" in playerAction) return "All in";
      if ("Bet" in playerAction) return `Bet`;
      if ("Raised" in playerAction) return `Raised`;
      if ("Called" in playerAction) return "Called";
      if ("Checked" in playerAction) return "Checked";
      return "";
    }, [playerAction]);

    const cardMarginBottom = useMemo(() => {
      const length = cards?.length ?? 0;
      if (length === 2) return [0, 0]
      if (length === 3) return [0, 4, 0]
      if (length === 4) return [0, 4, 4, 0]
      if (length === 5) return [0, 4, 5, 4, 0]
    }, [cards]);

    // Calculate remaining time countdown (reverse: 30, 29, 28, ..., 0)
    const remainingTime = useMemo(() => {
      if (turnProgress === undefined || !table?.config?.timer_duration) return null;
      const totalSeconds = table.config.timer_duration;
      // turnProgress is already the remainder (1 = all time remaining, 0 = no time remaining)
      // So remaining seconds = totalSeconds * turnProgress
      const remainingSeconds = Math.floor(totalSeconds * turnProgress);
      // Clamp to 0 minimum
      return Math.max(0, remainingSeconds);
    }, [turnProgress, table?.config?.timer_duration]);

    // Check if in warning zone (5, 4, 3, 2, 1, 0)
    const isWarning = useMemo(() => {
      return remainingTime !== null && remainingTime <= 5;
    }, [remainingTime]);

    return (
      <div className="z-1">
        <ScreenAvoidingElement>
          <div
            className={classNames(
              "flex items-center z-10",
              direction === "up" ? "flex-col" : "flex-col",
              "transition-transform ",
              isQueued ? "animate-pulse scale-90" : "scale-100",
            )}
          >
            <AnimatePresence>
              <Interactable
                key="player-tag"
                className={classNames("flex flex-col items-center justify-center",
                  {
                    [turnProgress === undefined ? "py-2.5 pl-2.5" : "p-1.5"]:
                      animatePots,
                    "transition-transform z-1": !animatePots,
                  },
                )}
                onClick={onClick}
              >
                <div className="flex justify-center items-center relative -mb-5">
                  <AvatarComponent
                    size="big"
                    avatar={avatar}
                    is_verified={is_verified}
                    isDealer={isDealer && !animatePots}
                  />


                  {cards && !("Folded" in (playerAction ?? {}) || "SittingOut" in (playerAction ?? {})) && (
                    <div className="flex flex-row absolute bottom-2 z-10">
                      {cards.map((card, index) => (
                        <CardComponent
                          key={index}
                          card={card}
                          hash={isSelf && cardHashes[index] ? cardHashes[index]! : undefined}
                          size="small"
                          className={classNames("transform", {
                            "-ml-7": index > 0
                          })}
                          style={{
                            transform: `rotate(${-10 + index * (20 / (cards.length - 1))}deg)`,
                            marginBottom: cardMarginBottom?.[index] ?? 0,
                            marginTop: 6 - (cardMarginBottom?.[index] ?? 0)
                          }}
                        />
                      ))}
                    </div>
                  )}
                </div>
                {animatePots && (
                  <Trapezoid className='z-20 w-full relative' shouldBlink={remainingTime !== null}>
                    <div className='w-full h-full flex flex-col items-center justify-center p-1 gap-1 px-2'>
                      <p className="type-button-2 text-lg whitespace-nowrap text-white border-b  w-full text-center pb-2">
                        {user_name}
                      </p>
                      <div className='pt-1'>
                        <CurrencyComponent forceFlex currencyValue={balance} size="small" className='flex!' currencyType={currency} />
                      </div>
                    </div>
                    <div className='absolute bottom-5 z-20'>
                      {animatePots && !!playerAction && !("None" in playerAction) && (
                        <SlideIn direction={direction} key="action">
                          <PlayerActionComponent
                            onClose={() => setPlayerAction(undefined)}
                            action={playerAction}
                          />
                        </SlideIn>
                      )}
                    </div>
                  </Trapezoid>
                )}
                {remainingTime !== null && (
                  <div className="flex items-center w-full px-4 absolute bottom-0">
                    <div className="flex-1 h-1 bg-gray-200 rounded-full overflow-hidden relative">
                      <motion.div
                        className={classNames(
                          "h-full mr-2",
                          isWarning ? "bg-red-500" : "bg-yellow-500"
                        )}
                        initial={{ width: "100%" }}
                        animate={{
                          width: `${(turnProgress ?? 0) * 100}%`,
                          backgroundColor: isWarning ? "#ef4444" : "#eab308",
                        }}
                        transition={{
                          duration: 0.1,
                          ease: "linear",
                        }}
                      />
                    </div>
                    <motion.span
                      className={classNames(
                        "text-xs font-bold px-1.5 py-0.5 rounded whitespace-nowrap absolute right-0 z-20",
                        isWarning ? "bg-red-500 text-white" : "bg-yellow-500 text-white"
                      )}
                      animate={
                        isWarning
                          ? {
                            backgroundColor: ["#eab308", "#ef4444"],
                            color: ["#ffffff", "#ffffff"],
                          }
                          : {
                            backgroundColor: "#eab308",
                            color: "#ffffff",
                          }
                      }
                      transition={{
                        duration: 0.5,
                        repeat: isWarning ? Infinity : 0,
                        ease: "easeInOut",
                      }}
                    >
                      {remainingTime}
                    </motion.span>
                  </div>
                )}
                {/* Countdown timer at bottom right */}

              </Interactable>

              {!animatePots && !!current_total_bet && (
                <div
                  key="bet"
                  className={classNames(
                    "material rounded-[12px] px-2 py-1 mt-1 justify-center items-center flex flex-col z-0",
                    potCls,
                  )}
                >
                  {potActionText && <p>{potActionText} </p>}
                  <CurrencyComponent currencyType={currency} currencyValue={current_total_bet} />
                </div>
              )}
            </AnimatePresence>
          </div>
        </ScreenAvoidingElement>
      </div>
    );
  },
  (prevProps, nextProps) => (
    IsSamePlayerAction(prevProps.player_action, nextProps.player_action) &&
    prevProps.user_name === nextProps.user_name &&
    IsSameAvatar(UnwrapOptional(prevProps.avatar), UnwrapOptional(nextProps.avatar)) &&
    prevProps.turnProgress === nextProps.turnProgress &&
    prevProps.isSelf === nextProps.isSelf &&
    IsSameHand(prevProps.cards, nextProps.cards) &&
    prevProps.isDealer === nextProps.isDealer,
    prevProps.isQueued === nextProps.isQueued &&
    prevProps.current_total_bet === nextProps.current_total_bet &&
    prevProps.balance === nextProps.balance &&
    UnwrapOptional(prevProps.is_verified) === UnwrapOptional(nextProps.is_verified) &&
    prevProps.direction === nextProps.direction &&
    IsSameCurrencyType(prevProps.currencyType, nextProps.currencyType) &&
    prevProps.onClick === nextProps.onClick
  )
);
PlayerTag.displayName = "PlayerTag";