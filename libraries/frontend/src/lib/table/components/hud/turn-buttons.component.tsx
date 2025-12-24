import { memo } from 'react';

import { WeirdKnobComponent } from '@zk-game-dao/ui';

import { useHUDBetting } from './hud-betting.context';

export const TurnButtonsComponent = memo(() => {
  const { raise, call, check, fold, allIn } = useHUDBetting();

  return (
    <div className="lg:gap-2 flex flex-row items-center justify-center">
      {fold && (
        <WeirdKnobComponent
          variant="red"
          {...fold}
          straightRightMobile={!!raise || !!call || !!check || !!allIn}
        >
          Fold
        </WeirdKnobComponent>
      )}
      {check && (
        <WeirdKnobComponent
          variant="gray"
          {...check}
          straightLeftMobile={!!fold}
          straightRightMobile={!!raise || !!call || !!allIn}
        >
          Check
        </WeirdKnobComponent>
      )}
      {call && (
        <WeirdKnobComponent
          variant="orange"
          {...call}
          straightLeftMobile={!!fold || !!check}
          straightRightMobile={!!raise || !!allIn}
        >
          Call
        </WeirdKnobComponent>
      )}
      {/* All-in button - only show for non-PLO games (Texas Hold'em, etc.) */}
      {/* For PLO: All-in is shown in the raise section when canRaise is false */}
      {allIn && !raise?.isPLO && (
        <WeirdKnobComponent
          variant="black"
          {...allIn}
          straightLeftMobile
          hideOnMobile={!!raise}
        >
          All in
        </WeirdKnobComponent>
      )}

      {/* Bet/Raise section - now just the button */}
      {raise && (
        <>
          {/* For PLO: if can't raise, show All-in button instead */}
          {raise.isPLO && !raise.canRaise ? (
            <WeirdKnobComponent
              variant="black"
              {...allIn}
              straightLeftMobile={!!fold || !!check || !!call}
            >
              All in
            </WeirdKnobComponent>
          ) : (
            /* Just the Raise/Bet button, no input */
            <WeirdKnobComponent
              variant="black"
              straightLeftMobile={!!fold || !!check || !!call || !!allIn}
              {...raise.cta}
            >
              {raise.actionLabel}
            </WeirdKnobComponent>
          )}
        </>
      )}
    </div>
  );
});
TurnButtonsComponent.displayName = 'TurnButtonsComponent';
