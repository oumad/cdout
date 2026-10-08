<#
.SYNOPSIS
  End-to-end smoke test of the native cdout app on Windows.

.DESCRIPTION
  Builds cdout-native with the scripted fake LLM, launches it against an
  isolated temp data dir and a hotkey that cannot clash with an installed
  cdout, then drives it with synthesized key presses and Windows UI
  Automation (egui exposes its widgets through AccessKit). Every step is
  checked against the app's log; failures print the log tail and exit 1.

  Screenshots of the app's own windows (never the whole screen) are written
  to -OutDir for visual inspection.

.PARAMETER Scenarios
  Any of: text, command, question, stop, markdown. Default: all.

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/native-smoke.ps1
#>
param(
    [string[]]$Scenarios = @("text", "command", "question", "stop", "markdown"),
    [string]$OutDir = "",
    [switch]$NoBuild
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$tauriDir = Join-Path $repo "src-tauri"
$exe = Join-Path $tauriDir "target\debug\cdout-native.exe"
if (-not $OutDir) { $OutDir = Join-Path $env:TEMP "cdout-native-smoke" }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$Hotkey = "Ctrl+Alt+Shift+F9"

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class Smoke {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
  public struct RECT { public int L, T, R, B; }
  public class Win { public IntPtr H; public bool Visible; public int L, T, W, Hgt; }
  public static List<Win> Windows(uint pid) {
    var list = new List<Win>();
    EnumWindows((h, l) => {
      uint p; GetWindowThreadProcessId(h, out p);
      if (p == pid) {
        RECT r; GetWindowRect(h, out r);
        list.Add(new Win { H = h, Visible = IsWindowVisible(h), L = r.L, T = r.T, W = r.R - r.L, Hgt = r.B - r.T });
      }
      return true;
    }, IntPtr.Zero);
    return list;
  }
  public static void Key(byte vk, bool up) { keybd_event(vk, 0, up ? 2u : 0u, UIntPtr.Zero); }
}
"@
[void][Smoke]::SetProcessDPIAware()

$script:failures = @()
$script:proc = $null
$script:log = ""

function Fail($msg) {
    Write-Host "  FAIL: $msg" -ForegroundColor Red
    $script:failures += $msg
}

function Pass($msg) { Write-Host "  ok: $msg" -ForegroundColor Green }

function Wait-Log([string]$pattern, [int]$timeoutMs = 10000, [int]$after = 0) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    while ($sw.ElapsedMilliseconds -lt $timeoutMs) {
        if (Test-Path $script:log) {
            $lines = @(Get-Content $script:log -ErrorAction SilentlyContinue)
            for ($i = $after; $i -lt $lines.Count; $i++) {
                if ($lines[$i] -match $pattern) { return $i + 1 }
            }
        }
        if ($script:proc.HasExited) { return -1 }
        Start-Sleep -Milliseconds 100
    }
    return -1
}

function Expect([string]$pattern, [string]$what, [int]$timeoutMs = 10000) {
    $n = Wait-Log $pattern $timeoutMs $script:cursor
    if ($n -lt 0) { Fail "$what (no '$pattern' in log within ${timeoutMs}ms)"; return $false }
    $script:cursor = $n
    Pass $what
    return $true
}

function Send-Chord {
    # Ctrl+Alt+Shift+F9
    foreach ($vk in 0x11, 0x12, 0x10, 0x78) { [Smoke]::Key($vk, $false) }
    Start-Sleep -Milliseconds 60
    foreach ($vk in 0x78, 0x10, 0x12, 0x11) { [Smoke]::Key($vk, $true) }
}

function Send-Escape { [Smoke]::Key(0x1B, $false); Start-Sleep -Milliseconds 40; [Smoke]::Key(0x1B, $true) }

function Visible-Windows {
    @([Smoke]::Windows([uint32]$script:proc.Id) | Where-Object { $_.Visible -and $_.W -gt 100 -and $_.Hgt -gt 100 })
}

function Snap([string]$name, $win) {
    if (-not $win) { Fail "no window to capture for $name"; return }
    $b = New-Object System.Drawing.Bitmap $win.W, $win.Hgt
    $g = [System.Drawing.Graphics]::FromImage($b)
    $g.CopyFromScreen($win.L, $win.T, 0, 0, $b.Size)
    $path = Join-Path $OutDir $name
    $b.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $b.Dispose()
    Write-Host "  screenshot: $path"
}

# Pixels of a small screen region, for before/after comparisons.
function Grab-Pixels([int]$x, [int]$y, [int]$w, [int]$h) {
    $b = New-Object System.Drawing.Bitmap $w, $h
    $g = [System.Drawing.Graphics]::FromImage($b)
    $g.CopyFromScreen($x, $y, 0, 0, $b.Size)
    $px = @()
    for ($j = 0; $j -lt $h; $j++) { for ($i = 0; $i -lt $w; $i++) { $px += $b.GetPixel($i, $j).ToArgb() } }
    $g.Dispose(); $b.Dispose()
    return ,$px
}

function Spotlight-Window { Visible-Windows | Where-Object { $_.W -lt 900 } | Select-Object -First 1 }
function Main-Window { Visible-Windows | Where-Object { $_.W -ge 900 } | Select-Object -First 1 }

# Find a UI element of our process by accessible name (wildcards allowed),
# retrying while AccessKit builds its tree.
function Find-Element([string]$namePattern, [int]$timeoutMs = 6000) {
    $AE = [System.Windows.Automation.AutomationElement]
    $cond = New-Object System.Windows.Automation.PropertyCondition($AE::ProcessIdProperty, $script:proc.Id)
    $sw = [Diagnostics.Stopwatch]::StartNew()
    while ($sw.ElapsedMilliseconds -lt $timeoutMs) {
        $wins = $AE::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, $cond)
        foreach ($w in $wins) {
            $all = $w.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
            foreach ($e in $all) {
                # -clike: case-sensitive, so "Answer" never matches "your own answer".
                if ($e.Current.Name -clike $namePattern) { return $e }
            }
        }
        Start-Sleep -Milliseconds 250
    }
    return $null
}

function Click-Element([string]$namePattern, [string]$what) {
    $e = Find-Element $namePattern
    if (-not $e) { Fail "$what (no element named '$namePattern')"; return $false }
    $invoke = $null
    if ($e.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$invoke)) {
        $invoke.Invoke()
    } else {
        $sel = $null
        if ($e.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$sel)) {
            $sel.Select()
        } else {
            Fail "$what (element '$namePattern' has no invoke pattern)"; return $false
        }
    }
    Pass $what
    return $true
}

function Start-App([string]$scenario, [hashtable]$config) {
    $data = Join-Path $env:TEMP ("cdout-smoke-" + $scenario + "-" + [guid]::NewGuid().ToString("N").Substring(0, 6))
    New-Item -ItemType Directory -Force -Path $data | Out-Null
    if ($config) {
        # No BOM: serde_json rejects one, and PowerShell 5's UTF8 adds it.
        [IO.File]::WriteAllText((Join-Path $data "cdout_config.json"), ($config | ConvertTo-Json))
    }
    $env:CDOUT_DATA_DIR = $data
    $env:CDOUT_HOTKEY = $Hotkey
    $env:CDOUT_FAKE_LLM = $scenario
    $env:CDOUT_FAKE_LLM_DELAY_MS = "40"
    $script:log = Join-Path $data "logs\cdout-native.log"
    $script:cursor = 0
    $script:proc = Start-Process -FilePath $exe -PassThru -RedirectStandardError (Join-Path $data "stderr.txt") -RedirectStandardOutput (Join-Path $data "stdout.txt")
    Write-Host "  pid=$($script:proc.Id) data=$data"
    return $data
}

function Stop-App($data) {
    if ($script:proc -and -not $script:proc.HasExited) {
        Stop-Process -Id $script:proc.Id -Force
        $script:proc.WaitForExit(3000) | Out-Null
    }
    $panics = Select-String -Path $script:log -Pattern "app.panic" -ErrorAction SilentlyContinue
    if ($panics) { Fail "panic logged: $($panics[0].Line)" }
    if ($script:failures.Count -gt 0) {
        Write-Host "  --- log tail ---"
        Get-Content $script:log -Tail 25 -ErrorAction SilentlyContinue | ForEach-Object { Write-Host "  $_" }
        Write-Host "  --- stderr tail ---"
        Get-Content (Join-Path $data "stderr.txt") -Tail 10 -ErrorAction SilentlyContinue | ForEach-Object { Write-Host "  $_" }
    }
}

function Type-Text([string]$text) {
    [System.Windows.Forms.SendKeys]::SendWait($text)
    Start-Sleep -Milliseconds 150
}

# Common opening: app up, hotkey works, Escape hides, submit a prompt.
function Open-Spotlight-And-Submit([string]$scenario, [string]$prompt) {
    if (-not (Expect "app\.start" "app started")) { return $false }
    if (-not (Expect "tray\.created" "tray icon created")) { return $false }
    if (-not (Expect "hotkey\.registered chord=$([regex]::Escape($Hotkey))" "hotkey registered")) { return $false }
    if (-not (Expect "main\.shown" "main window shown at launch")) { return $false }
    Start-Sleep -Milliseconds 800

    Send-Chord
    if (-not (Expect "spotlight\.shown" "hotkey shows the spotlight" 3000)) { return $false }
    Start-Sleep -Milliseconds 600
    $sp = Spotlight-Window
    if ($sp) { Pass "spotlight window visible ($($sp.W)x$($sp.Hgt))" } else { Fail "spotlight window not visible" }
    Snap "$scenario-1-spotlight.png" $sp
    # The window is the card (no transparent margin on Windows): pixels just
    # inside the left and top edges must be the card colour (gray-900), not a
    # white or black fill.
    if ($sp) {
        $edge = Grab-Pixels ($sp.L + 4) ($sp.T + [int]($sp.Hgt / 2)) 1 1
        $top = Grab-Pixels ($sp.L + [int]($sp.W / 2)) ($sp.T + 4) 1 1
        $ok = $true
        foreach ($argb in @($edge[0], $top[0])) {
            $c = [System.Drawing.Color]::FromArgb($argb)
            if ([Math]::Abs($c.R - 17) -gt 12 -or [Math]::Abs($c.G - 24) -gt 12 -or [Math]::Abs($c.B - 39) -gt 12) {
                $ok = $false; Fail "spotlight edge pixel is R$($c.R) G$($c.G) B$($c.B), expected the card colour"
            }
        }
        if ($ok) { Pass "spotlight window is the card (edges are card-coloured)" }
    }

    Send-Escape
    if (-not (Expect "spotlight\.hidden reason=escape" "Escape hides the spotlight" 3000)) { return $false }
    Start-Sleep -Milliseconds 400
    if (Spotlight-Window) { Fail "spotlight still visible after Escape" } else { Pass "spotlight window gone" }

    Send-Chord
    if (-not (Expect "spotlight\.shown" "hotkey shows it again" 3000)) { return $false }
    Start-Sleep -Milliseconds 700
    Type-Text $prompt
    Snap "$scenario-2-typed.png" (Spotlight-Window)
    Type-Text "{ENTER}"
    if (-not (Expect "spotlight\.submitted" "Enter submits")) { return $false }
    if (-not (Expect "spotlight\.hidden reason=submit" "spotlight hides on submit" 3000)) { return $false }
    if (-not (Expect "agent\.step\.start" "first agent step starts")) { return $false }
    return $true
}

function Scenario-Text {
    $data = Start-App "text" $null
    try {
        if (Open-Spotlight-And-Submit "text" "hello cdout") {
            Expect "agent\.stream\.delta" "text streams in" | Out-Null
            Expect "agent\.step\.done kind=text" "step completes with text" | Out-Null
            Start-Sleep -Milliseconds 800
            Snap "text-3-main.png" (Main-Window)
            $sessions = @(Get-ChildItem (Join-Path $data "sessions") -Filter *.json -ErrorAction SilentlyContinue)
            Start-Sleep -Milliseconds 900
            $sessions = @(Get-ChildItem (Join-Path $data "sessions") -Filter *.json -ErrorAction SilentlyContinue)
            if ($sessions.Count -eq 1) {
                $json = Get-Content $sessions[0].FullName -Raw
                if ($json -match "Hello from the fake model\.") { Pass "session saved with the reply" } else { Fail "session file lacks the streamed reply" }
            } else { Fail "expected 1 session file in the isolated data dir, found $($sessions.Count)" }
        }
    } finally { Stop-App $data }
}

function Scenario-Command {
    $data = Start-App "command" @{ approval_mode = "ask" }
    try {
        if (Open-Spotlight-And-Submit "command" "run something") {
            Expect "agent\.step\.done kind=command" "model proposes a command" | Out-Null
            Start-Sleep -Milliseconds 800
            Snap "command-3-approval.png" (Main-Window)
            Click-Element "*Execute*" "click Execute on the approval card" | Out-Null
            Expect "exec\.start kind=approved" "approved command starts" | Out-Null
            Expect "exec\.done ok=true" "command finishes" 15000 | Out-Null
            Expect "agent\.step\.done kind=text" "agent continues after the result" | Out-Null
            Start-Sleep -Milliseconds 600
            Snap "command-4-done.png" (Main-Window)
        }
    } finally { Stop-App $data }
}

function Scenario-Question {
    $data = Start-App "question" $null
    try {
        if (Open-Spotlight-And-Submit "question" "convert these") {
            Expect "agent\.step\.done kind=question" "model asks a question" | Out-Null
            Start-Sleep -Milliseconds 800
            Snap "question-3-card.png" (Main-Window)
            Click-Element "mp4" "pick the mp4 option" | Out-Null
            Start-Sleep -Milliseconds 500
            Snap "question-4-picked.png" (Main-Window)
            Click-Element "Answer" "click Answer" | Out-Null
            Expect "agent\.step\.start" "answer starts the next step" | Out-Null
            Expect "agent\.step\.done kind=text" "agent continues after the answer" | Out-Null
        }
    } finally { Stop-App $data }
}

function Scenario-Markdown {
    $data = Start-App "markdown" $null
    try {
        if (Open-Spotlight-And-Submit "markdown" "show me markdown") {
            Expect "agent\.step\.done kind=text" "rich reply completes" | Out-Null
            if ((Wait-Log "fonts\.loaded segoe-ui" 1000 0) -gt 0) { Pass "system fonts loaded" } else { Fail "Segoe UI not loaded" }
            Start-Sleep -Milliseconds 900
            Snap "markdown-3-main.png" (Main-Window)
        }
    } finally { Stop-App $data }
}

function Scenario-Stop {
    $data = Start-App "long" $null
    try {
        if (Open-Spotlight-And-Submit "stop" "write a lot") {
            Expect "agent\.stream\.delta" "long reply streaming" | Out-Null
            Start-Sleep -Milliseconds 700
            Click-Element "Stop" "click Stop mid-stream" | Out-Null
            Start-Sleep -Milliseconds 1200
            Snap "stop-3-stopped.png" (Main-Window)
            if (Find-Element "*CDOUT INTERNAL*" 3000) { Pass "interrupt note shown" } else { Fail "interrupt note not shown" }
        }
    } finally { Stop-App $data }
}

if (-not $NoBuild) {
    Write-Host "== build cdout-native (fake-llm)"
    Push-Location $tauriDir
    try {
        cargo build -p cdout-native --features fake-llm
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    } finally { Pop-Location }
}
if (-not (Test-Path $exe)) { throw "missing $exe" }

foreach ($s in $Scenarios) {
    Write-Host "== scenario: $s"
    switch ($s) {
        "text" { Scenario-Text }
        "command" { Scenario-Command }
        "question" { Scenario-Question }
        "stop" { Scenario-Stop }
        "markdown" { Scenario-Markdown }
        default { Fail "unknown scenario $s" }
    }
}

Write-Host ""
if ($script:failures.Count -gt 0) {
    Write-Host "SMOKE FAILED ($($script:failures.Count)):" -ForegroundColor Red
    $script:failures | ForEach-Object { Write-Host " - $_" }
    exit 1
}
Write-Host "SMOKE PASSED. Screenshots in $OutDir" -ForegroundColor Green
exit 0
