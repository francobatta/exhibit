import { Button as ButtonPart } from "@ui-sdk/react/button";
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

export const buttonVariants = cva(
  [
    "inline-flex shrink-0 items-center justify-center gap-[0.5em] whitespace-nowrap rounded-control",
    "font-display font-ui leading-none outline-none select-none",
    "transition-[color,background-color,border-color,opacity]",
    "data-disabled:opacity-40 data-pending:opacity-70",
  ],
  {
    variants: {
      variant: {
        /** Text on the wall: dim at rest, lit when hovered or focused. */
        plain:
          "text-label-dim hover:text-label focus-visible:bg-lamp/10 focus-visible:text-label",
        /** The one main action: label paper, lamp white when focused. */
        solid: "bg-label text-wall hover:bg-lamp focus-visible:bg-lamp",
        /** A secondary action, drawn with a hairline. */
        line: "border border-line text-label hover:border-label-dim focus-visible:border-lamp focus-visible:bg-lamp/10",
      },
      size: {
        md: "h-11 px-4 text-body",
        sm: "h-9 px-3 text-caption",
        icon: "size-11 text-body",
      },
    },
    defaultVariants: { variant: "plain", size: "md" },
  },
);

export type ButtonProps = Omit<ComponentProps<typeof ButtonPart>, "className"> &
  VariantProps<typeof buttonVariants> & { className?: string };

/** A UI SDK Button in our type and light. */
export function Button({ className, variant, size, ...props }: ButtonProps) {
  return <ButtonPart className={cn(buttonVariants({ variant, size }), className)} {...props} />;
}
