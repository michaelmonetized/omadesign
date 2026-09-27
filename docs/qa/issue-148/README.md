# Paragraph composition native QA

[e2e.mp4](e2e.mp4) records the actual native WGPU viewport at 1600×900,
10 frames per second, 300 frames / 30 seconds. Input is real egui keyboard,
pointer, and scrolling events through the application's ordinary handlers.

The recording selects text, exercises left/center/right/justify/force-justify
shortcuts, adds a third paragraph with independent center alignment, commits,
undoes and redoes the edit, enables dictionary hyphenation in the Paragraph
inspector, saves with Ctrl+S, loads that saved project, and exports SVG. This
recording was rerun on top of PR #162 after the migration fixes. The captured result
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

Integrated regression run: 726 passed and five were ignored before final fixes.
The two failures exposed new-text migration and an outdated offline credits
inventory. Both were corrected; the focused rerun passed all 18 text tests,
the agent save/reopen regression, and the credits inventory check. Three added
regressions protect fresh wrapped/point text, trailing soft hyphens and no-break
ranges during runt adjustment. Focused logs are included.

All three external Cairo/Poppler interoperability checks also passed; their logs
are included. The recording uses deterministic render-ready input replay, not
a realtime performance measurement.

The final paragraph audit also verifies Unicode Word wrapping separately from
Normal wrapping: Katakana words stay intact, Anywhere explicitly permits overflow
breaks, discretionary hyphens remain available, and runt avoidance only moves
whole Unicode words. All 10 composer regressions pass. These two followups do not
change the ASCII native scenario recorded above.
