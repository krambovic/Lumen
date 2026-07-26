# One reproducible launcher-icon pipeline. The generator owns every launcher
# resource - adaptive layers, themed layer and the legacy density PNGs - and
# refuses to emit a mark that leaves the 66dp adaptive-icon safe zone.
$generator = Join-Path $PSScriptRoot "tools\gen_launcher_icons.py"
python $generator
if ($LASTEXITCODE -ne 0) {
    throw "Launcher icon generation failed with exit code $LASTEXITCODE"
}
