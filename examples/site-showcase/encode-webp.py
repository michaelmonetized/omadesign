"""Encode native exports for the website; no cropping or image adjustments."""

from pathlib import Path
from tempfile import gettempdir

from PIL import Image


destination = Path("site/public/media/showcase")
intermediates = Path(gettempdir()) / "omadesign-site-showcase"
vector_exports = Path("examples/site-showcase/exports")
for name in ("design", "motion", "pixel", "photo"):
    vector = name in ("design", "motion")
    source = vector_exports if vector else intermediates
    with Image.open(source / f"{name}.png") as image:
        image.save(
            destination / f"{name}.webp",
            lossless=vector,
            quality=90,
            method=6,
        )
