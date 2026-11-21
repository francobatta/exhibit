// A work's own room, in two movements. The first is the work: its backdrop, the laurels it won,
// its placard in full, and its row of cues as on Home (components/Cues). The rest of what can be
// done waits in the item menu (X). Below, peeking in, the second: the owner's own review when they
// wrote one, who is in it, as portraits, then its record: the file, its reception (the public's
// scores and every award) and its history (every watch, from Exhibit and from each service).
// Down glides from one movement to the other; the backdrop darkens as it goes.

import { motion } from "motion/react";
import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode, type Ref } from "react";
import { useScreenBackdrop } from "@/components/Backdrop";
import { Cues } from "@/components/Cues";
import { Placard, placardLines, Stars, trackLines } from "@/components/Placard";
import { awardsByYear, bodyName, Laurels, orderedScores, pageOf, ScoreValue } from "@/components/Reception";
import { OriginMark, ServiceMark } from "@/components/ServiceMark";
import { ErrorState, Loading } from "@/components/states";
import { buttonVariants } from "@/components/ui/button";
import { Collapsible, CollapsiblePanel, CollapsibleTrigger } from "@/components/ui/collapsible";
import type { Dir } from "@/focus/bridge";
import { FocusGroup, useFocusItem, useInputMode } from "@/focus/react";
import { t, useT } from "@/i18n";
import {
  imageUrl,
  plain,
  type CastMember,
  type Id,
  type ItemDetail,
  type MediaFile,
  type RatingFact,
  type ReviewFact,
  type ServiceView,
  type Source,
  type Sourced,
  type WatchFact,
} from "@/lib/api";
import { EXHIBIT, bytes, clock, facts, initials, languageName, listed, loggedLine, resolution, serviceName, shortDay, votesText } from "@/lib/format";
import { soundMark } from "@/lib/marks";
import { focusSpring, glideScroll, numberToken, remPx, rems, stopGlide } from "@/lib/motion";
import { useCast, useItem, useItemSources, useServices } from "@/lib/queries";
import { cn } from "@/lib/utils";
import { openAsk, useNav } from "@/nav/stack";

export function Details({ id }: { id: Id }) {
  const t = useT();
  const query = useItem(id);
  if (query.isPending) return <Loading />;
  if (query.isError) {
    return (
      <ErrorState
        message={plain(query.error)}
        hint={t("common.gone")}
        action={{ label: t("common.goBack"), onAction: () => useNav.getState().back() }}
      />
    );
  }
  return <Shown detail={query.data} />;
}

function Shown({ detail }: { detail: ItemDetail }) {
  const item = detail.item;
  const sources = useItemSources(detail);
  const drive = sources.find((s) => !s.online)?.path;
  const cast = useCast(item.id).data ?? [];
  // An episode has no artwork of its own: it hangs on its show's (through its season).
  const parent = useItem(item.backdrop === null ? item.parent_id : null).data?.item;
  const show = useItem(parent?.backdrop === null ? parent.parent_id : null).data?.item;
  useScreenBackdrop(
    item.backdrop ?? parent?.backdrop ?? show?.backdrop ?? null,
    item.poster ?? parent?.poster ?? show?.poster ?? null,
  );

  const services = useServices().data;
  const review = detail.activity.reviews[0] ?? null;
  // The stars beside the review are the rating its own service gave.
  const reviewRating = review === null ? null : (detail.activity.ratings.find((r) => r.service === review.service) ?? null);
  const firstItems = review !== null || cast.length > 0;

  const scroller = useRef<HTMLDivElement>(null);
  const second = useRef<HTMLElement>(null);
  const reviewPart = useRef<HTMLElement>(null);
  const castPart = useRef<HTMLElement>(null);
  const [below, setBelow] = useState(false);
  const belowNow = useRef(below);
  // Where the scroller is gliding to, so a held key pages on from there, not from mid-glide.
  const target = useRef(0);
  const px = (token: string) => rems(token) * remPx();
  // The second movement stands just under the bar.
  const anchor = (down: boolean) => (down && second.current !== null ? second.current.offsetTop - px("--space-bar") : 0);
  const glide = (down: boolean, to = anchor(down)) => {
    const el = scroller.current;
    if (el === null) return;
    belowNow.current = down;
    setBelow(down);
    target.current = to;
    glideScroll(el, "top", to);
  };
  const lowest = () => {
    const el = scroller.current;
    return el === null ? 0 : Math.max(anchor(true), el.scrollHeight - el.clientHeight);
  };
  /** Where the movement stands to show the end of a part, or its top when it is short. */
  const showEnd = (part: HTMLElement | null) => {
    const el = scroller.current;
    if (el === null || part === null) return anchor(true);
    const end = part.offsetTop + part.offsetHeight + px("--space-edge") - el.clientHeight;
    return Math.min(lowest(), Math.max(anchor(true), end));
  };
  // The cast stays where it is when it already shows below the review; after a long review the
  // movement slides up to bring it under the bar.
  const castView = () => {
    const el = scroller.current;
    const part = castPart.current;
    if (el === null || part === null) return anchor(true);
    if (part.offsetTop + part.offsetHeight <= anchor(true) + el.clientHeight) return anchor(true);
    return Math.min(lowest(), Math.max(anchor(true), part.offsetTop - px("--space-bar")));
  };
  /**
   * A review longer than the window reads a page at a time: Down and Up on it glide through it
   * before focus moves on. False when it already shows to its end that way.
   */
  const page = (dir: 1 | -1): boolean => {
    const el = scroller.current;
    const part = reviewPart.current;
    if (el === null || part === null || part.offsetHeight === 0) return false;
    const step = el.clientHeight * numberToken("--details-review-page");
    const at = target.current;
    if (dir === 1) {
      const end = showEnd(part);
      if (end <= at + 1) return false;
      glide(true, Math.min(at + step, end));
    } else {
      const start = Math.max(anchor(true), part.offsetTop - px("--space-bar"));
      if (start >= at - 1) return false;
      glide(true, Math.max(at - step, start));
    }
    return true;
  };
  // A resized window (full screen, another monitor) keeps the movement in view where it stands.
  useEffect(() => {
    const el = scroller.current;
    if (el === null) return;
    const kept = new ResizeObserver(() => {
      stopGlide(el);
      target.current = anchor(belowNow.current);
      el.scrollTop = target.current;
    });
    kept.observe(el);
    return () => {
      kept.disconnect();
      stopGlide(el);
    };
  }, []);
  // Up from below comes back the way Down went: from the cast to the review, and from the review
  // (or the cast, with no review) to the work's first cue, which the nearest-item search would pass
  // over, the cues being under the bar; with nothing to focus below, to the cues' own place.
  // The review and the cast take focus only once their movement is in view (they are inert while
  // it peeks, so the nearest-item search from the bar cannot land on them): Down from the cues
  // brings it in, then lights the review, or else the portrait lit last.
  const lastCast = useRef<string | null>(null);
  const toSecond = useRef(false);
  /** The portrait lit last, else the first. */
  const portrait = () =>
    (lastCast.current === null ? null : document.querySelector<HTMLElement>(`[data-focus-key="${lastCast.current}"]`)) ?? focusable(CAST);
  useEffect(() => {
    if (!below || !toSecond.current) return;
    toSecond.current = false;
    (focusable(REVIEW) ?? portrait())?.focus({ preventScroll: true });
  }, [below]);
  const secondKeys = (e: KeyboardEvent<HTMLElement>) => {
    if (!below || (e.key !== "ArrowUp" && e.key !== "ArrowDown")) return;
    const from = e.target as HTMLElement;
    const fromReview = from.closest(REVIEW) !== null;
    const fromCast = from.closest(CAST) !== null;
    const stop = () => {
      e.preventDefault();
      e.stopPropagation();
    };
    if (fromReview && page(e.key === "ArrowDown" ? 1 : -1)) return stop();
    if (e.key === "ArrowDown") {
      // On from the review's end to the cast, as Up comes back from it.
      const next = fromReview ? portrait() : null;
      if (next === null) return;
      stop();
      next.focus({ preventScroll: true });
      return;
    }
    if (fromCast && review !== null) {
      stop();
      focusable(REVIEW)?.focus({ preventScroll: true });
      glide(true, showEnd(reviewPart.current));
      return;
    }
    if (!fromCast && !fromReview && firstItems) return;
    stop();
    if (fromCast || fromReview) focusable(CUES)?.focus({ preventScroll: true });
    glide(false);
  };

  return (
    // The wheel moves through the cues, the review and the cast, through the arrow keys (focus/input/wheel.ts).
    <div data-wheel-steps className="details absolute inset-0 overflow-clip" onKeyDownCapture={secondKeys}>
      <div aria-hidden className="details-shade pointer-events-none absolute inset-0" />
      <div ref={scroller} className="details-scroller absolute inset-0 overflow-y-hidden">
        <section
          aria-label={item.title}
          className="flex h-[calc(100%-var(--details-peek))] flex-col justify-end px-edge pt-bar pb-(--details-foot)"
        >
          <div className="flex max-w-measure flex-col gap-5">
            <Laurels awards={detail.awards} services={services} />
            <Placard item={item} detail={detail} size="hero" full more={[facts(item.genres)]} />
            <Cues
              id="details-cues"
              prefix="action"
              item={detail}
              drive={drive}
              progress={placardLines(item, detail).progress}
              play={detail.files.length > 0}
              onFocusCue={() => glide(false)}
              onEdge={(dir) => {
                if (dir[1] !== 1) return;
                toSecond.current = firstItems;
                glide(true);
              }}
            />
          </div>
        </section>
        <section ref={second} className="flex min-h-full flex-col gap-(--details-part-gap) px-edge pb-edge">
          {review !== null && (
            <YourReview
              ref={reviewPart}
              review={review}
              rating={reviewRating}
              inert={!below}
              onFocus={() => {
                // The pointer reaching it from the first movement; keys bring the movement in first.
                if (!belowNow.current) glide(true);
              }}
              onEdge={(dir) => {
                // Past the end of the review with no cast below it, the record comes into view.
                if (dir[1] === 1 && cast.length === 0) glide(true, lowest());
              }}
            />
          )}
          {cast.length > 0 && (
            <Cast
              ref={castPart}
              cast={cast}
              inert={!below}
              onFocus={(key) => {
                lastCast.current = key;
                glide(true, castView());
              }}
            />
          )}
          <Record detail={detail} sources={sources} />
        </section>
      </div>
    </div>
  );
}

const CUES = "[data-focus-group='details-cues']";
const REVIEW = "[data-focus-group='details-review']";
const CAST = "[data-focus-group='details-cast']";
/** The first focus item of a group, by its selector. */
const focusable = (group: string) => document.querySelector<HTMLElement>(`${group} [data-focus-key]`);

/** A part of the second movement, named quietly above it. */
function Part({ title, children, inert, ref }: { title: string; children: ReactNode; inert?: boolean; ref?: Ref<HTMLElement> }) {
  return (
    <section ref={ref} aria-label={title} inert={inert} className="flex flex-col gap-6">
      <h2 className="font-display text-caption font-ui tracking-[0.14em] text-label-faint uppercase">{title}</h2>
      {children}
    </section>
  );
}

/**
 * The owner's review: its stars, its day and the mark of the service it was written on, then the
 * text whole, in the reading face, as written. A review with spoilers is folded behind one press.
 * It is one focus stop either way, so Down and Up pass through it as through the cast.
 */
function YourReview({
  review,
  rating,
  inert,
  onFocus,
  onEdge,
  ref,
}: {
  review: Sourced<ReviewFact>;
  rating: Sourced<RatingFact> | null;
  inert: boolean;
  onFocus: () => void;
  onEdge: (dir: Dir) => void;
  ref?: Ref<HTMLElement>;
}) {
  const t = useT();
  const [open, setOpen] = useState(!review.spoiler);
  const when = review.on === null ? null : shortDay(review.on, { utc: true });
  const text = <ReviewText text={review.text} focus={!review.spoiler} />;
  return (
    <Part ref={ref} title={t("activity.yourReview")} inert={inert}>
      <FocusGroup
        id="details-review"
        orientation="vertical"
        reveals
        aria-label={t("activity.yourReview")}
        onFocus={onFocus}
        onEdge={onEdge}
        className="flex max-w-(--details-review-measure) flex-col items-start gap-6"
      >
        <p data-line="review-head" className="flex items-center gap-4 font-display text-lead text-label-dim">
          {rating && <Stars rating={rating} className="text-(length:--verdict-star)" />}
          {when && <span>{when}</span>}
          <ServiceMark service={review.service} size="panel" />
        </p>
        {review.spoiler ? (
          <Collapsible open={open} onOpenChange={setOpen}>
            <Fold open={open} />
            <CollapsiblePanel>{text}</CollapsiblePanel>
          </Collapsible>
        ) : (
          text
        )}
      </FocusGroup>
    </Part>
  );
}

/**
 * The fold over a review with spoilers: A reads it, and A again folds it away. It names the button
 * of the device in hand ("Press A to read", "Press Enter to read"), or asks for a click.
 */
function Fold({ open }: { open: boolean }) {
  const t = useT();
  const mode = useInputMode();
  const { ref, props } = useFocusItem<HTMLButtonElement>({
    key: "review:fold",
    hint: open ? t("activity.hideReview") : t("activity.readReview"),
  });
  // The trigger opens itself on a click, Enter and the pad's A; the focus item's click would run nothing.
  const { onClick: _click, ...focus } = props;
  const label = open
    ? t("activity.hideReview")
    : mode === "pointer"
      ? t("activity.spoilersClick")
      : t("activity.spoilers", { button: mode === "pad" ? "A" : t("keys.enter") });
  return (
    <CollapsibleTrigger ref={ref} {...focus} className={buttonVariants({ variant: "cue", size: "cue" })}>
      {label}
    </CollapsibleTrigger>
  );
}

/**
 * The review's words, kept to their paragraphs. Without a fold over them they are the review's
 * focus stop themselves: lit, they brighten from the soft paper of running text to the label's.
 */
function ReviewText({ text, focus }: { text: string; focus: boolean }) {
  const className = "font-reading text-lead leading-reading whitespace-pre-line text-pretty";
  if (!focus) return <p className={cn(className, "text-label-soft")}>{text}</p>;
  return <FocusedText text={text} className={className} />;
}

function FocusedText({ text, className }: { text: string; className: string }) {
  const t = useT();
  const { ref, props, focused } = useFocusItem<HTMLElement>({ key: "review:text" });
  return (
    <article
      ref={ref}
      {...props}
      aria-label={t("activity.yourReview")}
      className={cn(className, "outline-none", focused ? "text-label" : "text-label-soft")}
    >
      {text}
    </article>
  );
}

function Cast({
  cast,
  inert,
  onFocus,
  ref,
}: {
  cast: CastMember[];
  inert: boolean;
  onFocus: (key: string) => void;
  ref?: Ref<HTMLElement>;
}) {
  const t = useT();
  return (
    <Part ref={ref} title={t("details.cast")} inert={inert}>
      <FocusGroup
        id="details-cast"
        orientation="horizontal"
        reveals
        aria-label={t("details.cast")}
        onFocus={(e) => {
          const key = (e.target as HTMLElement).dataset.focusKey;
          if (key !== undefined) onFocus(key);
        }}
        className="flex items-start gap-(--details-cast-gap)"
      >
        {cast.map((m) => (
          <Portrait key={m.name} member={m} />
        ))}
      </FocusGroup>
    </Part>
  );
}

/** One of the cast: A looks for them across the library, in Ask. */
function Portrait({ member }: { member: CastMember }) {
  const t = useT();
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({
    key: `cast:${member.name}`,
    onSelect: () => openAsk(member.name),
    hint: t("hints.find"),
  });
  const src = imageUrl(member.portrait);
  const [failed, setFailed] = useState(false);
  const spring = focusSpring();
  return (
    <button
      ref={ref}
      {...props}
      type="button"
      aria-label={facts([member.name, member.character && t("details.as", { character: member.character })])}
      className="group flex w-(--details-portrait) shrink-0 flex-col gap-3 text-left outline-none"
    >
      <span className="relative block">
        <motion.span
          initial={false}
          animate={{ scale: focused ? numberToken("--work-focus-scale") : 1 }}
          transition={spring}
          className={cn(
            "relative block aspect-[2/3] overflow-hidden rounded-work bg-wall-raised",
            "transition-[filter] duration-(--dur-screen) ease-out",
            !focused && "[filter:var(--work-dim)]",
          )}
        >
          {src !== null && !failed ? (
            <img
              src={src}
              alt=""
              loading="lazy"
              decoding="async"
              draggable={false}
              onError={() => setFailed(true)}
              className="absolute inset-0 size-full object-cover"
            />
          ) : (
            <span
              aria-hidden
              className="absolute inset-0 grid place-items-center font-display text-display font-title tracking-display text-label-faint"
            >
              {initials(member.name)}
            </span>
          )}
        </motion.span>
        {focused && (
          <motion.span
            layoutId="cast-key"
            aria-hidden
            transition={spring}
            className="pointer-events-none absolute inset-(--work-focus-inset) rounded-work shadow-[inset_0_0_0_var(--hairline)_var(--lamp)]"
          />
        )}
      </span>
      <span className="flex flex-col gap-1 font-display">
        <span className={cn("text-lead font-ui transition-colors", focused ? "text-label" : "text-label-soft")}>
          {member.name}
        </span>
        {member.character && <span className="line-clamp-2 text-body text-label-dim">{member.character}</span>}
      </span>
    </button>
  );
}

const fileName = (rel: string) => rel.split(/[\\/]/).at(-1) ?? rel;

/** Distinct language names, the first three by name and the rest counted. */
function languages(codes: Array<string | null>): string {
  const names = [...new Set(codes.map(languageName).filter((n) => n !== null))];
  const rest = names.length - 3;
  return listed(rest > 0 ? [...names.slice(0, 3), t("units.more", { count: rest })] : names);
}

/** One version of the work: "4.2 GB, 1080p HDR10, English and French, subtitles English, D:\Movies, Offline". */
export function fileLine(file: MediaFile, source: Source | undefined): string {
  const tech = file.tech;
  const audio = tech ? languages(tech.audio.map((a) => a.lang)) : "";
  const subs = tech ? languages(tech.subtitles.map((s) => s.lang)) : "";
  return facts([
    bytes(file.size),
    tech ? resolution(tech.height, tech.hdr, tech.width) : null,
    audio,
    subs && t("details.subtitlesIn", { languages: subs }),
    source?.path,
    source?.online === false ? t("work.offline") : null,
  ]);
}

/** How far the owner got: "Watched 3 March", "Stopped at 1:04:12 on 2 March", "Never played". */
export function historyLine(detail: ItemDetail): string {
  const play = detail.play;
  const when = play === null ? null : shortDay(play.last_played_at);
  if (play?.watched) return when ? t("details.watchedOn", { when }) : t("details.watched");
  if (play !== null && play.position_s > 0) {
    const time = clock(play.position_s);
    return when ? t("details.stoppedAtOn", { time, when }) : t("details.stoppedAt", { time });
  }
  return t("details.neverPlayed");
}

/** Watches listed one by one before the rest are counted. */
const WATCHES = 5;

/** One watch: "Played in Exhibit 12 March", "Logged on Letterboxd 3 March 2019, rewatch". */
function watchLine(watch: Sourced<WatchFact>, services: ServiceView[] | undefined): string {
  let line: string;
  if (watch.service === EXHIBIT) {
    const date = watch.on === null ? null : shortDay(watch.on);
    line = date ? t("activity.playedInExhibitOn", { date }) : t("activity.playedInExhibit");
  } else {
    line = loggedLine(watch, services);
  }
  return watch.rewatch ? t("activity.withRewatch", { line }) : line;
}

/**
 * Where the eye's state comes from, when the owner's own choice here goes against a service: "You
 * marked it unwatched here; Letterboxd has it logged."
 */
function overrideNote(detail: ItemDetail, services: ServiceView[] | undefined): string | null {
  const logged = detail.activity.watches.find((w) => w.service !== EXHIBIT);
  if (detail.activity.watched_choice !== false || logged === undefined) return null;
  return t("activity.overridden", { service: serviceName(logged.service, services) });
}

/**
 * Every watch, newest first, each with the mark of where it was logged (Exhibit's own has none);
 * then a play stopped part-way. With neither, how far the owner got as before ("Never played").
 */
function Watches({ detail }: { detail: ItemDetail }) {
  const t = useT();
  const services = useServices().data;
  const watches = detail.activity.watches;
  const play = detail.play;
  const stopped = play !== null && !play.watched && play.position_s > 0;
  const note = overrideNote(detail, services);
  return (
    <div className="contents">
      <dt className="pt-0.5 font-display text-body text-label-faint">{t("details.watched")}</dt>
      <dd className="flex min-w-0 flex-col gap-1 font-display">
        {watches.slice(0, WATCHES).map((w, i) => (
          <span key={i} data-watch className="flex items-center gap-2.5 text-lead text-label">
            <span className="min-w-0 truncate">{watchLine(w, services)}</span>
            {w.service !== EXHIBIT && <ServiceMark service={w.service} size="line" />}
          </span>
        ))}
        {watches.length > WATCHES && (
          <span className="text-body text-label-dim">{t("activity.moreWatches", { count: watches.length - WATCHES })}</span>
        )}
        {(watches.length === 0 || stopped) && <span className="text-lead text-label">{historyLine(detail)}</span>}
        {note && <span className="text-body text-label-dim">{note}</span>}
      </dd>
    </div>
  );
}

/** How the work was named: "Matched by hand", or a doubt to settle. */
function matchLine(detail: ItemDetail): { value: string; note?: string } | null {
  const { match_state, match_source, tmdb_id } = detail.item;
  if (match_state === "needsreview") return { value: t("details.notSure"), note: t("details.fixInMenu") };
  if (match_state === "manual" || match_source === "manual") return { value: t("details.byHand") };
  if (tmdb_id !== null) return { value: t("details.automatically") };
  return null;
}

/** One line of the record: a quiet term, its value, and a dimmer note under it. */
function Line({ term, value, note }: { term: string; value: string | null; note?: string | null }) {
  if (!value) return null;
  return (
    <div className="contents">
      <dt className="pt-0.5 font-display text-body text-label-faint">{term}</dt>
      <dd className="flex min-w-0 flex-col gap-1 font-display">
        <span className="text-lead text-label">{value}</span>
        {note && <span className="truncate text-body text-label-dim">{note}</span>}
      </dd>
    </div>
  );
}

const Terms = ({ children }: { children: ReactNode }) => (
  <dl className="grid grid-cols-[var(--details-term)_minmax(0,1fr)] gap-x-6 gap-y-5">{children}</dl>
);

function Record({ detail, sources }: { detail: ItemDetail; sources: Source[] }) {
  const t = useT();
  const files = detail.files;
  const first = files.find((f) => f.tech !== null) ?? files[0];
  const tech = first?.tech ?? null;
  const tracks = trackLines(tech);
  const sound = tech ? soundMark(tech.audio) : null;
  const match = matchLine(detail);
  const added = shortDay(detail.item.added_at);
  const where = (f: MediaFile) => {
    const source = sources.find((s) => s.id === f.source_id);
    return {
      value: [bytes(f.size), source && t("details.on", { path: source.path })].filter(Boolean).join(" "),
      offline: source?.online === false,
    };
  };

  const reception = detail.reception.length > 0 || detail.awards.length > 0;
  return (
    <div
      className={cn(
        "grid gap-x-(--details-part-gap)",
        reception ? "grid-cols-[minmax(0,1.2fr)_minmax(0,1.2fr)_minmax(0,1fr)]" : "grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)]",
      )}
    >
      {files.length > 0 && (
        <Part title={files.length === 1 ? t("details.theFile") : t("details.theFiles")}>
          <Terms>
            <Line term={t("details.picture")} value={tech ? resolution(tech.height, tech.hdr, tech.width) : null} />
            <Line term={t("details.sound")} value={sound?.name ?? tracks.sound} note={sound ? tracks.sound : null} />
            <Line term={t("details.subtitles")} value={tracks.subtitles} />
            {files.length === 1 ? (
              <div data-file className="contents">
                <Line
                  term={t("details.file")}
                  value={where(files[0]!).value}
                  note={where(files[0]!).offline ? facts([fileName(files[0]!.rel_path), t("work.offline")]) : fileName(files[0]!.rel_path)}
                />
              </div>
            ) : (
              files.map((f, i) => (
                <div key={f.id} data-file className="contents">
                  <Line term={t("details.version", { n: i + 1 })} value={fileLine(f, sources.find((s) => s.id === f.source_id))} note={fileName(f.rel_path)} />
                </div>
              ))
            )}
          </Terms>
        </Part>
      )}
      {reception && <Reception detail={detail} />}
      <Part title={t("activity.history")}>
        <Terms>
          <Watches detail={detail} />
          <Line term={t("details.added")} value={added} />
          {match && <Line term={t("details.match")} value={match.value} note={match.note} />}
        </Terms>
      </Part>
    </div>
  );
}

/**
 * What the public made of the work: each score with its origin's mark, its number and how many
 * voted, and the page it comes from written out (the TV opens no browser, so the source is named
 * where it is shown); then every award kept, by year, wins before nominations.
 */
function Reception({ detail }: { detail: ItemDetail }) {
  const t = useT();
  const services = useServices().data;
  const scores = orderedScores(detail.reception, services);
  const years = awardsByYear(detail.awards);
  return (
    <Part title={t("reception.title")}>
      {scores.length > 0 && (
        <Terms>
          {scores.map(({ score, origin }) => (
            <div key={score.origin} data-score={score.origin} className="contents">
              <dt className="flex items-center gap-2 pt-0.5 font-display text-body text-label-faint">
                <OriginMark origin={origin} size="line" />
                <span className="truncate">{origin.name}</span>
              </dt>
              <dd className="flex min-w-0 flex-col gap-1 font-display">
                <span className="flex items-center gap-3 text-lead">
                  <ScoreValue score={score} origin={origin} />
                  {score.certified && <span className="text-body text-label-dim">{t("reception.certified")}</span>}
                </span>
                {(score.votes !== null || score.url !== null) && (
                  <span className="truncate text-body text-label-dim">{facts([votesText(score.votes), pageOf(score.url)])}</span>
                )}
              </dd>
            </div>
          ))}
        </Terms>
      )}
      {years.length > 0 && (
        <Terms>
          {years.map(({ year, awards }) => (
            <div key={year ?? "undated"} className="contents">
              <dt className="pt-0.5 font-display text-body text-label-faint tabular-nums">{year ?? t("reception.undated")}</dt>
              <dd className="flex min-w-0 flex-col gap-1.5 font-display text-body">
                {awards.map((a) => (
                  <span key={`${a.award}:${a.year}`} data-award={a.won ? "won" : "nominated"} className={a.won ? "text-label" : "text-label-dim"}>
                    {facts([a.name, bodyName(a.body, services)])}
                    {!a.won && <span className="text-label-faint">{` (${t("reception.nominated")})`}</span>}
                  </span>
                ))}
              </dd>
            </div>
          ))}
        </Terms>
      )}
    </Part>
  );
}
