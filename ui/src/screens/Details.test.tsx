import { cleanup } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { activeKey, screens, select, settle } from "../../test/app";
import { detail } from "../../test/fixtures";
import { tauri } from "../../test/setup";
import { installBridge } from "@/focus/bridge";
import type { LibraryStatus, MediaFile, TechInfo } from "@/lib/api";

const tech: TechInfo = {
  duration_s: 6840,
  width: 3840,
  height: 2160,
  video_codec: "hevc",
  hdr: "HDR10",
  audio: [{ codec: "truehd", channels: 8, lang: "eng" }, { codec: "ac3", channels: 6, lang: "fre" }],
  subtitles: [{ codec: "subrip", lang: "eng" }],
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
    tools: [],
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

test("lists one line per file and marks the one on a missing drive Offline", async () => {
  tauri.handle("status", () => status(false));
  tauri.handle("item", () =>
    detail({ files: [file(1, 1, "Taxi Driver/Taxi Driver (1976).mkv"), file(2, 2, "Taxi Driver 4K.mkv")], available: true }, { id: 7 }),
  );
  screens({ screen: "details", id: 7 });
  await settle();
  const lines = [...document.querySelectorAll("[data-file]")].map((f) => f.textContent);
  expect(lines).toEqual([
    "Taxi Driver (1976).mkv4.2 GB, 4K HDR10, English and French, subtitles English, D:\\Movies, Offline",
    "Taxi Driver 4K.mkv4.2 GB, 4K HDR10, English and French, subtitles English, E:\\Films",
  ]);
});

test("Play on a work whose drive is away never calls play, and says which drive", async () => {
  tauri.handle("status", () => status(false));
  tauri.handle("item", () => detail({ files: [file(1, 1, "Taxi Driver.mkv")], available: false }, { id: 7 }));
  screens({ screen: "details", id: 7 });
  await settle();
  expect(activeKey()).toBe("action:main");
  expect(document.activeElement?.textContent).toContain("The drive for D:\\Movies is not connected");
  await select();
  await settle();
  expect(tauri.calls().some((c) => c.cmd === "play")).toBe(false);
});
