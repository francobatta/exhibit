import { expect, test } from "bun:test";
import { compact, cycleGenre, describe, filterCount, span, toggled, wallQuery } from "./filters";

test("only what is set reaches the query; a sort in its own direction says nothing", () => {
  const view = { sort: "title" as const, descending: false, filters: { genres: [], any_genre: true, loved: null, seen: ["watched" as const] } };
  expect(wallQuery("movie", view)).toEqual({ kind: "movie", seen: ["watched"] });
  expect(wallQuery("movie", { ...view, sort: "year", descending: true })).toMatchObject({ sort: "year" });
  expect(wallQuery("movie", { ...view, sort: "year", descending: false })).toMatchObject({ sort: "year", descending: false });
  expect(wallQuery("other", { ...view, sort: "year", descending: false }, false)).toEqual({ kind: "other", seen: ["watched"] });
  expect(compact({ genres: ["Drama"], any_genre: false, year_min: null })).toEqual({ genres: ["Drama"] });
});

test("a genre is wanted, then left out, then neither", () => {
  let f = cycleGenre({}, "Horror");
  expect(f).toEqual({ genres: ["Horror"], without_genres: [] });
  f = cycleGenre(f, "Horror");
  expect(f).toEqual({ genres: [], without_genres: ["Horror"] });
  expect(cycleGenre(f, "Horror")).toEqual({ genres: [], without_genres: [] });
  expect(toggled(["a"], "b")).toEqual(["a", "b"]);
  expect(toggled(["a", "b"], "a")).toEqual(["b"]);
});

test("filters read as a line, each kind counted once", () => {
  const f = {
    genres: ["Drama", "Crime"],
    without_genres: ["Horror"],
    year_min: 1970,
    year_max: 1989,
    runtime_max: 120,
    seen: ["unwatched" as const, "started" as const],
    resolution: ["uhd" as const],
    sound: ["atmos" as const, "lossless" as const],
    audio: ["es"],
    subtitles: ["en", "fr"],
  };
  expect(describe(f)).toEqual([
    "Drama + Crime",
    "not Horror",
    "1970 – 1989",
    "Up to 2 h",
    "Not watched or In progress",
    "4K",
    "Dolby Atmos or Lossless",
    "Spanish audio",
    "English or French subtitles",
  ]);
  expect(describe({ genres: ["Drama", "Crime"], any_genre: true })).toEqual(["Drama or Crime"]);
  expect(filterCount(f)).toBe(8);
  expect(filterCount({ genres: [], loved: null })).toBe(0);
  expect(span(1970, null, String)).toBe("From 1970");
  expect(span(null, null, String)).toBeNull();
});
