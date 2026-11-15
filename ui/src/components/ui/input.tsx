import { Input as InputPart } from "@ui-sdk/react/input";
import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

export type InputProps = Omit<ComponentProps<typeof InputPart>, "className"> & {
  className?: string;
};

/** A text field on the wall: a hairline under the text, lamp light and caret when focused. */
export function Input({ className, ...props }: InputProps) {
  return (
    <InputPart
      className={cn(
        "h-11 w-full min-w-0 rounded-control border border-line bg-wall/40 px-4",
        "font-display text-body text-label placeholder:text-label-faint",
        "outline-none transition-[border-color,background-color]",
        "hover:border-label-faint focus:border-lamp/60 focus:bg-wall/70",
        "data-disabled:opacity-40",
        className,
      )}
      {...props}
    />
  );
}
