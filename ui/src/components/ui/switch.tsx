import { Switch as SwitchPart, SwitchThumb } from "@ui-sdk/react/switch";
import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

export type SwitchProps = Omit<ComponentProps<typeof SwitchPart>, "className" | "children"> & {
  className?: string;
};

/** A UI SDK Switch on the wall: a hairline track, the thumb in label paper, lamp white when on. */
export function Switch({ className, ...props }: SwitchProps) {
  return (
    <SwitchPart
      className={cn(
        "inline-flex h-7 w-12 shrink-0 items-center rounded-full border border-line bg-wall/40 p-0.5 outline-none",
        "transition-[background-color,border-color]",
        "data-checked:border-label-dim data-checked:bg-label-faint/40",
        "data-disabled:opacity-40",
        className,
      )}
      {...props}
    >
      <SwitchThumb
        className={cn(
          "block size-5.5 rounded-full bg-label-dim transition-[translate,background-color]",
          "data-checked:translate-x-5 data-checked:bg-lamp",
        )}
      />
    </SwitchPart>
  );
}
