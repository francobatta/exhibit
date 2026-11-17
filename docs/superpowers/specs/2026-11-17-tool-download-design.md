# Tool download: mpv and ffmpeg in one press

Date: 2026-11-17. Amends the main spec (`2026-11-14-exhibit-design.md`), sections 11 and 13.

## Why

A friend installs Exhibit on a new home-theatre PC. They know nothing about media players or codecs. Today, without mpv on the machine, Exhibit can only say "Not found" and offer a file browser. They need one press that makes Exhibit able to play.

Success: on a clean Windows PC, first run offers **Download mpv and ffmpeg**, the owner presses A, and a minute later Exhibit plays a film.

## Decisions (the owner's)

1. **ffprobe is no longer a tool.** It is removed from Exhibit everywhere. mpv reads technical details (duration, resolution, HDR, tracks), as it already does when ffprobe is missing. Exhibit's tools are **mpv** and **ffmpeg**.
2. **One download gets both.** A single action downloads mpv and ffmpeg together, from the same release.
3. **Source: shinchiro's builds** (`github.com/shinchiro/mpv-winbuild-cmake`), the first Windows build mpv.io lists. They are unofficial, so Exhibit says so before you download. Downloading is offered anyway.
4. **The build matches the machine.** Exhibit picks the asset for the OS and the CPU.
5. **The latest wins.** Each download asks for the latest release. Asset names change with every release, so they are matched by pattern and never hard-coded. When there has been more than one download, Exhibit uses the newest.
6. **Download is an action, not a mode.** Each tool is still found automatically or set by hand. A downloaded copy is simply one more place Exhibit looks, and the first. Settings gains no new options.
7. **Simple status.** For each tool, the owner sees where it came from, its version and its path, in plain words.
8. Downloading on the owner's request is not bundling. Spec section 11 ("Exhibit bundles no media tools") still holds; it gains this exception.

## Anti-goals

- A per-tool choice of mode or source. That is the growing pile of settings spec section 4 rules out.
- Checking for updates in the background. Nothing runs that the owner did not ask for.
- A confirm dialog before downloading. The disclaimer is a line beside the button, read before pressing.
- Replacing a path the owner set by hand.

## Approaches considered

| | Trade-off | |
|---|---|---|
| A. mpv's own GitHub CI build, plus FFmpeg from BtbN's `.zip` builds | mpv official; two sources; mpv.io says the CI builds "may be missing certain features" | Not chosen |
| **B. shinchiro for both** | One source, the build the owner already uses; unofficial; `.7z` only; no ffprobe | **Chosen** |
| C. Owner picks a source per tool | Most flexible; more settings and a choice the friend can't make | Rejected |

shinchiro ships no ffprobe; zhongfly doesn't either (checked 2026-11-17). Rather than add a second source just for ffprobe, the owner dropped ffprobe.

## Behaviour

### Where downloads go

- Tools folder: `%LOCALAPPDATA%\exhibit\tools`. Not `%APPDATA%`, because roaming profiles would sync ~150 MB. Not the install folder, because an update replaces it.
- When `EXHIBIT_DATA_DIR` is set (dev, tests), the tools folder is `<data dir>\tools`, so dev and tests never touch the real one.
- Each download goes to its own folder named by release tag and build, e.g. `tools\20261111-x86_64-v3\`, holding `mpv.exe`, `mpv.com`, its DLLs and `ffmpeg.exe`. It is unpacked into `tools\.partial-…\` first and renamed into place only once complete. A half-finished download is never used.

### Which copy is used

For each tool, in order:

1. A path set by hand (unchanged: never replaced).
2. The newest downloaded copy: the tools folder whose tag (`YYYYMMDD`) is highest and that holds the tool.
3. Detection as today: PATH, package-manager folders, usual install folders, beside a found tool.

Nothing about the download is stored in the database. The folder is the record.

After a successful download, older download folders are deleted. If one can't be (mpv is playing from it), it is left and removed at the next start.

### Picking the asset

From `GET https://api.github.com/repos/shinchiro/mpv-winbuild-cmake/releases/latest`:

- Arch: `aarch64` on ARM64; on x86-64, `x86_64-v3` when the CPU has the x86-64-v3 set (AVX2, BMI1, BMI2, FMA, F16C, LZCNT, MOVBE), else `x86_64`; `i686` on 32-bit.
- mpv: `^mpv-{arch}-\d{8}-git-[0-9a-f]+\.7z$` (this excludes `mpv-dev-*` and `mpv-debug-*`, and `x86_64` never matches `x86_64-v3`).
- ffmpeg: `^ffmpeg-{arch}-git-[0-9a-f]+\.7z$`.
- Both assets must be in the release. If either is missing, the download fails with a plain message and nothing changes.
- Check each file against the asset's `size`, and against its `digest` (sha256) when the API gives one.
- From each archive, unpack only what Exhibit runs: for mpv, the top-level `mpv.exe`, `mpv.com` and DLLs (skip `doc/`, `installer/`, the `.bat` updaters); for ffmpeg, `ffmpeg.exe`.
- If the newest downloaded folder already has this tag and arch, say "You already have the latest" and download nothing.

Download is offered only on Windows; shinchiro builds nothing else.

### What the owner sees

**Each tool, in Settings and in first run's tools step: two lines.**

```
mpv
Downloaded by Exhibit · version 0.42.0-dev · 11 Nov 2026
C:\Users\…\AppData\Local\exhibit\tools\20261111-x86_64-v3\mpv.exe

ffmpeg
Found on this PC · version N-128915
C:\ffmpeg\bin\ffmpeg.exe
```

- The first line says where it came from: **Downloaded by Exhibit**, **Found on this PC**, or **Set by you**. Then the version, and for a download its build date.
- The second line, quieter, is the path.
- Missing: **Not found**, then what it costs (today's consequence lines, kept).

**One download control, under the tools.**

```
[ Download mpv and ffmpeg ]
Built by shinchiro, an independent volunteer, not by the mpv or FFmpeg projects.
mpv's own website lists these builds first.
```

- Once a download exists, the button reads **Get the latest**.
- Per tool, as today: **Set path**, and when a path is set by hand, a plain button that undoes it (labelled for what takes over: "Use downloaded" or "Use detected").

**While downloading,** the button's place shows progress in words: "Downloading · 34 of 63 MB", then "Unpacking". Focus stays on it. Leaving the screen does not stop the download; coming back shows where it is. When it finishes, both tool lines update in place.

**Failure** leaves any earlier copy in use and says what happened and what to do, e.g. "Couldn't reach GitHub. Check the internet connection, then try again." Same for disk full, a file that fails its check, and a release that is missing a build for this PC.

### First run

The tools step appears when mpv or ffmpeg is missing (it now checks two tools, not three). Its main action, focused, is **Download mpv and ffmpeg**, with the disclaimer beneath. Then **Set path** for each missing tool, then **Skip**. While downloading, the owner can go on to the next step; the download carries on behind it, like the folder scan.

### Agent-first

The download is a library operation like any other (spec section 12): `download_tools` on `exhibit_core::Library`, reporting progress through a callback; a CLI command (`exhibit tools download`); and a Tauri command that forwards progress as events. Tool status gains `source: "manual" | "downloaded" | "detected"`.

## Removing ffprobe

- Core: `Tool::Ffprobe`, the ffprobe branch of probing, `parse_ffprobe`, its fixtures and tests. Probing becomes mpv only; "no tool can probe" becomes "mpv is missing".
- Database: delete any `tool.ffprobe` setting in a migration.
- CLI, UI (`api.ts`, `tools.ts`, Settings, first run and their tests), README, and the main spec (sections 11, 13, and the tool list call).
- **Known cost:** mpv probes more slowly than ffprobe, and it reports Dolby Vision plus HDR10 as HDR10 (an existing small debt). Both become the only path.

## Spec amendments

- Section 11: tools are mpv and ffmpeg. Exhibit bundles none, and downloads both on request from shinchiro's builds. Detection order: by hand, downloaded, detected. Technical details come from mpv.
- Section 13, Settings: "tools (mpv, ffmpeg: detected or downloaded, each overridable)". First run: "confirm the detected tools, offering to download what was not found".

## Calls made in this design (Claude's, for the owner to check)

1. Tools folder `%LOCALAPPDATA%\exhibit\tools`, or `<data dir>\tools` under `EXHIBIT_DATA_DIR`.
2. A download beats a detected copy but never a hand-set path.
3. ffmpeg stays optional at the code level: without it, thumbnails still come from mpv, as now. Removing that fallback would add a failure state for no gain.
4. No cancel button; leaving the screen doesn't stop a download.
5. The disclaimer is a line beside the button, not a dialog.
6. Only the files Exhibit runs are unpacked.

## Left to the builder

- The `.7z` crate (e.g. `sevenz-rust2`) and how progress events reach the UI.
- How "Downloading · 34 of 63 MB" is laid out and animated, within the UI rules.
- The exact wording of each failure, following the "what happened, what to do" rule.
