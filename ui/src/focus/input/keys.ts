// The keyboard is the focus system's one path; this file only decides which keys a text field
// keeps for itself and notes when real keys are in use.

import { useFocusState } from "../bridge";

const TEXT_TYPES = new Set(["text", "search", "url", "email", "tel", "password", "number", ""]);

export function isTextField(el: EventTarget | null): el is HTMLInputElement | HTMLTextAreaElement | HTMLElement {
  if (!(el instanceof HTMLElement)) return false;
  if (el instanceof HTMLTextAreaElement) return true;
  if (el instanceof HTMLInputElement) return TEXT_TYPES.has(el.type);
  return el.isContentEditable;
}

/**
 * Whether a key pressed in a text field belongs to the field. Characters, Backspace, Home and End
 * do; Up, Down, Enter, Escape and Tab go to the focus system. Left and Right stay with the field
 * while the caret can move that way, so an empty field, or a caret at the end, lets them through.
 */
export function fieldKeeps(event: KeyboardEvent): boolean {
  const el = event.target;
  if (!isTextField(el)) return false;
  switch (event.key) {
    case "ArrowUp":
    case "ArrowDown":
    case "Enter":
    case "Escape":
    case "Tab":
      return false;
    case "ArrowLeft":
    case "ArrowRight": {
      if (!(el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement)) return true;
      const start = el.selectionStart ?? 0;
      const end = el.selectionEnd ?? start;
      if (start !== end) return true;
      return event.key === "ArrowLeft" ? start > 0 : end < el.value.length;
    }
    default:
      return true;
  }
}

/** Sets input mode `keys` on real key presses; the gamepad's keys are synthetic and do not count. */
export function startKeys(): () => void {
  const onKey = (e: KeyboardEvent) => {
    if (e.isTrusted) useFocusState.getState().setMode("keys");
  };
  window.addEventListener("keydown", onKey, true);
  return () => window.removeEventListener("keydown", onKey, true);
}
