import { afterEach, describe, expect, test } from "bun:test";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { tauri } from "../../test/setup";
import type { ItemSummary } from "./api";
import { LibrarySync, useJobStatus, useSetLoved, useShelf } from "./queries";

function summary(id: number, over: Partial<ItemSummary> = {}): ItemSummary {
  return {
    id,
    kind: "movie",
    title: `Film ${id}`,
    year: 1979,
    season: null,
    episode: null,
    show_title: null,
    poster: null,
    backdrop: null,
    runtime_min: 117,
    width: 1920,
    height: 1080,
    hdr: null,
    watched: false,
    progress: null,
    loved: false,
    available: true,
    match_state: "matched",
    genres: [],
    path: null,
    ...over,
  };
}

function setup() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>
      <LibrarySync />
      {children}
    </QueryClientProvider>
  );
  return { client, wrapper };
}

afterEach(cleanup);

describe("library queries", () => {
  test("a library-changed event refetches a mounted shelf", async () => {
    let titles = ["Alien"];
    tauri.handle("shelf", () => titles.map((t, i) => summary(i + 1, { title: t })));
    const { wrapper } = setup();
    const { result } = renderHook(() => useShelf("recently_added", 20), { wrapper });
    await waitFor(() => expect(result.current.data?.map((s) => s.title)).toEqual(["Alien"]));

    titles = ["Alien", "Heat"];
    await act(() => tauri.emit("library-changed"));
    await waitFor(() =>
      expect(result.current.data?.map((s) => s.title)).toEqual(["Alien", "Heat"]),
    );
    expect(tauri.calls().filter((c) => c.cmd === "shelf")).toHaveLength(2);
  });

  test("setLoved shows at once, and a failure puts the cache back", async () => {
    tauri.handle("shelf", () => [summary(1), summary(2)]);
    let fail: (e: unknown) => void = () => {};
    tauri.handle(
      "set_loved",
      () => new Promise((_, reject) => (fail = reject)),
    );
    const { wrapper } = setup();
    const { result } = renderHook(
      () => ({ shelf: useShelf("recently_added", 20), love: useSetLoved() }),
      { wrapper },
    );
    await waitFor(() => expect(result.current.shelf.data).toHaveLength(2));

    act(() => result.current.love.mutate({ id: 2, loved: true }));
    await waitFor(() => expect(result.current.shelf.data?.[1]?.loved).toBe(true));
    expect(result.current.shelf.data?.[0]?.loved).toBe(false);

    await act(async () => fail({ kind: "db", message: "db: database is locked" }));
    await waitFor(() => expect(result.current.love.isError).toBe(true));
    expect(result.current.shelf.data?.[1]?.loved).toBe(false);
  });

  test("job status follows the job-status event", async () => {
    tauri.handle("job_status", () => ({ phase: null, done: 0, message: null, last_error: null }));
    const { wrapper } = setup();
    const { result } = renderHook(() => useJobStatus(), { wrapper });
    await waitFor(() => expect(result.current).not.toBeNull());
    await act(() =>
      tauri.emit("job-status", { phase: "scanning", done: 214, message: null, last_error: null }),
    );
    await waitFor(() => expect(result.current?.done).toBe(214));
  });
});

test("the fake bridge runs in a DOM", () => {
  const { container } = render(<p>wall</p>);
  expect(container.textContent).toBe("wall");
});
