import { useUser } from '@lib/user';
import classNames from 'classnames';
import { createContext, memo, ReactNode, useContext, useMemo } from 'react';

import { Card } from '@declarations/table_canister/table_canister.did';

import { SeatMetaData, useCurrentTableTurnProgressRemainder, useTable } from './table.context';
import { BuildHand } from '../utils/hand';
import { calculatePosition, TableSeatPosition } from '../utils/table-position';

type TableSeatContextValue = {
  userTurnProgress?: number;
  seatIndex: number;
  isQueued: boolean;
  isDealer: boolean;
  isSelf: boolean;
  cards: Card[];
  position: TableSeatPosition;
} & SeatMetaData;

const TableSeatContext = createContext<TableSeatContextValue>({} as TableSeatContextValue);

export const TableSeatContextProvider = memo<{ children: ReactNode, seatIndex: number, position: TableSeatPosition }>(({
  children,
  seatIndex,
  position
}) => {
  const { getSeat } = useTable();
  const { table } = useTable();
  const { user: zkpUser } = useUser();

  const seatMeta = getSeat(seatIndex);

  const userTurnProgress = useCurrentTableTurnProgressRemainder(
    table && table.current_player_index === BigInt(seatIndex),
  );

  const value = useMemo((): Omit<TableSeatContextValue, 'userTurnProgress'> => {
    const isSelf = !!zkpUser && !!seatMeta?.user && zkpUser.principal_id.compareTo(seatMeta.user?.principal_id) === 'eq';
    const isDealer = table?.dealer_position === BigInt(seatIndex);
    const isQueued = !!seatMeta?.status && 'QueuedForNextRound' in seatMeta.status;
    
    // Get current viewer's principal to check if they're in showdown
    // Folded players should not see showdown cards
    const currentViewerPrincipal = zkpUser?.principal_id?.toText();
    
    const cards = BuildHand(isSelf, table, seatMeta?.data, currentViewerPrincipal);
    return {
      ...seatMeta,
      cards,
      isSelf,
      isDealer,
      isQueued,
      position,
      seatIndex,
    };
  }, [zkpUser, seatMeta, table, seatIndex, position]);

  return (
    <TableSeatContext.Provider value={{
      ...value,
      userTurnProgress,
    }}>
      {children}
    </TableSeatContext.Provider>
  );
});
TableSeatContextProvider.displayName = 'TableSeatContextProvider';

/** Calculates the position of a seat around a pill shaped table */
export const PositionOnTable = memo<{
  /** Width of the pill shaped table */
  width: number;
  /** Height of the pill shaped table */
  height: number;

  seat: number;
  seatAmount: number;
  children: ReactNode;
}>(({ seat, seatAmount: maxSeats, width, height, children }) => {

  const { transform, positionContext, ...position } = useMemo(() => calculatePosition({
    seat,
    maxSeats,
    width,
    height,
  }), [seat, maxSeats, width, height]);

  return (
    <TableSeatContextProvider
      position={positionContext}
      seatIndex={seat}
    >
      <TableSeatContextConsumer>
        {({ isSelf }) => (
          <div className={classNames("absolute h-0 w-0", isSelf ? 'z-11' : 'z-10')} style={position}>
            <div className="flex">
              <div
                className="flex justify-center items-center"
                style={{ transform }}
              >
                {children}
              </div>
            </div>
          </div>
        )}
      </TableSeatContextConsumer>
    </TableSeatContextProvider>
  );
});
PositionOnTable.displayName = 'PositionOnTable';

export const useTableSeat = () => useContext(TableSeatContext);
export const TableSeatContextConsumer = TableSeatContext.Consumer;
