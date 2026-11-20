// A work's own room, in two movements. The first is the work: its backdrop, its placard in full,
// and its row of cues as on Home (components/Cues). The rest of what can be done waits in the
// item menu (X). Below, peeking in, the
// second: who is in it, as portraits, then its record, the file and its life in Exhibit. Down
// glides from one movement to the other; the backdrop darkens as it goes.

import { motion } from "motion/react";
import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { useScreenBackdrop } from "@/components/Backdrop";
import { Cues } from "@/components/Cues";
import { Placard, placardLines, trackLines } from "@/components/Placard";
import { ErrorState, Loading } from "@/components/states";
import { FocusGroup, useFocusItem } from "@/focus/react";
import { imageUrl, plain, type CastMember, type Id, type ItemDetail, type MediaFile, type Source } from "@/lib/api";
import { OFFLINE, bytes, clock, facts, language, listed, resolution, shortDay } from "@/lib/format";
import { soundMark } from "@/lib/marks";
import { focusSpring, glideScroll, numberToken, remPx, rems, stopGlide } from "@/lib/motion";
import { useCast, useItem, useItemSources } from "@/lib/queries";
import { cn } from "@/lib/utils";
import { openAsk, useNav } from "@/nav/stack";

export function Details({ id }: { id: Id }) {
  const query = useItem(id);
  if (query.isPending) return <Loading />;
  if (query.isError) {
    return (
      <ErrorState
        message={plain(query.error)}
        hint="It may have gone in the last scan. Go back to the collection."
        action={{ label: "Go back", onAction: () => useNav.getState().back() }}
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

  const scroller = useRef<HTMLDivElement>(null);
  const second = useRef<HTMLElement>(null);
  const [below, setBelow] = useState(false);
  const belowNow = useRef(below);
  // The second movement stands just under the bar.
  const anchor = (down: boolean) => (down && second.current !== null ? second.current.offsetTop - rems("--space-bar") * remPx() : 0);
  const glide = (down: boolean) => {
    const el = scroller.current;
    if (el === null) return;
    belowNow.current = down;
    setBelow(down);
    glideScroll(el, "top", anchor(down));
  };
  // A resized window (full screen, another monitor) keeps the movement in view where it stands.
  useEffect(() => {
    const el = scroller.current;
    if (el === null) return;
    const kept = new ResizeObserver(() => {
      stopGlide(el);
      el.scrollTop = anchor(belowNow.current);
    });
    kept.observe(el);
    return () => {
      kept.disconnect();
      stopGlide(el);
    };
  }, []);
  // Up from below always comes back to the work first: to its main cue, from the cast (which the
  // nearest-item search would pass over, the cues being under the bar), or, with no cast, to the
  // cues' own place.
  // The cast takes focus only once its movement is in view (it is inert while it peeks, so the
  // nearest-item search from the bar cannot land on it): Down from the cues brings it in, then
  // lights the portrait lit last.
  const lastCast = useRef<string | null>(null);
  const toCast = useRef(false);
  useEffect(() => {
    if (!below || !toCast.current) return;
    toCast.current = false;
    const at = lastCast.current === null ? null : document.querySelector<HTMLElement>(`[data-focus-key="${lastCast.current}"]`);
    (at ?? document.querySelector<HTMLElement>("[data-focus-group='details-cast'] [data-focus-key]"))?.focus({ preventScroll: true });
  }, [below]);
  const upFirst = (e: KeyboardEvent<HTMLElement>) => {
    if (!below || e.key !== "ArrowUp") return;
    const fromCast = (e.target as HTMLElement).closest("[data-focus-group='details-cast']") !== null;
    if (!fromCast && cast.length > 0) return;
    e.preventDefault();
    e.stopPropagation();
    if (fromCast) document.querySelector<HTMLElement>("[data-focus-key='action:main']")?.focus({ preventScroll: true });
    glide(false);
  };

  return (
    // The wheel moves through the cues and the cast, through the arrow keys (focus/input/wheel.ts).
    <div data-wheel-steps className="details absolute inset-0 overflow-clip" onKeyDownCapture={upFirst}>
      <div aria-hidden className="details-shade pointer-events-none absolute inset-0" />
      <div ref={scroller} className="details-scroller absolute inset-0 overflow-y-hidden">
        <section
          aria-label={item.title}
          className="flex h-[calc(100%-var(--details-peek))] flex-col justify-end px-edge pt-bar pb-(--details-foot)"
        >
          <div className="flex max-w-measure flex-col gap-5">
            <Placard item={item} detail={detail} size="hero" full more={[facts(item.genres)]} />
            <Cues
              id="details-cues"
              prefix="action"
              item={detail}
              drive={drive}
              progress={placardLines(item, detail).progress}
              onFocusCue={() => glide(false)}
              onEdge={(dir) => {
                if (dir[1] !== 1) return;
                toCast.current = cast.length > 0;
                glide(true);
              }}
            />
          </div>
        </section>
        <section ref={second} className="flex min-h-full flex-col gap-(--details-part-gap) px-edge pb-edge">
          {cast.length > 0 && (
            <Cast
              cast={cast}
              inert={!below}
              onFocus={(key) => {
                lastCast.current = key;
                glide(true);
              }}
            />
          )}
          <Record detail={detail} sources={sources} />
        </section>
      </div>
    </div>
  );
}

/** A part of the second movement, named quietly above it. */
function Part({ title, children, inert }: { title: string; children: ReactNode; inert?: boolean }) {
  return (
    <section aria-label={title} inert={inert} className="flex flex-col gap-6">
      <h2 className="font-display text-caption font-ui tracking-[0.14em] text-label-faint uppercase">{title}</h2>
      {children}
    </section>
  );
}

function Cast({ cast, inert, onFocus }: { cast: CastMember[]; inert: boolean; onFocus: (key: string) => void }) {
  return (
    <Part title="Cast" inert={inert}>
      <FocusGroup
        id="details-cast"
        orientation="horizontal"
        reveals
        aria-label="Cast"
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

const initials = (name: string) =>
  name
    .split(/\s+/)
    .filter(Boolean)
    .map((w) => w[0])
    .slice(0, 2)
    .join("");

/** One of the cast: A looks for them across the library, in Ask. */
function Portrait({ member }: { member: CastMember }) {
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({
    key: `cast:${member.name}`,
    onSelect: () => openAsk(member.name),
    hint: "Find",
  });
  const src = imageUrl(member.portrait);
  const [failed, setFailed] = useState(false);
  const spring = focusSpring();
  return (
    <button
      ref={ref}
      {...props}
      type="button"
      aria-label={facts([member.name, member.character && `as ${member.character}`])}
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
  const names = [...new Set(codes.map(language).filter((n) => n !== null))];
  const rest = names.length - 3;
  return listed(rest > 0 ? [...names.slice(0, 3), `${rest} more`] : names);
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
    subs && `subtitles ${subs}`,
    source?.path,
    source?.online === false ? OFFLINE : null,
  ]);
}

/** How far the owner got: "Watched 3 March", "Stopped at 1:04:12 on 2 March", "Never played". */
export function historyLine(detail: ItemDetail): string {
  const play = detail.play;
  const when = play === null ? null : shortDay(play.last_played_at);
  if (play?.watched) return when ? `Watched ${when}` : "Watched";
  if (play !== null && play.position_s > 0) {
    return when ? `Stopped at ${clock(play.position_s)} on ${when}` : `Stopped at ${clock(play.position_s)}`;
  }
  return "Never played";
}

/** How the work was named: "Matched by hand", or a doubt to settle. */
function matchLine(detail: ItemDetail): { value: string; note?: string } | null {
  const { match_state, match_source, tmdb_id } = detail.item;
  if (match_state === "needsreview") return { value: "Not sure this is the right title", note: "Fix match is in the menu" };
  if (match_state === "manual" || match_source === "manual") return { value: "Matched by hand" };
  if (tmdb_id !== null) return { value: "Matched automatically" };
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
      value: [bytes(f.size), source && `on ${source.path}`].filter(Boolean).join(" "),
      offline: source?.online === false,
    };
  };

  return (
    <div className="grid grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)] gap-x-(--details-part-gap)">
      {files.length > 0 && (
        <Part title={files.length === 1 ? "The file" : "The files"}>
          <Terms>
            <Line term="Picture" value={tech ? resolution(tech.height, tech.hdr, tech.width) : null} />
            <Line term="Sound" value={sound?.name ?? tracks.sound} note={sound ? tracks.sound : null} />
            <Line term="Subtitles" value={tracks.subtitles} />
            {files.length === 1 ? (
              <div data-file className="contents">
                <Line
                  term="File"
                  value={where(files[0]!).value}
                  note={where(files[0]!).offline ? `${fileName(files[0]!.rel_path)}, ${OFFLINE.toLowerCase()}` : fileName(files[0]!.rel_path)}
                />
              </div>
            ) : (
              files.map((f, i) => (
                <div key={f.id} data-file className="contents">
                  <Line term={`Version ${i + 1}`} value={fileLine(f, sources.find((s) => s.id === f.source_id))} note={fileName(f.rel_path)} />
                </div>
              ))
            )}
          </Terms>
        </Part>
      )}
      <Part title="In Exhibit">
        <Terms>
          <Line term="Watched" value={historyLine(detail)} />
          <Line term="Added" value={added} />
          {match && <Line term="Match" value={match.value} note={match.note} />}
        </Terms>
      </Part>
    </div>
  );
}
