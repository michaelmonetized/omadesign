#!/usr/bin/env python3
"""Derive app branding from Michael's official SVG without changing its artwork.

The wordmark viewport trims empty padding around the full rings and lettering.
The original square export remains the desktop icon and the source of truth.
"""
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
source = (ROOT / "media/logo.svg").read_text()
match = re.search(r"<svg\b[^>]*>", source)
if match is None:
    raise SystemExit("Official logo has no SVG root")
opening = match.group()
for name, value in [("width", "1920"), ("height", "1070"), ("viewBox", "40 465 1920 1070")]:
    opening, count = re.subn(rf'\b{name}="[^"]*"', f'{name}="{value}"', opening)
    if count != 1:
        raise SystemExit(f"Official logo must contain one {name} attribute")
(ROOT / "assets/omadesign.svg").write_text(source)
(ROOT / "assets/omadesign-wordmark.svg").write_text(
    source[:match.start()] + opening + source[match.end():]
)
