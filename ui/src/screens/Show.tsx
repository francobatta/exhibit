// A show: its placard on the left over its backdrop, a work in its own right (A runs its first
// action, "Play S1 E3"; X opens its menu); on the right its seasons, one chosen, and that season's
// episodes as a list of wide works. On arrival the next unwatched episode has focus and its season
// is chosen; Left goes to the show. A plays or resumes an episode; X opens the menu of an episode
// or a season (a season's marks all its episodes). When an episode ends watched, focus moves on
// to the next.

import { useCallback, useEffect, useLayoutEffect, useRef, useState, type FocusEvent, type KeyboardEvent } from "react";
import { ActionLine } from "@/components/ActionLine";
import { useScreenBackdrop } from "@/components/Backdrop";
import { openItemMenu, useMenuOpenOnKey } from "@/components/ItemMenu";
import { Lamp } from "@/components/Lamp";
import { Placard } from "@/components/Placard";
import { ErrorState, Loading } from "@/components/states";
import { Toggle } from "@/components/ui/toggle";
import { Work } from "@/components/Work";
import { focusItem, useFocusState } from "@/focus/bridge";
import { FocusGroup, useFocusItem } from "@/focus/react";
import { itemActions, runMain, runSelected, useActionContext } from "@/lib/actions";
import { plain, type Id, type ItemDetail, type ItemSummary } from "@/lib/api";
import { OFFLINE, WATCHED, count, episodeLabel, facts, runtime, started } from "@/lib/format";
import { rems, revealRow, stopGlide } from "@/lib/motion";
import { usePlayback } from "@/lib/playback";
import { useItem, useItemSources } from "@/lib/queries";
import { useNav } from "@/nav/stack";

export const episodeKey = (id: Id) => `episode:${id}`;
const seasonKey = (id: Id) => `season:${id}`;
/** The show's own work, the placard. */
export const SHOW_KEY = "show";

export function Show({ id }: { id: Id }) {
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
  return <Seasons show={query.data} />;
}

/** The episode to land on in a list: the show's next one if it is there, else the first not watched, else the first. */
function landing(episodes: ItemSummary[], next: ItemSummary | null): ItemSummary | undefined {
  return episodes.find((e) => e.id === next?.id) ?? episodes.find((e) => !e.watched) ?? episodes[0];
}

function Seasons({ show }: { show: ItemDetail }) {
  const id = show.item.id;
  const seasons = show.children.filter((c) => c.kind === "season");
  const next = show.next_episode;
  // The next episode's season; a show with no seasons lists its episodes itself.
  const [season, setSeason] = useState<Id | null>(
    () => seasons.find((s) => s.season === next?.season)?.id ?? seasons[0]?.id ?? null,
  );
  const chosen = useItem(season);
  const episodes =
    season === null ? show.children.filter((c) => c.kind === "episode") : (chosen.data?.children ?? null);
  useScreenBackdrop(show.item.backdrop, show.item.poster);

  // Arrival: the next episode takes focus, before the screen's scope settles on its first item.
  const arrived = useRef(false);
  useLayoutEffect(() => {
    if (arrived.current || episodes === null) return;
    arrived.current = true;
    const land = landing(episodes, next);
    if (land !== undefined) focusItem(episodeKey(land.id));
  }, [episodes, next]);

  // After an episode ends watched, its next one (perhaps in the next season) takes focus here.
  const top = useNav((s) => s.current.screen === "show" && s.current.id === id);
  const follow = usePlayback((s) => s.follow);
  const followed = usePlayback((s) => s.followed);
  const target = useItem(top ? follow : null).data;
  useEffect(() => {
    if (!top || follow === null || target?.item.id !== follow) return;
    const home = target.item.parent_id === id ? null : target.item.parent_id;
    if (home !== null && !seasons.some((s) => s.id === home)) return followed();
    // Through its tab, so focus waits there (choosing it) while the season's episodes arrive.
    if (home !== season && home !== null) return void focusItem(seasonKey(home));
    if (episodes?.some((e) => e.id === follow)) {
      focusItem(episodeKey(follow));
      followed();
    }
  }, [top, follow, target, id, seasons, season, episodes, followed]);

  if (episodes === null && !arrived.current) return <Loading />;
  return (
    // The wheel moves through the episodes and seasons, through the arrow keys (focus/input/wheel.ts).
    <div data-wheel-steps className="absolute inset-0 flex pt-bar">
      <div className="show-scrim flex min-w-0 flex-1 flex-col pr-edge pl-12">
        {/* The list comes first in the page, so the screen's focus settles on an episode; shown below the seasons. */}
        {episodes === null ? (
          <Loading />
        ) : (
          <Episodes key={season ?? "show"} episodes={episodes} />
        )}
        {seasons.length > 1 && (
          <SeasonRow seasons={seasons} chosen={season} onChoose={setSeason} />
        )}
      </div>
      {/* After the episodes in the page for the same reason; shown on the left. */}
      <div className="order-first flex w-(--show-placard) shrink-0 flex-col justify-end px-edge pb-edge">
        <ShowTitle show={show} seasons={seasons.length} />
      </div>
    </div>
  );
}

function ShowTitle({ show, seasons }: { show: ItemDetail; seasons: number }) {
  return (
    <FocusGroup id="show-title" orientation="vertical" aria-label="Show">
      <ShowWork show={show} seasons={seasons} />
    </FocusGroup>
  );
}

/** The show as a work: its placard and, under it, what A does on it. */
function ShowWork({ show, seasons }: { show: ItemDetail; seasons: number }) {
  const ctx = useActionContext();
  const action = itemActions(show)[0]!;
  // A show's files are its episodes': a play that cannot run names the next episode's drive.
  const next = useItem(show.next_episode?.id ?? null).data;
  const drive = useItemSources(next).find((s) => !s.online)?.path;
  const { ref, props, focused } = useFocusItem<HTMLDivElement>({
    key: SHOW_KEY,
    onSelect: () => runSelected(action, show, ctx),
    onMenu: (el) => openItemMenu(show, el),
  });
  // The show stays lit under its menu, as a work does.
  const menuOpen = useMenuOpenOnKey(SHOW_KEY);
  const lit = focused || menuOpen;
  return (
    <div
      ref={ref}
      {...props}
      role="button"
      aria-label={show.item.title}
      className="flex max-w-measure flex-col gap-5 outline-none"
    >
      <Placard
        item={show.item}
        detail={show}
        size="hero"
        more={[seasons > 0 ? count(seasons, "season", "seasons") : "", facts(show.item.genres)]}
      />
      <div className="relative w-fit pb-2">
        <ActionLine action={action} drive={drive} active={lit} />
        {lit && <Lamp />}
      </div>
    </div>
  );
}

function SeasonRow({ seasons, chosen, onChoose }: { seasons: ItemSummary[]; chosen: Id | null; onChoose: (id: Id) => void }) {
  return (
    <FocusGroup
      id="show-seasons"
      orientation="horizontal"
      aria-label="Seasons"
      onKeyDownCapture={firstToShow}
      className="order-first flex h-(--wall-controls) shrink-0 items-center gap-2 pl-4"
    >
      {seasons.map((s) => (
        <SeasonTab key={s.id} season={s} chosen={s.id === chosen} onChoose={() => onChoose(s.id)} />
      ))}
    </FocusGroup>
  );
}

function SeasonTab({ season, chosen, onChoose }: { season: ItemSummary; chosen: boolean; onChoose: () => void }) {
  // Moving onto a season chooses it; A goes down into its episodes.
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({
    key: seasonKey(season.id),
    onMenu: (el) => openItemMenu(season, el),
  });
  return (
    <Toggle
      ref={ref}
      {...props}
      onFocus={(e: FocusEvent<HTMLButtonElement>) => {
        props.onFocus(e);
        onChoose();
      }}
      variant="plain"
      pressed={chosen}
      className="relative focus-visible:bg-transparent focus:text-label"
    >
      {season.title.trim() || episodeLabel(season.season, null)}
      {focused && <Lamp />}
    </Toggle>
  );
}

function Episodes({ episodes }: { episodes: ItemSummary[] }) {
  const list = useRef<HTMLDivElement>(null);
  const ctx = useActionContext();
  const latest = useRef(ctx);
  useLayoutEffect(() => {
    latest.current = ctx;
  });
  useEffect(() => {
    const el = list.current;
    return () => {
      if (el !== null) stopGlide(el);
    };
  }, []);

  const open = useCallback((item: ItemSummary) => runMain(item, latest.current), []);
  // The list scrolls itself (`reveals`), as the wall does.
  const reveal = useCallback((row: HTMLElement) => {
    const el = list.current;
    if (el === null) return;
    revealRow(el, row.offsetTop, row.offsetTop + row.offsetHeight, useFocusState.getState().mode === "pointer");
  }, []);

  if (episodes.length === 0) {
    return <p className="pt-light font-display text-body text-label-faint">No episodes in this season yet.</p>;
  }
  return (
    <div ref={list} onKeyDownCapture={toShow} className="wall-fade relative min-h-0 flex-1 overflow-y-auto">
      <FocusGroup
        id="show-episodes"
        orientation="vertical"
        reveals
        aria-label="Episodes"
        className="flex flex-col gap-work-gap py-light"
      >
        {episodes.map((e) => (
          <EpisodeRow key={e.id} episode={e} onOpen={open} onReveal={reveal} />
        ))}
      </FocusGroup>
    </div>
  );
}

/**
 * Left from any episode goes to the show. The placard sits low on the left, so from the top of the
 * list the nearest thing that way would be the top bar.
 */
function toShow(event: KeyboardEvent<HTMLDivElement>) {
  if (event.key !== "ArrowLeft" || event.altKey || event.ctrlKey || event.metaKey) return;
  if (focusItem(SHOW_KEY)) event.preventDefault();
}

/** Left from the first season goes to the show, as from an episode (the top bar lies nearer that way). */
function firstToShow(event: KeyboardEvent<HTMLDivElement>) {
  if (event.target !== event.currentTarget.querySelector("[data-focus-key]")) return;
  toShow(event);
}

/** "E4, The We We Are, 53 min" and, under it, how far the owner got. */
export function episodeLines(e: ItemSummary): { title: string; state: string } {
  const done = started(e.watched, e.progress);
  const left = done !== null && e.runtime_min ? runtime(Math.max(1, Math.round(e.runtime_min * (1 - done)))) : null;
  return {
    title: facts([episodeLabel(null, e.episode), e.title, runtime(e.runtime_min)]),
    state: facts([e.watched ? WATCHED : left && `${left} left`, e.available ? null : OFFLINE]),
  };
}

function EpisodeRow({
  episode,
  onOpen,
  onReveal,
}: {
  episode: ItemSummary;
  onOpen: (item: ItemSummary) => void;
  onReveal: (row: HTMLElement) => void;
}) {
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({
    key: episodeKey(episode.id),
    onSelect: () => onOpen(episode),
    onMenu: (el) => openItemMenu(episode, el),
  });
  // The work stays lit under its menu, as on the shelves.
  const menuOpen = useMenuOpenOnKey(episodeKey(episode.id));
  const lit = focused || menuOpen;
  const lines = episodeLines(episode);
  return (
    <button
      ref={ref}
      {...props}
      onFocus={(e: FocusEvent<HTMLButtonElement>) => {
        props.onFocus(e);
        if (e.target === e.currentTarget) onReveal(e.currentTarget);
      }}
      type="button"
      className="flex shrink-0 items-center gap-8 pl-4 text-left outline-none"
    >
      <Work
        item={episode}
        shape="wide"
        focused={lit}
        size={rems("--show-episode")}
        light="episode-light"
        // The title hangs beside it; with no still, the frame carries the episode's number.
        mark={episodeLabel(null, episode.episode) || undefined}
      />
      <span className="flex min-w-0 flex-1 flex-col gap-1 font-display">
        <span className={lit ? "truncate text-lead font-ui text-label" : "truncate text-lead font-ui text-label-dim"}>
          {lines.title}
        </span>
        {lines.state && <span className="truncate text-body text-label-dim">{lines.state}</span>}
      </span>
    </button>
  );
}
