import { beforeEach, describe, expect, test } from "bun:test";
import { tauri } from "../../test/setup";
import type { ApiError, ErrorKind } from "./api";

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
  const error = (kind: ErrorKind, code: string | null, params: Record<string, string> = {}): ApiError => ({
    kind,
    code,
    params,
    message: `${kind}: the core's English`,
  });

  test("words an error from its code and parameters, never from the core's English", async () => {
    const api = await fresh();
    expect(api.plain(error("source_offline", "source.offline", { path: "D:\\Films" }))).toBe(
      "The drive for D:\\Films is not connected.",
    );
    expect(api.plain(error("invalid", "tmdb.key_rejected"))).toBe("TMDB rejected the key. Check it in Settings.");
  });

  test("an error without a known code is worded by its kind", async () => {
    const api = await fresh();
    expect(api.plain(error("db", null))).toBe("The library could not be read. Restart Exhibit if it keeps happening.");
    expect(api.plain(error("io", "no.such.code"))).toBe("Exhibit could not read or write a file.");
  });

  test("anything else a promise rejected with is something that went wrong", async () => {
    const api = await fresh();
    expect(api.plain(new Error("boom"))).toBe("Something went wrong.");
    expect(api.plain("io: the folder could not be read")).toBe("Something went wrong.");
  });

  test("a rejected key is known by its code", async () => {
    const api = await fresh();
    expect(api.keyRejected(error("invalid", "tmdb.key_rejected"))).toBe(true);
    expect(api.keyRejected(error("invalid", "tmdb.no_key"))).toBe(false);
  });
});
