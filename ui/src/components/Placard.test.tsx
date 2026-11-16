import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, test } from "bun:test";
import { CJK, LONG, clean, detail, fullItem, summary } from "../../test/fixtures";
import { Placard, placardLines } from "./Placard";

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
      facts: "1 h 54 min, 4K HDR10, Watched",
      overview: "A mentally unstable veteran works as a night-time taxi driver.",
    });
  });

  test("a summary alone says what it knows, including time left and offline", () => {
    const lines = placardLines(summary({ progress: 0.5, available: false }));
    expect(lines.credit).toBe("1976");
    expect(lines.facts).toBe("1 h 54 min, 1080p, 57 min left, Offline");
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
