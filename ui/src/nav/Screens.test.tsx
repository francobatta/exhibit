import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { tauri } from "../../test/setup";
import { installBridge } from "@/focus/bridge";
import { FocusScope } from "@/focus/react";
import type { ItemSummary } from "@/lib/api";
import { openSettings, Screens } from "./Screens";
import { useNav } from "./stack";

function summary(id: number): ItemSummary {
  return {
    id,
    kind: "show",
    title: `Film ${id}`,
    year: 1979,
    season: null,
    episode: null,
    show_title: null,
    poster: null,
    backdrop: null,
    runtime_min: 117,
    width: 1920,
    height: 1080,
    hdr: null,
    watched: false,
    progress: null,
    loved: false,
    available: true,
    match_state: "matched",
    genres: [],
    path: null,
    in_library: true,
  };
}

let uninstall: () => void = () => {};
beforeEach(() => {
  uninstall = installBridge();
  useNav.getState().replaceRoot({ screen: "home" });
  tauri.handle("shelf", () => [summary(1), summary(2), summary(3)]);
  tauri.handle("item", () => {
    throw { kind: "not_found", message: "not_found: no such item" };
  });
});
afterEach(() => {
  cleanup();
  uninstall();
  document.body.innerHTML = "";
});

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const press = (key: string) =>
  act(async () => {
    (document.activeElement ?? document.body).dispatchEvent(
      new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }),
    );
    await sleep(20);
  });
const settle = (ms = 60) => act(() => sleep(ms));
const activeKey = () => (document.activeElement as HTMLElement | null)?.dataset.focusKey;
const activeEntry = () => (document.activeElement as HTMLElement | null)?.closest("[data-entry]")?.getAttribute("data-entry");

function app() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <Screens />
    </QueryClientProvider>,
  );
}

test("Back returns focus to the item the previous screen remembered, through the real stack", async () => {
  // The shelves live in a Spotlight (these works are shows); Home is empty until the owner chooses a page.
  useNav.getState().replaceRoot({ screen: "spotlight", kind: "show" });
  app();
  await settle();
  const home = String(useNav.getState().entries[0]!.id);
  expect(activeKey()).toBe("home-continue_watching:item:1");
  await press("ArrowRight");
  expect(activeKey()).toBe("home-continue_watching:item:2");
  expect(useNav.getState().focusKey).toBe("home-continue_watching:item:2");

  // A on a show opens its screen; the show cannot be read here, so its error offers a way back.
  await act(async () => {
    (document.activeElement as HTMLElement).click();
    await sleep(20);
  });
  await settle();
  expect(useNav.getState().current).toEqual({ screen: "show", id: 2 });
  expect(activeKey()).toBe("state:action");
  expect(activeEntry()).not.toBe(home);

  await press("Escape");
  await settle(150);
  expect(useNav.getState().entries.length).toBe(1);
  expect(activeEntry()).toBe(home);
  expect(activeKey()).toBe("home-continue_watching:item:2");
  expect(useNav.getState().entries[0]!.focusKey).toBe("home-continue_watching:item:2");
});

test("the settings action does nothing when Settings is already on top or a modal is open", async () => {
  openSettings();
  expect(useNav.getState().entries.length).toBe(2);
  openSettings();
  expect(useNav.getState().entries.length).toBe(2);

  useNav.getState().replaceRoot({ screen: "home" });
  render(
    <FocusScope id="menu" modal label="Menu" onBack={() => {}}>
      <span />
    </FocusScope>,
  );
  await settle(20);
  openSettings();
  expect(useNav.getState().entries.length).toBe(1);
});
