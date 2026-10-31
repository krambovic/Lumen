"""Render the Windows icon from Android's shared Light launcher geometry."""
from pathlib import Path
import runpy

from PIL import Image, ImageDraw


def main() -> None:
    root = Path(__file__).resolve().parents[2]
    android = runpy.run_path(str(root / "android/tools/gen_launcher_icons.py"))
    # Balance the L optically: its stem and foot concentrate visual weight
    # toward the lower left, so move it slightly up and right.
    size = 1024
    icon = android["background_image"](size, android["LIGHT"])
    points = [(x + size * 0.025, y - size * 0.025)
              for x, y in android["placed"](size, size * 0.72)]
    ImageDraw.Draw(icon).polygon(points, fill=(0, 0, 0, 255))
    mask = Image.new("L", icon.size, 0)
    ImageDraw.Draw(mask).ellipse((0, 0, size - 1, size - 1), fill=255)
    icon.putalpha(mask)
    icon.save(root / "windows/assets/Lumen.png")
    icon = icon.resize((256, 256), Image.Resampling.LANCZOS)
    icon.save(root / "windows/assets/Lumen.ico", sizes=[(s, s) for s in (16, 20, 24, 32, 40, 48, 64, 128, 256)])


if __name__ == "__main__":
    main()
