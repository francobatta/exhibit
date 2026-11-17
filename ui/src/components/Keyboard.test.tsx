import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { useState } from "react";
import { installBridge } from "@/focus/bridge";
import { Keyboard } from "./Keyboard";

let uninstall: () => void = () => {};
beforeEach(() => {
  uninstall = installBridge();
});
afterEach(() => {
  cleanup();
  uninstall();
  document.body.innerHTML = "";
});

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

function Harness({ seen }: { seen: string[] }) {
  const [value, setValue] = useState("");
  return (
    <Keyboard
      open
      label="Title"
      value={value}
      onChange={(v) => {
        seen.push(v);
        setValue(v);
      }}
      onDone={() => {}}
      onClose={() => {}}
    />
  );
}

const key = (label: string) =>
  [...document.querySelectorAll<HTMLButtonElement>("[data-focus-key^='key:']")].find((b) => b.textContent === label)!;

test("typing a, b, then Delete and Space leaves 'a '", async () => {
  const seen: string[] = [];
  render(<Harness seen={seen} />);
  await act(() => sleep(40));
  for (const label of ["a", "b", "Delete", "Space"]) {
    await act(async () => {
      key(label).click();
      await sleep(10);
    });
  }
  expect(seen.at(-1)).toBe("a ");
  expect(document.querySelector("[data-keyboard-text]")?.textContent).toBe("a ");
});
