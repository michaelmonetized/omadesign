# SPDX-FileCopyrightText: 2026 Michael C Hurley
# SPDX-License-Identifier: GPL-2.0-or-later
"""Synthetic format-9 tiles, independent of private Affinity documents."""

from io import BytesIO
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import numpy as np
from PIL import Image
try:
    import compat
except ModuleNotFoundError:
    from inkaf import omadesign_compat as compat


class FloatTiles(unittest.TestCase):
    def test_byte_width_tiles_constant_alpha_and_quantization(self):
        child = {"BmpW": 65, "BmpH": 1}
        payloads = {}
        for component in range(1, 5):
            child[f"TWi{component}"] = 2
            child[f"THi{component}"] = 1
            child[f"Sta{component}"] = [3, 3] if component == 4 else [4, 4]
            child[f"Idx{component}"] = []
            if component == 4:
                continue
            for tile in range(2):
                key = f"{component}-{tile}"
                value = {1: 1.5, 2: -0.5, 3: 0.5}[component] if tile == 0 else 0.25
                payloads[key] = np.full((256, 64), value, dtype="<f4").tobytes()
                child[f"Idx{component}"].append({"Data": SimpleNamespace(data=key)})
        with patch.object(compat, "extract", lambda _, key: payloads[key]):
            image = Image.open(BytesIO(compat.decode_float_bitmap(child, None)))
        self.assertEqual(image.size, (65, 1))
        self.assertEqual(image.getpixel((0, 0)), (255, 0, 128, 255))
        self.assertEqual(image.getpixel((64, 0)), (64, 64, 64, 255))
        child["Sta1"] = [2, 2]
        with self.assertRaisesRegex(RuntimeError, "state 2"):
            compat.decode_float_bitmap(child, None)


if __name__ == "__main__":
    unittest.main()
