import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { tauri } from "../../test/setup";
import { summary } from "../../test/fixtures";
import { installBridge } from "@/focus/bridge";
import { FocusScope } from "@/focus/react";
import type { ItemSummary, Shelf } from "@/lib/api";
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
  document.body.innerHTML = "";
});

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const settle = (ms = 60) => act(() => sleep(ms));

function shelves(data: Partial<Record<Shelf, ItemSummary[]>>) {
  tauri.handle("shelf", (args) => data[args.shelf as Shelf] ?? []);
}

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
  home();
  await settle();
  expect(document.body.textContent).toContain("Nothing here yet.");
  expect(document.body.textContent).toContain("Exhibit is still looking through your folders.");
  expect(document.body.textContent).toContain("Scanning, 214 files");
  expect(document.querySelector("section[aria-label]")).toBeNull();
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
