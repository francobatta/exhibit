import { Toggle as TogglePart } from "@ui-sdk/react/toggle";
import type { VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";
import { buttonVariants } from "./button";

export type ToggleProps = Omit<ComponentProps<typeof TogglePart>, "className"> &
  Pick<VariantProps<typeof buttonVariants>, "size"> & { className?: string };

/** A button that stays pressed (Loved, Watched); pressed reads as label white. */
export function Toggle({ className, size, ...props }: ToggleProps) {
  return (
    <TogglePart
      className={cn(
        buttonVariants({ variant: "line", size }),
        "text-label-dim data-pressed:border-label-dim data-pressed:text-label",
        className,
      )}
      {...props}
    />
  );
}
