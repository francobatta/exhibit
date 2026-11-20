// Back, drawn: an arrow and the word where the bar's mark stands, on a screen stacked over another,
// and in the same place over Ask and first run's later steps. A focus item like any other, so the
// pad and the keys reach it too. B, Esc and the mouse's back button do the same thing.

import { useFocusItem } from "@/focus/react";
import { useT } from "@/i18n";
import { cn } from "@/lib/utils";
import { Lamp } from "./Lamp";
import { Button } from "./ui/button";

function Arrow() {
  return (
    <svg
      viewBox="0 0 24 24"
      aria-hidden
      fill="none"
      className="size-[1.25em] transition-transform duration-(--dur-control) group-hover:-translate-x-0.5 group-focus:-translate-x-0.5"
    >
      <path d="M19 12H5M11 6l-6 6 6 6" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

export interface BackButtonProps {
  focusKey: string;
  onBack: () => void;
  className?: string;
}

export function BackButton({ focusKey, onBack, className }: BackButtonProps) {
  const t = useT();
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({ key: focusKey, onSelect: onBack, hint: t("hints.back") });
  return (
    <Button
      ref={ref}
      {...props}
      className={cn("group relative focus-visible:bg-transparent focus:text-label", className)}
    >
      <Arrow />
      {t("hints.back")}
      {focused && <Lamp />}
    </Button>
  );
}
