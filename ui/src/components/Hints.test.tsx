import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, expect, test } from "bun:test";
import { useFocusState } from "@/focus/bridge";
import { Hints, hints, type HintContext } from "./Hints";

afterEach(() => {
  cleanup();
  useFocusState.setState({ mode: "keys", key: null, item: null, canBack: false, canAsk: false, modal: null, ownHints: false });
});

const at = (over: Partial<HintContext> = {}): HintContext => ({
  canBack: true,
  canAsk: true,
  sections: true,
  askLabel: "Search",
  ...over,
});
// The status is never fetched here: Y is Search until an agent is set up.
const line = () =>
  render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { enabled: false } } })}>
      <Hints />
    </QueryClientProvider>,
  );
const said = (list: ReturnType<typeof hints>) => list.map((h) => `${h.action} ${h.label}`).join(", ");

test("names what each button does for the focused item, and only what works there", () => {
  const item = { key: "x", onSelect: () => {}, onMenu: () => {}, hint: "Play" };
  expect(said(hints(item, at()))).toBe("select Play, menu Menu, ask Search, sections Sections, back Back");
  expect(said(hints({ key: "x" }, at({ canBack: false, sections: false })))).toBe("ask Search");
  expect(said(hints({ key: "x", onSelect: () => {}, disabled: true }, at({ canAsk: false, sections: false })))).toBe(
    "back Back",
  );
  // A toggle presses itself but names what A does; a slider's end says what the sides and LB RB do.
  expect(said(hints({ key: "x", hint: "Include" }, at({ canAsk: false, sections: false, canBack: false })))).toBe(
    "select Include",
  );
  expect(said(hints({ key: "x", adjust: "Adjust", leap: "10 years" }, at({ canAsk: false, canBack: false })))).toBe(
    "adjust Adjust, sections 10 years",
  );
});

test("the pad sees its buttons, the keyboard its keys, the mouse nothing", () => {
  useFocusState.setState({ item: { key: "x", onSelect: () => {} }, canBack: true, canAsk: true });
  const r = line();
  const shown = () => r.container.querySelector("[data-hints]");
  expect(shown()?.getAttribute("data-hints")).toBe("keys");
  expect(shown()?.textContent).toBe("EnterSelect/Search[]SectionsEscBack");
  act(() => useFocusState.getState().setMode("pad"));
  expect(shown()?.textContent).toBe("ASelectYSearchLBRBSectionsBBack");
  act(() => useFocusState.getState().setMode("pointer"));
  expect(shown()).toBeNull();
});

test("a dialog has no sections, and one with its own hints hides the line", () => {
  useFocusState.setState({ mode: "pad", item: { key: "x", onSelect: () => {} }, canBack: true, modal: "confirm" });
  const r = line();
  expect(r.container.textContent).toBe("ASelectBBack");
  act(() => useFocusState.setState({ modal: "keyboard", ownHints: true }));
  expect(r.container.textContent).toBe("");
});
