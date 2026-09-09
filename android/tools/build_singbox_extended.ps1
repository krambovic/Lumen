[CmdletBinding()]
param(
    [string]$Ref = "v1.14.0-extended-2.7.1",
    [string]$Repository = "https://github.com/shtorm-7/sing-box-extended.git",
    [string]$LumenRevision = "1",
    [string]$GoExecutable = "go",
    [string]$WorkDirectory = ""
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$patchPath = Join-Path $repoRoot "android\patches\sing-box-extended-interface-monitor.patch"
if (-not (Test-Path -LiteralPath $patchPath -PathType Leaf)) {
    throw "sing-box Android compatibility patch is missing: $patchPath"
}

$ownsWorkDirectory = [string]::IsNullOrWhiteSpace($WorkDirectory)
if ($ownsWorkDirectory) {
    $WorkDirectory = Join-Path ([IO.Path]::GetTempPath()) ("lumen-sing-box-android-" + [guid]::NewGuid().ToString("N"))
}
$workRoot = [IO.Path]::GetFullPath($WorkDirectory)
$source = Join-Path $workRoot "source"

try {
    New-Item -ItemType Directory -Path $workRoot -Force | Out-Null
    & git clone --filter=blob:none --depth 1 --branch $Ref $Repository $source
    if ($LASTEXITCODE -ne 0) { throw "failed to clone sing-box-extended ref $Ref" }

    & git -C $source apply --check $patchPath
    if ($LASTEXITCODE -ne 0) { throw "the Android compatibility patch is incompatible with $Ref" }
    & git -C $source apply $patchPath
    if ($LASTEXITCODE -ne 0) { throw "failed to apply the Android compatibility patch" }

    # Keep the network feature set of the extended Android CLI artifacts already
    # shipped by Lumen. The embedded admin web UI is intentionally omitted: the
    # app never exposes it, and its generated dist directory is not part of the
    # tagged source tree.
    $tags = @(
        "with_gvisor", "with_quic", "with_dhcp", "with_wireguard", "with_utls",
        "with_acme", "with_clash_api", "with_tailscale", "with_masque",
        "with_mtproxy", "with_trusttunnel", "with_call", "with_sudoku",
        "with_manager", "with_profiler", "badlinkname", "tfogo_checklinkname0"
    ) -join ","
    $sharedLdflags = (Get-Content -LiteralPath (Join-Path $source "release\LDFLAGS") -Raw).Trim()
    $upstreamVersion = $Ref.TrimStart([char]'v')
    $version = "$upstreamVersion-lumen.$LumenRevision"
    $ldflags = "-X github.com/sagernet/sing-box/constant.Version=$version $sharedLdflags -s -w -buildid="

    $sdkPath = $env:ANDROID_HOME
    if ([string]::IsNullOrWhiteSpace($sdkPath)) {
        $localProperties = Join-Path $repoRoot "android\local.properties"
        $sdkLine = Get-Content -LiteralPath $localProperties | Where-Object { $_ -like "sdk.dir=*" } | Select-Object -First 1
        if ($sdkLine) { $sdkPath = $sdkLine.Substring("sdk.dir=".Length).Replace('/', '\') }
    }
    if (-not (Test-Path -LiteralPath $sdkPath -PathType Container)) { throw "Android SDK not found" }
    $ndkRoot = Join-Path $sdkPath "ndk\28.0.13004108"
    if (-not (Test-Path -LiteralPath $ndkRoot -PathType Container)) { throw "Android NDK r28 (28.0.13004108) not found" }

    $toolchain = Join-Path $ndkRoot "toolchains\llvm\prebuilt\windows-x86_64\bin"
    $targets = @(
        @{ Abi = "arm64-v8a"; GoArch = "arm64"; Cc = "aarch64-linux-android23-clang.cmd"; Cxx = "aarch64-linux-android23-clang++.cmd"; GoArm = "" },
        @{ Abi = "armeabi-v7a"; GoArch = "arm"; Cc = "armv7a-linux-androideabi23-clang.cmd"; Cxx = "armv7a-linux-androideabi23-clang++.cmd"; GoArm = "7" },
        @{ Abi = "x86_64"; GoArch = "amd64"; Cc = "x86_64-linux-android23-clang.cmd"; Cxx = "x86_64-linux-android23-clang++.cmd"; GoArm = "" }
    )

    Push-Location $source
    try {
        foreach ($target in $targets) {
            $output = Join-Path $repoRoot "android\app\src\main\jniLibs\$($target.Abi)\libsingbox.so"
            $env:CGO_ENABLED = "1"
            # Matches the extended project's release binaries and keeps the
            # embedded executable compact enough for per-ABI APK packaging.
            $env:GOEXPERIMENT = "nodwarf5"
            $env:GOOS = "android"
            $env:GOARCH = $target.GoArch
            $env:GOARM = $target.GoArm
            $env:CC = Join-Path $toolchain $target.Cc
            $env:CXX = Join-Path $toolchain $target.Cxx
            & $GoExecutable build -trimpath -o $output -tags $tags -ldflags $ldflags ./cmd/sing-box
            if ($LASTEXITCODE -ne 0) { throw "failed to build sing-box-extended for $($target.Abi)" }
        }
    }
    finally {
        Pop-Location
    }
}
finally {
    if ($ownsWorkDirectory -and (Test-Path -LiteralPath $workRoot)) {
        $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
        if ($workRoot.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) {
            Remove-Item -LiteralPath $workRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}
