"""Draw the installer's images (option A, chosen by the owner on 2026-11-12).

    python tools/installer-images.py

Writes, for tauri.conf.json > bundle > windows > nsis:
  app/src-tauri/windows/images/sidebar.bmp  164x314, the welcome and finish
                                            pages' left image (sidebarImage)
  app/src-tauri/windows/images/header.bmp   150x57, the top band's right
                                            image (headerImage and
                                            uninstallerHeaderImage)

The logo is app/public/app-icon.svg redrawn: a dark rounded square, the "K"
in two light strokes and one orange stroke. The text is language-neutral:
the same images serve the English and the Turkish installer. Needs Pillow
and the Segoe UI Semibold font that ships with Windows. Drawn four times
larger and scaled down, so edges are smooth.
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "app" / "src-tauri" / "windows" / "images"
FONTS = Path("C:/Windows/Fonts")
S = 4  # supersampling

BG = (13, 14, 16)  # --bg (dark)
TILE = (22, 24, 28)  # the logo's square
TILE_EDGE = (42, 45, 51)
INK = (236, 233, 227)  # --ink (dark)
INK_DARK = (22, 24, 28)
SIGNAL = (255, 138, 61)  # --signal
MUTED = (139, 138, 134)


def font(name: str, size: float) -> ImageFont.FreeTypeFont:
    return ImageFont.truetype(str(FONTS / name), round(size * S))


def stroke(d: ImageDraw.ImageDraw, a, b, width: float, color) -> None:
    """A line with round ends (as the SVG's stroke-linecap="round")."""
    d.line([a, b], fill=color, width=round(width))
    r = width / 2
    for x, y in (a, b):
        d.ellipse([x - r, y - r, x + r, y + r], fill=color)


def logo(d: ImageDraw.ImageDraw, x: float, y: float, size: float, edge: bool) -> None:
    """The app icon at (x, y), `size` px wide (1x units)."""
    k = size * S / 100
    ox, oy = x * S, y * S
    p = lambda px, py: (ox + px * k, oy + py * k)  # noqa: E731
    d.rounded_rectangle(
        [p(0, 0), p(100, 100)],
        radius=22 * k,
        fill=TILE,
        outline=TILE_EDGE if edge else None,
        width=max(1, round(2 * k)) if edge else 0,
    )
    w = 13 * k
    stroke(d, p(34, 25), p(34, 75), w, INK)
    stroke(d, p(46, 51), p(68, 27), w, INK)
    stroke(d, p(51, 57), p(69, 75), w, SIGNAL)


def wordmark(d: ImageDraw.ImageDraw, x: float, baseline: float, size: float, color, centre: bool = False) -> None:
    """"key" in `color`, "motion" in orange, on one baseline."""
    f = font("seguisb.ttf", size)
    w1 = d.textlength("key", font=f)
    w2 = d.textlength("motion", font=f)
    left = x * S - (w1 + w2) / 2 if centre else x * S
    d.text((left, baseline * S), "key", font=f, fill=color, anchor="ls")
    d.text((left + w1, baseline * S), "motion", font=f, fill=SIGNAL, anchor="ls")


def canvas(w: int, h: int, bg):
    img = Image.new("RGB", (w * S, h * S), bg)
    return img, ImageDraw.Draw(img)


def save(img: Image.Image, w: int, h: int, name: str) -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    img.resize((w, h), Image.LANCZOS).save(OUT / name, "BMP")
    print(f"wrote {OUT / name} ({w}x{h})")


def sidebar() -> None:
    w, h = 164, 314
    img, d = canvas(w, h, BG)
    logo(d, 42, 96, 80, edge=True)
    wordmark(d, w / 2, 214, 20, INK, centre=True)
    f = font("segoeui.ttf", 10)
    d.text((w / 2 * S, 236 * S), "AJAZZ AK980 PRO", font=f, fill=MUTED, anchor="ms")
    save(img, w, h, "sidebar.bmp")


def header() -> None:
    w, h = 150, 57
    img, d = canvas(w, h, (255, 255, 255))
    logo(d, 14, 12, 33, edge=False)
    wordmark(d, 54, 35, 15, INK_DARK)
    save(img, w, h, "header.bmp")


if __name__ == "__main__":
    sidebar()
    header()
