import { act, cleanup } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { activeKey, press, screens, select, settle, sleep } from "../../test/app";
import { summary } from "../../test/fixtures";
import { tauri } from "../../test/setup";
import { SHOW, fakeShow } from "../../test/show";
import { openItemMenu, useItemMenu } from "@/components/ItemMenu";
import { installBridge } from "@/focus/bridge";

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

test("on entry the next unwatched episode has focus and its season is chosen", async () => {
  fakeShow(21);
  screens({ screen: "show", id: SHOW });
  await settle(120);
  expect(activeKey()).toBe("episode:21");
  expect(document.querySelector('[data-focus-key="season:102"]')?.hasAttribute("data-pressed")).toBe(true);
});

test("Mark all as watched on the show's menu marks the show once", async () => {
  fakeShow(12);
  tauri.handle("set_watched", () => null);
  screens({ screen: "show", id: SHOW });
  await settle(120);
  await act(async () => {
    openItemMenu(summary({ id: SHOW, kind: "show", title: "Severance", progress: 0.25, path: null }), document.activeElement as HTMLElement);
    for (let i = 0; i < 100 && !document.activeElement?.closest('[role="menu"]'); i++) await sleep(10);
  });
  for (let i = 0; i < 12 && !document.activeElement?.textContent?.startsWith("Mark all as watched"); i++) await press("ArrowDown");
  await select();
  await settle(120);
  expect(tauri.calls().filter((c) => c.cmd === "set_watched").map((c) => c.args)).toEqual([{ id: SHOW, watched: true }]);
});
