import { cleanup } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { activeKey, press, screens, select, settle, until } from "../../test/app";
import { activity, audioTrack, award, awardsService, detail, score, scoresService, service, subTrack } from "../../test/fixtures";
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
    tmdb_own_key: true,
    tmdb_builtin: false,
    agent_key: false,
    intro: true,
    language: "en-US",
    date_format: "auto",
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

const MARCH_3_2019 = Date.UTC(2019, 2, 3) / 1000;
const sectionNames = () => [...document.querySelectorAll("section[aria-label]")].map((s) => s.getAttribute("aria-label"));

test("details_shows_the_review_and_hides_a_spoiler_until_pressed", async () => {
  tauri.handle("status", () => status(true));
  tauri.handle("services", () => [service()]);
  tauri.handle("item", () =>
    detail(
      {
        files: [file(1, 1, "Taxi Driver.mkv")],
        activity: activity({
          ratings: [{ service: "letterboxd", on: MARCH_3_2019, value: 4.5, scale: 5 }],
          reviews: [{ service: "letterboxd", on: MARCH_3_2019, text: "He drives on into the night.", spoiler: true }],
        }),
      },
      { id: 7 },
    ),
  );
  tauri.handle("cast", () => [{ name: "Robert De Niro", character: "Travis Bickle", portrait: null }]);
  screens({ screen: "details", id: 7 });
  await settle();
  // The review comes first in the second movement, before the cast.
  const names = sectionNames();
  expect(names.indexOf("Your review")).toBeGreaterThan(-1);
  expect(names.indexOf("Your review")).toBeLessThan(names.indexOf("Cast"));
  const review = document.querySelector("section[aria-label='Your review']")!;
  expect(review.querySelector("[aria-label='Rated 4.5 out of 5']")?.textContent).toBe("★★★★½");
  expect(review.textContent).toContain("March 3, 2019");
  // Folded: the text is not on the page until the fold is pressed.
  expect(document.body.textContent).not.toContain("He drives on into the night.");
  expect(review.textContent).toContain("Contains spoilers.");

  // Down from the cues reaches the fold first, then the cast; Up comes back the same way.
  await press("ArrowDown");
  await settle();
  expect(activeKey()).toBe("review:fold");
  await select();
  await settle();
  expect(review.textContent).toContain("He drives on into the night.");
  expect(activeKey()).toBe("review:fold");
  await press("ArrowDown");
  expect(activeKey()).toBe("cast:Robert De Niro");
  await press("ArrowUp");
  expect(activeKey()).toBe("review:fold");
  await press("ArrowUp");
  await settle();
  expect(activeKey()).toBe("action:main");
});

test("history_lists_every_watch_with_its_source", async () => {
  const year = new Date().getFullYear();
  const played = new Date(year, 2, 12, 21, 30).getTime() / 1000;
  tauri.handle("status", () => status(true));
  tauri.handle("services", () => [service()]);
  tauri.handle("item", () =>
    detail(
      {
        files: [file(1, 1, "Taxi Driver.mkv")],
        play: { item_id: 7, position_s: 0, duration_s: 6840, watched: true, last_played_at: played },
        activity: activity({
          watched: true,
          watches: [
            { service: "exhibit", on: played, rewatch: false },
            { service: "letterboxd", on: MARCH_3_2019, rewatch: true },
          ],
        }),
      },
      { id: 7, added_at: played },
    ),
  );
  screens({ screen: "details", id: 7 });
  await settle();
  expect(sectionNames()).toContain("History");
  expect(sectionNames()).not.toContain("In Exhibit");
  const history = document.querySelector("section[aria-label='History']")!;
  const watches = [...history.querySelectorAll("[data-watch]")];
  expect(watches.map((w) => w.textContent)).toEqual(["Played in Exhibit March 12", "Logged on Letterboxd March 3, 2019, rewatch"]);
  // Each service's line carries its mark; Exhibit's own has none.
  await until(() => watches[1]!.querySelector("[data-service=letterboxd]") !== null);
  expect(watches[1]!.querySelector("[data-service=letterboxd]")).not.toBeNull();
  expect(watches[0]!.querySelector("[data-service]")).toBeNull();
  // Added and Match as before; no override note while the owner has not chosen otherwise.
  expect(history.textContent).toContain("Added");
  expect(history.textContent).toContain("Matched automatically");
  expect(history.textContent).not.toContain("You marked it unwatched");
});

test("history says when the owner's choice overrides a service's watch", async () => {
  tauri.handle("status", () => status(true));
  tauri.handle("services", () => [service()]);
  tauri.handle("item", () =>
    detail(
      {
        files: [file(1, 1, "Taxi Driver.mkv")],
        activity: activity({
          watched: false,
          watched_choice: false,
          watches: Array.from({ length: 7 }, (_, i) => ({ service: "letterboxd", on: MARCH_3_2019 - i * 86_400, rewatch: i < 6 })),
        }),
      },
      { id: 7 },
    ),
  );
  screens({ screen: "details", id: 7 });
  await settle();
  const history = document.querySelector("section[aria-label='History']")!;
  // At most five watches, then how many more.
  expect(history.querySelectorAll("[data-watch]").length).toBe(5);
  expect(history.textContent).toContain("and 2 more");
  expect(history.textContent).toContain("You marked it unwatched here; Letterboxd has it logged.");
});

test("a_film_not_in_the_library_has_no_play_cue", async () => {
  tauri.handle("status", () => status(true));
  tauri.handle("services", () => [service()]);
  tauri.handle("item", () => detail({ files: [], available: false }, { id: 7 }));
  screens({ screen: "details", id: 7 });
  await settle();
  expect(document.querySelector("[data-focus-key='action:main']")).toBeNull();
  expect(document.querySelector("[data-focus-key='action:love']")).not.toBeNull();
  expect(document.querySelector("[data-focus-key='action:watched']")).not.toBeNull();
  expect(document.querySelector("[data-line=facts]")?.textContent).toBe("Not in your library");
  // Nothing beside the row speaks for a play that is not there.
  expect(document.querySelector("[data-line=action]")?.textContent).not.toContain("not connected");
});

test("laurels_crown_the_title_and_reception_sits_beside_the_file", async () => {
  tauri.handle("status", () => status(true));
  tauri.handle("services", () => [scoresService(), awardsService()]);
  tauri.handle("item", () =>
    detail(
      {
        files: [file(1, 1, "Taxi Driver.mkv")],
        reception: [
          score("tomatometer", 89, { votes: 163, url: "https://www.rottentomatoes.com/m/taxi_driver", certified: true }),
          score("imdb", 8.2, { votes: 1_049_339 }),
        ],
        awards: [award("cannes", "Palme d'Or", 1976, true, 10), award("academy", "Best Actor", 1977, false, 2)],
      },
      { id: 7, title: "Taxi Driver" },
    ),
  );
  screens({ screen: "details", id: 7 });
  await settle();
  // One wreath, above the title, for the one win.
  const laurels = document.querySelector("[data-line=laurels]")!;
  expect([...laurels.querySelectorAll("[data-laurel]")].map((l) => l.textContent)).toEqual(["Palme d'OrCannes 1976"]);
  const title = document.querySelector("[data-line=title]")!;
  expect(laurels.compareDocumentPosition(title) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  // Reception, between the file and the history, scores in the origins' order, then the awards by year.
  expect(sectionNames().filter((n) => n !== null && ["The file", "Reception", "History"].includes(n))).toEqual(["The file", "Reception", "History"]);
  const reception = document.querySelector("section[aria-label='Reception']")!;
  expect([...reception.querySelectorAll("[data-score]")].map((s) => s.getAttribute("data-score"))).toEqual(["imdb", "tomatometer"]);
  expect(reception.textContent).toContain("1M votes");
  expect(reception.textContent).toContain("Certified");
  expect(reception.textContent).toContain("163 votes, rottentomatoes.com/m/taxi_driver");
  expect([...reception.querySelectorAll("[data-award]")].map((a) => a.textContent)).toEqual([
    "Palme d'Or, Cannes",
    "Best Actor, Academy Awards (nominated)",
  ]);
});

test("without_reception_there_are_no_laurels_and_no_part", async () => {
  tauri.handle("status", () => status(true));
  tauri.handle("services", () => [service()]);
  tauri.handle("item", () => detail({ files: [file(1, 1, "Heat.mkv")] }, { id: 7 }));
  screens({ screen: "details", id: 7 });
  await settle();
  expect(document.querySelector("[data-line=laurels]")).toBeNull();
  expect(sectionNames()).not.toContain("Reception");
});
