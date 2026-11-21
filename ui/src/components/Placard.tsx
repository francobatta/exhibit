// The wall label, read in clusters: the title; then "year, director", "runtime, watched state" with
// the marks of the formats the file is in (4K Ultra HD, HDR10, Dolby Atmos), the verdicts (the
// owner's rating as stars, with the mark of the service it came from, then up to three public
// scores, each with its origin's mark), and the languages it can be
// heard and read in; then a line or two of description. Plain text, commas between facts,
// every line built with `format.facts` so a missing fact leaves no trace. A line with nothing to
// say is left out, except on Home, where the placard keeps its shape (see `hold`).

import type { ReactNode } from "react";
import { language, t, useT } from "@/i18n";
import type { Item, ItemDetail, ItemSummary, RatingFact, Sourced, SourcedScore, TechInfo } from "@/lib/api";
import { useServices } from "@/lib/queries";
import { episodeLabel, facts, languageName, listed, number, runtime, started, stars, titleOf } from "@/lib/format";
import { pictureMarks, soundMark, type Mark } from "@/lib/marks";
import { cn } from "@/lib/utils";
import type { Progress } from "./ActionLine";
import { FitTitle } from "./FitTitle";
import { FormatMark } from "./FormatMark";
import { orderedScores, ScoreValue } from "./Reception";
import { OriginMark, ServiceMark } from "./ServiceMark";

export interface PlacardLines {
  title: string;
  credit: string;
  facts: string;
  /** The formats the file is in, as marks; shown after the facts. */
  marks: Mark[];
  /** The languages it can be heard in ("English, Russian and 3 more"), when the file has been read. */
  sound: string | null;
  /** The languages of its full subtitles, "None", or how many unnamed tracks. */
  subtitles: string | null;
  /** The hero's short form of both: "English, 19 subtitles". */
  languages: string | null;
  /** How far a started, unwatched work has got; shown beside its action. */
  progress: Progress | null;
  overview: string | null;
  /** What the owner made of it (their own rating, from the newest service that has one), and what the public did. */
  verdicts: { mine: Sourced<RatingFact> | null; public: SourcedScore[] };
}

/** Public scores on a placard, after the owner's rating. */
const PUBLIC_SCORES = 3;

/** Languages named before the rest are counted. */
const NAMED = 3;

/** Distinct names, the first few, and how many more: "English, Russian, Ukrainian and 2 more". */
function named(names: string[]): string | null {
  const all = [...new Set(names)];
  if (all.length === 0) return null;
  const rest = all.length - NAMED;
  return listed(rest > 0 ? [...all.slice(0, NAMED), t("units.more", { count: rest })] : all);
}

/**
 * What a file can be heard and read in. The film's own soundtrack comes first; commentaries and
 * forced subtitles (only the lines in another language) are left out.
 */
export function trackLines(tech: TechInfo | null): { sound: string | null; subtitles: string | null } {
  if (tech === null) return { sound: null, subtitles: null };
  const heard = tech.audio.filter((a) => !a.commentary).sort((a, b) => Number(b.original) - Number(a.original));
  const subs = tech.subtitles.filter((s) => !s.forced);
  const subNames = named(subs.map((s) => languageName(s.lang)).filter((n) => n !== null));
  return {
    sound: named(heard.map((a) => languageName(a.lang)).filter((n) => n !== null)),
    subtitles: subs.length === 0 ? t("placard.none") : (subNames ?? t("units.track", { count: subs.length })),
  };
}

/**
 * What a file can be heard and read in, short enough for one line beside the format marks: the
 * film's own language named, the others heard and the subtitles counted ("English and 2 other
 * languages, 19 subtitles"). Subtitles count by language, else by track when none is named.
 */
export function heardAndRead(tech: TechInfo | null): string | null {
  if (tech === null) return null;
  const heard = [
    ...new Set(
      tech.audio
        .filter((a) => !a.commentary)
        .sort((a, b) => Number(b.original) - Number(a.original))
        .map((a) => languageName(a.lang))
        .filter((n) => n !== null),
    ),
  ];
  const subs = tech.subtitles.filter((s) => !s.forced);
  const read = new Set(subs.map((s) => languageName(s.lang)).filter((n) => n !== null)).size || subs.length;
  const subtitles = read > 0 ? t("units.subtitle", { count: read }) : t("placard.noSubtitles");
  const first = heard[0];
  if (first === undefined) return subtitles.charAt(0).toLocaleUpperCase(language()) + subtitles.slice(1);
  const said = heard.length > 1 ? t("placard.heardMore", { first, count: heard.length - 1 }) : first;
  return facts([said, subtitles]);
}

const isSummary = (item: ItemSummary | Item): item is ItemSummary => "available" in item;
const yearOf = (y: number | null | undefined) => (typeof y === "number" && y > 0 ? String(y) : null);

/** The placard's text, from whatever is known: a summary, an item, a detail. */
export function placardLines(item: ItemSummary | Item, detail?: ItemDetail): PlacardLines {
  const full = detail?.item ?? (isSummary(item) ? null : item);
  const summary = isSummary(item) ? item : null;
  const kind = full?.kind ?? item.kind;

  const director = full?.people.find((p) => p.role === "director")?.name ?? null;
  const year = yearOf(full?.year ?? item.year);
  const credit =
    kind === "episode"
      ? facts([summary?.show_title, episodeLabel(full?.season ?? item.season, full?.episode ?? item.episode), year])
      : facts([director, year]);

  const tech = detail?.files.find((f) => f.tech !== null)?.tech ?? null;
  const picture = tech
    ? pictureMarks(tech.height, tech.hdr, tech.width)
    : summary
      ? pictureMarks(summary.height, summary.hdr, summary.width)
      : { marks: [], words: null };
  const sound = tech ? soundMark(tech.audio) : null;

  const minutes = full?.runtime_min ?? item.runtime_min;
  const play = detail?.play ?? null;
  const watched = play?.watched ?? summary?.watched ?? false;
  // How far it has got: from the play record when it is here, else from the summary.
  let progress: Progress | null = null;
  if (!watched && play !== null && play.position_s > 0 && play.duration_s > play.position_s) {
    const left = (play.duration_s - play.position_s) / 60;
    progress = { done: play.position_s / play.duration_s, left: t("placard.left", { time: runtime(Math.max(1, Math.round(left))) }) };
  } else {
    const done = summary ? started(watched, summary.progress) : null;
    if (done !== null && minutes) {
      progress = { done, left: t("placard.left", { time: runtime(Math.max(1, Math.round(minutes * (1 - done)))) }) };
    }
  }
  const available = detail?.available ?? summary?.available ?? true;
  // A film on the owner's watchlist that they do not have: it has no runtime or format to speak of.
  const absent = kind === "movie" && (detail ? detail.files.length === 0 : summary?.in_library === false);
  // A show has no runtime or picture of its own; it says how many seasons it has.
  const seasons = kind === "show" && detail ? detail.children.filter((c) => c.kind === "season").length : 0;

  return {
    title: titleOf(full ?? item),
    credit,
    facts: absent
      ? facts([t("activity.notInLibrary"), watched ? t("work.watched") : null])
      : facts([
          seasons > 0 ? t("units.season", { count: seasons }) : null,
          runtime(minutes),
          picture.words,
          watched ? t("work.watched") : null,
          available ? null : t("work.offline"),
        ]),
    marks: absent ? [] : sound === null ? picture.marks : [...picture.marks, sound],
    ...trackLines(tech),
    languages: absent ? null : heardAndRead(tech),
    progress,
    overview: full?.overview?.trim() || null,
    verdicts: {
      mine: detail?.activity.ratings.find((r) => stars(r.value, r.scale) !== "") ?? null,
      public: detail?.reception ?? [],
    },
  };
}

/** Titles at least this long are split at a natural break, when they have one. */
const SPLIT_FROM = 29;

/**
 * A long title as a poster sets it: the name, then its subtitle. It breaks before an alternative
 * title ("Dr. Strangelove", "or: How I Learned…"), else at the last dash ("Mission: Impossible",
 * "Dead Reckoning Part One"), else at the first colon ("Borat", "Cultural Learnings…"). A short
 * title, or one without a break, stays whole.
 */
export function splitTitle(title: string): { main: string; sub: string | null } {
  const whole = { main: title, sub: null };
  if (title.length < SPLIT_FROM) return whole;
  const at = (main: string, sub: string) => {
    const m = main.trim();
    const s = sub.trim();
    return m.length >= 2 && s.length >= 2 ? { main: m, sub: s } : whole;
  };
  // "or:" in English, "o:" in Spanish.
  const or = /\s((?:or|o)[:,]\s)/i.exec(title);
  if (or) return at(title.slice(0, or.index), title.slice(or.index + 1));
  const dash = Math.max(title.lastIndexOf(" – "), title.lastIndexOf(" — "), title.lastIndexOf(" - "));
  if (dash > 0) return at(title.slice(0, dash), title.slice(dash + 3));
  const colon = title.indexOf(": ");
  if (colon > 0) return at(title.slice(0, colon), title.slice(colon + 2));
  return whole;
}

/** A rating as stars on its own scale, named in words for a screen reader ("Rated 4.5 out of 5"). */
export function Stars({ rating, className }: { rating: RatingFact; className?: string }) {
  const t = useT();
  return (
    <span
      role="img"
      aria-label={t("activity.ratedOutOf", { value: number(rating.value), scale: number(rating.scale) })}
      className={cn("shrink-0 leading-none tracking-[0.06em] whitespace-nowrap text-label", className)}
    >
      {stars(rating.value, rating.scale)}
    </span>
  );
}

/** A verdict on the hero, set as a figure: the value large, what it is small beneath it. */
function Figure({ value, caption, ...data }: { value: ReactNode; caption: ReactNode } & Record<`data-${string}`, string>) {
  return (
    <span {...data} className="flex flex-col items-start gap-(--verdict-caption-gap)">
      {value}
      <span className="flex h-(--mark-caption) items-center gap-2 text-body text-label-dim">{caption}</span>
    </span>
  );
}

export interface PlacardProps {
  item: ItemSummary | Item;
  detail?: ItemDetail;
  /** `hero` on Home and Details, over the backdrop; `panel` beside a wall. */
  size: "hero" | "panel";
  /** Further lines after the facts, in the same voice (the cast, the genres). Empty ones are left out. */
  more?: string[];
  /** The whole description (the details screen), not two lines of it. */
  full?: boolean;
  /**
   * Keep every line in place, empty or not, so moving between works never moves the placard.
   * Default: a hero placard with two lines of description (Home, the gallery).
   */
  hold?: boolean;
  /**
   * A service with the owner's activity or the public's scores is on: a held placard keeps a line
   * for the verdicts too, so a rating arriving with the record moves nothing. Without one the line
   * does not exist.
   */
  holdVerdicts?: boolean;
  className?: string;
}

export function Placard({ item, detail, size, more = [], full = false, hold: holdProp, holdVerdicts = false, className }: PlacardProps) {
  const t = useT();
  const lines = placardLines(item, detail);
  const name = splitTitle(lines.title);
  const after = more.filter((l) => l !== "");
  const hero = size === "hero";
  // A hero placard stands on its action line, so it keeps the same lines whatever a work lacks
  // (a show has no runtime; a record still loading has no description): only the title, growing
  // upward, changes its height. Its full description (the details screen) is not held.
  const hold = holdProp ?? (hero && !full);
  const mine = lines.verdicts.mine;
  const services = useServices().data;
  const scores = orderedScores(lines.verdicts.public, services).slice(0, PUBLIC_SCORES);
  const verdicts = mine !== null || scores.length > 0 || (hold && holdVerdicts);
  const title = (
    <hgroup data-line="title" className={cn("flex flex-col", hero ? "gap-3" : "gap-1.5")}>
      <FitTitle
        text={name.main}
        maxLines={2}
        className={cn(
          "font-display text-label",
          hero ? "text-hero font-title tracking-display" : "text-title font-title tracking-title",
        )}
      />
      {name.sub !== null && (
        <p
          data-line="subtitle"
          className={cn("font-display font-ui text-balance text-label", hero ? "text-heading tracking-title" : "text-lead")}
        >
          {name.sub}
        </p>
      )}
    </hgroup>
  );
  const overview = (hold || lines.overview) && (
    <p
      data-line="overview"
      className={cn(
        "font-reading leading-reading text-label-soft",
        full ? "line-clamp-8" : "line-clamp-2",
        hold && "min-h-[2lh]",
        hero ? "text-lead" : "text-body",
      )}
    >
      {lines.overview}
    </p>
  );

  if (hero) {
    // A wall label: the title, a bright byline, then the story beside the verdicts as figures, with
    // the file under them. Where the placard is narrow (Show, a small window) the two stack.
    const story = Boolean(overview) || after.length > 0;
    return (
      <div className={cn("@container flex w-full flex-col gap-6", className)}>
        {title}
        {(hold || lines.credit) && (
          <p data-line="credit" className="min-h-[1lh] truncate font-display font-ui text-heading tracking-title text-label">
            {lines.credit}
          </p>
        )}
        <div
          data-line="body"
          className="grid gap-y-6 @placard-split:grid-cols-[minmax(0,1fr)_var(--placard-side)] @placard-split:items-end @placard-split:gap-x-(--placard-split-gap)"
        >
          {story && (
            <div data-line="story" className="flex min-w-0 flex-col gap-3">
              {overview}
              {after.map((l) => (
                <p key={l} className="line-clamp-2 font-display text-body text-label-dim">
                  {l}
                </p>
              ))}
            </div>
          )}
          <div className="flex min-w-0 flex-col gap-5 font-display text-body text-label-dim">
            {verdicts && (
              <div
                data-line="verdicts"
                className={cn("flex items-end gap-x-(--verdict-figure-gap) whitespace-nowrap", hold && holdVerdicts && "min-h-(--verdict-row)")}
              >
                {mine && (
                  <Figure
                    data-figure="mine"
                    value={<Stars rating={mine} className="text-(length:--verdict-figure-stars)" />}
                    caption={
                      <>
                        <ServiceMark service={mine.service} size="caption" />
                        <span>{t("placard.yours")}</span>
                      </>
                    }
                  />
                )}
                {scores.map(({ score, origin }) => (
                  <Figure
                    key={score.origin}
                    data-score={score.origin}
                    value={<ScoreValue score={score} origin={origin} className="text-(length:--verdict-figure) font-title" />}
                    caption={<OriginMark origin={origin} size="caption" />}
                  />
                ))}
              </div>
            )}
            {(hold || lines.facts || lines.marks.length > 0 || lines.languages) && (
              <p data-line="facts" className="flex min-h-(--mark-panel-tall) items-center gap-(--mark-gap)">
                {lines.facts && <span className="shrink-0 whitespace-nowrap">{lines.facts}</span>}
                {lines.marks.map((m) => (
                  <FormatMark key={m.id} mark={m} size="panel" />
                ))}
                {lines.languages && <span className="min-w-0 truncate">{lines.languages}</span>}
              </p>
            )}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className={cn("flex w-full flex-col gap-3", className)}>
      {title}
      {(hold || lines.credit || lines.facts || verdicts || after.length > 0) && (
        <div className="flex flex-col gap-0.5 font-display text-body text-label-dim">
          {(hold || lines.credit) && (
            <p data-line="credit" className="min-h-[1lh] truncate font-ui text-label">
              {lines.credit}
            </p>
          )}
          {(hold || lines.facts || lines.marks.length > 0) && (
            <p data-line="facts" className="flex min-h-(--mark-panel-tall) items-center gap-(--mark-gap)">
              {lines.facts && <span className="min-w-0 truncate">{lines.facts}</span>}
              {lines.marks.map((m) => (
                <FormatMark key={m.id} mark={m} size={size} />
              ))}
            </p>
          )}
          {verdicts && (
            <p
              data-line="verdicts"
              className="flex min-h-(--verdict-star-panel) items-center gap-x-3 text-(length:--verdict-star-panel) whitespace-nowrap"
            >
              {mine && <Stars rating={mine} />}
              {mine && <ServiceMark service={mine.service} size={size} />}
              {scores.map(({ score, origin }, i) => (
                <span
                  key={score.origin}
                  data-score={score.origin}
                  className={cn("flex items-center gap-2", (i > 0 || mine) && "ml-(--mark-gap)")}
                >
                  <OriginMark origin={origin} size={size} />
                  <ScoreValue score={score} origin={origin} className="text-[0.8em]" />
                </span>
              ))}
            </p>
          )}
          {after.map((l) => (
            <p key={l} className="line-clamp-2">
              {l}
            </p>
          ))}
        </div>
      )}
      {overview}
    </div>
  );
}
