import { memo } from "react";
import { shortenHash } from "../../types/rng.types";

/**
 * Simple hash badge displayed on the back of cards
 * Shows shortened hash for transparency
 */
export const CardHashBadge = memo<{
  hash: string;
  size?: "small" | "medium";
}>(({ hash, size = "small" }) => {
  const shortened = shortenHash(hash, 6, 4);
  
  return (
    <div
      className="absolute bottom-0 left-0 right-0 bg-black/60 text-white text-center backdrop-blur-sm"
      style={{
        fontSize: size === "small" ? "8px" : "10px",
        padding: size === "small" ? "2px 4px" : "3px 5px",
        lineHeight: 1.2,
      }}
      title={`Card Hash: ${hash}`}
    >
      {shortened}
    </div>
  );
});

CardHashBadge.displayName = "CardHashBadge";







