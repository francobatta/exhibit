// The gamepad, translated into the keyboard path: directions become arrow key presses on the
// focused element, A clicks it, the other buttons become the keys the bridge already knows.
// `step` holds all the logic and is pure; the poller around it only reads the pad each frame.

import { useFocusState } from "../bridge";

/** What one frame of a standard-mapping pad shows. */
export interface PadSnapshot {
  buttons: readonly boolean[];
  axes: readonly number[];
}

export type PadDirection = "up" | "down" | "left" | "right";
export type PadAction =
  | PadDirection
  | "select"
  | "back"
  | "menu"
  | "ask"
  | "previous"
  | "next"
  | "settings";

export interface PadState {
  /** Buttons held last frame, by index. */
  held: readonly boolean[];
  /** The direction held, and when it started and next repeats. */
  dir: PadDirection | null;
  since: number;
  next: number;
}

export const IDLE: PadState = { held: [], dir: null, since: 0, next: 0 };

const DEAD_ZONE = 0.5;
const FIRST_REPEAT = 380;
const REPEAT = 110;
const FAST_AFTER = 1200;
const FAST_REPEAT = 60;

const BUTTONS: Partial<Record<number, PadAction>> = {
  0: "select",
  1: "back",
  2: "menu",
  3: "ask",
  4: "previous",
  5: "next",
  9: "settings",
};

function directionOf(pad: PadSnapshot): PadDirection | null {
  const b = pad.buttons;
  if (b[12]) return "up";
  if (b[13]) return "down";
  if (b[14]) return "left";
  if (b[15]) return "right";
  const x = pad.axes[0] ?? 0;
  const y = pad.axes[1] ?? 0;
  if (Math.max(Math.abs(x), Math.abs(y)) <= DEAD_ZONE) return null;
  if (Math.abs(x) > Math.abs(y)) return x > 0 ? "right" : "left";
  return y > 0 ? "down" : "up";
}

/** One frame: the new state and what the owner asked for in it. A missing pad releases everything. */
export function step(
  prev: PadState,
  pad: PadSnapshot | null,
  now: number,
): { state: PadState; actions: PadAction[] } {
  if (pad === null) return { state: IDLE, actions: [] };
  const actions: PadAction[] = [];

  const dir = directionOf(pad);
  let { since, next } = prev;
  if (dir === null) {
    since = 0;
    next = 0;
  } else if (dir !== prev.dir) {
    actions.push(dir);
    since = now;
    next = now + FIRST_REPEAT;
  } else if (now >= next) {
    actions.push(dir);
    next = now + (now - since >= FAST_AFTER ? FAST_REPEAT : REPEAT);
  }

  pad.buttons.forEach((down, i) => {
    const action = BUTTONS[i];
    if (action !== undefined && down && !prev.held[i]) actions.push(action);
  });

  return { state: { held: [...pad.buttons], dir, since, next }, actions };
}

// `select` reaches here only with nothing focused: as Enter, the bridge puts focus back first.
const KEYS: Partial<Record<PadAction, string>> = {
  select: "Enter",
  up: "ArrowUp",
  down: "ArrowDown",
  left: "ArrowLeft",
  right: "ArrowRight",
  back: "Escape",
  menu: "m",
  ask: "/",
  previous: "[",
  next: "]",
};

function perform(action: PadAction, onSettings: () => void) {
  const target = (document.activeElement as HTMLElement | null) ?? document.body;
  if (action === "select" && target !== document.body) {
    target.click();
    return;
  }
  if (action === "settings") return onSettings();
  const key = KEYS[action]!;
  target.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
}

function readPad(): PadSnapshot | null {
  for (const g of navigator.getGamepads()) {
    if (g?.connected) return { buttons: g.buttons.map((b) => b.pressed), axes: g.axes };
  }
  return null;
}

/**
 * Polls in `requestAnimationFrame` only while a pad is connected and the window has focus.
 * Returns the stopper.
 */
export function startGamepad(options: { onSettings: () => void }): () => void {
  let state = IDLE;
  let raf = 0;

  const frame = (now: number) => {
    raf = 0;
    const r = step(state, readPad(), now);
    state = r.state;
    if (r.actions.length > 0) {
      useFocusState.getState().setMode("pad");
      for (const a of r.actions) perform(a, options.onSettings);
    }
    schedule();
  };
  const schedule = () => {
    if (raf === 0 && document.hasFocus() && readPad() !== null) raf = requestAnimationFrame(frame);
  };
  const pause = () => {
    if (raf !== 0) cancelAnimationFrame(raf);
    raf = 0;
    state = IDLE;
  };
  // A button still held from before the window lost focus (the A that started mpv) is not a press.
  const resume = () => {
    const pad = readPad();
    if (pad !== null) state = { ...IDLE, held: [...pad.buttons] };
    schedule();
  };

  window.addEventListener("gamepadconnected", schedule);
  window.addEventListener("focus", resume);
  window.addEventListener("blur", pause);
  schedule();
  return () => {
    pause();
    window.removeEventListener("gamepadconnected", schedule);
    window.removeEventListener("focus", resume);
    window.removeEventListener("blur", pause);
  };
}
