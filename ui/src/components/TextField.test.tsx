import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { installBridge, useFocusState, type InputMode } from "@/focus/bridge";
import { FocusGroup } from "@/focus/react";
import { TextField } from "./TextField";

let uninstall: () => void = () => {};
beforeEach(() => {
  uninstall = installBridge();
});
afterEach(() => {
  cleanup();
  uninstall();
  useFocusState.setState({ mode: "keys" });
  document.body.innerHTML = "";
});

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function selectIn(mode: InputMode) {
  render(
    <FocusGroup id="form" orientation="horizontal">
      <TextField focusKey="title" value="" onChange={() => {}} placeholder="Title" />
    </FocusGroup>,
  );
  const field = document.querySelector<HTMLInputElement>('[data-focus-key="title"]')!;
  await act(async () => {
    field.focus();
    useFocusState.getState().setMode(mode);
    // A, Enter or a click: the item is selected.
    field.click();
    await sleep(40);
  });
}

const keyboard = () => document.querySelector('[data-focus-scope="keyboard"]');

test("selected with the pad, the field opens the on-screen keyboard", async () => {
  await selectIn("pad");
  expect(keyboard()).not.toBeNull();
  expect(document.activeElement?.getAttribute("data-focus-key")).toBe("key:1:0");
});

test("selected with keys, the field keeps text focus and no keyboard opens", async () => {
  await selectIn("keys");
  expect(keyboard()).toBeNull();
  expect(document.activeElement?.getAttribute("data-focus-key")).toBe("title");
});
