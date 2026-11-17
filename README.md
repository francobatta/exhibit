# Exhibit

A desktop gallery for a local media collection, built for a home-theatre PC. Exhibit finds the
movies, shows and other videos in your folders, identifies them through TMDB, and presents them in
a TV-first interface you can drive with a gamepad, a keyboard or a mouse. It never plays anything
itself: it launches [mpv](https://mpv.io) and steps aside.

Windows 11 first. The status is stage 2 of 4: the core library and the window are built and under
the owner's whole-app check. The design and the plans are in `docs/superpowers/`.

## Layout

| Path | What |
|---|---|
| `crates/exhibit-core` | The library: scan, parse, identify (TMDB), probe, thumbnails, play state, SQLite |
| `crates/exhibit-cli` | The `exhibit` command line over the core |
| `crates/exhibit-app` | Tauri 2 shell: one command per library operation, a worker, folder watching, playback |
| `ui/` | React 19, TypeScript, Vite, Tailwind 4, Vercel UI SDK, Motion, TanStack Query (see `ui/AGENTS.md`) |
| `scripts/` | `dev.ps1` (run in dev mode), `cdp.ts` (drive and screenshot a dev window), `shot.ps1` |
| `docs/superpowers/` | Spec, stage plans, screenshots |

## Prerequisites

- Rust, stable MSVC toolchain.
- [Bun](https://bun.sh). Node.js is not used: run `bun` and `bunx --bun`, never `node`, `npm` or `npx`.
- mpv, which plays your files and reads their details, and ffmpeg, which makes thumbnails (optional: mpv stands in). Exhibit detects both, downloads both in one press from Settings or first run (`exhibit tools download`), or takes their paths by hand.
- A free TMDB key, optional: titles, artwork and matching. Without one, every file shows under its cleaned-up name.

Run `bun install` in `ui/` once.

## Development

```powershell
scripts/dev.ps1                 # dev window on the dev library (.superpowers/dev-data), hot reload
scripts/dev.ps1 -Data fresh     # empty temp library and no TMDB key: the first-run flow
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

## Checks

```powershell
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd ui; bun test --parallel; bunx --bun tsc --noEmit; bun run build
```

Release build: run `bun run tauri build` in `ui/`.
