import { act, cleanup } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { activeKey, screens, select, settle } from "../../test/app";
import { awardsService, scoresService } from "../../test/fixtures";
import { tauri } from "../../test/setup";
import { focusItem, installBridge } from "@/focus/bridge";
import type { LibraryStatus, ToolStatus } from "@/lib/api";
import { openAsk, useAskOverlay, useNav } from "@/nav/stack";

let uninstall: () => void = () => {};

const found = (tool: ToolStatus["tool"]): ToolStatus => ({ tool, path: `C:\\tools\\${tool}.exe`, version: `${tool} 1.0`, source: "detected", release: null, downloaded: false });
const missing = (tool: ToolStatus["tool"]): ToolStatus => ({ tool, path: null, version: null, source: null, release: null, downloaded: false });

const STATUS: LibraryStatus = { movies: 0, shows: 0, other: 0, needs_review: 0, unprobed: 0, sources: [], tmdb_key: false, tmdb_own_key: false, tmdb_builtin: false, agent_key: false, intro: true, language: "en-US", date_format: "auto" };

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
  expect(document.body.textContent).toContain("Downloaded the build of November 11, 2026.");
  expect(document.body.textContent).toContain("Downloaded by Exhibit, version 1.0, built November 11, 2026");
  await choose("first:tools:main");
  expect(heading()).toBe("TMDB key");
});

const BUILT_IN: LibraryStatus = { ...STATUS, tmdb_key: true, tmdb_builtin: true };

test("with exhibits own tmdb key there is no key step, and signing in for scores ends at Home", async () => {
  let approved = false;
  tauri.handle("status", () => BUILT_IN);
  tauri.handle("detect_tools", () => [found("mpv"), found("ffmpeg")]);
  tauri.handle("services", () => [scoresService({ username: approved ? "someone" : null })]);
  tauri.handle("start_sign_in", () => ({
    userCode: "7EFVV31O",
    verifyUrl: "https://example.test/device/",
    qrSvg: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 21 21"><path d="M0 0h1v1H0z"/></svg>',
    interval: 0.05,
    expiresIn: 1800,
  }));
  tauri.handle("poll_sign_in", () => (approved ? "done" : "pending"));
  screens({ screen: "firstRun" });
  await settle(120);
  expect(document.body.textContent).toContain("Step 1 of 2");

  await choose("first:folder:skip");
  expect(heading()).toBe("What critics and audiences think");
  expect(activeKey()).toBe("first:scores:sign-in");
  expect(document.body.textContent).toContain("Sign in to Scores to see scores from IMDb, Tomatometer, Metacritic, and Letterboxd");

  await choose("first:scores:sign-in");
  await settle(60);
  const panel = document.querySelector("[data-sign-in]")!;
  expect(panel.querySelector("[data-user-code]")?.textContent).toBe("7EFVV31O");
  expect(panel.querySelector("svg")).not.toBeNull();
  // The button gave way to the code; focus is on Cancel, never lost.
  expect(activeKey()).toBe("first:scores:sign-in:cancel");
  approved = true;
  await settle(300);
  expect(useNav.getState().current).toEqual({ screen: "home" });
});

test("a service already signed in, or none to sign in to, adds no step", async () => {
  tauri.handle("status", () => BUILT_IN);
  tauri.handle("detect_tools", () => [found("mpv"), found("ffmpeg")]);
  tauri.handle("services", () => [scoresService(), awardsService()]);
  screens({ screen: "firstRun" });
  await settle(120);
  expect(document.body.textContent).toContain("Step 1 of 1");
  await choose("first:folder:skip");
  expect(useNav.getState().current).toEqual({ screen: "home" });
});

test("the scores step can be skipped", async () => {
  tauri.handle("status", () => STATUS);
  tauri.handle("detect_tools", () => [found("mpv"), found("ffmpeg")]);
  tauri.handle("services", () => [scoresService({ username: null })]);
  screens({ screen: "firstRun" });
  await settle(120);
  expect(document.body.textContent).toContain("Step 1 of 3");
  await choose("first:folder:skip");
  expect(heading()).toBe("TMDB key");
  await choose("first:tmdb:skip");
  expect(heading()).toBe("What critics and audiences think");
  await choose("first:scores:skip");
  expect(useNav.getState().current).toEqual({ screen: "home" });
  expect(tauri.calls().some((c) => c.cmd === "start_sign_in")).toBe(false);
});
