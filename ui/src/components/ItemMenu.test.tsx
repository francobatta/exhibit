import { act, cleanup, render } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { detail, summary } from "../../test/fixtures";
import { until } from "../../test/app";
import { tauri } from "../../test/setup";
import { installBridge } from "@/focus/bridge";
import type { ItemSummary } from "@/lib/api";
import { ConfirmHost } from "./Dialog";
import { ItemMenuHost, openItemMenu, useItemMenu } from "./ItemMenu";

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
const tick = (ms = 40) => act(() => sleep(ms));
const press = (key: string) =>
  act(async () => {
    (document.activeElement ?? document.body).dispatchEvent(
      new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }),
    );
    await sleep(40);
  });
const select = () =>
  act(async () => {
    (document.activeElement as HTMLElement).click();
    await sleep(60);
  });
const focusedText = () => document.activeElement?.textContent ?? "";
const menuLabels = () => [...document.querySelectorAll('[role="menuitem"]')].map((m) => m.firstElementChild?.textContent);

// A menu left open when a test ends holds UI SDK's layer stack and delays the next test's.
async function close() {
  await press("Escape");
  await tick(80);
}

function setup(item: ItemSummary) {
  tauri.handle("item", () =>
    detail({ play: { item_id: item.id, position_s: 3852, duration_s: 6840, watched: false, last_played_at: 0 }, available: item.available }, { id: item.id }),
  );
  tauri.handle("set_watched", () => null);
  tauri.handle("move_to_other", () => null);
  const view = render(
    <QueryClientProvider client={new QueryClient()}>
      <button id="opener">work</button>
      <ItemMenuHost />
      <ConfirmHost />
    </QueryClientProvider>,
  );
  const opener = document.getElementById("opener")!;
  opener.focus();
  return { view, opener };
}

async function open(item: ItemSummary, opener: HTMLElement) {
  await act(async () => {
    openItemMenu(item, opener);
  });
  // It opens after a short wait for the full record and its dialog's open.
  await until(() => document.activeElement?.closest('[role="menu"]') != null);
  await tick(20);
}

test("opens on the work's actions with the first one focused, from the full record", async () => {
  const item = summary({ id: 7, progress: 0.56 });
  const { opener } = setup(item);
  await open(item, opener);
  expect(focusedText()).toContain("Resume");
  await tick();
  expect(menuLabels().slice(0, 2)).toEqual(["Resume from 1:04:12", "Play from the start"]);
  await close();
});

test("Back closes it and focus returns to the work", async () => {
  const item = summary({ id: 7 });
  const { opener } = setup(item);
  await open(item, opener);
  expect(document.querySelector('[role="menu"]')).not.toBeNull();
  await press("Escape");
  await tick(80);
  expect(document.querySelector('[role="menu"]')).toBeNull();
  expect(document.activeElement).toBe(opener);
  expect(useItemMenu.getState().open).toBe(false);
});

test("a choice runs once the menu has closed", async () => {
  const item = summary({ id: 7 });
  const { opener } = setup(item);
  await open(item, opener);
  await tick();
  for (let i = 0; i < 12 && !focusedText().startsWith("Mark as watched"); i++) await press("ArrowDown");
  await select();
  await tick(80);
  expect(tauri.calls().filter((c) => c.cmd === "set_watched")[0]?.args).toEqual({ id: 7, watched: true });
  expect(document.activeElement).toBe(opener);
});

test("Send to Other asks first; Back answers no and returns focus to the work", async () => {
  const item = summary({ id: 7 });
  const { opener } = setup(item);
  await open(item, opener);
  await tick();
  for (let i = 0; i < 12 && !focusedText().startsWith("Send to Other"); i++) await press("ArrowDown");
  await select();
  await tick(100);
  expect(document.querySelector('[role="dialog"][aria-label^="Send"]')).not.toBeNull();
  expect(focusedText()).toBe("Cancel");
  await press("Escape");
  await tick(100);
  expect(tauri.calls().some((c) => c.cmd === "move_to_other")).toBe(false);
  expect(document.activeElement).toBe(opener);
});

test("Send to Other goes ahead when confirmed", async () => {
  const item = summary({ id: 7 });
  const { opener } = setup(item);
  await open(item, opener);
  await tick();
  for (let i = 0; i < 12 && !focusedText().startsWith("Send to Other"); i++) await press("ArrowDown");
  await select();
  await tick(100);
  await press("ArrowRight");
  expect(focusedText()).toBe("Send to Other");
  await select();
  await tick(100);
  expect(tauri.calls().filter((c) => c.cmd === "move_to_other")[0]?.args).toEqual({ id: 7 });
});

test("an offline work's play actions are shown, disabled, with the reason", async () => {
  const item = summary({ id: 7, available: false });
  const { opener } = setup(item);
  await open(item, opener);
  await tick();
  const first = document.querySelector('[role="menuitem"]')!;
  expect(first.getAttribute("aria-disabled")).toBe("true");
  expect(first.textContent).toContain("Drive is not connected");
  await close();
});
