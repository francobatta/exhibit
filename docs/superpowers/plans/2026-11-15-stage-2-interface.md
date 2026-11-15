# Exhibit Stage 2: Interface Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Tauri 2 desktop window over `exhibit-core`: a TV-first gallery (Home, Movies, Shows, Other, Show, Item details, Needs review, Ask as plain search, Settings, First run) driven identically by gamepad, keyboard and mouse, that launches mpv and returns to where the owner was.

**Architecture:** A third workspace crate, `crates/exhibit-app`, is a thin Tauri shell: one command per `Library` operation, one background worker that owns its own `Library` connection for long jobs (scan, identify, probe, thumbnails), one thread per playback, and events that tell the window what changed. The React UI in `ui/` holds no library logic: it reads through TanStack Query, invalidates on a `library-changed` event, and renders through one focus system. No model calls, no MCP, no agent in this stage.

**Tech Stack:** Rust (Tauri 2, `notify`, existing `exhibit-core`); Bun as package manager, script runner and test runner (Node.js is not installed: never call `node`, `npm` or `npx`; use `bun`, `bunx --bun`); React 19, TypeScript (strict), Vite, Tailwind CSS 4, Vercel UI SDK (`@ui-sdk/react` + `@ui-sdk/core`, beta: the behaviour layer for components, focus, grids, virtualisation, layers and shortcuts), shadcn (project conventions: `components.json`, `cn`, CSS-variable tokens, owned components in `components/ui`), Motion (`motion` package), TanStack Query, Zustand; `bun test` with `happy-dom` and `@testing-library/react`. Fonts bundled through `@fontsource-variable/*` (no network fonts).

**Spec:** `docs/superpowers/specs/2026-11-14-exhibit-design.md` (sections 4, 8, 10, 11, 14, 16, 17). Stage 1 plan: `docs/superpowers/plans/2026-11-14-stage-1-core.md`.

**Not in this stage (ruled):** the MCP server; the built-in agent, agent shelves, taste profile UI, voice; passing a CLI `play` to the open window (needs cross-process messaging, built with stage 3); an installer.

## Global Constraints

- Windows 11 first; nothing that blocks a later Linux port without `#[cfg(windows)]` and a stub.
- Exhibit never renames, moves, deletes or writes inside a media folder.
- The UI never touches the database, the file system or the network itself. Everything goes through Tauri commands that call `exhibit_core::Library`. No behaviour exists in the UI that a `Library` method lacks.
- Exhibit sends mpv no playback commands and has no playback controls.
- Secrets never reach the UI, logs, events or error text. The UI can set, clear and ask "is it set", nothing else.
- The gallery opens and browses with no network, no TMDB key and no media tools.
- One focus system drives everything. Every action is reachable with a direction pad and four buttons (Xbox: A select, B back, X item menu, Y ask; LB/RB previous/next top-level section; Menu button opens Settings). Keyboard: arrows, Enter, Escape or Backspace (back), `m` (item menu), `/` or Ctrl+K (ask), `[` and `]` (sections). Mouse: moving the pointer over a focusable focuses it, click selects, right-click opens the item menu, wheel scrolls. No screen may handle an input device on its own.
- Layout is sized relative to the window: `html { font-size: calc(100vw / 120) }` (so 1rem is 16px at 1920 CSS px wide) and every dimension in the UI is in `rem`; no `px` except hairlines.
- Wall columns by window width in CSS px: up to 1920 gives 7, up to 2560 gives 9, wider gives 11.
- Every screen has designed empty, loading, offline-drive and error states. Errors say what happened and what to do, in plain words, sentence case, never apologising.
- Motion answers the owner's actions (moving focus, opening, returning) and is otherwise still. `prefers-reduced-motion` turns movement into short fades.
- A library of 5,000 items must scroll and move focus without dropped frames: grids and shelves are virtualised; artwork `<img>` elements carry `decoding="async"` and explicit dimensions.
- Copy: sentence case; no all-caps labels; no middle dots or em dashes as separators (use commas); buttons say what they do ("Resume from 1:04:12", "Add folder").
- Use the owner's chosen stack, not hand-rolled substitutes.
  - **Vercel UI SDK** (https://ui-sdk.dev, `@ui-sdk/react` and `@ui-sdk/core`) is the behaviour layer. It is new and in beta: do not write its API from memory. Before using it read `ui/node_modules/@ui-sdk/react/docs/llms.txt`, the component's page under `docs/components/`, and search `docs/core.md` by heading (plus `docs/rendering.md` when defining a primitive); follow the `ui-sdk` skill when it is installed. Use its components for every base primitive it has (button, toggle, input, dialog, alert-dialog, menu, select, tabs, segmented-control, command, scroll-area, toast, tooltip, separator, carousel where it fits), and its behaviours for everything interactive we build ourselves: `createRovingFocus`, `createGridFocus` (with `reveal`), `spatialMove` / `nearestInDirection`, `createFocusRecovery`, `keepFocus`, `createVirtualizer`, its layers for modal surfaces, its shortcuts hook, its pointer-kind helper. Hand-write a behaviour only when the docs show UI SDK has none, and say so in the report. If a beta API is broken or missing, report it with a minimal reproduction instead of silently working around it.
  - **shadcn** gives the project its conventions: `components.json`, the `cn` helper, CSS-variable tokens, and owned component files in `ui/src/components/ui/` (one file per primitive, styled with Tailwind through our tokens, each wrapping the UI SDK parts). Add a shadcn registry component (`bunx --bun shadcn@latest add <name>`) only for a primitive UI SDK lacks.
  - **Motion** (`motion/react`) drives every animation we author: focus light (`layoutId`), screen transitions (`AnimatePresence`), shared-element opening, backdrop crossfades. UI SDK's `data-starting-style` / `data-ending-style` attributes are used only for its own popups' enter and exit.
  - **Vercel AI SDK** is the stack for the built-in agent in stage 4; it is not installed in this stage, but `Ask` must be built so that stage 4 only swaps its result source: results arrive as `AskResult = { groups: Array<{ title: string; reason?: string; items: ItemSummary[] }> }` from a `useAsk(text)` hook, which in this stage wraps `search`.
- TypeScript strict, no `any`. No new runtime dependency beyond the Tech Stack without a line in the task report saying why.
- Database code stays plain `rusqlite` and hand-written SQL (owner rule).
- All timestamps Unix seconds, durations seconds. Watched threshold 0.90 (core constant; never restated in the UI).
- Tests never touch the network, the real credential store, real media tools or the owner's real data directory (`EXHIBIT_DATA_DIR` to a temp dir; `EXHIBIT_KEYRING_SERVICE` to a throwaway name).
- Every commit message ends with the Co-Authored-By line the implementer's harness specifies.

## Design direction (binding for every UI task)

A gallery at night. The owner approved "judge it in the running app": these tokens are the proposal and live only in `ui/src/styles/tokens.css`, so a change after their feedback is one file.

- **Walls.** No accent colour exists. Artwork is the only strong colour. Base wall `--wall: #0f1113`, raised surface `--wall-raised: #171a1d`, hairline `--line: rgb(236 232 223 / 0.10)`. The wall takes a tint from the focused title: `--tint` is set at runtime to the artwork's dominant hue at low lightness and chroma (`oklch(0.20 0.035 <hue>)`), and backgrounds blend toward it.
- **Light.** Focus is shown as light falling on the work, in lamp white `--lamp: #ffe9c4` (a 2700 K gallery spot), never as a coloured ring. Unfocused works sit at `brightness(0.62) saturate(0.85)`; the focused work is at full brightness, scaled 1.06, with a soft pool of light above and behind it (a radial gradient in `--lamp` at about 14% opacity mixed with the tint) and a 1px lamp-white keyline. Progress bars and the text caret are lamp white.
- **Text.** `--label: #ece8df` (the paper of a wall label), `--label-dim: #a09b90`, `--label-faint: #6b675f`.
- **Type.** Display and interface: Archivo Variable, using its width axis: titles are set between width 62 and 125 so a title fills the placard measure instead of wrapping or shrinking (component `FitTitle`); weights 600 to 750, tight tracking at large sizes. Reading text (overview lines, messages): Source Serif 4 Variable at a generous line height (1.5). Scale (rem): 0.875, 1, 1.25, 1.6, 2.4, 4, 6.5.
- **Placard.** A wall label, left aligned: title; then "year, director" on one line; then "runtime, resolution and HDR, watched state" on the next; then one line of description (two at most, clamped). No pills, no chips: plain text in `--label-dim`, commas between facts.
- **Works.** Posters are hung, not carded: 2:3, corner radius 0.125rem, no border, no drop shadow. Other items and episodes are 16:9. A work with no artwork is a typographic poster: wall-raised ground, its title in Archivo at width 75, weight 650, set bottom left.
- **Offline works** are desaturated to grey and marked "Offline" in the placard; the light still falls on them when focused.
- **Motion.** Focus light glides between works with a spring (stiffness 520, damping 40); the backdrop and tint crossfade over 450 ms, starting 140 ms after focus settles; opening a work grows its poster into the details screen (shared layout) in 320 ms and Back reverses it. Nothing loops, nothing animates on its own.

## Review Focus

1. **A drive is unplugged while the window is open.** Its items dim within seconds, Play is disabled with a message naming the drive, focus stays put, nothing is deleted. Pinned in Tasks 2 and 8.
2. **No TMDB key and no tools.** Every screen still works: cleaned-up names, typographic posters, no technical details, nothing in needs review, and Settings says what each missing piece would add. Pinned in Tasks 5 and 11.
3. **Awkward metadata.** A 90-character title, a title in CJK, no year, no overview, no director, zero runtime: placard and typographic poster never overflow, clip mid-glyph or show "undefined", "null", "NaN" or a dangling comma. Pinned in Task 5.
4. **The focused thing disappears.** A rescan removes or re-kinds the focused item, or a list shrinks under the focus: focus moves to the nearest remaining target in the same container, or to the container's fallback; it is never lost. Pinned in Task 4.
5. **Two inputs at once.** The pointer rests over a grid while the owner navigates by gamepad: content scrolling under a stationary pointer must not steal focus; only real pointer movement does. A gamepad unplugged and replugged keeps working. Pinned in Task 4.

## File Structure

```
Cargo.toml                              add crates/exhibit-app to members
crates/exhibit-app/
  Cargo.toml, build.rs, tauri.conf.json, capabilities/default.json, icons/
  src/main.rs                           window, plugins, state, command registration
  src/state.rs                          AppState: Mutex<Library> for quick calls, handles to worker and player
  src/commands.rs                       one #[tauri::command(async)] per Library operation
  src/error.rs                          exhibit_core::Error -> { kind, message } for the UI
  src/worker.rs                         refresh pipeline on its own Library; status and change events
  src/watch.rs                          source folder watching, drive arrival/removal, other-process changes
  src/playback.rs                       play on a thread, play-ended event, window refocus
  src/folders.rs                        list_folders for the in-app folder browser
scripts/shot.ps1                        capture the Exhibit window to a PNG, optionally sending keys first
ui/
  package.json, bunfig.toml, vite.config.ts, tsconfig.json, index.html, components.json
  src/main.tsx, src/App.tsx
  src/styles/tokens.css, src/styles/base.css
  src/lib/api.ts                        typed invoke wrappers and all shared types (mirrors serde output)
  src/lib/queries.ts                    TanStack Query hooks, invalidation on library-changed
  src/lib/format.ts                     runtime, position, resolution, list joining
  src/lib/tint.ts                       dominant hue of an image, cached
  src/nav/stack.ts, src/nav/Screens.tsx navigation stack with per-entry focus memory
  src/focus/bridge.ts                   moves between focus groups (UI SDK nearestInDirection), scopes, restore
  src/focus/react.tsx                   FocusScope, FocusGroup, useFocusItem, useInputMode
  src/focus/input/gamepad.ts, pointer.ts, keys.ts   devices translated into the one keyboard/focus path
  AGENTS.md (in ui/)                    points agents at the UI SDK docs shipped in node_modules
  src/components/                       Work, FitTitle, Placard, Shelf, Wall grid, Backdrop, TopBar, Hints,
                                        ItemMenu, Dialog, TextField, Keyboard (on-screen), FolderBrowser, states
  src/components/ui/                    owned primitives, shadcn conventions over UI SDK parts
  src/screens/Home.tsx, Wall.tsx, Show.tsx, Details.tsx, Review.tsx, Ask.tsx, Settings.tsx, FirstRun.tsx
  test/setup.ts                         happy-dom registration, Tauri invoke/event fakes
```

Tests sit beside their subject as `*.test.ts(x)`.

---

### Task 1: Core additions the window needs

**Files:**
- Modify: `crates/exhibit-core/src/{library.rs,volume.rs,secrets.rs,query.rs,scan.rs,identify.rs,probe.rs,tools.rs,player.rs,tmdb.rs,db.rs}`, `crates/exhibit-cli/src/{main.rs,commands.rs}`, `crates/exhibit-cli/tests/e2e.rs`

**Interfaces:**
- Consumes: stage 1 `Library`.
- Produces:
  - `Library: Send` (`Box<dyn Volumes + Send>`, `Box<dyn Secrets + Send>`); a compile-time test `fn assert_send<T: Send>() {} assert_send::<Library>()`.
  - `serde::{Serialize, Deserialize}` on `SearchQuery` (`#[serde(default)]`, all fields optional), `Sort` and `Shelf` (`rename_all = "snake_case"`: `title|added|year|runtime`, `continue_watching|recently_added|unwatched|loved`), `IdentifyScope` (`unmatched|review|unmatched_and_review|{"item": id}`), `ScanEvent`, `Tool` (existing lowercase names), and `Serialize` on `LibraryStatus`, `ToolStatus` (already).
  - `Library::lookup(&self, text: &str, kind: ItemKind, year: Option<i32>) -> Result<Vec<Candidate>>`: a TMDB search for "search by hand"; `kind` must be `Movie` or `Show` (else `Error::Invalid`); `score` is the title similarity the matcher already computes. CLI: `exhibit review search <text> --kind movie|show [--year n]`.
  - `Library::thumbnails_pending(&self, on: &mut dyn FnMut(Id)) -> Result<(u32, u32)>`: for every `other` item with no `backdrop`, an online first file and a probed duration, render one frame at 10% of the duration into `<cache>/thumb-<item_id>.jpg` with the existing `probe::thumbnail`, and store that file name in `items.backdrop`. Returns (made, failed). A failure leaves `backdrop` NULL and is retried next run only if the file's mtime changed (record nothing new: skip retry within the same process run is enough). `Error::Tool` when neither ffmpeg nor mpv exists. CLI: `exhibit scan` runs it last; JSON gains `"thumbnails": {"made","failed"} | null`.
  - `Library::refresh(&self, source: Option<Id>, on: &mut dyn FnMut(RefreshEvent)) -> Result<RefreshReport>`: the one pipeline the CLI `scan` and the window share: scan, identify (scope `Unmatched`; skipped with no TMDB key), probe (skipped with no tool), identify again with scope `Review`, thumbnails (skipped with no tool). `pub enum RefreshEvent { Phase(RefreshPhase), Scan(ScanEvent), Item(Id), File(Id) }`, `pub enum RefreshPhase { Scanning, Identifying, Probing, Rescoring, Thumbnails }` (serde snake_case), `pub struct RefreshReport { scanned: Vec<ScanReport>, identified: Option<IdentifyReport>, probed: Option<(u32,u32)>, rescored: Option<IdentifyReport>, thumbnails: Option<(u32,u32)> }`. `exhibit scan` becomes a call to it (flags `--no-probe`, `--no-identify` map to `RefreshOptions { identify: bool, probe: bool }` passed as a third parameter); its JSON keys stay as they are today.
  - `Library::data_version(&self) -> Result<i64>`: `PRAGMA data_version` (changes when another connection commits).
  - `Library::image_dir(&self) -> PathBuf` (public form of `cache_dir`).
  - Poster images are also stored small: the artwork download saves `<kind>-<id>-poster.jpg` at TMDB size `w780` (as today) and `<kind>-<id>-poster-s.jpg` at `w342`. The item still stores the large name; the small one is derived by the `-s` suffix. Items identified before this change get the small file on the next identify of any scope (a cheap pass that downloads only missing small files).
  - Every tool process the core starts (`tools.rs` run helper, `probe.rs`, `player.rs`) is created with `CREATE_NO_WINDOW` on Windows through one helper `tools::quiet(&mut Command)`, so a windowed app never flashes a console. mpv for playback is exempt (it must show its window).
  - `ItemDetail` gains `next_episode: Option<ItemSummary>`: for a show, the episode `next_episode` picks (None for other kinds or when every episode is watched).
  - `set_watched` on a show or season applies to every episode below it (verify; add if missing) and a test.

- [ ] **Step 1: Write failing tests**: `library_is_send`; `search_query_deserialises_from_partial_json` (`{"kind":"movie","sort":"year"}`); `lookup_returns_scored_candidates` and `lookup_rejects_other_kind` (fake provider); `thumbnails_fill_backdrop_for_other_items`, `thumbnails_skip_offline_and_unprobed` (fake tool batch file as in the probe tests); `refresh_runs_phases_in_order` (records `RefreshEvent::Phase` sequence: scanning, identifying, probing, rescoring, thumbnails) and `refresh_skips_identify_without_key`; `data_version_changes_after_other_connection_writes`; `small_poster_saved_beside_large`; `set_watched_on_show_marks_every_episode`; e2e: `review search` without a key exits 1 with `TMDB key`.
- [ ] **Step 2: Run** `cargo test --workspace`. Expected: the new tests fail to compile or fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo fmt --all`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`. Expected: clean.
- [ ] **Step 5: Commit** `feat(core): refresh pipeline, lookup, thumbnails and serde for the window`.

---

### Task 2: Tauri shell, commands, worker, watching, playback

**Files:**
- Create: everything under `crates/exhibit-app/`, `scripts/shot.ps1`, a placeholder `ui/` (Vite React TS scaffold whose only screen prints the JSON of `status`), `ui/src/lib/api.ts`
- Modify: root `Cargo.toml`, `.gitignore` (`node_modules`, `ui/dist`, `crates/exhibit-app/gen`)

**Interfaces:**
- Consumes: Task 1.
- Produces (Rust):
  - `AppState { lib: Mutex<Library>, worker: WorkerHandle, playing: AtomicBool, image_dir: PathBuf }`. Quick commands lock `lib`. Long work never holds it: the worker and each playback open their own `Library::open_default()` (SQLite WAL allows it).
  - Errors cross as `{ "kind": "not_found|invalid|offline|tool|provider|io|db", "message": String }` (same kinds the CLI prints).
  - Commands (all `#[tauri::command(async)]`, snake_case, arguments camelCase on the JS side as Tauri maps them): `boot() -> { status: LibraryStatus, imageDir: String, firstRun: bool }` (`firstRun` = no sources); `status`, `search(query)`, `item(id)`, `shelf(shelf, limit)`, `needs_review`, `history(limit)`, `set_watched(id, watched)`, `set_loved(id, loved)`, `apply_match(itemId, tmdbId, kind)`, `move_to_other(id)`, `lookup(text, kind, year)`, `add_source(path)`, `remove_source(id)`, `list_folders(path: Option<String>) -> Vec<{ name, path }>` (drive roots when `None`; sub-folders only, hidden and system folders skipped, sorted case-insensitively; unreadable gives `io`), `detect_tools`, `set_tool_path(tool, path: Option<String>)`, `set_secret(name, value: Option<String>)`, `refresh(source: Option<Id>)` (enqueue; returns at once), `identify(scope)` (enqueue), `job_status() -> JobStatus`, `play(id, resume) -> ()` (returns once mpv has been started or with the error), `window_fullscreen(on: bool)`.
  - Events: `library-changed` (no payload; coalesced to at most one per 400 ms); `job-status` with `JobStatus { phase: Option<RefreshPhase>, done: u32, message: Option<String>, last_error: Option<String> }`; `play-ended` with `{ outcome: PlayOutcome | null, error: {kind,message} | null, nextEpisode: Id | null }` (`nextEpisode` = `next_episode` of the show when the played item is an episode that ended watched).
  - `worker.rs`: one thread, an `mpsc` queue of `Job::Refresh(Option<Id>) | Job::Identify(IdentifyScope)`; identical queued jobs collapse; a `Refresh(None)` is queued at launch. Pure, testable core: `fn run_job(lib: &Library, job: Job, emit: &mut dyn FnMut(Emit))` with `enum Emit { Status(JobStatus), Changed }`.
  - `watch.rs`: every 3 s, compare `list_sources()` online flags and `data_version()` with the previous tick: any difference emits `library-changed`; a source that came online enqueues `Refresh(Some(id))`. A `notify` recursive watcher on each online source root enqueues `Refresh(Some(id))` 5 s after the last file event.
  - `playback.rs`: refuses a second play while one runs (`invalid`, "Something is already playing"); on exit emits `play-ended`, emits `library-changed`, and brings the window back to the front with keyboard focus.
  - Window: title `Exhibit`, opens fullscreen, restores the last fullscreen/size choice (`tauri-plugin-window-state`), dark background `#0f1113` before the UI paints (no white flash), single instance (`tauri-plugin-single-instance`: a second launch focuses the first). Env `EXHIBIT_WINDOWED=1` forces a 1600x900 window (development and screenshots). The asset protocol is enabled and scoped at runtime to `image_dir` only. No other plugin or capability (no shell, fs, http, dialog).
- Produces (`ui/src/lib/api.ts`): every type above and from core as TypeScript (`ItemSummary`, `ItemDetail`, `Item`, `MediaFile`, `TechInfo`, `PlayState`, `Candidate`, `Source`, `ToolStatus`, `LibraryStatus`, `SearchQuery`, `Shelf`, `Sort`, `ItemKind`, `MatchState`, `IdentifyScope`, `JobStatus`, `PlayEnded`, `ApiError`), one exported async function per command with the same name in camelCase (`setWatched(id, watched)`), `onLibraryChanged(cb)`, `onJobStatus(cb)`, `onPlayEnded(cb)` returning unlisten functions, and `imageUrl(name: string | null, size?: "s"): string | null` built on `convertFileSrc`.
- Produces (`scripts/shot.ps1`): `scripts/shot.ps1 -Out <png> [-Keys "<SendKeys string>"] [-WaitMs 800]`: finds the window titled `Exhibit`, brings it forward, optionally sends keys, waits, and saves a PNG of the window's client area. Exits 1 with a message when the window is not found.

- [ ] **Step 1: Scaffold** the crate and placeholder UI with `bun create tauri-app`-equivalent files by hand (Tauri 2, `frontendDist: ../../ui/dist`, `devUrl: http://localhost:1420`, `beforeDevCommand`/`beforeBuildCommand` run `bun run dev|build` in `ui/`). Add `@tauri-apps/cli` and `@tauri-apps/api` to `ui/` with `bun add`. `bun run tauri dev` (script in `ui/package.json` pointing at `../crates/exhibit-app`) must open a window.
- [ ] **Step 2: Write failing Rust tests** (temp data dir, fake volumes/secrets as core tests do): `run_job_refresh_emits_phases_then_changed`; `identical_jobs_collapse`; `error_kinds_map` (each core `Error` variant to its kind); `list_folders_lists_only_visible_subfolders`; `list_folders_none_lists_roots`; `play_ended_names_next_episode` (pure function `next_after(lib, outcome)`); `watch_tick_reports_source_going_offline` (pure function `diff(prev, now) -> Vec<WatchChange>`).
- [ ] **Step 3: Implement** until `cargo test -p exhibit-app` passes.
- [ ] **Step 4: Manual check, recorded in the report:** `EXHIBIT_WINDOWED=1 bun run tauri dev` with a temp `EXHIBIT_DATA_DIR`; the placeholder screen shows the status JSON; `scripts/shot.ps1 -Out <scratch>/shell.png` produces a readable PNG (open it and look).
- [ ] **Step 5: Run** fmt, `cargo test --workspace`, clippy `-D warnings`, `bun run build` in `ui/`. Commit `feat(app): Tauri shell over the library`.

---

### Task 3: UI foundation: tokens, type, data hooks, navigation stack, shell

**Files:**
- Create: `ui/src/styles/{tokens.css,base.css}`, `ui/src/lib/{queries.ts,format.ts,tint.ts}`, `ui/src/nav/{stack.ts,Screens.tsx}`, `ui/src/components/{TopBar.tsx,Backdrop.tsx,states.tsx}`, `ui/src/App.tsx`, `ui/test/setup.ts`, `ui/bunfig.toml`, `ui/components.json`, tests beside each `lib` and `nav` file
- Modify: `ui/package.json`, `ui/src/main.tsx`, `ui/vite.config.ts`

**Interfaces:**
- Consumes: `api.ts` (Task 2). Design direction section above, verbatim values.
- Produces:
  - `tokens.css`: every colour, type, spacing, radius and motion value as CSS custom properties, mapped into Tailwind 4 `@theme`. No component may hard-code a colour or duration.
  - Fonts: `@fontsource-variable/archivo` (with the width axis) and `@fontsource-variable/source-serif-4`, imported in `base.css`; `font-display: block` for Archivo.
  - `format.ts`: `runtime(min: number | null): string | null` ("2 h 42 min", "58 min", null for null or 0); `clock(seconds: number): string` ("1:04:12", "4:05"); `resolution(height: number | null, hdr: string | null): string | null` (2160 "4K", 1080 "1080p", 720 "720p", below "SD"; with HDR "4K Dolby Vision", "4K HDR10"); `facts(parts: Array<string | null | undefined>): string` (joins present parts with ", "; never emits "null", "undefined", "NaN" or a dangling comma); `episodeLabel(s: number | null, e: number | null): string` ("S2 E4", "E4", "").
  - `queries.ts`: `useBoot()`, `useStatus()`, `useShelf(shelf, limit)`, `useItems(query)` (infinite, pages of 500, flattened), `useItem(id)`, `useNeedsReview()`, `useJobStatus()` (event-fed), mutations `useSetWatched()`, `useSetLoved()`, `useApplyMatch()`, `useMoveToOther()`, `usePlay()`; one listener invalidates every library query on `library-changed`; mutations update the cache optimistically and roll back on error.
  - `tint.ts`: `useTint(imageName: string | null): string | null` returns an `oklch(0.20 0.035 H)` string from the image's dominant hue (draw the small poster to a 16x16 canvas, average the most saturated third of pixels), cached by name; null when unknown.
  - `nav/stack.ts` (Zustand): `type Route = {screen:"home"} | {screen:"wall", kind:"movie"|"show"|"other"} | {screen:"show", id} | {screen:"details", id} | {screen:"review"} | {screen:"settings"} | {screen:"firstRun"}`; `push(route)`, `back(): boolean` (false at the root), `replaceRoot(route)` (top-level sections replace, they do not stack), `current`, and per-entry `focusKey: string | null` saved on leave and restored on return (`setFocusKey`). Ask is an overlay, not a route.
  - `Screens.tsx`: renders the current entry with a Motion presence transition (push: new screen fades up 200 ms; back: reverse), keeps the previous entry mounted but inert so Back restores scroll and focus exactly.
  - Tauri's content security policy must allow UI SDK's injected styles: follow its "Content Security Policy" doc section (constructed style sheets; never `unsafe-inline` for scripts).
  - `TopBar`: the mark "Exhibit", sections Home, Movies, Shows, Other, "Needs review" with its count only when above zero, the ask field ("Ask for something" when an agent key is set, else "Search the library"), a settings button, and a quiet one-line job status ("Scanning, 214 files", "Looking up titles", last error) from `useJobStatus`.
  - `Backdrop`: full-window layer showing the focused work's backdrop image under gradients to `--wall`, crossfading per the Motion rule; sets `--tint` on `:root`.
  - `states.tsx`: `Empty`, `Loading` (still, no spinner loop: a single dim line of text), `ErrorState` (message, what to do, one action), `OfflineNote`.
- This task uses plain UI SDK buttons with native focus; Task 4 adds the focus bridge and devices. Screens other than a stub Home may render `Empty`.

- [ ] **Step 1: Set up** Tailwind 4; UI SDK per its own Quick Start (`bun add @ui-sdk/react @ui-sdk/core`; `@layer ui-sdk;` declared above `@import "tailwindcss"` in `base.css`; do NOT import its default `web.css`; create `ui/AGENTS.md` with the note pointing at `node_modules/@ui-sdk/react/docs/llms.txt`, `core.md` and `rendering.md`; install its agent skill at project scope with `bunx --bun skills add vercel-labs/ui --skill ui-sdk` and commit what it adds; if that command cannot run under Bun, skip it and say so); shadcn (`bunx --bun shadcn@latest init`, CSS variables, then point its variables at our tokens); the first owned primitives in `components/ui/` on UI SDK parts: `button.tsx`, `toggle.tsx`, `input.tsx`, `dialog.tsx`, `toast.tsx`; Motion, TanStack Query, Zustand, fonts; `bun test` with happy-dom preload and fakes for `@tauri-apps/api/core` `invoke` and `@tauri-apps/api/event` `listen` in `test/setup.ts`.
- [ ] **Step 2: Write failing tests**: `format.test.ts` with the exact strings above plus `facts(["1979", null, undefined, ""]) === "1979"` and `runtime(0) === null`; `stack.test.ts`: push/back restore `focusKey`; `replaceRoot` clears the stack; `back()` at root returns false; `queries.test.tsx`: a `library-changed` event refetches a mounted shelf; a failed `setLoved` rolls the cache back.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `bun test` and `bunx --bun tsc --noEmit` in `ui/`. Expected: pass. Look at it: run the app, capture with `scripts/shot.ps1`, check type renders in Archivo and Source Serif (not fallbacks) and the wall colour matches.
- [ ] **Step 5: Commit** `feat(ui): tokens, data hooks, navigation stack and shell`.

---

### Task 4: Focus system and input

One rule makes the system single: **real DOM focus is the focus.** Whatever has `document.activeElement` is the lit work. Keyboard events are the one path; the gamepad is translated into the same key events, and the pointer into `focus()` calls. UI SDK behaviours own movement inside a group; a thin bridge owns movement between groups, modal scopes and restoring focus.

**Files:**
- Create: `ui/src/focus/{bridge.ts,react.tsx}`, `ui/src/focus/input/{gamepad.ts,pointer.ts,keys.ts}`, `ui/src/components/Hints.tsx`, tests beside each
- Modify: `ui/src/App.tsx`, `ui/src/components/TopBar.tsx`, `ui/src/nav/Screens.tsx`

**Interfaces:**
- Consumes: `nav/stack.ts` `back()`, `setFocusKey`; UI SDK `createRovingFocus`, `createGridFocus`, `spatialMove`, `nearestInDirection`, `createFocusRecovery`, `keepFocus`, layers, shortcuts, pointer kind (read the "Focus and Keys", "Popups and Layers", "Shortcuts" and "Pointer Kind" sections of `docs/core.md` first).
- Produces:
  - `react.tsx`:
    - `<FocusGroup id orientation="horizontal"|"vertical"|"spatial" remember? onEdge?>`: a group of items with one tab stop, built on `createRovingFocus` (`spatial` uses `spatialMove()`), with `createFocusRecovery` so that when the focused item is removed focus goes to the item that takes its place, else the last item, else the scope fallback. Grids and virtualised rows (Task 5) build their own groups on `createGridFocus` / roving with `reveal` and join the bridge through the same registration.
    - `useFocusItem({ key, onSelect?, onMenu?, onBack?, disabled? }) -> { ref, props, focused }`: an item in the nearest group. `focused` mirrors real focus, which is what the lighting styles key on. `onSelect` fires on Enter and click; `onMenu` on `m`, the ContextMenu key and right-click.
    - `<FocusScope id modal? fallbackKey? restoreKey? onBack?>`: one per screen and per overlay. A modal scope is a UI SDK modal layer (focus trapped, the rest inert) and returns focus to where it was on close. On mount a scope focuses `restoreKey` if that item exists, else its first group's stop. On unmount of a screen scope it reports the focused item's key to `nav.setFocusKey`.
    - `useInputMode(): "pad" | "keys" | "pointer"` and `useFocusedKey()`.
  - `bridge.ts`: a document-level `keydown` listener that runs after UI SDK's handlers. For an arrow key that no group consumed (`!event.defaultPrevented`), it finds the destination group with `nearestInDirection` over the bounding boxes of the active scope's groups (pure function `crossMove(groups: Array<{ id: string; rect: Box }>, from: Box, dir: [number, number]): string | undefined`), then focuses that group's remembered item, or its item nearest the origin on the cross axis. Nothing that way: call the scope's `onEdge`, else do nothing. Escape and Backspace (outside text fields) that nothing consumed: the focused item's `onBack`, then the scope's, then `nav.back()`. Global keys through UI SDK shortcuts: `/` and Ctrl+K (ask), `[` and `]` (previous/next section), `m` (menu). Tab is swallowed: there is no tab order in this app.
  - `gamepad.ts`: polled in `requestAnimationFrame` only while a pad is connected and the window is focused. Standard mapping: d-pad 12 to 15 and the left stick (0.5 dead zone) become `ArrowUp/Down/Left/Right` `keydown` events dispatched on `document.activeElement`; A (0) calls `activeElement.click()`; B (1) dispatches `Escape`; X (2) dispatches `m`; Y (3) dispatches `/`; LB (4) and RB (5) dispatch `[` and `]`; Menu (9) opens Settings. A held direction repeats after 380 ms, then every 110 ms, then every 60 ms after 1.2 s. Exports the pure `step(prev: PadState, pad: PadSnapshot | null, now: number): { state: PadState; actions: PadAction[] }`. Sets input mode `pad`.
  - `pointer.ts`: a `pointermove` whose coordinates really changed focuses the focus item under the pointer (`focus({ preventScroll: true })`) and sets mode `pointer`; enter/over events caused by content scrolling under a still pointer are ignored. The cursor hides after 3 s without movement and at once on pad or key input. Right-click dispatches the menu action. Uses UI SDK's pointer-kind helper where it applies.
  - `keys.ts`: sets mode `keys` on real key input; inside a text field arrows left/right, Backspace and characters stay with the field while up/down, Enter and Escape leave it to the bridge.
  - Focused items scroll into view with `block: "nearest"`, smooth unless reduced motion; virtualised groups use `reveal` instead.
  - `Hints`: a bottom-right line naming what the buttons do here ("A Open, X Menu, Y Ask, B Back"), shown only in `pad` mode, derived from the focused item's handlers.

- [ ] **Step 1: Write failing tests** (use UI SDK's own testing guidance in `docs/core.md` "Testing"; give items explicit rects through `spatialMove({ rect })` or a mocked `getBoundingClientRect`, since happy-dom has no layout).
  - `bridge.test.ts`: `crossMove` picks the group below when pressing down from a shelf, the top bar when pressing up from the first shelf, `undefined` at a true edge; an arrow consumed by a group is not handled again; Escape order (item, scope, nav); Tab does nothing; `/` opens ask from inside any group but types a slash inside a text field.
  - `react.test.tsx`: removing the focused item moves focus to its replacement, the last item when it was last, and the scope's `fallbackKey` when the group empties (Review Focus 4); a modal scope traps arrows and restores the previous focus on close; a screen scope restores `restoreKey`; leaving a group and returning lands on the remembered item.
  - `gamepad.test.ts`: `step` emits one move on press, none while held under 380 ms, repeats at the stated cadence, emits select once per A press, and resumes after the pad goes `null` and returns (Review Focus 5).
  - `pointer.test.ts`: an enter event with unchanged coordinates does not focus; a real move does (Review Focus 5).
  - `keys.test.ts`: left/right inside a focused text field do not move focus; up/down do.
- [ ] **Step 2: Run** `bun test src/focus`. Expected: fail.
- [ ] **Step 3: Implement**, then wire `TopBar` (a horizontal `FocusGroup`) and `Screens` (each screen a `FocusScope` whose `restoreKey` is the entry's `focusKey`; `[` and `]` cycle Home, Movies, Shows, Other).
- [ ] **Step 4: Run** `bun test`, `tsc --noEmit`. Expected: pass. Manual: with the app running, arrow keys move the lit item along the top bar and `scripts/shot.ps1 -Keys "{RIGHT}{RIGHT}"` shows it.
- [ ] **Step 5: Commit** `feat(ui): one focus system for gamepad, keyboard and mouse`.

---

### Task 5: The works: poster, placard, shelf, wall grid, item menu

**Files:**
- Create: `ui/src/components/{Work.tsx,FitTitle.tsx,Placard.tsx,Shelf.tsx,WallGrid.tsx,ItemMenu.tsx,Dialog.tsx}`, `ui/src/lib/actions.ts`, tests beside each

**Interfaces:**
- Consumes: Tasks 3 and 4.
- Produces:
  - `Work({ item: ItemSummary, shape: "poster" | "wide", focused: boolean, size })`: artwork (small poster file in grids and shelves; 16:9 `backdrop` for `wide`), typographic poster when there is none, lamp-white progress bar when `progress` is set and not watched, a small quiet mark when watched, greyed when `available` is false. Lighting per Design direction. Pure presentational: no focus hook inside.
  - `FitTitle({ text, maxLines = 2 })`: sets Archivo's width axis between 62 and 125 so the text fills its measure; long titles go to the narrowest width, then wrap to `maxLines`, then end with an ellipsis. Pure helper `fitWidth(textWidthAt100: number, measure: number): number` clamps to 62..125.
  - `Placard({ item: ItemSummary | Item, detail?: ItemDetail, size: "hero" | "panel" })`: the wall label. Lines built only with `format.facts`.
  - `Shelf({ id, title, items, onOpen })`: one horizontal row, virtualised with UI SDK `createVirtualizer`, that is one focus group: roving focus whose `move` goes by index so works not yet rendered are reachable (reveal, then focus when the item binds), remembers its item when focus leaves and returns, keeps the focused work one slot in from the left edge, and reports `onFocusItem(item)`.
  - `WallGrid({ id, items, columns, shape, onOpen, onFocusItem, selection? })`: rows virtualised with `createVirtualizer`, focus by UI SDK `createGridFocus` (`rows`, `cols`, `reveal` scrolling the row into view, cells past the end of a short last row `disabled` so down clamps to the last item), leaves the arrow unconsumed only at true edges so the bridge can move on, keeps the focused row inside the middle band of the viewport.
  - `actions.ts`: `itemActions(item: ItemSummary | ItemDetail): Action[]` where `Action = { id: "play"|"resume"|"restart"|"watched"|"unwatched"|"love"|"unlove"|"details"|"fix"|"other", label: string, disabled?: string }`: the single list every surface uses (item menu, details screen, wall panel), e.g. label "Resume from 1:04:12", "Play", and for a show "Play S2 E4" when given an `ItemDetail` with `next_episode` or "Play next episode" when given only a summary (both call `play(showId)`; the core picks the episode), and `disabled: "Drive is not connected"` for play actions on an unavailable item. `runAction(action, item, ctx)` performs it.
  - `ItemMenu`: UI SDK `menu` parts inside a modal `FocusScope`, listing `itemActions` for the focused work; opened by X, `m` or right-click anywhere a work is focused; closes on B.
  - `Dialog`: `components/ui/dialog.tsx` (UI SDK dialog) inside a modal `FocusScope`.
- [ ] **Step 1: Write failing tests**: `fitWidth` (short text gives 125, exact fit gives the ratio, very long gives 62); `WallGrid` cell math as pure functions `cellOf(index, columns)`, `indexOf(cell, columns)` and `isPastEnd(cell, columns, count)`, and behaviour through key events: down from the last full row into a shorter one lands on the last item; right at the row end and up from row 0 are left unconsumed; `Shelf` restores its remembered index; `itemActions` for: unwatched movie ("Play"), half-watched ("Resume from 1:04:12", "Play from the start"), watched ("Play", "Mark as not watched"), show with a next episode ("Play S2 E4"), unavailable (play disabled with the reason), needs-review other item (includes "Fix match"); Review Focus 3: `Placard` and `Work` rendered with a 90-character title, a CJK title, and an item whose year, overview, runtime and people are all null contain none of "undefined", "null", "NaN", ", ," and no line starts or ends with a comma; Review Focus 2: a summary with no poster renders the typographic poster with the title text.
- [ ] **Step 2: Run** `bun test`. Expected: fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `bun test`, `tsc --noEmit`. Expected: pass.
- [ ] **Step 5: Commit** `feat(ui): works, placard, shelf, wall grid and item menu`.

---

### Task 6: Home (Spotlight)

**Files:**
- Create: `ui/src/screens/Home.tsx`, test beside it
- Modify: `ui/src/nav/Screens.tsx`

**Interfaces:**
- Consumes: `Shelf`, `Placard`, `Backdrop`, `useShelf`, `itemActions`.
- Produces: the Spotlight screen. The focused title fills the window with its backdrop and a hero placard at the left; its primary action is shown as the placard's button text (not separately focusable: A on the work runs the first action from `itemActions`; X opens the menu; holding nothing else). Shelves along the bottom, in this order, each only when it has items: Continue watching (`continue_watching`, 20), Recently added (`recently_added`, 30), Not watched yet (`unwatched`, 30), Loved (`loved`, 30). One shelf is fully visible with the next one's title peeking; up and down move between shelves with the row sliding, left and right within. Up from the first shelf goes to the top bar. Entry focus: first item of the first shelf, or the entry's saved `focusKey`.
  - Empty library with sources: "Nothing here yet. Exhibit is still looking through your folders." with the job status line. No sources: handled by first run (Task 11). All shelves empty but library not: show Recently added.
  - A is "open details" for shows (goes to the Show screen) and "play/resume" for movies, episodes and other, as given by `itemActions`.
- [ ] **Step 1: Write failing test** `Home.test.tsx` (faked `invoke`): shelves with no items are not rendered; order is as above; focusing a work updates the placard title; an empty library shows the empty message.
- [ ] **Step 2: Run.** Expected: fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Verify** `bun test`; then look: run against a data dir with real artwork, capture Home with `scripts/shot.ps1` at rest, after `{RIGHT}{RIGHT}`, and after `{DOWN}`; check the lit work, tint and placard against the Design direction and fix what is off. Save the three PNGs under `docs/superpowers/shots/stage-2/` (committed: they are the owner's sign-off material).
- [ ] **Step 5: Commit** `feat(ui): Home spotlight`.

---

### Task 7: Wall (Movies, Shows, Other)

**Files:**
- Create: `ui/src/screens/Wall.tsx`, `ui/src/components/WallControls.tsx`, tests beside them

**Interfaces:**
- Consumes: `WallGrid`, `Placard`, `useItems`, `itemActions`, `ItemMenu`.
- Produces: `Wall({ kind })`. Grid at the left, details panel (26% of the width) at the right showing the focused item's `Placard size="panel"`, its primary action label, and for an item with technical details: video, audio, subtitles, drive, watched lines (from `useItem`, fetched 250 ms after focus settles). Columns per Global Constraints, minus two when the panel is shown. Other uses `shape="wide"`, four columns up to 1920, five up to 2560, six beyond, and is grouped by folder: a group heading (the file's parent folder name) starts each group; groups are navigated as one continuous grid.
  - `WallControls` above the grid, reachable by up: Sort (Title, Recently added, Year, Runtime), and filters Not watched, Loved, Available now, 4K, plus Genre (a list dialog of the genres present). Each is a toggle; the row states the count ("412 movies"). Controls map one to one onto `SearchQuery` fields; state lives in the route entry so Back restores it.
  - A on a movie or other item plays or resumes; A on a show opens Show. X opens the item menu, which includes "Details" and "Select several". In selection mode A toggles a work, the controls row becomes "3 selected, Mark as watched, Mark as not watched, Done", B leaves the mode.
  - States: loading (dim "Hanging the collection"), empty ("No movies yet" with what to do: add a folder in Settings, or wait for the scan), no results for filters ("Nothing matches these filters" with a "Clear filters" action), offline items dimmed in place.
- [ ] **Step 1: Write failing tests**: `columnsFor(widthCssPx, kind, panel)` returns 7/9/11 (5/7/9 with panel) and 4/5/6 for other; toggling a filter issues the matching `SearchQuery` (`{kind:"movie", watched:false}`); `groupByFolder` keeps groups in path order and items in title order; selection mode marks all selected ids watched with one mutation call per id and exits; the empty and no-results states render their messages.
- [ ] **Step 2: Run.** Expected: fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Verify** `bun test`; capture Movies with `scripts/shot.ps1` at rest and after `{DOWN}{RIGHT}`; check and fix.
- [ ] **Step 5: Commit** `feat(ui): the wall for movies, shows and other`.

---

### Task 8: Details, Show, and playing

**Files:**
- Create: `ui/src/screens/{Details.tsx,Show.tsx}`, `ui/src/lib/playback.ts`, tests beside them
- Modify: `ui/src/App.tsx`

**Interfaces:**
- Consumes: `useItem`, `itemActions`/`runAction`, `onPlayEnded`, `nav`.
- Produces:
  - `Details({ id })`: backdrop, hero placard with the full overview, director and first six cast names, genres; a vertical action list from `itemActions` (first action focused on entry); "Files": one line per version (file name, size, resolution and HDR, audio and subtitle languages, drive, "Offline" when unreachable); history ("Watched 3 March", "Stopped at 1:04:12 on 2 March", "Never played"); match line for needs review or manual ("Matched by hand", "Needs review").
  - `Show({ id })`: placard for the show, a season selector (left/right), and that season's episodes as a vertical list of wide works with "E4, title, runtime" and progress. On entry the next unwatched episode is focused and its season selected (`next_episode` is given by the show detail's first `play`/`resume` action target). A on an episode plays or resumes it. The item menu on the show or a season offers "Mark all as watched" / "Mark all as not watched".
  - `playback.ts`: `usePlayback()` holding `{ playing: Id | null }`; `start(id, resume)` calls `play`; while playing, input is ignored and a still line reads "Playing in mpv". On `play-ended`: clear, invalidate, and if `nextEpisode` is set and the current screen is that show, focus that episode; otherwise focus stays exactly where it was. Errors from `play` or in `play-ended` show a toast (`components/ui/toast.tsx`, UI SDK toast) with the message; when the error kind is `tool`, the toast has the action "Open Settings" and names the path that was tried.
  - Play on an unavailable item never calls `play`: the action is disabled and its reason is shown ("Drive WD 20TB is not connected", using the source path's drive when the label is unknown: "The drive for D:\Movies is not connected") (Review Focus 1).
- [ ] **Step 1: Write failing tests**: Details lists one line per file and marks an offline one; a `tool` error produces the toast with "Open Settings"; `play-ended` with `nextEpisode` moves focus to that episode's key on the Show screen and not elsewhere; unavailable item: selecting Play does not invoke `play` and shows the reason; Show focuses the next unwatched episode on entry; "Mark all as watched" calls `setWatched(showId, true)` once.
- [ ] **Step 2: Run.** Expected: fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Verify** `bun test`; capture Details and Show; manual with real mpv if a media file is available in the dev data dir, else note it for Task 12.
- [ ] **Step 5: Commit** `feat(ui): details, show and playing through mpv`.

---

### Task 9: Text entry, Ask as search, and Needs review

**Files:**
- Create: `ui/src/components/{TextField.tsx,Keyboard.tsx}`, `ui/src/screens/{Ask.tsx,Review.tsx}`, `ui/src/components/MatchPicker.tsx`, tests beside them
- Modify: `ui/src/App.tsx`, `ui/src/components/TopBar.tsx`, `ui/src/lib/actions.ts` ("Fix match" opens `MatchPicker`)

**Interfaces:**
- Produces:
  - `TextField({ value, onChange, onSubmit, placeholder, secret? })`: a focusable that, when selected, takes native text focus. In `pad` mode selecting it opens `Keyboard` instead.
  - `Keyboard`: an on-screen keyboard as a modal scope docked at the bottom: letters, digits, space, backspace, a symbols page, "Done". X is backspace, Y is space, the Menu button is Done, B closes. It edits the field it was opened for.
  - `Ask`: an overlay (modal scope) opened by Y, `/`, Ctrl+K or selecting the top bar field from any screen, closing with B back to the exact previous focus. One large text field; below it, results as a shelf of works per kind (Movies, Shows, Episodes, Other) from `search({ text, limit: 40 })`, debounced 200 ms; A on a result runs its first action, X opens the menu. With no text: the last six distinct queries (kept in `localStorage`) as selectable lines. No agent in this stage: the placeholder is "Search the library". States: "Nothing matches" with the query quoted.
  - `Review`: the needs-review list. Each entry: the file path (relative, with its folder), the parsed guess, and its candidates as a row of works with title, year and a plain confidence word ("Likely", "Possible", "Unlikely" for score at or above 0.8, 0.6, below). Actions per entry: select a candidate (applies the match), "Search by hand", "Send to Other". After any action the entry leaves the list and focus goes to the next entry (Review Focus 4 behaviour through the focus system). Empty: "Nothing needs review."
  - `MatchPicker({ itemId })`: dialog used by Review's "Search by hand" and by "Fix match" anywhere: kind toggle (Movie, Show), text field, optional year, results from `lookup`, selecting one calls `applyMatch`. Without a TMDB key: explains that a key is needed and offers "Open Settings".
- [ ] **Step 1: Write failing tests**: `Keyboard` typing "ab", backspace, space yields "a "; `TextField` in pad mode opens the keyboard and in keys mode does not; Ask debounces and renders groups by kind, stores recents without duplicates and caps at six; Review applies a candidate with the candidate's kind and id, and after "Send to Other" focus is on the next entry; confidence words at 0.8, 0.6, 0.59; `MatchPicker` without a key shows the Settings prompt and never calls `lookup`.
- [ ] **Step 2: Run.** Expected: fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Verify** `bun test`; capture Ask with a query typed and Review.
- [ ] **Step 5: Commit** `feat(ui): search box, on-screen keyboard and needs review`.

---

### Task 10: Settings and first run

**Files:**
- Create: `ui/src/screens/{Settings.tsx,FirstRun.tsx}`, `ui/src/components/FolderBrowser.tsx`, tests beside them
- Modify: `ui/src/App.tsx` (route to first run when `boot().firstRun`)

**Interfaces:**
- Produces:
  - `FolderBrowser({ onPick })`: dialog over `listFolders`: drives first, then folders; A opens a folder, "Add this folder" picks the current one, B goes up a level and closes at the top. (A native dialog cannot be driven by a gamepad.) A typed path is also accepted through a `TextField`.
  - `Settings`: one screen, four parts and nothing else. **Folders**: each source with its path, "Connected" or "Not connected", "Remove" (asks once: "Remove this folder from Exhibit? Your files are not touched."); "Add folder"; "Scan now". **Tools**: mpv, ffprobe, ffmpeg, each "Found at <path>, version <v>" or "Not found", with what it supplies when missing ("mpv plays your files", "ffprobe reads resolution, HDR and tracks faster; mpv is used instead", "ffmpeg makes thumbnails for Other; mpv is used instead"), "Set path" (text field or `FolderBrowser` showing `.exe` files), and "Use detected" when a hand-set path exists. Opening Settings re-runs detection. **TMDB key**: "Set" or "Not set", a secret text field with "Paste" (reads the clipboard through the browser API), "Save", "Remove"; saving starts `identify("unmatched_and_review")`; a rejected key shows the job's error. **Agent key**: "Set"/"Not set", save and remove only, with the line "Used from the next version of Exhibit." **Display**: a single "Full screen" toggle (`windowFullscreen`).
  - `FirstRun`: three steps, each one screen with one primary action and "Skip": add a folder (starts `refresh` at once, so the library fills in behind); tools (shown only for tools not found; mpv missing explains that nothing can play until it is set); TMDB key (explains what the key adds and that it is free; skipping is fine). Finishing goes to Home.
- [ ] **Step 1: Write failing tests**: `FolderBrowser` descends, goes up, and picks the current path; removing a source asks before calling `removeSource`; a missing tool shows its line of consequence (Review Focus 2); saving a TMDB key calls `setSecret("tmdb", value)` then `identify`, and the value is cleared from component state afterwards; `FirstRun` skips the tools step when all three are found and can be completed with every step skipped.
- [ ] **Step 2: Run.** Expected: fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Verify** `bun test`; capture Settings and each first-run step from a fresh temp data dir.
- [ ] **Step 5: Commit** `feat(ui): settings and first run`.

---

### Task 11: Whole-app check on the owner's library and at scale

This task is run by the session lead with the owner's library (source `D:\Movies`), mpv at `C:\mpv\mpv-x86_64-v3-20260610-git-304426c390`.

- [ ] **Step 1:** `bun run tauri build` (release). Record installer and executable size and idle memory.
- [ ] **Step 2:** Walk every screen with keyboard only, then mouse only; capture each with `scripts/shot.ps1` into `docs/superpowers/shots/stage-2/`. Fix what differs between inputs.
- [ ] **Step 3:** Play a film, quit part-way, confirm return to the same focus and a Resume action; unplug-equivalent check by pointing a source at a missing folder: items dim and Play explains.
- [ ] **Step 4:** Scale: add `crates/exhibit-core/examples/seed.rs` (`cargo run -p exhibit-core --example seed -- <data_dir> 5000`: 4,000 movies, 60 shows with 900 episodes, 100 other, reusing any images found in the real cache round-robin). Open the Wall against it; hold right and down for ten seconds each; record dropped frames from the WebView performance panel or a `requestAnimationFrame` gap counter printed by a dev-only overlay (`?fps`). Fix anything over 5% long frames.
- [ ] **Step 5:** Report to the owner with the screenshots: what works, sizes, what is open, and the palette and type for their verdict. Ask them to try the Xbox controller.
