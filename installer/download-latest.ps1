param(
    [Parameter(Mandatory = $true)]
    [string] $InstallDir,

    [string] $Repo = "krambovic/bebra-kvn",
    [string] $AssetName = "BebraVPN-portable-windows-x64.zip"
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

function Invoke-GitHubJson {
    param([Parameter(Mandatory = $true)][string] $Uri)
    Invoke-RestMethod `
        -Uri $Uri `
        -UseBasicParsing `
        -Headers @{
            "Accept" = "application/vnd.github+json"
            "User-Agent" = "BebraVPN-Setup"
        }
}

function Find-BebraPortableAsset {
    param([Parameter(Mandatory = $true)] $Release)
    $assets = @($Release.assets)
    $asset = $assets | Where-Object { $_.name -eq $AssetName } | Select-Object -First 1
    if ($asset) {
        return $asset
    }
    return $assets |
        Where-Object { $_.name -like "*portable*windows*x64*.zip" -and $_.name -notlike "*.sha256" } |
        Select-Object -First 1
}

$release = Invoke-GitHubJson "https://api.github.com/repos/$Repo/releases/latest"
$asset = Find-BebraPortableAsset $release
if (-not $asset -or -not $asset.browser_download_url) {
    throw "Portable asset was not found in latest release $($release.tag_name)"
}

$workDir = Join-Path ([IO.Path]::GetTempPath()) ("BebraVPN-setup-" + [guid]::NewGuid().ToString("N"))
$zipPath = Join-Path $workDir $asset.name
$extractDir = Join-Path $workDir "extract"
New-Item -ItemType Directory -Path $workDir, $extractDir -Force | Out-Null
New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null

try {
    Invoke-WebRequest `
        -Uri $asset.browser_download_url `
        -OutFile $zipPath `
        -UseBasicParsing `
        -Headers @{ "User-Agent" = "BebraVPN-Setup" }

    Expand-Archive -LiteralPath $zipPath -DestinationPath $extractDir -Force

    $sourceDir = Join-Path $extractDir "BebraVPN"
    if (-not (Test-Path -LiteralPath (Join-Path $sourceDir "BebraVPN.exe"))) {
        $sourceDir = Get-ChildItem -LiteralPath $extractDir -Directory |
            Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName "BebraVPN.exe") } |
            Select-Object -ExpandProperty FullName -First 1
    }
    if (-not $sourceDir -or -not (Test-Path -LiteralPath (Join-Path $sourceDir "BebraVPN.exe"))) {
        throw "Downloaded portable archive does not contain BebraVPN.exe"
    }

    Get-ChildItem -LiteralPath $sourceDir -Force | ForEach-Object {
        Copy-Item -LiteralPath $_.FullName -Destination $InstallDir -Recurse -Force
    }

    $versionPath = Join-Path $InstallDir "installed-release.txt"
    "Installed from $($release.tag_name) at $(Get-Date -Format o)" | Set-Content -LiteralPath $versionPath -Encoding UTF8
}
finally {
    Remove-Item -LiteralPath $workDir -Recurse -Force -ErrorAction SilentlyContinue
}
