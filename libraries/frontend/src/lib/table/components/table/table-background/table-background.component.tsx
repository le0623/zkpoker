import classNames from 'classnames';
import { memo, PropsWithChildren, useMemo, useRef, useState } from 'react';

import { TableColor } from '@/src/models/table-color.model';
import { Card } from '@declarations/table_canister/table_canister.did';
import { Image, Interactable } from '@zk-game-dao/ui';

import { useTableRect } from '../../../context/table-ui.context';
import {
  ProvideTableVisuals, TableVisualsContextType, useTableVisuals
} from '../../../context/table-visuals.context';
import { CommunityCardsComponent } from '../../community-cards/community-cards.component';
import { RankCardsModalComponent } from '../../rank-cards-modal/rank-cards-modal.component';
import { IsSameHand } from '../../../../utils/compare';
import { CurrencyIconComponent, useIsBTC } from '@zk-game-dao/currency';
import BitcoinSymbol from './bitcoin-symbol.svg';

export const TableBackgroundComponent = memo<
  PropsWithChildren<{
    community_cards?: Card[];
    visuals?: Partial<TableVisualsContextType>;
    className?: string;
    showPlayers?: boolean;
    /** Only use to override dynamic calculations */
    tableSize?: { width: number; height: number };
    onShowWinners?(): void;
  }>
>(
  ({
    className,
    community_cards,
    children,
    visuals: visualsFromProps,
    onShowWinners,
    tableSize: tableSizeProps,
  }) => {
    const visuals = useTableVisuals(visualsFromProps);

    const [bgBright, bgDark] = useMemo(() => {
      switch (visuals.color) {
        case TableColor.Red:
          return ["#EC4849", "#AF2223"];
        case TableColor.Blue:
          return ["#64D3FF", "#3C7F99"];
        case TableColor.Purple:
          return ["#BF5AF2", "#6E348C"];
        case TableColor.Yellow:
          return ["#FFD50B", "#998007"];
        case TableColor.Black:
          return ["#1D1D1D", "#383838"];
        case TableColor.Green:
        default:
          return ["#30DB5B", "#248A3D"];
      }
    }, [visuals.color]);

    const containerRef = useRef<HTMLDivElement>(null);

    const tableSizeLocal = useTableRect(containerRef);
    const tableSize = useMemo(
      () => tableSizeProps ?? tableSizeLocal,
      [tableSizeProps, tableSizeLocal],
    );

    const isPortrait = useMemo(
      () => tableSize.width < tableSize.height,
      [tableSize.width, tableSize.height],
    );

    const woodSize = useMemo(
      () => Math.max(10, Math.floor(tableSize.width / 65)),
      [tableSize.width],
    );
    const upperSize = useMemo(
      () => Math.max(20, Math.floor(tableSize.width / 25)),
      [tableSize.width],
    );
    const innerLineSize = useMemo(
      () => Math.max(4, Math.floor(tableSize.width / 13)),
      [tableSize.width],
    );
    const lineThickness = useMemo(
      () => Math.max(4, Math.floor(tableSize.width / 180)),
      [tableSize.width],
    );
    const innerScale = useMemo(
      () =>
        isPortrait
          ? Math.min(1, tableSize.width / 460)
          : Math.min(1, tableSize.width / 780),
      [tableSize.width],
    );
    const blur = useMemo(() => lineThickness, [lineThickness]);
    const [showRanks, setShowRanks] = useState(false);
    const renderPotAnimations = useMemo(
      () => !isPortrait && tableSize.width > 800,
      [tableSize.width, isPortrait],
    );

    const inner = useMemo(() => {
      if (!community_cards && !children && !onShowWinners) return;
      return (
        <>
          {community_cards && (
            <Interactable onClick={() => setShowRanks(true)}>
              <CommunityCardsComponent community_cards={community_cards} />
            </Interactable>
          )}
          {renderPotAnimations && !!children && (
            <div className="w-full rounded-[32px] mt-4 border-[4px] border-material-main-2 relative h-[126px] flex flex-col justify-center items-center gap-1">
              {children}
            </div>
          )}

          {/* {onShowWinners && <PillComponent className='mt-2 mx-auto' onClick={onShowWinners}>See winners</PillComponent>} */}
          {!renderPotAnimations && !!children && (
            <div className="w-full rounded-[32px] mt-4 border-[4px] border-material-main-2 relative flex flex-col justify-center items-center gap-1">
              {children}
            </div>
          )}
        </>
      );
    }, [children, community_cards, onShowWinners, renderPotAnimations]);

    const background = useMemo(
      () =>
        `radial-gradient(50% 55.6% at 50% 44.4%, ${bgBright} 0%, ${bgDark} 100%)`,
      [visuals.color],
    );

    const isBTC = useIsBTC();

    return (
      <ProvideTableVisuals {...visuals}>
        <RankCardsModalComponent
          isOpen={showRanks}
          onClose={() => setShowRanks(false)}
        />

        <div
          className={classNames(className)}
          ref={containerRef}
          style={{
            "--table-blur": `${blur}px`,
            "--container-width": `${tableSize.width}px`,
            "--container-height": `${tableSize.height}px`,
          } as React.CSSProperties}
        >
          <div
            style={{
              ...tableSize,
              padding: `${woodSize}px`,
            }}
            className={classNames(
              "m-auto relative rounded-full from-[#6D290C] to-[#C89467] shadow-table z-0 overflow-hidden",
              {
                "aspect-1040/600 bg-linear-to-t": !isPortrait,
                "aspect-600/1040 bg-linear-to-r": isPortrait,
              },
            )}
          >

            <div className={classNames(
              'bg-black absolute opacity-[12%]',
              isPortrait ? 'inset-x-0 top-1/3 h-[5px]' : 'inset-y-0 left-1/3 w-[5px]'
            )} />
            <div className={classNames(
              'bg-black absolute opacity-[12%]',
              isPortrait ? 'inset-x-0 top-2/3 h-[5px]' : 'inset-y-0 left-2/3 w-[5px]'
            )} />

            <div className={classNames(
              'bg-black absolute opacity-[12%]',
              !isPortrait ? 'inset-x-0 top-1/2 h-[5px]' : 'inset-y-0 left-1/2 w-[5px]'
            )} />

            <div className="pointer-events-none w-full h-full rounded-full bg-linear-to-br from-[#FFEC72] to-[#5D5228] p-1 shadow-table-rim relative z-1">
              <div
                className="w-full h-full rounded-full relative z-10 overflow-hidden"
                style={{ background }}
              >
                <div
                  className="absolute shadow-table-bump rounded-full z-1 blur-(--table-blur)"
                  style={{ background, inset: `${upperSize}px` }}
                />
                <div
                  style={{ backgroundImage: "url(/images/grain.png)" }}
                  className="absolute -inset-5 rounded-full mix-blend-screen opacity-[0.08] z-2"
                />
                <div
                  className="absolute rounded-full border-material-main-2 inset-(--table-inner-line-size) z-3"
                  style={{
                    borderWidth: `${lineThickness}px`,
                    inset: `${innerLineSize}px`,
                  }}
                />

              </div>
            </div>

            {isBTC && (
              <div className='absolute inset-0 z-[1] flex justify-center items-center opacity-10' style={{ scale: innerScale }}>
                <Image
                  className='size-80 bg-blend-darker'
                  src={BitcoinSymbol}
                  alt="bitcoin"
                  type="svg"
                />
              </div>
            )}

            {inner && innerScale > 0.1 && (
              <div className="absolute inset-0 justify-center items-center flex">
                <div
                  className="flex flex-col justify-between z-1 relative"
                  style={{ transform: `scale(${innerScale})` }}
                >
                  {inner}
                </div>
              </div>
            )}

          </div>
        </div>
      </ProvideTableVisuals>
    );
  },
  (prevProps, nextProps) =>
    prevProps.className === nextProps.className &&
    IsSameHand(prevProps.community_cards, nextProps.community_cards) &&
    prevProps.visuals?.color === nextProps.visuals?.color &&
    prevProps.visuals?.cardColor === nextProps.visuals?.cardColor &&
    prevProps.children === nextProps.children &&
    prevProps.tableSize?.width === nextProps.tableSize?.width &&
    prevProps.tableSize?.height === nextProps.tableSize?.height &&
    prevProps.onShowWinners === nextProps.onShowWinners &&
    prevProps.showPlayers === nextProps.showPlayers
);
TableBackgroundComponent.displayName = "TableBackgroundComponent";
