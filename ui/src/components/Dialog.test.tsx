import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { useState } from "react";
import { installBridge } from "@/focus/bridge";
import { ConfirmHost, Dialog, confirm } from "./Dialog";

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
const press = (key: string) =>
  act(async () => {
    (document.activeElement ?? document.body).dispatchEvent(
      new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }),
    );
    await sleep(40);
  });

function Harness({ log }: { log: string[] }) {
  const [open, setOpen] = useState(true);
  return (
    <Dialog
      id="d"
      open={open}
      title="Remove this folder?"
      body="Its works leave the walls."
      initial={1}
      actions={[
        { label: "Keep", onSelect: () => log.push("keep") },
        { label: "Remove", onSelect: () => log.push("remove"), primary: true },
      ]}
      onBack={() => {
        log.push("back");
        setOpen(false);
      }}
    />
  );
}

test("a dialog names itself, focuses the chosen button, moves between buttons and closes on Back", async () => {
  const log: string[] = [];
  document.body.innerHTML = `<button id="opener">opener</button>`;
  document.getElementById("opener")!.focus();
  render(<Harness log={log} />, { container: document.body.appendChild(document.createElement("div")) });
  await act(() => sleep(40));
  expect(document.querySelector('[role="dialog"]')?.getAttribute("aria-label")).toBe("Remove this folder?");
  expect(document.activeElement?.textContent).toBe("Remove");
  await press("ArrowLeft");
  expect(document.activeElement?.textContent).toBe("Keep");
  await act(async () => {
    (document.activeElement as HTMLElement).click();
    await sleep(10);
  });
  await press("Escape");
  await act(() => sleep(60));
  expect(log).toEqual(["keep", "back"]);
  expect(document.activeElement?.id).toBe("opener");
});

test("confirm resolves with the owner's answer", async () => {
  render(<ConfirmHost />);
  let answer: boolean | undefined;
  await act(async () => {
    void confirm({ title: "Send Clip to Other?", body: "", confirm: "Send to Other" }).then((a) => {
      answer = a;
    });
    await sleep(40);
  });
  await press("ArrowRight");
  await act(async () => {
    (document.activeElement as HTMLElement).click();
    await sleep(60);
  });
  expect(answer).toBe(true);
});
