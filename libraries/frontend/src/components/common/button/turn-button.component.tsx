import { memo } from "react";
import { WeirdKnobProps } from "@zk-game-dao/ui";

export const TurnButtonComponent = memo<WeirdKnobProps>(({ mutate, isPending, children }) => {
  return (
    <button className='text-md cursor-pointer w-[100px] h-[42px] hover:scale-95 active:scale-90 transition-all duration-200'
      style={{
        backgroundImage: "url(/images/red-btn.png)",
        backgroundSize: "cover",
      }}
      onClick={mutate}
    >
      {isPending ? '' : children}
    </button>
  );
});

TurnButtonComponent.displayName = "TurnButtonComponent";