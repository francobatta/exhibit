# Exhibit

A desktop gallery for a local media collection, built for a home-theatre PC. Exhibit finds the
movies, shows and other videos in your folders, identifies them through TMDB, and presents them in
a TV-first interface you can drive with a gamepad, a keyboard or a mouse. It never plays anything
itself: it launches [mpv](https://mpv.io) and steps aside.

Windows 11 first. Stages 1 and 2 of 4, the core library and the window, are built and released as
0.0.2. The owner's whole-app check and stage 3 (the MCP server) come next. The design is
`docs/design.md`.

## Layout

| Path | What |
|---|---|
| `crates/exhibit-core` | The library: scan, parse, identify (TMDB), probe, thumbnails, play state, SQLite |
| `crates/exhibit-cli` | The `exhibit` command line over the core |
| `crates/exhibit-app` | Tauri 2 shell: one command per library operation, a worker, folder watching, playback |
| `crates/exhibit-letterboxd`, `exhibit-mdblist`, `exhibit-wikidata` | Service adapters: your Letterboxd activity, public scores (MDBList, signed in with a QR code), laurels (Wikidata) |
| `crates/exhibit-services` | The registry of adapters compiled in |
| `crates/exhibit-mcp` | The MCP server: the library's operations as tools for an outside agent |
| `ui/` | React 19, TypeScript, Vite, Tailwind 4, Vercel UI SDK, Motion, TanStack Query (see `ui/AGENTS.md`) |
| `scripts/` | `dev.ps1` (run in dev mode), `cdp.ts` (drive and screenshot a dev window), `shot.ps1`, `release.ps1` (build, tag and publish a release) |
| `docs/design.md` | The living design: intent, anti-goals, owner decisions |

## Prerequisites

- Rust, stable MSVC toolchain.
- [Bun](https://bun.sh). Node.js is not used: run `bun` and `bunx --bun`, never `node`, `npm` or `npx`.
- mpv, which plays your files and reads their details, and ffmpeg, which makes thumbnails (optional: mpv stands in). Exhibit detects both, downloads both in one press from Settings or first run (`exhibit tools download`), or takes their paths by hand.
- **Exhibit's own TMDB key**, for titles, artwork and matching, baked into the build so the owner needs none. It never goes into git: put it in the git-ignored file `tmdb-app-key` at the repo root, or set `EXHIBIT_TMDB_KEY` when building (`crates/exhibit-core/build.rs`). A file added after a build without one is picked up after `cargo clean -p exhibit-core`. A build without a key asks the owner for one, as Settings always allows ("Use your own key"). The key is only scrambled in the exe, never secret: it is a free, revocable key. Exhibit fetches TMDB's details of each work again within five months, a few at a time, as TMDB's terms ask.

Run `bun install` in `ui/` once.

## Development

```powershell
scripts/dev.ps1                 # dev window on the dev library (.superpowers/dev-data), hot reload
scripts/dev.ps1 -Data fresh     # empty temp library and no keys of yours: the first-run flow
scripts/dev.ps1 -Fullscreen     # full screen, as on the TV
scripts/dev.ps1 -Cdp            # also expose DevTools protocol on port 9333
scripts/dev.ps1 -Stop           # stop every dev process for this repo
```

- `-Data real` uses your real data directory and keyring.
- `-Data <path>` uses any other data directory.

The first start compiles Rust and takes a few minutes. After that, UI edits reload at once and Rust edits rebuild the shell.

With `-Cdp` the window can be driven and captured from a terminal:

```bash
bun scripts/cdp.ts keys ArrowRight Enter      # Enter is A, Escape is B, m is X, / is Y
bun scripts/cdp.ts shot home.jpg              # 1920x1080 JPEG
bun scripts/cdp.ts eval "document.title"
```

The environment variables behind the script:

| Variable | Effect |
|---|---|
| `EXHIBIT_DATA_DIR` | Data directory: database and image cache; also holds downloaded tools (`tools\`) instead of `%LOCALAPPDATA%\exhibit\tools` |
| `EXHIBIT_KEYRING_SERVICE` | Credential-store service name, so a dev run's TMDB key is kept apart from your real one |
| `EXHIBIT_WINDOWED=1` | A 1600x900 window instead of full screen |
| `EXHIBIT_TMDB_BUILTIN=off` | Leaves out the build's own TMDB key at run time, as if the build had none (the CLI's end-to-end tests) |
| `EXHIBIT_TMDB_KEY` | At build time: the TMDB key to bake in, instead of the file `tmdb-app-key` |

## Agents (MCP)

Exhibit serves its library to an outside agent (Claude Code, Claude Desktop, any MCP client) over
stdio: `exhibit mcp` from the CLI, or `Exhibit.exe mcp` from an installed build, which opens no
window. The tools read the library (overview, search by keyword and person, one work in full,
similar works, shelves, history), and write only Exhibit's own state: watched and loved, the taste
profile, exhibitions (curated shelves on Home), matches in review, and play. Three prompts start
the common jobs: `curate_exhibition`, `recommend`, `update_taste_profile`. An open window picks
up what an agent writes within a second.

**Setting an agent up** works like a hosted service's setup link. First run's last step, or
Settings > AI agent, copies one line to paste into the agent: "Read and follow the instructions in
`…\agent-setup.md` to set me up for Exhibit." On each copy, the window writes that file into the
data directory. It names the exe that wrote it and the library it shows (with `EXHIBIT_DATA_DIR`
and `EXHIBIT_KEYRING_SERVICE` when set), and tells the agent to check the server, register it at
user level in its own client (Claude Code, Claude Desktop, Codex, others), and confirm it
connects. The CLI does the same:

```powershell
Exhibit.exe mcp --setup   # writes agent-setup.md, prints the line to paste
Exhibit.exe mcp --check   # starts the server as a client would and reports the library
```

`Exhibit.exe` is a windowed app: PowerShell neither waits for it nor shows its output, so run these
two through `cmd /c` there. MCP clients start it with pipes and need nothing special. For the dev
library, `exhibit mcp --setup` with `EXHIBIT_DATA_DIR=.superpowers/dev-data` and
`EXHIBIT_KEYRING_SERVICE=exhibit-dev` writes instructions for a debug build on it.

## Checks

```powershell
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd ui; bun test --parallel; bunx --bun tsc --noEmit; bun run build
```

Release: `scripts/release.ps1 -Tag x.y.z` runs the checks, builds the exe and the installer, smoke-tests the build on an empty library, then tags and publishes a GitHub release. `-DryRun` stops before the commit. A bare build is `bun run tauri build` in `ui/`.
