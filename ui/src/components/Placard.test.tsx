import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, test } from "bun:test";
import type { ReactNode } from "react";
import { CJK, LONG, activity, audioTrack, clean, detail, fullItem, score, scoresService, service, subTrack, summary } from "../../test/fixtures";
import { tauri } from "../../test/setup";
import { stars } from "@/lib/format";
import { heardAndRead, Placard, placardLines, splitTitle, trackLines } from "./Placard";

afterEach(cleanup);

/** The placard as the app holds it: the services are asked for through the query client. */
function withServices(node: ReactNode, services = [service()]) {
  tauri.handle("services", () => services);
  return render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>{node}</QueryClientProvider>,
  );
}

const rated = (value: number, scale = 5) =>
  detail({ activity: activity({ ratings: [{ service: "letterboxd", on: 1_551_571_200, value, scale }] }) });
const lineNames = (root: ParentNode) => [...root.querySelectorAll("[data-line]")].map((l) => l.getAttribute("data-line"));

describe("verdicts", () => {
  test("the_verdicts_line_shows_my_rating_with_its_mark", async () => {
    const d = rated(4.5);
    expect(placardLines(d.item, d).verdicts).toEqual({ mine: { service: "letterboxd", on: 1_551_571_200, value: 4.5, scale: 5 }, public: [] });
    for (const size of ["hero", "panel"] as const) {
      const view = withServices(<Placard item={d.item} detail={d} size={size} />);
      const line = view.container.querySelector("[data-line=verdicts]")!;
      expect(line).not.toBeNull();
      // Beside a wall, after the facts; on the hero, beside the story and above the file line.
      const names = lineNames(view.container);
      if (size === "panel") expect(names.indexOf("verdicts")).toBeGreaterThan(names.indexOf("facts"));
      else {
        expect(names.indexOf("verdicts")).toBeGreaterThan(names.indexOf("story"));
        expect(names.indexOf("verdicts")).toBeLessThan(names.indexOf("facts"));
      }
      expect(line.querySelector("[aria-label='Rated 4.5 out of 5']")?.textContent).toBe("★★★★½");
      const mark = await waitFor(() => view.getByRole("img", { name: "Letterboxd" }));
      expect(line.contains(mark)).toBe(true);
      view.unmount();
    }
  });

  test("no_verdicts_line_without_activity", () => {
    const d = detail();
    expect(placardLines(d.item, d).verdicts).toEqual({ mine: null, public: [] });
    // No rating: no line, beside a wall or on Details.
    for (const props of [{ size: "panel" as const }, { size: "hero" as const, full: true }]) {
      const view = withServices(<Placard item={d.item} detail={d} {...props} />);
      expect(lineNames(view.container)).not.toContain("verdicts");
      view.unmount();
    }
    // Home's held placard keeps the line only while a service with the owner's activity is on.
    const off = withServices(<Placard item={summary()} size="hero" />);
    expect(lineNames(off.container)).not.toContain("verdicts");
    off.unmount();
    const on = withServices(<Placard item={summary()} size="hero" holdVerdicts />);
    expect(lineNames(on.container)).toContain("verdicts");
    expect(on.container.querySelector("[data-line=verdicts]")?.textContent).toBe("");
  });

  test("public_scores_follow_the_rating_in_the_origins_order_at_most_three", async () => {
    const d = detail({
      activity: activity({ ratings: [{ service: "letterboxd", on: 1, value: 4, scale: 5 }] }),
      reception: [score("letterboxd", 4.1, { scale: 5 }), score("metacritic", 94), score("imdb", 8.2), score("tomatometer", 89), score("unknown", 1)],
    });
    const view = withServices(<Placard item={d.item} detail={d} size="hero" />, [service(), scoresService()]);
    const line = view.container.querySelector("[data-line=verdicts]")!;
    await waitFor(() => expect(line.querySelectorAll("[data-score]").length).toBe(3));
    const order = [...line.querySelectorAll("[data-score]")].map((e) => e.getAttribute("data-score"));
    expect(order).toEqual(["imdb", "tomatometer", "metacritic"]);
    expect(view.getByRole("img", { name: "IMDb 8.2" }).textContent).toBe("8.2");
    expect(view.getByRole("img", { name: "Tomatometer 89%" })).not.toBeNull();
    expect(view.getByRole("img", { name: "Metacritic 94" }).hasAttribute("data-badge")).toBe(true);
    // The rating comes first.
    expect(line.firstElementChild?.querySelector("[role=img]")?.getAttribute("aria-label")).toBe("Rated 4 out of 5");
    // Without the service that describes them, scores are not shown.
    view.unmount();
    const bare = withServices(<Placard item={d.item} detail={d} size="panel" />, [service()]);
    await waitFor(() => expect(bare.getByRole("img", { name: "Letterboxd" })).not.toBeNull());
    expect(bare.container.querySelectorAll("[data-score]").length).toBe(0);
  });

  test("the_hero_sets_the_story_beside_the_verdicts", async () => {
    const d = detail(
      {
        activity: activity({ ratings: [{ service: "letterboxd", on: 1, value: 4.5, scale: 5 }] }),
        reception: [score("imdb", 8.2), score("tomatometer", 89), score("metacritic", 94)],
        files: [
          {
            id: 1,
            source_id: 1,
            rel_path: "a.mkv",
            size: 1,
            mtime: 0,
            item_id: 1,
            tech: { duration_s: 6840, width: 1920, height: 1080, video_codec: "h264", hdr: null, audio: [audioTrack()], subtitles: [subTrack()] },
          },
        ],
      },
      { overview: "A lonely veteran drives a taxi at night." },
    );
    const view = withServices(<Placard item={d.item} detail={d} size="hero" />, [service(), scoresService()]);
    const line = view.container.querySelector("[data-line=verdicts]")!;
    await waitFor(() => expect(line.querySelectorAll("[data-score]").length).toBe(3));
    expect(lineNames(view.container)).toEqual(["title", "body", "story", "credit", "overview", "verdicts", "facts"]);
    // Each score is a figure: its value, then its origin's mark beneath.
    for (const figure of line.querySelectorAll("[data-score]")) {
      const parts = [...figure.querySelectorAll("[role=img]")];
      expect(parts.length).toBe(2);
      expect(parts[1]!.hasAttribute("data-origin")).toBe(true);
    }
    // The owner's stars, with the service's mark and "Yours" beneath.
    const mine = line.querySelector("[data-figure=mine]")!;
    expect(mine.querySelector("[aria-label='Rated 4.5 out of 5']")).not.toBeNull();
    expect(mine.querySelector("[data-service=letterboxd]")).not.toBeNull();
    expect(mine.textContent).toContain("Yours");
    // The file line carries the languages; the tracks line is gone from the hero.
    expect(view.container.querySelector("[data-line=facts]")?.textContent).toContain("English, 1 subtitle");
    expect(view.container.querySelector("[data-line=tracks]")).toBeNull();
  });

  test("the_held_hero_keeps_an_empty_verdicts_row", () => {
    const on = withServices(<Placard item={summary()} size="hero" holdVerdicts />);
    expect(on.container.querySelector("[data-line=verdicts]")?.textContent).toBe("");
    expect(on.container.querySelector("[data-line=facts]")).not.toBeNull();
    on.unmount();
    const off = withServices(<Placard item={summary()} size="hero" />);
    expect(lineNames(off.container)).not.toContain("verdicts");
    expect(off.container.querySelector("[data-line=facts]")).not.toBeNull();
  });

  test("stars_for_4_5_of_5_are_four_and_a_half", () => {
    expect(stars(4.5, 5)).toBe("★★★★½");
    expect(stars(5, 5)).toBe("★★★★★");
    expect(stars(3, 5)).toBe("★★★");
    expect(stars(0.5, 5)).toBe("½");
    // On its own scale, to the nearest half.
    expect(stars(7, 10)).toBe("★★★★★★★");
    expect(stars(3.3, 5)).toBe("★★★½");
    expect(stars(0, 5)).toBe("");
  });

  test("a film with no file says it is not in the library, not its runtime", () => {
    const wanted = detail({ files: [], available: false });
    expect(placardLines(wanted.item, wanted).facts).toBe("Not in your library");
    expect(placardLines(wanted.item, wanted).languages).toBeNull();
    expect(placardLines(summary({ in_library: false, available: false, height: null, width: null })).facts).toBe("Not in your library");
    // A show has no files of its own and is in the library all the same.
    const show = detail({ files: [] }, { kind: "show", runtime_min: null });
    expect(placardLines(show.item, show).facts).not.toContain("Not in your library");
  });
});

describe("placardLines", () => {
  test("year and director; runtime, resolution and watched state; the overview", () => {
    const d = detail(
      {
        files: [
          {
            id: 1,
            source_id: 1,
            rel_path: "a.mkv",
            size: 1,
            mtime: 0,
            item_id: 1,
            tech: {
              duration_s: 6840,
              width: 3840,
              height: 1608,
              video_codec: "hevc",
              hdr: "HDR10",
              audio: [],
              subtitles: [],
            },
          },
        ],
        play: { item_id: 1, position_s: 0, duration_s: 6840, watched: true, last_played_at: 0 },
      },
      {
        overview: "A mentally unstable veteran works as a night-time taxi driver.",
        people: [
          { name: "Robert De Niro", role: "cast" },
          { name: "Martin Scorsese", role: "director" },
        ],
      },
    );
    expect(placardLines(d.item, d)).toEqual({
      title: "Taxi Driver",
      credit: "Martin Scorsese, 1976",
      facts: "1 h 54 min, Watched",
      marks: [
        { id: "ultra-hd", name: "4K Ultra HD" },
        { id: "hdr10", name: "HDR10" },
      ],
      sound: null,
      subtitles: "None",
      languages: "No subtitles",
      progress: null,
      overview: "A mentally unstable veteran works as a night-time taxi driver.",
      verdicts: { mine: null, public: [] },
    });
  });

  test("a summary alone says what it knows, including time left and offline", () => {
    const lines = placardLines(summary({ progress: 0.5, available: false }));
    expect(lines.credit).toBe("1976");
    expect(lines.facts).toBe("1 h 54 min, Offline");
    expect(lines.progress).toEqual({ done: 0.5, left: "57 min left" });
    expect(lines.marks.map((m) => m.id)).toEqual(["hd-1080p"]);
    expect(lines.overview).toBeNull();
  });

  test("the_file_line_names_what_is_heard_and_counts_the_subtitles", () => {
    const tech = (audio: ReturnType<typeof audioTrack>[], subtitles: ReturnType<typeof subTrack>[]) => ({
      duration_s: 1,
      width: 1920,
      height: 1080,
      video_codec: "h264",
      hdr: null,
      audio,
      subtitles,
    });
    const subs = (...langs: Array<string | null>) => langs.map((lang) => subTrack({ lang }));
    expect(heardAndRead(null)).toBeNull();
    expect(heardAndRead(tech([audioTrack()], subs("eng", "chi", "cze", "ger")))).toBe("English, 4 subtitles");
    expect(heardAndRead(tech([audioTrack()], subs("eng")))).toBe("English, 1 subtitle");
    expect(heardAndRead(tech([audioTrack()], []))).toBe("English, no subtitles");
    // A language with several tracks (full and SDH) is counted once.
    expect(heardAndRead(tech([audioTrack()], subs("eng", "eng", "spa")))).toBe("English, 2 subtitles");
    // Forced tracks are not the film's subtitles; unnamed tracks are counted when none is named.
    expect(heardAndRead(tech([audioTrack()], [subTrack({ forced: true }), ...subs(null, null)]))).toBe("English, 2 subtitles");
    // The original is named first and the rest are counted, so the two parts never run together; commentaries are left out.
    const heard = [
      audioTrack({ lang: "rus" }),
      audioTrack({ lang: "eng", original: true }),
      audioTrack({ lang: "fre", commentary: true }),
      audioTrack({ lang: "ukr" }),
      audioTrack({ lang: "ger" }),
      audioTrack({ lang: "ita" }),
    ];
    expect(heardAndRead(tech(heard, subs("eng")))).toBe("English and 4 other languages, 1 subtitle");
    expect(heardAndRead(tech(heard.slice(0, 2), []))).toBe("English and 1 other language, no subtitles");
    // Nothing heard: the subtitles alone, as a phrase of their own.
    expect(heardAndRead(tech([], []))).toBe("No subtitles");
  });

  test("an episode names its show and place", () => {
    const lines = placardLines(
      summary({ kind: "episode", title: "Ozymandias", show_title: "Breaking Bad", season: 5, episode: 14, year: 2013 }),
    );
    expect(lines.credit).toBe("Breaking Bad, S5 E14, 2013");
  });
});

describe("Placard", () => {
  test("awkward metadata never prints nonsense, in either size", () => {
    const cases = [
      { item: summary({ title: LONG }) },
      { item: summary({ title: CJK, year: null }) },
      {
        item: fullItem({ title: "Nothing known", year: null, overview: null, runtime_min: null, people: [] }),
        detail: detail({}, { title: "Nothing known", year: null, overview: null, runtime_min: 0, people: [] }),
      },
      { item: summary({ year: null, runtime_min: null, width: null, height: null, hdr: null }) },
    ];
    for (const size of ["hero", "panel"] as const) {
      for (const c of cases) {
        const { container, unmount } = withServices(<Placard item={c.item} detail={c.detail} size={size} />);
        for (const line of container.querySelectorAll("[data-line]")) clean(line.textContent ?? "");
        clean(container.textContent ?? "");
        unmount();
      }
    }
  });

  test("empty lines are left out, not drawn blank", () => {
    const { container } = withServices(
      <Placard item={summary({ year: null, runtime_min: null, width: null, height: null })} size="panel" />,
    );
    const lines = [...container.querySelectorAll("[data-line]")].map((l) => l.getAttribute("data-line"));
    expect(lines).toEqual(["title"]);
  });
});

describe("splitTitle", () => {
  test("a long title breaks where its poster would", () => {
    expect(splitTitle("Dr. Strangelove or: How I Learned to Stop Worrying and Love the Bomb")).toEqual({
      main: "Dr. Strangelove",
      sub: "or: How I Learned to Stop Worrying and Love the Bomb",
    });
    expect(splitTitle("Dr. Insólito o: Cómo aprendí a dejar de preocuparme y amar la bomba")).toEqual({
      main: "Dr. Insólito",
      sub: "o: Cómo aprendí a dejar de preocuparme y amar la bomba",
    });
    expect(splitTitle("Mission: Impossible – Dead Reckoning Part One")).toEqual({
      main: "Mission: Impossible",
      sub: "Dead Reckoning Part One",
    });
    expect(splitTitle("The Lord of the Rings: The Fellowship of the Ring")).toEqual({
      main: "The Lord of the Rings",
      sub: "The Fellowship of the Ring",
    });
  });
  test("a short title, or one with no break, stays whole", () => {
    expect(splitTitle("Mission: Impossible")).toEqual({ main: "Mission: Impossible", sub: null });
    expect(splitTitle("One Battle After Another")).toEqual({ main: "One Battle After Another", sub: null });
    expect(splitTitle("Harry Potter and the Philosopher's Stone")).toEqual({
      main: "Harry Potter and the Philosopher's Stone",
      sub: null,
    });
  });
});

describe("trackLines", () => {
  const tech = (audio = [audioTrack()], subtitles = [subTrack()]) => ({
    duration_s: 1, width: 1920, height: 1080, video_codec: "hevc", hdr: null, audio, subtitles,
  });
  test("the film's own language first, then the dubs, the rest counted", () => {
    const lines = trackLines(
      tech(
        [
          audioTrack({ lang: "rus" }),
          audioTrack({ lang: "ukr" }),
          audioTrack({ lang: "rus" }),
          audioTrack({ lang: "eng", original: true }),
          audioTrack({ lang: "fre" }),
          audioTrack({ lang: "ger" }),
          audioTrack({ lang: "eng", commentary: true }),
        ],
        [subTrack({ lang: "eng" }), subTrack({ lang: "rus", forced: true }), subTrack({ lang: "spa", hearing_impaired: true })],
      ),
    );
    // US English lists with a serial comma.
    expect(lines).toEqual({ sound: "English, Russian, Ukrainian, and 2 more", subtitles: "English and Spanish" });
  });
  test("no subtitles says so; unnamed tracks are counted; an unread file says nothing", () => {
    expect(trackLines(tech([audioTrack({ lang: null })], [])).subtitles).toBe("None");
    expect(trackLines(tech([audioTrack({ lang: null })], [])).sound).toBeNull();
    expect(trackLines(tech(undefined, [subTrack({ lang: null }), subTrack({ lang: "und" })])).subtitles).toBe("2 tracks");
    expect(trackLines(null)).toEqual({ sound: null, subtitles: null });
  });
});
