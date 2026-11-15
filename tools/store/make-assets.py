"""Draw the Microsoft Store package's logos from the app icon.

    python tools/store/make-assets.py

Reads app/src-tauri/icons/icon.png (512x512) and writes, into
app/src-tauri/store/Assets, every size Windows asks of an MSIX
(store/AppxManifest.xml names the base files; makepri, run by
tools/store/msix.mjs, lets Windows pick the size it needs):

  Square44x44Logo   taskbar, Start's app list, the title bar: scale-100…400
                    and targetsize-16…256, each also "altform-unplated"
                    (Windows 11 shows the unplated one, on no colour)
  Square150x150Logo Start's tile: scale-100…200 (400 would need 600 px, more
                    than the icon has; Windows then takes the 200)
  StoreLogo         the Store and the installer: scale-100…400

Run again when the icon changes. Needs Pillow. The images are scaled down
from the 512 pixel icon with a smooth filter, never up.
"""

from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "app" / "src-tauri" / "icons" / "icon.png"
OUT = ROOT / "app" / "src-tauri" / "store" / "Assets"

SCALES = (100, 125, 150, 200, 400)
TARGET_SIZES = (16, 24, 32, 48, 256)
BASES = {"Square44x44Logo": 44, "Square150x150Logo": 150, "StoreLogo": 50}


def scaled(icon: Image.Image, size: int) -> Image.Image:
    if size > icon.width:
        raise SystemExit(f"{size} px is larger than the {icon.width} px icon")
    return icon.resize((size, size), Image.LANCZOS)


def main() -> None:
    icon = Image.open(SOURCE).convert("RGBA")
    OUT.mkdir(parents=True, exist_ok=True)
    for old in OUT.glob("*.png"):
        old.unlink()
    written = 0
    for name, base in BASES.items():
        for scale in SCALES:
            size = round(base * scale / 100)
            if size > icon.width:
                continue
            scaled(icon, size).save(OUT / f"{name}.scale-{scale}.png", optimize=True)
            written += 1
    for size in TARGET_SIZES:
        image = scaled(icon, size)
        image.save(OUT / f"Square44x44Logo.targetsize-{size}.png", optimize=True)
        image.save(OUT / f"Square44x44Logo.targetsize-{size}_altform-unplated.png", optimize=True)
        written += 2
    print(f"{written} images in {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
