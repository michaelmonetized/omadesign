# Paragraph composition native QA

[e2e.mp4](e2e.mp4) records the actual native WGPU viewport at 1600×900,
10 frames per second, 300 frames / 30 seconds. Input is real egui keyboard,
pointer, and scrolling events through the application's ordinary handlers.

The recording selects text, exercises left/center/right/justify/force-justify
shortcuts, adds a third paragraph with independent center alignment, commits,
undoes and redoes the edit, enables dictionary hyphenation in the Paragraph
inspector, saves and reopens the project, and exports SVG. The captured result
has zero unresolved input targets. `saved.oma` remains editable.

Native QA caught and fixed arrow-key navigation stealing keyboard focus from
the canvas. The successful replay retains canvas focus and newly typed content.

Automated coverage exercises justification within 0.5 px, limit fallback,
last-line modes, paragraph edit persistence, Unicode/grapheme wrapping,
no-break ranges, discretionary hyphens, actual dictionary patterns, matching
caret/hit positions, and measured hyphenation effects on word-space stretch.
Threaded widow/orphan and keep rules are exercised by the dependent area-text
issue #150 because they only affect threaded frames.

Reproduce locally (no installation changes):

```sh
cargo build --bin capture_studios -j2
DISPLAY=:0 target/debug/capture_studios paragraphs /tmp/paragraph-qa --fps 10
cargo test --lib text::
```
