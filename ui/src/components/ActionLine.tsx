// A placard's last line: what A does on the work, as text with the key that does it. Not
// focusable. A disabled action says why instead ("Play, the drive for D:\Movies is not connected").

import { useInputMode } from "@/focus/react";
import { OFFLINE, type Action } from "@/lib/actions";
import { facts } from "@/lib/format";

/** Why the main action cannot run, as the second half of the line. */
function whyNot(reason: string, drive: string | undefined): string {
  if (reason === OFFLINE && drive !== undefined) return `the drive for ${drive} is not connected`;
  return reason.charAt(0).toLowerCase() + reason.slice(1);
}

/** The key that names the button for the input in use; none for the pointer. */
export function Keycap({ children }: { children: string }) {
  return (
    <kbd className="inline-flex h-(--keycap) min-w-(--keycap) items-center justify-center rounded-work px-2 font-display text-caption font-title text-label-dim ring-1 ring-line ring-inset">
      {children}
    </kbd>
  );
}

export function ActionLine({ action, drive }: { action: Pick<Action, "label" | "disabled">; drive: string | undefined }) {
  const mode = useInputMode();
  if (action.disabled) {
    return (
      <p className="flex h-(--keycap) items-center font-display text-lead text-label-dim">
        {facts([action.label, whyNot(action.disabled, drive)])}
      </p>
    );
  }
  const key = mode === "pad" ? "A" : mode === "keys" ? "Enter" : null;
  return (
    <p className="flex h-(--keycap) items-center gap-3 font-display text-lead font-ui text-label">
      {key !== null && <Keycap>{key}</Keycap>}
      {action.label}
    </p>
  );
}
