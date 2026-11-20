// Bottom right, only while the owner drives with a gamepad: what the buttons do here, from the
// focused item's handlers. "A Open, X Menu, Y Ask, B Back".

import { useFocusState, type ItemHandlers } from "@/focus/bridge";
import { useInputMode } from "@/focus/react";
import { t, useT } from "@/i18n";
import { cn } from "@/lib/utils";
import { useNav } from "@/nav/stack";

export function hints(item: ItemHandlers | null, canBack: boolean, canAsk = true): Array<[button: string, label: string]> {
  const out: Array<[string, string]> = [];
  if (item?.onSelect && !item.disabled) out.push(["A", item.hint ?? t("hints.open")]);
  if (item?.onMenu) out.push(["X", t("hints.menu")]);
  if (canAsk) out.push(["Y", t("hints.ask")]);
  if (canBack) out.push(["B", t("hints.back")]);
  return out;
}

/** "A Type, X Delete": each button in label white, what it does in dim. */
export function HintLine({ pairs, className }: { pairs: Array<[button: string, label: string]>; className?: string }) {
  return (
    <p className={cn("pointer-events-none font-display text-caption text-label-dim", className)}>
      {pairs.map(([button, label], i) => (
        <span key={button}>
          {i > 0 && ", "}
          <span className="font-title text-label">{button}</span> {label}
        </span>
      ))}
    </p>
  );
}

export function Hints() {
  useT();
  const mode = useInputMode();
  const item = useFocusState((s) => s.item);
  const canBack = useFocusState((s) => s.canBack);
  // First run has no Ask.
  const canAsk = useNav((s) => s.entries[0]!.route.screen !== "firstRun");
  if (mode !== "pad") return null;
  return <HintLine pairs={hints(item, canBack, canAsk)} className="fixed right-edge bottom-8 z-30" />;
}
