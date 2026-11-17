---
name: dev
description: Run the Exhibit app in dev mode (Tauri + Vite, hot reload) on the dev library, a fresh first-run library, or the real one; drive it with keys and take screenshots over CDP; stop it. Use when asked to run, start, try, look at or screenshot the app, or to check a UI change in the running window.
---

# Run Exhibit in dev mode

Everything goes through `scripts/dev.ps1` (start and stop) and `scripts/cdp.ts` (keys, screenshots, eval). Bun only; never `node`, `npm` or `npx`.

## Start

Run in the background (it does not exit while the window is open):

```powershell
& scripts/dev.ps1 -Cdp                 # dev library: .superpowers/dev-data, keyring exhibit-dev
& scripts/dev.ps1 -Data fresh -Cdp     # empty temp data dir, no TMDB key: first run
& scripts/dev.ps1 -Fullscreen          # as on the TV
```

- `-Data real` opens the owner's real data dir and keyring. Use it only when the owner asks.
- The first start compiles Rust, which takes minutes. Later starts take seconds.
- Ready means `curl -s http://127.0.0.1:9333/json/version` answers. Wait for it with a background `until` loop, not with sleeps.

## Look and drive

```bash
bun scripts/cdp.ts shot C:/path/to/scratch/home.jpg           # 1920x1080 JPEG; then open it with Read
bun scripts/cdp.ts keys ArrowRight ArrowRight Enter            # trusted key presses
bun scripts/cdp.ts eval "document.activeElement?.textContent"  # inspect the page
```

**Keys.** Every input device reaches the app as the same keys.

| Pad | Keyboard |
|---|---|
| A | `Enter` |
| B | `Escape` |
| X | `m` (item menu) |
| Y | `/` (Ask) |
| LB / RB | `[` / `]` (sections) |

Put scratch screenshots in your scratchpad. Only screenshots meant to stay go to `docs/superpowers/shots/`, which is git-ignored. Always open a screenshot and look at it before you say a change works.

## Stop

```powershell
& scripts/dev.ps1 -Stop
```

This ends Vite, `tauri dev` and `exhibit-app` for this repo. Run it when you are done, and before a fresh start if port 1420 is busy.

## Dev library facts

`.superpowers/README.md` lists what is in the dev library: sources, review fixtures and watched state. The rules for working with it:

- Never print, remove or overwrite the TMDB key in the `exhibit-dev` keyring.
- Keep the six Needs review fixtures unresolved. If a test resolves one, restore it.
- Never write inside `D:\`, the owner's media.
- mpv and ffmpeg are at `C:\mpv\mpv-x86_64-v3-20260610-git-304426c390`. Play only briefly, and only when the check needs it.
