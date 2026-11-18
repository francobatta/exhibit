import { act, cleanup } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { activeKey, screens, select, settle } from "../../test/app";
import { tauri } from "../../test/setup";
import { focusItem, installBridge } from "@/focus/bridge";
import type { LibraryStatus, ToolStatus } from "@/lib/api";
import { openAsk, useAskOverlay, useNav } from "@/nav/stack";

let uninstall: () => void = () => {};

const found = (tool: ToolStatus["tool"]): ToolStatus => ({ tool, path: `C:\\tools\\${tool}.exe`, version: `${tool} 1.0`, source: "detected", release: null, downloaded: false });
const missing = (tool: ToolStatus["tool"]): ToolStatus => ({ tool, path: null, version: null, source: null, release: null, downloaded: false });

const STATUS: LibraryStatus = { movies: 0, shows: 0, other: 0, needs_review: 0, unprobed: 0, sources: [], tmdb_key: false, agent_key: false, intro: true };

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
  const all = [found("mpv"), found("ffmpeg")];
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
  tauri.handle("status", () => STATUS);
  tauri.handle("detect_tools", () => [missing("mpv"), found("ffmpeg")]);
  tauri.handle("tools_download_status", () => ({ running: false, step: null, outcome: null }));
  screens({ screen: "firstRun" });
  await settle(120);

  expect(document.body.textContent).toContain("Step 1 of 3");
  await choose("first:folder:skip");
  expect(heading()).toBe("Media tools");
  expect(document.body.textContent).toContain("Step 2 of 3");
  expect(document.body.textContent).toContain("mpv plays your files and reads their details. Nothing can play until mpv is here.");
  await choose("first:tools:skip");
  expect(heading()).toBe("TMDB key");
  await choose("first:tmdb:skip");
  expect(useNav.getState().current).toEqual({ screen: "home" });
});

test("the tools step leads with the download, shows its progress, and goes on once it is done", async () => {
  let tools = [missing("mpv"), missing("ffmpeg")];
  tauri.handle("status", () => STATUS);
  tauri.handle("detect_tools", () => tools);
  tauri.handle("tools_download_status", () => ({ running: false, step: null, outcome: null }));
  tauri.handle("download_tools", () => true);
  screens({ screen: "firstRun" });
  await settle(120);
  await choose("first:folder:skip");

  const main = () => document.querySelector('[data-focus-key="first:tools:main"]')!;
  expect(activeKey()).toBe("first:tools:main");
  expect(main().textContent).toBe("Download mpv and ffmpeg");
  expect(document.body.textContent).toContain("not the mpv or FFmpeg projects");

  await select();
  // The press shows at once, before the download says anything.
  expect(main().textContent).toBe("Looking for the latest build");
  expect(tauri.calls().filter((c) => c.cmd === "download_tools")).toHaveLength(1);
  await act(() => tauri.emit("tools-download", { running: true, step: { step: "downloading", done: 34_000_000, total: 63_000_000 }, outcome: null }));
  await settle();
  expect(main().textContent).toBe("Downloading, 34 of 63 MB");
  expect(activeKey()).toBe("first:tools:main");
  // Meanwhile the owner may go on; the download carries on behind.
  expect(document.querySelector('[data-focus-key="first:tools:skip"]')!.textContent).toBe("Continue");

  tools = [
    { ...found("mpv"), source: "downloaded", release: "20261111", downloaded: true },
    { ...found("ffmpeg"), source: "downloaded", release: "20261111", downloaded: true },
  ];
  await act(() => tauri.emit("tools-download", { running: false, step: null, outcome: { kind: "done", release: "20261111", already_latest: false } }));
  await settle();
  await settle(60);
  expect(main().textContent).toBe("Continue");
  expect(activeKey()).toBe("first:tools:main");
  expect(document.body.textContent).toContain("Downloaded the build of 11 November 2026.");
  expect(document.body.textContent).toContain("Downloaded by Exhibit, version 1.0, built 11 November 2026");
  await choose("first:tools:main");
  expect(heading()).toBe("TMDB key");
});
