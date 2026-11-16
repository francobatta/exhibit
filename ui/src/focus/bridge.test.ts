import { afterEach, beforeEach, expect, test } from "bun:test";
import { useNav } from "@/nav/stack";
import {
  crossMove,
  installBridge,
  installShortcuts,
  markEdge,
  registerGroup,
  registerItem,
  registerScope,
  type Box,
  type ItemHandlers,
} from "./bridge";

const box = (left: number, top: number, width: number, height: number): Box => ({
  left,
  top,
  right: left + width,
  bottom: top + height,
});

function place<T extends HTMLElement>(el: T, b: Box): T {
  el.getBoundingClientRect = () =>
    ({ ...b, x: b.left, y: b.top, width: b.right - b.left, height: b.bottom - b.top }) as DOMRect;
  return el;
}

const topBar = box(0, 0, 1920, 96);
const shelf1 = box(0, 200, 1920, 300);
const shelf2 = box(0, 600, 1920, 300);
const poster = box(100, 220, 200, 260);

test("crossMove picks the group below when pressing down from a shelf", () => {
  const groups = [
    { id: "bar", rect: topBar },
    { id: "shelf2", rect: shelf2 },
  ];
  expect(crossMove(groups, poster, [0, 1])).toBe("shelf2");
});

test("crossMove picks the top bar when pressing up from the first shelf", () => {
  const groups = [
    { id: "bar", rect: topBar },
    { id: "shelf2", rect: shelf2 },
  ];
  expect(crossMove(groups, poster, [0, -1])).toBe("bar");
});

test("crossMove reaches a short group off to the side ahead, but never one beside", () => {
  const controls = { id: "controls", rect: box(0, 0, 300, 60), near: true };
  const grid = { id: "grid", rect: box(0, 100, 1900, 800), near: true };
  const farRightCell = box(1700, 120, 150, 220);
  // Up from the far right: no group within 45 degrees, the controls row still lies above.
  expect(crossMove([controls, grid], farRightCell, [0, -1])).toBe("controls");
  // Right at the end of a short shelf: the bar above is not to the right.
  const bar = { id: "bar", rect: box(0, 0, 1900, 60) };
  expect(crossMove([bar], box(300, 600, 150, 220), [1, 0])).toBeUndefined();
});

test("crossMove finds nothing at a true edge", () => {
  const groups = [
    { id: "bar", rect: topBar },
    { id: "shelf1", rect: shelf1 },
  ];
  expect(crossMove(groups, box(100, 620, 200, 260), [0, 1])).toBeUndefined();
  expect(crossMove(groups, box(1700, 220, 200, 260), [1, 0])).toBeUndefined();
});

// A page of plain elements registered with the bridge by hand, so these tests see the bridge alone.

let cleanups: Array<() => void> = [];
let uninstall: () => void = () => {};

function button(key: string, parent: HTMLElement, handlers: Partial<ItemHandlers> = {}) {
  const el = document.createElement("button");
  el.dataset.focusKey = key;
  el.tabIndex = -1;
  parent.append(el);
  cleanups.push(registerItem(el, () => ({ key, ...handlers })));
  return el;
}

function group(id: string, rect: Box, parent: HTMLElement, onEdge?: () => void) {
  const el = place(document.createElement("div"), rect);
  parent.append(el);
  const items: HTMLElement[] = [];
  cleanups.push(
    registerGroup({
      id,
      el,
      enter: () => {
        items[0]?.focus();
        return items.length > 0;
      },
      stop: () => items[0],
      items: () => items,
      onEdge,
    }),
  );
  return { el, items };
}

function scope(id: string, handlers: { onBack?: () => void; modal?: boolean } = {}) {
  const el = document.createElement("section");
  document.body.append(el);
  cleanups.push(registerScope({ id, el, modal: handlers.modal ?? false, onBack: handlers.onBack }));
  return el;
}

function press(key: string, target: Element = document.activeElement ?? document.body, init: KeyboardEventInit = {}) {
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init });
  target.dispatchEvent(event);
  return event;
}

beforeEach(() => {
  uninstall = installBridge();
  useNav.getState().replaceRoot({ screen: "home" });
});

afterEach(() => {
  uninstall();
  for (const c of cleanups.reverse()) c();
  cleanups = [];
  document.body.innerHTML = "";
});

function twoShelves() {
  const page = scope("home");
  const a = group("shelf1", shelf1, page);
  a.items.push(place(button("a1", a.el), poster), place(button("a2", a.el), box(400, 220, 200, 260)));
  const b = group("shelf2", shelf2, page);
  b.items.push(place(button("b1", b.el), box(100, 620, 200, 260)));
  return { a, b };
}

test("an arrow no group consumed moves to the group that way", () => {
  const { a, b } = twoShelves();
  a.items[0]!.focus();
  const event = press("ArrowDown");
  expect(document.activeElement).toBe(b.items[0]!);
  expect(event.defaultPrevented).toBe(true);
});

test("an arrow a group consumed is not handled again", () => {
  const { a } = twoShelves();
  a.items[0]!.addEventListener("keydown", (e) => {
    e.preventDefault();
    a.items[1]!.focus();
  });
  a.items[0]!.focus();
  press("ArrowDown");
  expect(document.activeElement).toBe(a.items[1]!);
});

test("an arrow a group consumed only to stay at its end still crosses", () => {
  const { a, b } = twoShelves();
  a.items[0]!.addEventListener("keydown", (e) => {
    e.preventDefault();
    markEdge(e);
  });
  a.items[0]!.focus();
  press("ArrowDown");
  expect(document.activeElement).toBe(b.items[0]!);
});

test("nothing that way calls the group's onEdge and leaves focus", () => {
  let edges = 0;
  const page = scope("home");
  const only = group("only", shelf1, page, () => edges++);
  only.items.push(place(button("x", only.el), poster));
  only.items[0]!.focus();
  press("ArrowDown");
  expect(edges).toBe(1);
  expect(document.activeElement).toBe(only.items[0]!);
});

test("Escape goes to the focused item's onBack first", () => {
  const calls: string[] = [];
  const page = scope("home", { onBack: () => calls.push("scope") });
  const g = group("g", shelf1, page);
  g.items.push(button("x", g.el, { onBack: () => calls.push("item") }));
  g.items[0]!.focus();
  expect(press("Escape").defaultPrevented).toBe(true);
  expect(calls).toEqual(["item"]);
});

test("Escape goes to the scope's onBack when the item has none", () => {
  const calls: string[] = [];
  const page = scope("home", { onBack: () => calls.push("scope") });
  const g = group("g", shelf1, page);
  g.items.push(button("x", g.el));
  g.items[0]!.focus();
  press("Escape");
  expect(calls).toEqual(["scope"]);
});

test("Escape and Backspace go back in the navigation stack when nothing else takes them", () => {
  useNav.getState().push({ screen: "settings" });
  useNav.getState().push({ screen: "review" });
  const page = scope("settings");
  const g = group("g", shelf1, page);
  g.items.push(button("x", g.el));
  g.items[0]!.focus();
  press("Escape");
  expect(useNav.getState().current).toEqual({ screen: "settings" });
  press("Backspace");
  expect(useNav.getState().current).toEqual({ screen: "home" });
});

test("Escape that something already handled is left alone", () => {
  useNav.getState().push({ screen: "settings" });
  const page = scope("settings");
  const g = group("g", shelf1, page);
  g.items.push(button("x", g.el));
  g.items[0]!.addEventListener("keydown", (e) => e.preventDefault());
  g.items[0]!.focus();
  press("Escape");
  expect(useNav.getState().current).toEqual({ screen: "settings" });
});

test("Backspace inside a text field stays with the field", () => {
  useNav.getState().push({ screen: "settings" });
  const input = document.createElement("input");
  input.value = "abc";
  document.body.append(input);
  input.focus();
  expect(press("Backspace").defaultPrevented).toBe(false);
  expect(useNav.getState().current).toEqual({ screen: "settings" });
});

test("a modal scope stops Back at itself", () => {
  useNav.getState().push({ screen: "settings" });
  let closed = 0;
  const page = scope("settings");
  const g = group("g", shelf1, page);
  g.items.push(button("x", g.el));
  const menu = scope("menu", { modal: true, onBack: () => closed++ });
  const m = group("m", shelf2, menu);
  m.items.push(button("m1", m.el));
  m.items[0]!.focus();
  press("Escape");
  expect(closed).toBe(1);
  expect(useNav.getState().current).toEqual({ screen: "settings" });
});

test("while a modal scope is open, arrows only reach groups inside it", () => {
  const page = scope("page");
  const outside = group("outside", shelf2, page);
  outside.items.push(place(button("o1", outside.el), box(100, 620, 200, 260)));
  const menu = scope("menu", { modal: true });
  const m = group("m", shelf1, menu);
  m.items.push(place(button("m1", m.el), poster));
  m.items[0]!.focus();
  press("ArrowDown");
  expect(document.activeElement).toBe(m.items[0]!);
});

test("Tab does nothing", () => {
  const { a } = twoShelves();
  a.items[0]!.focus();
  expect(press("Tab").defaultPrevented).toBe(true);
  expect(press("Tab", undefined, { shiftKey: true }).defaultPrevented).toBe(true);
  expect(document.activeElement).toBe(a.items[0]!);
});

test("/ opens ask from inside any group, but types a slash inside a text field", () => {
  let asks = 0;
  cleanups.push(installShortcuts({ onAsk: () => asks++, onSection: () => {} }));
  const { a } = twoShelves();
  a.items[1]!.focus();
  press("/");
  expect(asks).toBe(1);

  const input = document.createElement("input");
  document.body.append(input);
  input.focus();
  expect(press("/").defaultPrevented).toBe(false);
  expect(asks).toBe(1);

  press("k", input, { ctrlKey: true });
  expect(asks).toBe(2);
});

test("[ and ] ask for the previous and next section; m opens the focused item's menu", () => {
  const sections: number[] = [];
  let menus = 0;
  cleanups.push(installShortcuts({ onAsk: () => {}, onSection: (d) => sections.push(d) }));
  const page = scope("home");
  const g = group("g", shelf1, page);
  g.items.push(button("x", g.el, { onMenu: () => menus++ }));
  g.items[0]!.focus();
  press("[");
  press("]");
  press("m");
  expect(sections).toEqual([-1, 1]);
  expect(menus).toBe(1);
});

test("a key on nothing focused enters the first group", () => {
  const { a } = twoShelves();
  (document.activeElement as HTMLElement | null)?.blur();
  press("ArrowDown", document.body);
  expect(document.activeElement).toBe(a.items[0]!);
});
