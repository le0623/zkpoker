import { PublicTable } from '@declarations/table_canister/table_canister.did';
import { Principal } from '@dfinity/principal';

/**
 * Maps principal ID to seat number (1-based for display)
 * Returns undefined if principal not found in any seat
 */
export function getSeatNumberForPrincipal(
  table: PublicTable,
  principalId?: Principal
): number | undefined {
  if (!principalId) return undefined;
  
  const seatIndex = table.seats.findIndex((seat) => {
    if ('Occupied' in seat) {
      return seat.Occupied.compareTo(principalId) === 'eq';
    }
    if ('QueuedForNextRound' in seat) {
      return seat.QueuedForNextRound[0].compareTo(principalId) === 'eq';
    }
    return false;
  });
  
  return seatIndex >= 0 ? seatIndex + 1 : undefined; // +1 for 1-based display
}

/**
 * Gets all seat assignments for the current round
 * Returns a map of principal -> seat number
 */
export function getAllSeatAssignments(
  table: PublicTable
): Map<string, number> {
  const seatMap = new Map<string, number>();
  
  table.seats.forEach((seat, index) => {
    let principalId: Principal | undefined;
    
    if ('Occupied' in seat) {
      principalId = seat.Occupied;
    } else if ('QueuedForNextRound' in seat) {
      principalId = seat.QueuedForNextRound[0];
    }
    
    if (principalId) {
      seatMap.set(principalId.toText(), index + 1); // 1-based
    }
  });
  
  return seatMap;
}

