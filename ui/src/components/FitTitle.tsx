// A title that fits its measure without being squeezed. In order, it takes the first of:
//   one line at its natural width;
//   one line narrowed a little on Archivo's width axis (down to --stretch-title-min);
//   up to `maxLines` balanced lines, at the natural width, then narrowed a little;
//   the same at a smaller size, a step (--title-step) at a time, up to --title-steps steps.
// A title too long even then takes the lines it needs at the smallest size. It never expands, and
// it is never cut or clipped: set at a title's tight leading, its descenders and accents reach past
// its line boxes, so the box must not clip them.

import { useLayoutEffect, useRef, type ElementType } from "react";
import { numberToken, remPx } from "@/lib/motion";
import { cn } from "@/lib/utils";

/** The width (font-stretch %) between `min` and `max` at which text `textWidthAtMax` wide fills `measure`. */
export function fitWidth(textWidthAtMax: number, measure: number, min: number, max = 100): number {
  if (!(textWidthAtMax > 0) || !(measure > 0)) return max;
  return Math.min(max, Math.max(min, (max * measure) / textWidthAtMax));
}

export interface FitTitleProps {
  text: string;
  maxLines?: number;
  /** Size, weight, colour. */
  className?: string;
  as?: ElementType;
}

export function FitTitle({ text, maxLines = 2, className, as: Tag = "h2" }: FitTitleProps) {
  const box = useRef<HTMLElement>(null);
  const line = useRef<HTMLSpanElement>(null);

  useLayoutEffect(() => {
    const el = box.current;
    const inner = line.current;
    if (el === null || inner === null) return;
    const min = numberToken("--stretch-title-min");
    const max = numberToken("--stretch-title-max");
    const step = numberToken("--title-step");
    const steps = numberToken("--title-steps");
    const width = () => inner.getBoundingClientRect().width;
    // The line boxes the title's text runs over.
    const lines = () => inner.getClientRects().length;
    const set = (stretch: number, wrap: boolean) => {
      el.style.fontStretch = `${stretch}%`;
      el.style.whiteSpace = wrap ? "normal" : "nowrap";
    };

    const fit = () => {
      el.style.fontSize = "";
      set(max, false);
      const measure = el.clientWidth;
      const natural = width();
      if (!(natural > 0) || !(measure > 0) || natural <= measure) return;

      // One line, narrowed a little. The axis is not quite linear: a few corrections land it
      // just inside the edge.
      let w = fitWidth(natural, measure, min, max);
      set(w, false);
      for (let i = 0; i < 3 && width() > measure && w > min; i++) {
        w = Math.max(min, ((w * measure) / width()) * 0.995);
        set(w, false);
      }
      if (width() <= measure) return;

      // Balanced lines, then smaller sizes, each at the natural width first.
      const base = Number.parseFloat(getComputedStyle(el).fontSize) / remPx();
      for (let s = 0; s <= steps; s++) {
        if (s > 0) el.style.fontSize = `${base * step ** s}rem`;
        for (const stretch of [max, (max + min) / 2, min]) {
          set(stretch, true);
          if (lines() <= maxLines) return;
        }
      }
      // Too long for every size: it stays at the smallest, narrowest, on the lines it needs.
    };
    fit();
    // Fitting changes the title's height, never its width: only a new width fits it again.
    let measured = el.clientWidth;
    const observer = new ResizeObserver(() => {
      if (el.clientWidth === measured) return;
      measured = el.clientWidth;
      fit();
    });
    observer.observe(el);
    void document.fonts.ready.then(fit);
    return () => observer.disconnect();
  }, [text, maxLines]);

  return (
    <Tag ref={box} className={cn("w-full text-balance [overflow-wrap:anywhere]", className)}>
      <span ref={line}>{text}</span>
    </Tag>
  );
}
