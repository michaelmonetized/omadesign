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
