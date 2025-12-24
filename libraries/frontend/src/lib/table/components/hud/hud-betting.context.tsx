import {
  createContext, memo, ReactNode, useCallback, useContext, useEffect, useMemo, useState
} from 'react';

import { User } from '@declarations/table_canister/table_canister.did';
import { Queries, queryClient } from '@lib/data';
import { useUser } from '@lib/user';
import { callActorMutation } from '@lib/utils/call-actor-mutation';
import {
  FloatToTokenAmount, TokenAmountToFloat, TokenAmountToString
} from '@lib/utils/token-amount-conversion';
import { useMutation } from '@tanstack/react-query';
import { CurrencyType, useCurrencyManagerMeta } from '@zk-game-dao/currency';
import { useErrorModal } from '@zk-game-dao/ui';

import { useSound } from '../../../../context/sound.context';
import { useMyTableUser, useTable } from '../../context/table.context';

export type HUDContextType = {
  // Raising/Betting
  raise?: {
    quickActions: [bigint, string][];
    value: bigint;
    change(raiseValue: bigint): void;
    min: bigint;
    max: bigint;
    showInlineInput: boolean;
    setShowInlineInput(show: boolean): void;
    isOpeningBet: boolean;
    actionLabel: string; // "Bet" or "Raise"
    isPLO: boolean; // Is this a PLO4/PLO5 game?
    canRaise: boolean; // Can player raise, or only all-in? (PLO only)

    cta: {
      mutateExplicit(raiseValue: bigint): Promise<void>;
      mutate(): Promise<void>;
      isPending: boolean;
    };
  };

  // Check
  check?: {
    mutate(): void;
    isPending: boolean;
  };

  // Call
  call?: {
    mutate(): void;
    isPending: boolean;
    hoverLabel: string;
  };

  // Fold
  fold?: {
    mutate(): void;
    isPending: boolean;
  };

  allIn?: {
    mutate(): void;
    isPending: boolean;
  };

  autoCheckFold?: {
    mutate(isEnabled: boolean): void;
    isPending: boolean;
    data?: boolean;
  };

  tableUser?: User;
  currencyType: CurrencyType;
};

const HUDContext = createContext<HUDContextType>({
  currencyType: { Fake: null },
});

export const ProvideHUDBettingContext = memo<{ children: ReactNode }>(
  ({ children }) => {
    const { currentBet, table, actor: service, user, userIndex } = useTable();
    const [tableUser] = useMyTableUser();
    const [raiseTo, setRaiseTo] = useState<bigint>(0n);
    const { play } = useSound();
    const { user: zkpUser } = useUser();
    const { currencyType } = useTable();
    const meta = useCurrencyManagerMeta(currencyType);
    const callValue = useMemo(() => table.highest_bet, [table, user]);
    const showErrorModal = useErrorModal();
    const [showInlineInput, setShowInlineInput] = useState(false);
    // Live pot (sum of current_total_bet) to align with backend pot checks
    const livePot = useMemo(
      () =>
        table.user_table_data.reduce(
          (sum, [, data]) => sum + (data?.current_total_bet ?? 0n),
          0n,
        ),
      [table.user_table_data],
    );

    const isUserTurn = useMemo(() => table.current_player_index === userIndex, [
      table,
      userIndex,
    ]);

    // Detect if this is an opening bet (no one has bet yet on this street)
    const isOpeningBet = useMemo(() => table.highest_bet === 0n, [table.highest_bet]);
    const betActionLabel = useMemo(() => isOpeningBet ? "Bet" : "Raise", [isOpeningBet]);

    // Calculate minimum raise increment based on poker rules:
    // - Pre-flop: big blind
    // - Post-flop: size of last bet/raise (last_raise), fallback to big_blind if 0 (new street)
    const minRaiseIncrement = useMemo(() => {
      if (isOpeningBet) {
        return table.big_blind || 1n;
      }
      // Post-flop: use last_raise if available, otherwise fallback to big_blind
      // last_raise is 0 when a new betting street starts (flop/turn/river)
      return table.last_raise > 0n ? table.last_raise : (table.big_blind || 1n);
    }, [isOpeningBet, table.big_blind, table.last_raise]);

    const getRaiseToFromDelta = useCallback(
      (delta: bigint) => callValue + delta,
      [table, user],
    );
    const getPrice = useCallback(
      (value: bigint) => (!user?.data ? value : value - user.data.current_total_bet),
      [table, user],
    );

    const quickActions = useMemo((): [bigint, string][] => {
      let _quickActions: [bigint, string][] = [];
      if (!isUserTurn) return _quickActions;
      if (!table || !user || !tableUser || !user.data)
        return _quickActions.map(([amount, label]) => [
          TokenAmountToFloat(amount, meta),
          label,
        ]);

      const isPotLimit = "PotLimit" in table.config.game_type ||
        "PotLimitOmaha4" in table.config.game_type ||
        "PotLimitOmaha5" in table.config.game_type;

      const currentBet = user.data.current_total_bet;
      const callValue = table.highest_bet;

      if (isPotLimit) {
        // PLO "Rule of Three" for quick actions
        // Calculate live pot
        const livePot = table.user_table_data.reduce(
          (sum, [, data]) => sum + (data?.current_total_bet ?? 0n),
          table.pot,
        );
        
        const lastBet = callValue;
        const potBeforeLastBet = livePot - lastBet;
        const amountToCall = callValue - currentBet;
        
        // Full pot raise: call + pot after call
        const potAfterCall = potBeforeLastBet + lastBet + amountToCall;
        const potRaiseTo = callValue + potAfterCall;
        
        // Half pot raise: call + (pot after call / 2)
        const halfPotRaiseTo = callValue + (potAfterCall / 2n);
        
        // Use last_raise increment for post-flop, big_blind for pre-flop
        const minRaiseTo = callValue + minRaiseIncrement;

        // Pot
        if (potRaiseTo > currentBet && getPrice(potRaiseTo) <= tableUser.balance) {
          _quickActions.push([potRaiseTo, "Pot"]);
        }
        // 1/2 Pot
        if (halfPotRaiseTo > currentBet && halfPotRaiseTo < potRaiseTo && getPrice(halfPotRaiseTo) <= tableUser.balance) {
          _quickActions.push([halfPotRaiseTo, "1/2 Pot"]);
        }
        // Min raise
        if (
          minRaiseTo > currentBet &&
          minRaiseTo !== potRaiseTo &&
          minRaiseTo !== halfPotRaiseTo &&
          minRaiseTo <= potRaiseTo &&
          getPrice(minRaiseTo) <= tableUser.balance
        ) {
          _quickActions.push([minRaiseTo, "Min"]);
        }
      } else {
        // Non pot-limit logic (original)
        if (table.last_raise) {
          _quickActions.push(
            [getRaiseToFromDelta(table.last_raise * 2n), "Min"],
            [getRaiseToFromDelta(table.last_raise * 3n), "3x Last raise"],
          );
        } else if (table.big_blind) {
          _quickActions.push(
            [getRaiseToFromDelta(table.big_blind * 2n), "Min"],
            [getRaiseToFromDelta(table.big_blind * 3n), "3x BB"],
          );
        }

        const potToValue = getRaiseToFromDelta(table.pot);
        if (
          table.pot &&
          _quickActions.length > 0 &&
          potToValue > _quickActions[0][0]
        ) {
          _quickActions.push([potToValue, "Pot"]);
          const halfPotToValue = getRaiseToFromDelta(table.pot / 2n);
          if (halfPotToValue > _quickActions[0][0])
            _quickActions.push([halfPotToValue, "1/2 Pot"]);
        }

        _quickActions = _quickActions.filter(
          ([amount]) => getPrice(amount) < tableUser.balance,
        );
      }

      // All-in
      if (tableUser.balance > 0n) {
        const allInValue = currentBet + tableUser.balance;
        if (isPotLimit) {
          // Only show all-in if it's within pot limit
          const livePot = table.user_table_data.reduce(
            (sum, [, data]) => sum + (data?.current_total_bet ?? 0n),
            table.pot,
          );
          
          const lastBet = callValue;
          const potBeforeLastBet = livePot - lastBet;
          const amountToCall = callValue - currentBet;
          const potAfterCall = potBeforeLastBet + lastBet + amountToCall;
          const maxPotRaise = callValue + potAfterCall;
          
          if (allInValue <= maxPotRaise) {
            _quickActions.push([allInValue, "All in"]);
          }
        } else {
          _quickActions.push([allInValue, "All in"]);
        }
      }

      return _quickActions.sort((a, b) =>
        TokenAmountToFloat(a[0] - b[0], meta),
      );
    }, [
      table,
      tableUser,
      user,
      getRaiseToFromDelta,
      getPrice,
      isUserTurn,
      meta,
      minRaiseIncrement,
    ]);

    // Exclude "All in" from raise quick actions
    const raiseActions = useMemo(
      () => quickActions.filter(([, label]) => label !== "All in"),
      [quickActions],
    );

    const [min, max] = useMemo(() => {
      const isPotLimit = "PotLimit" in table.config.game_type ||
        "PotLimitOmaha4" in table.config.game_type ||
        "PotLimitOmaha5" in table.config.game_type;

      if (!user?.data || !table || !tableUser) return [0n, 0n];

      const currentBet = user.data.current_total_bet;
      const callValue = table.highest_bet;

      let calculatedMax: bigint;
      if (isPotLimit) {
        // PLO "Rule of Three" Calculation
        // Formula: Max raise-to = 3 × (last bet) + (pot before last bet)
        // Which simplifies to: Max = call + (pot_before + last_bet + call)
        //
        // Example: $20 in pot, opponent bets $10
        //   - Pot before bet: $20
        //   - Last bet: $10
        //   - Your call: $10
        //   - Pot after call: $20 + $10 + $10 = $40
        //   - Max raise-to: $10 (call) + $40 (pot) = $50
        
        // Step 1: Calculate live pot (pot from previous streets + current street bets)
        const livePot = table.user_table_data.reduce(
          (sum, [, data]) => sum + (data?.current_total_bet ?? 0n),
          table.pot,
        );
        
        // Step 2: Determine components
        const lastBet = callValue;
        const potBeforeLastBet = livePot - lastBet;
        const amountToCall = callValue - currentBet;
        
        // Step 3: Calculate pot after you call
        const potAfterCall = potBeforeLastBet + lastBet + amountToCall;
        
        // Step 4: Max raise-to = call value + pot after call
        const maxRaiseAmount = callValue + potAfterCall;
        
        // Step 5: Cap at all-in
        const allInAmount = currentBet + tableUser.balance;
        calculatedMax = maxRaiseAmount < allInAmount ? maxRaiseAmount : allInAmount;
      } else {
        calculatedMax = currentBet + tableUser.balance;
      }

      let calculatedMin: bigint;
      if (isOpeningBet) {
        // Pre-flop: minimum raise = big blind
        calculatedMin = table.big_blind || 1n;
      } else {
        // Post-flop: minimum raise = size of last bet/raise
        // minRaiseIncrement is already calculated above and handles the fallback logic
        calculatedMin = callValue + minRaiseIncrement;
        
        // For pot limit: ensure min doesn't exceed max (can happen if pot is small)
        if (isPotLimit && calculatedMin > calculatedMax) {
          calculatedMin = calculatedMax;
        }
      }

      // Don't clamp max based on quick actions - let the user input any value up to the theoretical max
      // Quick actions are just suggestions, not limits
      if (calculatedMin > calculatedMax) return [calculatedMax, calculatedMax];
      return [calculatedMin, calculatedMax];
    }, [raiseActions, table, user, tableUser, isOpeningBet, minRaiseIncrement]);

    useEffect(() => {
      setRaiseTo((v) =>
        FloatToTokenAmount(
          Math.min(
            Math.max(
              TokenAmountToFloat(v, meta),
              TokenAmountToFloat(min, meta),
            ),
            TokenAmountToFloat(max, meta),
          ),
          meta,
        ),
      );
    }, [min, max]);

    // Rest the value everytime your turn starts
    useEffect(() => {
      if (isUserTurn) {
        setRaiseTo(min);
      } else {
        setShowInlineInput(false);
      }
    }, [min, isUserTurn]);

    const { mutateAsync: submit, isPending } = useMutation({
      mutationFn: async (_raiseTo: bigint) => {
        if (!table || !tableUser || !user) throw "Table or user not found";
        const result = await service.place_bet(tableUser.principal_id, {
          Raised: _raiseTo,
        });
        if ("Err" in result) throw result.Err;
        await queryClient.invalidateQueries({ queryKey: ["table", table.id] });
        return result.Ok;
      },
      onError: showErrorModal,
    });

    useEffect(() => {
      play("turn-notification");
    }, []);

    const { mutate: check, isPending: checking } = useMutation({
      mutationFn: async () => {
        if (!table || !zkpUser) throw "Table or user not found";
        const result = await service.check(zkpUser.principal_id);
        if ("Err" in result) throw result.Err;
        await Queries.table.invalidate(table);
        return result.Ok;
      },
      onError: showErrorModal,
    });

    const minRequiredBet = useMemo(() => {
      if (!user?.data || !currentBet) return 0n;
      return currentBet - user.data.current_total_bet;
    }, [currentBet, user?.data?.current_total_bet]);

    const { mutate: call, isPending: isCalling } = useMutation({
      mutationFn: async () => {
        if (!table || !zkpUser) throw "Table or user not found";
        const result = await service.place_bet(zkpUser.principal_id, {
          Called: null,
        });
        if ("Err" in result) throw result.Err;
        await Queries.table.invalidate(table);
        return result.Ok;
      },
      onError: showErrorModal,
    });

    const { mutate: allIn, isPending: isGoingAllIn } = useMutation({
      mutationFn: async () => {
        if (!table || !zkpUser || !tableUser || !user?.data)
          throw "Table or user not found";
        const result = await service.place_bet(zkpUser.principal_id, {
          Raised: user.data.current_total_bet + tableUser.balance,
        });
        if ("Err" in result) throw result.Err;
        await Queries.table.invalidate(table);
        return result.Ok;
      },
      onError: showErrorModal,
    });

    const { mutate: fold, isPending: isFolding } = useMutation({
      mutationFn: async () => {
        if (!table || !zkpUser) throw "Table or user not found";
        return callActorMutation(service, 'fold', zkpUser.principal_id, table.current_player_index !== userIndex);
      },
      onSuccess: () => Queries.table.invalidate(table),
      onError: showErrorModal,
    });

    // const { mutate: setAutoCheckFold, isPending: isSettingAutoCheckFold } = useMutation({
    //   mutationFn: async (isEnabled: boolean) => {
    //     if (!table || !zkpUser) throw "Table or user not found";
    //     const result = await service.set_auto_check_fold(zkpUser.principal_id, isEnabled);
    //     if ("Err" in result) throw result.Err;
    //     await Queries.table.invalidate(table);
    //     return result.Ok;
    //   },
    //   onError: showErrorModal,
    // });

    const value = useMemo(() => {
      const v: HUDContextType = {
        currencyType,
      };

      if (user?.data && !("Folded" in (user.data.player_action ?? {})))
        v.fold = {
          isPending: isFolding,
          mutate: fold,
        };

      // if (user?.data) {
      //   v.autoCheckFold = {
      //     isPending: isSettingAutoCheckFold,
      //     mutate: setAutoCheckFold,
      //     data: user.data.auto_check_fold,
      //   };
      // }

      if (minRequiredBet === 0n) {
        v.check = {
          isPending: checking,
          mutate: check,
        };
      } else {
        v.call = {
          isPending: isCalling,
          mutate: call,
          hoverLabel: TokenAmountToString(minRequiredBet, meta),
        };
      }

      const isPotLimit = "PotLimit" in table.config.game_type ||
        "PotLimitOmaha4" in table.config.game_type ||
        "PotLimitOmaha5" in table.config.game_type;

      // Check if this is PLO4 or PLO5 specifically (not just any pot limit)
      const isPLO = "PotLimitOmaha4" in table.config.game_type ||
        "PotLimitOmaha5" in table.config.game_type;

      // For pot limit: always show raise controls on your turn
      // For others: show when we have any raise actions
      if ((isPotLimit && isUserTurn && user?.data && tableUser) || (!isPotLimit && raiseActions.length > 0)) {
        // Check if player can raise or only all-in (for PLO games)
        const currentBet = user?.data?.current_total_bet ?? 0n;
        const allInAmount = currentBet + (tableUser?.balance ?? 0n);
        const canRaise = allInAmount > min;

        v.raise = {
          min,
          max,
          quickActions: raiseActions.length > 0 ? raiseActions : quickActions,
          value: raiseTo,
          change: setRaiseTo,
          showInlineInput,
          setShowInlineInput,
          isOpeningBet,
          actionLabel: betActionLabel,
          isPLO,
          canRaise,
          cta: {
            async mutateExplicit(raiseValue) {
              setRaiseTo(raiseValue);
              await submit(raiseValue);
            },
            mutate: async () => {
              await submit(raiseTo);
            },
            isPending,
          },
        };
      }

      v.allIn = {
        isPending: isGoingAllIn,
        mutate: allIn,
      };

      return v;
    }, [
      minRequiredBet,
      checking,
      check,
      isCalling,
      call,
      isFolding,
      fold,
      isGoingAllIn,
      allIn,
      quickActions,
      raiseTo,
      setRaiseTo,
      submit,
      isPending,
      tableUser,
      getPrice,
      min,
      max,
      showInlineInput,
      setShowInlineInput,
      meta,
      user,
      userIndex,
      zkpUser,
      // setAutoCheckFold,
      // isSettingAutoCheckFold,
      currentBet,
      table,
      currencyType,
      play,
      showErrorModal,
      isUserTurn,
      isOpeningBet,
      betActionLabel,
      minRaiseIncrement,
    ]);

    return <HUDContext.Provider value={value}>{children}</HUDContext.Provider>;
  },
);
ProvideHUDBettingContext.displayName = "ProvideHUDBettingContext";

export const useHUDBetting = () => useContext(HUDContext);

export const HUDBettingConsumer = HUDContext.Consumer;

export const useSitOut = () => {
  const { actor, table } = useTable();
  const showErrorModal = useErrorModal();
  const [user, data] = useMyTableUser();

  const { mutate, isPending } = useMutation({
    mutationFn: async (sitOut: boolean) => {
      if (!table || !user) throw "Table or user not found";
      if (sitOut)
        return await callActorMutation(actor, "player_sitting_out", user.principal_id);
      return await callActorMutation(actor, "player_sitting_in", user.users_canister_id, user.principal_id, true);
    },
    onSuccess: () => Queries.table.invalidate(table),
    onError: showErrorModal,
  });

  const isSittingOut = useMemo(
    () => data && "SittingOut" in data.player_action,
    [data?.player_action],
  );
  const isSittingBackIn = useMemo(
    () =>
      !!(
        user?.principal_id &&
        table.queue.find(
          (v) =>
            "SittingIn" in v &&
            v.SittingIn[0].compareTo(user.principal_id) === "eq",
        )
      ),
    [user?.principal_id, table.queue],
  );

  return useMemo(
    () => ({
      sitOut: () => mutate(true),
      rejoin: () => mutate(false),
      isPending,
      isSittingOut,
      isSittingBackIn,
    }),
    [mutate, isPending, isSittingOut, isSittingBackIn],
  );
};
