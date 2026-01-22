/**
 * Card Hash Calculation Utility
 * 
 * Calculates SHA-256 hash for card provenance verification.
 * Must match backend implementation exactly.
 * 
 * Backend format: SHA-256(round_id || card || position)
 * - round_id: 8 bytes, little-endian
 * - card: string format "{value}:{suit}"
 * - position: 1 byte
 */

import type { Card } from '@declarations/table_canister/table_canister.did';
import { formatCard } from '../types/rng.types';

/**
 * Calculate card hash matching backend implementation
 * 
 * @param card - The card to hash
 * @param roundId - The game round ID
 * @param position - Position in shuffled deck (0-51)
 * @returns SHA-256 hash as hex string
 */
export async function calculateCardHash(
  card: Card,
  roundId: bigint,
  position: number
): Promise<string> {
  // Step 1: Convert round_id to 8 bytes (little-endian)
  const roundIdBytes = new Uint8Array(8);
  const roundIdView = new DataView(roundIdBytes.buffer);
  roundIdView.setBigUint64(0, roundId, true); // true = little-endian

  // Step 2: Format card as string
  const cardString = formatCard(card);
  const encoder = new TextEncoder();
  const cardBytes = encoder.encode(cardString);

  // Step 3: Convert position to 1 byte
  const positionBytes = new Uint8Array(1);
  positionBytes[0] = position;

  // Step 4: Combine all bytes
  const combinedBytes = new Uint8Array(
    roundIdBytes.length + cardBytes.length + positionBytes.length
  );
  combinedBytes.set(roundIdBytes, 0);
  combinedBytes.set(cardBytes, roundIdBytes.length);
  combinedBytes.set(positionBytes, roundIdBytes.length + cardBytes.length);

  // Step 5: Calculate SHA-256 hash
  const hashBuffer = await crypto.subtle.digest('SHA-256', combinedBytes);
  const hashArray = Array.from(new Uint8Array(hashBuffer));
  const hashHex = hashArray.map(b => b.toString(16).padStart(2, '0')).join('');

  return hashHex;
}

/**
 * Synchronous version using Web Crypto API (if available)
 * Falls back to async version
 */
export function calculateCardHashSync(
  card: Card,
  roundId: bigint,
  position: number
): string | null {
  // Web Crypto API requires async, so we can't do truly sync
  // This is a placeholder - always use async version
  return null;
}








