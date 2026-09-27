# Character spacing QA

The native `capture_studios spacing` scenario selects text and exercises Alt
tracking, Ctrl+Alt tracking, fixed leading, baseline shift, the Optical mode
menu, horizontal and vertical scale fields, tracking reset, a manual caret
pair, mixed baseline values, undo/redo, Ctrl+S, and the application's file
reopen path. The scenario fails unless all of those changes survive the saved
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
split default ligatures at the edited boundary so their movement remains
visible. This is an outline-based algorithm, not an attempt to match a
proprietary application's optical values.

Rich SVG outlines preserve exact positioned glyphs. HTML uses CSS spans for
representable metrics and inline outlined SVG for manual/optical kerning or
range scale. Text remains editable in the saved `.oma`.

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
