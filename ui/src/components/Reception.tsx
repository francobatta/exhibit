// The public's verdict and a film's laurels, drawn from whatever the services report. The window
// names no site: each score comes with its origin's name, mark and style from the adapter, and
// each award body with its id, worded by the catalogue (or the adapter's English name).

import { t, translated } from "@/i18n";
import type { AwardEntry, BodyInfo, OriginInfo, ServiceView, SourcedScore } from "@/lib/api";
import { scoreText } from "@/lib/format";
import { cn } from "@/lib/utils";
import { originsOf } from "./ServiceMark";

type Origin = OriginInfo & { place: number };

/** The scores whose origin a service describes, in the services' order (IMDb, Tomatometer, Metacritic…). */
export function orderedScores(scores: SourcedScore[], services: ServiceView[] | undefined): Array<{ score: SourcedScore; origin: Origin }> {
  const origins = originsOf(services);
  return scores
    .flatMap((score) => {
      const origin = origins.get(score.origin);
      return origin ? [{ score, origin }] : [];
    })
    .sort((a, b) => a.origin.place - b.origin.place);
}

/** A score as its origin writes it; a badge (Metacritic) is a square of the text colour with the number cut out of it. */
export function ScoreValue({ score, origin, className }: { score: SourcedScore; origin: OriginInfo; className?: string }) {
  const text = scoreText(score.value, origin.style);
  const name = t("reception.scoreOf", { origin: origin.name, score: text });
  if (origin.style === "badge") {
    return (
      <span
        role="img"
        aria-label={name}
        data-badge
        className={cn("inline-grid min-w-(--score-badge) place-items-center bg-label px-[0.2em] py-[0.08em] leading-none font-ui text-wall tabular-nums", className)}
      >
        {text}
      </span>
    );
  }
  return (
    <span role="img" aria-label={name} className={cn("font-ui leading-none text-label tabular-nums", className)}>
      {text}
    </span>
  );
}

/** A body's name in the owner's language, else the adapter's English one. */
export function bodyName(body: string, services: ServiceView[] | undefined): string {
  const worded = translated(`awards.bodies.${body}`, {});
  if (worded !== null) return worded;
  const all: BodyInfo[] = (services ?? []).flatMap((s) => s.bodies ?? []);
  return all.find((b) => b.id === body)?.name ?? body;
}

export interface Wreath {
  body: string;
  /** The body's most prominent win, which names the wreath. */
  top: AwardEntry;
  /** Wins at this body. */
  wins: number;
}

/** At most this many wreaths stand above a title. */
export const WREATHS = 3;

/** One wreath per body the film won at, the most prominent first, and how many wins are left unshown. */
export function wreaths(awards: AwardEntry[]): { shown: Wreath[]; more: number } {
  const byBody = new Map<string, Wreath>();
  for (const a of awards) {
    if (!a.won) continue;
    const w = byBody.get(a.body);
    if (!w) byBody.set(a.body, { body: a.body, top: a, wins: 1 });
    else {
      w.wins += 1;
      if (a.rank < w.top.rank || (a.rank === w.top.rank && (a.year ?? 9999) < (w.top.year ?? 9999))) w.top = a;
    }
  }
  const all = [...byBody.values()].sort((a, b) => a.top.rank - b.top.rank);
  const shown = all.slice(0, WREATHS);
  return { shown, more: all.slice(WREATHS).reduce((n, w) => n + w.wins, 0) };
}

/** One branch of a laurel: leaves in pairs along a stem that bows outward, smaller toward the tip. */
const BRANCH = (() => {
  const p0 = [25, 98];
  const c = [-6, 52];
  const p1 = [21, 5];
  const at = (s: number) => [0, 1].map((k) => (1 - s) ** 2 * p0[k]! + 2 * (1 - s) * s * c[k]! + s * s * p1[k]!) as [number, number];
  const slope = (s: number) => [0, 1].map((k) => 2 * (1 - s) * (c[k]! - p0[k]!) + 2 * s * (p1[k]! - c[k]!)) as [number, number];
  const leaves: Array<{ x: number; y: number; rx: number; ry: number; angle: number }> = [];
  for (let i = 0; i < 9; i++) {
    const s = 0.08 + i * 0.105;
    const [x, y] = at(s);
    const [dx, dy] = slope(s);
    const along = (Math.atan2(dy, dx) * 180) / Math.PI;
    const size = 1 - i * 0.045;
    for (const side of [-1, 1]) {
      const angle = along + side * 38;
      const rad = (angle * Math.PI) / 180;
      leaves.push({ x: x + Math.cos(rad) * 6.4 * size, y: y + Math.sin(rad) * 6.4 * size, rx: 7.6 * size, ry: 3 * size, angle });
    }
  }
  const [tx, ty] = at(1);
  const stem = `M${p0.join(" ")}Q${c.join(" ")} ${tx} ${ty}`;
  return { stem, leaves };
})();

function Branch({ mirrored }: { mirrored?: boolean }) {
  return (
    <svg viewBox="-12 -4 50 108" aria-hidden className={cn("h-(--laurel-height) w-auto shrink-0 fill-current", mirrored && "-scale-x-100")}>
      <path d={BRANCH.stem} fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" />
      {BRANCH.leaves.map((l, i) => (
        <ellipse key={i} cx={l.x} cy={l.y} rx={l.rx} ry={l.ry} transform={`rotate(${l.angle} ${l.x} ${l.y})`} />
      ))}
    </svg>
  );
}

/**
 * Laurels, as a festival poster wears them above a title: a wreath for each body the film won at,
 * named by its most prominent win ("Palme d'Or", "Cannes 1976"), at most three, and a line for
 * the rest. Nothing for a film without wins.
 */
export function Laurels({ awards, services }: { awards: AwardEntry[]; services: ServiceView[] | undefined }) {
  const { shown, more } = wreaths(awards);
  if (shown.length === 0) return null;
  return (
    <div data-line="laurels" className="flex flex-col gap-3 text-label">
      <ul aria-label={t("laurels.label")} className="flex flex-wrap items-center gap-(--laurel-gap)">
        {shown.map((w) => {
          const where = [bodyName(w.body, services), w.top.year].filter(Boolean).join(" ");
          return (
            <li key={w.body} data-laurel={w.body} className="flex items-center gap-1">
              <Branch />
              <p className="flex max-w-(--laurel-words) flex-col items-center gap-1 text-center font-display">
                <span className="text-lead leading-tight font-title tracking-title text-balance">{w.top.name}</span>
                <span className="text-caption whitespace-nowrap text-label-dim">{where}</span>
                {w.wins > 1 && <span className="text-caption text-label-dim">{t("laurels.wins", { count: w.wins })}</span>}
              </p>
              <Branch mirrored />
            </li>
          );
        })}
      </ul>
      {more > 0 && <p className="font-display text-body text-label-dim">{t("laurels.more", { count: more })}</p>}
    </div>
  );
}

/** "rottentomatoes.com/m/taxi_driver": a score's page, written out for a screen that cannot open it. */
export function pageOf(url: string | null): string | null {
  if (!url) return null;
  try {
    const u = new URL(url);
    return `${u.hostname.replace(/^www\./, "")}${u.pathname.replace(/\/$/, "")}`;
  } catch {
    return null;
  }
}

/** Awards by year, oldest first; the wins of a year before its nominations. */
export function awardsByYear(awards: AwardEntry[]): Array<{ year: number | null; awards: AwardEntry[] }> {
  const years = new Map<number | null, AwardEntry[]>();
  for (const a of [...awards].sort((x, y) => Number(y.won) - Number(x.won) || x.rank - y.rank)) {
    years.set(a.year, [...(years.get(a.year) ?? []), a]);
  }
  return [...years.entries()]
    .sort(([a], [b]) => (a ?? 9999) - (b ?? 9999))
    .map(([year, list]) => ({ year, awards: list }));
}
