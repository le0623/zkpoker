/**
 * RNG Transparency Types
 * 
 * Re-exports and type helpers for RNG transparency dashboard
 */

import type {
  RngMetadata,
  CardProvenance,
  RngStats,
  Card,
  DealStage,
} from '@declarations/table_canister/table_canister.did';
import type { Principal } from '@dfinity/principal';

// Re-export types from declarations
export type { RngMetadata, CardProvenance, RngStats, Card, DealStage };

/**
 * Format card for consistent string representation
 * Used for hash calculation to match backend format
 */
export function formatCard(card: Card): string {
  return `${card.value}:${card.suit}`;
}

/**
 * Shorten hash for display (first 8 chars + ... + last 4 chars)
 */
export function shortenHash(hash: string, prefixLength = 8, suffixLength = 4): string {
  if (hash.length <= prefixLength + suffixLength) {
    return hash;
  }
  return `${hash.slice(0, prefixLength)}...${hash.slice(-suffixLength)}`;
}

/**
 * Format timestamp (nanoseconds) to readable date string
 */
export function formatTimestamp(timestampNs: bigint): string {
  const timestampMs = Number(timestampNs / BigInt(1_000_000));
  return new Date(timestampMs).toLocaleString();
}

/**
 * Format bytes array for display
 */
export function formatBytes(bytes: Uint8Array | number[]): string {
  const arr = bytes instanceof Uint8Array ? Array.from(bytes) : bytes;
  return arr.map(b => b.toString(16).padStart(2, '0')).join(' ');
}

/**
 * Helper type for RNG dashboard state
 */
export interface RngDashboardState {
  isOpen: boolean;
  roundId?: bigint;
  cardHash?: string;
}

/**
 * Helper type for card hash calculation parameters
 */
export interface CardHashParams {
  card: Card;
  roundId: bigint;
  position: number;
}





