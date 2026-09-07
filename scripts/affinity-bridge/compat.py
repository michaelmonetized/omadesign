# SPDX-FileCopyrightText: 2026 Michael C Hurley
# SPDX-License-Identifier: GPL-2.0-or-later
"""Compatibility additions for the separately installed Inkscape Affinity importer.

This module runs only inside the GPL converter. It is not linked into Omadesign.
Affinity bitmap format 9 stores little-endian floating-point component samples in
256-byte-wide, 256-row tiles. Tile state 3 represents constant floating-point 1.
"""

from io import BytesIO
import logging
import math

import numpy as np
from PIL import Image

from inkaf.svg.raster import AFBitmap
from inkaf.utils import extract


def decode_float_bitmap(child, extractor):
    width, height = child["BmpW"], child["BmpH"]
    if width <= 0 or height <= 0 or width * height > 64 * 1024 * 1024:
        raise RuntimeError("Floating-point Affinity bitmap exceeds 64 megapixels")
    components = []
    for component in range(1, 5):
        cols = child.get(f"TWi{component}", math.ceil(width / 64))
        rows = child.get(f"THi{component}", math.ceil(height / 256))
        if cols <= 0 or rows <= 0 or cols * 64 < width or rows * 256 < height:
            raise RuntimeError("Invalid floating-point Affinity tile layout")
        if cols * rows > 8192:
            raise RuntimeError("Floating-point Affinity tile allocation is too large")
        states = child[f"Sta{component}"]
        indices = child[f"Idx{component}"]
        if len(states) != cols * rows:
            raise RuntimeError("Floating-point Affinity tile count does not match layout")
        plane = np.zeros((height, width), dtype=np.float32)
        next_block = 0
        for i, state in enumerate(states):
            y, x = (i // cols) * 256, (i % cols) * 64
            crop_h, crop_w = min(256, height - y), min(64, width - x)
            if state in (0, 1):
                tile = None
            elif state == 3:
                tile = 1.0
            elif state == 4:
                if next_block >= len(indices):
                    raise RuntimeError("Missing floating-point Affinity tile data")
                block = indices[next_block]
                next_block += 1
                data = extract(extractor, block["Data"].data)
                if len(data) == 65558:
                    data = data[21:-1]
                if len(data) != 65536:
                    raise RuntimeError("Invalid floating-point Affinity tile size")
                tile = np.frombuffer(data, dtype="<f4").reshape(256, 64)
            else:
                raise RuntimeError(f"Unsupported floating-point Affinity tile state {state}")
            if crop_h > 0 and crop_w > 0 and tile is not None:
                plane[y : y + crop_h, x : x + crop_w] = (
                    tile[:crop_h, :crop_w] if isinstance(tile, np.ndarray) else tile
                )
        if next_block != len(indices):
            raise RuntimeError("Unused floating-point Affinity tile data")
        # Preserve stored component values; no guessed gamma or ICC conversion.
        plane = np.nan_to_num(plane, nan=0.0, posinf=1.0, neginf=0.0)
        components.append(np.rint(np.clip(plane, 0.0, 1.0) * 255.0).astype(np.uint8))
    output = BytesIO()
    Image.fromarray(np.stack(components, axis=-1)).save(output, format="PNG")
    return output.getvalue()


def install():
    previous = AFBitmap.bitmap_from_blocks
    warned = False

    def convert(cls, child, extractor):
        nonlocal warned
        if child["Frmt"].id != 9:
            return previous(child, extractor)
        if not warned:
            logging.getLogger("inkaf.svg.convert").warning(
                "Floating-point Affinity raster channels are converted to 8-bit RGBA; "
                "values outside 0–1 are clipped and custom color profiles are not transformed."
            )
            warned = True
        return decode_float_bitmap(child, extractor)

    AFBitmap.bitmap_from_blocks = classmethod(convert)
