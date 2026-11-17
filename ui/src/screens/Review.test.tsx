import { act, cleanup } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { activeKey, screens, select, settle } from "../../test/app";
import { detail } from "../../test/fixtures";
import { tauri } from "../../test/setup";
import { focusItem, installBridge } from "@/focus/bridge";
import type { Candidate, Id } from "@/lib/api";

let uninstall: () => void = () => {};
beforeEach(() => {
  uninstall = installBridge();
  tauri.handle("status", () => ({ movies: 0, shows: 0, other: 3, needs_review: 3, unprobed: 0, sources: [], tmdb_key: true, agent_key: false, tools: [] }));
  tauri.handle("needs_review", () => [entry(11, "Solaris"), entry(12, "Crash"), entry(13, "Heat")]);
});
afterEach(() => {
  cleanup();
  uninstall();
  document.body.innerHTML = "";
});

const candidate = (tmdb_id: number, kind: Candidate["kind"], score: number): Candidate => ({ tmdb_id, kind, title: "x", year: 1972, score });

function entry(id: Id, title: string) {
  return detail(
    { files: [{ id, source_id: 1, rel_path: `${title}.mkv`, size: 1, mtime: 0, item_id: id, tech: null }] },
    {
      id,
      kind: "other",
      title,
      match_state: "needsreview",
      candidates: [candidate(id * 100, "movie", 0.82), candidate(id * 100 + 1, "show", 0.61)],
    },
  );
}

test("choosing a candidate applies it with the candidate's kind and id", async () => {
  tauri.handle("apply_match", () => null);
  screens({ screen: "review" });
  await settle(120);
  await act(async () => {
    focusItem("review-11:item:1");
  });
  await select();
  await settle();
  expect(tauri.calls().filter((c) => c.cmd === "apply_match").map((c) => c.args)).toEqual([{ itemId: 11, tmdbId: 1101, kind: "show" }]);
});

test("after Send to Other the entry leaves and focus is on the next entry", async () => {
  tauri.handle("move_to_other", () => null);
  screens({ screen: "review" });
  await settle(120);
  await act(async () => {
    focusItem("review-12:other");
  });
  await select();
  await settle(120);
  expect(tauri.calls().filter((c) => c.cmd === "move_to_other").map((c) => c.args)).toEqual([{ id: 12 }]);
  expect(document.querySelector('article[aria-label="Crash.mkv"]')).toBeNull();
  expect(activeKey()).toBe("review-13:item:0");
});
