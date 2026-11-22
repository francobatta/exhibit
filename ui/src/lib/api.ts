// Typed wrappers over the Tauri commands and events of `crates/exhibit-app`, and every type
// they carry. Field names mirror the serde output exactly (core types are snake_case).

import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { t, translated } from "@/i18n";

export type Id = number;

export type ItemKind = "movie" | "show" | "season" | "episode" | "other";
export type MatchState = "unmatched" | "matched" | "needsreview" | "manual";
export type MatchSource = "nfo" | "lookup" | "agent" | "manual";
/** A listing's order; `descending` unset takes the sort's own direction (`SORT_DESCENDS`). */
export type Sort = "title" | "year" | "added" | "runtime" | "played" | "my_rating" | "rating";
export type Seen = "unwatched" | "started" | "watched";
export type Resolution = "uhd" | "full_hd" | "hd" | "sd";
export type Picture = "dolby_vision" | "hdr10" | "hlg" | "sdr";
export type Sound = "atmos" | "dts_x" | "lossless" | "surround";
export type Shelf = "continue_watching" | "recently_added" | "unwatched" | "loved" | "watchlist";
export type Tool = "mpv" | "ffmpeg";

/** Where the copy in use came from: set by hand, Exhibit's own download, or found on the PC. */
export type ToolSource = "manual" | "downloaded" | "detected";
export type SecretName = "tmdb" | "agent";
export type IdentifyScope = "unmatched" | "review" | "unmatched_and_review" | { item: Id };
export type RefreshPhase =
  | "scanning"
  | "identifying"
  | "probing"
  | "rescoring"
  | "translating"
  | "thumbnails"
  | "reception"
  | "freshening";

export type Role = "director" | "writer" | "cinematographer" | "composer" | "editor" | "cast";

export interface Person {
  name: string;
  role: Role;
  character?: string;
}

/** What links two works (`similar`), strongest first. */
export type Link =
  | { kind: "recommended" }
  | { kind: "keyword"; keyword: string }
  | { kind: "person"; name: string; role: Role }
  | { kind: "genre"; genre: string };

export interface Similar {
  item: ItemSummary;
  score: number;
  because: Link[];
}

/** One of a work's cast; `portrait` is a cached image file name. */
export interface CastMember {
  name: string;
  character: string | null;
  portrait: string | null;
}

export interface Candidate {
  tmdb_id: number;
  kind: ItemKind;
  title: string;
  year: number | null;
  score: number;
  /** The cached poster's file name (its small sibling is on disk), as an item's `poster`. */
  poster: string | null;
  /** Its title in each language it was found in, when it was searched in several. */
  titles?: Partial<Record<string, string>>;
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
  /** The decoder's profile when it names more than the codec: "DTS-HD MA + DTS:X", "Dolby TrueHD + Dolby Atmos". */
  profile: string | null;
  /** The track's own name in the file ("Original", a dub studio). */
  title: string | null;
  /** The file marks it as the track to play. */
  default: boolean;
  /** The film's own soundtrack, not a dub. */
  original: boolean;
  commentary: boolean;
}

export interface SubTrack {
  codec: string;
  lang: string | null;
  /** Only the lines in another language, or signs. */
  forced: boolean;
  /** For the deaf and hard of hearing (SDH). */
  hearing_impaired: boolean;
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
  genres: string[];
  /** The first file, relative to its source, forward slashes; null for a show or season. */
  path: string | null;
  /** The work has a file; false for a film on the owner's watchlist they do not have yet. `available` says whether a file is online now. */
  in_library: boolean;
}

export interface ItemDetail {
  item: Item;
  files: MediaFile[];
  play: PlayState | null;
  children: ItemSummary[];
  available: boolean;
  /** For a show, the episode to play next. */
  next_episode: ItemSummary | null;
  activity: Activity;
  /** The public's scores, from switched-on services. */
  reception: SourcedScore[];
  /** Wins first, the most prominent first. */
  awards: AwardEntry[];
  /** TMDB's keywords, in English, most specific first. */
  keywords: string[];
}

/** A public score: `origin` is whose it is (an id in some service's `origins`), `service` the one that fetched it. */
export interface SourcedScore {
  service: string;
  origin: string;
  value: number;
  scale: number;
  votes: number | null;
  url: string | null;
  /** The origin's own seal on the score. */
  certified: boolean;
}

/** A win or nomination, named in the library's language. `body` is an id in its service's `bodies`. */
export interface AwardEntry {
  service: string;
  body: string;
  award: string;
  name: string;
  year: number | null;
  won: boolean;
  /** Lower is more prominent: the body's standing first. */
  rank: number;
}

/** A fact and the service that said it; `on` is the unix seconds of its day, UTC. `exhibit` is Exhibit itself. */
export type Sourced<T> = { service: string; on: number | null } & T;
export interface RatingFact {
  value: number;
  scale: number;
}
export interface ReviewFact {
  text: string;
  spoiler: boolean;
}
export interface WatchFact {
  rewatch: boolean;
}

/** Everything known about what the owner did with one work, from Exhibit and every switched-on service. Lists run newest first, undated last. */
export interface Activity {
  watched: boolean;
  loved: boolean;
  /** The owner's own choice in Exhibit, when made; it overrides the services. */
  watched_choice: boolean | null;
  loved_choice: boolean | null;
  ratings: Sourced<RatingFact>[];
  reviews: Sourced<ReviewFact>[];
  watches: Sourced<WatchFact>[];
  liked_by: string[];
  watchlisted_by: string[];
}

export interface HistoryEntry {
  item: ItemSummary;
  started_at: number;
  ended_at: number;
  position_s: number;
}

/**
 * What a listing is narrowed to; anything left out leaves it whole. Within a facet the choices are
 * alternatives, across facets they all apply; a work matches a technical one when any of its files
 * does. Genres are all required unless `any_genre`. Languages are named as `facets` names them.
 */
export interface Filters {
  genres?: string[];
  any_genre?: boolean;
  without_genres?: string[];
  /** TMDB keywords, in English, as `facets` names them; all required unless `any_keyword`. */
  keywords?: string[];
  any_keyword?: boolean;
  /** People by name in any role; all required. */
  people?: string[];
  year_min?: number | null;
  year_max?: number | null;
  /** Minutes. */
  runtime_min?: number | null;
  runtime_max?: number | null;
  seen?: Seen[];
  loved?: boolean | null;
  available?: boolean | null;
  resolution?: Resolution[];
  picture?: Picture[];
  sound?: Sound[];
  audio?: string[];
  subtitles?: string[];
  /** A switched-on service has the owner's rating, review or logged watch of the work. */
  rated?: boolean | null;
  reviewed?: boolean | null;
  logged?: boolean | null;
  /** On the owner's watchlist and not watched; `true` also lists the films the owner does not have yet. */
  watchlist?: boolean | null;
}

export interface SearchQuery extends Filters {
  text?: string | null;
  kind?: ItemKind | null;
  sort?: Sort | null;
  descending?: boolean | null;
  limit?: number | null;
  offset?: number | null;
  /** Also list the watchlist films the owner does not have yet, after the ones they have. */
  with_wanted?: boolean;
}

export interface Count<T> {
  value: T;
  count: number;
}

export interface ActivityCounts {
  rated: number;
  reviewed: number;
  logged: number;
  watchlist: number;
  not_in_library: number;
}

/** What a listing can be narrowed by, over all of it. Genres and languages most common first. */
export interface Facets {
  genres: Count<string>[];
  /** The keywords most works share, most common first. */
  keywords: Count<string>[];
  years: Count<number>[];
  runtimes: Count<number>[];
  resolution: Count<Resolution>[];
  picture: Count<Picture>[];
  sound: Count<Sound>[];
  audio: Count<string>[];
  subtitles: Count<string>[];
  activity: ActivityCounts;
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
  /** `null` when nothing was found. */
  source: ToolSource | null;
  /** The release (`YYYYMMDD`) of the downloaded copy in use. */
  release: string | null;
  /** A downloaded copy exists: it takes over when a path set by hand is cleared. */
  downloaded: boolean;
}

/** A step of downloading mpv and ffmpeg. */
export type DownloadStep =
  | { step: "checking" }
  | { step: "downloading"; done: number; total: number }
  | { step: "unpacking" };

export type DownloadOutcome =
  | { kind: "done"; release: string; already_latest: boolean }
  | { kind: "failed"; error: ApiError };

export interface DownloadStatus {
  running: boolean;
  /** The current step while running. */
  step: DownloadStep | null;
  /** How the most recent download ended, kept until the next one starts. */
  outcome: DownloadOutcome | null;
}

export interface LibraryStatus {
  movies: number;
  shows: number;
  other: number;
  needs_review: number;
  unprobed: number;
  sources: Source[];
  /** Exhibit can reach TMDB: with the owner's own key, or with Exhibit's. */
  tmdb_key: boolean;
  /** The owner set their own key; it is used instead of Exhibit's. */
  tmdb_own_key: boolean;
  /** This build carries a TMDB key of its own. */
  tmdb_builtin: boolean;
  agent_key: boolean;
  /** Whether the window plays the intro as it opens. */
  intro: boolean;
  /** The language in effect: the owner's choice, or the one Windows suggests. */
  language: string;
  /** How dates are written: `auto`, `day`, `month` or `numeric`. */
  date_format: string;
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
  /** A file with the asked-for extension (a tool's `.exe`), not a folder. */
  file: boolean;
}

export type Provides = "activity" | "reception" | "awards";
export type Need = "username" | "key" | "export_file" | "sign_in";
export type ScoreStyle = "decimal" | "percent" | "badge";

/** Whose scores a service reports, with the mark the window draws for them. */
export interface OriginInfo {
  id: string;
  name: string;
  mark_svg: string;
  style: ScoreStyle;
}

/** An award body; `name` is English, the catalogue words it in the owner's language. */
export interface BodyInfo {
  id: string;
  name: string;
}

/** What a service holds after a sync, counted in films. */
export interface SyncReport {
  logged: number;
  rated: number;
  reviewed: number;
  liked: number;
  watchlisted: number;
  /** Watchlisted films with no file. */
  not_in_library: number;
  unmatched: number;
}

export interface ServiceState {
  service: string;
  enabled: boolean;
  username: string | null;
  export_name: string | null;
  /** Unix seconds. */
  export_at: number | null;
  synced_at: number | null;
  /** The code of the last sync's failure (`service.unreachable`), for the window to word. */
  error: string | null;
  /** Films the matcher could not place surely; they are left out. */
  unmatched: number;
  /** Lookups left today, as the service last said. */
  remaining: number | null;
  /** The allowance is spent until then (unix seconds). */
  paused_until: number | null;
}

/** What a service is and how the owner stands with it. The adapter supplies the name, the mark and the page. */
export interface ServiceView {
  id: string;
  name: string;
  /** The mark as an SVG document. */
  markSvg: string;
  provides: Provides[];
  needs: Need[];
  exportPage: string | null;
  origins: OriginInfo[];
  bodies: BodyInfo[];
  state: ServiceState;
}

/** A sign-in under way: the owner enters `userCode` at `verifyUrl`, or scans `qrSvg` with a phone. */
export interface SignInView {
  userCode: string;
  verifyUrl: string;
  qrSvg: string;
  /** Seconds between asks. */
  interval: number;
  /** Seconds the code stays good. */
  expiresIn: number;
}

export type SignInState = "pending" | "slow_down" | "done";

/** An export file found in Downloads. */
export interface FoundExport {
  path: string;
  name: string;
  /** Unix seconds. */
  modified: number;
}

export interface JobStatus {
  /** `null` when the worker is idle. */
  phase: RefreshPhase | null;
  /** Files or items handled so far in the current phase. */
  done: number;
  /** The most recent job's failure, kept until the next job ends. */
  last_error: ApiError | null;
  /** What the most recent export import brought in; kept until the next one. */
  last_import: SyncReport | null;
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

/**
 * What a failed command rejects with. `code` names what happened (`source.offline`) and `params`
 * fill in its wording (`path`); the window words it from its catalogue (`errors.<code>`), else
 * from the kind. `message` is the core's English, for logs only.
 */
export interface ApiError {
  kind: ErrorKind;
  code: string | null;
  params: Record<string, string>;
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

/** Whether an error (or a job's `last_error`) is TMDB refusing the key. */
export const keyRejected = (e: unknown): boolean => isApiError(e) && e.code === "tmdb.key_rejected";

/**
 * An error as the owner reads it, in the window's language: what happened and what to do, worded
 * from its code ("The drive for D:\Films is not connected."), else from its kind. The core's
 * English never shows. Anything else a promise rejected with is "Something went wrong."
 */
export function plain(e: unknown): string {
  if (!isApiError(e)) return t("errors.unknown");
  return (
    (e.code === null ? null : translated(`errors.${e.code}`, e.params ?? {})) ??
    translated(`errors.kind.${e.kind}`, {}) ??
    t("errors.unknown")
  );
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
export const facets = (kind: ItemKind | null) => invoke<Facets>("facets", { kind });
export const similar = (id: Id, limit: number) => invoke<Similar[]>("similar", { id, limit });
export const item = (id: Id) => invoke<ItemDetail>("item", { id });
/** A work's cast with portraits; the first ask fetches and caches them. */
export const cast = (id: Id) => invoke<CastMember[]>("cast", { id });
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
/** Drive roots for `null`; otherwise visible sub-folders, sorted, then with `ext` (`"exe"`) the files ending in it. */
export const listFolders = (path: string | null, ext: string | null = null) =>
  invoke<Folder[]>("list_folders", { path, ext });
export const detectTools = () => invoke<ToolStatus[]>("detect_tools");
export const setToolPath = (tool: Tool, path: string | null) =>
  invoke<void>("set_tool_path", { tool, path });
/** Starts downloading mpv and ffmpeg; false when a download is already running. Progress arrives as `tools-download`. */
export const downloadTools = () => invoke<boolean>("download_tools");
export const toolsDownloadStatus = () => invoke<DownloadStatus>("tools_download_status");
/** `null` clears it. The value is never sent back. */
export const setSecret = (name: SecretName, value: string | null) =>
  invoke<void>("set_secret", { name, value });
/** Queues a refresh of one source or all; returns at once. */
export const refresh = (source: Id | null) => invoke<void>("refresh", { source });
/** Queues an identification; returns at once. */
export const identify = (scope: IdentifyScope) => invoke<void>("identify", { scope });
export const jobStatus = () => invoke<JobStatus>("job_status");
/** Every service the app knows, and how the owner stands with it. */
export const services = () => invoke<ServiceView[]>("services");
export const setServiceEnabled = (id: string, on: boolean) => invoke<void>("set_service_enabled", { id, on });
/** Asks the site whether the name is good, saves it and queues the first sync; returns how many recent diary entries it shows. */
export const connectService = (id: string, username: string) => invoke<number>("connect_service", { id, username });
export const forgetService = (id: string) => invoke<void>("forget_service", { id });
/** Queues the import of an export file; the result arrives in `job-status` as `last_import`. */
export const importServiceExport = (id: string, path: string) => invoke<void>("import_service_export", { id, path });
export const openExportPage = (id: string) => invoke<void>("open_export_page", { id });
export const findServiceExport = (id: string) => invoke<FoundExport | null>("find_service_export", { id });
/** Films the service named that could not be placed surely: title and year. */
export const serviceUnmatched = (id: string) => invoke<Array<[string, number | null]>>("service_unmatched", { id });
/** Starts signing in to a service; the window shows the code until the owner approves. */
export const startSignIn = (id: string) => invoke<SignInView>("start_sign_in", { id });
export const pollSignIn = (id: string) => invoke<SignInState>("poll_sign_in", { id });
/** Resolves once the player has started; the end arrives as `play-ended`. */
export const play = (id: Id, resume: boolean) => invoke<void>("play", { id, resume });
/** Takes effect from the next launch. */
export const setIntro = (on: boolean) => invoke<void>("set_intro", { on });
export const setLanguage = (language: string) => invoke<void>("set_language", { language });
export const setDateFormat = (format: string) => invoke<void>("set_date_format", { format });
export const windowFullscreen = (on: boolean) => invoke<void>("window_fullscreen", { on });
export const windowIsFullscreen = () => invoke<boolean>("window_is_fullscreen");

// Events. Each returns a promise of its unlisten function.

export const onLibraryChanged = (cb: () => void): Promise<UnlistenFn> =>
  listen("library-changed", () => cb());
export const onJobStatus = (cb: (s: JobStatus) => void): Promise<UnlistenFn> =>
  listen<JobStatus>("job-status", (e) => cb(e.payload));
export const onToolsDownload = (cb: (s: DownloadStatus) => void): Promise<UnlistenFn> =>
  listen<DownloadStatus>("tools-download", (e) => cb(e.payload));
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
