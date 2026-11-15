// Motion's timings, read from tokens.css so durations live in one place. Values are read once,
// after the stylesheet has loaded; a missing token reads as 0 (an instant change).

let style: CSSStyleDeclaration | null = null;

function token(name: string): string {
  if (typeof document === "undefined") return "";
  style ??= getComputedStyle(document.documentElement);
  return style.getPropertyValue(name).trim();
}

/** A duration token ("450ms", "0.45s") in seconds, as Motion takes it. */
export function seconds(name: string): number {
  const v = token(name);
  const n = Number.parseFloat(v);
  if (!Number.isFinite(n)) return 0;
  return v.endsWith("ms") ? n / 1000 : n;
}

/** The spring focus light glides with. */
export function focusSpring() {
  return {
    type: "spring" as const,
    stiffness: Number.parseFloat(token("--focus-stiffness")),
    damping: Number.parseFloat(token("--focus-damping")),
  };
}

/** The `cubic-bezier` of `--curve-out` as Motion's four numbers. */
export function easeOut(): [number, number, number, number] | "easeOut" {
  const m = token("--curve-out").match(/cubic-bezier\(([^)]+)\)/);
  const n = m?.[1]?.split(",").map((x) => Number.parseFloat(x));
  return n?.length === 4 && n.every(Number.isFinite)
    ? (n as [number, number, number, number])
    : "easeOut";
}

/** A length token in rem as CSS text, for Motion values that need a unit. */
export function length(name: string): string {
  return token(name) || "0rem";
}
