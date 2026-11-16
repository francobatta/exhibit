// The mouse wheel, translated into arrow keys, for screens that move by steps rather than scroll
// (Home's shelves, and the Wall and Show screens next). An element opts in with the attribute
// `data-wheel-steps`; a wheel gesture over it becomes arrow keys sent to the focused element,
// exactly as the gamepad sends them, so the same focus path (groups, the bridge) does the moving.
//
// - Vertical wheel: one ArrowDown or ArrowUp per gesture. A burst of events (a notched wheel spun,
//   a trackpad's inertia) is one gesture, and a short lockout after a step keeps one flick from
//   skipping several rows.
// - Horizontal wheel, or Shift with the vertical wheel: ArrowRight or ArrowLeft, one at once and
//   then one per stretch of travel, a few at most per gesture.
// Elsewhere the wheel keeps its native scrolling.

/** A pause this long ends a gesture; a trackpad's inertia sends events faster than this. */
const GESTURE_GAP_MS = 140;
/** After a vertical step, wheel that starts a new gesture is ignored for this long. */
const LOCKOUT_MS = 320;
/** Travel (px) for each further horizontal step within one gesture, and the most per gesture. */
const STRIDE_X = 120;
const MAX_STEPS_X = 4;
/** Travel smaller than this (px) is noise and starts nothing. */
const DEAD_ZONE = 4;
/** `deltaMode` lines and pages, in px. */
const LINE_PX = 40;
const PAGE_PX = 800;

type Axis = "x" | "y";
export type WheelKey = "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight";

export interface WheelState {
  axis: Axis | null;
  last: number;
  /** Horizontal travel since the last step. */
  travel: number;
  steps: number;
  /** Whether this gesture may step at all (false when it began during the lockout). */
  live: boolean;
  lockedUntil: number;
}

export const initialWheel = (): WheelState => ({ axis: null, last: -Infinity, travel: 0, steps: 0, live: false, lockedUntil: -Infinity });

/** One wheel event, in px, against the gesture so far: the keys to send and the next state. Pure. */
export function wheelStep(
  state: WheelState,
  event: { dx: number; dy: number; shift: boolean; at: number },
): { keys: WheelKey[]; state: WheelState } {
  let { dx, dy } = event;
  if (event.shift && dx === 0) [dx, dy] = [dy, 0];
  const axis: Axis = Math.abs(dy) >= Math.abs(dx) ? "y" : "x";
  const fresh = event.at - state.last > GESTURE_GAP_MS || axis !== state.axis;
  const s: WheelState = fresh
    ? { ...state, axis, travel: 0, steps: 0, live: event.at >= state.lockedUntil }
    : { ...state };
  s.last = event.at;
  const keys: WheelKey[] = [];
  if (!s.live) return { keys, state: s };

  if (axis === "y") {
    if (s.steps === 0 && Math.abs(dy) >= DEAD_ZONE) {
      keys.push(dy > 0 ? "ArrowDown" : "ArrowUp");
      s.steps = 1;
      s.lockedUntil = event.at + LOCKOUT_MS;
    }
    return { keys, state: s };
  }

  const key: WheelKey = dx > 0 ? "ArrowRight" : "ArrowLeft";
  if (s.steps === 0) {
    if (Math.abs(dx) < DEAD_ZONE) return { keys, state: s };
    keys.push(key);
    s.steps = 1;
    s.travel = 0;
    return { keys, state: s };
  }
  s.travel += dx;
  while (Math.abs(s.travel) >= STRIDE_X && s.steps < MAX_STEPS_X) {
    keys.push(s.travel > 0 ? "ArrowRight" : "ArrowLeft");
    s.travel -= Math.sign(s.travel) * STRIDE_X;
    s.steps += 1;
  }
  return { keys, state: s };
}

const toPx = (delta: number, mode: number) => delta * (mode === 1 ? LINE_PX : mode === 2 ? PAGE_PX : 1);

export function startWheel(): () => void {
  let state = initialWheel();

  const onWheel = (e: WheelEvent) => {
    if (!(e.target instanceof Element) || e.target.closest("[data-wheel-steps]") === null) return;
    e.preventDefault();
    const result = wheelStep(state, {
      dx: toPx(e.deltaX, e.deltaMode),
      dy: toPx(e.deltaY, e.deltaMode),
      shift: e.shiftKey,
      at: e.timeStamp,
    });
    state = result.state;
    for (const key of result.keys) {
      const target = (document.activeElement as HTMLElement | null) ?? document.body;
      target.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
    }
  };

  document.addEventListener("wheel", onWheel, { passive: false });
  return () => document.removeEventListener("wheel", onWheel);
}
