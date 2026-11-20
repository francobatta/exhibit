import { Toggle as TogglePart } from "@ui-sdk/react/toggle";
import type { VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";
import { buttonVariants } from "./button";

export type ToggleProps = Omit<ComponentProps<typeof TogglePart>, "className"> &
  Pick<VariantProps<typeof buttonVariants>, "size"> & {
    /** `line` (Loved, Watched): a hairline that firms up when pressed. `plain`: text on the wall, framed by a hairline when pressed. `cue`: a placard's round cue; pressed shows in its filled icon. */
    variant?: "line" | "plain" | "cue";
    className?: string;
  };

const pressed = {
  line: "data-pressed:border-label-dim",
  plain: "data-pressed:ring-1 data-pressed:ring-inset data-pressed:ring-label-faint",
  cue: "",
};

/** A button that stays pressed; pressed reads as label white. */
export function Toggle({ className, size, variant = "line", ...props }: ToggleProps) {
  return (
    <TogglePart
      className={cn(
        buttonVariants({ variant, size }),
        "text-label-dim data-pressed:text-label",
        pressed[variant],
        className,
      )}
      {...props}
    />
  );
}
