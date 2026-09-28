# Nonuniform text resize and save/reopen

The original reproduction doubled a point-text object's width while keeping its
height unchanged. Existing contours stretched, but explicit range tracking and
manual kerning did not: fresh composition drifted by **8.80011 px**. The committed
[test-before.log](test-before.log) records that failure before the fix.

`TypeRun::scale_character_widths` applies the remaining horizontal/vertical scale
ratio to glyph width, explicit em tracking, and manual pairs. Inherited tracking
stays inherited because its legacy run-level pixel value already scales
horizontally. Uniform sizing retains relative em values.

[tests.log](tests.log) records all 10 character-metrics regressions passing. The new
regression checks every contour vertex after fresh composition and native
encode/decode at scale ratios `(2,1)`, `(0.6,1.5)`, and `(1.25,1.25)`. Its fixture
combines inherited tracking, positive and negative explicit tracking, a separately
scaled range, and positive and negative manual pairs.

[e2e.mp4](e2e.mp4) is the actual 1600×900 native WGPU viewport, 180 frames at 10 fps
(18 seconds). The initial editable text is seeded; the recording scrolls the
ordinary inspector, expands Layout, replaces Width with 600 through its numeric
text field, checks the live contours, undoes and redoes that edit, sends Ctrl+S,
and reopens the saved file through the application loader. Each checkpoint
requires the mapped, recomposed, and reopened contour vertices to agree within
0.001 px. The replacement width arrives as one text-input event, so it is one
numeric-field edit for the undo/redo check.

[result.json](result.json) confirms completion and zero unresolved input targets.
The four checkpoint receipts cover resize, undo, redo and reopened state.
[saved.oma](saved.oma) is the exact Ctrl+S file. Its font reference points to the
recorder checkout's bundled OFL EB Garamond fixture; the recording is not a test
of font packaging or a realtime performance benchmark. [native.png](native.png)
shows the reopened result; [resized.png](resized.png) shows the native Width
control at 600. The maximum live recomposition difference was 0.00006104 px;
recomposing the reopened document had zero difference. ffprobe confirms H.264,
1600×900, 180 frames and 18.000000 seconds; ffmpeg decoded the whole movie without
errors. Product and harness source is `99db3f83` (including fix `429a4e28`).

Reproduce from the repository root, with a graphical session and ffmpeg:

```sh
cargo build --lib --bin capture_studios -j2
DISPLAY=:0 target/debug/capture_studios spacing-resize /path/to/isolated-qa --fps 10
cargo test --lib text::metrics::tests::
```

The later area-text PR reuses the same helper in its independent frame/story
transform; this PR #165 recording specifically exercises point text.
