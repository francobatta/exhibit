// The one list of what can be done to a work. The item menu, the details screen and the wall's
// panel all show `itemActions(item)` and run the chosen one through `runAction`, so a label or a
// rule changes in one place.

import { useMemo } from "react";
import { confirm, type ConfirmRequest } from "@/components/Dialog";
import { openMatchPicker } from "@/components/MatchPicker";
import { toast } from "@/components/ui/toast";
import { leaveAsk, useNav } from "@/nav/stack";
import { plain, type Id, type ItemDetail, type ItemSummary } from "./api";
import { clock, episodeLabel } from "./format";
import { usePlayback } from "./playback";
import { useMoveToOther, useSetLoved, useSetWatched } from "./queries";

export type ActionId =
  | "play"
  | "resume"
  | "restart"
  | "watched"
  | "unwatched"
  | "love"
  | "unlove"
  | "details"
  | "fix"
  | "other";

export interface Action {
  id: ActionId;
  label: string;
  /** Why it cannot run now; absent when it can. */
  disabled?: string;
}

export type ActionItem = ItemSummary | ItemDetail;

export const OFFLINE = "Drive is not connected";

/** Why a work cannot play: its drive by the folder it holds, when known. */
export function offlineReason(drive: string | undefined): string {
  return drive === undefined ? OFFLINE : `The drive for ${drive} is not connected`;
}

/** Why `action` cannot run, a missing drive named by `drive`; undefined when it can. */
export function reasonOf(action: Pick<Action, "disabled">, drive: string | undefined): string | undefined {
  return action.disabled === OFFLINE ? offlineReason(drive) : action.disabled;
}

const isDetail = (item: ActionItem): item is ItemDetail => "item" in item;

/** The fields every action needs, from either shape. */
export function facetsOf(item: ActionItem) {
  if (isDetail(item)) {
    const play = item.play;
    return {
      id: item.item.id,
      kind: item.item.kind,
      title: item.item.title,
      available: item.available,
      loved: item.item.loved,
      watched: play?.watched ?? false,
      inProgress: play !== null && !play.watched && play.position_s > 0,
      resumeAt: play !== null && play.position_s > 0 ? play.position_s : null,
      matchState: item.item.match_state,
      seen: seenOf(item.children.length > 0 && item.children.every((c) => c.watched), item.children.some((c) => c.watched || (c.progress ?? 0) > 0)),
    };
  }
  return {
    id: item.id,
    kind: item.kind,
    title: item.title,
    available: item.available,
    loved: item.loved,
    watched: item.watched,
    inProgress: !item.watched && item.progress !== null && item.progress > 0,
    resumeAt: null,
    matchState: item.match_state,
    seen: seenOf(item.watched, (item.progress ?? 0) > 0),
  };
}

/** How much of a show or season has been watched: none of it, some, or all. */
function seenOf(all: boolean, any: boolean): "none" | "some" | "all" {
  return all ? "all" : any ? "some" : "none";
}

function playLabel(item: ActionItem): string {
  const { kind } = facetsOf(item);
  if (kind !== "show" && kind !== "season") return "Play";
  const next = isDetail(item) ? item.next_episode : null;
  const label = next ? episodeLabel(next.season, next.episode) : "";
  return label ? `Play ${label}` : "Play next episode";
}

/** What can be done to `item`, the main action first. */
export function itemActions(item: ActionItem): Action[] {
  const f = facetsOf(item);
  const offline = f.available ? {} : { disabled: OFFLINE };
  const list: Action[] = [];
  const container = f.kind === "show" || f.kind === "season";

  if (!container && f.inProgress) {
    list.push({
      id: "resume",
      label: f.resumeAt !== null ? `Resume from ${clock(f.resumeAt)}` : "Resume",
      ...offline,
    });
    list.push({ id: "restart", label: "Play from the start", ...offline });
  } else {
    list.push({ id: "play", label: playLabel(item), ...offline });
  }
  // A season is only ever seen on its show's screen, which is its details.
  if (f.kind !== "season") list.push({ id: "details", label: "Details" });
  if (container) {
    if (f.seen !== "all") list.push({ id: "watched", label: "Mark all as watched" });
    if (f.seen !== "none") list.push({ id: "unwatched", label: "Mark all as not watched" });
  } else {
    list.push(
      f.watched
        ? { id: "unwatched", label: "Mark as not watched" }
        : { id: "watched", label: "Mark as watched" },
    );
  }
  list.push(f.loved ? { id: "unlove", label: "Remove from loved" } : { id: "love", label: "Love" });
  if (f.kind === "movie" || f.kind === "show" || f.kind === "other") {
    list.push({ id: "fix", label: "Fix match" });
  }
  if (f.kind === "movie" || f.kind === "show") list.push({ id: "other", label: "Send to Other" });
  return list;
}

/** What A does on a work: a show (or season) opens; anything else plays or resumes. */
export function mainAction(item: ActionItem): Action {
  const actions = itemActions(item);
  const { kind } = facetsOf(item);
  const container = kind === "show" || kind === "season";
  return (container ? actions.find((a) => a.id === "details") : undefined) ?? actions[0]!;
}

/** Runs A on `item`; one that cannot run says why. */
export function runMain(item: ActionItem, ctx: ActionContext): void {
  const action = mainAction(item);
  if (action.disabled) ctx.fail(action.disabled);
  else void runAction(action, item, ctx);
}

/** What `runAction` needs from the app: the library mutations, navigation and two dialogs. */
export interface ActionContext {
  play(v: { id: Id; resume: boolean }): Promise<unknown>;
  setWatched(v: { id: Id; watched: boolean }): Promise<unknown>;
  setLoved(v: { id: Id; loved: boolean }): Promise<unknown>;
  moveToOther(v: { id: Id }): Promise<unknown>;
  /** Opens the details (or show) screen. */
  open(item: ActionItem): void;
  /** Opens the match picker. */
  fix(item: ActionItem): void;
  /** Asks before something that moves a work; resolves true to go ahead. */
  confirm(request: ConfirmRequest): Promise<boolean>;
  /** Says what went wrong, in plain words. */
  fail(message: string): void;
}

/** Runs `action` on `item`. Never throws: a failure goes to `ctx.fail`. */
export async function runAction(action: Action, item: ActionItem, ctx: ActionContext): Promise<void> {
  if (action.disabled) return;
  const f = facetsOf(item);
  const id = f.id;
  try {
    switch (action.id) {
      case "play":
      case "restart":
        await ctx.play({ id, resume: false });
        return;
      case "resume":
        await ctx.play({ id, resume: true });
        return;
      case "watched":
      case "unwatched":
        await ctx.setWatched({ id, watched: action.id === "watched" });
        return;
      case "love":
      case "unlove":
        await ctx.setLoved({ id, loved: action.id === "love" });
        return;
      case "details":
        ctx.open(item);
        return;
      case "fix":
        ctx.fix(item);
        return;
      case "other": {
        const go = await ctx.confirm({
          title: `Send ${f.title} to Other?`,
          body: "It leaves Movies and Shows and hangs with home videos and clips. Fix match brings it back.",
          confirm: "Send to Other",
        });
        if (go) await ctx.moveToOther({ id });
        return;
      }
    }
  } catch (e) {
    ctx.fail(plain(e));
  }
}

/**
 * The app's `ActionContext`: the real mutations, the navigation stack (once Ask, if open, has
 * closed), the match picker, the confirm dialog, a toast.
 */
export function useActionContext(): ActionContext {
  const start = usePlayback((s) => s.start);
  const watched = useSetWatched();
  const loved = useSetLoved();
  const other = useMoveToOther();
  const push = useNav((s) => s.push);
  return useMemo(
    () => ({
      play: ({ id, resume }) => start(id, resume),
      setWatched: watched.mutateAsync,
      setLoved: loved.mutateAsync,
      moveToOther: other.mutateAsync,
      open: (item) => {
        const f = facetsOf(item);
        leaveAsk(() => push({ screen: f.kind === "show" ? "show" : "details", id: f.id }));
      },
      fix: (item) => openMatchPicker(facetsOf(item).id),
      confirm,
      fail: (message) => toast(message),
    }),
    [start, watched.mutateAsync, loved.mutateAsync, other.mutateAsync, push],
  );
}
