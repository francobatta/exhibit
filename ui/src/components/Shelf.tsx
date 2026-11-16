// One horizontal row of works, virtualised with UI SDK `createVirtualizer` and one focus group.
// Arrows move by index (UI SDK roving `move`), so a work not rendered yet is reachable: the shelf
// scrolls it in and focus follows when it binds. The focused work sits one slot in from the left
// edge while the shelf slides under it; returning to the shelf lands on the work it remembers.

import { createVirtualizer } from "@ui-sdk/core";
import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore, type FocusEvent } from "react";
import { useFocusState } from "@/focus/bridge";
import { FocusGroup, useFocusItem, type GroupMove } from "@/focus/react";
import type { ItemSummary } from "@/lib/api";
import { remPx, rems, scrollBehavior } from "@/lib/motion";
import { cn } from "@/lib/utils";
import { openItemMenu, useMenuOpenOn } from "./ItemMenu";
import { Work, workLabel, type WorkShape } from "./Work";

export interface ShelfProps {
  /** Unique among live focus groups; also the shelf's light. */
  id: string;
  title: string;
  items: ItemSummary[];
  /** A, Enter or a click on a work. */
  onOpen: (item: ItemSummary) => void;
  /** Every work that takes focus, from any device. */
  onFocusItem?: (item: ItemSummary) => void;
  shape?: WorkShape;
  /** A work's width in rem. Default 11 for posters, 21 for wide works. */
  size?: number;
  /** The light's shared id. Default "work-light": one light for the screen. */
  light?: string;
  className?: string;
}

const OVERSCAN = 4;

export const itemKey = (item: ItemSummary) => `item:${item.id}`;


interface CellProps {
  item: ItemSummary;
  index: number;
  shape: WorkShape;
  size: number;
  light: string;
  onOpen: (item: ItemSummary) => void;
  onFocused: (index: number) => void;
}

const ShelfCell = memo(function ShelfCell({ item, index, shape, size, light, onOpen, onFocused }: CellProps) {
  const el = useRef<HTMLButtonElement | null>(null);
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({
    key: itemKey(item),
    onSelect: () => onOpen(item),
    onMenu: () => openItemMenu(item, el.current),
  });
  const menuOpen = useMenuOpenOn(el);
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
      onFocus={(e: FocusEvent<HTMLButtonElement>) => {
        props.onFocus(e);
        if (e.target === e.currentTarget) onFocused(index);
      }}
      type="button"
      aria-label={workLabel(item)}
      aria-posinset={index + 1}
      data-index={index}
      className="absolute top-0 left-0 outline-none"
      style={{ transform: `translateX(${index * (size + rems("--space-work-gap"))}rem)`, width: `${size}rem` }}
    >
      <Work item={item} shape={shape} focused={focused || menuOpen} size={size} light={light} />
    </button>
  );
});

export function Shelf({
  id,
  title,
  items,
  onOpen,
  onFocusItem,
  shape = "poster",
  size = shape === "poster" ? 11 : 21,
  light = "work-light",
  className,
}: ShelfProps) {
  const section = useRef<HTMLElement>(null);
  const latest = useRef({ items, onOpen, onFocusItem, size });
  useLayoutEffect(() => {
    latest.current = { items, onOpen, onFocusItem, size };
  });
  // The work the shelf remembers stays rendered wherever it scrolls, so focus can come back to it.
  const remembered = useRef<string | null>(null);
  const indexOf = useMemo(() => new Map(items.map((it, i) => [itemKey(it), i])), [items]);
  const indexRef = useRef(indexOf);
  indexRef.current = indexOf;

  const [virtual] = useState(() =>
    createVirtualizer({
      count: () => latest.current.items.length,
      estimate: () => (latest.current.size + rems("--space-work-gap")) * remPx(),
      key: (i) => String(latest.current.items[i]?.id ?? i),
      horizontal: true,
      overscan: OVERSCAN,
      keep: () => {
        const at = remembered.current === null ? undefined : indexRef.current.get(remembered.current);
        return at === undefined ? [] : [at];
      },
    }),
  );
  const rendered = useSyncExternalStore(virtual.subscribe, virtual.items, virtual.items);

  const scroller = useCallback(() => section.current?.querySelector<HTMLElement>("[data-focus-group]") ?? null, []);

  useEffect(() => {
    const el = scroller();
    if (el === null) return;
    const disconnect = virtual.connect(el);
    const onResize = () => virtual.reset();
    window.addEventListener("resize", onResize);
    return () => {
      disconnect();
      window.removeEventListener("resize", onResize);
    };
  }, [virtual, scroller]);

  useEffect(() => {
    virtual.refresh();
  }, [items, size, virtual]);

  const move = useCallback<GroupMove>(
    (event, { from }) => {
      if (event.altKey || event.ctrlKey || event.metaKey) return undefined;
      const list = latest.current.items;
      const at = indexRef.current.get(from.key);
      if (at === undefined) return undefined;
      const to =
        event.key === "ArrowRight" ? at + 1 : event.key === "ArrowLeft" ? at - 1 : event.key === "Home" ? 0 : event.key === "End" ? list.length - 1 : null;
      if (to === null) return undefined;
      // At an end the press stays here, and the bridge may carry it to the next group.
      if (to < 0 || to >= list.length) return from.key;
      if (!virtual.items().some((v) => v.index === to)) virtual.scrollToIndex(to, { align: "center" });
      return itemKey(list[to]!);
    },
    [virtual],
  );

  const open = useCallback((item: ItemSummary) => latest.current.onOpen(item), []);

  const onFocused = useCallback(
    (index: number) => {
      const item = latest.current.items[index];
      if (item === undefined) return;
      remembered.current = itemKey(item);
      latest.current.onFocusItem?.(item);
      if (useFocusState.getState().mode === "pointer") return;
      // One slot in from the left edge; the bridge leaves scrolling to the shelf (`reveals`).
      const slot = (latest.current.size + rems("--space-work-gap")) * remPx();
      scroller()?.scrollTo({ left: Math.max(0, (index - 1) * slot), behavior: scrollBehavior() });
      section.current?.scrollIntoView?.({ block: "nearest", behavior: scrollBehavior() });
    },
    [scroller],
  );

  const height = size * (shape === "poster" ? 1.5 : 9 / 16);
  const gap = rems("--space-work-gap");
  return (
    <section ref={section} aria-label={title} className={cn("flex flex-col", className)}>
      <h2 className="mb-6 px-edge font-display text-lead font-ui text-label-dim">{title}</h2>
      <FocusGroup
        id={id}
        orientation="horizontal"
        move={move}
        reveals
        aria-label={title}
        className="-mt-light overflow-x-auto overflow-y-hidden px-edge pt-light pb-8 [scrollbar-width:none]"
      >
        <div
          className="relative"
          style={{ width: `${items.length * (size + gap) - gap}rem`, height: `${height}rem` }}
        >
          {rendered.map((v) => {
            const item = items[v.index];
            if (item === undefined) return null;
            return (
              <ShelfCell
                key={item.id}
                item={item}
                index={v.index}
                shape={shape}
                size={size}
                light={light}
                onOpen={open}
                onFocused={onFocused}
              />
            );
          })}
        </div>
      </FocusGroup>
    </section>
  );
}
