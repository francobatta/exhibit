<#
.SYNOPSIS
Runs Exhibit in dev mode (Vite + Tauri, hot reload), or stops a dev run.

.DESCRIPTION
-Data picks the library the window opens:
  dev    .superpowers/dev-data: films from D:\Movies, a fake Severance show, six files in
         Needs review. Keyring service exhibit-dev, which holds the TMDB key. (default)
  fresh  A new empty data dir in %TEMP% and a throwaway keyring: first run, no TMDB key.
  real   The owner's real data dir and keyring. Only when asked for.
  <path> Any other data dir, with the throwaway keyring unless -Keyring is given.

The window opens at 1600x900 unless -Fullscreen is given. -Cdp exposes WebView2's DevTools
protocol on port 9333, for scripted input and screenshots. -Stop ends every dev process that
belongs to this repo: Vite, tauri dev and exhibit-app.

.EXAMPLE
scripts/dev.ps1
scripts/dev.ps1 -Data fresh
scripts/dev.ps1 -Cdp
scripts/dev.ps1 -Stop
#>
param(
    [string]$Data = "dev",
    [string]$Keyring = "",
    [switch]$Fullscreen,
    [switch]$Cdp,
    [int]$CdpPort = 9333,
    [switch]$Stop
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot

if ($Stop) {
    $mine = Get-CimInstance Win32_Process | Where-Object {
        ($_.Name -in "exhibit-app.exe", "Exhibit.exe") -or
        ($_.CommandLine -and $_.CommandLine.Contains($repo) -and
            ($_.CommandLine -match "vite|tauri"))
    }
    foreach ($p in $mine) {
        try { Stop-Process -Id $p.ProcessId -Force -ErrorAction Stop } catch {}
    }
    Write-Host "Stopped $(@($mine).Count) dev process(es)."
    return
}

foreach ($dir in "$env:USERPROFILE\.cargo\bin", "$env:USERPROFILE\.bun\bin") {
    if (($env:PATH -split ";") -notcontains $dir) { $env:PATH = "$dir;$env:PATH" }
}

switch ($Data) {
    "dev" {
        $env:EXHIBIT_DATA_DIR = Join-Path $repo ".superpowers\dev-data"
        $service = "exhibit-dev"
    }
    "fresh" {
        $env:EXHIBIT_DATA_DIR = Join-Path $env:TEMP "exhibit-fresh-$(Get-Date -Format yyyyMMdd-HHmmss)"
        New-Item -ItemType Directory -Force $env:EXHIBIT_DATA_DIR | Out-Null
        $service = "exhibit-throwaway"
    }
    "real" {
        Remove-Item Env:EXHIBIT_DATA_DIR -ErrorAction SilentlyContinue
        $service = ""
    }
    default {
        $env:EXHIBIT_DATA_DIR = (Resolve-Path $Data).Path
        $service = "exhibit-throwaway"
    }
}
if ($Keyring) { $service = $Keyring }
if ($service) {
    $env:EXHIBIT_KEYRING_SERVICE = $service
} else {
    Remove-Item Env:EXHIBIT_KEYRING_SERVICE -ErrorAction SilentlyContinue
}

if ($Fullscreen) {
    Remove-Item Env:EXHIBIT_WINDOWED -ErrorAction SilentlyContinue
} else {
    $env:EXHIBIT_WINDOWED = "1"
}
if ($Cdp) {
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$CdpPort"
}

$dataLine = if ($env:EXHIBIT_DATA_DIR) { $env:EXHIBIT_DATA_DIR } else { "real data dir" }
$keyLine = if ($service) { $service } else { "default" }
Write-Host "Exhibit dev: data $dataLine, keyring $keyLine$(if ($Cdp) { ", CDP on $CdpPort" })"

Push-Location (Join-Path $repo "ui")
try {
    bun run tauri dev
} finally {
    Pop-Location
}
