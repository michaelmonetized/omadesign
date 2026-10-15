# Pattern tile period and opacity corrections

Source: resvg 0.47.0, crates.io checksum `9be183ad6a216aa96f33e4c8033b0988b8b3ea6fd2359d19af5bac4643fd8e81`. Dual MIT/Apache-2.0 licenses and source copyright notices retained. CLI, examples and development-only dependencies omitted.

`src/path.rs` uses the rounded tile pixel dimensions to derive its render and inverse shader scales, preserving the original vector period. Minimum tile dimensions are one pixel so small patterns remain visible when fitting large art. Public APIs and authoring data are unchanged.

Pattern fill/stroke opacity scales the already allocated premultiplied tile, with shader opacity left at one. In tiny-skia 0.12, pattern opacity and antialias coverage share `current_coverage`; edge rendering overwrites paint opacity. Applying opacity to tile samples keeps a 40% pattern at or below 40% alpha, including curved text edges. No extra tile allocation or public API change is introduced.
