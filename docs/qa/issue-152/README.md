# Character spacing QA

[e2e.mp4](e2e.mp4) records the actual native WGPU viewport at 1600×900,
480 frames / 48 seconds at 10 fps, with zero unresolved input targets.
The recorded product revision is `af0697c3578316fa90d17a1b45ecd79ab27c03ed`,
the actual stacked branch including the preceding inspector, effects, paragraph
and OpenType changes. The library was rebuilt from this checkout immediately
before the recorder was linked and copied to its isolated executable.
`result.json` verifies the saved document. `recorded.oma` is the exact recorded
file; `saved.oma` is its portable copy with identical EB Garamond bytes in
`.omabrand/fonts` and only the font reference changed to its native archive ID. `native.png` shows the mixed baseline field beside live text; `preferences.json`
records the changed increment.

The native `capture_studios spacing` scenario selects text and exercises Alt
tracking, Ctrl+Alt tracking, fixed leading, baseline shift, the Optical mode
menu, horizontal and vertical scale fields, tracking reset, a manual caret
pair, mixed baseline values, undo/redo, Ctrl+S, the application's file
reopen path, and an actual Config increment edit saved to an isolated preferences
file. The scenario fails unless all of those changes survive the saved
file and all pointer/keyboard targets resolve.

The Character grid stores tracking and manual kerning in 1/1000 em. Legacy
run-level tracking remains px: `em = px / font_size * 1000`. Legacy fixed and
auto leading retain their prior spacing. New ranges use Auto(percent) or
Fixed(px); each composed line uses its largest leading. Absolute baseline
shift and fixed leading scale with document transforms and responsive font
sizing; em tracking, pair values, scale percentages and Auto percentages stay
relative. Documents require version 12 only when the new spacing fields exist.

Optical kerning samples each glyph's outline at 16 shared heights and adjusts
the lower-quartile side-profile gap toward 0.075 em, capped at ±0.15 em. Font
profiles and pair adjustments are cached. Manual pairs add to any mode and
split standard, contextual, discretionary and historical ligatures at the edited boundary so their movement remains
visible. This is an outline-based algorithm, not an attempt to match a
proprietary application's optical values.

Rich SVG outlines preserve exact positioned glyphs. HTML uses CSS spans for
representable metrics and inline outlined SVG for manual/optical kerning or
range scale. Text remains editable in the saved `.oma`.

Validation: the existing inspector-width regression across frames and personas,
all 34 text tests, the responsive character scaling test, all 21
existing shortcut tests, and all 3 HTML layout export tests pass.

Focused regression coverage includes legacy tracking conversion, per-line
leading, cache invalidation, manual pair insertion/deletion, ligature
boundaries, metrics/none/optical differences, shifted and scaled carets,
selection and hit testing, configurable shortcut steps and reset, defaults
for newly typed text, clipboard ranges, undo/redo, save/load, and responsive
and geometric transforms.

```sh
cargo test --lib text::
cargo test --lib character_scaling_tests
cargo build --bin capture_studios -j2
DISPLAY=:0 target/debug/capture_studios spacing /path/to/qa --fps 10
```

Recording timeline:

- 0–11 s: select text; tracking ×5, leading, baseline shift; choose Optical;
  drag horizontal and vertical scale.
- 12–25 s: reset tracking, apply a new range value, shift two characters,
  adjust one manual pair, and use the ×5 chord again.
- 26–32 s: inspect mixed values, commit, undo, redo.
- 34–38 s: Ctrl+S, reopen through the app loader, select the reopened text.
- 40–46 s: open Config, change the tracking/kerning increment, and close it.

The replay verifies a new document tab with fresh history after reopening,
compares the active app's spacing data with disk, and verifies the saved
Preferences increment. SVG uses the production exporter; HTML uses a frame
containing the reopened text and the production portable HTML exporter.

The OFL font and license are included. `package-font.py` reproduces the portable
copy from the untouched recorded document and the repository's test font.
