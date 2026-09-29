# Offline inference components

- U²-Net and U²-NetP: Xuebin Qin, Zichen Zhang, Chenyang Huang, Masood Dehghan,
  Osmar R. Zaiane, and Martin Jagersand; Apache-2.0.
  https://github.com/xuebinqin/U-2-Net
- DIS / IS-Net (optional model): Xuebin Qin and the DIS contributors; Apache-2.0.
  https://github.com/xuebinqin/DIS
- ONNX Runtime 1.28.0: Copyright (c) Microsoft Corporation; MIT.
  https://github.com/microsoft/onnxruntime/tree/v1.28.0
- ort and ort-sys 2.0.0-rc.13: Copyright (c) 2023-2026 pyke.io and
  Copyright (c) 2020 Nicolas Bigaouette; MIT OR Apache-2.0.
  https://github.com/pykeio/ort

The model bytes are unchanged rembg v0.0.0 release assets. The model registry
in `src/ml/models.rs` pins their sizes and SHA-256 hashes; the fast model is
embedded in the executable. The build-only runtime fetcher pins both official
Linux CPU archives by SHA-256. No model or runtime is fetched automatically by
the installed application. The high-quality models require an explicit download.

Full upstream license texts and ONNX Runtime's ThirdPartyNotices accompany this
file. U²-Net, DIS, and ort publish no separate Apache NOTICE file in the checked
upstream roots. Copyright/license notices found in Rust dependencies are also
preserved in the sibling rust notices directory. These attributions do not claim
ownership of the upstream models. No Real-ESRGAN or ncnn component ships in #144;
those belong to the subsequent #145 implementation.
