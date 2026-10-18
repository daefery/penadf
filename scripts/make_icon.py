"""Builds the PenaDF icon (direction A: page with folded corner, red pen nib, ink signature).

Writes:
  assets/penadf.svg        master artwork (256 x 256)
  assets/penadf-256.png    preview / README
  assets/penadf.ico        Windows icon, 16-256 px
  assets/penadf-file.svg   (reserved for a future PDF file-type icon)

Small sizes are drawn separately, not shrunk: at 16-32 px the signature becomes two
clean strokes and the outline thickens, so the icon stays readable in Explorer.

Run: uv run --with cairosvg --with pillow python scripts/make_icon.py
"""
import io
import os

import cairosvg
from PIL import Image

RED, RED_D, INK, PAPER, EDGE, FOLD = "#C41E3A", "#9E1730", "#1E2230", "#FFFFFF", "#8A8F9C", "#E4E6EA"


def nib(fill, cut, hole):
    # Fountain-pen nib pointing up with its tip at (0, 0); rotated into place by the caller.
    return f"""
    <path d="M0 0 L-30 -62 Q-34 -76 -24 -88 L-24 -118 H24 L24 -88 Q34 -76 30 -62 Z" fill="{fill}"/>
    <path d="M0 -4 V-60" stroke="{cut}" stroke-width="6" stroke-linecap="round"/>
    <circle cx="0" cy="-64" r="9" fill="{hole}"/>
    <rect x="-24" y="-118" width="48" height="14" fill="{cut}" opacity=".35"/>"""


def full(edge_w=7, sig_w=12):
    """The detailed icon, used for 48 px and up."""
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">
  <path d="M40 16 H150 L204 70 V230 a10 10 0 0 1 -10 10 H40 a10 10 0 0 1 -10 -10 V26 a10 10 0 0 1 10 -10 Z" fill="{PAPER}" stroke="{EDGE}" stroke-width="{edge_w}"/>
  <path d="M150 16 V60 a10 10 0 0 0 10 10 H204" fill="{FOLD}" stroke="{EDGE}" stroke-width="{edge_w}" stroke-linejoin="round"/>
  <path d="M56 196 c12 -26 24 -30 28 -18 s-6 26 6 18 s18 -32 30 -20 s4 18 18 10" fill="none" stroke="{INK}" stroke-width="{sig_w}" stroke-linecap="round" stroke-linejoin="round"/>
  <g transform="translate(148 190) rotate(38) scale(.95)">{nib(RED, RED_D, PAPER)}</g>
</svg>"""


def small():
    """Simplified for 16-32 px: thick outline, one bold stroke, a solid nib without detail."""
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">
  <path d="M40 18 H146 L202 74 V226 a12 12 0 0 1 -12 12 H40 a12 12 0 0 1 -12 -12 V30 a12 12 0 0 1 12 -12 Z" fill="{PAPER}" stroke="#6E7382" stroke-width="18"/>
  <path d="M146 18 V62 a12 12 0 0 0 12 12 H202" fill="{FOLD}" stroke="#6E7382" stroke-width="18" stroke-linejoin="round"/>
  <path d="M58 190 c16 -34 34 -34 36 -10 s22 -30 40 0" fill="none" stroke="{INK}" stroke-width="24" stroke-linecap="round" stroke-linejoin="round"/>
  <g transform="translate(150 196) rotate(38) scale(1.05)">
    <path d="M0 0 L-32 -64 Q-36 -78 -26 -90 L-26 -120 H26 L26 -90 Q36 -78 32 -64 Z" fill="{RED}"/>
  </g>
</svg>"""


def png(svg, size):
    data = cairosvg.svg2png(bytestring=svg.encode(), output_width=size, output_height=size)
    return Image.open(io.BytesIO(data)).convert("RGBA")


os.makedirs("assets", exist_ok=True)
master = full()
open("assets/penadf.svg", "w").write(master)
png(master, 256).save("assets/penadf-256.png")

frames = {16: png(small(), 16), 24: png(small(), 24), 32: png(small(), 32)}
for s in (48, 64, 128, 256):
    frames[s] = png(master, s)
sizes = sorted(frames)
frames[256].save("assets/penadf.ico", sizes=[(s, s) for s in sizes], append_images=[frames[s] for s in sizes if s != 256])
for s in (16, 32):  # previews for review
    frames[s].save(f"assets/icon-options/final-{s}.png")
print("wrote assets/penadf.svg, assets/penadf-256.png, assets/penadf.ico", sizes)

# Installer side panel for the Welcome and Finish pages (NSIS wants 164 x 314, 24-bit BMP).
panel = Image.new("RGB", (164, 314), "#F4F2EC")
logo = png(master, 112)
panel.paste(logo, (26, 72), logo)
from PIL import ImageDraw, ImageFont
d = ImageDraw.Draw(panel)
for path in ("/mnt/c/Windows/Fonts/segoeuib.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"):
    if os.path.exists(path):
        d.text((82, 214), "PenaDF", fill=INK, font=ImageFont.truetype(path, 24), anchor="mm")
        break
d.rectangle((0, 300, 164, 314), fill=RED)
panel.save("installer/wizard.bmp")
print("wrote installer/wizard.bmp")

# Window/taskbar icon embedded in the app at build time (raw 64x64 RGBA, see src/main.rs).
open("assets/penadf-64.rgba", "wb").write(png(master, 64).tobytes())
print("wrote assets/penadf-64.rgba")

# MSIX / Microsoft Store tiles (packaging/msix/Assets). Logos sit on a transparent background
# with padding, as Windows' Start menu and taskbar expect.
def tile(size, logo_frac, w=None):
    w = w or size
    canvas = Image.new("RGBA", (w, size), (0, 0, 0, 0))
    s = int(size * logo_frac)
    art = png(small() if s <= 32 else master, s)
    canvas.alpha_composite(art, ((w - s) // 2, (size - s) // 2))
    return canvas

os.makedirs("packaging/msix/Assets", exist_ok=True)
tile(50, 0.9).save("packaging/msix/Assets/StoreLogo.png")
tile(44, 0.92).save("packaging/msix/Assets/Square44x44Logo.png")
tile(150, 0.66).save("packaging/msix/Assets/Square150x150Logo.png")
tile(150, 0.66, w=310).save("packaging/msix/Assets/Wide310x150Logo.png")
tile(44, 0.92).save("packaging/msix/Assets/FileLogo.png")
# Unplated taskbar icons (no tile plate behind them) at the scales Windows asks for.
for s in (16, 24, 32, 48, 256):
    tile(s, 1.0).save(f"packaging/msix/Assets/Square44x44Logo.targetsize-{s}_altform-unplated.png")
print("wrote packaging/msix/Assets")

# macOS app icon (packaging/macos/PenaDF.icns). Apple's icon grid: the artwork sits inside
# roughly 80% of the canvas. Pillow writes all the sizes macOS needs from one 1024 image.
mac = Image.new("RGBA", (1024, 1024), (0, 0, 0, 0))
art = png(master, 824)
mac.alpha_composite(art, (100, 100))
os.makedirs("packaging/macos", exist_ok=True)
mac.save("packaging/macos/PenaDF.icns", format="ICNS")
print("wrote packaging/macos/PenaDF.icns")
