import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { useState } from "react";
import { films, scoresService, service, summary } from "../../test/fixtures";
import { until } from "../../test/app";
import { tauri } from "../../test/setup";
import { ItemMenuHost, useItemMenu } from "@/components/ItemMenu";
import { DEFAULT_VIEW, type WallKind, type WallView } from "@/nav/stack";
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

let shownView: WallView = DEFAULT_VIEW;
function Harness({ initial, kind }: { initial: WallView; kind: WallKind }) {
  const [view, setView] = useState(initial);
  shownView = view;
  return <Wall kind={kind} view={view} onView={setView} />;
}

let client = new QueryClient();
function wall(items: ItemSummary[], initial: WallView = DEFAULT_VIEW, kind: WallKind = "movie") {
  tauri.handle("search", () => items);
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <FocusScope id="entry">
        <Harness initial={initial} kind={kind} />
      </FocusScope>
      <ItemMenuHost />
    </QueryClientProvider>,
  );
}

const ACTIVITY = { rated: 40, reviewed: 0, logged: 50, watchlist: 3, not_in_library: 1 };
function facets() {
  tauri.handle("facets", () => ({
    genres: [], years: [], runtimes: [], resolution: [], picture: [], sound: [], audio: [], subtitles: [], activity: ACTIVITY,
  }));
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

test("activity_section_only_with_activity", async () => {
  facets();
  tauri.handle("services", () => [service()]);
  wall(films(3));
  await settle();
  await click(document.querySelector("[data-focus-key='wall:filters']"));
  await until(() => document.querySelector("section[aria-label='Your activity']") !== null);
  const section = document.querySelector("section[aria-label='Your activity']")!;
  // After Watching; each choice with how many it leaves. Reviewed has none, so it is not offered.
  const sections = [...document.querySelectorAll("section[aria-label]")].map((s) => s.getAttribute("aria-label"));
  expect(sections.slice(0, 2)).toEqual(["Watching", "Your activity"]);
  const chips = [...section.querySelectorAll("[data-focus-key]")].map((c) => c.getAttribute("data-focus-key"));
  expect(chips).toEqual(["filters:rated", "filters:logged", "filters:watchlist"]);
  expect(section.querySelector("[data-focus-key='filters:rated']")?.textContent).toBe("Rated40");
  await click(section.querySelector("[data-focus-key='filters:watchlist']"));
  expect(searches().at(-1)).toEqual({ kind: "movie", watchlist: true, limit: 500, offset: 0 });
  await click(section.querySelector("[data-focus-key='filters:watchlist']"));
  expect(searches().at(-1)).toEqual({ kind: "movie", limit: 500, offset: 0 });
  cleanup();
  document.body.innerHTML = "";
  tauri.handle("services", () => [service({ enabled: false })]);
  wall(films(3));
  await settle();
  await click(document.querySelector("[data-focus-key='wall:filters']"));
  await until(() => document.querySelector("section[aria-label='Watching']") !== null);
  expect(document.querySelector("section[aria-label='Your activity']")).toBeNull();
});

test("my_rating_sort_only_with_activity", async () => {
  tauri.handle("services", () => [service()]);
  wall(films(3));
  await settle();
  const rating = () => document.querySelector("[data-focus-key='wall:sort:my_rating']");
  expect(rating()?.textContent).toBe("Your rating");
  await click(rating());
  // Highest first, unrated last: the core's own direction, so nothing more is asked.
  expect(searches().at(-1)).toEqual({ kind: "movie", sort: "my_rating", limit: 500, offset: 0 });
  cleanup();
  document.body.innerHTML = "";
  // Shows have no rating from the service.
  wall(films(3), DEFAULT_VIEW, "show");
  await settle();
  expect(document.querySelector("[data-focus-key='wall:sort:year']")).not.toBeNull();
  expect(rating()).toBeNull();
  cleanup();
  document.body.innerHTML = "";
  tauri.handle("services", () => []);
  wall(films(3));
  await settle();
  expect(document.querySelector("[data-focus-key='wall:sort:year']")).not.toBeNull();
  expect(rating()).toBeNull();
});

test("watchlist_view_counts_and_dims_wanted_works", async () => {
  facets();
  tauri.handle("services", () => [service()]);
  const items = [
    summary({ id: 1, title: "Heat" }),
    summary({ id: 2, title: "Ran", available: false }),
    summary({ id: 3, title: "Paris, Texas", in_library: false, available: false }),
  ];
  wall(items, { ...DEFAULT_VIEW, filters: { watchlist: true } });
  await settle(120);
  const row = document.querySelector("[data-focus-group='wall-controls'] [role='status']")!.textContent;
  expect(row).toBe("3 films on your watchlist, 1 not in your library");
  const work = (id: number) => document.querySelector(`[data-focus-key='item:${id}'] [data-work]`)!;
  expect(work(3).hasAttribute("data-wanted")).toBe(true);
  expect(work(3).hasAttribute("data-offline")).toBe(false);
  expect(work(2).hasAttribute("data-offline")).toBe(true);
  expect(work(1).hasAttribute("data-wanted")).toBe(false);
  // The panel offers no play for a film the owner does not have: Details, the heart and the eye.
  await act(async () => {
    document.querySelector<HTMLElement>("[data-focus-key='item:3']")!.focus();
    await sleep(40);
  });
  const panel = document.querySelector("aside[aria-label='Focused work'] [data-line=action]")!;
  expect(panel.querySelector("button:not([aria-pressed])")).toBeNull();
  expect(panel.textContent).toBe("Details");
  expect(panel.querySelectorAll("[aria-pressed]").length).toBe(2);
  expect(document.querySelector("aside[aria-label='Focused work']")?.textContent).toContain("Not in your library");
});

test("turning_off_drops_hidden_filters_from_the_view", async () => {
  let services = [service()];
  tauri.handle("services", () => services);
  wall(films(3), { sort: "my_rating", descending: false, filters: { watchlist: true, rated: true, loved: true } });
  await settle();
  expect(searches().at(-1)).toMatchObject({ watchlist: true, rated: true, loved: true, sort: "my_rating" });
  services = [service({ enabled: false })];
  await act(async () => {
    await client.invalidateQueries({ queryKey: ["library", "services"] });
    await sleep(60);
  });
  expect(shownView).toEqual({ sort: null, descending: false, filters: { loved: true } });
  expect(searches().at(-1)).toEqual({ kind: "movie", loved: true, limit: 500, offset: 0 });
  expect(document.querySelector("[data-focus-key='wall:sort:my_rating']")).toBeNull();
});

test("rating_sort_only_while_a_service_brings_scores", async () => {
  tauri.handle("services", () => [scoresService()]);
  wall(films(3), DEFAULT_VIEW, "show");
  await settle();
  const rating = () => document.querySelector("[data-focus-key='wall:sort:rating']");
  expect(rating()?.textContent).toBe("Rating");
  await click(rating());
  expect(searches().at(-1)).toEqual({ kind: "show", sort: "rating", limit: 500, offset: 0 });
  cleanup();
  document.body.innerHTML = "";
  // Signed out, there is no rating to sort by, and a view sorted by it goes back to the wall's own order.
  tauri.handle("services", () => [scoresService({ username: null })]);
  wall(films(3), { sort: "rating", descending: true, filters: {} });
  await settle();
  expect(rating()).toBeNull();
  expect(searches().at(-1)).toEqual({ kind: "movie", limit: 500, offset: 0 });
});
