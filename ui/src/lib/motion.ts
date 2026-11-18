// Motion's timings and the layout numbers code needs, read from tokens.css so values live in one
// place. A token that is missing is a bug, and throws.

import { animate, motionValue, type MotionValue } from "motion/react";

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

/** A `cubic-bezier` token as Motion's four numbers. */
export function curve(name: string): [number, number, number, number] {
  const n = token(name)
    .match(/cubic-bezier\(([^)]+)\)/)?.[1]
    ?.split(",")
    .map((x) => Number.parseFloat(x));
  if (n?.length !== 4 || !n.every(Number.isFinite)) throw new Error(`${name} is not a cubic-bezier`);
  return n as [number, number, number, number];
}

/** The `cubic-bezier` of `--curve-out` as Motion's four numbers. */
export function easeOut(): [number, number, number, number] {
  return curve("--curve-out");
}

/** A length token as CSS text, for Motion values that need a unit. */
export function length(name: string): string {
  return token(name);
}

/** The px size of 1rem now (it follows the window's short side; see base.css). */
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

// One Motion value per scroller, holding its position while it glides.
const glides = new WeakMap<HTMLElement, MotionValue<number>>();

const position = (el: HTMLElement, axis: "left" | "top") => (axis === "left" ? el.scrollLeft : el.scrollTop);

/**
 * Slides a scroller to `to` on the focus spring, or at once when the owner prefers reduced motion.
 * Motion writes the position every frame; a native smooth scroll is not used, since in WebView2 it
 * is cut short by any other scroll in the same chain, such as Chromium's own focus scroll. A glide
 * retargeted while it runs keeps its speed, so a held direction moves as one motion.
 */
export function glideScroll(el: HTMLElement, axis: "left" | "top", to: number): void {
  let value = glides.get(el);
  if (scrollBehavior() === "auto") {
    value?.stop();
    el.scrollTo({ [axis]: to });
    return;
  }
  if (value === undefined) {
    value = motionValue(position(el, axis));
    value.on("change", (v) => el.scrollTo({ [axis]: v }));
    glides.set(el, value);
  } else if (!value.isAnimating()) {
    // At rest it starts from where the scroller is now, whatever moved it since.
    value.jump(position(el, axis));
  }
  animate(value, to, focusSpring());
}

/** Stops a scroller's glide; for a scroller leaving the page. */
export function stopGlide(el: HTMLElement): void {
  glides.get(el)?.stop();
}

/** Where a vertical list keeps the focused row: between these fractions of its height. */
const BAND = { top: 0.22, bottom: 0.78 };

/**
 * Glides the vertical scroller `el` so the row spanning `top` to `bottom` (in its scroll
 * coordinates) is in view. Keys and the pad keep the row inside the band, so the rows around it
 * show too (a row taller than the band is centred). A pointer only brings a row it touches fully
 * into view, so the list never slides away from under it.
 */
export function revealRow(el: HTMLElement, top: number, bottom: number, pointer: boolean): void {
  const h = el.clientHeight;
  let target = el.scrollTop;
  if (pointer) {
    if (bottom > el.scrollTop + h) target = bottom - h;
    else if (top < el.scrollTop) target = top;
  } else {
    const band = { top: h * BAND.top, bottom: h * BAND.bottom };
    if (bottom - top > band.bottom - band.top) target = top - (h - (bottom - top)) / 2;
    else if (top - el.scrollTop < band.top) target = top - band.top;
    else if (bottom - el.scrollTop > band.bottom) target = bottom - band.bottom;
  }
  target = Math.max(0, Math.min(target, el.scrollHeight - h));
  if (Math.abs(target - el.scrollTop) > 1) glideScroll(el, "top", target);
}
