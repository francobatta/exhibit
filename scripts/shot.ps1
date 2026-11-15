<#
.SYNOPSIS
Saves a PNG of the Exhibit window's client area, optionally sending keys first.

.EXAMPLE
scripts/shot.ps1 -Out C:\temp\home.png -Keys "{RIGHT}{RIGHT}{ENTER}" -WaitMs 800

The window is the visible top-level window titled exactly "Exhibit". Its pixels are read with
PrintWindow (PW_RENDERFULLCONTENT), which works while the window is partly covered; when that
yields nothing the window is brought forward and its area copied from the screen. A capture
wider than -MaxWidth (default 1920) is scaled down to that width, keeping its proportions.
#>
param(
    [Parameter(Mandatory = $true)][string]$Out,
    [string]$Keys = "",
    [int]$WaitMs = 800,
    [int]$MaxWidth = 1920
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms

Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class ExhibitShot {
    public delegate bool EnumProc(IntPtr hwnd, IntPtr lParam);
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)]
    public struct POINT { public int X, Y; }

    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lParam);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int max);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hwnd, int cmd);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hwnd, out RECT r);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd, ref POINT p);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr value);

    public static IntPtr Find(string title) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((h, l) => {
            if (!IsWindowVisible(h)) return true;
            var sb = new StringBuilder(256);
            GetWindowText(h, sb, sb.Capacity);
            if (sb.ToString() == title) { found = h; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }

    public static void Forward(IntPtr hwnd) {
        if (IsIconic(hwnd)) ShowWindow(hwnd, 9); // SW_RESTORE
        if (GetForegroundWindow() == hwnd) return;
        // A tap of Alt lets a background process take the foreground. Only when needed: in a window
        // that is already in front, a lone Alt enters menu mode and the keys after it are lost.
        keybd_event(0x12, 0, 0, UIntPtr.Zero);
        keybd_event(0x12, 0, 2, UIntPtr.Zero);
        SetForegroundWindow(hwnd);
    }
}
"@

# Physical pixels, not scaled ones (DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).
[void][ExhibitShot]::SetProcessDpiAwarenessContext([IntPtr]::new(-4))

$hwnd = [ExhibitShot]::Find("Exhibit")
if ($hwnd -eq [IntPtr]::Zero) {
    Write-Error "No visible window titled 'Exhibit' was found. Start the app first." -ErrorAction Continue
    exit 1
}

[ExhibitShot]::Forward($hwnd)
# WebView2 takes keyboard focus a moment after the window comes forward; keys sent sooner are lost.
Start-Sleep -Milliseconds 500
if ($Keys -ne "") {
    [System.Windows.Forms.SendKeys]::SendWait($Keys)
}
Start-Sleep -Milliseconds $WaitMs

$rect = New-Object ExhibitShot+RECT
[void][ExhibitShot]::GetClientRect($hwnd, [ref]$rect)
$w = $rect.Right - $rect.Left
$h = $rect.Bottom - $rect.Top
if ($w -le 0 -or $h -le 0) {
    Write-Error "The Exhibit window has no visible area (minimised?)." -ErrorAction Continue
    exit 1
}

function Test-Blank([System.Drawing.Bitmap]$bmp) {
    # PrintWindow yields an all-black image when it cannot render the window.
    foreach ($fx in 0.25, 0.5, 0.75) {
        foreach ($fy in 0.25, 0.5, 0.75) {
            $c = $bmp.GetPixel([int]($bmp.Width * $fx), [int]($bmp.Height * $fy))
            if ($c.R -ne 0 -or $c.G -ne 0 -or $c.B -ne 0) { return $false }
        }
    }
    return $true
}

$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
# PW_CLIENTONLY (1) | PW_RENDERFULLCONTENT (2)
$ok = [ExhibitShot]::PrintWindow($hwnd, $hdc, 3)
$g.ReleaseHdc($hdc)

if (-not $ok -or (Test-Blank $bmp)) {
    [ExhibitShot]::Forward($hwnd)
    Start-Sleep -Milliseconds 300
    $origin = New-Object ExhibitShot+POINT
    [void][ExhibitShot]::ClientToScreen($hwnd, [ref]$origin)
    $g.CopyFromScreen($origin.X, $origin.Y, 0, 0, (New-Object System.Drawing.Size $w, $h))
}
$g.Dispose()

$Out = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Out)
$dir = Split-Path -Parent $Out
if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Force $dir | Out-Null }
if ($MaxWidth -gt 0 -and $bmp.Width -gt $MaxWidth) {
    $sh = [int][Math]::Round($bmp.Height * $MaxWidth / $bmp.Width)
    $small = New-Object System.Drawing.Bitmap $MaxWidth, $sh
    $sg = [System.Drawing.Graphics]::FromImage($small)
    $sg.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $sg.DrawImage($bmp, 0, 0, $MaxWidth, $sh)
    $sg.Dispose()
    $bmp.Dispose()
    $bmp = $small
}
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Output $Out
