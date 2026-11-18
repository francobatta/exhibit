import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { until } from "../../test/app";
import { tauri } from "../../test/setup";
import { Intro } from "./Intro";

const layer = () => document.querySelector('[data-slot="intro"]');

function mount() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <Intro />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  tauri.handle("boot", () => new Promise(() => {}));
});
afterEach(() => {
  cleanup();
  delete window.__exhibitIntro;
});

test("without the shell's word, there is no intro", () => {
  mount();
  expect(layer()).toBeNull();
});

test("a press during the intro goes no further, and the intro still opens onto Home", async () => {
  window.__exhibitIntro = true;
  const reached: string[] = [];
  const onKey = (e: KeyboardEvent) => reached.push(e.key);
  document.addEventListener("keydown", onKey);
  mount();
  expect(layer()).not.toBeNull();

  await act(async () => {
    document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
  });
  expect(reached).toEqual([]);

  // The library never answers here; the intro opens anyway once its wait runs out.
  await until(() => layer() === null, 2000);
  expect(layer()).toBeNull();
  await act(async () => {
    document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
  });
  expect(reached).toEqual(["Enter"]);
  document.removeEventListener("keydown", onKey);
});
