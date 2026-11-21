import { useEffect, useState, type CSSProperties } from "react";
import {
  Toast,
  ToastAction,
  ToastDescription,
  ToastRegion,
  ToastTitle,
  toast as sdkToast,
  type ToastOptions,
} from "@ui-sdk/react/toast";
import { cn } from "@/lib/utils";
import { buttonVariants } from "./button";

const SHOWN_MS = 4000;

type Send = (message: string | ToastOptions<string>, options?: ToastOptions<string>) => string;

// A message already showing is updated in place rather than stacked again, so pressing an
// unavailable Play three times says it once.
function once(send: Send): Send {
  return (message, options) => {
    const title = typeof message === "string" ? message : message.title;
    return send(message, { id: title, duration: SHOWN_MS, ...options });
  };
}

export const toast = Object.assign(once(sdkToast), {
  error: once(sdkToast.error),
  dismiss: sdkToast.dismiss,
});

/**
 * Where `toast()` messages appear: the foot of the right-hand rail, standing on the hint line,
 * clear of the hero's artwork and placard. The newest is in front at the bottom; older ones are
 * an edge above it until the stack is hovered or focused. Place once at the app root.
 */
export function Toaster() {
  const away = useAway();
  return (
    <ToastRegion
      side="bottom"
      align="end"
      maxVisible={3}
      shortcut={null}
      data-away={away || undefined}
      className={cn(
        "fixed right-edge bottom-(--toast-floor) z-50 h-(--toast-front-height) w-(--rail)",
        "data-expanded:h-[calc(var(--toast-stack-height)+(var(--toast-count)-1)*0.75rem)]",
      )}
    >
      {(item) => (
        <Toast
          className={cn(
            "absolute inset-x-0 bottom-0 z-[calc(10-var(--toast-index))] flex h-(--toast-height) origin-top items-start gap-6 overflow-hidden select-none",
            "rounded-control border border-line bg-wall-raised px-6 py-5",
            "-translate-y-[calc(var(--toast-index)*0.75rem)] scale-[calc(1-var(--toast-index)*0.04)]",
            "transition-[translate,scale,opacity] duration-(--dur-screen) *:transition-opacity *:duration-(--dur-screen)",
            "not-data-front:not-data-expanded:h-(--toast-front-height) not-data-front:not-data-expanded:opacity-60 not-data-front:not-data-expanded:*:opacity-0",
            "data-expanded:-translate-y-[calc(var(--toast-offset)+var(--toast-index)*0.75rem)] data-expanded:scale-100",
            "data-starting-style:translate-y-4 data-starting-style:opacity-0 data-ending-style:opacity-0",
            "motion-reduce:transition-opacity",
          )}
        >
          {/* A long path (a drive's folder) wraps anywhere rather than running past the edge. */}
          <div className="flex min-w-0 flex-1 flex-col gap-1 wrap-anywhere">
            <ToastTitle className="font-display text-body font-ui text-label" />
            <ToastDescription className="font-reading text-body leading-reading text-label-dim" />
          </div>
          <ToastAction className={buttonVariants({ variant: "line", size: "sm" })} />
          {Number.isFinite(item.duration) && (
            <span
              key={item.version}
              aria-hidden
              className="toast-timer"
              style={{ "--toast-ms": `${item.duration}ms` } as CSSProperties}
            />
          )}
        </Toast>
      )}
    </ToastRegion>
  );
}

// The SDK holds toast timers while the window is inactive or hidden; the timer line holds with them.
function useAway(): boolean {
  const [away, setAway] = useState(() => !document.hasFocus() || document.hidden);
  useEffect(() => {
    const check = () => setAway(!document.hasFocus() || document.hidden);
    window.addEventListener("focus", check);
    window.addEventListener("blur", check);
    document.addEventListener("visibilitychange", check);
    return () => {
      window.removeEventListener("focus", check);
      window.removeEventListener("blur", check);
      document.removeEventListener("visibilitychange", check);
    };
  }, []);
  return away;
}
