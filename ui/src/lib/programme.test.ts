import { describe, expect, test } from "bun:test";
import type { ItemSummary } from "./api";
import { momentOf, southern } from "./moment";
import { pick, programme } from "./programme";

const at = (y: number, m: number, d: number, h: number) => new Date(y, m - 1, d, h, 30);

describe("momentOf", () => {
  test("names the part of the day; after midnight still belongs to the evening before", () => {
    expect(momentOf(at(2026, 11, 18, 9), "Europe/Madrid").daypart).toBe("morning");
    expect(momentOf(at(2026, 11, 18, 14), "Europe/Madrid").daypart).toBe("afternoon");
    expect(momentOf(at(2026, 11, 18, 21), "Europe/Madrid").daypart).toBe("evening");
    const late = momentOf(at(2026, 11, 19, 1), "Europe/Madrid");
    expect(late.daypart).toBe("late");
    expect(late.day).toBe(momentOf(at(2026, 11, 18, 21), "Europe/Madrid").day);
  });

  test("the weekend runs from Friday evening through Sunday", () => {
    // 2026-11-20 is a Friday.
    expect(momentOf(at(2026, 11, 20, 14), "Europe/Madrid").weekend).toBe(false);
    expect(momentOf(at(2026, 11, 20, 21), "Europe/Madrid").weekend).toBe(true);
    expect(momentOf(at(2026, 11, 22, 14), "Europe/Madrid").weekend).toBe(true);
    // Early Monday is still Sunday night.
    expect(momentOf(at(2026, 11, 23, 2), "Europe/Madrid").weekend).toBe(true);
    expect(momentOf(at(2026, 11, 23, 9), "Europe/Madrid").weekend).toBe(false);
  });

  test("the seasons run the other way south of the equator", () => {
    expect(southern("America/Argentina/Buenos_Aires")).toBe(true);
    expect(southern("America/Mexico_City")).toBe(false);
    expect(momentOf(at(2026, 11, 18, 21), "Europe/Madrid").season).toBe("autumn");
    expect(momentOf(at(2026, 11, 18, 21), "America/Argentina/Buenos_Aires").season).toBe("spring");
    expect(momentOf(at(2026, 1, 10, 21), "Australia/Sydney").season).toBe("summer");
  });
});

describe("programme", () => {
  test("a weekend evening adds a long film before the season's shelf; no watchlist without a service", () => {
    const ids = programme(momentOf(at(2026, 11, 21, 21), "Europe/Madrid"), false).map((r) => r.id);
    expect(ids).toEqual(["continue", "long", "season", "acclaimed", "loved"]);
  });

  test("late at night: short ones and the loved", () => {
    const ids = programme(momentOf(at(2026, 11, 19, 1), "Europe/Madrid"), true).map((r) => r.id);
    expect(ids).toEqual(["continue", "short", "loved"]);
  });
});

const work = (id: number, more: Partial<ItemSummary> = {}) => ({ id, watched: false, available: true, ...more }) as ItemSummary;

describe("pick", () => {
  const evening = momentOf(at(2026, 11, 18, 21), "Europe/Madrid");

  test("something to resume comes first", () => {
    const chosen = pick(
      [
        { id: "continue", items: [work(1), work(2)] },
        { id: "watchlist", items: [work(3)] },
      ],
      evening,
    );
    expect(chosen).toEqual({ item: work(1), row: "continue" });
  });

  test("a work that can play now beats one on a drive that is away", () => {
    const chosen = pick(
      [
        { id: "continue", items: [work(1, { available: false })] },
        { id: "watchlist", items: [work(3)] },
      ],
      evening,
    );
    expect(chosen?.row).toBe("watchlist");
  });

  test("one of a shelf's first three, held through the evening and changing by day", () => {
    const rows = [{ id: "acclaimed" as const, items: [work(1), work(2), work(3), work(4)] }];
    const picks = new Set<number>();
    for (let d = 10; d < 20; d++) {
      const tonight = pick(rows, momentOf(at(2026, 11, d, 21), "Europe/Madrid"))!.item.id;
      expect(pick(rows, momentOf(at(2026, 11, d + 1, 2), "Europe/Madrid"))).not.toBeNull();
      expect([1, 2, 3]).toContain(tonight);
      picks.add(tonight);
    }
    expect(picks.size).toBeGreaterThan(1);
  });

  test("new and loved works are not put forward while another shelf has something", () => {
    const chosen = pick(
      [
        { id: "recent", items: [work(1)] },
        { id: "season", items: [work(2)] },
      ],
      evening,
    );
    expect(chosen?.row).toBe("season");
  });
});
