import { beforeEach, describe, expect, test } from "bun:test";
import { tauri } from "../../test/setup";

// A fresh copy of the module per test, so no test depends on another having called `boot`.
let copies = 0;
const fresh = () => import(`./api.ts?copy=${copies++}`) as Promise<typeof import("./api")>;

beforeEach(() => {
  tauri.handle("boot", () => ({ status: {}, imageDir: "C:\\Data\\cache\\images", firstRun: true }));
});

describe("imageUrl", () => {
  test("is null before boot and for a missing name", async () => {
    const api = await fresh();
    expect(api.imageUrl("movie-603-poster.jpg")).toBeNull();
    await api.boot();
    expect(api.imageUrl(null)).toBeNull();
  });

  test("points into the image folder, small posters beside large ones", async () => {
    const api = await fresh();
    await api.boot();
    const url = (p: string) => `http://asset.localhost/${encodeURIComponent(p)}`;
    expect(api.imageUrl("movie-603-poster.jpg")).toBe(
      url("C:\\Data\\cache\\images\\movie-603-poster.jpg"),
    );
    expect(api.imageUrl("movie-603-poster.jpg", "s")).toBe(
      url("C:\\Data\\cache\\images\\movie-603-poster-s.jpg"),
    );
    expect(api.imageUrl("movie-603-backdrop.jpg", "s")).toBe(
      url("C:\\Data\\cache\\images\\movie-603-backdrop.jpg"),
    );
  });
});

test("commands send camelCase arguments under their snake_case names", async () => {
  const api = await fresh();
  for (const cmd of ["apply_match", "set_watched", "identify"]) tauri.handle(cmd, () => null);
  await api.applyMatch(4, 603, "movie");
  await api.setWatched(4, true);
  await api.identify({ item: 4 });
  expect(tauri.calls()).toEqual([
    { cmd: "apply_match", args: { itemId: 4, tmdbId: 603, kind: "movie" } },
    { cmd: "set_watched", args: { id: 4, watched: true } },
    { cmd: "identify", args: { scope: { item: 4 } } },
  ]);
});

describe("plain", () => {
  test("drops the core's kind prefix and starts with a capital", async () => {
    const api = await fresh();
    expect(api.plain({ kind: "invalid", message: "invalid: TMDB key not set" })).toBe(
      "TMDB key not set",
    );
    expect(api.plain({ kind: "source_offline", message: "source offline: D:\\ is not connected" })).toBe(
      "D:\\ is not connected",
    );
    expect(api.plain({ kind: "tool", message: "tool error: mpv not found at C:\\mpv\\mpv.exe" })).toBe(
      "mpv not found at C:\\mpv\\mpv.exe",
    );
    expect(api.plain("io: the folder could not be read")).toBe("The folder could not be read");
    expect(api.plain({ kind: "db", message: "" })).toBe("Something went wrong.");
  });
});
