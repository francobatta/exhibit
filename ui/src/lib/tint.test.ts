import { describe, expect, test } from "bun:test";
import { dominantHue, tintFor } from "./tint";

/** RGBA pixels from [r, g, b] triples. */
function pixels(...rgb: Array<[number, number, number]>): Uint8ClampedArray {
  return new Uint8ClampedArray(rgb.flatMap(([r, g, b]) => [r, g, b, 255]));
}

type Rgb = [number, number, number];
const repeat = (n: number, v: Rgb): Rgb[] => Array.from({ length: n }, () => v);

describe("dominantHue", () => {
  test("follows the most saturated third, not the grey majority", () => {
    // Two thirds near-grey, one third strong orange (oklch hue about 50).
    const hue = dominantHue(pixels(...repeat(20, [90, 90, 92]), ...repeat(10, [230, 110, 30])));
    expect(hue).not.toBeNull();
    expect(hue!).toBeGreaterThan(35);
    expect(hue!).toBeLessThan(70);
  });

  test("blue reads as blue", () => {
    const hue = dominantHue(pixels(...repeat(12, [30, 60, 200])));
    expect(hue!).toBeGreaterThan(240);
    expect(hue!).toBeLessThan(275);
  });

  test("a grey image has no hue", () => {
    expect(dominantHue(pixels(...repeat(30, [120, 120, 120]), ...repeat(5, [20, 20, 20])))).toBeNull();
    expect(dominantHue(new Uint8ClampedArray())).toBeNull();
  });
});

test("tintFor sets the hue at low lightness and chroma", () => {
  expect(tintFor(52.4)).toBe("oklch(0.20 0.035 52)");
});
