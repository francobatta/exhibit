import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, test } from "bun:test";
import { FitTitle, fitWidth } from "./FitTitle";

afterEach(cleanup);

describe("fitWidth", () => {
  test("a short title stretches to the widest", () => {
    expect(fitWidth(200, 736)).toBe(125);
  });
  test("a title that fits once narrowed takes the ratio", () => {
    expect(fitWidth(800, 736)).toBeCloseTo(92, 5);
    expect(fitWidth(736, 736)).toBe(100);
  });
  test("a very long title goes to the narrowest", () => {
    expect(fitWidth(4000, 736)).toBe(62);
  });
  test("nothing measured leaves the normal width", () => {
    expect(fitWidth(0, 736)).toBe(100);
    expect(fitWidth(300, 0)).toBe(100);
    expect(fitWidth(Number.NaN, 736)).toBe(100);
  });
});

test("renders its text whole, as one heading", () => {
  const { container } = render(<FitTitle text="Y Tu Mamá También" />);
  expect(container.querySelector("h2")?.textContent).toBe("Y Tu Mamá También");
});
