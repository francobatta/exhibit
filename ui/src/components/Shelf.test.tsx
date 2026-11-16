import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, mock, test } from "bun:test";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { films } from "../../test/fixtures";
import { installBridge, openMenu } from "@/focus/bridge";
import { FocusGroup, FocusScope, useFocusItem } from "@/focus/react";
import type { ItemSummary } from "@/lib/api";
import { useItemMenu } from "./ItemMenu";
import { Shelf } from "./Shelf";

let uninstall: () => void = () => {};
beforeEach(() => {
  uninstall = installBridge();
});
afterEach(() => {
  cleanup();
  uninstall();
  useItemMenu.setState({ item: null, anchor: null, open: false });
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
const works = () => document.querySelectorAll("[data-work]").length;

function place(el: Element | null, left: number, top: number, width: number, height: number) {
  (el as HTMLElement).getBoundingClientRect = () =>
    ({ left, top, right: left + width, bottom: top + height, width, height, x: left, y: top }) as DOMRect;
}

function Above() {
  const { ref, props } = useFocusItem<HTMLButtonElement>({ key: "above" });
  return <button ref={ref} {...props}>above</button>;
}

function Page({ items, onOpen = () => {}, onFocusItem }: { items: ItemSummary[]; onOpen?: (i: ItemSummary) => void; onFocusItem?: (i: ItemSummary) => void }) {
  return (
    <QueryClientProvider client={new QueryClient()}>
      <FocusScope id="page">
        <FocusGroup id="top" orientation="horizontal">
          <Above />
        </FocusGroup>
        <Shelf id="recent" title="Recently added" items={items} onOpen={onOpen} onFocusItem={onFocusItem} />
      </FocusScope>
    </QueryClientProvider>
  );
}

function layout() {
  place(document.querySelector('[data-focus-group="top"]'), 0, 0, 2000, 50);
  place(document.querySelector('[data-focus-key="above"]'), 0, 0, 100, 50);
  place(document.querySelector('[data-focus-group="recent"]'), 0, 100, 2000, 300);
  document.querySelectorAll<HTMLElement>('[data-focus-group="recent"] [data-focus-key]').forEach((el) => {
    const i = Number(el.dataset.index);
    place(el, 72 + i * 200, 100, 176, 264);
  });
}

test("a shelf of 300 mounts only a few works", async () => {
  render(<Page items={films(300)} />);
  await tick();
  expect(works()).toBeGreaterThan(0);
  expect(works()).toBeLessThan(30);
});

test("arrows walk the shelf by index, report the focused work and open it", async () => {
  const focusedItems: number[] = [];
  const opened = mock((_i: ItemSummary) => {});
  render(<Page items={films(12)} onOpen={opened} onFocusItem={(i) => focusedItems.push(i.id)} />);
  await tick();
  layout();
  await press("ArrowDown");
  layout();
  expect(active()).toBe("recent:item:1");
  await press("ArrowRight");
  await press("ArrowRight");
  expect(active()).toBe("recent:item:3");
  expect(focusedItems).toEqual([1, 2, 3]);
  await act(async () => {
    (document.activeElement as HTMLElement).click();
    await sleep(10);
  });
  expect(opened.mock.calls[0]![0].id).toBe(3);
});

test("Left at the first work is left for the bridge; Up leaves the shelf", async () => {
  render(<Page items={films(5)} />);
  await tick();
  layout();
  await press("ArrowDown");
  layout();
  await press("ArrowLeft");
  expect(active()).toBe("recent:item:1");
  await press("ArrowUp");
  expect(active()).toBe("above");
});

test("returning to the shelf lands on the work it remembers", async () => {
  render(<Page items={films(12)} />);
  await tick();
  layout();
  await press("ArrowDown");
  layout();
  await press("ArrowRight");
  await press("ArrowRight");
  await press("ArrowRight");
  expect(active()).toBe("recent:item:4");
  await press("ArrowUp");
  expect(active()).toBe("above");
  layout();
  await press("ArrowDown");
  expect(active()).toBe("recent:item:4");
});

test("End reaches the last work though it was not rendered", async () => {
  render(<Page items={films(300)} />);
  await tick();
  layout();
  await press("ArrowDown");
  expect(document.querySelector('[data-focus-key="recent:item:300"]')).toBeNull();
  await press("End");
  await tick(50);
  expect(active()).toBe("recent:item:300");
});

// X, m and right-click all reach the focused item's onMenu through the bridge's openMenu.
test("X, m and right-click open the item menu on the focused work", async () => {
  render(<Page items={films(5)} />);
  await tick();
  layout();
  await press("ArrowDown");
  await act(async () => {
    openMenu();
    await sleep(5);
  });
  const state = useItemMenu.getState();
  expect(state.open).toBe(true);
  expect(state.item && "id" in state.item ? state.item.id : null).toBe(1);
  expect(state.anchor).toBe(document.querySelector<HTMLElement>('[data-focus-key="recent:item:1"]'));
});
