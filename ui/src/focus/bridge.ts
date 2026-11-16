// The one focus system, framework-free. Real DOM focus is the focus: whatever has
// `document.activeElement` is lit. UI SDK's roving and grid focus move it inside a group; this
// bridge moves it between groups (UI SDK's `nearestInDirection` over the groups' boxes), runs
// Back, swallows Tab, keeps the global keys, and tracks what is focused and how the owner is
// driving. Groups, scopes and items register here; `react.tsx` does that for components.

import { nearestInDirection, type SpatialRect } from "@ui-sdk/core";
import { registerShortcut } from "@ui-sdk/core/shortcuts";
import { create } from "zustand";
import { scrollBehavior } from "@/lib/motion";
import { useNav } from "@/nav/stack";
import { fieldKeeps, isTextField } from "./input/keys";

export type Box = SpatialRect;
export type Dir = readonly [number, number];
export type InputMode = "pad" | "keys" | "pointer";

export interface ItemHandlers {
  key: string;
  onSelect?: () => void;
  onMenu?: () => void;
  onBack?: () => void;
  disabled?: boolean;
}

export interface GroupEntry {
  id: string;
  /** The group's root; its box is the group's box. */
  el: HTMLElement;
  /** Focuses the item to land on when arriving from `from` moving `dir`. False when it has none. */
  enter(from: Box, dir: Dir): boolean;
  /** The element holding the group's tab stop, if it has items. */
  stop(): HTMLElement | undefined;
  /** Its items' elements, in order. */
  items(): HTMLElement[];
  /** Called when an arrow leaves this group and no group lies that way. */
  onEdge?: (dir: Dir) => void;
  /** A virtualised group brings items into view itself (`reveal`), so the bridge does not scroll. */
  reveals?: boolean;
}

export interface ScopeEntry {
  id: string;
  el: HTMLElement;
  modal: boolean;
  onBack?: () => void;
  /** Hears the key of every item focused inside this scope (and not inside a scope within it). */
  onFocusKey?: (key: string) => void;
  /** False while a screen sits under another one. Absent means active. */
  active?: boolean;
  /** Where focus belongs in the scope now: its restoreKey, else fallbackKey, else its first stop. */
  target?: () => HTMLElement | undefined;
  /** Where focus goes when what held it inside the scope is gone: fallbackKey, else the first stop. */
  fallback?: () => HTMLElement | undefined;
}

interface FocusState {
  mode: InputMode;
  /** The focused item's key, or null when focus is not on an item. */
  key: string | null;
  item: ItemHandlers | null;
  /** Whether Back would do something from here. */
  canBack: boolean;
  setMode(mode: InputMode): void;
}

export const useFocusState = create<FocusState>()((set, get) => ({
  mode: "keys",
  key: null,
  item: null,
  canBack: false,
  setMode: (mode) => {
    if (get().mode !== mode) set({ mode });
  },
}));

const groups = new Set<GroupEntry>();
const scopes = new Map<HTMLElement, ScopeEntry>();
const items = new WeakMap<HTMLElement, () => ItemHandlers>();
const edges = new WeakSet<Event>();

export function registerGroup(entry: GroupEntry): () => void {
  groups.add(entry);
  return () => groups.delete(entry);
}

export function registerScope(entry: ScopeEntry): () => void {
  scopes.set(entry.el, entry);
  return () => {
    if (scopes.get(entry.el) === entry) scopes.delete(entry.el);
  };
}

/** Makes `el` a focus item; `handlers` is read on every use, so it can return the latest ones. */
export function registerItem(el: HTMLElement, handlers: () => ItemHandlers): () => void {
  items.set(el, handlers);
  return () => {
    if (items.get(el) === handlers) items.delete(el);
  };
}

/** A group consumed an arrow only to stay at its end; the bridge may still cross with it. */
export function markEdge(event: Event): void {
  edges.add(event);
}

/**
 * The part of `rect` straight ahead of `from` when moving along `dir`: where the two overlap across
 * the movement, only the overlap. UI SDK `nearestInDirection` scores centres inside a 45 degree
 * cone, so a wide group just above a work near the screen's edge (a full-width shelf) has its
 * centre far to the side and loses to a group further away; seen from the work, it is ahead.
 */
function ahead(rect: Box, from: Box, dir: Dir): Box {
  if (dir[0] === 0) {
    const left = Math.max(rect.left, from.left);
    const right = Math.min(rect.right, from.right);
    return left < right ? { ...rect, left, right } : rect;
  }
  const top = Math.max(rect.top, from.top);
  const bottom = Math.min(rect.bottom, from.bottom);
  return top < bottom ? { ...rect, top, bottom } : rect;
}

/** Whether `rect`'s centre lies within 45 degrees of `dir` from `from`'s, as UI SDK measures it. */
function inCone(rect: Box, from: Box, dir: Dir): boolean {
  const x = (rect.left + rect.right) / 2 - (from.left + from.right) / 2;
  const y = (rect.top + rect.bottom) / 2 - (from.top + from.bottom) / 2;
  const forward = x * dir[0] + y * dir[1];
  return forward > 0 && Math.abs(dir[0] ? y : x) <= forward;
}

/**
 * The id of the group nearest `from` in `dir`, or undefined at a true edge. Pure. Only a group
 * that lies that way counts: at the end of a shelf nothing is to the right, so Right stays rather
 * than jumping to the top bar far above.
 */
export function crossMove(
  candidates: Array<{ id: string; rect: Box }>,
  from: Box,
  dir: Dir,
): string | undefined {
  const straight = candidates
    .map((g) => ({ key: g.id, rect: ahead(g.rect, from, dir) }))
    .filter((g) => inCone(g.rect, from, dir));
  return nearestInDirection(from, straight, dir);
}

const live = (el: Element) => el.isConnected && el.closest("[inert]") === null;

function inDocumentOrder<T extends { el: Element }>(list: T[]): T[] {
  return list.sort((a, b) =>
    a.el.compareDocumentPosition(b.el) & Node.DOCUMENT_POSITION_FOLLOWING ? -1 : 1,
  );
}

/** The topmost open modal scope: the one opened last that is still on the page. */
function topModal(): ScopeEntry | undefined {
  let top: ScopeEntry | undefined;
  for (const s of scopes.values()) if (s.modal && s.el.isConnected) top = s;
  return top;
}

/** Whether a modal scope is open. */
export function modalOpen(): boolean {
  return topModal() !== undefined;
}

/** The screen the owner is on: the last active scope that is not modal and not inert. */
function activeScreen(): ScopeEntry | undefined {
  let top: ScopeEntry | undefined;
  for (const s of scopes.values()) if (!s.modal && s.active !== false && live(s.el)) top = s;
  return top;
}

/** The registered groups inside `within`, in document order, inert or not. */
export function groupsIn(within: Element): GroupEntry[] {
  return inDocumentOrder([...groups].filter((g) => g.el.isConnected && within.contains(g.el)));
}

/**
 * Where focus goes back to when an overlay opened from `opener` closes, captured as it opens:
 * the opener; if it is gone, the item that took its place in its group, else the group's last
 * item; if the group is gone too, the opener's scope's fallback. Elements need only be on the
 * page, since the page is still inert under the closing overlay when this runs.
 */
export function returnTarget(opener: Element | null): () => HTMLElement | undefined {
  const el = opener instanceof HTMLElement && opener !== document.body ? opener : undefined;
  const group = el === undefined ? undefined : groupOf(el);
  const index = el === undefined || group === undefined ? -1 : group.items().indexOf(el);
  const scope = el === undefined ? undefined : scopeOf(el);
  return () => {
    if (el?.isConnected) return el;
    if (group !== undefined && groups.has(group) && group.el.isConnected) {
      const now = group.items().filter((i) => i !== el && i.isConnected);
      const pick = (index >= 0 ? now[index] : undefined) ?? now.at(-1);
      if (pick !== undefined) return pick;
    }
    return scope?.el.isConnected ? scope.fallback?.() : undefined;
  };
}

/** No live element in the active scope holds focus: it fell to the page, into an inert layer, or out from under a modal. */
function stranded(): boolean {
  const a = document.activeElement;
  if (a === null || a === document.body || !live(a)) return true;
  const modal = topModal();
  return modal !== undefined && !modal.el.contains(a);
}

/** Puts focus back where the active scope wants it. False when the scope has nowhere to put it. */
function restoreFocus(): boolean {
  const target = (topModal() ?? activeScreen())?.target?.();
  if (target === undefined) return false;
  target.focus({ preventScroll: true });
  return true;
}

/** The live groups a move may reach: inside the topmost modal while one is open, else anywhere. */
export function liveGroups(within?: Element): GroupEntry[] {
  const modal = topModal();
  return inDocumentOrder(
    [...groups].filter(
      (g) =>
        live(g.el) &&
        (modal === undefined || modal.el.contains(g.el)) &&
        (within === undefined || within.contains(g.el)),
    ),
  );
}

function groupOf(el: Element | null): GroupEntry | undefined {
  let best: GroupEntry | undefined;
  for (const g of groups) {
    if (el && g.el.contains(el) && (best === undefined || best.el.contains(g.el))) best = g;
  }
  return best;
}

function scopeOf(el: Element | null): ScopeEntry | undefined {
  for (let n: Element | null = el; n; n = n.parentElement) {
    const s = scopes.get(n as HTMLElement);
    if (s) return s;
  }
  return undefined;
}

/** The focus item `target` is in, if any. */
export function itemAt(target: EventTarget | null): HTMLElement | undefined {
  for (let n = target instanceof Element ? target : null; n; n = n.parentElement) {
    if (n instanceof HTMLElement && items.has(n)) return n;
  }
  return undefined;
}

function handlersOf(el: Element | null): ItemHandlers | undefined {
  const item = itemAt(el);
  return item ? items.get(item)?.() : undefined;
}

/** The focus item with `key` under `root`. */
export function findItem(root: ParentNode, key: string): HTMLElement | undefined {
  for (const el of root.querySelectorAll<HTMLElement>("[data-focus-key]")) {
    if (el.dataset.focusKey === key && items.has(el)) return el;
  }
  return undefined;
}

/** Focuses the item with `key` anywhere live on the page. */
export function focusItem(key: string): boolean {
  const el = findItem(document, key);
  if (el === undefined || !live(el)) return false;
  el.focus();
  return true;
}

const DIRS: Record<string, Dir> = {
  ArrowUp: [0, -1],
  ArrowDown: [0, 1],
  ArrowLeft: [-1, 0],
  ArrowRight: [1, 0],
};

const rectOf = (el: Element): Box => el.getBoundingClientRect();
const hasArea = (b: Box) => b.right > b.left || b.bottom > b.top;

function moveAcross(dir: Dir): boolean {
  const active = document.activeElement;
  const from = groupOf(active);
  const reachable = liveGroups();
  if (from === undefined) {
    // Focus was lost (or never placed): an arrow enters the first group there is.
    for (const g of reachable) if (g.enter(rectOf(g.el), dir)) return true;
    return false;
  }
  const origin = rectOf(itemAt(active) ?? from.el);
  const others = reachable.filter(
    (g) => g !== from && !g.el.contains(from.el) && !from.el.contains(g.el),
  );
  const tried = new Set<string>();
  // A group with nothing in it yet is skipped in favour of the next one that way.
  for (;;) {
    const candidates = others
      .filter((g) => !tried.has(g.id))
      .map((g) => ({ id: g.id, rect: rectOf(g.el), g }))
      .filter((c) => hasArea(c.rect));
    const id = crossMove(candidates, origin, dir);
    if (id === undefined) break;
    tried.add(id);
    const target = candidates.find((c) => c.id === id)!.g;
    if (target.enter(origin, dir)) return true;
  }
  from.onEdge?.(dir);
  return false;
}

/** Back from where focus is: the item's onBack, else its scope's, else the navigation stack. */
export function goBack(): void {
  const active = document.activeElement;
  const item = handlersOf(active);
  if (item?.onBack) return item.onBack();
  const scope = scopeOf(active) ?? topModal();
  if (scope?.onBack) return scope.onBack();
  if (scope?.modal) return;
  useNav.getState().back();
}

function computeCanBack(): boolean {
  const active = document.activeElement;
  if (handlersOf(active)?.onBack) return true;
  const scope = scopeOf(active) ?? topModal();
  if (scope?.onBack) return true;
  if (scope?.modal) return false;
  return useNav.getState().entries.length > 1;
}

/** Opens the focused item's menu, if it has one. */
export function openMenu(): void {
  handlersOf(document.activeElement)?.onMenu?.();
}

function onKeyDown(event: KeyboardEvent) {
  if (event.key === "Tab") {
    event.preventDefault();
    return;
  }
  if (event.defaultPrevented && !edges.has(event)) return;
  const dir = DIRS[event.key];
  const acts =
    dir !== undefined ||
    event.key === "Escape" ||
    event.key === "Enter" ||
    (event.key === "Backspace" && !isTextField(event.target));
  // Focus is never stranded: the first key brings it back, and that is all the key does.
  if (acts && stranded() && restoreFocus()) {
    event.preventDefault();
    return;
  }
  if (dir !== undefined) {
    if (event.altKey || event.ctrlKey || event.metaKey || fieldKeeps(event)) return;
    if (moveAcross(dir)) event.preventDefault();
    return;
  }
  if (event.key === "Escape" || (event.key === "Backspace" && !isTextField(event.target))) {
    event.preventDefault();
    goBack();
  }
}


function onFocusIn(event: FocusEvent) {
  const el = itemAt(event.target);
  const state = useFocusState.getState();
  if (el === undefined) {
    useFocusState.setState({ key: null, item: null, canBack: computeCanBack() });
    return;
  }
  const item = items.get(el)!();
  useFocusState.setState({ key: item.key, item, canBack: computeCanBack() });
  scopeOf(el)?.onFocusKey?.(item.key);
  if (state.mode !== "pointer" && !groupOf(el)?.reveals) {
    el.scrollIntoView({ block: "nearest", inline: "nearest", behavior: scrollBehavior() });
  }
}

function onFocusOut(event: FocusEvent) {
  if (event.relatedTarget === null) {
    useFocusState.setState({ key: null, item: null, canBack: computeCanBack() });
  }
}

/** Listens on the document; returns the uninstaller. */
export function installBridge(): () => void {
  document.addEventListener("keydown", onKeyDown);
  document.addEventListener("focusin", onFocusIn);
  document.addEventListener("focusout", onFocusOut);
  const offNav = useNav.subscribe(() =>
    useFocusState.setState({ canBack: computeCanBack() }),
  );
  return () => {
    document.removeEventListener("keydown", onKeyDown);
    document.removeEventListener("focusin", onFocusIn);
    document.removeEventListener("focusout", onFocusOut);
    offNav();
  };
}

/**
 * The global keys, through UI SDK's shared shortcuts: `/` and Ctrl+K ask, `[` and `]` change
 * section, `m` opens the focused item's menu. Plain keys never fire inside a text field, and a
 * UI SDK modal pauses them, as page shortcuts.
 */
export function installShortcuts(actions: {
  onAsk: () => void;
  onSection: (delta: -1 | 1) => void;
}): () => void {
  const offs = [
    registerShortcut("/", () => actions.onAsk(), { description: "Ask" }),
    registerShortcut("mod+k", () => actions.onAsk(), { description: "Ask" }),
    registerShortcut("[", () => actions.onSection(-1), { description: "Previous section" }),
    registerShortcut("]", () => actions.onSection(1), { description: "Next section" }),
    registerShortcut("m", openMenu, { description: "Item menu" }),
  ];
  return () => {
    for (const off of offs) off();
  };
}
