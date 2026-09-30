# Complex motion playback

The supplied Cloud.oma fixture is 1920×1080 with 28 layers, 36 blurred cloud ellipses and 90 effects. Its contents were preserved. The JSON files identify the local source by SHA-256; the artwork is not redistributed here.

Using the same 1248×798 viewport and 0.625 camera scale, `motion_profile` renders two passes through 12 fixed motion times. The second pass measures a warm working set. The baseline binary was built from the accepted raster-motion/ACP branch at 69379959; the candidate includes 86eaac2e.

| Warm CPU render | Baseline | Candidate |
| --- | ---: | ---: |
| Median | 312.72 ms | 88.51 ms |
| Maximum of 12 samples | 547.26 ms | 102.94 ms |

This is a 3.53× median CPU-render improvement, not displayed FPS or a promise of real-time rendering for every composition. The first pass still populates caches. Image resolution, effect quality and editable source structure are retained.

The fixes retain prepared pixels and immutable appearance planes, remove translation cancellation from effect coordinates, and keep normal effect composition consistent during fades. Native effect storage is capped at 256 MiB and appearance storage at 256 MiB per render thread, with frequency admission for larger scenes. This explicitly trades bounded memory for less repeated computation.

A representative frame at 0.5454546 seconds differs from the baseline by at most 3/255 per RGBA channel, with mean absolute difference 0.01933/255; no channel differs by more than 3. Canonical local coordinates and stable fade composition account for those rounding differences. The compositor suite retains exact cached-versus-fresh comparisons for masks, blends, transforms, motion and paint invalidation.

Validation: 68 compositor tests passed (1 ignored benchmark), 21 filter tests passed, and the additional fade-continuity test passed. The shared-identity test confirms changed pixels/effects cannot reuse stale planes. Additional native and integrated validation is recorded with the 0.6.2 QA candidate.

Reproduce locally:

```sh
cargo build --release --bin motion_profile --locked --offline
./target/release/motion_profile /path/to/Cloud.oma /path/to/frame.png
```

An optional final `layers` argument reports isolated layer costs for diagnosis. It changes cache warming and should not be used for the before/after measurement above.
