import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";
import { films } from "../../test/fixtures";
import { installBridge } from "@/focus/bridge";
import { FocusGroup, FocusScope, useFocusItem } from "@/focus/react";
import type { ItemSummary } from "@/lib/api";
import { WallGrid, cellOf, indexOf, wallColumns } from "./WallGrid";

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
const tick = (ms = 20) => act(() => sleep(ms));
const press = (key: string) =>
  act(async () => {
    (document.activeElement ?? document.body).dispatchEvent(
      new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }),
    );
    await sleep(20);
  });
const active = () => (document.activeElement as HTMLElement | null)?.dataset.focusKey;

function place(el: Element | null, left: number, top: number, width: number, height: number) {
  if (el === null) return;
  (el as HTMLElement).getBoundingClientRect = () =>
    ({ left, top, right: left + width, bottom: top + height, width, height, x: left, y: top }) as DOMRect;
}

describe("cell math", () => {
  test("index to cell and back", () => {
    expect(cellOf(0, 7)).toEqual({ row: 0, col: 0 });
    expect(cellOf(9, 7)).toEqual({ row: 1, col: 2 });
    expect(indexOf({ row: 1, col: 2 }, 7)).toBe(9);
    for (let i = 0; i < 40; i++) expect(indexOf(cellOf(i, 9), 9)).toBe(i);
  });
  test("columns by window width", () => {
    expect(wallColumns(1280)).toBe(7);
    expect(wallColumns(1920)).toBe(7);
    expect(wallColumns(2560)).toBe(9);
    expect(wallColumns(3840)).toBe(11);
  });
});

function Side({ k }: { k: string }) {
  const { ref, props } = useFocusItem<HTMLButtonElement>({ key: k });
  return (
    <button ref={ref} {...props}>
      {k}
    </button>
  );
}

function Page({
  items,
  onOpen = () => {},
  onFocusItem = () => {},
}: {
  items: ItemSummary[];
  onOpen?: (i: ItemSummary) => void;
  onFocusItem?: (i: ItemSummary) => void;
}) {
  return (
    <FocusScope id="page">
      <FocusGroup id="above" orientation="horizontal">
        <Side k="above" />
      </FocusGroup>
      <WallGrid id="wall" items={items} columns={7} shape="poster" onOpen={onOpen} onFocusItem={onFocusItem} />
      <FocusGroup id="right" orientation="vertical">
        <Side k="right" />
      </FocusGroup>
    </FocusScope>
  );
}

// The grid at x 0..1400, y 100..; cells 200 apart; "above" over it, "right" beside it.
function layout() {
  place(document.querySelector('[data-focus-group="above"]'), 0, 0, 1400, 50);
  place(document.querySelector('[data-focus-key="above"]'), 0, 0, 100, 50);
  place(document.querySelector('[data-focus-group="wall"]'), 0, 100, 1400, 900);
  place(document.querySelector('[data-focus-group="right"]'), 1500, 300, 300, 100);
  place(document.querySelector('[data-focus-key="right"]'), 1500, 300, 300, 50);
  document.querySelectorAll<HTMLElement>("[data-cell]").forEach((el) => {
    const i = Number(el.dataset.index);
    place(el, (i % 7) * 200, 100 + Math.floor(i / 7) * 300, 180, 270);
  });
}

async function enterAt(index: number) {
  layout();
  await press("ArrowDown");
  layout();
  for (let i = 0; i < index % 7; i++) await press("ArrowRight");
  for (let i = 0; i < Math.floor(index / 7); i++) await press("ArrowDown");
}

test("a wall of 5,000 mounts a bounded number of works", async () => {
  render(<Page items={films(5000)} />);
  await tick();
  const mounted = document.querySelectorAll("[data-work]").length;
  expect(mounted).toBeGreaterThan(0);
  expect(mounted).toBeLessThan(100);
});

test("down from the last full row into a shorter one lands on the last item", async () => {
  render(<Page items={films(10)} />);
  await tick();
  await enterAt(5);
  expect(active()).toBe("item:6");
  await press("ArrowDown");
  expect(active()).toBe("item:10");
});

test("down into a cell that exists goes straight down", async () => {
  render(<Page items={films(10)} />);
  await tick();
  await enterAt(1);
  await press("ArrowDown");
  expect(active()).toBe("item:9");
});

test("right at the row's end is left for the bridge", async () => {
  render(<Page items={films(20)} />);
  await tick();
  await enterAt(6);
  expect(active()).toBe("item:7");
  await press("ArrowRight");
  expect(active()).toBe("right");
});

test("up from row 0 is left for the bridge", async () => {
  render(<Page items={films(20)} />);
  await tick();
  await enterAt(3);
  await press("ArrowUp");
  expect(active()).toBe("above");
});

test("down from the last row stays on the wall's last row", async () => {
  render(<Page items={films(10)} />);
  await tick();
  await enterAt(8);
  await press("ArrowDown");
  expect(active()).toBe("item:9");
});

test("focus is reported and Enter opens", async () => {
  const opened = mock((_i: ItemSummary) => {});
  const seen: number[] = [];
  render(<Page items={films(10)} onOpen={opened} onFocusItem={(i) => seen.push(i.id)} />);
  await tick();
  await enterAt(2);
  expect(seen).toEqual([1, 2, 3]);
  await press("Enter");
  expect(opened.mock.calls[0]![0].id).toBe(3);
});

test("coming back to the wall lands on the cell it remembers", async () => {
  render(<Page items={films(20)} />);
  await tick();
  await enterAt(13);
  expect(active()).toBe("item:14");
  await press("ArrowRight");
  expect(active()).toBe("right");
  layout();
  // Nearest from "right" would be item 7, on row 0; the wall remembers item 14.
  await press("ArrowLeft");
  expect(active()).toBe("item:14");
});
