import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { useState } from "react";
import { films, summary } from "../../test/fixtures";
import { tauri } from "../../test/setup";
import { ItemMenuHost, useItemMenu } from "@/components/ItemMenu";
import { DEFAULT_VIEW, type WallView } from "@/nav/stack";
import { installBridge, openMenu } from "@/focus/bridge";
import { FocusScope } from "@/focus/react";
import type { ItemSummary } from "@/lib/api";
import { Wall, columnsFor, groupByFolder } from "./Wall";

let uninstall: () => void = () => {};
beforeEach(() => {
  uninstall = installBridge();
  tauri.handle("item", () => {
    throw { kind: "not_found", message: "not_found: no such item" };
  });
  tauri.handle("status", () => ({ movies: 0, shows: 0, other: 0, needs_review: 0, unprobed: 0, sources: [], tmdb_key: false, agent_key: false }));
  tauri.handle("job_status", () => ({ phase: null, done: 0, message: null, last_error: null }));
});
afterEach(() => {
  cleanup();
  uninstall();
  useItemMenu.setState({ item: null, anchor: null, open: false });
  document.body.innerHTML = "";
});

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const settle = (ms = 60) => act(() => sleep(ms));
const click = (el: Element | null) =>
  act(async () => {
    (el as HTMLElement).click();
    await sleep(60);
  });
const byText = (selector: string, text: string) =>
  [...document.querySelectorAll(selector)].find((el) => el.textContent?.trim() === text) ?? null;
const searches = () => tauri.calls().filter((c) => c.cmd === "search").map((c) => c.args.query);

function Harness({ initial }: { initial: WallView }) {
  const [view, setView] = useState(initial);
  return <Wall kind="movie" view={view} onView={setView} />;
}

function wall(items: ItemSummary[], initial: WallView = DEFAULT_VIEW) {
  tauri.handle("search", () => items);
  return render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <FocusScope id="entry">
        <Harness initial={initial} />
      </FocusScope>
      <ItemMenuHost />
    </QueryClientProvider>,
  );
}

describe("columnsFor", () => {
  test("posters by window width, two fewer beside the panel; wide works four to six", () => {
    expect([1600, 1920, 2560, 3840].map((w) => columnsFor(w, "movie", false))).toEqual([7, 7, 9, 11]);
    expect([1920, 2560, 3840].map((w) => columnsFor(w, "show", true))).toEqual([5, 7, 9]);
    expect([1920, 2560, 3840].map((w) => columnsFor(w, "other", true))).toEqual([4, 5, 6]);
  });
});

test("groupByFolder keeps groups in path order and items in title order", () => {
  const paths: Record<number, string> = {
    1: "D:\\Video\\Trips\\b.mkv",
    2: "D:\\Video\\Birthdays\\z.mkv",
    3: "D:\\Video\\Trips\\a.mkv",
    4: "D:\\Video\\Birthdays\\y.mkv",
  };
  const items = [
    summary({ id: 1, title: "Lisbon" }),
    summary({ id: 2, title: "Ana turns six" }),
    summary({ id: 3, title: "Kyoto" }),
    summary({ id: 4, title: "Ana turns five" }),
  ];
  const groups = groupByFolder(items, (i) => paths[i.id]!);
  expect(groups.map((g) => g.title)).toEqual(["Birthdays", "Trips"]);
  expect(groups.map((g) => g.items.map((i) => i.title))).toEqual([["Ana turns five", "Ana turns six"], ["Kyoto", "Lisbon"]]);
});

test("a filter toggled on asks the library for exactly that", async () => {
  wall(films(3));
  await settle();
  expect(searches()[0]).toMatchObject({ kind: "movie" });
  expect(searches()[0]).not.toHaveProperty("watched");
  await click(byText("[data-focus-key^='wall:filter:']", "Not watched"));
  expect(searches().at(-1)).toMatchObject({ kind: "movie", watched: false });
});

test("selection mode marks every selected work watched, one call each, and ends", async () => {
  tauri.handle("set_watched", () => null);
  wall(films(3));
  await settle();
  await act(async () => {
    document.querySelector<HTMLElement>('[data-focus-key="item:1"]')!.focus();
    openMenu();
    for (let i = 0; i < 100 && byText('[role="menuitem"] span', "Select several") === null; i++) await sleep(10);
    await sleep(20);
  });
  await click(byText('[role="menuitem"] span', "Select several")!.closest('[role="menuitem"]'));
  await settle(120);
  await click(document.querySelector('[data-focus-key="item:2"]'));
  await click(document.querySelector('[data-focus-key="item:3"]'));
  expect(document.body.textContent).toContain("3 selected");
  await click(byText("button", "Mark as watched"));
  await settle();
  const marked = tauri.calls().filter((c) => c.cmd === "set_watched").map((c) => c.args);
  expect(marked).toEqual([
    { id: 1, watched: true },
    { id: 2, watched: true },
    { id: 3, watched: true },
  ]);
  expect(document.body.textContent).not.toContain("selected");
  expect(byText("[data-focus-key^='wall:sort:']", "Title")).not.toBeNull();
});

test("an empty wall says what to do; filters that match nothing offer to clear them", async () => {
  wall([]);
  await settle();
  expect(document.body.textContent).toContain("No movies yet");
  cleanup();
  wall([], { ...DEFAULT_VIEW, unwatched: true });
  await settle();
  expect(document.body.textContent).toContain("Nothing matches these filters");
  expect(byText("button", "Clear filters")).not.toBeNull();
});
