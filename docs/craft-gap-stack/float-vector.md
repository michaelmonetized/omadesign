# Native float vector first layer

The existing `deep_color::composite` now integrates path coverage in floating point. It does not rasterize a byte Pixmap or byte mask and widen that result. The ordinary U8 compositor is unchanged.

Straight edges use analytic area integration, including intersections and nonzero/even-odd winding. Existing cubic and quadratic geometry is adaptively flattened at a 0.00001 device-pixel control-distance bound; nonmonotone controls subdivide too. Curves requiring more than 26 subdivision levels fail explicitly. This is a numerical geometry approximation, not a claim that the existing cubic ellipse approximation is an exact circle.

The shape path cache, shared text shaping/font cache and `outline_path`/`paint_paths` remain the geometry sources. Centered strokes use the existing floating-point stroke/dash geometry with the saved cap, join, width and dash phase. Pixel coverage is never converted through U8. Shape fill and stroke compose together before shape opacity; layer opacity and the existing native layer mask apply once at the existing layer boundary.

Supported in this cut:

- Flat unfiltered closed/open paths, ordinary shapes, holes, shape rotation and painter order, with solid fill and centered solid/gradient stroke.
- Plain editable text and its existing affine transform, whole-object fill, and supported solid/None character overrides using the shared original glyph curves.
- Untagged linear and affine/focal/two-circle radial gradients, with existing field coordinates and Pad/Repeat/Reflect spread. Stop RGB/alpha interpolate premultiplied in authored sRGB, without integer rounding. Paint samples use the output pixel center; analytic geometry coverage multiplies that sample.
- ICC-tagged solid vector paint through the existing Little CMS conversion and the actual document intent/profile. Native backgrounds retain HDR/negative and adjacent high-depth samples through the existing float blend equations.

Explicit refusals remain for retained patterns/filter graphs, vector shape/live artwork masks, frame/parent clips and placed shape images, source SVG/frame/path text, non-solid span overrides, inside/outside closed stroke alignment, group/appearance effects, Shape/Conic gradients and profiled gradients needing batched float ICC evaluation. Existing native layer masks and unfiltered group composition retain their prior float paths. Direct PDF still rejects original native color/pixels/masks before any U8 tile or normalization; other unsupported precision routes keep their admission guards.

`composite_with_cancel` passes the job cancellation owner through allocation, edge/row/paint work and checks before publication. Native PNG/TIFF raster Document export uses it. Existing PSD caller cancellation around its legacy encoding entry remains unchanged; this cut does not claim prompt cancellation inside every format encoder. The legacy `composite` wrapper remains available. Float layer/object/row, edge and scanline-event storage uses the existing shared allocation pool and stays owned until drop. Shared geometry/font caches and the existing stroke/path library remain unchanged; no universal third-party heap or cancellation-latency claim is made. The resource module remains the exact qualified 0b seam; the separately owned materialized-memory repair is not consumed.

This cut does not close the full FMT-04 vector/paint/effect gap. Analytic precision tests, source preservation, actual cancellation/admission failure and float TIFF output are separate from terminal display and physical/editor interoperability proof. See the exact implementation receipt before treating source as qualified.
