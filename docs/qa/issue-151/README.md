# OpenType character ranges native QA

[e2e.mp4](e2e.mp4): actual native WGPU viewport, 1600×900, 200 frames,
10 fps / 20 seconds, zero unresolved pointer or keyboard targets.

The replay selects editable text, opens the discovered OpenType feature panel,
applies real small-cap substitutions, types new text inheriting the preceding
range, selects one character, applies an independent override, displays the
font's alternate-glyph thumbnails, commits, undoes, redoes, saves and reopens,
and exports HTML plus exact outlined SVG.

`result.json` records three normalized, non-overlapping character ranges with
opposing `smcp` values and verifies that newly typed content survived reopening.
The screenshot shows the font-specific controls and actual glyph thumbnails.

Automated tests use bundled SIL OFL EB Garamond to verify GSUB/GPOS discovery,
range-limited substitutions across UTF-8 boundaries, ligature on/off ranges,
editing inheritance, serialization, HTML spans, and supported stylistic sets,
figure variants, small caps, swashes, fractions, ordinal, superscript and
subscript forms. Unavailable font features are omitted from the inspector.

```sh
cargo build --bin capture_studios -j2
DISPLAY=:0 target/debug/capture_studios opentype /tmp/opentype-qa --fps 10
cargo test --lib text::
```
