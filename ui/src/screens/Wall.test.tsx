import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { useState } from "react";
import { films, summary } from "../../test/fixtures";
import { until } from "../../test/app";
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

test("sorting: A picks a sort, A again turns its order round, a third A clears it", async () => {
  const pressed = () => [...document.querySelectorAll("[data-focus-key^='wall:sort:'][aria-pressed='true']")].length;
  wall(films(3));
  await settle();
  expect(searches()[0]).toEqual({ kind: "movie", limit: 500, offset: 0 });
  await click(document.querySelector("[data-focus-key='wall:sort:year']"));
  expect(searches().at(-1)).toMatchObject({ kind: "movie", sort: "year" });
  expect(searches().at(-1)).not.toHaveProperty("descending");
  await click(document.querySelector("[data-focus-key='wall:sort:year']"));
  expect(searches().at(-1)).toMatchObject({ sort: "year", descending: false });
  await click(document.querySelector("[data-focus-key='wall:sort:year']"));
  expect(searches().at(-1)).toEqual({ kind: "movie", limit: 500, offset: 0 });
  expect(pressed()).toBe(0);
  await click(document.querySelector("[data-focus-key='wall:sort:title']"));
  expect(pressed()).toBe(1);
  await click(document.querySelector("[data-focus-key='wall:sort:title']"));
  expect(searches().at(-1)).toMatchObject({ descending: true });
  expect(searches().at(-1)).not.toHaveProperty("sort");
  await click(document.querySelector("[data-focus-key='wall:sort:title']"));
  expect(searches().at(-1)).toEqual({ kind: "movie", limit: 500, offset: 0 });
  expect(pressed()).toBe(0);
});

test("the filter sheet asks the library for exactly what is chosen, and the row says it", async () => {
  tauri.handle("facets", () => ({
    genres: [{ value: "Drama", count: 2 }, { value: "Horror", count: 1 }],
    years: [], runtimes: [], resolution: [{ value: "uhd", count: 1 }], picture: [], sound: [],
    audio: [{ value: "es", count: 1 }], subtitles: [],
  }));
  wall(films(3));
  await settle();
  await click(document.querySelector("[data-focus-key='wall:filters']"));
  await until(() => document.querySelector("[data-focus-key='filters-seen:unwatched']") !== null);
  await click(document.querySelector("[data-focus-key='filters-seen:unwatched']"));
  expect(searches().at(-1)).toEqual({ kind: "movie", seen: ["unwatched"], limit: 500, offset: 0 });
  // A genre is wanted, then left out.
  await click(document.querySelector("[data-focus-key='filters:genre:Horror']"));
  expect(searches().at(-1)).toMatchObject({ genres: ["Horror"] });
  await click(document.querySelector("[data-focus-key='filters:genre:Horror']"));
  expect(searches().at(-1)).toMatchObject({ without_genres: ["Horror"] });
  expect(searches().at(-1)).not.toHaveProperty("genres");
  await click(document.querySelector("[data-focus-key='filters-resolution:uhd']"));
  await click(document.querySelector("[data-focus-key='filters-audio:es']"));
  expect(searches().at(-1)).toMatchObject({ resolution: ["uhd"], audio: ["es"] });
  // No years or runtimes in the wall: no ranges to choose.
  expect(document.querySelector("[data-focus-key^='filters-year']")).toBeNull();
  await click(document.querySelector("[data-focus-key='filters:done']"));
  await settle(300);
  const row = document.querySelector("[data-focus-group='wall-controls']")!.textContent;
  expect(row).toContain("not Horror · Not watched · 4K · Spanish audio");
  await click(byText("button", "Clear filters"));
  expect(searches().at(-1)).toEqual({ kind: "movie", limit: 500, offset: 0 });
});

test("selection mode marks every selected work watched, one call each, and ends", async () => {
  tauri.handle("set_watched", () => null);
  wall(films(3));
  await settle();
  await act(async () => {
    document.querySelector<HTMLElement>('[data-focus-key="item:1"]')!.focus();
    openMenu();
  });
  await until(() => byText('[role="menuitem"] span', "Select several") !== null);
  await settle(20);
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
  wall([], { ...DEFAULT_VIEW, filters: { seen: ["unwatched"] } });
  await settle();
  expect(document.body.textContent).toContain("Nothing matches these filters");
  expect(byText("button", "Clear filters")).not.toBeNull();
});
