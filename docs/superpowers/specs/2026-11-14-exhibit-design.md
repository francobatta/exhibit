# Exhibit: design

Date: 2026-11-14. Status: awaiting owner approval.

Items marked **(call)** were decided by the design author, not the owner, and are open to correction.

## 1. Intent

Exhibit is a desktop gallery for a local media collection, built for a home-theatre PC. It finds the movies, shows and other videos in one or more local folders, identifies them, and presents them in a fluid, TV-first interface. It never plays anything itself: it launches mpv and steps aside.

It exists because the owner has used Kodi, Jellyfin, Plex and MPC-HC and left all of them:

- Jellyfin and Plex are servers, with accounts, subscriptions, mobile apps and too many settings.
- Kodi is local and appliance-like, but its interface is dated and inconsistent, and it insists on being the player.
- None of them is built to be driven by an AI agent.

The ideal setup is Exhibit plus mpv, each doing only what it is best at.

## 2. Goals

In priority order:

1. **The interface is the product.** Fluid, modern, consistent, readable from a couch. Quality of motion and components is a headline feature.
2. **Robust identification of messy files.** Rip names and random folder structures are the normal case.
3. **Agent-first.** Everything the app can do is available to an outside agent as tools, and optionally to a built-in agent bar.
4. **Small footprint.** Small installer, low idle memory, nothing running when the app is closed.
5. **Simple, local recommendations** produced by the agent reasoning over the library and viewing history.

## 3. Non-goals

- Playing media, or any playback controls, subtitles, HDR or audio handling. mpv owns all of it.
- A server, remote access, a web or phone client, or any second device.
- Accounts, subscriptions, telemetry.
- Transcoding, live TV, downloading, podcast feed subscriptions.
- Music, photos, books, games.
- Importing viewing history from Kodi, Jellyfin, Plex or any other app (owner). Marking a whole show, or several selected films, as watched in one action covers the need.
- Disc rips as folder structures or ISO images.
- A plugin system or theming system.
- Bundling or managing an AI model.
- Linux and macOS in the first version. Nothing should be built in a way that blocks them later.

## 4. Anti-goals

The project has failed, even if it works, when:

- Settings multiply. There should be one screen of settings and strong defaults.
- Exhibit tries to be clever about playback instead of running mpv with the owner's own configuration.
- Screens behave differently from one another, or differently under gamepad and mouse.
- The gallery is useless without an API key or without an agent.
- A wrong match is shown confidently. An honest "needs review" beats a silent wrong guess.

## 5. Constraints

- Windows 11 first. The owner's machine currently has Git and Bun installed. Rust, Node.js and mpv are not on the PATH; the Rust toolchain must be installed before building, and mpv's location must be configurable.
- Media lives on internal drives and external direct-attached drives that may be disconnected at any time.
- Input is gamepad or mouse, with a TV as the main screen and a monitor as a second use. No other devices.
- Internet is available for metadata and the optional agent, but the gallery must open and browse with no connection.

## 6. Approaches considered

| Approach | Trade-off | Outcome |
|---|---|---|
| Tauri, Rust core, React UI | Tiny installer and low memory by using the browser engine built into Windows. React gives shadcn, Motion and Vercel's AI SDK at full strength. Two languages; a different engine on Linux later means an animation testing pass there. | **Chosen by the owner.** |
| Tauri, Rust core, SvelteKit UI | Lighter UI runtime, but shadcn and Motion are ports or substitutes, and new components arrive in React first. | Rejected: weakens the top goal. |
| Electron, TypeScript throughout | One language and identical rendering everywhere, but an installer over 100 MB and heavy idle memory. | Rejected: contradicts the footprint goal. |
| Next.js as the UI framework | Must be statically exported inside a desktop shell, which disables its server features. | Dropped early. |
| Go with Wails | Workable, thinner ecosystem, no advantage over Rust here. | Dropped early. |

## 7. Decisions made by the owner

- Exhibit launches mpv and never reproduces media.
- Agent support is both a tool surface for outside agents and a built-in agent bar, built in that order.
- The built-in agent uses a cloud model through the owner's own API key, and is optional.
- TV-first layout, also usable on a monitor. Gamepad and mouse are both first-class.
- The agent is reached through a single catch-all text box, with voice as an additional way to fill it.
- Movies and shows are core types. Podcasts are local files only and come later. Concerts, anime, lectures, home videos and anything unidentified live in "Other".
- Home uses the Spotlight layout. Movies, Shows and Other use the Wall layout. The ask box is one button away on every screen.
- Stack: Tauri 2, Rust core, React with Vite, shadcn, Motion, Vercel AI SDK, SQLite.

## 8. Architecture

```
            +---------------------------+
            |  React UI (Tauri window)  |
            +-------------+-------------+
                          |
  exhibit CLI ---+        |        +--- MCP server (stdio)
                 v        v        v
            +---------------------------+
            |   exhibit-core (Rust)     |
            |  library API = tool set   |
            +--+-------+-------+-----+--+
               |       |       |     |
            SQLite  scanner  metadata  mpv link
```

**exhibit-core** is a Rust library holding all behaviour: database, scanning, identification, metadata, playback launching and history. It exposes one set of operations (section 12). The UI, the command-line tool and the MCP server are three thin front ends over that same set. This is what "agent-first" means structurally: there is no feature the UI has that a tool lacks.

**(call)** The CLI and MCP server are one extra small binary (`exhibit`, with `exhibit mcp` starting the MCP server). They work whether or not the window is open, by opening the same database. SQLite runs in write-ahead mode so several processes can share it. How the open window learns of changes made by another process is left to the builder.

**(call) Where model calls run.** All calls to the AI model happen in the window, through the AI SDK. The core never calls a model. Consequences, all intended:

- Agent matching and agent shelves only run while the window is open, started by the UI. A rescan from the CLI or MCP runs name parsing and TMDB lookup only.
- An outside agent using the MCP server is itself the agent: it can list needs review and apply matches with the same tools.
- The key lives in the Windows credential store and is handed to the window by the core on request.

**(call) Play from the CLI or MCP.** If the window is open, the request is passed to it and behaves exactly like a play started in the UI. If not, the calling process launches mpv and tracks it: `exhibit play` stays running until mpv exits, then records position and watched state. If an MCP client disconnects mid-playback, that session's position is lost; this is accepted in exchange for having no background helper.

Nothing runs in the background when the app is closed. There is no service and no tray icon.

## 9. Data

SQLite, stored with an image cache under the user's application data folder. The builder designs the schema; it must represent:

- **Sources:** each folder the owner added, with the identity of the drive it lives on so an unplugged drive can be recognised when it returns, even under a different drive letter.
- **Files:** path, size, modified time, technical details (resolution, HDR, audio tracks, subtitle tracks, duration), and whether the file is currently reachable.
- **Items:** movie, show, season, episode, or other. An item can have several files (two versions of one film).
- **Match state:** matched, needs review, or manually set; the confidence score; the external ID.
- **Metadata:** title, year, overview, people, genres, runtime, artwork references. Stored locally in full, so browsing needs no network.
- **Viewing history:** resume position, watched flag, play dates.
- **Taste:** a single "loved" flag per item **(call: one flag, not star ratings, to keep it to one button on a gamepad)**, and a short free-text taste profile that the agent maintains and the owner can read and edit.
- **Full-text search** over titles, people, genres and overviews.

## 10. Scanning and identification

**Scanning.** Walk each source for video files. Detect added, changed, moved and removed files by path, size and modified time. Scan on launch and on demand, and watch connected sources for changes while the app is open. Scanning never blocks the interface; items appear as they are found.

**Disconnected drives.** When a source's drive is absent, its items stay in the library with all metadata and history, shown dimmed and marked offline. They are never deleted automatically. Play is disabled with a plain message naming the drive.

**What is on disk (owner).** Single video files, named inconsistently, sometimes with a `.nfo` file beside them. Disc folder structures, ISO images and films split across files are out of scope.

**`.nfo` files.** When a `.nfo` beside a file or in its folder carries a TMDB, IMDb or TVDB ID, that ID is used directly and the item counts as matched without scoring. A `.nfo` without an ID contributes its title and year to name parsing. Exhibit only reads these files and never writes them.

**Identification** otherwise runs in three layers:

1. **Name parsing.** Extract title, year, season and episode numbers from file and folder names, discarding release tags (resolution, codec, group names, and so on). Folder context counts: a folder of similarly named numbered files is probably one show.
2. **Online lookup with a confidence score.** Search TMDB for the parsed title. Score candidates on title similarity, year, runtime against the file's actual duration, and episode-count fit for shows. Above a threshold, accept. Below it, mark as needs review.
3. **Agent fallback**, only when an API key is set. The agent sees the unresolved paths, parsed guesses and top candidates, and proposes matches. **(call)** Agent proposals above the threshold are accepted and labelled as agent-matched; the rest stay in needs review.

**Needs review** is a visible section, not an error log. Each entry shows the file, the best guesses, and three actions: pick a candidate, search by hand, or send to Other. A manual choice is permanent and never overwritten by a rescan.

**Other.** Anything not confidently a movie or show. Shown with a thumbnail, a cleaned-up name and duration, grouped by folder. **(call)** Anime that TMDB identifies as a show or movie will appear under Shows or Movies, since telling it apart reliably is not possible; any item can be moved to Other by hand and will stay there.

**Metadata source.** **(call)** TMDB for movies and shows, using a free API key the owner enters once at first run. The key can be skipped: without it, every file appears under its cleaned-up name with a thumbnail and no artwork, and nothing is sent to needs review. Identification can be run over the whole library at any later time, so adding the key afterwards fills everything in. Artwork is downloaded to the local cache at sizes suited to a 4K TV.

**Technical details and thumbnails.** **(call)** Read resolution, HDR and track information directly from the file containers in Rust, and generate thumbnails for Other by asking mpv to render a single frame. This avoids shipping ffmpeg. If native parsing proves unreliable, the builder may propose an alternative, but must not add a large bundled dependency without asking.

## 11. Playback

- Play runs the configured mpv with the file path, fullscreen, and a start position when resuming. The owner's mpv configuration is used untouched.
- Exhibit connects to mpv's control channel only to read position, duration and end-of-file. It sends no playback commands.
- When mpv exits, Exhibit saves the position, marks the item watched past a threshold **(call: 90 percent)**, and returns focus to exactly where the owner was.
- For a show, Play on the show means the next unwatched episode. **(call)** Episodes do not auto-advance in the first version; after an episode ends, Exhibit returns with the next episode focused.
- **(call)** The player command is a single setting. A different player can be used for launching, without resume or watched tracking. This is not a supported feature beyond that one field.
- If mpv is missing or fails to start, say so plainly with the path that was tried and a button to set it.

## 12. Tool surface

One set of operations in exhibit-core, exposed identically to the UI, the CLI (JSON output) and the MCP server. At minimum:

- Search the library by text and by filters (type, genre, year, runtime, watched state, loved, resolution, availability).
- Get one item in full, including files and history.
- List shelves: continue watching, recently added, unwatched, loved.
- Play an item or resume it.
- Mark watched or unwatched; set or clear loved.
- List needs review; apply a match; move an item to Other.
- Read viewing history.
- Read and update the taste profile.
- Add or remove a source; trigger a rescan; report scan status.

Anything destructive to the owner's files is out of scope: Exhibit never renames, moves or deletes media.

## 13. Built-in agent

- Present only when an API key is set. With no key, the ask box becomes plain library search and nothing else changes.
- **(call)** Uses Anthropic's API with a current Claude model, through Vercel's AI SDK. The key is kept in the Windows credential store, never in the database or a plain file.
- The agent has the tool surface from section 12 and nothing else. It has no file system or shell access.
- One text box takes anything: "something tense under two hours", "what did I stop watching last week", "play the next Severance", "mark all of season one watched".
- Answers appear as a shelf of items with one line of reasoning each, not as a chat transcript. **(call)** There is no persistent chat history; the last few requests are offered as shortcuts.
- **Recommendations** are the agent using search, history, loved flags and the taste profile. There is no separate recommendation engine. With a key set, Home gains one or two agent-written shelves, refreshed when history changes rather than on every launch, to keep cost and latency down.
- **Voice** comes after the text box works. **(call)** The first approach to try is the speech input already built into Windows, since it adds no footprint; the builder should check it works from a gamepad before committing to it.

## 14. Interface

**Design language.** A gallery at night. Dark walls, artwork as the only strong colour, and the background taking its tint from the focused title. Each title is described by a placard, like a wall label in a museum: title, year, director, runtime, one line of description. Focus is shown as light falling on the focused work. One typeface family for display and one for text. Motion responds to the owner's actions (moving focus, opening, returning) and is otherwise still. The mockup at `.superpowers/brainstorm/home-layouts.html` shows layout only; palette and type there are placeholders.

**Screens.**

- **Home (Spotlight).** One focused title fills the screen with its backdrop and placard. Shelves run along the bottom: continue watching, recently added, agent shelves when available. Left and right move within a shelf, up and down between shelves.
- **Movies, Shows, Other (Wall).** A dense poster grid with a details panel for the focused item, plus sorting and a small set of filters. On a monitor this is the main working view.
- **Show.** Seasons and episodes, with the next unwatched episode focused on entry.
- **Item details.** Full placard, file details, versions, and actions: play or resume, mark watched, loved, fix match.
- **Needs review.** As in section 10.
- **Ask.** The text box, summoned by one gamepad button or a keyboard shortcut from anywhere, with an on-screen keyboard when a gamepad is in use.
- **Settings.** One screen: sources, mpv location, TMDB key, agent key. **(call)** Nothing else in the first version.
- **First run.** Add a folder, confirm mpv, enter the TMDB key. The library fills in live behind it.

**Input.** One focus system drives everything. Gamepad directions, keyboard arrows and mouse hover all move the same focus, so no screen can work under one input and not another. Every action is reachable with a direction pad and four buttons. **(call)** The window opens fullscreen by default and remembers if the owner changes that.

**Scaling.** Layout is sized relative to the window so the same screens work on a 4K TV at three metres and a monitor at arm's length. **(call)** The Wall shows more columns on a monitor instead of offering a density setting.

**States.** Every screen has designed empty, loading, offline-drive and error states. Errors say what happened and what to do, in plain words.

**Performance.** A library of several thousand items must scroll and move focus without dropped frames. Grids are virtualised and artwork is loaded at the size displayed.

## 15. Build order

1. **Core:** database, scanning, name parsing, TMDB matching, needs review, the operations in section 12, with tests. Usable through the CLI alone.
2. **Interface:** shell, focus system, Home, Wall, Show, details, settings, first run, mpv launching and resume.
3. **Outside agents:** the MCP server over the same operations.
4. **Built-in agent:** ask box, agent matching fallback, agent shelves, taste profile.
5. **Later:** voice, podcasts as local files, Linux.

Each stage ends with something the owner can run and judge.

## 16. Testing

- The name parser is tested against a large list of real-world rip names, including the owner's own, which should be collected from the library early.
- Matching is tested against recorded TMDB responses so tests run offline.
- Scanning is tested on temporary folder trees, including moves, deletions and a source that disappears.
- The mpv link is tested against a stand-in that speaks the same protocol, plus a manual check with real mpv.
- The interface has tests for the focus system, since consistency across inputs is an explicit goal.

## 17. Left to the builder

- Database schema and migration approach.
- Rust crates, React state and routing libraries, and project layout.
- The scoring formula and thresholds for matching, tuned against the test list.
- How other processes notify the open window of changes.
- Final palette, typefaces and motion details, within the design language in section 14, shown to the owner before being applied widely.
- Gamepad button mapping.

## 18. How the work is run

The session lead (Fable 5.2) directs and reviews. Implementation tasks are delegated to subagents: Opus for work needing design judgement (core architecture, the matcher, the focus system, visual design), Sonnet for well-specified tasks.
