import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, test } from "bun:test";
import { CJK, LONG, clean, summary } from "../../test/fixtures";
import { tauri } from "../../test/setup";
import { boot } from "@/lib/api";
import { Work, workLabel } from "./Work";

afterEach(cleanup);

describe("Work", () => {
  test("a work with no poster is a typographic poster with its title", () => {
    const { container } = render(<Work item={summary({ poster: null, title: "Taxi Driver" })} shape="poster" focused={false} />);
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("[data-typographic]")?.textContent).toContain("Taxi Driver");
  });

  test("a poster loads the small file, lazily, with its dimensions", async () => {
    tauri.handle("boot", () => ({ status: {}, imageDir: "C:\\data\\images", firstRun: false }));
    await boot();
    const { container } = render(
      <Work item={summary({ poster: "movie-603-poster.jpg" })} shape="poster" focused={false} />,
    );
    const img = container.querySelector("img")!;
    expect(img.getAttribute("src")).toContain("movie-603-poster-s.jpg");
    expect(img.getAttribute("loading")).toBe("lazy");
    expect(img.getAttribute("decoding")).toBe("async");
    expect(Number(img.getAttribute("width"))).toBeGreaterThan(0);
    expect(Number(img.getAttribute("height"))).toBeGreaterThan(0);
    expect(container.querySelector("[data-typographic]")).toBeNull();
  });

  test("progress shows as a bar when started and not watched, a mark when watched", () => {
    const half = render(<Work item={summary({ progress: 0.5 })} shape="poster" focused={false} />);
    const bar = half.container.querySelector<HTMLElement>("[data-progress]");
    expect(bar?.style.width).toBe("50%");
    expect(half.container.querySelector("[data-watched]")).toBeNull();
    half.unmount();
    const seen = render(<Work item={summary({ watched: true, progress: 0.97 })} shape="poster" focused={false} />);
    expect(seen.container.querySelector("[data-progress]")).toBeNull();
    expect(seen.container.querySelector("[data-watched]")).not.toBeNull();
  });

  test("an unavailable work is marked offline, lit or not", () => {
    const { container, rerender } = render(
      <Work item={summary({ available: false })} shape="poster" focused={false} />,
    );
    const root = container.firstElementChild as HTMLElement;
    expect(root.hasAttribute("data-offline")).toBe(true);
    rerender(<Work item={summary({ available: false })} shape="poster" focused />);
    expect(root.hasAttribute("data-offline")).toBe(true);
    expect(root.hasAttribute("data-lit")).toBe(true);
  });

  test("focus puts the light on the work", () => {
    const { container, rerender } = render(<Work item={summary()} shape="poster" focused={false} />);
    expect(container.querySelector("[data-light]")).toBeNull();
    rerender(<Work item={summary()} shape="poster" focused />);
    expect(container.querySelector("[data-light]")).not.toBeNull();
  });

  test("awkward metadata never prints nonsense", () => {
    const awkward = [
      summary({ title: LONG }),
      summary({ title: CJK }),
      summary({ title: "Untitled", year: null, runtime_min: null, width: null, height: null }),
      summary({ kind: "episode", title: "Pilot", show_title: null, season: null, episode: null, year: null }),
    ];
    for (const item of awkward) {
      const { container, unmount } = render(<Work item={item} shape="poster" focused />);
      clean(container.textContent ?? "");
      clean(workLabel(item));
      unmount();
    }
    expect(workLabel(summary({ title: CJK, year: null }))).toBe(CJK);
  });
});
