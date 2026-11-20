// A work's state looks the same on every screen: a heart for loved, an eye for watched (Lucide,
// shadcn's icon set). The icon fills when the state holds: by `on`, or inside a pressed toggle.

import { Eye, Heart, type LucideProps } from "lucide-react";
import { cn } from "@/lib/utils";

type StateIconProps = Omit<LucideProps, "ref"> & { on?: boolean };

const line = "shrink-0 [stroke-width:var(--icon-stroke)]";

export function LovedIcon({ on = false, className, ...props }: StateIconProps) {
  return (
    <Heart
      aria-hidden
      className={cn(line, "in-data-pressed:fill-current", on && "fill-current", className)}
      {...props}
    />
  );
}

/** The eye's pupil fills; the eye itself stays a line. */
export function WatchedIcon({ on = false, className, ...props }: StateIconProps) {
  return (
    <Eye
      aria-hidden
      className={cn(line, "in-data-pressed:[&_circle]:fill-current", on && "[&_circle]:fill-current", className)}
      {...props}
    />
  );
}
