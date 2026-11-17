import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { tauri } from "../../test/setup";
import { detail, summary } from "../../test/fixtures";
import { screens, select } from "../../test/app";
import { toast } from "@/components/ui/toast";
import { installBridge } from "@/focus/bridge";
import { usePlayback } from "@/lib/playback";
import { FocusScope } from "@/focus/react";
import type { ItemSummary, Shelf, Source } from "@/lib/api";
import { Home } from "./Home";

let uninstall: () => void = () => {};
beforeEach(() => {
  uninstall = installBridge();
  tauri.handle("item", () => {
    throw { kind: "not_found", message: "not_found: no such item" };
  });
  tauri.handle("job_status", () => ({ phase: "scanning", done: 214, message: null, last_error: null }));
});
afterEach(() => {
  cleanup();
  uninstall();
  toast.dismiss();
  // A play started here would hold the playing layer over later screen tests.
  usePlayback.setState({ playing: null });
  document.body.innerHTML = "";
});

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const settle = (ms = 60) => act(() => sleep(ms));

function shelves(data: Partial<Record<Shelf, ItemSummary[]>>) {
  tauri.handle("shelf", (args) => data[args.shelf as Shelf] ?? []);
}

function status(sources: Source[]) {
  tauri.handle("status", () => ({
    movies: 1, shows: 0, other: 0, needs_review: 0, unprobed: 0, tmdb_key: true, agent_key: false, sources,
  }));
}
const MOVIES: Source = { id: 1, path: "D:\\Movies", volume_id: "a", online: true };

function home() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <FocusScope id="entry">
        <Home />
      </FocusScope>
    </QueryClientProvider>,
  );
}

const titles = () => [...document.querySelectorAll("section[aria-label]")].map((s) => s.getAttribute("aria-label"));
const placardTitle = () => document.querySelector('[data-placard] [data-line="title"]')?.textContent;

test("renders only the shelves that have items, in their fixed order", async () => {
  shelves({
    loved: [summary({ id: 4, title: "Ran" })],
    recently_added: [summary({ id: 1 }), summary({ id: 2, title: "Heat" })],
    continue_watching: [summary({ id: 3, title: "Stalker", progress: 0.4 })],
  });
  home();
  await settle();
  expect(titles()).toEqual(["Continue watching", "Recently added", "Loved"]);
});

test("focusing a work puts its title on the placard", async () => {
  shelves({ recently_added: [summary({ id: 1, title: "Taxi Driver" }), summary({ id: 2, title: "Heat" })] });
  home();
  await settle();
  expect(placardTitle()).toBe("Taxi Driver");
  await act(async () => {
    document.querySelector<HTMLElement>('[data-focus-key="home-recently_added:item:2"]')!.focus();
    await sleep(20);
  });
  expect(placardTitle()).toBe("Heat");
});

test("an empty library says Exhibit is still looking, with what the worker is doing", async () => {
  shelves({});
  status([MOVIES]);
  home();
  await settle();
  expect(document.body.textContent).toContain("Nothing here yet.");
  expect(document.body.textContent).toContain("Exhibit is still looking through your folders.");
  expect(document.body.textContent).toContain("Scanning, 214 files");
  expect(document.querySelector("section[aria-label]")).toBeNull();
});

test("with no folders, the empty library says to add one and offers Settings", async () => {
  shelves({});
  status([]);
  tauri.handle("job_status", () => ({ phase: null, done: 0, message: null, last_error: null }));
  home();
  await settle();
  expect(document.body.textContent).toContain("Add the folders that hold your films and shows in Settings.");
  expect(document.body.textContent).not.toContain("still looking");
  expect(document.activeElement?.textContent).toBe("Open Settings");
});

test("A on a film whose drive is away never plays, and the message names the drive", async () => {
  shelves({ recently_added: [summary({ id: 7, title: "Heat", available: false })] });
  tauri.handle("play", () => null);
  status([{ ...MOVIES, online: false }]);
  tauri.handle("item", () =>
    detail({ available: false, files: [{ id: 1, source_id: 1, rel_path: "Heat.mkv", size: 1, mtime: 0, item_id: 7, tech: null }] }, { id: 7 }),
  );
  screens({ screen: "home" });
  await settle(120);
  await select();
  await settle(120);
  expect(tauri.calls().some((c) => c.cmd === "play")).toBe(false);
  expect(document.querySelector('[data-slot="toast-title"]')?.textContent).toBe("The drive for D:\\Movies is not connected");
});

test("A on a film plays it; the hero's action line says so", async () => {
  shelves({ recently_added: [summary({ id: 7, title: "Heat" })] });
  tauri.handle("play", () => null);
  home();
  await settle();
  expect(document.querySelector("[data-placard]")?.textContent).toContain("Play");
  await act(async () => {
    (document.activeElement as HTMLElement).click();
    await sleep(20);
  });
  expect(tauri.calls().filter((c) => c.cmd === "play")).toEqual([{ cmd: "play", args: { id: 7, resume: false } }]);
});
