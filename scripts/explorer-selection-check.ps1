<#
.SYNOPSIS
  Regression check for Explorer window/tab/selection detection on Windows.

.DESCRIPTION
  Opens one throwaway Explorer window on a temp folder, adds a second tab,
  selects a file in each, then switches tabs through UI Automation (the
  ground truth for "which tab is showing") and asks the resolver which
  folder and files it sees. Also covers the two real-world failure modes:

   - a busy desktop (the resolver must find Explorer however many top-level
     windows exist), and
   - a stale title: Explorer does not refresh a background window's title
     when its tab changes, so the check switches tabs while another window
     is in the foreground and expects the resolver to ignore the title.

  Closes the window and removes the temp folders afterwards. Does not type
  into anything; only UI Automation patterns and COM are used.

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/explorer-selection-check.ps1
#>
param([switch]$NoBuild)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$AE = [System.Windows.Automation.AutomationElement]
$repo = Split-Path -Parent $PSScriptRoot
$tauriDir = Join-Path $repo "src-tauri"
$probe = Join-Path $tauriDir "target\debug\examples\explorer_probe.exe"

$script:failures = @()
function Pass($m) { Write-Host "  ok: $m" -ForegroundColor Green }
function Fail($m) { Write-Host "  FAIL: $m" -ForegroundColor Red; $script:failures += $m }

if (-not $NoBuild) {
    Push-Location $tauriDir
    try {
        cargo build -q -p cdout-core --example explorer_probe
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    } finally { Pop-Location }
}
if (-not (Test-Path $probe)) { throw "missing $probe" }

function Resolve-Now {
    $json = & $probe --json 2>&1 | Out-String
    try { return ($json | ConvertFrom-Json) } catch { throw "probe output was not JSON: $json" }
}

$root = Join-Path $env:TEMP "cdout-selcheck"
Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
$A = Join-Path $root "AlphaFolder"; $B = Join-Path $root "BetaFolder"
New-Item -ItemType Directory -Force -Path $A, $B | Out-Null
"a" | Set-Content "$A\a1.txt"; "a" | Set-Content "$A\a2.txt"; "a" | Set-Content "$A\a10.txt"
"b" | Set-Content "$B\b1.txt"

$shell = New-Object -ComObject Shell.Application
function Tabs($hwnd) { @($shell.Windows() | Where-Object { $_.HWND -eq $hwnd }) }
function TabFor($hwnd, $suffix) { Tabs $hwnd | Where-Object { $_.LocationURL -like "*$suffix" } | Select-Object -First 1 }

Write-Host "== desktop"
$topLevel = (Resolve-Now).frames
Write-Host "  Explorer windows before the test: $($topLevel.Count)"

Write-Host "== open a window with two tabs"
$before = @($shell.Windows() | ForEach-Object { $_.HWND })
Start-Process explorer.exe -ArgumentList "`"$A`""
$win = $null
for ($i = 0; $i -lt 50 -and -not $win; $i++) {
    Start-Sleep -Milliseconds 200
    $win = $shell.Windows() | Where-Object { $_.LocationURL -like "*AlphaFolder" -and ($before -notcontains $_.HWND) } | Select-Object -First 1
}
if (-not $win) { throw "Explorer window for $A did not appear" }
$hwnd = $win.HWND
try {
    $cond = New-Object System.Windows.Automation.PropertyCondition($AE::NativeWindowHandleProperty, [int]$hwnd)
    $winEl = $AE::RootElement.FindFirst([System.Windows.Automation.TreeScope]::Children, $cond)
    if (-not $winEl) { throw "UIA could not find the Explorer window" }
    $btnCond = New-Object System.Windows.Automation.PropertyCondition($AE::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)
    $add = $winEl.FindAll([System.Windows.Automation.TreeScope]::Descendants, $btnCond) | Where-Object { $_.Current.Name -match "new tab" } | Select-Object -First 1
    if (-not $add) { throw "no 'Add New Tab' button (tabs need Windows 11 22H2+)" }
    $add.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    $tabB = $null
    for ($i = 0; $i -lt 50 -and -not $tabB; $i++) {
        Start-Sleep -Milliseconds 200
        $tabB = Tabs $hwnd | Where-Object { $_.LocationURL -notlike "*AlphaFolder" } | Select-Object -First 1
    }
    if (-not $tabB) { throw "second tab did not appear" }
    $tabB.Navigate($B)
    for ($i = 0; $i -lt 50; $i++) { Start-Sleep -Milliseconds 200; if ((TabFor $hwnd "BetaFolder")) { break } }
    $tabA = TabFor $hwnd "AlphaFolder"; $tabB = TabFor $hwnd "BetaFolder"
    if (-not $tabA -or -not $tabB) { throw "could not find both tabs through Shell.Application" }
    Pass "window $hwnd has tabs AlphaFolder and BetaFolder"

    # Select a2 + a10 in A (natural order must put a2 before a10) and b1 in B.
    $tabA.Document.SelectItem($tabA.Document.Folder.ParseName("a2.txt"), 29)
    $tabA.Document.SelectItem($tabA.Document.Folder.ParseName("a10.txt"), 9)
    $tabB.Document.SelectItem($tabB.Document.Folder.ParseName("b1.txt"), 29)
    Start-Sleep -Milliseconds 400

    $tabCond = New-Object System.Windows.Automation.PropertyCondition($AE::ControlTypeProperty, [System.Windows.Automation.ControlType]::TabItem)
    function Select-Tab($name) {
        $item = $winEl.FindAll([System.Windows.Automation.TreeScope]::Descendants, $tabCond) | Where-Object { $_.Current.Name -like "$name*" } | Select-Object -First 1
        if (-not $item) { throw "no tab item named $name" }
        $item.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
        Start-Sleep -Milliseconds 700
    }
    function Check($label, $expectPath, $expectFiles) {
        $r = Resolve-Now
        $target = $r.target_explorer_hwnd
        if ($target -ne "$hwnd") { Fail "${label}: resolver targeted window $target, expected $hwnd (title '$($r.target_explorer_title)')" }
        $files = @($r.selection)
        $okPath = ($r.resolved_path -eq $expectPath)
        $okFiles = (($files -join "|") -eq ($expectFiles -join "|"))
        if ($okPath -and $okFiles) { Pass "${label}: $($r.resolution) -> $expectPath [$($files.Count) file(s)]" }
        else { Fail "${label}: got path='$($r.resolved_path)' files=[$($files -join ', ')] via $($r.resolution); expected '$expectPath' [$($expectFiles -join ', ')]" }
        return $r
    }

    Write-Host "== active-tab detection (window in the foreground)"
    Select-Tab "AlphaFolder"
    Check "tab A active" $A @("$A\a2.txt", "$A\a10.txt") | Out-Null
    Select-Tab "BetaFolder"
    Check "tab B active" $B @("$B\b1.txt") | Out-Null

    Write-Host "== stale title (window in the background while its tab changes)"
    # Put this console (or any other window) in front, then switch tabs
    # without activating Explorer. Explorer leaves the title as it was.
    Add-Type @"
using System; using System.Runtime.InteropServices;
public static class FG { [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h); [DllImport("kernel32.dll")] public static extern IntPtr GetConsoleWindow(); }
"@
    [void][FG]::SetForegroundWindow([FG]::GetConsoleWindow())
    Start-Sleep -Milliseconds 400
    Select-Tab "AlphaFolder"
    $r = Check "tab A active, window in background" $A @("$A\a2.txt", "$A\a10.txt")
    Write-Host "  (window title now: '$($r.target_explorer_title)')"
    $activeTab = @($r.tabs | Where-Object { $_.is_active })
    if ($activeTab.Count -eq 1 -and $activeTab[0].z_index -eq 0) { Pass "active tab chosen by Z-order, not by title" } else { Fail "active tab was not chosen by Z-order: $($activeTab | ConvertTo-Json -Compress)" }

    Write-Host "== empty selection is reported as empty, not borrowed from another tab"
    $tabA.Document.SelectItem($tabA.Document.Folder.ParseName("a2.txt"), 4)
    $tabA.Document.SelectItem($tabA.Document.Folder.ParseName("a10.txt"), 4)
    Start-Sleep -Milliseconds 400
    Check "tab A active, nothing selected" $A @() | Out-Null

    Write-Host "== search results tab: selection has paths, location has none"
    $sel = Resolve-Now
    $nonFile = @($sel.tabs | Where-Object { $_.in_target_frame -and -not $_.path })
    Write-Host "  (non-filesystem tabs in the target window: $($nonFile.Count); covered by unit tests)"
} finally {
    try { foreach ($t in (Tabs $hwnd)) { $t.Quit() } } catch {}
    Start-Sleep -Milliseconds 500
    try { foreach ($t in (Tabs $hwnd)) { $t.Quit() } } catch {}
    Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
}

Write-Host ""
if ($script:failures.Count -gt 0) {
    Write-Host "EXPLORER CHECK FAILED ($($script:failures.Count)):" -ForegroundColor Red
    $script:failures | ForEach-Object { Write-Host " - $_" }
    exit 1
}
Write-Host "EXPLORER CHECK PASSED" -ForegroundColor Green
exit 0
