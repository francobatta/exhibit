import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, test } from "bun:test";
import { CJK, LONG, audioTrack, clean, detail, fullItem, subTrack, summary } from "../../test/fixtures";
import { Placard, placardLines, splitTitle, trackLines } from "./Placard";

afterEach(cleanup);

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
      credit: "1976, Martin Scorsese",
      facts: "1 h 54 min, Watched",
      marks: [
        { id: "ultra-hd", name: "4K Ultra HD" },
        { id: "hdr10", name: "HDR10" },
      ],
      sound: null,
      subtitles: "None",
      progress: null,
      overview: "A mentally unstable veteran works as a night-time taxi driver.",
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
        const { container, unmount } = render(<Placard item={c.item} detail={c.detail} size={size} />);
        for (const line of container.querySelectorAll("[data-line]")) clean(line.textContent ?? "");
        clean(container.textContent ?? "");
        unmount();
      }
    }
  });

  test("empty lines are left out, not drawn blank", () => {
    const { container } = render(
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
