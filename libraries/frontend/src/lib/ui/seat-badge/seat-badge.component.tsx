import { memo } from "react";
import classNames from "classnames";

export const SeatBadge = memo<{
  seatNumber: number;
  visible: boolean;
  className?: string;
}>(({ seatNumber, visible, className }) => {
  if (!visible) return null;

  return (
    <div
      className={classNames(
        "absolute -top-2 -right-2 bg-neutral-700 text-white",
        "rounded-full w-6 h-6 flex items-center justify-center",
        "text-xs font-bold border-2 border-neutral-500",
        "z-20 shadow-lg",
        className
      )}
    >
      {seatNumber}
    </div>
  );
});
SeatBadge.displayName = "SeatBadge";
