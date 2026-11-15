import { afterEach, beforeEach, expect, test } from "bun:test";
import { registerItem, useFocusState } from "../bridge";
import { startPointer } from "./pointer";

let stop: () => void = () => {};
let unregister: Array<() => void> = [];

function item(key: string, onMenu?: () => void): HTMLButtonElement {
  const el = document.createElement("button");
  el.dataset.focusKey = key;
  el.tabIndex = -1;
  document.body.append(el);
  unregister.push(registerItem(el, () => ({ key, onMenu })));
  return el;
}

const pointer = (type: string, target: Element, x: number, y: number) =>
  target.dispatchEvent(
    new PointerEvent(type, { clientX: x, clientY: y, bubbles: true, pointerType: "mouse" }),
  );

beforeEach(() => {
  stop = startPointer();
});

afterEach(() => {
  stop();
  for (const u of unregister) u();
  unregister = [];
  document.body.innerHTML = "";
  useFocusState.getState().setMode("keys");
});

test("content scrolling under a still pointer does not take focus; a real move does", () => {
  const before = item("before");
  const under = item("under");
  before.focus();

  // The pointer rests at (40, 40) over `before`'s old place.
  pointer("pointermove", document.body, 40, 40);
  // The page scrolls; `under` slides beneath the pointer, which has not moved.
  pointer("pointerover", under, 40, 40);
  pointer("pointerenter", under, 40, 40);
  pointer("pointermove", under, 40, 40);
  expect(document.activeElement).toBe(before);

  pointer("pointermove", under, 42, 41);
  expect(document.activeElement).toBe(under);
  expect(useFocusState.getState().mode).toBe("pointer");
});

test("the first pointer event after start only records where the pointer rests", () => {
  const before = item("before");
  const under = item("under");
  before.focus();
  // Nothing has moved since launch; content slides under the resting cursor.
  pointer("pointerover", under, 40, 40);
  expect(document.activeElement).toBe(before);
  pointer("pointermove", under, 40, 40);
  expect(document.activeElement).toBe(before);
  pointer("pointermove", under, 41, 40);
  expect(document.activeElement).toBe(under);
});

test("right-click inside a text field keeps the native menu", () => {
  let menus = 0;
  const field = document.createElement("input");
  document.body.append(field);
  unregister.push(registerItem(field, () => ({ key: "ask", onMenu: () => menus++ })));
  field.focus();
  const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true, button: 2 });
  field.dispatchEvent(event);
  expect(event.defaultPrevented).toBe(false);
  expect(menus).toBe(0);
});

test("moving over something that is not a focus item leaves focus where it is", () => {
  const a = item("a");
  a.focus();
  const plain = document.createElement("div");
  document.body.append(plain);
  pointer("pointermove", plain, 5, 5);
  pointer("pointermove", plain, 9, 9);
  expect(document.activeElement).toBe(a);
});

test("right-click focuses the item and opens its menu", () => {
  let menus = 0;
  const a = item("a");
  const b = item("b", () => menus++);
  a.focus();
  const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true, button: 2 });
  b.dispatchEvent(event);
  expect(event.defaultPrevented).toBe(true);
  expect(document.activeElement).toBe(b);
  expect(menus).toBe(1);
});

test("the cursor hides at once on pad input and comes back on a real move", () => {
  const root = document.documentElement;
  pointer("pointermove", document.body, 1, 1);
  expect(root.hasAttribute("data-cursor-hidden")).toBe(false);
  useFocusState.getState().setMode("pad");
  expect(root.hasAttribute("data-cursor-hidden")).toBe(true);
  pointer("pointermove", document.body, 2, 2);
  expect(root.hasAttribute("data-cursor-hidden")).toBe(false);
});
