import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { useState } from "react";
import { installBridge, useFocusState } from "../bridge";
import { FocusGroup, FocusScope, useFocusItem } from "../react";
import { fieldKeeps } from "./keys";

function field(value: string, caret: number) {
  const input = document.createElement("input");
  input.value = value;
  document.body.append(input);
  input.focus();
  input.setSelectionRange(caret, caret);
  return input;
}

/** A keydown whose target is `target`, without dispatching it. */
function keydown(target: Element, key: string): KeyboardEvent {
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
  Object.defineProperty(event, "target", { value: target });
  return event;
}

function keeps(input: HTMLInputElement, key: string) {
  return fieldKeeps(keydown(input, key));
}

afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
});

test("inside a text field, characters, Backspace and in-text arrows stay with the field", () => {
  const input = field("hello", 2);
  expect(keeps(input, "ArrowLeft")).toBe(true);
  expect(keeps(input, "ArrowRight")).toBe(true);
  expect(keeps(input, "Backspace")).toBe(true);
  expect(keeps(input, "a")).toBe(true);
  expect(keeps(input, "/")).toBe(true);
  expect(keeps(input, "Home")).toBe(true);
});

test("inside a text field, up, down, Enter and Escape are left to the focus system", () => {
  const input = field("hello", 2);
  expect(keeps(input, "ArrowUp")).toBe(false);
  expect(keeps(input, "ArrowDown")).toBe(false);
  expect(keeps(input, "Enter")).toBe(false);
  expect(keeps(input, "Escape")).toBe(false);
});

test("an arrow at the caret's end of the text leaves the field", () => {
  expect(keeps(field("hello", 0), "ArrowLeft")).toBe(false);
  expect(keeps(field("hello", 5), "ArrowRight")).toBe(false);
  expect(keeps(field("", 0), "ArrowRight")).toBe(false);
});

test("keys outside a text field are not the field's", () => {
  const b = document.createElement("button");
  document.body.append(b);
  expect(fieldKeeps(keydown(b, "a"))).toBe(false);
});

// The same in a running focus system: a field as an item of a horizontal group.

function Item({ k, children }: { k: string; children?: (p: object) => React.ReactNode }) {
  const { ref, props } = useFocusItem({ key: k });
  return children ? children({ ref, ...props }) : <button ref={ref} {...props}>{k}</button>;
}

function Bar() {
  const [text, setText] = useState("hello");
  return (
    <FocusScope id="page">
      <FocusGroup id="bar" orientation="horizontal">
        <Item k="before" />
        <Item k="field">
          {(p) => <input {...p} value={text} onChange={(e) => setText(e.target.value)} />}
        </Item>
        <Item k="after" />
      </FocusGroup>
      <FocusGroup id="below" orientation="horizontal">
        <Item k="below" />
      </FocusGroup>
    </FocusScope>
  );
}

function place(el: Element, left: number, top: number, width: number, height: number) {
  (el as HTMLElement).getBoundingClientRect = () =>
    ({ left, top, right: left + width, bottom: top + height, width, height, x: left, y: top }) as DOMRect;
}

let uninstall: () => void = () => {};
beforeEach(() => {
  uninstall = installBridge();
});
afterEach(() => uninstall());

const tick = () => act(() => new Promise((r) => setTimeout(r, 10)));
const press = (key: string) =>
  act(() => {
    (document.activeElement ?? document.body).dispatchEvent(
      new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }),
    );
  });

test("left and right inside a focused text field do not move focus; up and down do", async () => {
  const r = render(<Bar />);
  await tick();
  const q = (k: string) => r.container.querySelector(`[data-focus-key="${k}"]`) as HTMLElement;
  place(r.container.querySelector('[data-focus-group="bar"]')!, 0, 0, 1000, 50);
  place(r.container.querySelector('[data-focus-group="below"]')!, 0, 200, 1000, 50);
  place(q("before"), 0, 0, 100, 50);
  place(q("field"), 200, 0, 300, 50);
  place(q("after"), 600, 0, 100, 50);
  place(q("below"), 250, 200, 100, 50);

  const input = q("field") as HTMLInputElement;
  await act(() => input.focus());
  input.setSelectionRange(2, 2);
  await press("ArrowLeft");
  expect(document.activeElement).toBe(input);
  await press("ArrowRight");
  expect(document.activeElement).toBe(input);
  await press("ArrowDown");
  expect(document.activeElement).toBe(q("below"));
  await press("ArrowUp");
  expect(document.activeElement).toBe(input);
  expect(useFocusState.getState().key).toBe("field");
});
