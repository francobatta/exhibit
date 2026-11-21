<#
.SYNOPSIS
Builds, checks, smoke-tests, tags and publishes an Exhibit release for Windows.

.DESCRIPTION
  scripts/release.ps1 -Tag 0.0.2            # the whole release
  scripts/release.ps1 -Tag 0.0.2 -DryRun    # everything but commit, tag, push and GitHub release;
                                            # the version edits are put back at the end

Steps, in order; any failure stops the run:
  1. Preflight: tag is x.y.z and newer than every earlier tag; on main, clean, not behind origin;
     the tag and its release do not exist yet; gh is signed in; no Exhibit is running; Exhibit's
     own TMDB key is at hand (EXHIBIT_TMDB_KEY, or the git-ignored file tmdb-app-key).
  2. Version: writes the tag into Cargo.toml (workspace), every exhibit-* version pin,
     tauri.conf.json and ui/package.json.
  3. Checks: cargo fmt, clippy, cargo test, bun install, bun test, tsc.
  4. Build: `bun run tauri build` (release exe + NSIS installer) with the TMDB key baked in, then
     checks the exe's version.
  5. Smoke test: starts the release exe on an empty temp library with its own keyring name,
     waits for the first-run screen over CDP, checks the build carries its TMDB key, screenshots
     it, closes it, deletes the temp library.
  6. Publish: commits the version, makes an annotated tag, pushes main and the tag, and creates
     the GitHub release with the installer and a portable exe.

Before step 6 a failure puts the version files back. After it starts, nothing is undone: see
.claude/skills/release/SKILL.md for recovery.
#>
param(
    [Parameter(Mandatory = $true)][string]$Tag,
    [switch]$DryRun,
    # Optional trailer lines for the release commit, e.g. a Co-Authored-By line.
    [string]$CommitTrailer = "",
    [int]$CdpPort = 9334
)

$ErrorActionPreference = "Stop"
$repo = Split-Path $PSScriptRoot -Parent
Set-Location $repo

function Write-Step([string]$name) { Write-Host "`n== $name" -ForegroundColor Cyan }

# Runs a native command and stops the release if it fails. Stderr passes through untouched.
function Invoke-Checked([string]$what, [scriptblock]$run) {
    $eap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try { & $run } finally { $ErrorActionPreference = $eap }
    if ($LASTEXITCODE -ne 0) { throw "$what failed (exit code $LASTEXITCODE)." }
}

# Runs a native command for its output; returns $null when it fails.
function Get-Output([scriptblock]$run) {
    $eap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try { $out = & $run 2>$null } finally { $ErrorActionPreference = $eap }
    if ($LASTEXITCODE -ne 0) { return $null }
    return $out
}

# Replaces the first match of $pattern (which captures the text before and after the version)
# in $path, or every match with -All, and fails when nothing matches.
function Set-VersionIn([string]$path, [string]$pattern, [string]$version, [switch]$All) {
    $full = Join-Path $repo $path
    $text = [IO.File]::ReadAllText($full)
    $re = New-Object System.Text.RegularExpressions.Regex($pattern, "Multiline")
    if (-not $re.IsMatch($text)) { throw "No version found in $path. Has its layout changed?" }
    $count = if ($All) { -1 } else { 1 }
    $new = $re.Replace($text, "`${1}$version`${2}", $count)
    [IO.File]::WriteAllText($full, $new, (New-Object System.Text.UTF8Encoding($false)))
    Write-Host "  $path -> $version"
}

# A path dependency on one of the workspace's own crates that also names a version, such as
# exhibit-core = { version = "0.0.1", path = "../exhibit-core" }. Each must follow the workspace
# version, or cargo cannot select it. Found in every crate, so a new crate needs no edit here.
$pinPattern = '(exhibit-[\w-]+\s*=\s*\{\s*version\s*=\s*")[^"]+(")'
$pinnedManifests = @(Get-ChildItem (Join-Path $repo "crates") -Directory |
    ForEach-Object { "crates/$($_.Name)/Cargo.toml" } |
    Where-Object { (Test-Path (Join-Path $repo $_)) -and
        ([IO.File]::ReadAllText((Join-Path $repo $_)) -match $pinPattern) })

$versionFiles = @(
    "Cargo.toml",
    "Cargo.lock",
    "crates/exhibit-app/tauri.conf.json",
    "ui/package.json"
) + $pinnedManifests

# ---------------------------------------------------------------- 1. Preflight
Write-Step "Preflight"

if ($Tag -notmatch '^\d+\.\d+\.\d+$') {
    throw "Tag '$Tag' is not x.y.z (for example 0.0.2). Tags are bare versions, without a leading v."
}
if (-not (Get-Output { gh auth status })) { throw "gh is not signed in. Run: gh auth login" }

$branch = (Get-Output { git rev-parse --abbrev-ref HEAD })
if ($branch -ne "main") { throw "On branch '$branch'. Releases are made from main." }
if (Get-Output { git status --porcelain }) { throw "The working tree has changes. Commit or stash them first." }

Invoke-Checked "git fetch" { git fetch --quiet --tags origin }
$behind = [int](Get-Output { git rev-list --count HEAD..origin/main })
if ($behind -gt 0) { throw "main is $behind commit(s) behind origin/main. Pull first." }
$ahead = Get-Output { git log --oneline origin/main..HEAD }
if ($ahead) {
    Write-Host "  These local commits will be pushed with the release:"
    $ahead | ForEach-Object { Write-Host "    $_" }
}

if (Get-Output { git rev-parse -q --verify "refs/tags/$Tag" }) { throw "Tag $Tag already exists locally." }
if (Get-Output { git ls-remote --tags origin "refs/tags/$Tag" }) { throw "Tag $Tag already exists on origin." }
if (Get-Output { gh release view $Tag --json tagName }) { throw "A GitHub release named $Tag already exists." }

$earlier = @(Get-Output { git tag --list } | Where-Object { $_ -match '^\d+\.\d+\.\d+$' } |
    Sort-Object { [version]$_ })
$previous = if ($earlier.Count -gt 0) { $earlier[-1] } else { $null }
if ($previous -and ([version]$Tag -le [version]$previous)) {
    throw "Tag $Tag is not newer than the latest tag, $previous."
}
Write-Host "  Previous tag: $(if ($previous) { $previous } else { 'none' })"

# Exhibit's own TMDB key is baked into the build (crates/exhibit-core/build.rs), so nobody has to
# find one. It never goes into git: it comes from EXHIBIT_TMDB_KEY or the git-ignored file.
$tmdbKey = $env:EXHIBIT_TMDB_KEY
$tmdbKeyFile = Join-Path $repo "tmdb-app-key"
if (-not $tmdbKey -and (Test-Path $tmdbKeyFile)) { $tmdbKey = (Get-Content -Raw $tmdbKeyFile).Trim() }
if (-not $tmdbKey) {
    throw "No TMDB key to build in. Put Exhibit's key in tmdb-app-key at the repo root (git-ignored), " +
        "or set EXHIBIT_TMDB_KEY. A release without one would ask every owner for a key."
}
Write-Host "  TMDB key to build in: found"

# Exhibit is single-instance: a second one hands itself to the first and exits, so the smoke
# test needs none running. Checked here and again just before the smoke test, after the build.
function Assert-NoExhibitRunning {
    $running = Get-Process -Name "exhibit-app", "Exhibit" -ErrorAction SilentlyContinue
    if ($running) {
        $where = ($running | ForEach-Object { "$($_.Id) $($_.Path)" }) -join "; "
        throw "Exhibit is running ($where). Close it, or run scripts/dev.ps1 -Stop for the dev app. " +
            "A second Exhibit hands itself to the first, so the smoke test would not see the new build."
    }
}
Assert-NoExhibitRunning
try {
    Invoke-WebRequest -UseBasicParsing -TimeoutSec 2 "http://127.0.0.1:$CdpPort/json/version" | Out-Null
    throw "Port $CdpPort already answers. Close what is using it, or pass -CdpPort."
} catch [System.Net.WebException] { }

# From here on, a failure before publishing puts the version files back.
$published = $false
try {
    # ------------------------------------------------------------ 2. Version
    Write-Step "Version $Tag"
    Set-VersionIn "Cargo.toml" '(\[workspace\.package\][\s\S]*?^version\s*=\s*")[^"]+(")' $Tag
    foreach ($manifest in $pinnedManifests) { Set-VersionIn $manifest $pinPattern $Tag -All }
    Set-VersionIn "crates/exhibit-app/tauri.conf.json" '(^\s*"version"\s*:\s*")[^"]+(")' $Tag
    Set-VersionIn "ui/package.json" '(^\s*"version"\s*:\s*")[^"]+(")' $Tag

    # ------------------------------------------------------------ 3. Checks
    Write-Step "Checks"
    Invoke-Checked "cargo fmt" { cargo fmt --all --check }
    Invoke-Checked "cargo clippy" { cargo clippy --workspace --all-targets -- -D warnings }
    Invoke-Checked "cargo test" { cargo test --workspace }
    Push-Location ui
    try {
        Invoke-Checked "bun install" { bun install --frozen-lockfile }
        Invoke-Checked "bun test" { bun test --parallel }
        Invoke-Checked "tsc" { bunx --bun tsc --noEmit }
    } finally { Pop-Location }

    # ------------------------------------------------------------ 4. Build
    Write-Step "Build"
    Push-Location ui
    # Set for the build only, so the key is baked in even over a build made without one.
    $savedKey = $env:EXHIBIT_TMDB_KEY
    $env:EXHIBIT_TMDB_KEY = $tmdbKey
    try { Invoke-Checked "tauri build" { bun run tauri build } } finally {
        $env:EXHIBIT_TMDB_KEY = $savedKey
        Pop-Location
    }

    $exe = Join-Path $repo "target\release\exhibit-app.exe"
    $setup = Join-Path $repo "target\release\bundle\nsis\Exhibit_${Tag}_x64-setup.exe"
    if (-not (Test-Path $setup)) { throw "The installer is missing: $setup" }
    $built = (Get-Item $exe).VersionInfo.ProductVersion
    if ($built -ne $Tag) { throw "The exe reports version '$built', not $Tag." }
    Write-Host "  $exe ($built)"
    Write-Host "  $setup"

    # ------------------------------------------------------------ 5. Smoke test
    Write-Step "Smoke test"
    Assert-NoExhibitRunning
    $data = Join-Path $env:TEMP "exhibit-release-smoke-$Tag-$(Get-Date -Format yyyyMMddHHmmss)"
    New-Item -ItemType Directory -Force $data | Out-Null
    $shot = Join-Path $env:TEMP "exhibit-release-$Tag.jpg"
    $envNames = "EXHIBIT_DATA_DIR", "EXHIBIT_KEYRING_SERVICE", "EXHIBIT_WINDOWED",
        "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", "CDP_PORT"
    $savedEnv = @{}
    foreach ($n in $envNames) { $savedEnv[$n] = [Environment]::GetEnvironmentVariable($n) }
    $env:EXHIBIT_DATA_DIR = $data                          # empty library: never the owner's
    $env:EXHIBIT_KEYRING_SERVICE = "exhibit-release-check" # no keys of the owner's: only the build's own
    $env:EXHIBIT_WINDOWED = "1"                            # also skips saving window state
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$CdpPort"
    $env:CDP_PORT = "$CdpPort"
    $app = Start-Process -FilePath $exe -PassThru
    try {
        $deadline = (Get-Date).AddSeconds(60)
        $page = $null
        while (-not $page -and (Get-Date) -lt $deadline) {
            try {
                $page = Invoke-RestMethod -TimeoutSec 2 "http://127.0.0.1:$CdpPort/json" |
                    Where-Object { $_.type -eq "page" -and $_.url -like "http://tauri.localhost*" }
            } catch { }
            if ($app.HasExited) {
                throw "The release exe exited at start (exit code $($app.ExitCode)). With code 0, an " +
                    "Exhibit opened since the check most likely took over: close it and run again."
            }
            if (-not $page) { Start-Sleep -Milliseconds 500 }
        }
        if (-not $page) { throw "The release window did not open within 60 s." }

        # First run is on screen once its text is there and the intro (the E) has finished and gone.
        $shown = "!document.querySelector('[data-slot=intro]') && document.body.innerText.includes('Welcome to Exhibit')"
        $deadline = (Get-Date).AddSeconds(30)
        $ok = $false
        while (-not $ok -and (Get-Date) -lt $deadline) {
            $ok = "$(Get-Output { bun scripts/cdp.ts eval $shown })".Trim() -eq "true"
            if (-not $ok) { Start-Sleep -Milliseconds 500 }
        }
        if (-not $ok) {
            $text = "$(Get-Output { bun scripts/cdp.ts eval 'document.body.innerText' })"
            throw "The release window never showed first run with the intro finished. Its text: $text"
        }
        # The build carries its own TMDB key: first run asks for none.
        $builtIn = "window.__TAURI_INTERNALS__.invoke('status').then((s) => s.tmdb_builtin)"
        if ("$(Get-Output { bun scripts/cdp.ts eval $builtIn })".Trim() -ne "true") {
            throw "The release exe carries no TMDB key of its own."
        }
        Write-Host "  The build carries its own TMDB key."
        Invoke-Checked "screenshot" { bun scripts/cdp.ts shot $shot | Out-Null }
        Write-Host "  First run showed. Screenshot: $shot"
    } finally {
        if (-not $app.HasExited) { Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue }
        $app.WaitForExit(10000) | Out-Null
        foreach ($n in $envNames) { [Environment]::SetEnvironmentVariable($n, $savedEnv[$n]) }
        for ($i = 0; $i -lt 10 -and (Test-Path $data); $i++) {
            try { Remove-Item -Recurse -Force $data -Confirm:$false } catch { Start-Sleep -Milliseconds 500 }
        }
        if (Test-Path $data) { Write-Warning "Could not delete the smoke library: $data" }
    }

    if ($DryRun) {
        Write-Step "Dry run: stopping before commit, tag, push and release"
        return
    }

    # ------------------------------------------------------------ 6. Publish
    Write-Step "Publish"
    $bundle = Join-Path $repo "target\release\bundle"
    $portable = Join-Path $bundle "Exhibit_${Tag}_x64_portable.exe"
    Copy-Item $exe $portable -Force

    $range = if ($previous) { "$previous..HEAD" } else { "HEAD" }
    $changes = @(Get-Output { git log --no-merges --pretty="- %s" $range })
    $notes = Join-Path $bundle "release-notes-$Tag.md"
    $body = @"
Exhibit $Tag for Windows 11 (x64).

**Downloads**
- ``Exhibit_${Tag}_x64-setup.exe``: installer. It installs for the current user, needs no admin rights, and adds a Start menu entry.
- ``Exhibit_${Tag}_x64_portable.exe``: a single exe that runs as is.

**Needs**
- WebView2, which ships with Windows 11. If it is missing, the installer fetches it.
- mpv and ffmpeg. First run can download both in one press.
- Nothing else: Exhibit brings its own TMDB key. You can use your own in Settings.

The builds are not code-signed, so SmartScreen may warn on first launch: choose **More info**, then **Run anyway**.
"@
    if ($previous -and $changes.Count -gt 0) {
        $body += "`n**Changes since $previous**`n" + ($changes -join "`n") + "`n"
    }
    [IO.File]::WriteAllText($notes, $body, (New-Object System.Text.UTF8Encoding($false)))

    $message = "release: $Tag"
    if ($CommitTrailer) { $message += "`n`n$CommitTrailer" }
    $msgFile = Join-Path $bundle "release-commit-$Tag.txt"
    [IO.File]::WriteAllText($msgFile, $message, (New-Object System.Text.UTF8Encoding($false)))

    Invoke-Checked "git add" { git add -- $versionFiles }
    Invoke-Checked "git commit" { git commit --quiet -F $msgFile }
    $published = $true  # from the commit on, nothing is rolled back
    Invoke-Checked "git tag" { git tag -a $Tag -m "Exhibit $Tag" }
    Invoke-Checked "git push main" { git push --quiet origin HEAD:main }
    Invoke-Checked "git push tag" { git push --quiet origin "refs/tags/$Tag" }
    Invoke-Checked "gh release create" {
        gh release create $Tag $setup $portable --title "Exhibit $Tag" --notes-file $notes --verify-tag
    }
    $url = Get-Output { gh release view $Tag --json url --jq .url }
    Write-Step "Released $Tag"
    Write-Host "  $url"
    Write-Host "  Assets: $(Split-Path $setup -Leaf), $(Split-Path $portable -Leaf)"
} finally {
    if (-not $published) {
        Invoke-Checked "restore version files" { git checkout HEAD -- $versionFiles }
        Write-Host "  Version files put back."
    }
}
