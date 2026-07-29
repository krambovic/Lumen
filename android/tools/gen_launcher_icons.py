"""Generate every Android launcher resource from one geometry definition.

The Lumen mark is a six-point polygon traced from the approved artwork
(tools/assets/lumen_launcher_source.png). Everything below - the adaptive
foreground, the monochrome silhouette, the background plate and the legacy
density PNGs - is derived from it, so the layers can never drift apart.

Adaptive-icon rule enforced here: the 108dp canvas is masked down to 72dp and
only the central 66dp is safe, so the mark lives in the FOREGROUND, inside that
66dp circle, and the BACKGROUND is a plain full-bleed gradient. Putting artwork
in the background layer is what made the icon crop differently on every
launcher.

The adaptive layers are emitted twice, in res/drawable and res/drawable-night,
so the icon follows the system theme. The legacy density PNGs cannot theme
themselves (pre-API-26 launchers have no night qualifier for mipmaps here), so
they keep the light variant.

On top of that auto-theming `ic_launcher`, two FIXED variants are emitted -
`ic_launcher_light` and `ic_launcher_dark` - from the same geometry and the same
two palettes. They live only in res/drawable (no -night twin), so they never
re-theme themselves; the launcher-icon picker points an <activity-alias> at one
of them and the user's choice sticks. Everything comes out of this one file, so
the three icons cannot drift apart.

Run from any directory: python android/tools/gen_launcher_icons.py
"""
from pathlib import Path
from math import hypot
from typing import NamedTuple, Optional

from PIL import Image, ImageDraw


ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "android" / "tools" / "assets" / "lumen_launcher_source.png"
RES = ROOT / "android" / "app" / "src" / "main" / "res"

# Mark traced from the approved artwork, in source-image pixels.
SOURCE_CANVAS = 1254.0
GLYPH = [
    (490.5, 211.5),    # top of the stem
    (490.5, 871.0),    # stem meets the bar
    (1016.5, 871.0),   # bar top right
    (893.5, 1023.5),   # bar chamfer
    (345.0, 1023.5),   # bar bottom left
    (345.0, 361.0),    # stem chamfer
]

CANVAS_DP = 108.0      # adaptive icon canvas
MASK_DP = 72.0         # what the launcher mask keeps
SAFE_DP = 66.0         # what the launcher mask guarantees
VIEWPORT = 1024.0      # vector drawable units
GLYPH_HEIGHT_DP = 50.0 # mark height on the 108dp canvas


# Only the plate and the mark colour change between themes; the geometry is
# shared. The light variant goes to res/drawable, the dark one to
# res/drawable-night, so the launcher follows the system theme.
class Theme(NamedTuple):
    folder: str
    bg_top: str
    bg_bottom: str
    glyph: str
    diagonal_top: Optional[str]
    diagonal_bottom: Optional[str]


# White plate, black mark. Both stops are the same colour, so the plate is flat.
LIGHT = Theme("drawable", "#FFFFFFFF", "#FFFFFFFF", "#FF000000", None, None)
# Dark plate from the approved artwork: white mark over a black vertical
# gradient with the characteristic lighter diagonal facet on the right.
DARK = Theme(
    "drawable-night",
    "#FF212121",
    "#FF0B0B0B",
    "#FFFFFFFF",
    "#FF2A2A2A",
    "#FF111111",
)
THEMES = (LIGHT, DARK)


# A launcher-icon variant the user can pin from Customization. Both land in
# res/drawable with no -night twin, so the qualifier can never re-theme them:
# picking "Light" has to stay light on a device in dark mode.
class Fixed(NamedTuple):
    name: str     # resource base name, e.g. "ic_launcher_light"
    theme: Theme  # colours only; the folder of the Theme is ignored here


FIXED = (
    Fixed("ic_launcher_light", LIGHT),
    # Same palette as the -night variant, but parked in the unqualified folder.
    Fixed(
        "ic_launcher_dark",
        Theme(
            "drawable",
            DARK.bg_top,
            DARK.bg_bottom,
            DARK.glyph,
            DARK.diagonal_top,
            DARK.diagonal_bottom,
        ),
    ),
)

LEGACY_DENSITIES = {
    "mipmap-mdpi": 48,
    "mipmap-hdpi": 72,
    "mipmap-xhdpi": 96,
    "mipmap-xxhdpi": 144,
    "mipmap-xxxhdpi": 192,
}


def rgb(color: str) -> tuple:
    """#AARRGGBB -> (r, g, b)."""
    return tuple(int(color[i:i + 2], 16) for i in (3, 5, 7))


def glyph_extent() -> tuple:
    """Width and height of the traced mark, in source pixels."""
    xs = [p[0] for p in GLYPH]
    ys = [p[1] for p in GLYPH]
    return max(xs) - min(xs), max(ys) - min(ys)


def placed(size: float, height: float) -> list:
    """Place the mark centred in a square of `size`, scaled to `height`."""
    xs = [p[0] for p in GLYPH]
    ys = [p[1] for p in GLYPH]
    width, source_height = glyph_extent()
    scale = height / source_height
    left = (size - width * scale) / 2.0
    top = (size - height) / 2.0
    return [
        (left + (x - min(xs)) * scale, top + (y - min(ys)) * scale)
        for x, y in GLYPH
    ]


def check_safe_zone() -> None:
    """The mark must fit the 66dp safe circle, whatever mask a launcher uses."""
    width, height = glyph_extent()
    width_dp = GLYPH_HEIGHT_DP * width / height
    diagonal = hypot(width_dp, GLYPH_HEIGHT_DP)
    if diagonal > SAFE_DP:
        raise ValueError(
            f"mark is {width_dp:.2f}x{GLYPH_HEIGHT_DP:.2f}dp, diagonal "
            f"{diagonal:.2f}dp exceeds the {SAFE_DP:.0f}dp safe zone"
        )
    print(
        f"mark {width_dp:.2f}x{GLYPH_HEIGHT_DP:.2f}dp on a {CANVAS_DP:.0f}dp "
        f"canvas, diagonal {diagonal:.2f}dp <= {SAFE_DP:.0f}dp safe zone"
    )


def verify_trace() -> None:
    """Cheap guard that GLYPH still matches the approved artwork."""
    if not SOURCE.is_file():
        return
    image = Image.open(SOURCE).convert("L")
    if image.width != image.height or image.width != int(SOURCE_CANVAS):
        raise ValueError(f"artwork must be {SOURCE_CANVAS:.0f}px square, got {image.size}")
    mask = image.point(lambda v: 255 if v > 180 else 0).getbbox()
    xs = [p[0] for p in GLYPH]
    ys = [p[1] for p in GLYPH]
    traced = (min(xs), min(ys), max(xs), max(ys))
    if max(abs(a - b) for a, b in zip(mask, traced)) > 2.0:
        raise ValueError(f"GLYPH {traced} no longer matches artwork {mask}")


def path_data(points: list) -> str:
    head = f"M{points[0][0]:.2f},{points[0][1]:.2f}"
    tail = "".join(f" L{x:.2f},{y:.2f}" for x, y in points[1:])
    return f"{head}{tail} Z"


def write(text: str, folder: str, name: str) -> None:
    destination = RES / folder / name
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(text, encoding="utf-8", newline="\n")
    print("wrote", destination)


def save(image: Image.Image, folder: str, name: str) -> None:
    destination = RES / folder / name
    destination.parent.mkdir(parents=True, exist_ok=True)
    image.save(destination, "PNG", optimize=True)
    print("wrote", destination)


def verify_dark_outputs() -> None:
    """Every place that means "dark launcher icon" must use the approved artwork."""
    equivalent_layers = (
        (
            RES / "drawable-night" / "ic_launcher_background.xml",
            RES / "drawable" / "ic_launcher_dark_background.xml",
        ),
        (
            RES / "drawable-night" / "ic_launcher_foreground.xml",
            RES / "drawable" / "ic_launcher_dark_foreground.xml",
        ),
    )
    for automatic, fixed in equivalent_layers:
        if automatic.read_bytes() != fixed.read_bytes():
            raise ValueError(f"dark icon layers drifted apart: {automatic} != {fixed}")

    for folder, pixels in LEGACY_DENSITIES.items():
        for suffix in ("", "_round"):
            path = RES / folder / f"ic_launcher_dark{suffix}.png"
            with Image.open(path) as image:
                if image.size != (pixels, pixels):
                    raise ValueError(
                        f"{path} is {image.size}, expected {(pixels, pixels)}"
                    )
    print("verified one dark artwork across system-night, fixed, adaptive and legacy icons")


def vector(comment: str, body: str, aapt: bool = False) -> str:
    namespace = '\n    xmlns:aapt="http://schemas.android.com/aapt"' if aapt else ""
    return (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        f"<!-- {comment} -->\n"
        '<vector xmlns:android="http://schemas.android.com/apk/res/android"'
        f"{namespace}\n"
        f'    android:width="{CANVAS_DP:.0f}dp"\n'
        f'    android:height="{CANVAS_DP:.0f}dp"\n'
        f'    android:viewportWidth="{VIEWPORT:.0f}"\n'
        f'    android:viewportHeight="{VIEWPORT:.0f}">\n'
        f"{body}"
        "</vector>\n"
    )


def adaptive_icon(base: str = "ic_launcher") -> str:
    # The monochrome layer is shared: the system tints it, so it is palette-free.
    return (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">\n'
        f'    <background android:drawable="@drawable/{base}_background" />\n'
        f'    <foreground android:drawable="@drawable/{base}_foreground" />\n'
        '    <monochrome android:drawable="@drawable/ic_launcher_monochrome" />\n'
        "</adaptive-icon>\n"
    )


def background_layer(theme: Theme) -> str:
    diagonal = ""
    if theme.diagonal_top is not None and theme.diagonal_bottom is not None:
        diagonal = (
            '    <path android:pathData="M870,0 L1024,0 L1024,1024 L348,1024 Z">\n'
            '        <aapt:attr name="android:fillColor">\n'
            '            <gradient\n'
            '                android:type="linear"\n'
            '                android:startX="870"\n'
            '                android:startY="0"\n'
            '                android:endX="348"\n'
            '                android:endY="1024">\n'
            f'                <item android:offset="0" android:color="{theme.diagonal_top}" />\n'
            f'                <item android:offset="1" android:color="{theme.diagonal_bottom}" />\n'
            "            </gradient>\n"
            "        </aapt:attr>\n"
            "    </path>\n"
        )
    return vector(
        "Full-bleed plate. No artwork here: the launcher mask crops this layer.",
        '    <path android:pathData="M0,0h1024v1024h-1024z">\n'
        '        <aapt:attr name="android:fillColor">\n'
        '            <gradient\n'
        '                android:type="linear"\n'
        '                android:startX="512"\n'
        '                android:startY="0"\n'
        '                android:endX="512"\n'
        '                android:endY="1024">\n'
        f'                <item android:offset="0" android:color="{theme.bg_top}" />\n'
        f'                <item android:offset="1" android:color="{theme.bg_bottom}" />\n'
        "            </gradient>\n"
        "        </aapt:attr>\n"
        "    </path>\n"
        + diagonal,
        aapt=True,
    )


def foreground_layer(theme: Theme, mark: str) -> str:
    return vector(
        "Lumen mark, centred inside the 66dp safe zone of the 108dp canvas.",
        '    <path\n'
        f'        android:fillColor="{theme.glyph}"\n'
        f'        android:pathData="{mark}" />\n',
    )


def vertical_gradient(size: int, top_color: str, bottom_color: str) -> Image.Image:
    top, bottom = rgb(top_color), rgb(bottom_color)
    image = Image.new("RGBA", (1, size))
    pixels = image.load()
    for y in range(size):
        t = y / max(size - 1, 1)
        pixels[0, y] = tuple(
            int(round(a + (b - a) * t)) for a, b in zip(top, bottom)
        ) + (255,)
    return image.resize((size, size), Image.Resampling.NEAREST)


def background_image(size: int, theme: Theme) -> Image.Image:
    image = vertical_gradient(size, theme.bg_top, theme.bg_bottom)
    if theme.diagonal_top is None or theme.diagonal_bottom is None:
        return image

    facet = vertical_gradient(size, theme.diagonal_top, theme.diagonal_bottom)
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).polygon(
        [
            (size * 870 / VIEWPORT, 0),
            (size, 0),
            (size, size),
            (size * 348 / VIEWPORT, size),
        ],
        fill=255,
    )
    image.paste(facet, (0, 0), mask)
    return image


def legacy(size: int, round_mask: bool, theme: Theme) -> Image.Image:
    scale = 4
    large = size * scale
    image = background_image(large, theme)
    if round_mask:
        mask = Image.new("L", image.size, 0)
        ImageDraw.Draw(mask).ellipse((0, 0, large - 1, large - 1), fill=255)
        image.putalpha(mask)
    # Same proportion as the adaptive icon: the mark fills GLYPH_HEIGHT_DP of
    # the MASK_DP the launcher actually shows.
    height = large * GLYPH_HEIGHT_DP / MASK_DP
    ImageDraw.Draw(image).polygon(placed(large, height), fill=rgb(theme.glyph) + (255,))
    return image.resize((size, size), Image.Resampling.LANCZOS)


def main() -> None:
    check_safe_zone()
    verify_trace()

    mark = path_data(placed(VIEWPORT, GLYPH_HEIGHT_DP * VIEWPORT / CANVAS_DP))

    for theme in THEMES:
        write(background_layer(theme), theme.folder, "ic_launcher_background.xml")
        write(foreground_layer(theme, mark), theme.folder, "ic_launcher_foreground.xml")
        print(
            f"{theme.folder}: plate {theme.bg_top} -> {theme.bg_bottom}, "
            f"mark {theme.glyph}"
        )

    write(
        vector(
            "Themed-icon layer: the same mark as a flat silhouette, tinted by the system.",
            '    <path\n'
            '        android:fillColor="#FF000000"\n'
            f'        android:pathData="{mark}" />\n',
        ),
        LIGHT.folder,
        "ic_launcher_monochrome.xml",
    )
    write(adaptive_icon(), "mipmap-anydpi-v26", "ic_launcher.xml")
    write(adaptive_icon(), "mipmap-anydpi-v26", "ic_launcher_round.xml")

    # Pre-API-26 launchers draw these bitmaps unthemed, so they stay light.
    for folder, pixels in LEGACY_DENSITIES.items():
        save(legacy(pixels, False, LIGHT), folder, "ic_launcher.png")
        save(legacy(pixels, True, LIGHT), folder, "ic_launcher_round.png")

    # The two variants an <activity-alias> can pin. Identical geometry, one of
    # the two palettes, and no -night twin, so the user's pick never flips.
    for fixed in FIXED:
        write(background_layer(fixed.theme), "drawable", f"{fixed.name}_background.xml")
        write(foreground_layer(fixed.theme, mark), "drawable", f"{fixed.name}_foreground.xml")
        write(adaptive_icon(fixed.name), "mipmap-anydpi-v26", f"{fixed.name}.xml")
        write(adaptive_icon(fixed.name), "mipmap-anydpi-v26", f"{fixed.name}_round.xml")
        for folder, pixels in LEGACY_DENSITIES.items():
            save(legacy(pixels, False, fixed.theme), folder, f"{fixed.name}.png")
            save(legacy(pixels, True, fixed.theme), folder, f"{fixed.name}_round.png")
        print(
            f"fixed {fixed.name}: plate {fixed.theme.bg_top} -> "
            f"{fixed.theme.bg_bottom}, mark {fixed.theme.glyph}"
        )

    verify_dark_outputs()

    stale = RES / "drawable-nodpi" / "ic_launcher_artwork.png"
    if stale.is_file():
        stale.unlink()
        print("removed", stale)
    folder = stale.parent
    if folder.is_dir() and not any(folder.iterdir()):
        folder.rmdir()
        print("removed", folder)


if __name__ == "__main__":
    main()
