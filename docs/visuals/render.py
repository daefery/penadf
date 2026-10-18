"""Render the README visuals into docs/assets/.

    python3 docs/visuals/render.py

The pages in this folder embed real screenshots of the app (docs/assets/screen-*.png,
taken with `cargo build --features screenshot` and EFRAME_SCREENSHOT_TO, using the
fictional sample PDFs from `cargo run --example make_sample`) and of the installer
(docs/assets/install-*.png). Each page is rendered at 2x for sharp text.
"""

from __future__ import annotations

import shutil
import subprocess
from pathlib import Path

HERE = Path(__file__).resolve().parent
ASSETS = HERE.parent / "assets"

#: page -> (width, height) in CSS pixels; must match each page's body size.
PAGES = {
    "hero": (1600, 820),
    "flow": (1600, 600),
    "features": (1600, 900),
    "screens": (1600, 1020),
    "installer": (1600, 640),
}


def main() -> None:
    chrome = shutil.which("google-chrome") or shutil.which("chromium")
    if not chrome:
        raise SystemExit("needs google-chrome or chromium")
    for name, (width, height) in PAGES.items():
        target = ASSETS / f"{name}.png"
        subprocess.run(
            [chrome, "--headless=new", "--disable-gpu", "--hide-scrollbars", "--allow-file-access-from-files",
             "--virtual-time-budget=4000", "--force-device-scale-factor=2", f"--window-size={width},{height}",
             f"--screenshot={target}", f"file://{HERE / (name + '.html')}"],
            check=True, capture_output=True,
        )
        print(target)


if __name__ == "__main__":
    main()
