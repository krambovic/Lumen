# Creates a desktop shortcut that starts the Python (QML) client elevated
# with --side-by-side, so it runs next to the Rust client without the
# single-instance mutex. Both clients still share the same profile in
# %LOCALAPPDATA%\Lumen\data: the last one to save wins.
param(
    [string]$Python = "",
    [string]$Destination = [Environment]::GetFolderPath("Desktop")
)
$ErrorActionPreference = "Stop"
$windowsDir = Split-Path -Parent $PSScriptRoot
if (-not $Python) {
    $python = (Get-Command python.exe -ErrorAction Stop).Source
    $Python = Join-Path (Split-Path -Parent $python) "pythonw.exe"
}
if (-not (Test-Path $Python)) { throw "pythonw.exe not found: $Python" }

$link = Join-Path $Destination "Lumen (Python, admin).lnk"
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($link)
$shortcut.TargetPath = $Python
$shortcut.Arguments = '"' + (Join-Path $windowsDir "run_qml.py") + '" --side-by-side'
$shortcut.WorkingDirectory = $windowsDir
$icon = Join-Path $windowsDir "assets\app.ico"
if (Test-Path $icon) { $shortcut.IconLocation = $icon }
$shortcut.Description = "Lumen QML client next to the Rust build (elevated)"
$shortcut.Save()

# Shell link header byte 0x15, flag 0x20 = SLDF_RUNAS_USER ("Run as administrator").
$bytes = [System.IO.File]::ReadAllBytes($link)
$bytes[0x15] = $bytes[0x15] -bor 0x20
[System.IO.File]::WriteAllBytes($link, $bytes)
Write-Output $link
