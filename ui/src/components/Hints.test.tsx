import { act, cleanup, render } from "@testing-library/react";
import { afterEach, expect, test } from "bun:test";
import { useFocusState } from "@/focus/bridge";
import { Hints, hints } from "./Hints";

afterEach(() => {
  cleanup();
  useFocusState.setState({ mode: "keys", key: null, item: null, canBack: false });
});

test("names what each button does for the focused item", () => {
  const all = hints({ key: "x", onSelect: () => {}, onMenu: () => {} }, true);
  expect(all.map(([b, l]) => `${b} ${l}`).join(", ")).toBe("A Open, X Menu, Y Ask, B Back");
  expect(hints({ key: "x" }, false)).toEqual([["Y", "Ask"]]);
  expect(hints({ key: "x", onSelect: () => {}, disabled: true }, false)).toEqual([["Y", "Ask"]]);
});

test("shows only in pad mode", () => {
  useFocusState.setState({ item: { key: "x", onSelect: () => {} }, canBack: true });
  const r = render(<Hints />);
  expect(r.container.textContent).toBe("");
  act(() => useFocusState.getState().setMode("pad"));
  expect(r.container.textContent).toBe("A Open, Y Ask, B Back");
  act(() => useFocusState.getState().setMode("pointer"));
  expect(r.container.textContent).toBe("");
});
