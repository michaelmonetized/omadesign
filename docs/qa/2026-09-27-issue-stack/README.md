# September 27 issue stack verification

The ten issue PRs are stacked on existing [PR #147](https://github.com/michaelmonetized/omadesign/pull/147). Issues #144 and #145 are covered by existing PRs #146 and #147; #158 is excluded. Each row links the issue-specific native recording, editable artifacts, assertions, source/binary provenance and reproduction instructions.

| Stack order | Issue | PR | Native QA |
| --- | --- | --- | --- |
| 1 | #154 Flip controls | [#159](https://github.com/michaelmonetized/omadesign/pull/159) | [37 seconds](../issue-154/README.md) + [12-second Motion followup](../issue-154/motion-review/README.md) |
| 2 | #155 Alignment | [#160](https://github.com/michaelmonetized/omadesign/pull/160) | [57 seconds](../issue-155/README.md) + [10-second group followup](../issue-155/parent-review/README.md) |
| 3 | #156 Chrome | [#161](https://github.com/michaelmonetized/omadesign/pull/161) | [42 seconds](../issue-156/README.md) |
| 4 | #153 Independent effects | [#162](https://github.com/michaelmonetized/omadesign/pull/162) | [23 seconds](../issue-153/README.md) |
| 5 | #148 Paragraph composition | [#163](https://github.com/michaelmonetized/omadesign/pull/163) | [30 seconds](../issue-148/README.md) |
| 6 | #151 OpenType ranges | [#164](https://github.com/michaelmonetized/omadesign/pull/164) | [20 seconds](../issue-151/README.md) |
| 7 | #152 Character spacing | [#165](https://github.com/michaelmonetized/omadesign/pull/165) | [48 seconds](../issue-152/README.md) |
| 8 | #149 Text on paths | [#166](https://github.com/michaelmonetized/omadesign/pull/166) | [60 seconds](../issue-149/README.md) |
| 9 | #150 Area text and threads | [#167](https://github.com/michaelmonetized/omadesign/pull/167) | [Lifecycle and frame options](../issue-150/README.md) |
| 10 | #157 Agent attachments | [#168](https://github.com/michaelmonetized/omadesign/pull/168) | [35 seconds](../issue-157/README.md) |

Recordings contain actual native WGPU framebuffer frames driven through the production egui input handlers. They use deterministic render-ready replay time, not realtime performance measurements. Clipboard scenarios read the real OS clipboard. Agent delivery exercises an actual local ACP subprocess with the capability matrix required by #157; named external provider services were not independently exercised.

All compilation and editor tests ran locally on this ARM64 Linux machine. The accepted 0.5.0 human QA evidence and installed stable application were preserved. No merge or release was performed.

## Integrated tests

The complete reviewed source at `0df60015415f2a3cb4144a3ef1180ad40833a77f` passed **821 tests, zero failures**, with five explicitly ignored tests, in 267.55 seconds. The test executable was copied to a stable path before running so subprocess tests used the same binary throughout. The test build revision and the published source-equivalent revision are both recorded in `tested-source.json`; intervening differences only add area-text QA artifacts. [Full log](tests.log), [build log](build.log), [all-targets check](all-targets.log).

Three ignored Cairo/Poppler interoperability tests were then run explicitly and passed with the real external tools: [editable import](imports_cairo_generated_pdf_as_editable_vector_artwork.log), [masks/effects](poppler_matches_masked_gradient_effects_and_backdrop_dependent_fallbacks.log), and [pages/layers/alpha](poppler_renders_exported_pages_layers_and_soft_alpha.log). The remaining two ignored tests require private PDF/camera fixtures and were not run.

The suite includes mixed typography file-version progression (5 through 12), paragraph composition, actual OpenType substitutions, range spacing, path geometry, threaded source and clipboard preservation, independent effects, alignment/history, compact UI and agent attachment failure recovery. The area-text acceptance includes its additional wrap and vertical-alignment regression coverage. All ten individual branch source trees pass the offline all-targets check; [per-layer SHA receipts and logs](layer-checks/summary.json) are included.
