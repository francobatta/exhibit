# Exhibit: design

The living design. It says what Exhibit is, what it must never become, and every owner decision
still in force. When a feature changes a decision, edit this file in the same commit; when the code
and this file disagree, ask the owner which is right. The history of each decision is in git
(`git log -- docs/`).

## 1. Intent

Exhibit is a desktop gallery for a local media collection, built for a home-theatre PC. It finds the
movies, shows and other videos in local folders, identifies them, and presents them in a fluid,
TV-first interface. It never plays anything itself: it launches mpv and steps aside.

The owner has used Kodi, Jellyfin, Plex and MPC-HC and left all of them: Jellyfin and Plex are
servers with accounts and too many settings; Kodi is dated, inconsistent and insists on being the
player; none is built to be driven by an AI agent. The ideal setup is Exhibit plus mpv, each doing
only what it is best at.

## 2. Goals, in order

1. **The interface is the product.** Fluid, modern, consistent, readable from a couch.
2. **Robust identification of messy files.** Rip names and random folders are the normal case.
3. **Agent-first.** Everything the app can do is a tool for an outside agent, and later for a
   built-in agent bar.
4. **Small footprint.** Small installer, low idle memory, nothing running when the app is closed.
5. **Simple, local recommendations**, by the agent reasoning over the library and history.

## 3. Non-goals

- Playing media, or any playback controls, subtitles, HDR or audio handling. mpv owns all of it.
- A server, remote access, a web or phone client, or any second device.
- Accounts, subscriptions, telemetry.
- Transcoding, live TV, downloading media, podcast feeds. Music, photos, books, games.
- Importing viewing history from Kodi, Jellyfin, Plex or any other app, except through a service the
  owner connects (Letterboxd), whose watches count as watched under the owner's own choice.
- Disc rips as folder structures or ISO images; films split across files.
- A plugin system or theming system. Bundling or managing an AI model.
- Linux and macOS in the first version, though nothing may block them later.

## 4. Anti-goals

The project has failed, even if it works, when:

- Settings multiply. One screen of settings and strong defaults.
- Exhibit tries to be clever about playback instead of running mpv with the owner's configuration.
- Screens behave differently from one another, or differently under gamepad, keyboard and mouse.
- The gallery is useless without an API key or without an agent.
- A wrong match is shown confidently. An honest "needs review" beats a silent wrong guess.
- A screen waits on the network, or a service's failure shows on a screen that did not ask for it.

## 5. Constraints

- Windows 11 first. Media lives on internal and external drives that may vanish at any time.
- Input is gamepad, keyboard or mouse; a TV is the main screen, a monitor the second.
- Internet is used for metadata, services and the optional agent; the gallery opens and browses
  with no connection.
- Stack (owner): Tauri 2, Rust core, React with Vite, Vercel UI SDK with shadcn conventions,
  Motion, TanStack Query, SQLite. Chosen over Svelte (weaker components), Electron (footprint),
  Next.js and Wails.

## 6. Architecture

```
  React UI (Tauri window)     exhibit CLI     MCP server (stage 3)
               \                  |                 /
                +------ exhibit-core (Rust): one set of operations ------+
                        SQLite · scanner · identify · probe · mpv link · services
```

- **exhibit-core** holds all behaviour. The UI, the CLI and the MCP server are thin front ends over
  the same operations, so no feature is UI-only. The UI never touches the database, files or
  network itself.
- The CLI and MCP server open the same database (SQLite in WAL mode) whether or not the window is
  open.
- **Model calls run only in the window** (Vercel AI SDK); the core never calls a model. The agent's
  key lives in the Windows credential store.
- **Play from the CLI or MCP:** passed to an open window if there is one (stage 3); otherwise the
  caller launches mpv and waits to record position and watched state.
- Nothing runs in the background when the app is closed: no service, no tray icon.
- **Services are adapters** (`crates/exhibit-letterboxd`, `-mdblist`, `-wikidata`, registered in
  `exhibit-services`) behind one neutral interface in `service.rs`. The core names no site; a test
  enforces it. A second site is a second adapter and touches no screen.

## 7. Library and identification

- **Sources** remember their drive's identity, so an unplugged drive is recognised on return under
  any letter. Its items stay, dimmed and marked offline, never deleted; Play names the drive.
- **Items:** movie, show, season, episode or other; one item may have several files.
- **Scanning** runs on launch, on demand and on file-system changes, never blocks the interface,
  and skips extras folders (featurettes, trailers and similar).
- **`.nfo` files** with a TMDB, IMDb or TVDB id match directly; without one they add title and year.
  Exhibit only reads them. It never renames, moves or deletes media.
- **Identification:** name parsing (folder context counts), then TMDB search scored on title, year,
  runtime and episode fit, searched in the library's languages. Above the threshold it is accepted;
  below it goes to **Needs review** (pick a candidate, search by hand, or send to Other). A manual
  choice is permanent. Stage 4 adds an agent fallback.
- **Other:** anything not confidently a movie or show, with a thumbnail, a cleaned name and
  duration, grouped by folder. Any item can be moved there by hand and stays.
- **TMDB:** Exhibit ships its own key (baked in from the git-ignored `tmdb-app-key`, lightly
  scrambled, accepted as recoverable). Settings keeps "Use your own key". Without any key, files
  show under cleaned names with thumbnails. TMDB data is re-fetched within five months, a few at a
  time. Before Exhibit has users, register a separate key instead of the owner's dev key.
- **Tools are mpv and ffmpeg** (ffprobe was removed). Order: a path set by hand, then a
  downloaded copy, then one detected. One press downloads both from shinchiro's builds (unofficial,
  said beside the button), latest release, matched to the machine. No background update checks.
  mpv reads technical details; ffmpeg makes thumbnails, with mpv standing in.
- **Formats:** Dolby Vision, HDR10, each track's best sound (DTS-HD MA, DTS:X, Atmos), and the
  original, commentary, forced and SDH flags. Marks are drawn in the text colour and may come from
  reputable, not only official, sources. The sound mark names the best format on any track.
- **What a work is about:** with each work's details TMDB gives its keywords (English only), its
  crew (director, writer, cinematographer, composer, editor) and its recommendations, all in the
  one request. They feed the keyword and people filters, search by crew, and `similar`: the works
  in the library most like one, scored offline from TMDB's recommendation either way, shared
  director and writer, keywords weighted by how few works share them, the rest of the crew and
  cast, and a little for genre, each result saying what links it.

## 8. Playback

- mpv runs with the file, full screen, and a start position when resuming. The owner's mpv
  configuration is untouched; Exhibit only reads position, duration and end-of-file.
- On exit: save the position, mark watched past 90%, return focus to exactly where the owner was.
- Play on a show means the next unwatched episode. No auto-advance; the next episode is focused on
  return.
- If mpv is missing or fails, say so with the path tried and a button to fix it.

## 9. Services

- **Letterboxd** (no API application): the public RSS feed plus the account export ZIP, with a
  guided export in Settings. Ratings wherever rated, the review and history on Details, activity
  filters, a your-rating sort, and the whole watchlist one way, including films not owned, shown
  unlit and unplayable.
- **Merging rule:** the owner's choice in Exhibit, else Exhibit's own play, else an enabled service.
  Un-marking a film Letterboxd logged changes Exhibit only.
- **MDBList** public scores (device sign-in with a QR code) and **Wikidata** laurels, one per body.
- Every score carries its source's mark; none reads as Exhibit's own. An export row that cannot be
  matched surely is left out and counted, never guessed. Adapters only read.
- A service that is off or never set up leaves no trace: no empty rows, greyed filters or nags.
  Services are one part of Settings.

## 10. Interface

**Design language.** A gallery at night: dark walls, artwork as the only strong colour, the
background tinted by the focused title, light falling on the focused work, and a placard like a
museum wall label. Approved tokens: no accent colour, lamp-white light, Archivo (width axis) and
Source Serif 4. No accent colour, and no setting for one (owner, 2026-11-22): colour comes from
the artwork and from the lamp, whose warmth follows the time of day (daylight in the morning, the
evening lamp, lower and warmer after midnight).

**Screens.**

- **Home** is the programme for now, films and shows together. The part of the day names it and
  sets its shelves: This morning (short ones, new arrivals), This afternoon and Tonight (your
  watchlist, the season's shelf, well reviewed), After hours (short ones, loved). Weekends, from
  Friday evening, add a long film; the season, by hemisphere, adds one shelf of its own. One work
  is put forward with focus on its Play: something to resume, else the first suitable work, varied
  by day and held through the evening, preferring one that can play now. A greeting above the
  placard names the programme and why; beside Play, when the work would end. No setting.
- **Movies** has the Spotlight (one focused title with
  its backdrop and placard) beside its wall, and a row of views under the top bar that one button
  switches; **Shows** and **Other** use the Wall.
- **Wall:** a virtualised poster grid with sort (each either way; A again reverses) and a live
  filter sheet beside it: genres wanted or left out, the most shared keywords, year and runtime
  ranges, picture, sound, languages and activity. Deterministic; the agent comes later.
- **Show:** seasons and episodes, the next unwatched episode focused on entry.
- **Details:** the full placard (with the crew beyond the director), Reception, Your review, the
  cast, More like this (each with its strongest link: "Also by Brian De Palma"), History, file
  details, versions, and play or resume, watched, loved, fix match.
- **Hero placard:** the title alone; the story (byline "director, year" over the description)
  beside the verdicts as large figures with their marks beneath; the file in two quiet lines
  (runtime, state, marks; then "English and 2 other languages, 19 subtitles"). Stacks where narrow.
- **Needs review**, **Ask** (one button from anywhere, on-screen keyboard with a pad; plain search
  until stage 4), **First run** (add a folder, offer to download missing tools, MDBList sign-in, then the AI agent).
- **Settings:** one screen. Sources, tools, TMDB key, agent key, Services, AI agent, Display (full screen,
  intro), Language and Dates.
- **Intro:** the lit E spawns, creeps while the library opens, then zooms through its hole onto
  Home; about 1.5 s, silent, any button skips without acting on Home; one switch in Settings.

**Input.** One focus system for gamepad, keyboard and mouse. A d-pad and four buttons reach
everything. Back only, no forward; it returns to where the owner was. "← Back" stands in the bar
on stacked screens; the mouse's back button is Back. Hints follow the last device: pad glyphs, key
caps, nothing for the mouse. The window opens full screen and remembers a change.

**Languages.** `en-US`, `es-ES`, `es-MX` (separate choices), defaulting from Windows. Dates are a
second setting. Every string comes from a catalogue (`ui/src/i18n/locales`); TMDB text is stored
per language with English as the base. Errors travel as codes with parameters and are worded by
the window. The CLI stays English; mpv keeps its own language settings. No flash of English.

**Scaling and speed.** Sized in `rem` against the window, so one layout serves a 4K TV at three
metres and a monitor; the Wall shows more columns rather than offering a density setting. Several
thousand items scroll without dropped frames; artwork loads at its shown size. Every screen has
designed empty, loading, offline and error states.

## 11. Stages

1. **Core** and CLI. Done.
2. **Interface,** then languages, navigation, sort and filters, services, the hero placard. Done;
   released as 0.0.2.
3. **Outside agents:** the MCP server (`exhibit mcp`, `Exhibit.exe mcp`; Rust, rmcp, stdio)
   over the same operations, built. Tools read the library and write only Exhibit's own state
   (watched, loved, taste profile, exhibitions, matches, play); answers are compact JSON so an
   agent spends its tokens on the library. Exhibitions: a curator hangs 3 to 24 works the owner
   has, with a title, a wall text and a note on each; at most six hang, as shelves on Home after
   Continue watching. A shelf opens with its wall text and who hung it, where three works would
   be; the note on the focused work sits above its placard, once the owner is on that shelf.
   **Onboarding an agent** follows a hosted service's setup link (owner, 2026-11): one button,
   "Copy setup prompt", on first run's last step and in Settings, copies one line that sends the
   agent to `agent-setup.md`, written into the data directory on each copy for this exe and this
   library; the file has the agent check the server (`mcp --check`), register it at user level in
   its own client, and confirm. The program installs as `Exhibit.exe`, the path the file names;
   a release ships the installer alone, no portable exe (owner, 2026-11).
   Still to do: hand `play` to an open window instead of starting mpv itself.
4. **Built-in agent** (Anthropic API through the Vercel AI SDK, owner's key, optional): one text box
   whose answers come back as a shelf with a line of reasoning each, not a chat. Agent matching,
   agent shelves on Home, a taste profile the owner can read and edit. Ask is shaped so stage 4
   swaps only `useAsk`.
5. **Later:** voice (try Windows speech input first), podcasts as local files, Linux.

## 12. Testing

The parser against real rip names; matching against recorded TMDB responses; scanning on temporary
trees; the mpv link against a stand-in; services against fakes, never the network; the focus system
across inputs. Tests support the work; they are not the deliverable. No mutation testing.
