# Font search and pixel selection regressions

The fixes are committed in `8ee01b4ea08e0dac9891694b82dd3f4f377a5cf2`, following production reference `28844960`. The candidate still reports package version `0.6.0`; this directory documents fixes prepared for the next release, not an already shipped `0.6.1` package.

Each regression was reproduced with the new test present and its corresponding production path retaining the old behavior, then rerun with the fix. The pixel-selection before run already included the independent font-popup fix. These are focused behavioral comparisons, not pristine whole-commit performance measurements. The [receipt](receipt.json) pins the full revisions, relevant source hashes and original/sanitized log hashes.

| Behavior | Before | After | Evidence |
| --- | --- | --- | --- |
| Font search stays open when clicked; typing filters the list, selection changes text, undo restores it | Failed: clicking search dismissed the picker | Passed | [before](font-before.log), [after](font-after.log) |
| Escape and outside clicks dismiss font search without changing text | Passed | Passed | [before](font-before.log), [after](font-after.log) |
| Pixel selection survives a new layer and clips both an outstanding old-layer stroke and a new-layer stroke; three-step undo/redo restores both strokes and layer creation | Failed: selection became `None` | Passed | [before](before-brush.log), [after](after-brush.log) |
| Original soft-mask coverage, coordinate space, allocation, generation and selection-path nodes survive layer creation, switching and undo/redo | Failed: selection became `None` | Passed | [before](before-coordinates.log), [after](after-coordinates.log) |

The coordinate test covers pixel/vector insertion at the root and inside a group, with scaled, rotated, sheared source rasters both inside and partly outside the document. It compares mapped coverage on the new layer and native coverage on the source layer, then confirms explicit deselection still clears the mask. Existing layer hierarchy, lock, selection-identity and undo tests also pass: [5 tests](after-layer-groups.log), including the new coordinate test.

The font tests drive egui pointer/key events against the actual popup; pixel tests exercise the actual marquee, brush, layer and history paths. They do not constitute human desktop QA. No live user document was manipulated for these checks. Test elapsed times are retained as log evidence and are not performance results.

Repository/target absolute paths and panic thread IDs are replaced in the published logs. Extra blank lines at the end of published logs are removed. Assertions, synthetic mask values, test outcomes and durations are unchanged. The interrupted invocation that used an unintended local Cargo target ran no tests and is excluded from this evidence.
