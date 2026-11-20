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

# The vendored QWindowKit DLLs (qml_app/vendor/qwk) are built for the Qt
# minor version pinned in requirements.txt. An interpreter with another
# PyQt6 shows "entry point not found" for QWKCore.dll and runs without the
# custom frame (the window cannot be dragged), so pick one that matches.
$pin = Select-String -Path (Join-Path $windowsDir "requirements.txt") -Pattern 'PyQt6>=(\d+\.\d+)' |
    Select-Object -First 1
if (-not $pin) { throw "PyQt6 pin not found in requirements.txt" }
$qtMinor = $pin.Matches[0].Groups[1].Value

function Test-Interpreter([string]$exe) {
    if (-not (Test-Path $exe)) { return $false }
    $version = & $exe -c "from PyQt6.QtCore import QT_VERSION_STR; print(QT_VERSION_STR)" 2>$null
    return ($LASTEXITCODE -eq 0) -and ("$version".Trim().StartsWith("$qtMinor."))
}

if (-not $Python) {
    $candidates = @()
    if (Get-Command py.exe -ErrorAction SilentlyContinue) {
        $candidates += (& py.exe -0p 2>$null) | ForEach-Object {
            if ($_ -match '([A-Za-z]:\\.+python\.exe)\s*$') { $Matches[1] }
        }
    }
    $candidates += Get-Command python.exe -All -ErrorAction SilentlyContinue |
        Where-Object { $_.Source -notlike "*WindowsApps*" } | ForEach-Object { $_.Source }
    $match = $candidates | Select-Object -Unique | Where-Object { Test-Interpreter $_ } | Select-Object -First 1
    if (-not $match) { throw "No Python with PyQt6 $qtMinor.x found (install: pip install -r requirements.txt)" }
    $Python = Join-Path (Split-Path -Parent $match) "pythonw.exe"
}
if (-not (Test-Path $Python)) { throw "pythonw.exe not found: $Python" }
Write-Output "Using $Python (Qt $qtMinor)"

$link = Join-Path $Destination "Lumen (Python, admin).lnk"
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($link)
$shortcut.TargetPath = $Python
$shortcut.Arguments = '"' + (Join-Path $windowsDir "run_qml.py") + '" --side-by-side'
$shortcut.WorkingDirectory = $windowsDir
$icon = Join-Path $windowsDir "assets\Lumen.ico"
if (Test-Path $icon) { $shortcut.IconLocation = $icon }
$shortcut.Description = "Lumen QML client next to the Rust build (elevated)"
$shortcut.Save()

# Shell link header byte 0x15, flag 0x20 = SLDF_RUNAS_USER ("Run as administrator").
$bytes = [System.IO.File]::ReadAllBytes($link)
$bytes[0x15] = $bytes[0x15] -bor 0x20
[System.IO.File]::WriteAllBytes($link, $bytes)
Write-Output $link
