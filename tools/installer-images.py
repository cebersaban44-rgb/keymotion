"""Draw the installer's images (option A, chosen by the owner on 2026-11-12).

    python tools/installer-images.py

Writes, for tauri.conf.json > bundle > windows > nsis:
  app/src-tauri/windows/images/sidebar.bmp     164x314, the welcome and
                                               finish pages' left image
                                               (sidebarImage)
  app/src-tauri/windows/images/header.bmp      150x57, the top band's right
                                               image (headerImage and
                                               uninstallerHeaderImage)
and, for app/src-tauri/windows/installer-hooks.nsh:
  app/src-tauri/windows/images/sidebar@4x.bmp  656x1256
  app/src-tauri/windows/images/header@4x.bmp   600x228

The first two are the 100% (96 DPI) images NSIS shows as they are. At other
display scales NSIS stretches them to the bigger box without smoothing, which
looks pixelated (K15, 2026-11-12). The installer hooks draw the @4x images
down to the box's real size instead, smoothly; the hooks carry these sizes
(kmSidebar4x, kmHeader4x), so keep them in step.

The logo is app/public/app-icon.svg redrawn: a dark rounded square, the "K"
in two light strokes and one orange stroke. The text is language-neutral:
the same images serve the English and the Turkish installer. Needs Pillow
and the Segoe UI Semibold font that ships with Windows. Drawn four times
larger than the output and scaled down, so edges are smooth.
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


class Pen:
    """Draws in 100% units (px at 96 DPI) on a canvas `scale` times larger."""

    def __init__(self, w: int, h: int, scale: int, bg):
        self.w, self.h, self.scale = w, h, scale
        self.z = S * scale
        self.img = Image.new("RGB", (w * self.z, h * self.z), bg)
        self.d = ImageDraw.Draw(self.img)

    def font(self, name: str, size: float) -> ImageFont.FreeTypeFont:
        return ImageFont.truetype(str(FONTS / name), round(size * self.z))

    def stroke(self, a, b, width: float, color) -> None:
        """A line with round ends (as the SVG's stroke-linecap="round")."""
        self.d.line([a, b], fill=color, width=round(width))
        r = width / 2
        for x, y in (a, b):
            self.d.ellipse([x - r, y - r, x + r, y + r], fill=color)

    def logo(self, x: float, y: float, size: float, edge: bool) -> None:
        """The app icon at (x, y), `size` wide."""
        k = size * self.z / 100
        ox, oy = x * self.z, y * self.z
        p = lambda px, py: (ox + px * k, oy + py * k)  # noqa: E731
        self.d.rounded_rectangle(
            [p(0, 0), p(100, 100)],
            radius=22 * k,
            fill=TILE,
            outline=TILE_EDGE if edge else None,
            width=max(1, round(2 * k)) if edge else 0,
        )
        w = 13 * k
        self.stroke(p(34, 25), p(34, 75), w, INK)
        self.stroke(p(46, 51), p(68, 27), w, INK)
        self.stroke(p(51, 57), p(69, 75), w, SIGNAL)

    def wordmark(self, x: float, baseline: float, size: float, color, centre: bool = False) -> None:
        """"key" in `color`, "motion" in orange, on one baseline."""
        f = self.font("seguisb.ttf", size)
        w1 = self.d.textlength("key", font=f)
        w2 = self.d.textlength("motion", font=f)
        left = x * self.z - (w1 + w2) / 2 if centre else x * self.z
        self.d.text((left, baseline * self.z), "key", font=f, fill=color, anchor="ls")
        self.d.text((left + w1, baseline * self.z), "motion", font=f, fill=SIGNAL, anchor="ls")

    def text(self, x: float, baseline: float, s: str, name: str, size: float, color) -> None:
        f = self.font(name, size)
        self.d.text((x * self.z, baseline * self.z), s, font=f, fill=color, anchor="ms")

    def save(self, name: str) -> None:
        OUT.mkdir(parents=True, exist_ok=True)
        w, h = self.w * self.scale, self.h * self.scale
        self.img.resize((w, h), Image.LANCZOS).save(OUT / name, "BMP")
        print(f"wrote {OUT / name} ({w}x{h})")


def sidebar(scale: int, name: str) -> None:
    w, h = 164, 314
    pen = Pen(w, h, scale, BG)
    pen.logo(42, 96, 80, edge=True)
    pen.wordmark(w / 2, 214, 20, INK, centre=True)
    pen.text(w / 2, 236, "AJAZZ AK980 PRO", "segoeui.ttf", 10, MUTED)
    pen.save(name)


def header(scale: int, name: str) -> None:
    pen = Pen(150, 57, scale, (255, 255, 255))
    pen.logo(14, 12, 33, edge=False)
    pen.wordmark(54, 35, 15, INK_DARK)
    pen.save(name)


if __name__ == "__main__":
    sidebar(1, "sidebar.bmp")
    header(1, "header.bmp")
    sidebar(4, "sidebar@4x.bmp")
    header(4, "header@4x.bmp")
