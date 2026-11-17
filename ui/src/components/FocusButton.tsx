// A button that is a focus item and carries the lamp when focused: the control of a form-like
// screen (Settings, first run, the folder browser). It must sit in a FocusGroup.

import type { ReactNode } from "react";
import { useFocusItem } from "@/focus/react";
import { cn } from "@/lib/utils";
import { Lamp } from "./Lamp";
import { Button } from "./ui/button";

export interface FocusButtonProps {
  focusKey: string;
  onSelect: () => void;
  /** `solid` for the one action that goes ahead, `line` for the others, `plain` for quiet ones. */
  variant?: "solid" | "line" | "plain";
  disabled?: boolean;
  className?: string;
  children: ReactNode;
}

const focusStyle = {
  solid: "",
  line: "focus:border-lamp",
  plain: "focus-visible:bg-transparent focus:text-label",
};

export function FocusButton({ focusKey, onSelect, variant = "line", disabled, className, children }: FocusButtonProps) {
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({ key: focusKey, onSelect, disabled });
  return (
    <Button ref={ref} {...props} variant={variant} className={cn("relative aria-disabled:opacity-40", focusStyle[variant], className)}>
      {children}
      {focused && <Lamp />}
    </Button>
  );
}
