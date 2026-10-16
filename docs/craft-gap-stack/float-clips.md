# Native float clips and leaf masks

This layer extends the existing precise compositor. It keeps the document, paint, pixel, font, path-cache and resource models from the qualified float-vector layer.

Frame ownership uses `FrameLayout.parent/frame/clip`, the original cached path and `compositor::shape_parent_transform`. Shape bitmap placement uses `compositor::shape_mask_transform`. Frame backgrounds precede their children even when children occur earlier in the flat store. Parent rotations also rotate paint coordinates and bitmap placement. Frame clipping uses the same winding rule as the shared frame renderer.

The coverage sweep tracks each path's winding independently. It integrates only intervals inside the painted geometry and every geometric clip. Opposite half-pixel regions therefore have zero intersection; multiplying their independently averaged coverages would incorrectly produce one quarter.

A leaf bitmap mask reads original `Pixels::sample` values, including authoritative native samples, without a Pixmap, byte mask, display-cache reconstruction or new cache. Premultiplied luminance/alpha is interpolated bilinearly. Transformed source half-texel knots partition the covered intervals; clipped trapezoids are triangulated and use degree-two quadrature. This integrates the bilinear field over the actual covered region rather than multiplying two pixel averages. Existing float32 placement and adaptive curve approximation still set the geometric precision boundary. Paint shaders retain the preceding layer's pixel-center sampling contract.

Supported source subset:

- Unit-opacity, unit-fill-opacity, Normal, unfiltered frame containers, including nested rotations and clip toggles, within the existing renderer's 64-ancestor transform bound.
- Previously supported fills, plain text and centered strokes through those geometric clips.
- A single leaf fill or stroke with unprofiled RGB mask samples in `[0,1]`, from original U8, U16 or F32 pixels. Original mask alpha participates before interpolation. Non-byte values and same-display-cache distinctions remain authoritative.
- Existing native layer masks and admitted encoded-output ownership remain separate, unchanged stages.

Named refusals remain for frame subtree opacity/blending/bitmap masks, masked fill-plus-stroke or character-paint combinations requiring joint appearance isolation, live artwork masks, CMYK clip/mask combinations, profiled or out-of-range bitmap mask fields, patterns/filter graphs, layout/source/frame/path text, non-solid spans, inside/outside strokes, effects and profiled RGB gradients. The direct PDF native-first refusal remains unchanged.

New numeric edge, scanline, event, identifier and pixel buffers use the same admitted resource pool and cancellation owner. Bounded clip/winding/polygon metadata lives on the stack and cached paths are shared. No resource policy, materialized-memory credit repair, model/schema, app version, dependency, Photo/wire path or direct-raster export optimization is included.

The source tests cover analytic disjoint/overlap/sub-byte intersections, winding holes, texel-field integrals, premultiplied alpha, native-mask/cache distinctions, HDR/negative content, rotated nested placement, frame painter order, gradients/strokes, OMA round trips, native TIFF alpha, actual admission/cancellation cleanup, and preserved U8/PDF controls. Their execution is held for the coordinator's candidate authorization. This source layer alone does not close FMT-04 or establish runtime/device/editor proof.
