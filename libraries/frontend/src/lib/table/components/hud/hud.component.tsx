import classNames from "classnames";
import { AnimatePresence, motion } from "framer-motion";
import React, { memo, useMemo, useState, useCallback, useEffect } from "react";

import { useUser } from "@lib/user";
import { TokenAmountToString } from "@lib/utils/token-amount-conversion";
import {
  ButtonComponent,
  DynamicSizeComponent,
  Modal,
  ModalFooterPortal,
  TitleTextComponent,
  UnwrapOptional,
  WeirdKnobComponent,
} from "@zk-game-dao/ui";
import {
  CurrencyInputComponent,
  useCurrencyManagerMeta,
} from "@zk-game-dao/currency";

import {
  useCurrentTableTurnProgressRemainder,
  useNewRoundProgress,
  useTable,
} from "../../context/table.context";
import { CardComponent } from "../card/card.component";
import { HudBalanceComponent } from "./hud-balance.component";
import {
  HUDBettingConsumer,
  ProvideHUDBettingContext,
  useSitOut,
} from "./hud-betting.context";
import { HudPlayButtonsComponent } from "./hud-play-buttons.component";
import { HUDQuickActionsComponent } from "./hud-quick-actions.component";
import { HudSeperator } from "./hud-seperator.component";
import { useTournament } from "../../../tournament/context/tournament.context";
import { useEnterTexts } from "../../../tournament/components/enter-modal.component";

// Helper component to handle the input row with hooks
const HUDInputRow = memo<{ raise: any; currencyType: any }>(
  ({ raise, currencyType }) => {
    const meta = useCurrencyManagerMeta(currencyType);
    const [isDragging, setIsDragging] = useState(false);

    const handleSliderChange = useCallback(
      (e: React.MouseEvent<HTMLDivElement>) => {
        const rect = e.currentTarget.getBoundingClientRect();
        const padding = 24; // px-6 = 24px on each side
        const effectiveWidth = rect.width - padding * 2;
        const percent = Math.max(
          0,
          Math.min(1, (e.clientX - rect.left - padding) / effectiveWidth)
        );
        const range = Number(raise.max) - Number(raise.min);
        const newValue = BigInt(
          Math.round(Number(raise.min) + percent * range)
        );
        raise.change(newValue);
      },
      [raise]
    );

    const handleMouseDown = useCallback(
      (e: React.MouseEvent<HTMLDivElement>) => {
        setIsDragging(true);
        handleSliderChange(e);
      },
      [handleSliderChange]
    );

    const handleMouseMove = useCallback(
      (e: MouseEvent) => {
        if (!isDragging) return;
        const slider = document.getElementById("raise-slider");
        if (!slider) return;

        const rect = slider.getBoundingClientRect();
        const padding = 24; // px-6 = 24px on each side
        const effectiveWidth = rect.width - padding * 2;
        const percent = Math.max(
          0,
          Math.min(1, (e.clientX - rect.left - padding) / effectiveWidth)
        );
        const range = Number(raise.max) - Number(raise.min);
        const newValue = BigInt(
          Math.round(Number(raise.min) + percent * range)
        );
        raise.change(newValue);
      },
      [isDragging, raise]
    );

    const handleMouseUp = useCallback(() => {
      setIsDragging(false);
    }, []);

    useEffect(() => {
      if (isDragging) {
        window.addEventListener("mousemove", handleMouseMove);
        window.addEventListener("mouseup", handleMouseUp);
        return () => {
          window.removeEventListener("mousemove", handleMouseMove);
          window.removeEventListener("mouseup", handleMouseUp);
        };
      }
    }, [isDragging, handleMouseMove, handleMouseUp]);

    // Calculate step increment based on currency type
    const stepIncrement = useMemo(() => {
      if (meta.decimals === 0) return 1n;
      // For currencies with decimals, use 0.1 of the base unit, or 1 if that's too small
      // Convert 0.1 to bigint: 0.1 * 10^decimals
      if (meta.renderedDecimalPlaces !== undefined && meta.renderedDecimalPlaces > 0) {
        const stepValue = meta.renderedDecimalPlaces < 1 ? 0.1 : 1;
        return BigInt(Math.round(stepValue * Math.pow(10, meta.decimals)));
      }
      // Default to 0.01 * 10^decimals (0.01 converted to bigint)
      return BigInt(Math.round(0.01 * Math.pow(10, meta.decimals)));
    }, [meta.decimals, meta.renderedDecimalPlaces]);

    const handleDecrement = useCallback(() => {
      const decremented = raise.value > stepIncrement
        ? raise.value - stepIncrement
        : raise.min!;
      const newValue = decremented >= raise.min! ? decremented : raise.min!;
      raise.change(newValue);
    }, [raise, stepIncrement]);

    const handleIncrement = useCallback(() => {
      const incremented = raise.value + stepIncrement;
      const newValue = incremented <= raise.max! ? incremented : raise.max!;
      raise.change(newValue);
    }, [raise, stepIncrement]);

    return (
      <motion.div
        variants={{
          visible: {
            opacity: 1,
            y: -8,
            scale: 1,
          },
          hidden: {
            opacity: 0,
            y: 16,
            scale: 0.9,
          },
        }}
        initial="hidden"
        animate="visible"
        exit="hidden"
        className="flex flex-col justify-center items-center gap-2 whitespace-nowrap px-4 relative z-11"
      >
        <div className="absolute inset-3 bg-black blur-2xl opacity-30" />

        {/* Quick Actions on top line */}
        <div className="relative z-10 w-full flex justify-center">
          <HUDQuickActionsComponent
            quickActions={raise.quickActions}
            onChange={raise.change}
            currentValue={raise.value}
          />
        </div>

        <div className="flex items-center justify-center gap-2 relative z-10">
          <CurrencyInputComponent
            currencyType={currencyType}
            value={raise.value}
            onChange={raise.change}
            min={raise.min}
            max={raise.max}
            className="w-32 rounded-xl bg-neutral-400 bg-opacity-70"
            hideMaxQuickAction
            hideMinQuickAction
          />
          {raise.min !== undefined && raise.max !== undefined && (
            <>
              {/* Decrement button */}
              <button
                onClick={handleDecrement}
                disabled={raise.value <= raise.min}
                className={classNames(
                  "w-10 h-10 rounded-xl bg-neutral-400 bg-opacity-70 flex items-center justify-center",
                  "text-white text-xl font-bold transition-all duration-200",
                  "hover:bg-opacity-90 active:scale-95",
                  raise.value <= raise.min && "opacity-50 cursor-not-allowed"
                )}
              >
                -
              </button>

              {/* Slider bar - increased length */}
              <div className="flex flex-col gap-1 w-[250px]">
                {/* Interactive Slider bar - expanded clickable area */}
                <div
                  id="raise-slider"
                  className="relative h-12 cursor-pointer flex items-center px-6"
                  onMouseDown={handleMouseDown}
                >
                  {/* Visual track */}
                  <div className="absolute left-6 right-6 h-2 bg-neutral-400 bg-opacity-70 rounded-full" />

                  {/* Chip indicator - positioned relative to track */}
                  <img
                    src="/icons/chip-black.svg"
                    alt="slider"
                    className={`absolute top-1/2 -translate-y-1/2 w-12 h-12 pointer-events-none ${isDragging ? "scale-110" : ""} transition-all duration-150 z-10`}
                    style={{
                      left: `calc((100% - 48px) * ${Math.min(
                        1,
                        Math.max(
                          0,
                          (Number(raise.value) - Number(raise.min)) /
                          (Number(raise.max) - Number(raise.min))
                        )
                      )})`,
                    }}
                  />
                </div>
              </div>

              {/* Increment button */}
              <button
                onClick={handleIncrement}
                disabled={raise.value >= raise.max}
                className={classNames(
                  "w-10 h-10 rounded-xl bg-neutral-400 bg-opacity-70 flex items-center justify-center",
                  "text-white text-xl font-bold transition-all duration-200",
                  "hover:bg-opacity-90 active:scale-95",
                  raise.value >= raise.max && "opacity-50 cursor-not-allowed"
                )}
              >
                +
              </button>
            </>
          )}
        </div>
      </motion.div>
    );
  }
);
HUDInputRow.displayName = "HUDInputRow";

export const HUDComponent = memo<{
  openRngDashboard?: (roundId?: bigint) => void;
}>(({ openRngDashboard }) => {
  const { isOngoing, table, isJoined, userIndex, user } = useTable();
  const { user: zkpUser } = useUser();

  const turnProgress = useCurrentTableTurnProgressRemainder(
    isJoined && table.current_player_index === userIndex
  );
  const newRoundProgress = useNewRoundProgress(isJoined);
  const progress = useMemo(
    () => newRoundProgress ?? turnProgress,
    [newRoundProgress, turnProgress]
  );
  const sitout = useSitOut();

  const [showSitOutModal, setShowSitOutModal] = useState(false);
  const tournament = useTournament();
  const texts = useEnterTexts();

  return (
    <AnimatePresence>
      {!!zkpUser && (
        <motion.div
          initial={{ opacity: 0, y: 32, scale: 1.1 }}
          animate={{ opacity: 1, y: 0, scale: 1 }}
          exit={{ opacity: 0, y: 16, scale: 0.9 }}
          className="flex flex-col relative z-10 items-center px-4 lg:px-0"
        >
          <ProvideHUDBettingContext>
            <HUDBettingConsumer>
              {({ raise, currencyType }) => (
                <AnimatePresence>
                  {isJoined &&
                    table.current_player_index === userIndex &&
                    isOngoing &&
                    raise &&
                    raise.quickActions?.length > 0 && (
                      <HUDInputRow raise={raise} currencyType={currencyType} />
                    )}
                </AnimatePresence>
              )}
            </HUDBettingConsumer>

            <div className="relative flex items-center flex-col">
              {/* Cards are now displayed on the table, not in HUD */}

              <div className="material rounded-[12px] lg:rounded-[24px] z-10 relative gap-2 flex flex-col items-center justify-center p-2 lg:p-3 lg:whitespace-nowrap w-full lg:w-auto ">
                <div className="absolute inset-0 rounded-[12px] lg:rounded-[24px] overflow-hidden">
                  {progress !== undefined && (
                    <motion.div
                      variants={{
                        visible: (v) => ({
                          right: !v ? "100%" : `${Math.floor((1 - v) * 100)}%`,
                        }),
                      }}
                      initial={false}
                      className={classNames(
                        "absolute -left-4 -inset-y-4 blur-[8px] transition-colors",
                        progress < 0.2
                          ? "animate-pulse bg-material-medium-1"
                          : "bg-material-main-3"
                      )}
                      animate="visible"
                      custom={progress}
                    />
                  )}
                  <div
                    style={{
                      backgroundImage: "url(/images/grain.png)",
                      backgroundSize: "1194px 834px",
                    }}
                    className="absolute inset-0 mix-blend-screen opacity-[0.08] z-0 pointer-events-none"
                  />
                </div>

                <HUDBettingConsumer>
                  {({ tableUser, raise, autoCheckFold, currencyType }) => (
                    <AnimatePresence>
                      {isJoined && tableUser && (
                        <HudBalanceComponent
                          balance={tableUser.balance}
                          currencyType={currencyType}
                        />
                      )}

                      <div className="gap-2 flex flex-row items-center justify-center">
                        {!raise?.showInlineInput && isJoined && (
                          <>
                            {!sitout.isSittingOut &&
                              autoCheckFold &&
                              isOngoing && (
                                <div
                                  className={classNames(
                                    "transition-transform",
                                    { "scale-90": autoCheckFold.data }
                                  )}
                                >
                                  <WeirdKnobComponent
                                    mutate={() =>
                                      autoCheckFold.mutate(!autoCheckFold.data)
                                    }
                                    isPending={autoCheckFold.isPending}
                                    variant={
                                      autoCheckFold.data
                                        ? "gray"
                                        : "transparent"
                                    }
                                  >
                                    Check/Fold
                                  </WeirdKnobComponent>
                                </div>
                              )}

                            {isOngoing && !sitout.isSittingOut && (
                              <>
                                <WeirdKnobComponent
                                  mutate={() => setShowSitOutModal(true)}
                                  isPending={
                                    sitout.isPending || showSitOutModal
                                  }
                                  variant="transparent"
                                >
                                  Sit out
                                </WeirdKnobComponent>

                                <Modal
                                  open={showSitOutModal}
                                  onClose={() => setShowSitOutModal(false)}
                                >
                                  <TitleTextComponent
                                    title="Sit Out"
                                    text="If you choose to sit out while the game is in progress, your hand will automatically fold."
                                  />
                                  <ModalFooterPortal>
                                    <ButtonComponent
                                      variant="naked"
                                      onClick={() => setShowSitOutModal(false)}
                                    >
                                      Cancel
                                    </ButtonComponent>
                                    <ButtonComponent
                                      color="red"
                                      onClick={async () => {
                                        await sitout.sitOut();
                                        setShowSitOutModal(false);
                                      }}
                                      isLoading={sitout.isPending}
                                    >
                                      Fold & Sit out
                                    </ButtonComponent>
                                  </ModalFooterPortal>
                                </Modal>
                              </>
                            )}

                            {/* RNG Transparency Button */}
                            {openRngDashboard && (
                              <WeirdKnobComponent
                                mutate={() => openRngDashboard()}
                                isPending={false}
                                variant="transparent"
                              >
                                🎲 RNG
                              </WeirdKnobComponent>
                            )}

                            {/* Show separator if there are any buttons before play buttons */}
                            {((!sitout.isSittingOut &&
                              autoCheckFold &&
                              isOngoing) ||
                              (isOngoing && !sitout.isSittingOut) ||
                              openRngDashboard) && <HudSeperator desktopOnly />}
                          </>
                        )}

                        <DynamicSizeComponent
                          animateWidth
                          animateHeight={false}
                          className="whitespace-nowrap justify-center items-center"
                        >
                          <div className="flex flex-row">
                            <HudPlayButtonsComponent
                              tournament_table_id={tournament?.user?.table?.id}
                              tournament_is_running={tournament?.isRunning}
                              tournament_start_time={
                                tournament?.data.start_time
                              }
                              tournament_state={tournament?.data.state}
                              tournament_join_type={tournament?.joinType}
                              tournamentUserTextsTitle={texts?.title}
                              isSittingOut={sitout.isSittingOut}
                              isSittingBackIn={sitout.isSittingBackIn}
                              isSittingOutPending={sitout.isPending}
                              rejoin={sitout.rejoin}
                              sitOut={sitout.sitOut}
                              userIndex={userIndex}
                              userPlayerAction={user?.data?.player_action}
                              userIsQueuedForNextRound={
                                user && "QueuedForNextRound" in user.status
                              }
                              isTableOngoing={isOngoing}
                              current_player_index={table.current_player_index}
                              tableId={table.id}
                              isTablePaused={UnwrapOptional(
                                table.config.is_paused
                              )}
                              tableHasMoreThanOnePlayer={
                                table.seats.filter((v) => !("Empty" in v))
                                  .length > 1
                              }
                            />
                          </div>
                        </DynamicSizeComponent>
                      </div>
                    </AnimatePresence>
                  )}
                </HUDBettingConsumer>
              </div>
            </div>
          </ProvideHUDBettingContext>
        </motion.div>
      )}
    </AnimatePresence>
  );
});
HUDComponent.displayName = "HUDComponent";
