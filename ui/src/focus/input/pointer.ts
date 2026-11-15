// The mouse, translated into focus: a pointer that really moves over a focus item focuses it,
// so hover and focus are one light. Content scrolling under a still pointer makes the browser
// send over/move events at unchanged coordinates; those are ignored, so a gamepad driving a
// grid under a resting pointer keeps its focus. Right-click opens the item's menu.

import { createPointerKind } from "@ui-sdk/core";
import { itemAt, openMenu, useFocusState } from "../bridge";
import { isTextField } from "./keys";

const HIDE_AFTER = 3000;

export function startPointer(): () => void {
  const kind = createPointerKind();
  const root = document.documentElement;
  let last: { x: number; y: number } | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const hide = (hidden: boolean) => root.toggleAttribute("data-cursor-hidden", hidden);

  const onMove = (e: PointerEvent) => {
    if (e.pointerType === "touch") return;
    if (last !== null && last.x === e.clientX && last.y === e.clientY) return;
    const first = last === null;
    last = { x: e.clientX, y: e.clientY };
    // The first event may come from content moving under a cursor that rests where it was left.
    if (first) return;
    useFocusState.getState().setMode("pointer");
    hide(false);
    clearTimeout(timer);
    timer = setTimeout(() => hide(true), HIDE_AFTER);

    if (kind.get().hover === "none") return;
    const item = itemAt(e.target);
    if (item !== undefined && item !== document.activeElement) item.focus({ preventScroll: true });
  };

  // The ContextMenu key also sends this event, at the focused element.
  const onContextMenu = (e: MouseEvent) => {
    // A text field keeps its own menu, with Paste.
    if (isTextField(e.target)) return;
    e.preventDefault();
    const item = itemAt(e.target);
    if (item !== undefined && item !== document.activeElement) item.focus({ preventScroll: true });
    openMenu();
  };

  const offMode = useFocusState.subscribe((s, prev) => {
    if (s.mode !== prev.mode && s.mode !== "pointer") {
      clearTimeout(timer);
      hide(true);
    }
  });

  document.addEventListener("pointermove", onMove);
  document.addEventListener("pointerover", onMove);
  document.addEventListener("contextmenu", onContextMenu);
  return () => {
    clearTimeout(timer);
    offMode();
    hide(false);
    document.removeEventListener("pointermove", onMove);
    document.removeEventListener("pointerover", onMove);
    document.removeEventListener("contextmenu", onContextMenu);
  };
}
