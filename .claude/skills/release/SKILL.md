---
name: release
description: Use when asked to release, publish, ship or tag a version of Exhibit, build the production Windows app or its installer, or upload a build to GitHub Releases.
---

# Release Exhibit for Windows

A release is one run of `scripts/release.ps1`. It turns a clean `main` into a tagged commit and a GitHub release that carries an installer and a portable exe. The script runs every step and stops at the first failure, so nobody has to remember the steps.

## 1. Get the tag first

The tag is a bare version, `x.y.z` (`0.0.2`, not `v0.0.2`), newer than every earlier tag. It also becomes the app's version.

**If the user did not give a tag, ask before doing anything else.** Read the latest tag first, so the question can offer real choices:

```bash
git fetch --tags -q && git tag --list | grep -E '^[0-9]+\.[0-9]+\.[0-9]+$' | sort -V | tail -1
```

Ask with AskUserQuestion. Offer the next patch first, then the next minor (for example `0.0.2`, then `0.1.0`); the user can type another. If they give `v0.0.2`, use `0.0.2` and say so.

## 2. Clear the way

- Stop the dev app: `& scripts/dev.ps1 -Stop`. Exhibit is single-instance, so while any Exhibit runs, the new build hands itself to it and the smoke test sees nothing.
- If an installed Exhibit is open, ask the user to close it. Never kill their copy.
- The tree must be clean and on `main`. If it is not, tell the user what is uncommitted and stop. Never stash or commit their work to make room for a release.

## 3. Run it

Run in the background, because it compiles in release mode and takes minutes:

```powershell
& scripts/release.ps1 -Tag 0.0.2 -CommitTrailer "Co-Authored-By: <the attribution line for this session>"
```

- `-DryRun` runs everything up to and including the smoke test, then puts the version files back. Nothing is committed, tagged, pushed or uploaded. Use it when the user wants to try a release, or after you change the script.
- `-CdpPort` (default 9334) is the DevTools port for the smoke test. It is kept apart from the dev app's 9333.

The user asked for the release, so pushing the tag and creating the release is the request itself. Don't ask again before the publish step.

## 4. Check the result

1. Open the smoke screenshot the script prints (`%TEMP%\exhibit-release-<tag>.jpg`) with Read. It must show the first-run screen ("Welcome to Exhibit") with its fonts and dark wall. It must not show a blank page, an unstyled page, or the intro's lone "E".
2. Confirm the release: `gh release view <tag> --json url,assets --jq '.url, (.assets[] | "\(.name) \(.size)")'`. It must list the two assets.
3. Report the release URL, the assets and their sizes, and what the checks covered. Say that the installer itself was not run.

## What the script does

| Step | What happens | Stops when |
|---|---|---|
| 1. Preflight | Fetches tags | The tag isn't `x.y.z`, or isn't newer than the latest tag; not on `main`; the tree is dirty; `main` is behind `origin/main`; the tag or its release already exists; `gh` isn't signed in; `exhibit-app` or `Exhibit` is running; the CDP port answers |
| 2. Version | Writes the tag into `Cargo.toml` (`[workspace.package]`), every `exhibit-* = { version = ..., path = ... }` pin in `crates/*/Cargo.toml`, `crates/exhibit-app/tauri.conf.json` and `ui/package.json`. `Cargo.lock` follows at the first cargo run | A file's version line can't be found |
| 3. Checks | `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`; in `ui/`: `bun install --frozen-lockfile`, `bun test --parallel`, `bunx --bun tsc --noEmit` | Any check fails |
| 4. Build | `bun run tauri build` in `ui/`. Its `beforeBuildCommand` builds the UI; Tauri builds `target/release/Exhibit.exe` and the NSIS installer. The script then reads the exe's ProductVersion | The installer is missing, or the exe's version isn't the tag |
| 5. Smoke test | Starts the release exe windowed, on an empty temp data dir, with keyring service `exhibit-release-check` and DevTools on `-CdpPort`. Waits for the `http://tauri.localhost` page, then for the intro (`[data-slot=intro]`) to finish and the "Welcome to Exhibit" text to show, and takes a screenshot; runs `cmd /c "Exhibit.exe" mcp --check` (cmd, because PowerShell neither waits for a windowed exe nor passes on its output); then closes the app and deletes the temp dir | Exhibit started during the build; the exe exits (code 0 means another Exhibit took over); no window appears in 60 s; first run doesn't show in 30 s; or the MCP check fails |
| 6. Publish | Copies the exe to `Exhibit_<tag>_x64_portable.exe`; writes release notes (downloads, needs, SmartScreen, and "Changes since" from `git log`); commits `release: <tag>`; makes an annotated tag; pushes `main`, then the tag; `gh release create <tag> <setup> <portable> --verify-tag` | Any git or gh command fails |

Build output: `target/release/bundle/nsis/Exhibit_<tag>_x64-setup.exe`, `target/release/bundle/Exhibit_<tag>_x64_portable.exe`, and the notes in `target/release/bundle/release-notes-<tag>.md`. `target/` is git-ignored.

If steps 1 to 5 fail, the script puts the version files back (`git checkout HEAD -- ...`). Fix the cause and run it again with the same tag.

## When publishing fails partway

From the commit on, nothing is undone automatically. Find out how far it got, then finish by hand. Never delete or move a tag or release that has already been pushed without asking the user.

| State | Finish with |
|---|---|
| Committed, no tag | `git tag -a <tag> -m "Exhibit <tag>"`, then push both as below |
| Tagged locally, not pushed | `git push origin HEAD:main` then `git push origin refs/tags/<tag>` |
| Tag pushed, no release | `gh release create <tag> target/release/bundle/nsis/Exhibit_<tag>_x64-setup.exe target/release/bundle/Exhibit_<tag>_x64_portable.exe --title "Exhibit <tag>" --notes-file target/release/bundle/release-notes-<tag>.md --verify-tag` |
| Release made, an asset is missing | `gh release upload <tag> <file>` |

Check each state with `git log -1 --oneline`, `git ls-remote --tags origin <tag>` and `gh release view <tag>`.

## By hand, step by step

This is what the script automates, for when it needs debugging or changing.

1. **Version.** Set `x.y.z` in the places listed in step 2 of the table. Every `exhibit-* = { version = ... }` pin must match the workspace version, or cargo fails with `failed to select a version for the requirement exhibit-...`.
2. **Checks.** Run the commands from step 3 of the table, the same as `README.md` → Checks.
3. **Build.** In `ui/`, run `bun run tauri build`. In `tauri.conf.json`, `bundle.active: true` with `targets: ["nsis"]` makes the per-user installer. With `active: false`, the build makes only the bare exe. The first build downloads NSIS into Tauri's cache. The exe embeds the UI, and Windows 11 provides WebView2; the installer fetches WebView2 if it is missing.
4. **Smoke test.** Never start the release exe on the owner's real data or keyring. Set these environment variables, then start `target\release\Exhibit.exe`:
   - `EXHIBIT_DATA_DIR` to a new temp dir;
   - `EXHIBIT_KEYRING_SERVICE=exhibit-release-check`;
   - `EXHIBIT_WINDOWED=1`, which also skips the window-state plugin, so the installed app's saved window stays untouched;
   - `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9334`.

   Then run `$env:CDP_PORT=9334; bun scripts/cdp.ts eval "document.body.innerText"` and `... shot <file>`. `scripts/cdp.ts` finds both dev pages (`localhost:1420`) and release pages (`tauri.localhost`).
5. **Publish.** Commit the version files, run `git tag -a <tag> -m "Exhibit <tag>"`, then push `main` and then the tag. Run `gh release create` with the installer and the portable copy of the exe.

## Never

- Never start the smoke test with `-Data real`, the owner's data dir or the `exhibit` or `exhibit-dev` keyrings.
- Never run the installer on this machine as a check unless the user asks. It installs Exhibit for real.
- Never use `--force` on tags or pushes, and never `gh release delete`, unless the user asks.
- Never skip a failing check to get a release out. Report it instead.
