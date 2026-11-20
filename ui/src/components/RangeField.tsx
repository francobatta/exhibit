// A span of years or minutes over the works that have them: a bar for each value (or group of
// values) shows how many works sit there, lit inside the span, with a thumb at each end. The thumbs
// are native range inputs, one over the other: each is a focus item, Up and Down move between them
// and out, Left and Right move the focused end (LB and RB, `[` and `]`, by a leap), and a pointer
// drags them. UI SDK's Slider keeps its inputs inside its own parts, out of the focus system's reach.

import type { KeyboardEvent } from "react";
import { FocusGroup, useFocusItem } from "@/focus/react";
import { useT } from "@/i18n";
import type { Count } from "@/lib/api";
import { cn } from "@/lib/utils";

export interface RangeFieldProps {
  id: string;
  label: string;
  /** How many works have each value. */
  counts: Count<number>[];
  /** Values a bar gathers (10 minutes). Default 1. */
  bucket?: number;
  step: number;
  /** How far LB and RB move an end, and what the hints call it. */
  leap: { by: number; label: string };
  /** The span; null is open at that end. */
  min: number | null | undefined;
  max: number | null | undefined;
  onChange: (min: number | null, max: number | null) => void;
  /** A value as the end's name reads it ("1972", "1 h 30 min"). */
  name: (value: number) => string;
}

/** The values the field runs over: the first and last bucket that has works. Null with fewer than two. */
export function extent(counts: Count<number>[], bucket = 1): [number, number] | null {
  if (counts.length === 0) return null;
  const values = counts.map((c) => c.value);
  const lo = Math.floor(Math.min(...values) / bucket) * bucket;
  const hi = Math.floor(Math.max(...values) / bucket) * bucket;
  return hi > lo ? [lo, hi] : null;
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

function Thumb({
  id,
  end,
  value,
  lo,
  hi,
  step,
  leap,
  label,
  text,
  onValue,
}: {
  id: string;
  end: 0 | 1;
  value: number;
  lo: number;
  hi: number;
  step: number;
  leap: RangeFieldProps["leap"];
  label: string;
  text: string;
  onValue: (value: number) => void;
}) {
  const t = useT();
  const { ref, props } = useFocusItem<HTMLInputElement>({ key: `${id}:${end}`, adjust: t("hints.adjust"), leap: leap.label });
  const by: Record<string, number> = { ArrowLeft: -step, ArrowRight: step, "[": -leap.by, "]": leap.by };
  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    const delta = by[e.key];
    if (delta !== undefined && !e.altKey && !e.ctrlKey && !e.metaKey) {
      e.preventDefault();
      onValue(clamp(value + delta, lo, hi));
      return;
    }
    props.onKeyDown(e);
    // The input would step itself on Up and Down; those move focus here.
    if (e.key === "ArrowUp" || e.key === "ArrowDown") e.preventDefault();
  };
  return (
    <input
      ref={ref}
      {...props}
      onKeyDown={onKeyDown}
      type="range"
      min={lo}
      max={hi}
      step={step}
      value={value}
      onChange={(e) => onValue(Number(e.target.value))}
      aria-label={label}
      aria-valuetext={text}
      className={cn(
        "range-thumb pointer-events-none absolute inset-0 m-0 h-full w-full appearance-none bg-transparent outline-none",
        end === 1 && "z-10",
      )}
    />
  );
}

export function RangeField({ id, label, counts, bucket = 1, step, leap, min, max, onChange, name }: RangeFieldProps) {
  const t = useT();
  const span = extent(counts, bucket);
  if (span === null) return null;
  const [lo, hi] = span;
  const from = clamp(min ?? lo, lo, hi);
  const to = clamp(max ?? hi, lo, hi);
  const set = (a: number, b: number) => onChange(a <= lo ? null : a, b >= hi ? null : b);
  const bars: Array<{ at: number; n: number }> = [];
  for (let at = lo; at <= hi; at += bucket) bars.push({ at, n: 0 });
  for (const c of counts) {
    const bar = bars[Math.floor((c.value - lo) / bucket)];
    if (bar !== undefined) bar.n += c.count;
  }
  const most = Math.max(...bars.map((b) => b.n));
  const pct = (v: number) => `${((v - lo) / (hi - lo)) * 100}%`;
  return (
    <div role="group" aria-label={label} className="flex flex-col">
      <div aria-hidden className="flex h-(--range-bars) items-end gap-px px-(--range-inset)">
        {bars.map((b) => (
          <span
            key={b.at}
            data-in={b.at + bucket - 1 >= from && b.at <= to ? "" : undefined}
            className="min-h-px flex-1 rounded-t-[1px] bg-label-dim opacity-30 transition-opacity duration-(--dur-control) data-in:opacity-100"
            style={{ height: `${(b.n / most) * 100}%` }}
          />
        ))}
      </div>
      <FocusGroup id={id} orientation="vertical" className="relative h-(--range-height)">
        <div aria-hidden className="absolute inset-x-(--range-inset) top-1/2 h-0.5 -translate-y-1/2 rounded-full bg-line">
          <span className="absolute inset-y-0 bg-label-dim" style={{ left: pct(from), right: `calc(100% - ${pct(to)})` }} />
        </div>
        <Thumb id={id} end={0} value={from} lo={lo} hi={hi} step={step} leap={leap} label={t("filters.start")} text={name(from)} onValue={(v) => set(Math.min(v, to), to)} />
        <Thumb id={id} end={1} value={to} lo={lo} hi={hi} step={step} leap={leap} label={t("filters.end")} text={name(to)} onValue={(v) => set(from, Math.max(v, from))} />
      </FocusGroup>
      <div aria-hidden className="flex justify-between px-(--range-inset) font-display text-caption text-label-faint tabular-nums">
        <span>{name(lo)}</span>
        <span>{name(hi)}</span>
      </div>
    </div>
  );
}
