// The focus system for components: scopes (one per screen and per overlay), groups (one tab stop
// each, arrows inside them owned by UI SDK's roving focus), items, and the root that installs the
// bridge and the input devices. See bridge.ts for movement between groups.

import {
  createFocusRecovery,
  createRovingFocus,
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
  installBridge,
  installShortcuts,
  itemAt,
  liveGroups,
  markEdge,
  registerGroup,
  registerItem,
  registerScope,
  useFocusState,
  type Box,
  type Dir,
  type InputMode,
  type ScopeEntry,
} from "./bridge";
import { startGamepad } from "./input/gamepad";
import { fieldKeeps, startKeys } from "./input/keys";
import { startPointer } from "./input/pointer";

export type Orientation = "horizontal" | "vertical" | "spatial";

interface ItemProps {
  disabled?: boolean;
}

interface GroupOptions {
  orientation?: Orientation;
  remember?: boolean;
  /** Where focus goes when the group empties under it: the scope's fallback. */
  fallback?: () => HTMLElement | undefined;
}

const focusGroup = definePrimitive({
  name: "focus-group",
  options: { orientation: "string", remember: "boolean", fallback: "value" },
  state: (_options: GroupOptions) => ({ stop: null as string | null }),
  api: (context) => {
    const { orientation, remember = true } = context.options();
    const roving = createRovingFocus<{ stop: string | null }, ItemProps>(context, {
      slot: "item",
      key: "stop",
      orientation: () => (context.options().orientation === "vertical" ? "vertical" : "horizontal"),
      remember,
      disabled: (item) => item.props.disabled === true,
      ...(orientation === "spatial" ? { move: spatialMove() } : {}),
    });
    // The order as focus last saw it, so a removed item's place can be found afterwards.
    let order: string[] = [];
    const recovery = createFocusRecovery<ItemProps>({
      fallback: (lost) => {
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

/** Lands in a group: its remembered item, else the one nearest `from` that way, else nearest on the cross axis. */
function enter(group: GroupHandle, useStop: boolean, from: Box, dir: Dir): boolean {
  const items = group.api.roving.items();
  if (items.length === 0) return false;
  const stop = group.instance.get().stop;
  let key = useStop && items.some((i) => i.key === stop) ? stop! : undefined;
  if (key === undefined) {
    const boxes = items.map((i) => ({ key: i.key, rect: i.el.getBoundingClientRect() as Box }));
    key = nearestInDirection(from, boxes, dir);
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
  children?: ReactNode;
}

/** A group of focus items with one tab stop, built on UI SDK's `createRovingFocus`. */
export function FocusGroup({ id, orientation, remember = true, onEdge, children, onFocus, ...props }: FocusGroupProps) {
  const scope = useContext(ScopeContext);
  const group = usePrimitive(focusGroup, { orientation, remember, fallback: scope?.fallback });
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
        enter: (from, dir) => enter(group, visited.current && remember, from, dir),
        stop: () => stopOf(group),
        onEdge: (dir) => edge.current?.(dir),
      }),
    [id, group, remember],
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
  /** `m`, the ContextMenu key, right-click, and the pad's X. */
  onMenu?: () => void;
  /** Escape or Backspace (B) while this item has focus, before the scope's and the stack's. */
  onBack?: () => void;
  /** Can be focused but not selected. */
  disabled?: boolean;
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
          return { key: o.key, onSelect: o.onSelect, onMenu: o.onMenu, onBack: o.onBack, disabled: o.disabled };
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
  /** A UI SDK modal layer: focus trapped, the rest of the page inert, focus returned on unmount. */
  modal?: boolean;
  /** The item focus falls back to when a group empties under it. */
  fallbackKey?: string;
  /** The item to focus on mount, and when the scope becomes active again. */
  restoreKey?: string | null;
  /** Back from inside the scope when the focused item has no onBack. A modal's close. */
  onBack?: () => void;
  /** False while the scope sits under another screen; becoming true restores `restoreKey`. Default true. */
  active?: boolean;
  /** Hears each item focused inside the scope; a screen passes it to the navigation stack. */
  onFocusKey?: (key: string) => void;
  /** The modal's accessible name. */
  label?: string;
  /** Classes for the modal's content, or for a screen scope's element (default `contents`). */
  className?: string;
  /** When set, a modal renders a backdrop with these classes. */
  backdropClassName?: string;
  children?: ReactNode;
}

/** How long a scope waits for `restoreKey` to arrive with its data before it settles elsewhere. */
const RESTORE_WAIT = 1500;

const live = (el: Element) => el.isConnected && el.closest("[inert]") === null;

/** One per screen and per overlay. */
export function FocusScope({
  id,
  modal = false,
  active = true,
  label,
  className,
  backdropClassName,
  children,
  ...handlers
}: FocusScopeProps) {
  const el = useRef<HTMLDivElement>(null);
  const latest = useRef(handlers);
  useLayoutEffect(() => {
    latest.current = handlers;
  });
  // What had focus before a modal opened, read before the modal takes it.
  const [previous] = useState(() => (modal ? (document.activeElement as HTMLElement | null) : null));

  const fallback = useCallback(() => {
    const root = el.current;
    if (root === null) return undefined;
    const key = latest.current.fallbackKey;
    const byKey = key === undefined ? undefined : findItem(root, key);
    if (byKey !== undefined && live(byKey)) return byKey;
    for (const g of liveGroups(root)) {
      const stop = g.stop();
      if (stop?.isConnected) return stop;
    }
    return undefined;
  }, []);
  const context = useMemo(() => ({ fallback }), [fallback]);

  useEffect(() => {
    const entry: ScopeEntry = {
      id,
      el: el.current!,
      modal,
      get onBack() {
        return latest.current.onBack;
      },
      get onFocusKey() {
        return latest.current.onFocusKey;
      },
    };
    return registerScope(entry);
  }, [id, modal]);

  useEffect(() => {
    if (!modal || previous === null) return;
    return () => keepFocus(() => (live(previous) ? previous : undefined));
  }, [modal, previous]);

  useEffect(() => {
    if (!active) return;
    const root = el.current!;
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
        const target = findItem(root, want);
        if (target !== undefined && live(target)) {
          target.focus({ preventScroll: true });
          done = true;
          return;
        }
      }
      if (!final) return;
      for (const g of liveGroups(root)) {
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
  }, [active, modal]);

  if (!modal) {
    return (
      <ScopeContext value={context}>
        <div ref={el} data-focus-scope={id} className={className ?? "contents"}>
          {children}
        </div>
      </ScopeContext>
    );
  }

  return (
    <ScopeContext value={context}>
      <Dialog
        open
        modal
        dismissible={false}
        onDismissAttempt={(details) => {
          // Escape reaches the bridge unhandled; a press outside the overlay means Back.
          if (details.reason === "outside-press") latest.current.onBack?.();
        }}
      >
        {backdropClassName !== undefined && <DialogBackdrop className={backdropClassName} />}
        <DialogContent aria-label={label} initialFocus="none" className={className}>
          <div ref={el} data-focus-scope={id} className="contents">
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
      startGamepad({ onSettings: () => latest.current.onSettings() }),
    ];
    return () => {
      for (const off of offs.reverse()) off();
    };
  }, []);
  return null;
}
