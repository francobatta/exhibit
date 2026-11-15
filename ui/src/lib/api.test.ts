import { beforeAll, describe, expect, test } from "bun:test";

const calls: Array<{ cmd: string; args: unknown }> = [];

beforeAll(() => {
  // A stand-in for the Tauri page bridge: records commands, answers `boot`.
  const g = globalThis as unknown as Record<string, unknown>;
  g.window = globalThis;
  g.__TAURI_INTERNALS__ = {
    invoke: async (cmd: string, args: unknown) => {
      calls.push({ cmd, args });
      if (cmd === "boot") {
        return { status: {}, imageDir: "C:\\Data\\cache\\images", firstRun: true };
      }
      return null;
    },
    convertFileSrc: (path: string, protocol = "asset") =>
      `http://${protocol}.localhost/${encodeURIComponent(path)}`,
  };
});

describe("imageUrl", () => {
  test("is null before boot and for a missing name", async () => {
    const api = await import("./api");
    expect(api.imageUrl("movie-603-poster.jpg")).toBeNull();
    await api.boot();
    expect(api.imageUrl(null)).toBeNull();
  });

  test("points into the image folder, small posters beside large ones", async () => {
    const api = await import("./api");
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
  const api = await import("./api");
  calls.length = 0;
  await api.applyMatch(4, 603, "movie");
  await api.setWatched(4, true);
  await api.identify({ item: 4 });
  expect(calls).toEqual([
    { cmd: "apply_match", args: { itemId: 4, tmdbId: 603, kind: "movie" } },
    { cmd: "set_watched", args: { id: 4, watched: true } },
    { cmd: "identify", args: { scope: { item: 4 } } },
  ]);
});
