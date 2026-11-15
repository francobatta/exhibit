// Bottom right, only while the owner drives with a gamepad: what the buttons do here, from the
// focused item's handlers. "A Open, X Menu, Y Ask, B Back".

import { useFocusState, type ItemHandlers } from "@/focus/bridge";
import { useInputMode } from "@/focus/react";

export function hints(item: ItemHandlers | null, canBack: boolean): Array<[button: string, label: string]> {
  const out: Array<[string, string]> = [];
  if (item?.onSelect && !item.disabled) out.push(["A", "Open"]);
  if (item?.onMenu) out.push(["X", "Menu"]);
  out.push(["Y", "Ask"]);
  if (canBack) out.push(["B", "Back"]);
  return out;
}

export function Hints() {
  const mode = useInputMode();
  const item = useFocusState((s) => s.item);
  const canBack = useFocusState((s) => s.canBack);
  if (mode !== "pad") return null;
  return (
    <p className="pointer-events-none fixed right-edge bottom-8 z-30 font-display text-caption text-label-dim">
      {hints(item, canBack).map(([button, label], i) => (
        <span key={button}>
          {i > 0 && ", "}
          <span className="font-title text-label">{button}</span> {label}
        </span>
      ))}
    </p>
  );
}
