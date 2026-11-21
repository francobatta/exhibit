import { expect, test } from "bun:test";
import { IDLE, step, type PadAction, type PadSnapshot, type PadState } from "./gamepad";

/** A standard-mapping pad with the given buttons held and the left stick at `axes`. */
function pad(held: number[] = [], axes: [number, number] = [0, 0]): PadSnapshot {
  return { buttons: Array.from({ length: 17 }, (_, i) => held.includes(i)), axes: [...axes, 0, 0] };
}

/** Feeds snapshots at the given times; returns the actions emitted at each time that emitted any. */
function run(frames: Array<[number, PadSnapshot | null]>, from: PadState = IDLE) {
  let state = from;
  const out: Array<[number, PadAction[]]> = [];
  for (const [now, snap] of frames) {
    const r = step(state, snap, now);
    state = r.state;
    if (r.actions.length > 0) out.push([now, r.actions]);
  }
  return { out, state };
}

const every = (from: number, to: number, by: number, snap: PadSnapshot | null) =>
  Array.from({ length: Math.floor((to - from) / by) + 1 }, (_, i) => [from + i * by, snap] as [number, PadSnapshot | null]);

test("a d-pad press emits one move", () => {
  expect(run([[0, pad([13])]]).out).toEqual([[0, ["down"]]]);
  expect(run([[0, pad([12])]]).out).toEqual([[0, ["up"]]]);
  expect(run([[0, pad([14])]]).out).toEqual([[0, ["left"]]]);
  expect(run([[0, pad([15])]]).out).toEqual([[0, ["right"]]]);
});

test("a held direction is quiet for 380 ms, then repeats every 110 ms, then every 60 ms after 1.2 s", () => {
  const { out } = run(every(0, 1500, 10, pad([15])));
  expect(out.map(([t]) => t)).toEqual([
    0, 380, 490, 600, 710, 820, 930, 1040, 1150, 1260, 1320, 1380, 1440, 1500,
  ]);
  expect(out.every(([, a]) => a.length === 1 && a[0] === "right")).toBe(true);
});

test("nothing repeats while held under 380 ms", () => {
  const { out } = run(every(0, 370, 10, pad([13])));
  expect(out).toEqual([[0, ["down"]]]);
});

test("releasing and pressing again moves at once", () => {
  const { out } = run([
    [0, pad([13])],
    [100, pad()],
    [150, pad([13])],
  ]);
  expect(out).toEqual([
    [0, ["down"]],
    [150, ["down"]],
  ]);
});

test("changing direction while held moves at once in the new direction", () => {
  const { out } = run([
    [0, pad([13])],
    [100, pad([15])],
  ]);
  expect(out).toEqual([
    [0, ["down"]],
    [100, ["right"]],
  ]);
});

test("the left stick moves past a 0.5 dead zone, on its stronger axis", () => {
  expect(run([[0, pad([], [0.3, 0.45])]]).out).toEqual([]);
  expect(run([[0, pad([], [0, 0.7])]]).out).toEqual([[0, ["down"]]]);
  expect(run([[0, pad([], [-0.9, 0.6])]]).out).toEqual([[0, ["left"]]]);
  expect(run([[0, pad([], [0.2, -0.8])]]).out).toEqual([[0, ["up"]]]);
});

test("A emits select once per press", () => {
  const { out } = run([
    [0, pad([0])],
    [16, pad([0])],
    [500, pad([0])],
    [516, pad()],
    [532, pad([0])],
  ]);
  expect(out).toEqual([
    [0, ["select"]],
    [532, ["select"]],
  ]);
});

test("the face, shoulder and menu buttons map to their actions", () => {
  expect(run([[0, pad([1])]]).out).toEqual([[0, ["back"]]]);
  expect(run([[0, pad([2])]]).out).toEqual([[0, ["menu"]]]);
  expect(run([[0, pad([3])]]).out).toEqual([[0, ["ask"]]]);
  expect(run([[0, pad([4])]]).out).toEqual([[0, ["previous"]]]);
  expect(run([[0, pad([5])]]).out).toEqual([[0, ["next"]]]);
  expect(run([[0, pad([9])]]).out).toEqual([[0, ["settings"]]]);
  expect(run([[0, pad([8])]]).out).toEqual([[0, ["views"]]]);
  expect(run([[0, pad([6, 7, 10, 11, 16])]]).out).toEqual([]);
});

test("a pad that goes away and comes back keeps working", () => {
  const { out, state } = run([
    [0, pad([13])],
    [50, null],
    [60, null],
    [70, pad([13])],
    [80, pad([0])],
  ]);
  expect(out).toEqual([
    [0, ["down"]],
    [70, ["down"]],
    [80, ["select"]],
  ]);
  // Unplugged mid-hold, no repeat fires while it is gone.
  expect(run(every(100, 1000, 10, null), state).out).toEqual([]);
});

test("a button held while the pad returns counts as a new press", () => {
  const { out } = run([
    [0, pad([0])],
    [20, null],
    [40, pad([0])],
  ]);
  expect(out).toEqual([
    [0, ["select"]],
    [40, ["select"]],
  ]);
});
