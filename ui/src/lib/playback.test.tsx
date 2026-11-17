import { act, cleanup } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { activeKey, screens, select, settle } from "../../test/app";
import { detail } from "../../test/fixtures";
import { tauri } from "../../test/setup";
import { SHOW, fakeShow } from "../../test/show";
import { toast } from "@/components/ui/toast";
import { installBridge } from "@/focus/bridge";
import type { PlayEnded } from "./api";
import { usePlayback } from "./playback";
import { useNav } from "@/nav/stack";

let uninstall: () => void = () => {};
beforeEach(() => {
  uninstall = installBridge();
  tauri.handle("play", () => null);
});
afterEach(() => {
  cleanup();
  uninstall();
  toast.dismiss();
  usePlayback.setState({ playing: null, follow: null, next: null, failure: null, problem: null, problemOpen: false });
  document.body.innerHTML = "";
});

const ended = (p: Partial<PlayEnded>) =>
  act(async () => {
    await tauri.emit("play-ended", { outcome: null, error: null, nextEpisode: null, ...p });
  });

function film() {
  tauri.handle("status", () => ({ movies: 1, shows: 0, other: 0, needs_review: 0, unprobed: 0, sources: [], tmdb_key: true, agent_key: false, tools: [] }));
  tauri.handle("item", () => detail({}, { id: 7 }));
}

test("a missing player says so in a dialog, names the path tried, and offers Settings to the pad", async () => {
  film();
  tauri.handle("play", () => {
    throw { kind: "tool", message: "tool error: mpv not found at C:\\mpv\\mpv.exe" };
  });
  screens({ screen: "details", id: 7 });
  await settle();
  await select();
  await settle(200);
  expect(usePlayback.getState().playing).toBeNull();
  const dialog = document.querySelector('[role="dialog"]');
  expect(dialog?.textContent).toContain("mpv could not start");
  expect(dialog?.textContent).toContain("mpv not found at C:\\mpv\\mpv.exe");
  // Its first button has focus, a focus item like any other.
  expect(activeKey()).toBe("player-problem:0");
  expect(document.activeElement?.textContent).toBe("Open Settings");
  await select();
  await settle(200);
  expect(useNav.getState().current).toEqual({ screen: "settings" });
});

test("an episode that ends watched moves focus to the next one on its show, and nowhere else", async () => {
  fakeShow(12);
  screens({ screen: "show", id: SHOW });
  await settle(120);
  expect(activeKey()).toBe("episode:12");
  await select();
  expect(tauri.calls().filter((c) => c.cmd === "play").map((c) => c.args)).toEqual([{ id: 12, resume: false }]);
  expect(document.body.textContent).toContain("Playing in mpv");
  await ended({ nextEpisode: 21 });
  await settle(200);
  expect(activeKey()).toBe("episode:21");

  // On a film's details, the same event leaves focus where it was.
  cleanup();
  film();
  screens({ screen: "details", id: 7 });
  await settle();
  await select();
  await ended({ nextEpisode: 21 });
  await settle(200);
  expect(activeKey()).toBe("action:main");
});
