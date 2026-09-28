# Explicit spacing through threaded transforms

PR165's corrected horizontal spacing helper also applies when an explicit transform scales a story through its follower frame. Normal frame resize continues to reflow text without stretching its typography.

The regression drives `transform_shape_with_text_scale` through a follower, with inherited tracking, positive and negative range tracking, a mixed horizontal scale, and positive/negative manual kerning. It compares the newly shaped glyph IDs, positions and advances against the original scaled by the requested horizontal ratio, then encodes/decodes both frames and compares their contours and typography.

The old hscale-only path [fails](before.log). Both threaded-transform regressions [pass in the integrated suite](after.log) after reusing `scale_character_widths(sx / sy)`. The point-text native width/undo/redo/save/reopen workflow is in [PR165's recording](../../issue-152/nonuniform-resize/README.md), and the threaded native workflow is in [this PR's integrity recording](../readiness/README.md).
