//! Byte-identical ports of the elevated PowerShell install/restart scripts.

use std::path::PathBuf;

use super::powershell_literal;

pub const EXE_NAME: &str = "Lumen.exe";

pub struct UpdateScriptContext {
    pub pid: u32,
    pub setup_path: PathBuf,
    pub current_app_dir: PathBuf,
    pub app_dir: PathBuf,
    pub temp_dir: PathBuf,
    pub expected_version: String,
    pub expected_hash: String,
    pub portable: bool,
}

/// `"\r\n".join([...])` of the portable or installed script, 1:1 with
/// `app_updater.py`.
pub fn build_update_script(ctx: &UpdateScriptContext) -> String {
    let lines = if ctx.portable {
        portable_script(ctx)
    } else {
        installed_script(ctx)
    };
    lines.join("\r\n")
}

fn quoted(ctx: &UpdateScriptContext) -> [String; 8] {
    let exe_path = ctx.app_dir.join(EXE_NAME);
    [
        format!("$pidToWait = {}", ctx.pid),
        format!("$setupPath = {}", powershell_literal(&ctx.setup_path.to_string_lossy())),
        format!("$currentAppDir = {}", powershell_literal(&ctx.current_app_dir.to_string_lossy())),
        format!("$appDir = {}", powershell_literal(&ctx.app_dir.to_string_lossy())),
        format!("$exePath = {}", powershell_literal(&exe_path.to_string_lossy())),
        format!("$tempDir = {}", powershell_literal(&ctx.temp_dir.to_string_lossy())),
        format!("$expectedVersion = {}", powershell_literal(&ctx.expected_version)),
        format!("$expectedHash = {}", powershell_literal(&ctx.expected_hash)),
    ]
}

fn header(ctx: &UpdateScriptContext) -> Vec<String> {
    let [pid_to_wait, setup_path, current_app_dir, app_dir, exe_path, temp_dir, expected_version, expected_hash] =
        quoted(ctx);
    vec![
        "$ErrorActionPreference = 'Stop'".into(),
        pid_to_wait,
        setup_path,
        current_app_dir,
        app_dir,
        exe_path,
        "$fallbackExe = Join-Path $currentAppDir 'Lumen.exe'".into(),
        temp_dir,
        expected_version,
        expected_hash,
        "$logDir = Join-Path (Join-Path $appDir 'data') 'logs'".into(),
        "$runtimeDir = Join-Path (Join-Path $appDir 'data') 'runtime'".into(),
        "$errorLog = Join-Path $logDir 'update_error.log'".into(),
    ]
}

/// Portable-only fallback: when the current directory has no Lumen.exe, the
/// catch block may relaunch the legacy LumenKVN.exe instead.
fn portable_fallback_line() -> String {
    "if (-not (Test-Path -LiteralPath $fallbackExe)) { $fallbackExe = Join-Path $currentAppDir 'LumenKVN.exe' }".into()
}

/// Lines shared by both scripts: wait for the current process, stop the app
/// and every bundled core, then stop only driver services whose ImagePath
/// points into our own install roots (a co-installed GoodbyeDPI/ByeDPI or
/// standalone zapret uses the same generic service names and must not be
/// torn down by our updater).
fn shutdown_lines() -> Vec<String> {
    vec![
        "for ($i = 0; $i -lt 120; $i++) {".to_string(),
        "    if (-not (Get-Process -Id $pidToWait -ErrorAction SilentlyContinue)) { break }".to_string(),
        "    Start-Sleep -Milliseconds 500".to_string(),
        "}".to_string(),
        "$proc = Get-Process -Id $pidToWait -ErrorAction SilentlyContinue".to_string(),
        "if ($proc) { Stop-Process -Id $pidToWait -Force }".to_string(),
        "Get-Process -Name 'Lumen','LumenKVN','Lumen-qml','LumenKVN-qml' -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue".to_string(),
        "foreach ($coreName in @('xray','sing-box','singbox','winws','winws2','tun2socks','warp-svc')) { Get-Process -Name $coreName -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue }".to_string(),
        "$ownedRoots = @($appDir, $currentAppDir) | Where-Object { $_ } | ForEach-Object { $_.ToLower() }".to_string(),
    ]
}

fn catch_block() -> Vec<String> {
    vec![
        "catch {".into(),
        "    if (Test-Path -LiteralPath $exePath) { Start-Process -FilePath $exePath -ArgumentList '--relaunched' -WorkingDirectory $appDir -ErrorAction SilentlyContinue | Out-Null } elseif (Test-Path -LiteralPath $fallbackExe) { Start-Process -FilePath $fallbackExe -ArgumentList '--relaunched' -WorkingDirectory $currentAppDir -ErrorAction SilentlyContinue | Out-Null }".into(),
        "    New-Item -ItemType Directory -Path $logDir -Force | Out-Null".into(),
        "    ($_ | Out-String) | Set-Content -LiteralPath $errorLog -Encoding UTF8".into(),
        "    throw".into(),
        "}".into(),
    ]
}

fn portable_script(ctx: &UpdateScriptContext) -> Vec<String> {
    let mut lines = header(ctx);
    // Python inserts the fallback rewrite between $fallbackExe and $tempDir.
    lines.insert(7, portable_fallback_line());
    lines.extend(shutdown_lines());
    lines.extend([
        r"foreach ($driverName in @('Monkey','WinDivert','WinDivert14','WinDivert64','WinDivert2')) {".to_string(),
        r"    $svcKey = 'HKLM:\SYSTEM\CurrentControlSet\Services\' + $driverName".to_string(),
        "    $svcImage = (Get-ItemProperty -Path $svcKey -Name ImagePath -ErrorAction SilentlyContinue).ImagePath".to_string(),
        "    if (-not $svcImage) { continue }".to_string(),
        "    $svcImage = $svcImage.ToLower()".to_string(),
        "    $owned = $false".to_string(),
        "    foreach ($root in $ownedRoots) { if ($svcImage.Contains($root)) { $owned = $true } }".to_string(),
        "    if (-not $owned) { continue }".to_string(),
        "    & sc.exe stop $driverName *> $null".to_string(),
        "    & sc.exe delete $driverName *> $null".to_string(),
        "}".to_string(),
        "Start-Sleep -Milliseconds 1200".to_string(),
        "try {".to_string(),
        "    if ((Get-FileHash -LiteralPath $setupPath -Algorithm SHA256).Hash -ine $expectedHash) { throw 'Update payload failed SHA-256 verification' }".to_string(),
        "    $tempExtractDir = Join-Path $tempDir 'extract'".to_string(),
        "    New-Item -ItemType Directory -Path $tempExtractDir -Force | Out-Null".to_string(),
        "    Expand-Archive -LiteralPath $setupPath -DestinationPath $tempExtractDir -Force".to_string(),
        "    $srcDir = $tempExtractDir".to_string(),
        "    if (Test-Path -Path (Join-Path $tempExtractDir 'Lumen')) {".to_string(),
        "        $srcDir = Join-Path $tempExtractDir 'Lumen'".to_string(),
        "    } elseif (Test-Path -Path (Join-Path $tempExtractDir 'LumenKVN')) {".to_string(),
        "        $srcDir = Join-Path $tempExtractDir 'LumenKVN'".to_string(),
        "    }".to_string(),
        "    $currentDataDir = Join-Path $currentAppDir 'data'".to_string(),
        "    $targetDataDir = Join-Path $appDir 'data'".to_string(),
        "    New-Item -ItemType Directory -Path $appDir -Force | Out-Null".to_string(),
        "    if ($currentAppDir -ine $appDir -and (Test-Path -LiteralPath $currentDataDir) -and -not (Test-Path -LiteralPath $targetDataDir)) {".to_string(),
        "        Copy-Item -LiteralPath $currentDataDir -Destination $appDir -Recurse -Force".to_string(),
        "    }".to_string(),
        "    Get-ChildItem -Path $appDir -Force | ForEach-Object {".to_string(),
        "        $name = $_.Name.ToLower()".to_string(),
        "        if ($name -ne 'data' -and $name -ne 'portable.txt' -and $name -ne 'portable' -and $_.FullName -ne $tempDir) {".to_string(),
        "            Remove-Item -LiteralPath $_.FullName -Recurse -Force -ErrorAction SilentlyContinue".to_string(),
        "        }".to_string(),
        "    }".to_string(),
        "    Get-ChildItem -Path $srcDir -Force | ForEach-Object {".to_string(),
        "        Move-Item -LiteralPath $_.FullName -Destination $appDir -Force".to_string(),
        "    }".to_string(),
        "    if (-not (Test-Path -LiteralPath $exePath)) { throw 'Updated Lumen.exe was not found' }".to_string(),
        "    $started = Start-Process -FilePath $exePath -ArgumentList '--relaunched' -WorkingDirectory $appDir -PassThru -ErrorAction Stop".to_string(),
        "    Start-Sleep -Seconds 5".to_string(),
        "    if ($started.HasExited) {".to_string(),
        "        throw ('Updated application exited immediately with code ' + $started.ExitCode)".to_string(),
        "    }".to_string(),
        "    $legacyPortableNames = @('Lumen KVN','LumenKVN','lumen-kvn','Lumen_KVN')".to_string(),
        "    if ($currentAppDir -ine $appDir -and $legacyPortableNames -icontains (Split-Path -Leaf $currentAppDir)) {".to_string(),
        "        Remove-Item -LiteralPath $currentAppDir -Recurse -Force -ErrorAction SilentlyContinue".to_string(),
        "    }".to_string(),
        "}".to_string(),
    ]);
    lines.extend(catch_block());
    lines.push("Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue".into());
    lines.push(String::new());
    lines
}

fn installed_script(ctx: &UpdateScriptContext) -> Vec<String> {
    let mut lines = header(ctx);
    lines.push("$setupLog = Join-Path $logDir 'setup_update.log'".into());
    lines.push(r"if ((Split-Path -Leaf $appDir) -ieq 'Program') { throw 'Install directory resolved to C:\Program; aborting update.' }".into());
    lines.extend([
        "New-Item -ItemType Directory -Path $appDir -Force | Out-Null".to_string(),
        "New-Item -ItemType Directory -Path $logDir -Force | Out-Null".to_string(),
        "New-Item -ItemType Directory -Path $runtimeDir -Force | Out-Null".to_string(),
        "$adminIdentity = [Security.Principal.WindowsIdentity]::GetCurrent()".to_string(),
        "$adminPrincipal = New-Object Security.Principal.WindowsPrincipal($adminIdentity)".to_string(),
        "if (-not $adminPrincipal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Lumen updater must run as administrator to replace zapret drivers.' }".to_string(),
    ]);
    lines.extend(shutdown_lines());
    lines.extend([
        r"function Remove-LumenDrivers($roots) {".to_string(),
        r"    foreach ($driverName in @('Monkey','WinDivert','WinDivert14','WinDivert64','WinDivert2')) {".to_string(),
        r"        $svcKey = 'HKLM:\SYSTEM\CurrentControlSet\Services\' + $driverName".to_string(),
        "        $svcImage = (Get-ItemProperty -Path $svcKey -Name ImagePath -ErrorAction SilentlyContinue).ImagePath".to_string(),
        "        if (-not $svcImage) { continue }".to_string(),
        "        $svcImage = $svcImage.ToLower()".to_string(),
        "        $owned = $false".to_string(),
        "        foreach ($root in $roots) { if ($svcImage.Contains($root)) { $owned = $true } }".to_string(),
        "        if (-not $owned) { continue }".to_string(),
        "        & sc.exe stop $driverName *> $null".to_string(),
        "        & sc.exe delete $driverName *> $null".to_string(),
        "    }".to_string(),
        "}".to_string(),
        "Remove-LumenDrivers $ownedRoots".to_string(),
        "$zapretExeDir = Join-Path (Join-Path $appDir 'zapret') 'exe'".to_string(),
        "$currentDataDir = Join-Path $currentAppDir 'data'".to_string(),
        "if ($currentAppDir -ine $appDir -and (Test-Path -LiteralPath $currentDataDir)) {".to_string(),
        "    Copy-Item -LiteralPath $currentDataDir -Destination $appDir -Recurse -Force".to_string(),
        "}".to_string(),
        "foreach ($driverFile in @('Monkey64.sys','WinDivert32.sys','WinDivert64.sys')) {".to_string(),
        "    Remove-Item -LiteralPath (Join-Path $zapretExeDir $driverFile) -Force -ErrorAction SilentlyContinue".to_string(),
        "}".to_string(),
        "Start-Sleep -Milliseconds 1200".to_string(),
        "try {".to_string(),
        "    if ((Get-FileHash -LiteralPath $setupPath -Algorithm SHA256).Hash -ine $expectedHash) { throw 'Update payload failed SHA-256 verification' }".to_string(),
        r#"    $installDirArg = '/DIR="' + $appDir + '"'"#.to_string(),
        r#"    $logArg = '/LOG="' + $setupLog + '"'"#.to_string(),
        "    $installerArgs = @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/CLOSEAPPLICATIONS','/FORCECLOSEAPPLICATIONS','/RESTARTEXITCODE=3010',$installDirArg,$logArg)".to_string(),
        "    $install = $null".to_string(),
        "    for ($attempt = 1; $attempt -le 3; $attempt++) {".to_string(),
        "        $install = Start-Process -FilePath $setupPath -ArgumentList $installerArgs -Wait -PassThru -ErrorAction Stop".to_string(),
        "        if ($install.ExitCode -eq 0 -or $install.ExitCode -eq 3010) { break }".to_string(),
        "        Get-Process -Name 'Lumen','LumenKVN','Lumen-qml','LumenKVN-qml' -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue".to_string(),
        "        foreach ($coreName in @('xray','sing-box','singbox','winws','winws2','tun2socks','warp-svc')) { Get-Process -Name $coreName -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue }".to_string(),
        "        Remove-LumenDrivers $ownedRoots".to_string(),
        "        foreach ($driverFile in @('Monkey64.sys','WinDivert32.sys','WinDivert64.sys')) {".to_string(),
        "            Remove-Item -LiteralPath (Join-Path $zapretExeDir $driverFile) -Force -ErrorAction SilentlyContinue".to_string(),
        "        }".to_string(),
        "        Start-Sleep -Seconds 2".to_string(),
        "    }".to_string(),
        "    if ($install.ExitCode -ne 0 -and $install.ExitCode -ne 3010) {".to_string(),
        "        $setupTail = ''".to_string(),
        "        if (Test-Path -LiteralPath $setupLog) { $setupTail = (Get-Content -LiteralPath $setupLog -Tail 30 -ErrorAction SilentlyContinue) -join [Environment]::NewLine }".to_string(),
        "        throw ('Installer exited with code ' + $install.ExitCode + [Environment]::NewLine + 'Setup log tail:' + [Environment]::NewLine + $setupTail) }".to_string(),
        r"    $uninstallKeys = @('HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{9B0BE72A-7D80-4D43-9871-3A5F0DA0D9C6}_is1','HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\{9B0BE72A-7D80-4D43-9871-3A5F0DA0D9C6}_is1','HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{9B0BE72A-7D80-4D43-9871-3A5F0DA0D9C6}_is1','HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\LumenKVN_is1','HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\LumenKVN_is1','HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\LumenKVN_is1')".to_string(),
        r"    foreach ($uk in $uninstallKeys) { try { $loc = (Get-ItemProperty -LiteralPath $uk -ErrorAction Stop).InstallLocation; if ($loc -and (Test-Path -LiteralPath (Join-Path $loc 'Lumen.exe'))) { $appDir = $loc.TrimEnd('\'); $exePath = Join-Path $appDir 'Lumen.exe'; break } elseif ($loc -and (Test-Path -LiteralPath (Join-Path $loc 'LumenKVN.exe'))) { $appDir = $loc.TrimEnd('\'); $exePath = Join-Path $appDir 'Lumen.exe'; break } } catch {} }".to_string(),
        "    $currentDataDir = Join-Path $currentAppDir 'data'".to_string(),
        "    if ($currentAppDir -ine $appDir -and (Test-Path -LiteralPath $currentDataDir)) {".to_string(),
        "        Copy-Item -LiteralPath $currentDataDir -Destination $appDir -Recurse -Force".to_string(),
        "    }".to_string(),
        "    if (-not (Test-Path -LiteralPath $exePath)) { throw 'Updated Lumen.exe was not installed' }".to_string(),
        "    $versionFile = Join-Path $runtimeDir 'update_version.txt'".to_string(),
        "    Remove-Item -LiteralPath $versionFile -Force -ErrorAction SilentlyContinue".to_string(),
        "    & $exePath '--version-file' $versionFile".to_string(),
        "    $versionProbeExitCode = $LASTEXITCODE".to_string(),
        "    if ($versionProbeExitCode -ne 0) { throw ('Version probe exited with code ' + $versionProbeExitCode) }".to_string(),
        "    $versionLine = Get-Content -LiteralPath $versionFile -ErrorAction SilentlyContinue | Select-Object -First 1".to_string(),
        "    if ($null -eq $versionLine -or [string]::IsNullOrWhiteSpace([string]$versionLine)) { throw 'Updated executable did not write version probe file' }".to_string(),
        "    $installedVersion = ([string]$versionLine).Trim().TrimStart('v')".to_string(),
        "    if ($expectedVersion -and $installedVersion -ne $expectedVersion.TrimStart('v')) { throw ('Updated executable reports v' + $installedVersion + ', expected v' + $expectedVersion) }".to_string(),
        "    $oldExePath = Join-Path $currentAppDir 'Lumen.exe'".to_string(),
        "    $oldLegacyExePath = Join-Path $currentAppDir 'LumenKVN.exe'".to_string(),
        "    $currentRoot = [System.IO.Path]::GetPathRoot($currentAppDir)".to_string(),
        "    $currentParent = Split-Path -Parent $currentAppDir".to_string(),
        r"    $isRootProgramDir = ((Split-Path -Leaf $currentAppDir) -ieq 'Program' -and $currentParent.TrimEnd('\') -ieq $currentRoot.TrimEnd('\'))".to_string(),
        "    $legacyDirNames = @('Lumen KVN','LumenKVN','lumen-kvn','Lumen_KVN')".to_string(),
        "    $isLegacyNamedDir = $legacyDirNames -icontains (Split-Path -Leaf $currentAppDir)".to_string(),
        "    $hasLegacyApp = (Test-Path -LiteralPath $oldExePath) -or (Test-Path -LiteralPath $oldLegacyExePath)".to_string(),
        "    if ($currentAppDir -ine $appDir -and ($isRootProgramDir -or $isLegacyNamedDir) -and $hasLegacyApp) {".to_string(),
        "        Remove-Item -LiteralPath $currentAppDir -Recurse -Force -ErrorAction SilentlyContinue".to_string(),
        "    }".to_string(),
        r"    $legacyRootProgram = 'C:\Program'".to_string(),
        "    $legacyRootExe = Join-Path $legacyRootProgram 'Lumen.exe'".to_string(),
        "    $legacyRootOldExe = Join-Path $legacyRootProgram 'LumenKVN.exe'".to_string(),
        "    if ($legacyRootProgram -ine $appDir -and ((Test-Path -LiteralPath $legacyRootExe) -or (Test-Path -LiteralPath $legacyRootOldExe))) {".to_string(),
        "        Remove-Item -LiteralPath $legacyRootProgram -Recurse -Force -ErrorAction SilentlyContinue".to_string(),
        "    }".to_string(),
        "    $started = Start-Process -FilePath $exePath -ArgumentList '--relaunched' -WorkingDirectory $appDir -PassThru -ErrorAction Stop".to_string(),
        "    Start-Sleep -Seconds 5".to_string(),
        "    if ($started.HasExited) {".to_string(),
        "        throw ('Updated application exited immediately with code ' + $started.ExitCode)".to_string(),
        "    }".to_string(),
        "}".to_string(),
    ]);
    lines.extend(catch_block());
    lines.push("Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue".into());
    lines.push(String::new());
    lines
}
