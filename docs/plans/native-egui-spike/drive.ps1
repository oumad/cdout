$ErrorActionPreference = "Stop"
$dir = $PSScriptRoot
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class KB {
  [DllImport("user32.dll")] public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, UIntPtr dwExtraInfo);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern int GetWindowTextW(IntPtr hWnd, System.Text.StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern IntPtr FindWindowW(string cls, string title);
}
"@
function Chord { [KB]::keybd_event(0x11,0,0,[UIntPtr]::Zero); [KB]::keybd_event(0x12,0,0,[UIntPtr]::Zero); [KB]::keybd_event(0x10,0,0,[UIntPtr]::Zero); [KB]::keybd_event(0x78,0,0,[UIntPtr]::Zero); Start-Sleep -m 60; [KB]::keybd_event(0x78,0,2,[UIntPtr]::Zero); [KB]::keybd_event(0x10,0,2,[UIntPtr]::Zero); [KB]::keybd_event(0x12,0,2,[UIntPtr]::Zero); [KB]::keybd_event(0x11,0,2,[UIntPtr]::Zero) }
function Snap($name) { $b = New-Object System.Drawing.Bitmap 760,400; $g=[System.Drawing.Graphics]::FromImage($b); $g.CopyFromScreen(240,240,0,0,$b.Size); $b.Save("$dir\$name",[System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $b.Dispose() }
function Fg { $h=[KB]::GetForegroundWindow(); $sb=New-Object System.Text.StringBuilder 256; [void][KB]::GetWindowTextW($h,$sb,256); "fg='$($sb.ToString())'" }
function Vis { $h=[KB]::FindWindowW($null,"cdout-spike"); if ($h -eq [IntPtr]::Zero) { "win=notfound" } else { "win=found visible=$([KB]::IsWindowVisible($h))" } }
try {
$p = Start-Process -FilePath "$dir\target\debug\cdout-spike.exe" -WorkingDirectory $dir -PassThru
Start-Sleep -s 4
"t0 " + (Fg) + " " + (Vis); Snap "shot1_shown.png"
Chord; Start-Sleep -s 1
"t1 after chord " + (Fg) + " " + (Vis); Snap "shot2_hidden.png"
Chord; Start-Sleep -s 1
"t2 after chord " + (Fg) + " " + (Vis); Snap "shot3_shown_again.png"
[System.Windows.Forms.SendKeys]::SendWait("hello cdout") 2>$null
Start-Sleep -m 600; Snap "shot4_typed.png"
} finally { if ($p -and -not $p.HasExited) { Stop-Process -Id $p.Id -Force }; Start-Sleep -m 500; "exited=" + $p.HasExited }
