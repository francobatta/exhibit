import {
  Collapsible as CollapsiblePart,
  CollapsiblePanel as CollapsiblePanelPart,
  CollapsibleTrigger as CollapsibleTriggerPart,
} from "@ui-sdk/react/collapsible";
import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

/** A UI SDK Collapsible: one panel that its trigger shows and hides. The root draws nothing. */
export function Collapsible({ className, ...props }: Omit<ComponentProps<typeof CollapsiblePart>, "className"> & { className?: string }) {
  return <CollapsiblePart className={cn("contents", className)} {...props} />;
}

/** The trigger: a native button, styled by the caller (a placard's cue, say). */
export function CollapsibleTrigger(props: ComponentProps<typeof CollapsibleTriggerPart>) {
  return <CollapsibleTriggerPart {...props} />;
}

/**
 * The panel: it comes in by fading up, never by growing its height (only opacity animates), and
 * at once when the owner prefers reduced motion.
 */
export function CollapsiblePanel({ className, ...props }: Omit<ComponentProps<typeof CollapsiblePanelPart>, "className"> & { className?: string }) {
  return (
    <CollapsiblePanelPart
      className={cn(
        "transition-opacity duration-(--dur-screen) ease-out motion-reduce:transition-none",
        "data-starting-style:opacity-0 data-ending-style:opacity-0",
        className,
      )}
      {...props}
    />
  );
}
