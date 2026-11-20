// The focus system for components: scopes (one per screen and per overlay), groups (one tab stop
// each, arrows inside them owned by UI SDK's roving focus), items, and the root that installs the
// bridge and the input devices. See bridge.ts for movement between groups.

import {
  createFocusRecovery,
  createRovingFocus,
  type RovingFocusMoveContext,
  definePrimitive,
  keepFocus,
  nearestInDirection,
  spatialMove,
} from "@ui-sdk/core";
import { Dialog, DialogBackdrop, DialogContent } from "@ui-sdk/react/dialog";
import { usePart, usePrimitive, type PartProps, type PrimitiveHandle } from "@ui-sdk/react/primitive";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type FocusEvent,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import {
  findItem,
  goBack,
  groupsIn,
  installBridge,
  installShortcuts,
  itemAt,
  liveGroups,
  markEdge,
  menuButton,
  registerGroup,
  registerItem,
  registerScope,
  returnTarget,
  useFocusState,
  type Box,
  type Dir,
  type InputMode,
  type ScopeEntry,
} from "./bridge";
import { startGamepad } from "./input/gamepad";
import { fieldKeeps, startKeys } from "./input/keys";
import { startPointer } from "./input/pointer";
import { startWheel } from "./input/wheel";

export type Orientation = "horizontal" | "vertical" | "spatial";

interface ItemProps {
  disabled?: boolean;
}

/** Where an arrow goes inside a group: a key, maybe of an item not rendered yet, or undefined to let it go. */
export type GroupMove = (event: globalThis.KeyboardEvent, context: RovingFocusMoveContext<ItemProps>) => string | undefined;

interface GroupOptions {
  orientation?: Orientation;
  move?: GroupMove;
  remember?: boolean;
  /** Where focus goes when the group empties under it: the scope's fallback. */
  fallback?: () => HTMLElement | undefined;
}

const focusGroup = definePrimitive({
  name: "focus-group",
  options: { orientation: "string", remember: "boolean", fallback: "value", move: "value" },
  state: (_options: GroupOptions) => ({ stop: null as string | null }),
  api: (context) => {
    const { orientation, remember = true, move } = context.options();
    const roving = createRovingFocus<{ stop: string | null }, ItemProps>(context, {
      slot: "item",
      key: "stop",
      orientation: () => (context.options().orientation === "vertical" ? "vertical" : "horizontal"),
      remember,
      // No `disabled`: it would hand the stop past an unavailable action, so a screen would open
      // on the next one ("Mark watched" under an offline Play) and A would run it. Focus lands
      // where the layout puts it, and a disabled item says why it can't run.
      ...(move !== undefined
        ? { move: (event: globalThis.KeyboardEvent, c: RovingFocusMoveContext<ItemProps>) => context.options().move?.(event, c) }
        : orientation === "spatial"
          ? { move: spatialMove() }
          : {}),
    });
    // The order as focus last saw it, so a removed item's place can be found afterwards.
    let order: string[] = [];
    const recovery = createFocusRecovery<ItemProps>({
      fallback: (lost) => {
        // An item back under the same key (its element re-created or re-attached as the list
        // re-rendered) was not lost: focus goes to it.
        const back = roving.items().find((i) => i.key === lost.key && i.el.isConnected);
        if (back !== undefined) return back.el;
        const now = roving.items().filter((i) => i.key !== lost.key && i.el.isConnected);
        const at = order.indexOf(lost.key);
        return (at >= 0 ? now[at] : undefined)?.el ?? now.at(-1)?.el ?? context.options().fallback?.();
      },
    });
    return {
      roving,
      recovery,
      snapshot: () => {
        order = roving.items().map((i) => i.key);
      },
    };
  },
  slots: (slot) => ({ root: slot({}) }),
});

type GroupHandle = PrimitiveHandle<typeof focusGroup>;

const GroupContext = createContext<GroupHandle | null>(null);
const ScopeContext = createContext<{ fallback: () => HTMLElement | undefined } | null>(null);

function stopOf(group: GroupHandle): HTMLElement | undefined {
  const items = group.api.roving.items();
  const stop = group.instance.get().stop;
  return (items.find((i) => i.key === stop) ?? items[0])?.el;
}

function centre(b: Box): [number, number] {
  return [(b.left + b.right) / 2, (b.top + b.bottom) / 2];
}

/**
 * Where a move up or down is measured from: a box wider than every item it moves to (a search
 * field over a shelf) counts from its start, an item's width of it, so Down lands on the first
 * work under the field and not the one under its middle.
 */
function startOf(from: Box, boxes: Array<{ rect: Box }>, dir: Dir): Box {
  if (dir[1] === 0) return from;
  const widest = Math.max(...boxes.map((b) => b.rect.right - b.rect.left));
  if (from.right - from.left <= widest) return from;
  return { left: from.left, top: from.top, right: from.left + widest, bottom: from.bottom };
}

/**
 * Lands in a group: its `entry` item when it names one, else its remembered item, else the one
 * nearest `from` that way, else nearest on the cross axis.
 */
function enter(group: GroupHandle, useStop: boolean, from: Box, dir: Dir, entry?: string): boolean {
  const items = group.api.roving.items();
  if (items.length === 0) return false;
  const stop = group.instance.get().stop;
  let key = entry !== undefined && items.some((i) => i.key === entry) ? entry : undefined;
  key ??= useStop && items.some((i) => i.key === stop) ? stop! : undefined;
  if (key === undefined) {
    const boxes = items.map((i) => ({ key: i.key, rect: i.el.getBoundingClientRect() as Box }));
    key = nearestInDirection(startOf(from, boxes, dir), boxes, dir);
    if (key === undefined) {
      const axis = dir[0] === 0 ? 0 : 1;
      const origin = centre(from)[axis];
      key = boxes.reduce((best, b) =>
        Math.abs(centre(b.rect)[axis] - origin) < Math.abs(centre(best.rect)[axis] - origin) ? b : best,
      ).key;
    }
  }
  group.api.roving.focus(key);
  return true;
}

export interface FocusGroupProps extends Omit<PartProps<"div">, "children" | "ref" | "id"> {
  id: string;
  /** Fixed for the group's life. `spatial` moves to the nearest item on screen (UI SDK `spatialMove`). */
  orientation: Orientation;
  /** Coming back to the group lands on the item focused last. Default true. */
  remember?: boolean;
  /** An arrow left the group and no group lies that way. */
  onEdge?: (dir: Dir) => void;
  /** Replaces the arrows, Home and End inside the group (UI SDK roving `move`). Present or absent for the group's life. */
  move?: GroupMove;
  /** The group scrolls its own items into view, so the bridge does not. */
  reveals?: boolean;
  /** A screen opening lands on a later group first; arrows still reach this one. */
  aside?: boolean;
  /** The item an arrow from another group always lands on (a row whose first item is its main one). */
  entry?: string;
  children?: ReactNode;
}

/** A group of focus items with one tab stop, built on UI SDK's `createRovingFocus`. */
export function FocusGroup({
  id,
  orientation,
  remember = true,
  onEdge,
  move,
  reveals = false,
  aside = false,
  entry,
  children,
  onFocus,
  ...props
}: FocusGroupProps) {
  const scope = useContext(ScopeContext);
  const group = usePrimitive(focusGroup, { orientation, remember, fallback: scope?.fallback, move });
  const root = useRef<HTMLDivElement>(null);
  const visited = useRef(false);
  const edge = useRef(onEdge);
  useLayoutEffect(() => {
    edge.current = onEdge;
  });

  useEffect(
    () =>
      registerGroup({
        id,
        el: root.current!,
        enter: (from, dir) => enter(group, visited.current && remember, from, dir, entry),
        stop: () => stopOf(group),
        items: () => group.api.roving.items().map((i) => i.el),
        onEdge: (dir) => edge.current?.(dir),
        reveals,
        aside,
      }),
    [id, group, remember, reveals, aside, entry],
  );

  const part = usePart(
    group,
    "root",
    {},
    {
      ...props,
      ref: root,
      "data-focus-group": id,
      onFocus: (e: FocusEvent<HTMLDivElement>) => {
        visited.current = true;
        group.api.snapshot();
        onFocus?.(e);
      },
      children,
    } as PartProps<"div">,
    "div",
  );
  return <GroupContext value={group}>{part}</GroupContext>;
}

export interface FocusItemOptions {
  key: string;
  /** Enter, click, and the pad's A. */
  onSelect?: () => void;
  /** `m`, the ContextMenu key, right-click, and the pad's X; given the item's element, to set the menu beside. */
  onMenu?: (el: HTMLElement) => void;
  /** Escape or Backspace (B) while this item has focus, before the scope's and the stack's. */
  onBack?: () => void;
  /** Can be focused but not selected. */
  disabled?: boolean;
  /** What A does, for the pad hints ("Type"); default "Open". */
  hint?: string;
}

export interface FocusItemProps<T extends HTMLElement> {
  tabIndex: number;
  "data-focus-key": string;
  "data-focused": "" | undefined;
  "aria-disabled": true | undefined;
  onFocus: (e: FocusEvent<T>) => void;
  onBlur: (e: FocusEvent<T>) => void;
  onKeyDown: (e: KeyboardEvent<T>) => void;
  onClick: () => void;
}

const clicksOnEnter = (el: HTMLElement) =>
  el instanceof HTMLButtonElement ||
  (el instanceof HTMLAnchorElement && el.href !== "") ||
  (el instanceof HTMLInputElement && ["button", "submit", "reset", "checkbox", "radio"].includes(el.type));

/** An item of the nearest FocusGroup. Spread `props` and pass `ref` to the element that takes focus. */
export function useFocusItem<T extends HTMLElement = HTMLElement>(options: FocusItemOptions) {
  const group = useContext(GroupContext);
  if (group === null) throw new Error("useFocusItem must be used inside a FocusGroup");
  const latest = useRef(options);
  useLayoutEffect(() => {
    latest.current = options;
  });

  // One object per item for the group's behaviours; its props are kept current.
  const part = useMemo<{ key: string; props: ItemProps }>(
    () => ({ key: options.key, props: {} }),
    [options.key],
  );
  part.props = { disabled: options.disabled };

  const state = useSyncExternalStore(
    group.instance.subscribe,
    () => group.instance.get() as { stop: string | null },
    () => group.instance.get() as { stop: string | null },
  );
  const [focused, setFocused] = useState(false);

  // Stable, or React would unbind and rebind the item on every render.
  const ref = useCallback(
    (el: T | null) => {
      if (el === null) return;
      const offs = [
        group.api.roving.bind(el, part),
        group.api.recovery.bind(el, part),
        registerItem(el, () => {
          const o = latest.current;
          const menu = o.onMenu;
          return {
            key: o.key,
            onSelect: o.onSelect,
            onMenu: menu && (() => menu(el)),
            onBack: o.onBack,
            disabled: o.disabled,
            hint: o.hint,
          };
        }),
      ];
      return () => {
        for (const off of offs) off();
      };
    },
    [group, part],
  );

  const select = () => {
    const o = latest.current;
    if (!o.disabled) o.onSelect?.();
  };

  const props: FocusItemProps<T> = {
    tabIndex: group.api.roving.tabIndex(state, part) === "0" ? 0 : -1,
    "data-focus-key": options.key,
    "data-focused": focused ? "" : undefined,
    "aria-disabled": options.disabled ? true : undefined,
    onFocus: (e) => {
      if (e.target === e.currentTarget) setFocused(true);
    },
    onBlur: (e) => {
      if (e.target === e.currentTarget) setFocused(false);
    },
    onKeyDown: (e) => {
      if (e.defaultPrevented || e.target !== e.currentTarget) return;
      const native = e.nativeEvent;
      if (fieldKeeps(native)) return;
      if (e.key === "Enter" && !clicksOnEnter(e.currentTarget)) {
        e.preventDefault();
        select();
        return;
      }
      const moved = group.api.roving.keydown(native, e.currentTarget);
      if (moved !== undefined && moved.key === moved.from && e.key.startsWith("Arrow")) markEdge(native);
    },
    onClick: select,
  };

  return { ref, props, focused };
}

export interface FocusScopeProps {
  id: string;
  /** A UI SDK modal Dialog layer: focus trapped, the rest of the page inert, focus returned on close. */
  modal?: boolean;
  /** A modal's open state. Keep the scope mounted and set this false to close it through the Dialog. Default true. */
  open?: boolean;
  /** The item focus falls back to when a group empties under it. */
  fallbackKey?: string;
  /** The item to focus on mount, and when the scope becomes active again. */
  restoreKey?: string | null;
  /** Back from inside the scope when the focused item has no onBack. A modal's close. */
  onBack?: () => void;
  /** X (`m`) inside the scope when the focused item has no menu. */
  onMenu?: () => void;
  /** A modal's Y (`/`, Ctrl+K); without it, Y does nothing while the modal is on top. */
  onAsk?: () => void;
  /** A modal's pad Menu button; without it, that button does nothing while the modal is on top. */
  onDone?: () => void;
  /** False while the scope sits under another screen; becoming true restores `restoreKey`. Default true. */
  active?: boolean;
  /** Hears each item focused inside the scope; a screen passes it to the navigation stack. */
  onFocusKey?: (key: string) => void;
  /** The modal's accessible name. */
  label?: string;
  /** Classes for the modal's content, or for a screen scope's element (default `contents`). */
  className?: string;
  /** A modal has finished closing (after its exit, with focus returned). */
  onClosed?: () => void;
  /** When set, a modal renders a backdrop with these classes. */
  backdropClassName?: string;
  /** Where a modal puts focus as it opens: nowhere (its items settle it), or on itself, for a layer with no items that must still hold the keys. */
  initialFocus?: "none" | "content";
  children?: ReactNode;
}

/** How long a scope waits for `restoreKey` to arrive with its data before it settles elsewhere. */
const RESTORE_WAIT = 1500;

const live = (el: Element) => el.isConnected && el.closest("[inert]") === null;

/** One per screen and per overlay. */
export function FocusScope({
  id,
  modal = false,
  open = true,
  active = true,
  label,
  className,
  backdropClassName,
  initialFocus = "none",
  children,
  ...handlers
}: FocusScopeProps) {
  // The scope's element; a modal's comes and goes with the Dialog's content.
  const [node, setNode] = useState<HTMLDivElement | null>(null);
  const isOpen = !modal || open;
  const isActive = modal ? open : active;
  const latest = useRef({ ...handlers, active: isActive });
  useLayoutEffect(() => {
    latest.current = { ...handlers, active: isActive };
  });

  // Where a modal returns focus, captured as it opens, before it takes focus.
  const back = useRef<() => HTMLElement | undefined>(() => undefined);
  useLayoutEffect(() => {
    if (modal && open) back.current = returnTarget(document.activeElement);
  }, [modal, open]);

  const fallback = useCallback(() => {
    if (node === null) return undefined;
    const key = latest.current.fallbackKey;
    const byKey = key === undefined ? undefined : findItem(node, key);
    if (byKey !== undefined) return byKey;
    for (const g of groupsIn(node)) {
      const stop = g.stop();
      if (stop?.isConnected) return stop;
    }
    return undefined;
  }, [node]);
  const target = useCallback(() => {
    if (node === null) return undefined;
    const key = latest.current.restoreKey;
    return (key == null ? undefined : findItem(node, key)) ?? fallback();
  }, [node, fallback]);
  const context = useMemo(() => ({ fallback }), [fallback]);

  useEffect(() => {
    if (node === null) return;
    const entry: ScopeEntry = {
      id,
      el: node,
      modal: modal && open,
      get onBack() {
        return latest.current.onBack;
      },
      get onMenu() {
        return latest.current.onMenu;
      },
      get onAsk() {
        return latest.current.onAsk;
      },
      get onDone() {
        return latest.current.onDone;
      },
      get onFocusKey() {
        return latest.current.onFocusKey;
      },
      get active() {
        return latest.current.active;
      },
      target,
      fallback,
    };
    return registerScope(entry);
  }, [id, modal, open, node, target, fallback]);

  // A modal unmounted while open never closes through the Dialog; return focus by hand.
  const openRef = useRef(open);
  useLayoutEffect(() => {
    openRef.current = open;
  });
  useEffect(() => {
    if (!modal) return;
    return () => {
      if (openRef.current) keepFocus(() => back.current());
    };
  }, [modal]);

  useEffect(() => {
    if (!isActive || node === null) return;
    const root = node;
    const want = latest.current.restoreKey ?? null;
    let final = want === null;
    let done = false;

    const attempt = () => {
      if (done) return;
      const a = document.activeElement;
      const onItem = a !== null && itemAt(a) !== undefined && live(a);
      // Already inside, or (for a screen) the owner has moved to an item elsewhere, like the top bar.
      if (onItem && (root.contains(a) || !modal)) {
        done = true;
        return;
      }
      if (want !== null) {
        const found = findItem(root, want);
        if (found !== undefined && live(found)) {
          found.focus({ preventScroll: true });
          done = true;
          return;
        }
      }
      if (!final) return;
      const groups = liveGroups(root);
      for (const g of [...groups.filter((g) => !g.aside), ...groups.filter((g) => g.aside)]) {
        const stop = g.stop();
        if (stop !== undefined) {
          stop.focus({ preventScroll: true });
          done = true;
          return;
        }
      }
    };

    attempt();
    if (done) return;
    const observer = new MutationObserver(attempt);
    observer.observe(root, { childList: true, subtree: true });
    const timer = setTimeout(() => {
      final = true;
      attempt();
    }, RESTORE_WAIT);
    return () => {
      done = true;
      observer.disconnect();
      clearTimeout(timer);
    };
  }, [isActive, modal, node]);

  if (!modal) {
    return (
      <ScopeContext value={context}>
        <div ref={setNode} data-focus-scope={id} className={className ?? "contents"}>
          {children}
        </div>
      </ScopeContext>
    );
  }

  return (
    <ScopeContext value={context}>
      <Dialog
        open={isOpen}
        modal
        dismissible={false}
        onOpenChangeComplete={(next) => {
          if (!next) latest.current.onClosed?.();
        }}
        onDismissAttempt={(details) => {
          // The Dialog takes Escape on its content and marks it handled, so the bridge's
          // document listener skips it: Escape is Back from here, as Backspace is there.
          if (details.reason === "escape-key") goBack();
          // A press outside the overlay means the modal's own Back.
          else if (details.reason === "outside-press") latest.current.onBack?.();
        }}
      >
        {backdropClassName !== undefined && <DialogBackdrop className={backdropClassName} />}
        <DialogContent
          aria-label={label}
          initialFocus={initialFocus}
          finalFocus={() => back.current()}
          className={className}
        >
          <div ref={setNode} data-focus-scope={id} className="contents">
            {children}
          </div>
        </DialogContent>
      </Dialog>
    </ScopeContext>
  );
}

/** How the owner is driving right now. */
export function useInputMode(): InputMode {
  return useFocusState((s) => s.mode);
}

/** The key of the focused item, or null. */
export function useFocusedKey(): string | null {
  return useFocusState((s) => s.key);
}

export interface FocusRootProps {
  onAsk: () => void;
  onSection: (delta: -1 | 1) => void;
  onSettings: () => void;
}

/** Installs the bridge, the global keys and the three input devices, once, near the app's root. */
export function FocusRoot(props: FocusRootProps) {
  const latest = useRef(props);
  useLayoutEffect(() => {
    latest.current = props;
  });
  useEffect(() => {
    const offs = [
      installBridge(),
      installShortcuts({
        onAsk: () => latest.current.onAsk(),
        onSection: (d) => latest.current.onSection(d),
      }),
      startKeys(),
      startPointer(),
      startWheel(),
      startGamepad({ onSettings: () => menuButton(latest.current.onSettings) }),
    ];
    return () => {
      for (const off of offs.reverse()) off();
    };
  }, []);
  return null;
}
