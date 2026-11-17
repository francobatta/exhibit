import { act, cleanup } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { activeKey, screens, select, settle } from "../../test/app";
import { tauri } from "../../test/setup";
import { focusItem, installBridge } from "@/focus/bridge";
import type { LibraryStatus, ToolStatus } from "@/lib/api";
import { openAsk, useAskOverlay, useNav } from "@/nav/stack";

let uninstall: () => void = () => {};

const found = (tool: ToolStatus["tool"]): ToolStatus => ({ tool, path: `C:\\tools\\${tool}.exe`, version: `${tool} 1.0`, manual: false });

const STATUS: LibraryStatus = { movies: 0, shows: 0, other: 0, needs_review: 0, unprobed: 0, sources: [], tmdb_key: false, agent_key: false };

beforeEach(() => {
  uninstall = installBridge();
  // Home's own reads, for once first run is done.
  tauri.handle("shelf", () => []);
  tauri.handle("search", () => []);
  tauri.handle("history", () => []);
  tauri.handle("job_status", () => ({ phase: null, done: 0, message: null, last_error: null }));
});
afterEach(() => {
  cleanup();
  uninstall();
  document.body.innerHTML = "";
});

const heading = () => document.querySelector("h1")?.textContent;

async function choose(key: string) {
  await act(async () => {
    focusItem(key);
  });
  await select();
  await settle();
}

test("with every tool found, the tools step is left out, and skipping every step ends at Home", async () => {
  const all = [found("mpv"), found("ffprobe"), found("ffmpeg")];
  tauri.handle("status", () => STATUS);
  tauri.handle("detect_tools", () => all);
  screens({ screen: "firstRun" });
  await settle(120);
  expect(heading()).toBe("Welcome to Exhibit");
  expect(document.body.textContent).toContain("Step 1 of 2");
  expect(activeKey()).toBe("first:folder:main");
  // Ask stays out of first run.
  openAsk();
  expect(useAskOverlay.getState().open).toBe(false);

  await choose("first:folder:skip");
  expect(heading()).toBe("TMDB key");
  expect(activeKey()).toBe("first:tmdb:field");

  await choose("first:tmdb:skip");
  expect(useNav.getState().entries.map((e) => e.route)).toEqual([{ screen: "home" }]);
  expect(tauri.calls().some((c) => c.cmd === "set_secret" || c.cmd === "add_source")).toBe(false);
});

test("a missing tool gets its own step, which can be skipped too", async () => {
  const tools = [{ tool: "mpv", path: null, version: null, manual: false } as ToolStatus, found("ffprobe"), found("ffmpeg")];
  tauri.handle("status", () => STATUS);
  tauri.handle("detect_tools", () => tools);
  screens({ screen: "firstRun" });
  await settle(120);

  expect(document.body.textContent).toContain("Step 1 of 3");
  await choose("first:folder:skip");
  expect(heading()).toBe("Media tools");
  expect(document.body.textContent).toContain("Step 2 of 3");
  expect(document.body.textContent).toContain("mpv plays your files. Nothing can play until mpv is set.");
  await choose("first:tools:skip");
  expect(heading()).toBe("TMDB key");
  await choose("first:tmdb:skip");
  expect(useNav.getState().current).toEqual({ screen: "home" });
});
