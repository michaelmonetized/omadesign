# OpenType character ranges native QA

[e2e.mp4](e2e.mp4): actual native WGPU viewport, 1600×900, 200 frames,
10 fps / 20 seconds, zero unresolved pointer or keyboard targets.

The replay selects editable text, opens the discovered OpenType feature panel,
applies real small-cap substitutions, types new text inheriting the preceding
range, selects one character, applies an independent override, displays the
font's alternate-glyph thumbnails, commits, undoes, redoes, presses Ctrl+S and reads the saved file,
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

Final integrated verification: **25 typography tests passed** (`tests.log`). The native assertion compares actual glyph IDs: selected lowercase **d** at character 33 returns to its normal glyph, while adjacent **i** at 34 retains small caps. Both settings persist in the Ctrl+S file. Playback is deterministic render-ready replay, not a realtime performance measurement.
