import { expect, test } from "bun:test";
import { compact, cycleGenre, describe, filterCount, span, toggled, wallQuery, withoutActivity } from "./filters";

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

test("activity_filters_count_each_and_read_in_the_summary", () => {
  const f = { rated: true, reviewed: true, logged: null, watchlist: true, loved: true };
  expect(filterCount(f)).toBe(4);
  expect(describe(f)).toEqual(["Rated", "Reviewed", "On your watchlist", "Loved"]);
  expect(wallQuery("movie", { sort: "my_rating", descending: true, filters: { watchlist: true } })).toEqual({
    kind: "movie",
    watchlist: true,
    sort: "my_rating",
  });
});

test("turning_off_drops_hidden_filters_from_the_view", () => {
  const view = { sort: "my_rating" as const, descending: false, filters: { watchlist: true, rated: true, reviewed: null, loved: true } };
  // The sort goes back to the kind's own; filters a service gave go; the rest stay.
  expect(withoutActivity(view)).toEqual({ sort: null, descending: false, filters: { loved: true } });
  expect(withoutActivity({ sort: "year", descending: true, filters: { logged: true } })).toEqual({ sort: "year", descending: true, filters: {} });
  // Nothing a service gave: nothing to drop.
  expect(withoutActivity({ sort: "year", descending: true, filters: { loved: true, rated: null } })).toBeNull();
});
