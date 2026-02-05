import classNames from 'classnames';
import { AnimatePresence, motion } from 'framer-motion';
import { memo } from 'react';

import { User } from '@declarations/users_canister/users_canister.did';
import CheckBadgeIcon16 from '@heroicons/react/16/solid/CheckBadgeIcon';
import CheckBadgeIcon24 from '@heroicons/react/24/solid/CheckBadgeIcon';
import { DealerButtonComponent } from '@lib/table/components/table/dealer-button.component';
import { IsSameAvatar } from '@lib/utils/compare';
import { UnwrapOptional } from '@lib/utils/optional';
import { Image, PillComponent } from '@zk-game-dao/ui';

export const AVATAR_EMOJI_STYLES = [
  { className: "bg-green-500" },
  { className: "bg-red-500" },
  { className: "bg-[#FF6861]" },
  { className: "bg-[#64D3FF]" },
  { className: "bg-yellow-500" },
  { className: "bg-purple-500" },
  { className: "bg-black" },
  { className: "bg-neutral-200" },
];

type AvatarSize = "microscopic" | "between-small-and-microscopic" | "small" | "medium" | "big";

type AvatarProps = {
  size?: AvatarSize;
  className?: string;
  onEdit?(): void;
  isDealer?: boolean;
} & Pick<Partial<User>, "avatar" | 'is_verified'>;

export const AvatarComponent = memo<AvatarProps>(({ size = "small", className, onEdit, avatar, isDealer = false, is_verified }) => {
  return (
    <div
      className={classNames(className, "transition-all grow-0 shrink-0", {
        relative: !className?.includes("absolute"),
        "w-[120px] h-[120px]": size === "big",
        "w-[52px] h-[52px]": size === "medium",
        "w-[32px] h-[32px]": size === "small",
        "w-[28px] h-[28px]": size === "between-small-and-microscopic",
        "w-[24px] h-[24px]": size === "microscopic",
      })}
    >
      <div
        className={classNames(
          "rounded-full material justify-center items-center flex w-full h-full relative"
        )}
      >
        {avatar?.[0]?.Emoji?.emoji !== undefined ? (
          <Image
            className="object-contain w-full h-full"
            width={48}
            height={48}
            type="png"
            src={`/nfts/avatars/emojis/${avatar[0].Emoji.emoji}.png`}
            alt={`emoji-${avatar[0].Emoji.emoji}`}
          />
        ) : (
          <img src="/icons/person.svg" alt="avatar" />
        )}
        {UnwrapOptional(is_verified) && size === 'big' && <CheckBadgeIcon24
          className={classNames(
            'size-6 absolute -right-1.5 fill-purple-500',
          )}
        />}
        {UnwrapOptional(is_verified) && size === 'medium' && <CheckBadgeIcon24
          className={classNames(
            'size-5 absolute -right-1.5 fill-purple-500',
          )}
        />}
        {UnwrapOptional(is_verified) && size === 'small' && <CheckBadgeIcon24
          className={classNames(
            'size-4 absolute -right-1.5 fill-purple-500',
          )}
        />}
        {UnwrapOptional(is_verified) && size === 'microscopic' && <CheckBadgeIcon16
          className={classNames(
            'size-3 absolute -right-1 fill-purple-500',
          )}
        />}
      </div>

      <AnimatePresence>
        {/* {progress !== undefined && (
          <CircularProgressBarComponent
            progress={progress}
            className="absolute left-0 top-0 z-[1] w-full"
          />
        )} */}
        {onEdit && (
          <PillComponent
            onClick={onEdit}
            size="small"
            className="absolute bottom-0 left-1/2 -translate-x-1/2 translate-y-1/2 z-1"
          >
            Edit
          </PillComponent>
        )}
        {isDealer && (
          <motion.div
            initial={{ opacity: 0, y: -4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: 0 }}
            className="absolute -bottom-1 -right-1"
          >
            <DealerButtonComponent />
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
},
  (prevProps, nextProps) =>
    prevProps.size === nextProps.size &&
    prevProps.className === nextProps.className &&
    IsSameAvatar(UnwrapOptional(prevProps.avatar), UnwrapOptional(nextProps.avatar)) &&
    UnwrapOptional(prevProps.is_verified) === UnwrapOptional(nextProps.is_verified) &&
    prevProps.isDealer === nextProps.isDealer &&
    prevProps.onEdit === nextProps.onEdit
);
AvatarComponent.displayName = "AvatarComponent";

export default AvatarComponent;
