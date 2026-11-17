// A work's details: its backdrop and full placard (the whole description, who made it and who is
// in it, its genres), what can be done to it as a list (the first one has focus on arrival), then
// what is known about it: its files, how far the owner got, and how it was matched.

import { useCallback, useRef } from "react";
import { useScreenBackdrop } from "@/components/Backdrop";
import { openItemMenu } from "@/components/ItemMenu";
import { Lamp } from "@/components/Lamp";
import { Placard } from "@/components/Placard";
import { ErrorState, Loading } from "@/components/states";
import { FocusGroup, useFocusItem } from "@/focus/react";
import { OFFLINE, itemActions, offlineReason, runAction, useActionContext, type Action, type ActionId } from "@/lib/actions";
import { plain, type Id, type ItemDetail, type MediaFile, type Source } from "@/lib/api";
import { bytes, clock, facts, language, listed, resolution, shortDay } from "@/lib/format";
import { useItem, useItemSources } from "@/lib/queries";
import { cn } from "@/lib/utils";
import { useNav } from "@/nav/stack";

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

/** Who is in it: "With Robert De Niro, Jodie Foster, …", the first six. */
const CAST = 6;
function castLine(detail: ItemDetail): string {
  const names = detail.item.people.filter((p) => p.role === "cast").map((p) => p.name).slice(0, CAST);
  return names.length > 0 ? `With ${facts(names)}` : "";
}

function Shown({ detail }: { detail: ItemDetail }) {
  const item = detail.item;
  const sources = useItemSources(detail);
  const drive = sources.find((s) => !s.online)?.path;
  // An episode has no artwork of its own: it hangs on its show's (through its season).
  const parent = useItem(item.backdrop === null ? item.parent_id : null).data?.item;
  const show = useItem(parent?.backdrop === null ? parent.parent_id : null).data?.item;
  useScreenBackdrop(
    item.backdrop ?? parent?.backdrop ?? show?.backdrop ?? null,
    item.poster ?? parent?.poster ?? show?.poster ?? null,
  );
  // Everything but Details itself; a play that cannot run names the drive it waits for.
  const actions = itemActions(detail)
    .filter((a) => a.id !== "details")
    .map((a) => (a.disabled === OFFLINE ? { ...a, disabled: offlineReason(drive) } : a));

  return (
    // The wheel moves through the actions, through the arrow keys (focus/input/wheel.ts).
    <div data-wheel-steps className="absolute inset-0 flex flex-col px-edge pt-bar pb-edge">
      <div className="flex min-h-0 flex-1 flex-col justify-end pb-12">
        <Placard item={item} detail={detail} size="hero" full more={[castLine(detail), facts(item.genres)]} />
      </div>
      <div className="flex items-start gap-20">
        <Actions detail={detail} actions={actions} />
        <Record detail={detail} sources={sources} />
      </div>
    </div>
  );
}

/** One focus key per place in the list, so an action that turns into its opposite keeps focus. */
const SLOT: Record<ActionId, string> = {
  play: "main",
  resume: "main",
  restart: "restart",
  watched: "watched",
  unwatched: "watched",
  love: "love",
  unlove: "love",
  details: "details",
  fix: "fix",
  other: "other",
};

function Actions({ detail, actions }: { detail: ItemDetail; actions: Action[] }) {
  const ctx = useActionContext();
  return (
    <FocusGroup id="details-actions" orientation="vertical" aria-label="Actions" className="flex w-(--details-actions) shrink-0 flex-col">
      {actions.map((a) => (
        <ActionItem
          key={SLOT[a.id]}
          action={a}
          onRun={() => void runAction(a, detail, ctx)}
          onMenu={(el) => openItemMenu(detail, el)}
        />
      ))}
    </FocusGroup>
  );
}

function ActionItem({ action, onRun, onMenu }: { action: Action; onRun: () => void; onMenu: (el: HTMLElement | null) => void }) {
  const el = useRef<HTMLButtonElement | null>(null);
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({
    key: `action:${SLOT[action.id]}`,
    onSelect: onRun,
    onMenu: () => onMenu(el.current),
    disabled: action.disabled !== undefined,
  });
  const bind = useCallback(
    (node: HTMLButtonElement | null) => {
      el.current = node;
      return ref(node);
    },
    [ref],
  );
  return (
    <button
      ref={bind}
      {...props}
      type="button"
      className="flex flex-col items-start gap-0.5 py-2 text-left font-display text-lead font-ui text-label-dim outline-none transition-colors hover:text-label focus:text-label aria-disabled:text-label-faint"
    >
      <span className="relative pb-2">
        {action.label}
        {focused && <Lamp />}
      </span>
      {action.disabled && <span className="-mt-1.5 font-display text-body font-text text-label-dim">{action.disabled}</span>}
    </button>
  );
}

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
    source?.online === false ? "Offline" : null,
  ]);
}

const fileName = (rel: string) => rel.split(/[\\/]/).at(-1) ?? rel;

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

/** How the work was named, when the owner should know: chosen by hand, or still to check. */
function matchLine(detail: ItemDetail): string | null {
  const { match_state, match_source } = detail.item;
  if (match_state === "needsreview") return "Needs review";
  if (match_state === "manual" || match_source === "manual") return "Matched by hand";
  return null;
}

function Record({ detail, sources }: { detail: ItemDetail; sources: Source[] }) {
  const match = matchLine(detail);
  const row = "contents";
  const label = "text-label-faint";
  return (
    <dl className="grid min-w-0 flex-1 grid-cols-[auto_minmax(0,1fr)] gap-x-6 gap-y-2 py-2 font-display text-body">
      {detail.files.length > 0 && (
        <div className={row}>
          <dt className={label}>{detail.files.length === 1 ? "File" : "Files"}</dt>
          <dd className="flex min-w-0 flex-col gap-3">
            {detail.files.map((f) => {
              const source = sources.find((s) => s.id === f.source_id);
              return (
                <div key={f.id} data-file className={cn("flex min-w-0 flex-col", source?.online === false && "opacity-70")}>
                  <span className="truncate text-label">{fileName(f.rel_path)}</span>
                  <span className="truncate text-label-dim">{fileLine(f, source)}</span>
                </div>
              );
            })}
          </dd>
        </div>
      )}
      <div className={row}>
        <dt className={label}>History</dt>
        <dd className="text-label-dim">{historyLine(detail)}</dd>
      </div>
      {match !== null && (
        <div className={row}>
          <dt className={label}>Match</dt>
          <dd className="text-label-dim">{match}</dd>
        </div>
      )}
    </dl>
  );
}
