import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { summary } from "../../test/fixtures";
import { tauri } from "../../test/setup";
import { installBridge } from "@/focus/bridge";
import { Ask, recentSearches, rememberSearch } from "./Ask";

let uninstall: () => void = () => {};
beforeEach(() => {
  uninstall = installBridge();
  localStorage.clear();
});
afterEach(() => {
  cleanup();
  uninstall();
  document.body.innerHTML = "";
});

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

function type(text: string) {
  const field = document.querySelector<HTMLInputElement>('[data-focus-key="ask:field"]')!;
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(field, text);
  field.dispatchEvent(new Event("input", { bubbles: true }));
}

test("searches once typing pauses, and shows the results as a shelf per kind", async () => {
  // As the core answers: titles by default (an episode's hit counts for its show), episodes when asked.
  tauri.handle("search", (args) =>
    (args.query as { kind?: string }).kind === "episode"
      ? [summary({ id: 3, kind: "episode", title: "Heat Wave", show_title: "Severance", season: 1, episode: 2 })]
      : [summary({ id: 1, title: "Heat" }), summary({ id: 2, kind: "show", title: "Heated" })],
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <Ask open />
    </QueryClientProvider>,
  );
  await act(() => sleep(40));
  await act(async () => {
    type("h");
    await sleep(60);
    type("he");
    await sleep(60);
    type("heat");
    await sleep(100);
  });
  expect(tauri.calls().filter((c) => c.cmd === "search")).toEqual([]);
  await act(() => sleep(250));
  expect(tauri.calls().filter((c) => c.cmd === "search").map((c) => c.args)).toEqual([
    { query: { text: "heat", limit: 40 } },
    { query: { text: "heat", kind: "episode", limit: 40 } },
  ]);
  // The results arrive and render.
  await act(() => sleep(60));
  const shelves = [...document.querySelectorAll("section[aria-label]")].map((s) => s.getAttribute("aria-label"));
  expect(shelves).toEqual(["Movies", "Shows", "Episodes"]);
});

test("keeps the last six distinct searches, newest first", () => {
  for (const q of ["heat", "ran", "Heat ", "stalker", "paprika", "ikiru", "nomadland", "solaris"]) rememberSearch(q);
  expect(recentSearches()).toEqual(["solaris", "nomadland", "ikiru", "paprika", "stalker", "Heat"]);
});
