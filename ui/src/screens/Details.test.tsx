import { cleanup } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { activeKey, press, screens, select, settle } from "../../test/app";
import { audioTrack, detail, subTrack } from "../../test/fixtures";
import { tauri } from "../../test/setup";
import { installBridge } from "@/focus/bridge";
import type { LibraryStatus, MediaFile, TechInfo } from "@/lib/api";

const tech: TechInfo = {
  duration_s: 6840,
  width: 3840,
  height: 2160,
  video_codec: "hevc",
  hdr: "HDR10",
  audio: [audioTrack({ codec: "truehd", channels: 8 }), audioTrack({ codec: "ac3", lang: "fre" })],
  subtitles: [subTrack()],
};
const file = (id: number, source_id: number, rel_path: string): MediaFile => ({
  id,
  source_id,
  rel_path,
  size: 4_200_000_000,
  mtime: 0,
  item_id: 7,
  tech,
});

function status(online: boolean): LibraryStatus {
  return {
    movies: 1,
    shows: 0,
    other: 0,
    needs_review: 0,
    unprobed: 0,
    sources: [
      { id: 1, path: "D:\\Movies", volume_id: "a", online },
      { id: 2, path: "E:\\Films", volume_id: "b", online: true },
    ],
    tmdb_key: true,
    agent_key: false,
    intro: true,
  };
}

let uninstall: () => void = () => {};
beforeEach(() => {
  uninstall = installBridge();
  tauri.handle("play", () => null);
});
afterEach(() => {
  cleanup();
  uninstall();
  document.body.innerHTML = "";
});

test("lists each version of a work, the one on a missing drive Offline", async () => {
  tauri.handle("status", () => status(false));
  tauri.handle("item", () =>
    detail({ files: [file(1, 1, "Taxi Driver/Taxi Driver (1976).mkv"), file(2, 2, "Taxi Driver 4K.mkv")], available: true }, { id: 7 }),
  );
  screens({ screen: "details", id: 7 });
  await settle();
  const lines = [...document.querySelectorAll("[data-file]")].map((f) => f.textContent);
  expect(lines).toEqual([
    "Version 14.2 GB, 4K HDR10, English and French, subtitles English, D:\\Movies, OfflineTaxi Driver (1976).mkv",
    "Version 24.2 GB, 4K HDR10, English and French, subtitles English, E:\\FilmsTaxi Driver 4K.mkv",
  ]);
});

test("Play on a work whose drive is away never calls play, and the cues say which drive", async () => {
  tauri.handle("status", () => status(false));
  tauri.handle("item", () => detail({ files: [file(1, 1, "Taxi Driver.mkv")], available: false }, { id: 7 }));
  screens({ screen: "details", id: 7 });
  await settle();
  expect(activeKey()).toBe("action:main");
  expect(document.querySelector("[data-line=action]")?.textContent).toContain("The drive for D:\\Movies is not connected");
  await select();
  await settle();
  expect(tauri.calls().some((c) => c.cmd === "play")).toBe(false);
});

test("the heart loves the work and shows it, and the cast hangs below with who they play", async () => {
  tauri.handle("status", () => status(true));
  tauri.handle("item", () => detail({ files: [file(1, 1, "Taxi Driver.mkv")], available: true }, { id: 7 }));
  tauri.handle("cast", () => [
    { name: "Robert De Niro", character: "Travis Bickle", portrait: null },
    { name: "Jodie Foster", character: "Iris", portrait: null },
  ]);
  tauri.handle("set_loved", () => null);
  screens({ screen: "details", id: 7 });
  await settle();
  await press("ArrowRight");
  expect(activeKey()).toBe("action:love");
  expect(document.activeElement?.getAttribute("aria-pressed")).toBe("false");
  await select();
  await settle();
  expect(tauri.calls().find((c) => c.cmd === "set_loved")?.args).toEqual({ id: 7, loved: true });
  expect(document.activeElement?.getAttribute("aria-pressed")).toBe("true");
  const cast = [...document.querySelectorAll("[data-focus-group=details-cast] [data-focus-key]")];
  expect(cast.map((c) => c.getAttribute("aria-label"))).toEqual(["Robert De Niro, as Travis Bickle", "Jodie Foster, as Iris"]);
});
