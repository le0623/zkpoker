import { useEffect, useState, useMemo } from "react";
import type { Card } from "@declarations/table_canister/table_canister.did";
import { calculateCardHash } from "../utils/card-hash";

/**
 * Hook to calculate card hash asynchronously
 * Returns null while calculating, then the hash string
 */
export function useCardHash(
  card: Card | undefined,
  roundId: bigint | undefined,
  position: number
): string | null {
  const [hash, setHash] = useState<string | null>(null);

  // Create a stable key from the card to avoid infinite loops
  const cardKey = useMemo(() => {
    if (!card) return "undefined";
    return `${Object.keys(card.value)[0]}-${Object.keys(card.suit)[0]}`;
  }, [card]);

  useEffect(() => {
    if (!card || roundId === undefined) {
      setHash(null);
      return;
    }

    let cancelled = false;

    calculateCardHash(card, roundId, position)
      .then((h) => {
        if (!cancelled) setHash(h);
      })
      .catch((err) => {
        console.error("Failed to calculate card hash:", err);
        if (!cancelled) setHash(null);
      });

    return () => {
      cancelled = true;
    };
  }, [cardKey, roundId, position]);

  return hash;
}

/**
 * Hook to calculate multiple card hashes at once
 * More efficient than calling useCardHash multiple times
 */
export function useCardHashes(
  cards: (Card | undefined)[],
  roundId: bigint | undefined
): (string | null)[] {
  const [hashes, setHashes] = useState<(string | null)[]>([]);

  // Create a stable serialized key from the cards array to avoid infinite loops
  const cardsKey = useMemo(() => {
    return cards
      .map((card) =>
        card
          ? `${Object.keys(card.value)[0]}-${Object.keys(card.suit)[0]}`
          : "undefined"
      )
      .join(",");
  }, [cards]);

  useEffect(() => {
    if (!roundId || cards.length === 0) {
      setHashes(cards.map(() => null));
      return;
    }

    let cancelled = false;

    Promise.all(
      cards.map((card, idx) =>
        card
          ? calculateCardHash(card, roundId, idx).catch(() => null)
          : Promise.resolve(null)
      )
    ).then((results) => {
      if (!cancelled) setHashes(results);
    });

    return () => {
      cancelled = true;
    };
  }, [cardsKey, roundId]);

  return hashes;
}





