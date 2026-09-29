# September 27 readiness fixes

The three requested blocker categories are fixed on PR165, PR167 and PR168; PR166 is restacked to carry the spacing fix. Existing issue-specific acceptance recordings and the accepted 0.5.0 human QA evidence are preserved.

| Fix | Behavior | Native proof |
| --- | --- | --- |
| Text save/reopen spacing, PR165 | Nonuniform transforms scale explicit tracking and manual pair spacing with glyph widths. | [18-second resize, undo/redo, Ctrl+S and reopen](../issue-152/nonuniform-resize/README.md) |
| Text integrity, PR167 | Follower selection retains the active visible frame, including rotations. Populated links preserve supported inherited formatting; incompatible font/size/paint is rejected before mutation. Empty sources gain no extra paragraph. | [25-second link, selection, replacement, undo/redo and saved-document replay](../issue-150/readiness/README.md) |
| Agent reliability/privacy, PR168 | Failed startup, initialization rejection and disconnect preserve unsent/current drafts and attachments. Pending paste ranges follow exact editor operations. Private cache directories/files use 0700/0600 and cache-local Git exclusion. | [16-second real ACP startup failure, restored drafts, edit and retry](../issue-157/readiness/README.md) |

The spacing helper also covers explicit scaling through a follower frame. [Before failure and passing rendered-glyph/save-reopen regression](../issue-150/threaded-spacing/README.md).

## Combined checks

The complete functional source at `5af45677f57b4dd131f344fa27fee73f37e3ccb2` passed **833 tests, zero failures, five ignored**, in 302.80 seconds. [Full log](tests.log), [all-target check](all-targets.log), [build log](build.log). A copied test executable remained stable while other local worktrees built. Recorded source hashes bind the evidence to the tested files.

Three ignored interoperability tests were then explicitly run and passed with installed Cairo/Poppler: [editable import](imports_cairo_generated_pdf_as_editable_vector_artwork.log), [mask/effect rendering](poppler_matches_masked_gradient_effects_and_backdrop_dependent_fallbacks.log), [pages/layers/alpha](poppler_renders_exported_pages_layers_and_soft_alpha.log). The remaining two ignored tests require private PDF/camera fixtures and were not run.

A final seven-line followup updates agent failure status labels and adds one assertion; no other source differs from the full-suite build. The final combined source passes [all-target checking](final-all-targets.log) and the [startup exit/rejection regression](final-status-regression.log). The agent recording was repeated after this followup. [Exact revisions, source hashes and command receipts](tested-source.json).

All three new native recordings have zero unresolved input/assertion errors and pass full MP4 decoding. Captures use actual WGPU viewport frames and production egui input handling with deterministic replay time; they are not realtime performance measurements. Agent delivery uses a real local ACP fixture, not a named external provider service.

All desktop builds/tests ran locally on ARM64 Linux. This work updates the PR stack; it does not merge, package or deploy a production release. Issue #158 remains excluded. YouTube still shows advanced-feature verification pending; the three new clips are available directly in their PRs and queued after the original 15 for the existing unlisted playlist.
