// A show: its placard on the left over its backdrop; on the right its seasons, one chosen, and that
// season's episodes as a list of wide works. On arrival the next unwatched episode has focus and
// its season is chosen. A plays or resumes an episode; X opens the menu of an episode or a season
// (a season's marks all its episodes). When an episode ends watched, focus moves on to the next.

import { useCallback, useEffect, useLayoutEffect, useRef, useState, type FocusEvent } from "react";
import { useScreenBackdrop } from "@/components/Backdrop";
import { openItemMenu } from "@/components/ItemMenu";
import { Lamp } from "@/components/Lamp";
import { Placard } from "@/components/Placard";
import { ErrorState, Loading } from "@/components/states";
import { Toggle } from "@/components/ui/toggle";
import { Work } from "@/components/Work";
import { focusItem, useFocusState } from "@/focus/bridge";
import { FocusGroup, useFocusItem } from "@/focus/react";
import { runMain, useActionContext } from "@/lib/actions";
import { plain, type Id, type ItemDetail, type ItemSummary } from "@/lib/api";
import { OFFLINE, WATCHED, count, episodeLabel, facts, runtime, started } from "@/lib/format";
import { glideScroll, rems, stopGlide } from "@/lib/motion";
import { usePlayback } from "@/lib/playback";
import { useItem } from "@/lib/queries";
import { useNav } from "@/nav/stack";

export const episodeKey = (id: Id) => `episode:${id}`;
const seasonKey = (id: Id) => `season:${id}`;

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
      <div className="flex w-(--show-placard) shrink-0 flex-col justify-end px-edge pb-edge">
        <Placard
          item={show.item}
          detail={show}
          size="hero"
          more={[seasons.length > 0 ? count(seasons.length, "season", "seasons") : "", facts(show.item.genres)]}
        />
      </div>
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
    </div>
  );
}

function SeasonRow({ seasons, chosen, onChoose }: { seasons: ItemSummary[]; chosen: Id | null; onChoose: (id: Id) => void }) {
  return (
    <FocusGroup
      id="show-seasons"
      orientation="horizontal"
      aria-label="Seasons"
      className="order-first flex h-(--wall-controls) shrink-0 items-center gap-2 pl-4"
    >
      {seasons.map((s) => (
        <SeasonTab key={s.id} season={s} chosen={s.id === chosen} onChoose={() => onChoose(s.id)} />
      ))}
    </FocusGroup>
  );
}

function SeasonTab({ season, chosen, onChoose }: { season: ItemSummary; chosen: boolean; onChoose: () => void }) {
  const el = useRef<HTMLButtonElement | null>(null);
  // Moving onto a season chooses it; A goes down into its episodes.
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({
    key: seasonKey(season.id),
    onMenu: () => openItemMenu(season, el.current),
  });
  const bind = useCallback(
    (node: HTMLButtonElement | null) => {
      el.current = node;
      return ref(node);
    },
    [ref],
  );
  return (
    <Toggle
      ref={bind}
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

/** Where a focused row sits in the list as it scrolls: this far down from the top, as a share of its height. */
const BAND = 0.3;

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
  // Keys and the pad keep the focused row high in the list; a pointer only brings a row it
  // touches fully into view, so the list never slides away from under it.
  const reveal = useCallback((row: HTMLElement) => {
    const el = list.current;
    if (el === null) return;
    const top = row.offsetTop;
    const bottom = top + row.offsetHeight;
    if (useFocusState.getState().mode !== "pointer") {
      glideScroll(el, "top", Math.max(0, top - el.clientHeight * BAND));
    } else if (bottom > el.scrollTop + el.clientHeight) {
      glideScroll(el, "top", bottom - el.clientHeight);
    } else if (top < el.scrollTop) {
      glideScroll(el, "top", top);
    }
  }, []);

  if (episodes.length === 0) {
    return <p className="pt-light font-display text-body text-label-faint">No episodes in this season yet.</p>;
  }
  return (
    <div ref={list} className="wall-fade relative min-h-0 flex-1 overflow-y-auto">
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
  const el = useRef<HTMLButtonElement | null>(null);
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({
    key: episodeKey(episode.id),
    onSelect: () => onOpen(episode),
    onMenu: () => openItemMenu(episode, el.current),
  });
  const bind = useCallback(
    (node: HTMLButtonElement | null) => {
      el.current = node;
      return ref(node);
    },
    [ref],
  );
  const lines = episodeLines(episode);
  return (
    <button
      ref={bind}
      {...props}
      onFocus={(e: FocusEvent<HTMLButtonElement>) => {
        props.onFocus(e);
        if (e.target === e.currentTarget) onReveal(e.currentTarget);
      }}
      type="button"
      className="flex shrink-0 items-center gap-8 pl-4 text-left outline-none"
    >
      <Work item={episode} shape="wide" focused={focused} size={rems("--show-episode")} light="episode-light" />
      <span className="flex min-w-0 flex-1 flex-col gap-1 font-display">
        <span className={focused ? "truncate text-lead font-ui text-label" : "truncate text-lead font-ui text-label-dim"}>
          {lines.title}
        </span>
        {lines.state && <span className="truncate text-body text-label-dim">{lines.state}</span>}
      </span>
    </button>
  );
}
