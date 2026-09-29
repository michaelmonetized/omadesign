# Bundled U²-NetP

Unmodified `u2netp.onnx` from
https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx

Size: 4,574,861 bytes.
SHA-256: `309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8`.

U²-Net authors: Xuebin Qin, Zichen Zhang, Chenyang Huang, Masood Dehghan,
Osmar R. Zaiane, Martin Jagersand. Apache-2.0; full license in
`vendor/ml-notices/U2Net-LICENSE`. Model architecture and preprocessing:
https://github.com/xuebinqin/U-2-Net.

Input: N=1, RGB CHW, 320×320, ImageNet mean/std. Outputs are sigmoid
probabilities, used without any per-tile min/max normalization.
# Real-ESRGAN upscaling

`realesr-general-x4v3.onnx` is embedded in the executable for offline use. The
three larger models are explicit, verified downloads from this repository's
`upscale-models-v1` model-assets release; it is not an application update.

`upscale-manifest.json` records every source/weight URL, source revision, SHA-256,
architecture setting, state-dictionary key, and converted artifact size/hash.
All weights originate at xinntao/Real-ESRGAN. No Upscayl code or models are used.
The Real-ESRGAN license is BSD-3-Clause; the BasicSR architecture utilities used
at conversion time are Apache-2.0. Full texts ship in `vendor/ml-notices/`.
Neither pinned source tree supplies a standalone Apache NOTICE file.

Reproduce with a build-only virtualenv (Python 3.14):

```sh
python -m venv target/upscale-conversion/venv
target/upscale-conversion/venv/bin/python -m pip install torch==2.9.1 onnx==1.20.0 numpy==2.4.1
target/upscale-conversion/venv/bin/python scripts/convert-upscale-models.py
```

The script follows upstream `scripts/pytorch2onnx.py`, extending it to all four
architectures with named NCHW input/output, fixed batch 1, dynamic H/W, fp32 and
opset 17. Native x2 input dimensions must be even (pixel-unshuffle); runtime
tiles pad odd edges and crop them back. Each rerun verifies the pinned ONNX
hashes, not only the input weights. PyTorch/ONNX/Python are never needed by users.
