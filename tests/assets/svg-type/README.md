# SVG type acceptance

`affine-styles.svg` is an original Omadesign test drawing under the repository MIT license. It exercises a full affine matrix, mixed size/paint/weight, absolute character positions, character rotation, Arabic RTL, vertical writing, and text on a curve. The focused Rust tests compare the native document and exported SVG with both resvg and independently installed librsvg (`rsvg-convert`). Fonts are requested by family; missing-family substitution is tested separately and reported by the importer. No Storytold code or assets are copied.
