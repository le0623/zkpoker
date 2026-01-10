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
      if ("Folded" in action || "AllIn" in action || "SittingOut" in action)
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
          "material type-button-3 px-[14px] h-9 rounded-full flex justify-center items-center whitespace-nowrap",
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
              {animatePots && !!playerAction && !("None" in playerAction) && (
                <SlideIn direction={direction} key="action">
                  <PlayerActionComponent
                    onClose={() => setPlayerAction(undefined)}
                    action={playerAction}
                  />
                </SlideIn>
              )}
              {cards && (
                <div className="flex flex-row -mb-2">
                  {cards.map((card, index) => (
                    <CardComponent
                      key={index}
                      card={card}
                      hash={isSelf && cardHashes[index] ? cardHashes[index]! : undefined}
                      size={
                        !isSelf
                          ? "microscopic"
                          : "between-microscopic-and-small"
                      }
                      className={classNames("transform", {
                        "-ml-[5px]": index > 0,
                        [index === 0 ? "-rotate-[1deg]" : "rotate-[1deg]"]:
                          animatePots,
                        [index === 0
                          ? "-rotate-[10deg] translate-x-1"
                          : "rotate-[10deg] -translate-x-1"]: !animatePots,
                      })}
                    />
                  ))}
                </div>
              )}
              <Interactable
                key="player-tag"
                className={classNames("flex flex-row items-center rounded-full grow", {
                  "scale-150": !animatePots && turnProgress !== undefined,
                  [turnProgress === undefined ? "py-2.5 pl-2.5" : "p-1.5"]:
                    animatePots,
                  "min-w-[123px]": isSelf && cards && animatePots,
                  "material gap-2.5 pr-5": animatePots,
                  "transition-transform z-1": !animatePots,
                },
                  isSelf && [
                    {
                      'border-green-500 border-4': animatePots,
                      'ring-green-500 border-transparent border-2 ring-3': !animatePots,
                    }
                  ]
                )}
                onClick={onClick}
              >
                <AvatarComponent
                  progress={turnProgress}
                  size={
                    !animatePots
                      ? "medium"
                      : turnProgress !== undefined
                        ? "big"
                        : "small"
                  }
                  avatar={avatar}
                  is_verified={is_verified}
                  isDealer={isDealer && !animatePots}
                />
                {animatePots && (
                  <div className="flex flex-col justify-between items-start gap-[2px] h-[30px]">
                    <p className="type-button-2 whitespace-nowrap -mt-0.5">
                      {user_name}
                    </p>
                    <div className='flex flex-row grow shrink-0 w-full pr-3'>
                      <CurrencyComponent forceFlex currencyValue={balance} size="small" className='flex!' currencyType={currency} />
                    </div>
                  </div>
                )}
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