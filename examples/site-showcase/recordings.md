# Native studio recordings

The website's five clips contain 156 seconds and 24 chapters of the actual native
application. They are continuous WGPU viewport recordings at 1600 × 900, 30 fps,
with no audio. They do not contain HTML panel replicas or still-image slideshows.

## What is recorded

`src/bin/capture_studios.rs` prepares a disposable starting document, then sends
real pointer, drag, scroll and keyboard events through egui. Every subsequent
panel action and artwork edit uses the native UI. Captured frames come from the
native viewport screenshot API; the only extra drawing is a small pointer
indicator. Motion uses the actual document tracks and native renderer, with the
clock advanced once per recorded frame.

Design and Motion start from the editable original `design.oma` and `motion.oma`
examples in this folder. Pixel and Photo start from the original generated iris
painting and fictional coastal landscape documented in [README.md](README.md)
and [prompts.json](prompts.json). The recordings demonstrate new brush, healing,
mask, development and animation edits; they do not claim the imported starting
images were painted or photographed by hand in the app.

The shared kit is created beneath the system temporary directory. It copies the
approved refined logo, the two native example projects and the original Fieldwork
landscape SVG into a temporary `.omabrand`, writes two project palettes, and copies
two local fonts into a temporary typography kit. Source artwork, user documents,
installed fonts and the user's running application are left untouched. Recorded
edits are not saved over the starting projects.

| Clip | Seconds | Chapters, in seconds |
| --- | ---: | --- |
| Design | 42 | Layout 0; Appearance 6; Typography 12; Reshape 18; Effects 24; Trace 30; Layers 36 |
| Pixel | 30 | Brush 0; Retouch 6; Mask 12; Color 18; Layers 24 |
| Photo | 24 | Light 0; Color 6; Detail 12; Library 18 |
| Motion | 30 | Presets 0; Keyframes 6; Appearance 12; Reshape in Design 18; Layers 24 |
| Brand kit | 30 | Palettes 0; Brand assets 10; Typography 20 |

The Motion reshape chapter deliberately switches the same document to Design,
changes its geometry, and returns to Motion. Trace demonstrates the actual
settings panel; this clip does not claim to trace the vector poster.

## Recreate

Run from the repository root in a working graphical session. A current Rust
toolchain, native GPU support, Python 3, and `ffmpeg`/`ffprobe` are required. FFmpeg
must provide `libx264` and `libwebp`. The shared kit also needs these local fonts:

- `/usr/share/fonts/gsfonts/NimbusSans-Bold.otf`
- `/usr/share/fonts/gsfonts/NimbusRoman-Regular.otf`

```sh
cargo build --release --bin capture_studios
capture_root="$(mktemp -d /tmp/omadesign-recordings.XXXXXX)"
mkdir -p "$capture_root/config" "$capture_root/data" "$capture_root/frames"
for scene in photo pixel brand-kit design motion; do
  XDG_CONFIG_HOME="$capture_root/config" XDG_DATA_HOME="$capture_root/data" \
    target/release/capture_studios "$scene" "$capture_root/frames"
done
python3 scripts/publish-recordings.py "$capture_root/frames"
```

Each scene opens its own isolated native window and closes when recording is
complete. The capture utility ignores ordinary external input to that window and
uses its explicit replay events. It does not drive or close another app window.
Temporary kit folders are named `omadesign-recording-<process-id>` under the system
temporary directory and may be removed after review.

The capture folder contains each MP4, a native PNG and text-position snapshot
every two seconds, and an `*-errors.json` report. Inspect those frames and the
continuous clip before publishing. The publisher refuses unresolved input targets
or incorrect video dimensions, codec, frame count or duration. It copies the
validated H.264 fast-start videos, encodes WebP posters, and writes WebVTT captions
and a chapter manifest with file sizes and SHA-256 hashes into
`site/public/media/recordings`. It does not modify the recorded video frames.
