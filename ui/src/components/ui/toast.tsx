import {
  Toast,
  ToastAction,
  ToastDescription,
  ToastRegion,
  ToastTitle,
} from "@ui-sdk/react/toast";
import { cn } from "@/lib/utils";
import { buttonVariants } from "./button";

export { toast } from "@ui-sdk/react/toast";

/**
 * Where `toast()` messages appear: top right under the bar, over the artwork and clear of the
 * placards, action lists and shelves on the left and the pad hints at the bottom; one at a time
 * in front, quiet. Place once at the app root.
 */
export function Toaster() {
  return (
    <ToastRegion
      side="top"
      align="end"
      maxVisible={3}
      shortcut={null}
      className="fixed top-bar right-edge z-50 h-(--toast-front-height) w-[30rem]"
    >
      {() => (
        <Toast
          className={cn(
            "absolute inset-x-0 top-0 flex h-(--toast-height) items-start gap-6 overflow-hidden",
            "rounded-control border border-line bg-wall-raised px-6 py-5",
            "translate-y-[calc(var(--toast-index)*0.75rem)] scale-[calc(1-var(--toast-index)*0.04)]",
            "transition-[translate,scale,opacity] duration-(--dur-screen)",
            "data-starting-style:-translate-y-4 data-starting-style:opacity-0",
            "data-ending-style:opacity-0 not-data-front:opacity-60",
            "motion-reduce:transition-opacity",
          )}
        >
          {/* A long path (a drive's folder) wraps anywhere rather than running past the edge. */}
          <div className="flex min-w-0 flex-1 flex-col gap-1 wrap-anywhere">
            <ToastTitle className="font-display text-body font-ui text-label" />
            <ToastDescription className="font-reading text-body leading-reading text-label-dim" />
          </div>
          <ToastAction className={buttonVariants({ variant: "line", size: "sm" })} />
        </Toast>
      )}
    </ToastRegion>
  );
}
