// One horizontal row of works, virtualised with UI SDK `createVirtualizer` and one focus group.
// Arrows move by index (UI SDK roving `move`), so a work not rendered yet is reachable: the shelf
// scrolls it in and focus follows when it binds. The focused work sits one slot in from the left
// edge while the shelf slides under it; returning to the shelf lands on the work it remembers.

import { createVirtualizer } from "@ui-sdk/core";
import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore, type FocusEvent } from "react";
import { focusItem, useFocusState } from "@/focus/bridge";
import { FocusGroup, useFocusItem, type GroupMove } from "@/focus/react";
import type { ItemSummary } from "@/lib/api";
import { glideScroll, remPx, rems, scrollBehavior, stopGlide } from "@/lib/motion";
import { cn } from "@/lib/utils";
import { openItemMenu, useMenuOpenOn } from "./ItemMenu";
import { ASPECT, Work, workLabel, type WorkShape } from "./Work";

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
  /** A work's width in rem. Default `--work-poster` or `--work-wide` from tokens.css. */
  size?: number;
  /** The light's shared id. Default "work-light": one light for the screen. */
  light?: string;
  /** The works cannot be reached (a screen shows the shelf only in passing); the title still can. */
  inert?: boolean;
  /**
   * With this, the title is a pointer target that enters the shelf: it calls `onEnter` (where the
   * screen makes the shelf reachable), then focuses the work the shelf remembers, else its first.
   */
  onEnter?: () => void;
  className?: string;
}

const OVERSCAN = 4;

/**
 * A work's focus key, namespaced by its shelf: a film on two shelves is two places, and Back
 * returns to the one the owner left. The wall grid keys works as `item:<id>`.
 */
export const itemKey = (shelf: string, item: ItemSummary) => `${shelf}:item:${item.id}`;

interface CellProps {
  shelf: string;
  item: ItemSummary;
  index: number;
  shape: WorkShape;
  size: number;
  light: string;
  onOpen: (item: ItemSummary) => void;
  onFocused: (index: number) => void;
}

const ShelfCell = memo(function ShelfCell({ shelf, item, index, shape, size, light, onOpen, onFocused }: CellProps) {
  const el = useRef<HTMLButtonElement | null>(null);
  const { ref, props, focused } = useFocusItem<HTMLButtonElement>({
    key: itemKey(shelf, item),
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

export const Shelf = memo(function Shelf({
  id,
  title,
  items,
  onOpen,
  onFocusItem,
  shape = "poster",
  size = rems(shape === "poster" ? "--work-poster" : "--work-wide"),
  light = "work-light",
  inert,
  onEnter,
  className,
}: ShelfProps) {
  const section = useRef<HTMLElement>(null);
  const latest = useRef({ items, onOpen, onFocusItem, size });
  useLayoutEffect(() => {
    latest.current = { items, onOpen, onFocusItem, size };
  });
  // The work the shelf remembers stays rendered wherever it scrolls, so focus can come back to it.
  const remembered = useRef<string | null>(null);
  const indexOf = useMemo(() => new Map(items.map((it, i) => [itemKey(id, it), i])), [items, id]);
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
      stopGlide(el);
    };
  }, [virtual, scroller]);

  useEffect(() => {
    virtual.refresh();
  }, [items, size, virtual]);

  // One slot in from the left edge; the bridge leaves scrolling to the shelf (`reveals`).
  const slideTo = useCallback(
    (index: number) => {
      const slot = (latest.current.size + rems("--space-work-gap")) * remPx();
      const el = scroller();
      if (el !== null) glideScroll(el, "left", Math.max(0, (index - 1) * slot));
    },
    [scroller],
  );

  // The least glide that shows the whole work at `index`, inside the shelf's edge padding.
  const reveal = useCallback(
    (index: number) => {
      const el = scroller();
      if (el === null) return;
      const rem = remPx();
      const edge = rems("--space-edge") * rem;
      const left = edge + index * (latest.current.size + rems("--space-work-gap")) * rem;
      const right = left + latest.current.size * rem;
      if (right > el.scrollLeft + el.clientWidth - edge) glideScroll(el, "left", right - el.clientWidth + edge);
      else if (left < el.scrollLeft + edge) glideScroll(el, "left", left - edge);
    },
    [scroller],
  );

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
      // The shelf slides the target to its slot now; a work not rendered yet renders as it nears,
      // and focus follows when it binds. Its focus then asks for the same slot, so there is one scroll.
      slideTo(to);
      return itemKey(id, list[to]!);
    },
    [slideTo, id],
  );

  const open = useCallback((item: ItemSummary) => latest.current.onOpen(item), []);

  const onFocused = useCallback(
    (index: number) => {
      const item = latest.current.items[index];
      if (item === undefined) return;
      remembered.current = itemKey(id, item);
      latest.current.onFocusItem?.(item);
      // A pointer brings a work partly past the edge just into view; it does not slide the shelf
      // away from under itself.
      if (useFocusState.getState().mode === "pointer") return reveal(index);
      slideTo(index);
      section.current?.scrollIntoView({ block: "nearest", behavior: scrollBehavior() });
    },
    [slideTo, reveal, id],
  );

  const enter = useCallback(() => {
    const first = latest.current.items[0];
    if (first === undefined) return;
    onEnter?.();
    focusItem(remembered.current ?? itemKey(id, first));
  }, [onEnter, id]);

  const height = size * ASPECT[shape];
  const gap = rems("--space-work-gap");
  return (
    <section ref={section} aria-label={title} className={cn("flex flex-col", className)}>
      <h2 className="mb-6 px-edge font-display text-lead font-ui text-label-dim">
        {onEnter ? (
          // Not a focus item: a pointer target only, which keeps focus where it is until it enters.
          <button
            type="button"
            tabIndex={-1}
            onPointerDown={(e) => e.preventDefault()}
            onClick={enter}
            className="cursor-pointer transition-colors hover:text-label"
          >
            {title}
          </button>
        ) : (
          title
        )}
      </h2>
      <FocusGroup
        id={id}
        inert={inert}
        orientation="horizontal"
        move={move}
        reveals
        aria-label={title}
        className="-mt-light overflow-x-auto overflow-y-hidden px-edge pt-light pb-8"
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
                shelf={id}
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
});
