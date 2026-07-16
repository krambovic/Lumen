[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$OutputPath,
    [string]$Ref = "v1.13.14-extended-2.5.1",
    [string]$Repository = "https://github.com/shtorm-7/sing-box-extended.git",
    [string]$LumenRevision = "1",
    [string]$WorkDirectory = ""
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$patchPath = Join-Path $repoRoot "patches\sing-box-extended-direct-masque.patch"
if (-not (Test-Path -LiteralPath $patchPath -PathType Leaf)) {
    throw "sing-box compatibility patch is missing: $patchPath"
}

$output = [IO.Path]::GetFullPath($OutputPath)
$outputDirectory = Split-Path -Parent $output
New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null

$ownsWorkDirectory = [string]::IsNullOrWhiteSpace($WorkDirectory)
if ($ownsWorkDirectory) {
    $WorkDirectory = Join-Path ([IO.Path]::GetTempPath()) ("lumen-sing-box-" + [guid]::NewGuid().ToString("N"))
}
$workRoot = [IO.Path]::GetFullPath($WorkDirectory)
$source = Join-Path $workRoot "source"

try {
    New-Item -ItemType Directory -Path $workRoot -Force | Out-Null
    & git clone --filter=blob:none --depth 1 --branch $Ref $Repository $source
    if ($LASTEXITCODE -ne 0) {
        throw "failed to clone sing-box-extended ref $Ref"
    }

    & git -C $source apply --check $patchPath
    if ($LASTEXITCODE -ne 0) {
        throw "the Lumen MASQUE patch is incompatible with sing-box-extended $Ref"
    }
    & git -C $source apply $patchPath
    if ($LASTEXITCODE -ne 0) {
        throw "failed to apply the Lumen MASQUE patch"
    }

    $tags = (Get-Content -LiteralPath (Join-Path $source "release\DEFAULT_BUILD_TAGS_WINDOWS") -Raw).Trim()
    $sharedLdflags = (Get-Content -LiteralPath (Join-Path $source "release\LDFLAGS") -Raw).Trim()
    $upstreamVersion = $Ref.TrimStart([char]'v')
    $version = "$upstreamVersion-lumen.$LumenRevision"
    $ldflags = "-X github.com/sagernet/sing-box/constant.Version=$version $sharedLdflags -s -w -buildid="

    $env:CGO_ENABLED = "0"
    $env:GOOS = "windows"
    $env:GOARCH = "amd64"
    Push-Location $source
    try {
        & go build -trimpath -o $output -tags $tags -ldflags $ldflags ./cmd/sing-box
        if ($LASTEXITCODE -ne 0) {
            throw "failed to build sing-box-extended"
        }
    }
    finally {
        Pop-Location
    }

    & $output version
    if ($LASTEXITCODE -ne 0) {
        throw "built sing-box executable did not start"
    }
}
finally {
    if ($ownsWorkDirectory -and (Test-Path -LiteralPath $workRoot)) {
        $resolvedTemp = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
        if ($workRoot.StartsWith($resolvedTemp, [StringComparison]::OrdinalIgnoreCase)) {
            Remove-Item -LiteralPath $workRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}
