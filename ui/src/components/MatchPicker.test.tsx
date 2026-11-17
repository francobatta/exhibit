import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { detail } from "../../test/fixtures";
import { tauri } from "../../test/setup";
import { installBridge } from "@/focus/bridge";
import { MatchPicker } from "./MatchPicker";

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

test("without a TMDB key it offers Settings and never looks anything up", async () => {
  tauri.handle("status", () => ({ movies: 0, shows: 0, other: 1, needs_review: 1, unprobed: 0, sources: [], tmdb_key: false, agent_key: false, tools: [] }));
  tauri.handle("item", () => detail({}, { id: 7, kind: "other", title: "Solaris", match_state: "needsreview" }));
  tauri.handle("lookup", () => []);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <MatchPicker itemId={7} open onClose={() => {}} />
    </QueryClientProvider>,
  );
  await act(() => sleep(80));
  const dialog = document.querySelector('[role="dialog"]');
  expect(dialog?.textContent).toContain("TMDB, which needs a key");
  expect(document.activeElement?.textContent).toBe("Open Settings");
  expect(tauri.calls().some((c) => c.cmd === "lookup")).toBe(false);
});
