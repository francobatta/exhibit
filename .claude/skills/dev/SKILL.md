---
name: dev
description: Open the Exhibit app in dev mode (Tauri + Vite, hot reload) on the dev library, a fresh first-run library, or the real one, and stop it. Use when asked to run, start or open the app.
---

# Run Exhibit in dev mode

Open the app and stop there. Don't screenshot, drive or check anything unless asked. Bun only; never `node`, `npm` or `npx`.

## Start

Run in the background (it does not exit while the window is open):

```powershell
& scripts/dev.ps1 -Cdp                 # dev library: .superpowers/dev-data, keyring exhibit-dev
& scripts/dev.ps1 -Data fresh -Cdp     # empty temp data dir, no TMDB key: first run
& scripts/dev.ps1 -Fullscreen          # as on the TV
```

- `-Data real` opens the owner's real data dir and keyring. Use it only when the owner asks.
- The first start compiles Rust, which takes minutes. Later starts take seconds.
- Ready means `curl -s http://127.0.0.1:9333/json/version` answers. Wait for it with a background `until` loop, not with sleeps. Then say it's open.

## Stop

```powershell
& scripts/dev.ps1 -Stop
```

This ends Vite, `tauri dev` and `exhibit-app` for this repo. Run it when asked, and before a fresh start if port 1420 is busy.
