// A title that fills its measure: Archivo's width axis is set between 62 and 125 so the line
// meets the right edge instead of wrapping or shrinking. A title too long even at 62 wraps to
// `maxLines` and then ends with an ellipsis.

import { useLayoutEffect, useRef, type ElementType } from "react";
import { cn } from "@/lib/utils";

export const STRETCH_MIN = 62;
export const STRETCH_MAX = 125;

/** The width (font-stretch %) at which text `textWidthAt100` wide at 100% fills `measure`. */
export function fitWidth(textWidthAt100: number, measure: number): number {
  if (!(textWidthAt100 > 0) || !(measure > 0)) return 100;
  return Math.min(STRETCH_MAX, Math.max(STRETCH_MIN, (100 * measure) / textWidthAt100));
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
    const fit = () => {
      el.style.whiteSpace = "nowrap";
      el.style.display = "block";
      el.style.fontStretch = "100%";
      const measure = el.clientWidth;
      const at100 = inner.getBoundingClientRect().width;
      let w = fitWidth(at100, measure);
      el.style.fontStretch = `${w}%`;
      // The axis is not perfectly linear: a few corrections land it just inside the edge.
      for (let i = 0; i < 3 && w > STRETCH_MIN; i++) {
        const now = inner.getBoundingClientRect().width;
        if (now <= measure) break;
        w = Math.max(STRETCH_MIN, ((w * measure) / now) * 0.995);
        el.style.fontStretch = `${w}%`;
      }
      const wraps = at100 > 0 && measure > 0 && inner.getBoundingClientRect().width > measure + 1;
      if (wraps) {
        el.style.whiteSpace = "normal";
        el.style.display = "-webkit-box";
      }
    };
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(el);
    void document.fonts?.ready.then(fit);
    return () => observer.disconnect();
  }, [text]);

  return (
    <Tag
      ref={box}
      title={text}
      className={cn(
        "w-full overflow-hidden text-ellipsis [overflow-wrap:anywhere] [-webkit-box-orient:vertical]",
        className,
      )}
      style={{ WebkitLineClamp: maxLines, lineClamp: maxLines }}
    >
      <span ref={line}>{text}</span>
    </Tag>
  );
}
