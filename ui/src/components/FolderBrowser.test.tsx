import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { press, select, settle } from "../../test/app";
import { tauri } from "../../test/setup";
import { installBridge } from "@/focus/bridge";
import { FolderBrowser, parentOf } from "./FolderBrowser";

let uninstall: () => void = () => {};
beforeEach(() => {
  uninstall = installBridge();
});
afterEach(() => {
  cleanup();
  uninstall();
  document.body.innerHTML = "";
});

const TREE: Record<string, string[]> = {
  "": ["C:\\", "D:\\"],
  "D:\\": ["D:\\Films", "D:\\Shows"],
  "D:\\Films": ["D:\\Films\\New"],
};

test("the folder above a path; none above a drive", () => {
  expect(parentOf("D:\\Films\\New")).toBe("D:\\Films");
  expect(parentOf("D:\\Films")).toBe("D:\\");
  expect(parentOf("D:\\")).toBeNull();
});

test("descends into a folder, goes up a level with Back, and picks the folder shown", async () => {
  tauri.handle("list_folders", ({ path }) =>
    (TREE[(path as string | null) ?? ""] ?? []).map((p) => ({ name: p.split("\\").filter(Boolean).at(-1), path: p, file: false })),
  );
  const picked: string[] = [];
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <FolderBrowser open mode="folder" title="Add a folder" placeholder="Or type a folder's path" onPick={(p) => picked.push(p)} onClose={() => {}} />
    </QueryClientProvider>,
  );
  await settle(120);
  const shown = () => document.querySelector('[data-focus-scope="browser"]')!.textContent!;
  expect(shown()).toContain("Drives");

  // D:\, then Films.
  await press("ArrowDown");
  await select();
  await settle();
  expect(shown()).toContain("Shows");
  await select();
  await settle();
  expect(shown()).toContain("D:\\Films");
  expect(shown()).toContain("New");

  // Back goes up a level, to the folder just left.
  await press("Escape");
  await settle();
  expect(shown()).toContain("Shows");
  expect(document.activeElement?.textContent).toBe("Films");

  // "Add this folder" picks the folder shown.
  const add = [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Add this folder")!;
  await act(async () => {
    add.focus();
  });
  await select();
  expect(picked).toEqual(["D:\\"]);
});
