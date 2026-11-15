// The wall takes a tint from the focused work: the dominant hue of its poster, at the low
// lightness and chroma the Design direction fixes, as an `oklch()` colour.

import { useEffect, useState } from "react";
import { imageUrl } from "./api";

const SIDE = 16;
/** Below this mean chroma (oklab) the picture is grey and has no hue worth taking. */
const GREY = 0.03;

const toLinear = (c: number) => {
  const v = c / 255;
  return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
};

/** sRGB to oklab's a and b. */
function ab(r8: number, g8: number, b8: number): [number, number] {
  const r = toLinear(r8);
  const g = toLinear(g8);
  const b = toLinear(b8);
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return [
    1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  ];
}

/**
 * The oklch hue (degrees) of the most saturated third of the pixels (RGBA bytes), averaged as
 * vectors so reds either side of 0 do not cancel; null for an image without colour.
 */
export function dominantHue(rgba: Uint8ClampedArray): number | null {
  const px: Array<[number, number, number]> = [];
  for (let i = 0; i + 3 < rgba.length; i += 4) {
    if (rgba[i + 3]! < 128) continue;
    const [a, b] = ab(rgba[i]!, rgba[i + 1]!, rgba[i + 2]!);
    px.push([a, b, Math.hypot(a, b)]);
  }
  if (px.length === 0) return null;
  px.sort((x, y) => y[2] - x[2]);
  const top = px.slice(0, Math.max(1, Math.ceil(px.length / 3)));
  let sa = 0;
  let sb = 0;
  let sc = 0;
  for (const [a, b, c] of top) {
    sa += a;
    sb += b;
    sc += c;
  }
  if (sc / top.length < GREY) return null;
  const deg = (Math.atan2(sb, sa) * 180) / Math.PI;
  return (deg + 360) % 360;
}

export function tintFor(hue: number): string {
  return `oklch(0.20 0.035 ${Math.round(hue) % 360})`;
}

const cache = new Map<string, string | null>();
const pending = new Map<string, Promise<string | null>>();

function measure(name: string): Promise<string | null> {
  const url = imageUrl(name, "s");
  if (url === null) return Promise.resolve(null);
  const known = pending.get(name);
  if (known) return known;
  const p = new Promise<string | null>((resolve) => {
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.decoding = "async";
    img.onload = () => {
      try {
        const canvas = document.createElement("canvas");
        canvas.width = SIDE;
        canvas.height = SIDE;
        const ctx = canvas.getContext("2d", { willReadFrequently: true });
        if (ctx === null) return resolve(null);
        ctx.drawImage(img, 0, 0, SIDE, SIDE);
        const hue = dominantHue(ctx.getImageData(0, 0, SIDE, SIDE).data);
        const tint = hue === null ? null : tintFor(hue);
        cache.set(name, tint);
        resolve(tint);
      } catch {
        // A canvas the asset protocol tainted cannot be read: the hue stays unknown.
        resolve(null);
      }
    };
    img.onerror = () => resolve(null);
    img.src = url;
  }).finally(() => pending.delete(name));
  pending.set(name, p);
  return p;
}

/** The tint for a poster image name, or null while unknown or when it has no colour. */
export function useTint(imageName: string | null): string | null {
  const [known, setKnown] = useState<{ name: string; tint: string | null } | null>(() =>
    imageName !== null && cache.has(imageName)
      ? { name: imageName, tint: cache.get(imageName)! }
      : null,
  );

  useEffect(() => {
    if (imageName === null) return;
    if (cache.has(imageName)) {
      setKnown({ name: imageName, tint: cache.get(imageName)! });
      return;
    }
    let live = true;
    void measure(imageName).then((tint) => {
      if (live) setKnown({ name: imageName, tint });
    });
    return () => {
      live = false;
    };
  }, [imageName]);

  return known !== null && known.name === imageName ? known.tint : null;
}
