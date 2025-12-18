import { memo } from 'react';

import { CurrencyInputComponent, useCurrencyManagerMeta } from '@zk-game-dao/currency';
import { WeirdKnobComponent } from '@zk-game-dao/ui';
import { TokenAmountToString } from '@lib/utils/token-amount-conversion';

import { useTableUIContext } from '../../context/table-ui.context';
import { useTable } from '../../context/table.context';
import { useHUDBetting } from './hud-betting.context';
import { HudSeperator } from './hud-seperator.component';

export const TurnButtonsComponent = memo(() => {
  const { raise, call, check, fold, allIn } = useHUDBetting();
  const { orientation } = useTableUIContext();
  const { currencyType: currency } = useTable();
  const meta = useCurrencyManagerMeta(currency);
  console.log(raise?.min, raise?.max, "raise min max");
  const rangeLabel =
    raise && raise.min !== undefined && raise.max !== undefined
      ? `${TokenAmountToString(raise.min, meta)} ~ ${TokenAmountToString(raise.max, meta)}`
      : undefined;

  if (raise?.showInlineInput)
    return (
      <div className="gap-2 flex flex-row items-center justify-center">
        <WeirdKnobComponent mutate={() => raise.setShowInlineInput(false)}>
          Cancel
        </WeirdKnobComponent>
        <CurrencyInputComponent
          value={raise.value}
          onChange={(v) => raise.change(v)}
          min={raise.min}
          max={raise.max}
          currencyType={currency}
          className="w-64"
          hideMaxQuickAction
          hideMinQuickAction
        />
        {rangeLabel && <div className="text-xs text-white/70">{rangeLabel}</div>}
        <WeirdKnobComponent variant="black" {...raise.cta}>
          Raise
        </WeirdKnobComponent>
      </div>
    );

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
      {allIn && (
        <WeirdKnobComponent
          variant="black"
          {...allIn}
          straightLeftMobile
          hideOnMobile={!!raise}
        >
          All in
        </WeirdKnobComponent>
      )}

      {/* Raise */}
      {raise && (
        <>
          {orientation === "landscape" || raise.showInlineInput ? (
            <>
              {orientation === "landscape" ? (
                <HudSeperator />
              ) : (
                <WeirdKnobComponent variant="black" {...raise.cta}>
                  Raise
                </WeirdKnobComponent>
              )}
              <CurrencyInputComponent
                currencyType={currency}
                value={raise.value}
                onChange={raise.change}
                min={raise.min}
                max={raise.max}
                className="w-32"
                hideMaxQuickAction
                hideMinQuickAction
              />
              {rangeLabel && <div className="text-xs text-white/70">{rangeLabel}</div>}
              <WeirdKnobComponent variant="black" {...raise.cta}>
                Raise
              </WeirdKnobComponent>
            </>
          ) : (
            <>
              <WeirdKnobComponent
                variant="black"
                straightLeftMobile={!!fold || !!check || !!call || !!allIn}
                mutate={() => raise.setShowInlineInput(true)}
              >
                Raise
              </WeirdKnobComponent>
            </>
          )}
        </>
      )}
    </div>
  );
});
TurnButtonsComponent.displayName = 'TurnButtonsComponent';
