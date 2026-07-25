# Generates every launcher icon asset from the original Lumen logo.
# v4: the logo alpha is boosted (the source PNG is semi transparent, which made
# the "L" look washed out) and every legacy bitmap is masked so launchers that
# do not support adaptive icons still get a properly rounded icon.
Add-Type -AssemblyName System.Drawing

$res = "C:\lumen-kvn-release\android\app\src\main\res"
$logoPath = "C:\lumen-kvn-release\windows\assets\Lumen.png"
$bgColor = [System.Drawing.ColorTranslator]::FromHtml("#0B0B0F")

$srcLogo = New-Object System.Drawing.Bitmap([System.Drawing.Image]::FromFile($logoPath))

# --- Opaque-ify the logo: alpha x2.2 so faint antialiased strokes become solid ---
$logo = New-Object System.Drawing.Bitmap($srcLogo.Width, $srcLogo.Height, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
for ($y = 0; $y -lt $srcLogo.Height; $y++) {
    for ($x = 0; $x -lt $srcLogo.Width; $x++) {
        $p = $srcLogo.GetPixel($x, $y)
        if ($p.A -eq 0) { continue }
        $a = [int][math]::Round($p.A * 2.2)
        if ($a -gt 255) { $a = 255 }
        $logo.SetPixel($x, $y, [System.Drawing.Color]::FromArgb($a, $p.R, $p.G, $p.B))
    }
}
$srcLogo.Dispose()

function New-Graphics($bmp) {
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
    return $g
}

function Draw-Logo($g, $size, $scale) {
    $target = [int][math]::Round($size * $scale)
    $ratio = [double]$logo.Width / [double]$logo.Height
    $w = $target
    $h = $target
    if ($ratio -gt 1.0) { $h = [int][math]::Round($target / $ratio) }
    else { $w = [int][math]::Round($target * $ratio) }
    $x = [int][math]::Round(($size - $w) / 2.0)
    $y = [int][math]::Round(($size - $h) / 2.0)
    # Drawn twice: the second pass removes any residual translucency.
    $g.DrawImage($logo, $x, $y, $w, $h)
    $g.DrawImage($logo, $x, $y, $w, $h)
}

function Save-Png($bmp, $path) {
    $dir = Split-Path $path -Parent
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
}

# --- Adaptive foreground (transparent, logo inside the 66% safe zone) ---
$fgSize = 432
$fg = New-Object System.Drawing.Bitmap($fgSize, $fgSize, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$g = New-Graphics $fg
Draw-Logo $g $fgSize 0.58
$g.Dispose()
Save-Png $fg "$res\drawable-nodpi\ic_launcher_fg.png"
$fg.Dispose()

# --- Monochrome layer for Android 13+ themed icons (solid white silhouette) ---
$monoSize = 432
$monoSrc = New-Object System.Drawing.Bitmap($monoSize, $monoSize, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$g = New-Graphics $monoSrc
Draw-Logo $g $monoSize 0.56
$g.Dispose()
$mono = New-Object System.Drawing.Bitmap($monoSize, $monoSize, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
for ($y = 0; $y -lt $monoSize; $y++) {
    for ($x = 0; $x -lt $monoSize; $x++) {
        $p = $monoSrc.GetPixel($x, $y)
        if ($p.A -gt 24) {
            $a = [int][math]::Round($p.A * 1.6)
            if ($a -gt 255) { $a = 255 }
            $mono.SetPixel($x, $y, [System.Drawing.Color]::FromArgb($a, 255, 255, 255))
        }
    }
}
$monoSrc.Dispose()
Save-Png $mono "$res\drawable-nodpi\ic_launcher_mono.png"
$mono.Dispose()

# --- Legacy mipmap icons for launchers without adaptive icon support ---
$densities = @{
    "mipmap-mdpi" = 48
    "mipmap-hdpi" = 72
    "mipmap-xhdpi" = 96
    "mipmap-xxhdpi" = 144
    "mipmap-xxxhdpi" = 192
}

foreach ($d in $densities.Keys) {
    $size = $densities[$d]

    # Squircle variant (28% corner radius, closer to the system mask)
    $bmp = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = New-Graphics $bmp
    $radius = [int][math]::Round($size * 0.28)
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d2 = $radius * 2
    $maxX = $size - $d2
    $maxY = $size - $d2
    $path.AddArc(0, 0, $d2, $d2, 180, 90)
    $path.AddArc($maxX, 0, $d2, $d2, 270, 90)
    $path.AddArc($maxX, $maxY, $d2, $d2, 0, 90)
    $path.AddArc(0, $maxY, $d2, $d2, 90, 90)
    $path.CloseFigure()
    $brush = New-Object System.Drawing.SolidBrush($bgColor)
    $g.FillPath($brush, $path)
    $g.SetClip($path)
    Draw-Logo $g $size 0.64
    $g.Dispose()
    $brush.Dispose()
    $path.Dispose()
    Save-Png $bmp "$res\$d\ic_launcher.png"
    $bmp.Dispose()

    # Fully round variant
    $bmpR = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = New-Graphics $bmpR
    $circle = New-Object System.Drawing.Drawing2D.GraphicsPath
    $circle.AddEllipse(0, 0, $size, $size)
    $brush = New-Object System.Drawing.SolidBrush($bgColor)
    $g.FillPath($brush, $circle)
    $g.SetClip($circle)
    Draw-Logo $g $size 0.60
    $g.Dispose()
    $brush.Dispose()
    $circle.Dispose()
    Save-Png $bmpR "$res\$d\ic_launcher_round.png"
    $bmpR.Dispose()
}

$logo.Dispose()
Write-Output "ICONS_OK"
