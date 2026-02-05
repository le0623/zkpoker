import classNames from 'classnames';
import { motion } from 'framer-motion';
import { memo, ReactNode, useState } from 'react';

import { Interactable } from '@zk-game-dao/ui';

export const HUDQuickActionComponent = memo<{
  label: ReactNode;
  onClick(): void;
  isSelected: boolean;
  currentValue: bigint;
  targetValue: bigint;
}>(({ label, onClick, isSelected, currentValue, targetValue }) => {
  return (
    <Interactable
      onClick={onClick}
      className={classNames(
        "flex material leading-none transition-all duration-200 relative",
        "text-white type-button-2 p-2 rounded-sm",
        "active:scale-95 hover:scale-105",
        isSelected
          ? "bg-material-main-3 bg-opacity-100 shadow-lg ring-2 ring-white ring-opacity-50"
          : "bg-neutral-400 bg-opacity-70 hover:bg-opacity-90",
        currentValue === targetValue && !isSelected && "ring-1 ring-white ring-opacity-30"
      )}
    >
      <motion.p
        animate={{
          scale: isSelected ? [1, 1.1, 1] : 1,
        }}
        transition={{ duration: 0.3 }}
      >
        {label}
      </motion.p>
    </Interactable>
  );
});
HUDQuickActionComponent.displayName = "QuickAction";

type HUDQuickActionsComponentProps = {
  quickActions: [bigint, string][];
  onChange(raiseValue: bigint): void;
  currentValue: bigint;
};

export const HUDQuickActionsComponent = memo<HUDQuickActionsComponentProps>(({ quickActions, onChange, currentValue }) => {
  const [selectedAmount, setSelectedAmount] = useState<bigint | null>(null);

  const handleQuickAction = (amount: bigint) => {
    setSelectedAmount(amount);
    onChange(amount);
    // Reset selection after a short delay to show feedback
    setTimeout(() => setSelectedAmount(null), 300);
  };

  return (
    <motion.div
      variants={{
        visible: {
          opacity: 1,
          y: -8,
          scale: 1,
        },
        hidden: {
          opacity: 0,
          y: 16,
          scale: 0.9,
        },
      }}
      initial="hidden"
      animate="visible"
      exit="hidden"
      className="flex flex-row pt-4 justify-center items-center gap-1.5 whitespace-nowrap relative"
    >
      {quickActions.map(([amount, label]) => (
        <HUDQuickActionComponent
          key={label}
          onClick={() => handleQuickAction(amount)}
          label={label}
          isSelected={selectedAmount === amount}
          currentValue={currentValue}
          targetValue={amount}
        />
      ))}
    </motion.div>
  );
}, (prevProps, nextProps) => {
  if (prevProps.quickActions.length !== nextProps.quickActions.length ||
    prevProps.onChange !== nextProps.onChange ||
    prevProps.currentValue !== nextProps.currentValue)
    return false;

  const sortedPrevActions = [...prevProps.quickActions].sort((a, b) => Number(a[0] - b[0]));
  const sortedNextActions = [...nextProps.quickActions].sort((a, b) => Number(a[0] - b[0]));
  return sortedPrevActions.every((action, index) => sortedNextActions[index][0] === action[0] && sortedNextActions[index][1] === action[1]);
});
HUDQuickActionsComponent.displayName = "QuickActions";
