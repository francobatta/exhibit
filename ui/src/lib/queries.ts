// TanStack Query over `api.ts`. Every library query lives under the "library" key, so the one
// `library-changed` listener (LibrarySync) refreshes them all. Mutations show their effect at once
// and put the cache back if the command fails.

import {
  keepPreviousData,
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
  type InfiniteData,
  type QueryClient,
  type QueryKey,
} from "@tanstack/react-query";
import { useCallback, useEffect, useMemo } from "react";
import * as api from "./api";
import type {
  ApiError,
  DownloadStatus,
  Id,
  ItemDetail,
  ItemKind,
  ItemSummary,
  JobStatus,
  SearchQuery,
  Shelf,
  Source,
} from "./api";

export const keys = {
  boot: ["boot"] as const,
  job: ["job"] as const,
  library: ["library"] as const,
  status: ["library", "status"] as const,
  shelf: (shelf: Shelf, limit: number) => ["library", "shelf", shelf, limit] as const,
  items: (query: SearchQuery) => ["library", "items", query] as const,
  item: (id: Id) => ["library", "item", id] as const,
  cast: (id: Id) => ["library", "cast", id] as const,
  needsReview: ["library", "needs-review"] as const,
  ask: (text: string) => ["library", "ask", text] as const,
  // Not under "library": detecting runs each tool, so a library change does not ask again.
  tools: ["tools"] as const,
  toolsDownload: ["tools-download"] as const,
  fullscreen: ["fullscreen"] as const,
};

const PAGE = 500;

/** Mount once, inside the QueryClientProvider: refreshes the library when it changes. */
export function LibrarySync(): null {
  const client = useQueryClient();
  useEffect(() => {
    const off = api.onLibraryChanged(() => {
      void client.invalidateQueries({ queryKey: keys.library });
    });
    return () => {
      void off.then((f) => f());
    };
  }, [client]);
  return null;
}

/** Boot once: the status at start, the image folder and whether this is the first run. */
export function useBoot() {
  return useQuery({ queryKey: keys.boot, queryFn: api.boot, staleTime: Infinity });
}

export function useStatus() {
  return useQuery({ queryKey: keys.status, queryFn: api.status });
}

export function useShelf(shelf: Shelf, limit: number) {
  return useQuery({ queryKey: keys.shelf(shelf, limit), queryFn: () => api.shelf(shelf, limit) });
}

/**
 * Every item matching `query`, loaded in pages of 500 one after another; `items` is the pages so
 * far, flattened. With `enabled` false only what is cached is returned. A new query keeps showing
 * the last one's items until its own arrive, so a list that is re-sorted or filtered stays in
 * place (and keeps focus) meanwhile.
 */
export function useItems(query: Omit<SearchQuery, "limit" | "offset">, enabled = true) {
  const result = useInfiniteQuery({
    queryKey: keys.items(query),
    enabled,
    queryFn: ({ pageParam }) => api.search({ ...query, limit: PAGE, offset: pageParam }),
    initialPageParam: 0,
    getNextPageParam: (last, all) => (last.length < PAGE ? undefined : all.length * PAGE),
    placeholderData: keepPreviousData,
  });
  const { hasNextPage, isFetchingNextPage, fetchNextPage } = result;
  useEffect(() => {
    if (hasNextPage && !isFetchingNextPage) void fetchNextPage();
  }, [hasNextPage, isFetchingNextPage, fetchNextPage]);
  const items = useMemo(() => result.data?.pages.flat() ?? [], [result.data]);
  return { ...result, items };
}

/** Library changes refresh a record through `library-changed`; nothing else makes it stale. */
const ITEM_STALE = 10 * 60_000;

/**
 * One full record. With `fetch` false only what is cached is returned, so a screen can show a
 * record it has seen at once and ask for a new one only once focus has settled on it.
 */
export function useItem(id: Id | null, fetch = true) {
  return useQuery({
    queryKey: keys.item(id ?? -1),
    queryFn: () => api.item(id!),
    enabled: id !== null && fetch,
    staleTime: ITEM_STALE,
  });
}

/** A work's cast with portraits. Portraits are fetched once and kept, so it is asked for only once per visit. */
export function useCast(id: Id) {
  return useQuery({ queryKey: keys.cast(id), queryFn: () => api.cast(id), staleTime: Number.POSITIVE_INFINITY });
}

/** The folders a full record's files are on, as the library knows them now; none until both are known. */
export function useItemSources(detail: ItemDetail | undefined): Source[] {
  const sources = useStatus().data?.sources;
  return useMemo(() => {
    const ids = new Set(detail?.files.map((f) => f.source_id));
    return sources?.filter((s) => ids.has(s.id)) ?? [];
  }, [detail, sources]);
}

/**
 * The drive a work waits for, read on demand: the folder of its first file on an offline source.
 * A show's files are its episodes': it waits for its next episode's drive.
 */
export function useOfflineDrive(): (id: Id) => Promise<string | undefined> {
  const client = useQueryClient();
  return useCallback(
    async function driveOf(id: Id): Promise<string | undefined> {
      const [detail, status] = await Promise.all([
        client.fetchQuery({ queryKey: keys.item(id), queryFn: () => api.item(id), staleTime: ITEM_STALE }),
        client.fetchQuery({ queryKey: keys.status, queryFn: api.status }),
      ]);
      const next = detail.next_episode;
      if (detail.files.length === 0 && next !== null) return driveOf(next.id);
      const ids = new Set(detail.files.map((f) => f.source_id));
      return status.sources.find((s) => ids.has(s.id) && !s.online)?.path;
    },
    [client],
  );
}

/** What Ask shows for some text: groups of works, each with a title and perhaps why it is there. */
export interface AskResult {
  groups: Array<{ title: string; reason?: string; items: ItemSummary[] }>;
}

/** The kinds Ask groups by, in order; seasons are reached through their shows. */
const ASK_GROUPS: Array<[ItemKind, string]> = [
  ["movie", "Movies"],
  ["show", "Shows"],
  ["episode", "Episodes"],
  ["other", "Other"],
];

/** The library search as Ask's groups, by kind; a kind with nothing found is left out. */
export function askGroups(items: ItemSummary[]): AskResult {
  return {
    groups: ASK_GROUPS.map(([kind, title]) => ({ title, items: items.filter((i) => i.kind === kind) })).filter(
      (g) => g.items.length > 0,
    ),
  };
}

/** How many works Ask asks the library for, of the titles and of the episodes each. */
const ASK_LIMIT = 40;

/**
 * Ask's results for `text` (already debounced): in this stage, the library search by kind. The
 * plain search finds titles (an episode's hit counts for its show), so episodes are asked apart.
 */
export function useAsk(text: string) {
  return useQuery({
    queryKey: keys.ask(text),
    queryFn: async (): Promise<AskResult> => {
      const [titles, episodes] = await Promise.all([
        api.search({ text, limit: ASK_LIMIT }),
        api.search({ text, kind: "episode", limit: ASK_LIMIT }),
      ]);
      return askGroups([...titles, ...episodes]);
    },
    enabled: text !== "",
    placeholderData: keepPreviousData,
  });
}

/** The media tools, detected afresh each time a screen showing them opens, and after a path is set. */
export function useTools() {
  return useQuery({ queryKey: keys.tools, queryFn: api.detectTools, staleTime: 0, refetchOnMount: "always" });
}

/**
 * The mpv and ffmpeg download, kept current by the `tools-download` event, and `start`. Starting
 * shows at once, before the command answers. A finished download detects the tools again.
 */
export function useToolsDownload() {
  const client = useQueryClient();
  const { data } = useQuery({ queryKey: keys.toolsDownload, queryFn: api.toolsDownloadStatus, staleTime: Infinity });
  useEffect(() => {
    const off = api.onToolsDownload((s) => {
      client.setQueryData(keys.toolsDownload, s);
      if (s.outcome?.kind === "done") void client.invalidateQueries({ queryKey: keys.tools });
    });
    return () => {
      void off.then((f) => f());
    };
  }, [client]);
  const start = useCallback(() => {
    if (client.getQueryData<DownloadStatus>(keys.toolsDownload)?.running) return;
    client.setQueryData<DownloadStatus>(keys.toolsDownload, { running: true, step: { step: "checking" }, outcome: null });
    api.downloadTools().catch((e: unknown) => {
      client.setQueryData<DownloadStatus>(keys.toolsDownload, {
        running: false,
        step: null,
        outcome: { kind: "failed", message: api.plain(e) },
      });
    });
  }, [client]);
  return { status: data ?? null, start };
}

/** Whether the window is full screen, read when asked; `set` changes it and reads it back. */
export function useFullscreen() {
  const client = useQueryClient();
  const query = useQuery({ queryKey: keys.fullscreen, queryFn: api.windowIsFullscreen, staleTime: 0 });
  const set = useCallback(
    async (on: boolean) => {
      await api.windowFullscreen(on);
      await client.invalidateQueries({ queryKey: keys.fullscreen });
    },
    [client],
  );
  return { on: query.data, set };
}

export function useNeedsReview() {
  return useQuery({ queryKey: keys.needsReview, queryFn: api.needsReview });
}

/** The worker's status, kept current by the `job-status` event; null until first known. */
export function useJobStatus(): JobStatus | null {
  const client = useQueryClient();
  const { data } = useQuery({ queryKey: keys.job, queryFn: api.jobStatus, staleTime: Infinity });
  useEffect(() => {
    const off = api.onJobStatus((s) => client.setQueryData(keys.job, s));
    return () => {
      void off.then((f) => f());
    };
  }, [client]);
  return data ?? null;
}

// Optimistic edits. A library query holds summaries (arrays, or pages of them), one detail, or
// a list of details; `edit` rewrites the item with the given id wherever it appears.

type Edit = { summary?: (s: ItemSummary) => ItemSummary; detail?: (d: ItemDetail) => ItemDetail };

const isSummary = (v: unknown): v is ItemSummary =>
  typeof v === "object" && v !== null && "id" in v && "available" in v && !("item" in v);
const isDetail = (v: unknown): v is ItemDetail =>
  typeof v === "object" && v !== null && "item" in v && "files" in v;
const isInfinite = (v: unknown): v is InfiniteData<ItemSummary[]> =>
  typeof v === "object" && v !== null && "pages" in v && "pageParams" in v;

function editData(data: unknown, id: Id, e: Edit): unknown {
  if (Array.isArray(data)) {
    return data.map((v: unknown) => editData(v, id, e));
  }
  if (isInfinite(data)) {
    return { ...data, pages: data.pages.map((p) => editData(p, id, e) as ItemSummary[]) };
  }
  if (isSummary(data)) {
    return data.id === id && e.summary ? e.summary(data) : data;
  }
  if (isDetail(data)) {
    const d = data.item.id === id && e.detail ? e.detail(data) : data;
    return {
      ...d,
      children: d.children.map((c) => editData(c, id, e) as ItemSummary),
      next_episode: d.next_episode && (editData(d.next_episode, id, e) as ItemSummary),
    };
  }
  return data;
}

type Snapshot = Array<[QueryKey, unknown]>;

async function applyEdit(client: QueryClient, id: Id, e: Edit): Promise<Snapshot> {
  await client.cancelQueries({ queryKey: keys.library });
  const before = client.getQueriesData({ queryKey: keys.library });
  client.setQueriesData({ queryKey: keys.library }, (data: unknown) => editData(data, id, e));
  return before;
}

function restore(client: QueryClient, snapshot: Snapshot | undefined) {
  for (const [key, data] of snapshot ?? []) client.setQueryData(key, data);
}

function useOptimistic<V extends { id: Id }>(
  run: (v: V) => Promise<void>,
  edit: (v: V) => Edit,
) {
  const client = useQueryClient();
  return useMutation<void, ApiError, V, Snapshot>({
    mutationFn: run,
    onMutate: (v) => applyEdit(client, v.id, edit(v)),
    onError: (_e, _v, snapshot) => restore(client, snapshot),
  });
}

export function useSetWatched() {
  return useOptimistic<{ id: Id; watched: boolean }>(
    ({ id, watched }) => api.setWatched(id, watched),
    ({ watched }) => ({
      summary: (s) => ({ ...s, watched, progress: watched ? null : s.progress }),
      detail: (d) => ({ ...d, play: d.play && { ...d.play, watched } }),
    }),
  );
}

export function useSetLoved() {
  return useOptimistic<{ id: Id; loved: boolean }>(
    ({ id, loved }) => api.setLoved(id, loved),
    ({ loved }) => ({
      summary: (s) => ({ ...s, loved }),
      detail: (d) => ({ ...d, item: { ...d.item, loved } }),
    }),
  );
}

export function useApplyMatch() {
  return useOptimistic<{ id: Id; tmdbId: number; kind: ItemKind }>(
    ({ id, tmdbId, kind }) => api.applyMatch(id, tmdbId, kind),
    ({ tmdbId }) => ({
      summary: (s) => ({ ...s, match_state: "manual" }),
      detail: (d) => ({
        ...d,
        item: { ...d.item, match_state: "manual", match_source: "manual", tmdb_id: tmdbId },
      }),
    }),
  );
}

export function useMoveToOther() {
  return useOptimistic<{ id: Id }>(
    ({ id }) => api.moveToOther(id),
    () => ({
      summary: (s) => ({ ...s, kind: "other", match_state: "manual" }),
      detail: (d) => ({ ...d, item: { ...d.item, kind: "other", match_state: "manual" } }),
    }),
  );
}
