import { act, cleanup } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { screens, select, settle } from "../../test/app";
import { tauri } from "../../test/setup";
import { focusItem, installBridge } from "@/focus/bridge";
import type { LibraryStatus, ToolStatus } from "@/lib/api";
import { keyError } from "./Settings";

let uninstall: () => void = () => {};

const MPV: ToolStatus = { tool: "mpv", path: "C:\\mpv\\mpv.exe", version: "mpv v0.40.0", manual: false };
const missing = (tool: ToolStatus["tool"]): ToolStatus => ({ tool, path: null, version: null, manual: false });

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
    ...over,
  };
}

beforeEach(() => {
  uninstall = installBridge();
  tauri.handle("status", () => status());
  tauri.handle("detect_tools", () => [MPV, missing("ffprobe"), missing("ffmpeg")]);
  tauri.handle("window_is_fullscreen", () => false);
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
  expect(tools).toContain("Found at C:\\mpv\\mpv.exe, version 0.40.0");
  expect(tools).toContain("ffprobe reads resolution, HDR and tracks faster; mpv is used instead.");
  expect(tools).toContain("ffmpeg makes thumbnails for Other; mpv is used instead.");
  expect(calls("detect_tools").length).toBeGreaterThan(0);
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
  expect(keyError("invalid: TMDB rejected the key; set a valid one with `exhibit key set tmdb`")).toBe(
    "TMDB rejected the key. Check the key, then save it again.",
  );
});
