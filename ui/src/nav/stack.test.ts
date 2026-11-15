import { beforeEach, describe, expect, test } from "bun:test";
import { useNav } from "./stack";

const nav = () => useNav.getState();

beforeEach(() => nav().replaceRoot({ screen: "home" }));

describe("navigation stack", () => {
  test("starts at its root", () => {
    expect(nav().current).toEqual({ screen: "home" });
    expect(nav().entries).toHaveLength(1);
  });

  test("push and back return to the entry and its focus", () => {
    nav().setFocusKey("shelf:recent:12");
    nav().push({ screen: "details", id: 12 });
    expect(nav().current).toEqual({ screen: "details", id: 12 });
    expect(nav().focusKey).toBeNull();

    nav().setFocusKey("play");
    nav().push({ screen: "settings" });
    expect(nav().back()).toBe(true);
    expect(nav().current).toEqual({ screen: "details", id: 12 });
    expect(nav().focusKey).toBe("play");

    expect(nav().back()).toBe(true);
    expect(nav().current).toEqual({ screen: "home" });
    expect(nav().focusKey).toBe("shelf:recent:12");
  });

  test("back at the root stays and says so", () => {
    expect(nav().back()).toBe(false);
    expect(nav().current).toEqual({ screen: "home" });
    expect(nav().entries).toHaveLength(1);
  });

  test("replaceRoot clears the stack", () => {
    nav().push({ screen: "details", id: 1 });
    nav().push({ screen: "show", id: 2 });
    nav().replaceRoot({ screen: "wall", kind: "movie" });
    expect(nav().entries).toHaveLength(1);
    expect(nav().current).toEqual({ screen: "wall", kind: "movie" });
    expect(nav().focusKey).toBeNull();
    expect(nav().back()).toBe(false);
  });

  test("each entry has its own identity, and the direction of the last move is known", () => {
    const first = nav().entries[0]?.id;
    nav().push({ screen: "details", id: 3 });
    expect(nav().direction).toBe("push");
    expect(nav().entries[1]?.id).not.toBe(first);
    nav().back();
    expect(nav().direction).toBe("back");
    expect(nav().entries[0]?.id).toBe(first);
  });
});
