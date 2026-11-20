import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, mock, test } from "bun:test";
import { installBridge } from "@/focus/bridge";
import { FocusGroup, FocusScope, useFocusItem } from "@/focus/react";
import { RangeField, extent } from "./RangeField";

let uninstall: () => void = () => {};
beforeEach(() => {
  uninstall = installBridge();
});
afterEach(() => {
  cleanup();
  uninstall();
  document.body.innerHTML = "";
});

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const press = (key: string) =>
  act(async () => {
    (document.activeElement ?? document.body).dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
    await sleep(20);
  });
const focused = () => (document.activeElement as HTMLElement | null)?.dataset.focusKey;

function Below() {
  const { ref, props } = useFocusItem<HTMLButtonElement>({ key: "below" });
  return <button ref={ref} {...props} />;
}

function place(el: Element, top: number) {
  (el as HTMLElement).getBoundingClientRect = () =>
    ({ left: 0, top, right: 400, bottom: top + 40, width: 400, height: 40, x: 0, y: top }) as DOMRect;
}

const counts = [1960, 1975, 1990].map((value) => ({ value, count: 1 }));

function field(min: number | null, max: number | null, onChange: (a: number | null, b: number | null) => void) {
  return render(
    <FocusScope id="s">
      <RangeField id="year" label="Year" counts={counts} step={1} leap={{ by: 10, label: "10 years" }} min={min} max={max} onChange={onChange} name={String} />
      <FocusGroup id="below" orientation="horizontal">
        <Below />
      </FocusGroup>
    </FocusScope>,
  );
}

test("the extent runs from the first group of values to the last", () => {
  expect(extent(counts)).toEqual([1960, 1990]);
  expect(extent([{ value: 94, count: 1 }, { value: 170, count: 2 }], 10)).toEqual([90, 170]);
  expect(extent([{ value: 2001, count: 3 }])).toBeNull();
});

test("Left and Right move the focused end, a leap moves ten, and an end at its limit is open", async () => {
  const onChange = mock((_a: number | null, _b: number | null) => {});
  field(null, 1980, onChange);
  for (const el of document.querySelectorAll("[data-focus-group='year'], [data-focus-key^='year:']")) place(el, 100);
  for (const el of document.querySelectorAll("[data-focus-group='below'], [data-focus-key='below']")) place(el, 200);
  await act(async () => {
    document.querySelector<HTMLElement>("[data-focus-key='year:0']")!.focus();
  });
  await press("ArrowRight");
  expect(onChange).toHaveBeenLastCalledWith(1961, 1980);
  await press("]");
  expect(onChange).toHaveBeenLastCalledWith(1970, 1980);
  await press("ArrowLeft");
  expect(onChange).toHaveBeenLastCalledWith(null, 1980);
  // Down reaches the other end, which stops at the first; Down again leaves the field.
  await press("ArrowDown");
  expect(focused()).toBe("year:1");
  await press("[");
  expect(onChange).toHaveBeenLastCalledWith(null, 1970);
  await press("ArrowDown");
  expect(focused()).toBe("below");
});
