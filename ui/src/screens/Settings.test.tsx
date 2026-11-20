import { act, cleanup } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { screens, select, settle } from "../../test/app";
import { tauri } from "../../test/setup";
import { useBackdrop } from "@/components/Backdrop";
import { focusItem, installBridge } from "@/focus/bridge";
import type { LibraryStatus, ToolStatus } from "@/lib/api";
import { useNav } from "@/nav/stack";
import { keyError } from "./Settings";
import { t } from "@/i18n";

let uninstall: () => void = () => {};

const MPV: ToolStatus = { tool: "mpv", path: "C:\\mpv\\mpv.exe", version: "mpv v0.40.0", source: "detected", release: null, downloaded: false };
const missing = (tool: ToolStatus["tool"]): ToolStatus => ({ tool, path: null, version: null, source: null, release: null, downloaded: false });

function status(over: Partial<LibraryStatus> = {}): LibraryStatus {
  return {
    movies: 0,
    shows: 0,
    other: 0,
    needs_review: 0,
    unprobed: 0,
    sources: [{ id: 4, path: "D:\\Films", volume_id: "a", online: true }],
    tmdb_key: false,
    agent_key: false,
    intro: true,
    language: "en-US",
    date_format: "auto",
    ...over,
  };
}

beforeEach(() => {
  uninstall = installBridge();
  tauri.handle("status", () => status());
  tauri.handle("detect_tools", () => [MPV, missing("ffmpeg")]);
  tauri.handle("window_is_fullscreen", () => false);
  tauri.handle("tools_download_status", () => ({ running: false, step: null, outcome: null }));
});
afterEach(() => {
  cleanup();
  uninstall();
  document.body.innerHTML = "";
});

const calls = (cmd: string) => tauri.calls().filter((c) => c.cmd === cmd).map((c) => c.args);
const button = (within: ParentNode, label: string) => [...within.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === label);

async function focusAndSelect(key: string) {
  await act(async () => {
    focusItem(key);
  });
  await select();
  await settle();
}

test("removing a folder asks first, and only Remove removes it", async () => {
  tauri.handle("remove_source", () => null);
  screens({ screen: "settings" });
  await settle(120);

  await focusAndSelect("source:4:remove");
  const dialog = document.querySelector('[data-focus-scope="confirm"]')!;
  expect(dialog.textContent).toContain("Remove this folder from Exhibit?");
  expect(dialog.textContent).toContain("Your files are not touched.");
  expect(calls("remove_source")).toEqual([]);

  await act(async () => {
    button(dialog, "Remove")!.click();
  });
  await settle();
  expect(calls("remove_source")).toEqual([{ id: 4 }]);
});

test("a missing tool says what it would supply", async () => {
  screens({ screen: "settings" });
  await settle(120);
  const tools = document.querySelector('section[aria-label="Tools"]')!.textContent!;
  expect(tools).toContain("Found on this PC, version 0.40.0");
  expect(tools).toContain("C:\\mpv\\mpv.exe");
  expect(tools).toContain("ffmpeg makes thumbnails for Other; mpv is used instead.");
  expect(tools).not.toContain("ffprobe");
  expect(calls("detect_tools").length).toBeGreaterThan(0);
});

test("opened for a missing mpv, Settings lands on the download, and a failure says what to do", async () => {
  tauri.handle("detect_tools", () => [missing("mpv"), missing("ffmpeg")]);
  tauri.handle("download_tools", () => true);
  screens({ screen: "settings", part: "tools" });
  await settle(120);
  const download = document.querySelector('[data-focus-key="tools:download"]')!;
  expect(document.activeElement).toBe(download);
  expect(download.textContent).toBe("Download mpv and ffmpeg");

  await select();
  await act(() =>
    tauri.emit("tools-download", {
      running: false,
      step: null,
      outcome: { kind: "failed", error: { kind: "provider", code: "download.unreachable", params: {}, message: "provider error: could not reach GitHub" } },
    }),
  );
  await settle();
  expect(document.querySelector('section[aria-label="Tools"] [role="alert"]')!.textContent).toBe(
    "Could not reach GitHub. Check the internet connection, then try again.",
  );
  expect(document.activeElement).toBe(download);
});

test("a path set by hand hands back to the download", async () => {
  tauri.handle("detect_tools", () => [
    { ...MPV, source: "manual", downloaded: true },
    { ...missing("ffmpeg"), path: "C:\\x\\ffmpeg.exe", version: "ffmpeg version 8.0", source: "downloaded", release: "20261111", downloaded: true },
  ]);
  tauri.handle("set_tool_path", () => null);
  screens({ screen: "settings" });
  await settle(120);
  const tools = document.querySelector('section[aria-label="Tools"]')!;
  expect(tools.textContent).toContain("Set by you, version 0.40.0");
  expect(tools.textContent).toContain("Downloaded by Exhibit, version 8.0, built November 11, 2026");
  expect(button(tools, "Get the latest")).toBeDefined();
  await focusAndSelect("tool:mpv:clear");
  expect(calls("set_tool_path")).toEqual([{ tool: "mpv", path: null }]);
});

test("saving a TMDB key stores it, starts identifying, and clears the field", async () => {
  tauri.handle("set_secret", () => null);
  tauri.handle("identify", () => null);
  screens({ screen: "settings", part: "tmdb" });
  await settle(120);
  const field = document.querySelector<HTMLInputElement>('[data-focus-key="tmdb:field"]')!;
  expect(document.activeElement).toBe(field);

  await act(async () => {
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    set.call(field, "abc123");
    field.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await focusAndSelect("tmdb:save");

  const order = tauri.calls().filter((c) => c.cmd === "set_secret" || c.cmd === "identify");
  expect(order).toEqual([
    { cmd: "set_secret", args: { name: "tmdb", value: "abc123" } },
    { cmd: "identify", args: { scope: "unmatched_and_review" } },
  ]);
  expect(field.value).toBe("");
});

test("a rejected key says what to do here, not on the command line", () => {
  const rejected = { kind: "invalid" as const, code: "tmdb.key_rejected", params: {}, message: "invalid: …" };
  expect(keyError(rejected, t)).toBe("TMDB rejected the key. Check the key, then save it again.");
});

test("the intro switch turns the intro off for the next launch", async () => {
  tauri.handle("set_intro", () => null);
  screens({ screen: "settings" });
  await settle(120);
  expect(document.querySelector('[data-focus-key="display:intro"]')?.getAttribute("aria-checked")).toBe("true");

  await focusAndSelect("display:intro");
  expect(calls("set_intro")).toEqual([{ on: false }]);
});

test("Settings stands on the bare wall, and Back gives the screen beneath its backdrop again", async () => {
  tauri.handle("home", () => ({ rows: [] }));
  screens({ screen: "home" });
  act(() => useBackdrop.getState().show("film-backdrop", "film-poster"));
  await act(async () => useNav.getState().push({ screen: "settings" }));
  await settle(120);
  expect(useBackdrop.getState()).toMatchObject({ image: null, poster: null });

  await act(async () => void useNav.getState().back());
  await settle(120);
  expect(useBackdrop.getState()).toMatchObject({ image: "film-backdrop", poster: "film-poster" });
});

// Services

const STATE = { service: "letterboxd", enabled: true, username: null as string | null, export_name: null, export_at: null as number | null, synced_at: null, error: null as string | null, unmatched: 0 };
const service = (state: Partial<typeof STATE> = {}) => ({
  id: "letterboxd",
  name: "Letterboxd",
  markSvg: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"/>',
  provides: ["activity"],
  needs: ["username", "export_file"],
  exportPage: "https://example.test/export",
  state: { ...STATE, ...state },
});
const typeInto = (key: string, text: string) =>
  act(async () => {
    const field = document.querySelector<HTMLInputElement>(`[data-focus-key="${key}"]`)!;
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(field, text);
    field.dispatchEvent(new Event("input", { bubbles: true }));
  });
const servicesPart = () => document.querySelector('section[aria-label="Services"]');

test("services_part_shows_each_service_from_the_registry", async () => {
  tauri.handle("services", () => [service({ username: "ana" })]);
  screens({ screen: "settings" });
  await settle(120);
  const part = servicesPart()!;
  expect(part.textContent).toContain("Letterboxd");
  expect(part.querySelector('[data-focus-key="service:letterboxd:enabled"]')?.getAttribute("aria-checked")).toBe("true");
  expect(button(part, "Bring in your whole history")).toBeDefined();
});

test("with_no_services_registered_the_part_is_absent", async () => {
  tauri.handle("services", () => []);
  screens({ screen: "settings" });
  await settle(120);
  expect(servicesPart()).toBeNull();
  expect(document.querySelector('section[aria-label="TMDB key"]')).not.toBeNull();
});

test("saving_a_username_says_what_was_found", async () => {
  tauri.handle("services", () => [service()]);
  tauri.handle("connect_service", () => 12);
  screens({ screen: "settings" });
  await settle(120);
  await focusAndSelect("service:letterboxd:username");
  await typeInto("service:letterboxd:username", "ana");
  await focusAndSelect("service:letterboxd:connect");
  expect(calls("connect_service")).toEqual([{ id: "letterboxd", username: "ana" }]);
  expect(servicesPart()!.textContent).toContain("Found @ana: 12 recent diary entries.");
});

test("a_found_export_is_offered_and_imported", async () => {
  tauri.handle("services", () => [service({ username: "ana" })]);
  tauri.handle("find_service_export", () => ({ path: "C:\Downloads\export.zip", name: "export.zip", modified: 1 }));
  tauri.handle("import_service_export", () => null);
  tauri.handle("open_export_page", () => null);
  screens({ screen: "settings" });
  await settle(120);
  expect(calls("find_service_export")).toEqual([]);
  await focusAndSelect("service:letterboxd:history");
  await settle(120);
  await focusAndSelect("service:letterboxd:open");
  expect(calls("open_export_page")).toEqual([{ id: "letterboxd" }]);
  expect(servicesPart()!.textContent).toContain("Your Letterboxd export from today is in Downloads.");
  await focusAndSelect("service:letterboxd:bring");
  expect(calls("import_service_export")).toEqual([{ id: "letterboxd", path: "C:\Downloads\export.zip" }]);
  expect(document.querySelector('[data-focus-key="service:letterboxd:bring"]')).toBeNull();
});

test("a_service_error_is_worded_from_its_code", async () => {
  tauri.handle("services", () => [service({ username: "ana", error: "service.unreachable" })]);
  screens({ screen: "settings" });
  await settle(120);
  expect(servicesPart()!.textContent).toContain("Exhibit could not reach the service.");
});

test("forgetting_asks_first", async () => {
  tauri.handle("services", () => [service({ username: "ana" })]);
  tauri.handle("forget_service", () => null);
  screens({ screen: "settings" });
  await settle(120);
  await focusAndSelect("service:letterboxd:forget");
  const dialog = document.querySelector('[data-focus-scope="confirm"]')!;
  expect(dialog.textContent).toContain("in Exhibit only");
  expect(calls("forget_service")).toEqual([]);
  await act(async () => {
    button(dialog, "Forget")!.click();
  });
  await settle();
  expect(calls("forget_service")).toEqual([{ id: "letterboxd" }]);
});
