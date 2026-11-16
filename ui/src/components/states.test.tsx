import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, mock, test } from "bun:test";
import { installBridge } from "@/focus/bridge";
import { FocusScope } from "@/focus/react";
import { Empty, ErrorState } from "./states";

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

test("an error's action is a focus item the pad reaches and presses", async () => {
  const retry = mock(() => {});
  render(
    <FocusScope id="page">
      <ErrorState message="The library could not be read." hint="Try again." action={{ label: "Try again", onAction: retry }} />
    </FocusScope>,
  );
  await act(() => sleep(20));
  const button = document.activeElement as HTMLElement;
  expect(button.textContent).toBe("Try again");
  expect(button.dataset.focusKey).toBe("state:Try again");
  await act(async () => {
    button.click();
    await sleep(5);
  });
  expect(retry).toHaveBeenCalledTimes(1);
});

test("an empty state's action is reached by an arrow when focus has nowhere else", async () => {
  const add = mock(() => {});
  render(<Empty title="Nothing on the walls yet" action={{ label: "Add folder", onAction: add }} />);
  await act(async () => {
    document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true, cancelable: true }));
    await sleep(10);
  });
  expect((document.activeElement as HTMLElement).dataset.focusKey).toBe("state:Add folder");
});
