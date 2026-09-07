# RAW decoder validation

`synthetic.dng` is an original 64 × 48, uncompressed 16-bit Bayer DNG with a
deterministic sensor ramp, DNG camera matrix, white balance and exposure metadata.
It contains no thumbnail or JPEG. `generate-fixture.py` recreates it independently
from the TIFF tag layout. It is MIT licensed with Omadesign; no private photograph
or third-party camera fixture is included in the repository.

The tests verify actual sensor demosaicing, more than 256 output levels,
orientation, metadata, unapplied DNG baseline exposure, immutable input and
rejection of truncated or oversized data. An optional ignored test compares a
locally supplied camera file against an independent LibRaw 0.22.2 render:

```sh
OMA_RAW_FIXTURE=/path/image.DNG OMA_RAW_REFERENCE=/path/reference.rgb16le \
  cargo test --lib formats::raw::tests::private_camera_fixture -- --ignored --nocapture
```

Reference settings are `dcraw_emu -disars -mem -w -W -g 1 1 -6 -q 3 -H 0 -T`; extract its
16-bit RGB samples without color conversion into little-endian `rgb16le`. Minor
rounding/demosaic differences can occur between compilers and OpenMP builds.
