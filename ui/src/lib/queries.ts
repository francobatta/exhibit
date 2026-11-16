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
import { useEffect, useMemo } from "react";
import * as api from "./api";
import type {
  ApiError,
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
  needsReview: ["library", "needs-review"] as const,
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
 * far, flattened. A new query keeps showing the last one's items until its own arrive, so a list
 * that is re-sorted or filtered stays in place (and keeps focus) meanwhile.
 */
export function useItems(query: Omit<SearchQuery, "limit" | "offset">) {
  const result = useInfiniteQuery({
    queryKey: keys.items(query),
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

/** The folders a full record's files are on, as the library knows them now; none until both are known. */
export function useItemSources(detail: ItemDetail | undefined): Source[] {
  const sources = useStatus().data?.sources;
  return useMemo(() => {
    const ids = new Set(detail?.files.map((f) => f.source_id));
    return sources?.filter((s) => ids.has(s.id)) ?? [];
  }, [detail, sources]);
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

/** Starts playback; the end arrives as `play-ended` and refreshes the library. */
export function usePlay() {
  return useMutation<void, ApiError, { id: Id; resume: boolean }>({
    mutationFn: ({ id, resume }) => api.play(id, resume),
  });
}
