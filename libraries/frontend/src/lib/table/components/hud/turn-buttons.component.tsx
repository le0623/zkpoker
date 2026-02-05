import { memo } from 'react';

import { TurnButtonComponent } from '@lib/ui/button/turn-button.component';
import { useHUDBetting } from './hud-betting.context';

export const TurnButtonsComponent = memo(() => {
  const { raise, call, check, fold, allIn } = useHUDBetting();

  return (
    <div className="lg:gap-2 flex flex-row items-center justify-center">
      {fold && (
        <TurnButtonComponent
          {...fold}
          straightRightMobile={!!raise || !!call || !!check || !!allIn}
        >
          Fold
        </TurnButtonComponent>
      )}
      {check && (
        <TurnButtonComponent
          {...check}
          straightLeftMobile={!!fold}
          straightRightMobile={!!raise || !!call || !!allIn}
        >
          Check
        </TurnButtonComponent>
      )}
      {call && (
        <TurnButtonComponent
          {...call}
          straightLeftMobile={!!fold || !!check}
          straightRightMobile={!!raise || !!allIn}
        >
          Call
        </TurnButtonComponent>
      )}
      {/* All-in button - only show for non-PLO games (Texas Hold'em, etc.) */}
      {/* For PLO: All-in is shown in the raise section when canRaise is false */}
      {allIn && !raise?.isPLO && (
        <TurnButtonComponent
          {...allIn}
          straightLeftMobile
          hideOnMobile={!!raise}
        >
          All in
        </TurnButtonComponent>
      )}

      {/* Bet/Raise section - now just the button */}
      {raise && (
        <>
          {/* For PLO: if can't raise, show All-in button instead */}
          {raise.isPLO && !raise.canRaise ? (
            <TurnButtonComponent
              {...allIn}
              straightLeftMobile={!!fold || !!check || !!call}
            >
              All in
            </TurnButtonComponent>
          ) : (
            /* Just the Raise/Bet button, no input */
            <TurnButtonComponent
              straightLeftMobile={!!fold || !!check || !!call || !!allIn}
              {...raise.cta}
            >
              {raise.actionLabel}
            </TurnButtonComponent>
          )}
        </>
      )}
    </div>
  );
});
TurnButtonsComponent.displayName = 'TurnButtonsComponent';
