// Typed wrappers over the Tauri commands and events of `crates/exhibit-app`, and every type
// they carry. Field names mirror the serde output exactly (core types are snake_case).

import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Id = number;

export type ItemKind = "movie" | "show" | "season" | "episode" | "other";
export type MatchState = "unmatched" | "matched" | "needsreview" | "manual";
export type MatchSource = "nfo" | "lookup" | "agent" | "manual";
export type Sort = "title" | "added" | "year" | "runtime";
export type Shelf = "continue_watching" | "recently_added" | "unwatched" | "loved";
export type Tool = "mpv" | "ffprobe" | "ffmpeg";
export type SecretName = "tmdb" | "agent";
export type IdentifyScope = "unmatched" | "review" | "unmatched_and_review" | { item: Id };
export type RefreshPhase = "scanning" | "identifying" | "probing" | "rescoring" | "thumbnails";

export interface Person {
  name: string;
  /** "director" | "cast" */
  role: string;
}

export interface Candidate {
  tmdb_id: number;
  kind: ItemKind;
  title: string;
  year: number | null;
  score: number;
}

export interface Item {
  id: Id;
  kind: ItemKind;
  parent_id: Id | null;
  title: string;
  year: number | null;
  season: number | null;
  episode: number | null;
  overview: string | null;
  runtime_min: number | null;
  genres: string[];
  people: Person[];
  poster: string | null;
  backdrop: string | null;
  tmdb_id: number | null;
  match_state: MatchState;
  match_source: MatchSource | null;
  confidence: number | null;
  candidates: Candidate[];
  loved: boolean;
  added_at: number;
}

export interface AudioTrack {
  codec: string;
  channels: number;
  lang: string | null;
}

export interface SubTrack {
  codec: string;
  lang: string | null;
}

export interface TechInfo {
  duration_s: number;
  width: number;
  height: number;
  video_codec: string;
  /** "HDR10" | "HLG" | "Dolby Vision" */
  hdr: string | null;
  audio: AudioTrack[];
  subtitles: SubTrack[];
}

export interface MediaFile {
  id: Id;
  source_id: Id;
  rel_path: string;
  size: number;
  mtime: number;
  item_id: Id | null;
  tech: TechInfo | null;
}

export interface PlayState {
  item_id: Id;
  position_s: number;
  duration_s: number;
  watched: boolean;
  last_played_at: number;
}

export interface ItemSummary {
  id: Id;
  kind: ItemKind;
  title: string;
  year: number | null;
  season: number | null;
  episode: number | null;
  show_title: string | null;
  poster: string | null;
  backdrop: string | null;
  runtime_min: number | null;
  width: number | null;
  height: number | null;
  hdr: string | null;
  watched: boolean;
  progress: number | null;
  loved: boolean;
  available: boolean;
  match_state: MatchState;
}

export interface ItemDetail {
  item: Item;
  files: MediaFile[];
  play: PlayState | null;
  children: ItemSummary[];
  available: boolean;
  /** For a show, the episode to play next. */
  next_episode: ItemSummary | null;
}

export interface HistoryEntry {
  item: ItemSummary;
  started_at: number;
  ended_at: number;
  position_s: number;
}

export interface SearchQuery {
  text?: string | null;
  kind?: ItemKind | null;
  genre?: string | null;
  year_min?: number | null;
  year_max?: number | null;
  runtime_max_min?: number | null;
  watched?: boolean | null;
  loved?: boolean | null;
  min_width?: number | null;
  min_height?: number | null;
  available?: boolean | null;
  sort?: Sort | null;
  limit?: number | null;
  offset?: number | null;
}

export interface Source {
  id: Id;
  path: string;
  volume_id: string;
  online: boolean;
}

export interface ToolStatus {
  tool: Tool;
  path: string | null;
  version: string | null;
  manual: boolean;
}

export interface LibraryStatus {
  movies: number;
  shows: number;
  other: number;
  needs_review: number;
  unprobed: number;
  sources: Source[];
  tmdb_key: boolean;
  agent_key: boolean;
  tools: ToolStatus[];
}

export interface Boot {
  status: LibraryStatus;
  imageDir: string;
  /** No sources yet. */
  firstRun: boolean;
}

export interface Folder {
  name: string;
  path: string;
}

export interface JobStatus {
  /** `null` when the worker is idle. */
  phase: RefreshPhase | null;
  /** Files or items handled so far in the current phase. */
  done: number;
  message: string | null;
  /** The most recent job's failure, kept until the next job ends. */
  last_error: string | null;
}

export interface PlayOutcome {
  item_id: Id;
  position_s: number;
  duration_s: number;
  watched: boolean;
}

export type ErrorKind =
  | "not_found"
  | "invalid"
  | "source_offline"
  | "tool"
  | "provider"
  | "io"
  | "db";

/** What a failed command rejects with. */
export interface ApiError {
  kind: ErrorKind;
  message: string;
}

export interface PlayEnded {
  outcome: PlayOutcome | null;
  error: ApiError | null;
  /** The show's next episode when the played episode ended watched. */
  nextEpisode: Id | null;
}

export function isApiError(e: unknown): e is ApiError {
  return (
    typeof e === "object" &&
    e !== null &&
    typeof (e as { kind?: unknown }).kind === "string" &&
    typeof (e as { message?: unknown }).message === "string"
  );
}

/**
 * The message of an error as the owner should read it: the core's `<kind>: ` prefix removed.
 * Also takes a bare message (such as `JobStatus.last_error`) or anything a promise rejected with.
 */
export function plain(e: unknown): string {
  const text = isApiError(e)
    ? e.message
    : typeof e === "string"
      ? e
      : e instanceof Error
        ? e.message
        : "";
  const stripped = text.trim().replace(/^[a-z_]+: /, "");
  if (stripped === "") return "Something went wrong.";
  return stripped.charAt(0).toUpperCase() + stripped.slice(1);
}

// Commands. Each rejects with an `ApiError`.

let imageDir: string | null = null;

/** Also remembers the image folder for `imageUrl`. */
export async function boot(): Promise<Boot> {
  const b = await invoke<Boot>("boot");
  imageDir = b.imageDir;
  return b;
}

export const status = () => invoke<LibraryStatus>("status");
export const search = (query: SearchQuery) => invoke<ItemSummary[]>("search", { query });
export const item = (id: Id) => invoke<ItemDetail>("item", { id });
export const shelf = (shelf: Shelf, limit: number) =>
  invoke<ItemSummary[]>("shelf", { shelf, limit });
export const needsReview = () => invoke<ItemDetail[]>("needs_review");
export const history = (limit: number) => invoke<HistoryEntry[]>("history", { limit });
export const setWatched = (id: Id, watched: boolean) =>
  invoke<void>("set_watched", { id, watched });
export const setLoved = (id: Id, loved: boolean) => invoke<void>("set_loved", { id, loved });
export const applyMatch = (itemId: Id, tmdbId: number, kind: ItemKind) =>
  invoke<void>("apply_match", { itemId, tmdbId, kind });
export const moveToOther = (id: Id) => invoke<void>("move_to_other", { id });
export const lookup = (text: string, kind: ItemKind, year: number | null) =>
  invoke<Candidate[]>("lookup", { text, kind, year });
/** Also queues the new folder's first refresh. */
export const addSource = (path: string) => invoke<Source>("add_source", { path });
export const removeSource = (id: Id) => invoke<void>("remove_source", { id });
/** Drive roots for `null`; otherwise visible sub-folders, sorted. */
export const listFolders = (path: string | null) => invoke<Folder[]>("list_folders", { path });
export const detectTools = () => invoke<ToolStatus[]>("detect_tools");
export const setToolPath = (tool: Tool, path: string | null) =>
  invoke<void>("set_tool_path", { tool, path });
/** `null` clears it. The value is never sent back. */
export const setSecret = (name: SecretName, value: string | null) =>
  invoke<void>("set_secret", { name, value });
/** Queues a refresh of one source or all; returns at once. */
export const refresh = (source: Id | null) => invoke<void>("refresh", { source });
/** Queues an identification; returns at once. */
export const identify = (scope: IdentifyScope) => invoke<void>("identify", { scope });
export const jobStatus = () => invoke<JobStatus>("job_status");
/** Resolves once the player has started; the end arrives as `play-ended`. */
export const play = (id: Id, resume: boolean) => invoke<void>("play", { id, resume });
export const windowFullscreen = (on: boolean) => invoke<void>("window_fullscreen", { on });

// Events. Each returns a promise of its unlisten function.

export const onLibraryChanged = (cb: () => void): Promise<UnlistenFn> =>
  listen("library-changed", () => cb());
export const onJobStatus = (cb: (s: JobStatus) => void): Promise<UnlistenFn> =>
  listen<JobStatus>("job-status", (e) => cb(e.payload));
export const onPlayEnded = (cb: (p: PlayEnded) => void): Promise<UnlistenFn> =>
  listen<PlayEnded>("play-ended", (e) => cb(e.payload));

// Artwork.

/** `movie-603-poster.jpg` has a small sibling `movie-603-poster-s.jpg`; other names do not. */
export function smallName(name: string): string {
  return name.endsWith("-poster.jpg") ? `${name.slice(0, -".jpg".length)}-s.jpg` : name;
}

/** A URL the page can load for an image file name stored on an item. */
export function imageUrl(name: string | null, size?: "s"): string | null {
  if (name === null || imageDir === null) return null;
  const file = size === "s" ? smallName(name) : name;
  const sep = imageDir.includes("\\") ? "\\" : "/";
  const dir = imageDir.endsWith(sep) ? imageDir : imageDir + sep;
  return convertFileSrc(dir + file);
}
