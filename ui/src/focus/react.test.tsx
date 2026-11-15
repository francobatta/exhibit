import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { useState, type ReactNode } from "react";
import { installBridge, useFocusState } from "./bridge";
import { FocusGroup, FocusScope, useFocusedKey, useFocusItem } from "./react";

let uninstall: () => void = () => {};
beforeEach(() => {
  uninstall = installBridge();
});
afterEach(() => {
  cleanup();
  uninstall();
  document.body.innerHTML = "";
});

const tick = (ms = 10) => act(() => new Promise((r) => setTimeout(r, ms)));
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
// Recovery, restore and keepFocus finish a microtask or a task later, so every action waits inside act.
const press = (key: string) =>
  act(async () => {
    (document.activeElement ?? document.body).dispatchEvent(
      new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }),
    );
    await sleep(5);
  });
const click = (id: string) =>
  act(async () => {
    document.getElementById(id)!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await sleep(10);
  });
const active = () => (document.activeElement as HTMLElement | null)?.dataset.focusKey;
const q = (k: string) => document.querySelector(`[data-focus-key="${k}"]`) as HTMLElement;

function place(el: Element | null, left: number, top: number, width: number, height: number) {
  (el as HTMLElement).getBoundingClientRect = () =>
    ({ left, top, right: left + width, bottom: top + height, width, height, x: left, y: top }) as DOMRect;
}

/** Lays out a horizontal group as a row at `top`: the group spans the width, items 100 wide from `left`. */
function row(id: string, top: number, keys: string[], left = 0) {
  place(document.querySelector(`[data-focus-group="${id}"]`), 0, top, 2000, 100);
  keys.forEach((k, i) => place(q(k), left + i * 150, top, 100, 100));
}

function Item({ k, onBack }: { k: string; onBack?: () => void }) {
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({ key: k, onBack });
  return (
    <button ref={ref} {...props} data-lit={focused ? "" : undefined}>
      {k}
    </button>
  );
}

function Row({ id, keys }: { id: string; keys: string[] }) {
  return (
    <FocusGroup id={id} orientation="horizontal">
      {keys.map((k) => (
        <Item key={k} k={k} />
      ))}
    </FocusGroup>
  );
}

test("focused mirrors real focus and moves with the arrows inside a group", async () => {
  render(
    <FocusScope id="page">
      <Row id="r" keys={["a", "b", "c"]} />
    </FocusScope>,
  );
  await tick();
  expect(active()).toBe("a");
  expect(q("a").hasAttribute("data-lit")).toBe(true);
  await press("ArrowRight");
  expect(active()).toBe("b");
  expect(q("a").hasAttribute("data-lit")).toBe(false);
  expect(q("b").hasAttribute("data-lit")).toBe(true);
  expect(q("b").hasAttribute("data-focused")).toBe(true);
  expect(useFocusState.getState().key).toBe("b");
});

function Shrinking({ initial, fallbackKey }: { initial: string[]; fallbackKey?: string }) {
  const [keys, setKeys] = useState(initial);
  return (
    <FocusScope id="page" fallbackKey={fallbackKey}>
      <FocusGroup id="top" orientation="horizontal">
        <Item k="home" />
      </FocusGroup>
      <Row id="r" keys={keys} />
      <button id="remove" tabIndex={-1} onClick={() => setKeys((ks) => ks.filter((k) => k !== active()))} />
    </FocusScope>
  );
}

async function removeFocused() {
  await click("remove");
}

test("removing the focused item moves focus to the item that takes its place", async () => {
  render(<Shrinking initial={["a", "b", "c", "d"]} />);
  await tick();
  await act(() => q("b").focus());
  await removeFocused();
  expect(active()).toBe("c");
});

test("removing the focused last item moves focus to the new last item", async () => {
  render(<Shrinking initial={["a", "b", "c"]} />);
  await tick();
  await act(() => q("c").focus());
  await removeFocused();
  expect(active()).toBe("b");
});

test("emptying the group moves focus to the scope's fallbackKey", async () => {
  render(<Shrinking initial={["only"]} fallbackKey="home" />);
  await tick();
  await act(() => q("only").focus());
  await removeFocused();
  expect(active()).toBe("home");
});

test("a whole group leaving with focus falls back to the scope", async () => {
  function Page() {
    const [shown, setShown] = useState(true);
    return (
      <FocusScope id="page" fallbackKey="home">
        <FocusGroup id="top" orientation="horizontal">
          <Item k="home" />
        </FocusGroup>
        {shown && <Row id="r" keys={["a", "b"]} />}
        <button id="hide" tabIndex={-1} onClick={() => setShown(false)} />
      </FocusScope>
    );
  }
  render(<Page />);
  await tick();
  await act(() => q("b").focus());
  await click("hide");
  await tick();
  expect(active()).toBe("home");
});

test("a screen scope restores restoreKey on mount", async () => {
  render(
    <FocusScope id="page" restoreKey="c">
      <Row id="r" keys={["a", "b", "c"]} />
    </FocusScope>,
  );
  await tick();
  expect(active()).toBe("c");
});

test("a screen scope waits for restoreKey to arrive with its data", async () => {
  function Late() {
    const [keys, setKeys] = useState(["a"]);
    return (
      <FocusScope id="page" restoreKey="late">
        <Row id="r" keys={keys} />
        <button id="load" tabIndex={-1} onClick={() => setKeys(["a", "b", "late"])} />
      </FocusScope>
    );
  }
  render(<Late />);
  await tick();
  await click("load");
  await tick();
  expect(active()).toBe("late");
});

test("a screen scope with no restoreKey focuses its first group's stop", async () => {
  render(
    <FocusScope id="page">
      <Row id="first" keys={["a", "b"]} />
      <Row id="second" keys={["c"]} />
    </FocusScope>,
  );
  await tick();
  expect(active()).toBe("a");
});

test("a scope that becomes active again restores its restoreKey, and reports focus while active", async () => {
  const reported: string[] = [];
  function Stack() {
    const [top, setTop] = useState<"one" | "two">("one");
    return (
      <>
        <div inert={top !== "one"}>
          <FocusScope
            id="one"
            restoreKey="b"
            active={top === "one"}
            onFocusKey={top === "one" ? (k) => reported.push(k) : undefined}
          >
            <Row id="r1" keys={["a", "b"]} />
          </FocusScope>
        </div>
        <div inert={top !== "two"}>
          <FocusScope id="two" active={top === "two"}>
            <Row id="r2" keys={["x"]} />
          </FocusScope>
        </div>
        <button id="swap" tabIndex={-1} onClick={() => setTop((t) => (t === "one" ? "two" : "one"))} />
      </>
    );
  }
  render(<Stack />);
  await tick();
  expect(active()).toBe("b");
  await act(() => q("a").focus());
  expect(reported.at(-1)).toBe("a");
  const swap = () => click("swap");
  await swap();
  await tick();
  expect(active()).toBe("x");
  await swap();
  await tick();
  // The stack would pass the remembered key; here restoreKey stays "b".
  expect(active()).toBe("b");
});

test("leaving a group and returning lands on the remembered item", async () => {
  render(
    <FocusScope id="page">
      <Row id="top" keys={["t1", "t2", "t3"]} />
      <Row id="bottom" keys={["b1", "b2", "b3"]} />
    </FocusScope>,
  );
  await tick();
  row("top", 0, ["t1", "t2", "t3"]);
  row("bottom", 300, ["b1", "b2", "b3"]);
  await press("ArrowRight");
  await press("ArrowRight");
  expect(active()).toBe("t3");
  await press("ArrowDown");
  // Entering a group for the first time picks the item nearest on the cross axis.
  expect(active()).toBe("b3");
  await press("ArrowLeft");
  await press("ArrowLeft");
  expect(active()).toBe("b1");
  await press("ArrowUp");
  // Coming back lands on the remembered item, not the nearest one.
  expect(active()).toBe("t3");
});

test("an arrow at a group's end crosses to the group that way", async () => {
  render(
    <FocusScope id="page">
      <FocusGroup id="list" orientation="vertical">
        <Item k="v1" />
        <Item k="v2" />
      </FocusGroup>
      <Row id="row" keys={["r1"]} />
    </FocusScope>,
  );
  await tick();
  place(document.querySelector('[data-focus-group="list"]'), 0, 0, 100, 200);
  place(q("v1"), 0, 0, 100, 100);
  place(q("v2"), 0, 100, 100, 100);
  row("row", 400, ["r1"]);
  await press("ArrowDown");
  expect(active()).toBe("v2");
  await press("ArrowDown");
  expect(active()).toBe("r1");
});

function Modal({ open, onClose, children }: { open: boolean; onClose: () => void; children: ReactNode }) {
  return open ? (
    <FocusScope id="menu" modal label="Item menu" onBack={onClose}>
      {children}
    </FocusScope>
  ) : null;
}

test("a modal scope traps arrows and returns focus to where it was on close", async () => {
  function Page() {
    const [open, setOpen] = useState(false);
    return (
      <FocusScope id="page">
        <Row id="shelf" keys={["a", "b"]} />
        <Row id="below" keys={["c"]} />
        <button id="open" tabIndex={-1} onClick={() => setOpen(true)} />
        <Modal open={open} onClose={() => setOpen(false)}>
          <Row id="menu-actions" keys={["play", "watched"]} />
        </Modal>
      </FocusScope>
    );
  }
  render(<Page />);
  await tick();
  row("shelf", 0, ["a", "b"]);
  row("below", 600, ["c"]);
  await act(() => q("b").focus());
  await click("open");
  await tick();
  row("menu-actions", 300, ["play", "watched"]);
  expect(active()).toBe("play");
  await press("ArrowRight");
  expect(active()).toBe("watched");
  await press("ArrowUp");
  await press("ArrowDown");
  await press("ArrowRight");
  expect(active()).toBe("watched");
  await press("Escape");
  await tick();
  expect(document.querySelector('[data-focus-key="play"]')).toBeNull();
  expect(active()).toBe("b");
});

test("useFocusedKey follows real focus", async () => {
  let seen: string | null = null;
  function Watch() {
    seen = useFocusedKey();
    return null;
  }
  render(
    <FocusScope id="page">
      <Row id="r" keys={["a", "b"]} />
      <Watch />
    </FocusScope>,
  );
  await tick();
  expect(seen === "a").toBe(true);
  await press("ArrowRight");
  expect(seen === "b").toBe(true);
});
