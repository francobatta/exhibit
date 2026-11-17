// A placard's last line: what A does on the work, as text with the key that does it. Not
// focusable. A disabled action says why instead ("Play, the drive for D:\Movies is not connected").

import { useInputMode } from "@/focus/react";
import { reasonOf, type Action } from "@/lib/actions";
import { facts } from "@/lib/format";
import { cn } from "@/lib/utils";

/** Why the main action cannot run, as the second half of the line. */
function whyNot(action: Pick<Action, "disabled">, drive: string | undefined): string {
  const why = reasonOf(action, drive) ?? "";
  return why.charAt(0).toLowerCase() + why.slice(1);
}

/** The key that names the button for the input in use; none for the pointer. */
export function Keycap({ children }: { children: string }) {
  return (
    <kbd className="inline-flex h-(--keycap) min-w-(--keycap) items-center justify-center rounded-work px-2 font-display text-caption font-title text-label-dim ring-1 ring-line ring-inset">
      {children}
    </kbd>
  );
}

export interface ActionLineProps {
  action: Pick<Action, "label" | "disabled">;
  drive: string | undefined;
  /** Whether A would run it now. A line under a work that does not have focus is quiet and names no key. Default true. */
  active?: boolean;
}

export function ActionLine({ action, drive, active = true }: ActionLineProps) {
  const mode = useInputMode();
  if (action.disabled) {
    return (
      <p className="flex h-(--keycap) items-center font-display text-lead text-label-dim">
        {facts([action.label, whyNot(action, drive)])}
      </p>
    );
  }
  const key = !active ? null : mode === "pad" ? "A" : mode === "keys" ? "Enter" : null;
  return (
    <p className={cn("flex h-(--keycap) items-center gap-3 font-display text-lead font-ui", active ? "text-label" : "text-label-dim")}>
      {key !== null && <Keycap>{key}</Keycap>}
      {action.label}
    </p>
  );
}
