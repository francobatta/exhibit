// Motion's timings and the layout numbers code needs, read from tokens.css so values live in one
// place. A token that is missing is a bug, and throws.

import { animate } from "motion/react";

// A computed style is live, so one read of it stays current.
let root: CSSStyleDeclaration | null = null;

function token(name: string): string {
  root ??= getComputedStyle(document.documentElement);
  const v = root.getPropertyValue(name).trim();
  if (v === "") throw new Error(`The design token ${name} is not defined (tokens.css)`);
  return v;
}

function number(name: string): number {
  const n = Number.parseFloat(token(name));
  if (!Number.isFinite(n)) throw new Error(`The design token ${name} is not a number`);
  return n;
}

/** A duration token ("450ms", "0.45s") in seconds, as Motion takes it. */
export function seconds(name: string): number {
  const n = number(name);
  return token(name).endsWith("ms") ? n / 1000 : n;
}

/** The spring focus light glides with. */
export function focusSpring() {
  return {
    type: "spring" as const,
    stiffness: number("--focus-stiffness"),
    damping: number("--focus-damping"),
  };
}

/** The `cubic-bezier` of `--curve-out` as Motion's four numbers. */
export function easeOut(): [number, number, number, number] {
  const n = token("--curve-out")
    .match(/cubic-bezier\(([^)]+)\)/)?.[1]
    ?.split(",")
    .map((x) => Number.parseFloat(x));
  if (n?.length !== 4 || !n.every(Number.isFinite)) throw new Error("--curve-out is not a cubic-bezier");
  return n as [number, number, number, number];
}

/** A length token as CSS text, for Motion values that need a unit. */
export function length(name: string): string {
  return token(name);
}

/** The px size of 1rem now (it follows the window width). */
export function remPx(): number {
  root ??= getComputedStyle(document.documentElement);
  return Number.parseFloat(root.fontSize);
}

/** A unitless number token ("1.06"). */
export function numberToken(name: string): number {
  return number(name);
}

/** A rem length token as a number of rem ("1.75rem" gives 1.75). */
export function rems(name: string): number {
  return number(name);
}

/** How code-driven scrolls move: smooth, or at once when the owner prefers reduced motion. */
export function scrollBehavior(): ScrollBehavior {
  return matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth";
}

const glides = new WeakMap<Element, { stop(): void }>();

/**
 * Slides a scroller to `to` on the focus spring, or at once when the owner prefers reduced motion.
 * Motion writes the position every frame. A native smooth scroll is not used: in WebView2 it is cut
 * short by any other scroll in the same chain, such as Chromium's own focus scroll.
 */
export function glideScroll(el: HTMLElement, axis: "left" | "top", to: number): void {
  glides.get(el)?.stop();
  if (scrollBehavior() === "auto") {
    el.scrollTo({ [axis]: to });
    return;
  }
  const from = axis === "left" ? el.scrollLeft : el.scrollTop;
  glides.set(el, animate(from, to, { ...focusSpring(), onUpdate: (v) => el.scrollTo({ [axis]: v }) }));
}
