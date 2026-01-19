import { ProfileModalComponent } from "#ui/profile/profile-modal.component";
import { PlayerTag } from "@lib/ui/player-tag/player-tag.component";
import { SeatBadge } from "@lib/ui/seat-badge/seat-badge.component";
import { useUser } from "@lib/user";
import { memo, useMemo, useState } from "react";

import { useTableSeat } from "../../../context/table-seat.context";
import { useTable } from "../../../context/table.context";
import { TakeASeatComponent } from "./take-a-seat.component";

export const TablePlayer = memo(() => {
  const { isJoined, currencyType: currency, table } = useTable();
  const { user: zkpUser } = useUser();
  const {
    user,
    data,
    cards,
    isSelf,
    position,
    userTurnProgress,
    isDealer,
    isQueued,
    seatIndex,
  } = useTableSeat();
  const [isShowingProfile, setIsShowingProfile] = useState(false);

  // Seat number display logic:
  // - Before game starts (Fresh): Show ALL seat numbers (occupied + empty)
  // - During game: Show for occupied seats, AND for empty seats if new player is viewing
  // - After game ends: Show ALL seat numbers again
  const showSeatNumbers = useMemo(() => {
    const isBeforeGameStart = "Fresh" in table.deal_stage;
    const isGameEnded = !!table.sorted_users[0];
    const isNewPlayerViewing = !!zkpUser && !isJoined;

    // If seat is occupied, always show seat number
    if (user) {
      return true;
    }

    // For empty seats: show before game starts, after game ends, or when new player is viewing
    return isBeforeGameStart || isGameEnded || isNewPlayerViewing;
  }, [user, table.deal_stage, table.sorted_users, zkpUser, isJoined]);

  if (user)
    return (
      <div className="relative">
        <SeatBadge seatNumber={seatIndex + 1} visible={showSeatNumbers} />
        <ProfileModalComponent
          user={user}
          onClose={() => setIsShowingProfile(false)}
          isOpen={isShowingProfile}
        />
        <PlayerTag
          onClick={() => setIsShowingProfile(true)}
          {...data}
          {...user}
          currencyType={currency}
          cards={cards}
          isSelf={isSelf}
          direction={position.vertical === "top" ? "down" : "up"}
          turnProgress={userTurnProgress}
          isQueued={isQueued}
          isDealer={isDealer}
        />
      </div>
    );

  if (!!zkpUser && !isJoined)
    return (
      <div className="relative">
        <SeatBadge seatNumber={seatIndex + 1} visible={showSeatNumbers} />
        <TakeASeatComponent />
      </div>
    );

  // Empty seat (no user and current user not trying to join)
  return (
    <div className="relative">
      <SeatBadge seatNumber={seatIndex + 1} visible={showSeatNumbers} />
    </div>
  );

  return <></>;
});
TablePlayer.displayName = "TablePlayer";
