"""Regenerate every Android launcher asset from the desktop Lumen.ico source.

Run: python android/tools/gen_launcher_icons.py
"""
import os
from PIL import Image, ImageDraw

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SRC = os.path.join(ROOT, "windows", "assets", "Lumen.ico")
RES = os.path.join(ROOT, "android", "app", "src", "main", "res")
BG = (11, 11, 15, 255)


def load_logo():
    im = Image.open(SRC)
    im.size = (256, 256)
    im = im.convert("RGBA")
    # Trim transparent padding so the glyph can be centered precisely.
    box = im.getbbox()
    return im.crop(box) if box else im


def fit(logo, canvas, ratio):
    target = int(canvas * ratio)
    w, h = logo.size
    scale = min(target / w, target / h)
    resized = logo.resize((max(1, int(w * scale)), max(1, int(h * scale))), Image.LANCZOS)
    out = Image.new("RGBA", (canvas, canvas), (0, 0, 0, 0))
    out.paste(resized, ((canvas - resized.width) // 2, (canvas - resized.height) // 2), resized)
    return out


def save(img, *parts):
    path = os.path.join(RES, *parts)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    img.save(path, "PNG")
    print("wrote", path)


def main():
    logo = load_logo()

    # Adaptive icon foreground: 108dp canvas, glyph inside the 66dp safe zone.
    fg = fit(logo, 432, 0.56)
    save(fg, "drawable-nodpi", "ic_launcher_fg.png")

    # Monochrome layer: flat white silhouette taken from the glyph alpha.
    alpha = fg.split()[3]
    mono = Image.new("RGBA", fg.size, (255, 255, 255, 255))
    mono.putalpha(alpha)
    save(mono, "drawable-nodpi", "ic_launcher_mono.png")

    # Legacy raster launcher icons for pre-O devices.
    for folder, px in (("mipmap-mdpi", 48), ("mipmap-hdpi", 72), ("mipmap-xhdpi", 96),
                       ("mipmap-xxhdpi", 144), ("mipmap-xxxhdpi", 192)):
        glyph = fit(logo, px, 0.62)
        plate = Image.new("RGBA", (px, px), BG)

        square = Image.new("RGBA", (px, px), (0, 0, 0, 0))
        mask = Image.new("L", (px, px), 0)
        ImageDraw.Draw(mask).rounded_rectangle([0, 0, px - 1, px - 1], radius=int(px * 0.22), fill=255)
        square.paste(plate, (0, 0), mask)
        square.alpha_composite(glyph)
        save(square, folder, "ic_launcher.png")

        round_icon = Image.new("RGBA", (px, px), (0, 0, 0, 0))
        rmask = Image.new("L", (px, px), 0)
        ImageDraw.Draw(rmask).ellipse([0, 0, px - 1, px - 1], fill=255)
        round_icon.paste(plate, (0, 0), rmask)
        round_icon.alpha_composite(glyph)
        save(round_icon, folder, "ic_launcher_round.png")

    # In-app logo copy kept in sync with the launcher glyph.
    save(fit(logo, 192, 0.9), "drawable", "ic_launcher.png")


if __name__ == "__main__":
    main()
