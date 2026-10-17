# Scalar filter families

The Channels adjustment dialog exposes all 30 existing Omadesign raster filter/effect choices for saved alpha and Quick Mask. The existing raster filter entry follows the same target. Numeric labels and ranges come from one shared settings catalog. Filter strength and the captured soft selection blend the completed operation once.

Scalar and RGBA evaluation share the spatial kernels. Scalar processing carries one float per pixel, without an RGBA proxy or byte-mask reconstruction. Neighborhood filters repeat grid edges. Alpha threshold, feather, growth and shrinkage edit coverage; tonal and color operations interpret it as a grayscale image. Color results use Omadesign's shared luminance coefficients, retaining equal RGB values directly. Chroma key converts keyed transparency to zero coverage. Grayscale and saturation-only operations preserve an already-gray channel's values. Existing RGB premultiplication and alpha blending remain on the RGBA path.

The same worker owns preview and full Apply. Cancellation is checked inside parallel processing and between blur batches; a cancelled result cannot publish. Target, allocation, selection and document guards remain. Apply uses the existing scalar command; Cancel and Original view do not change the document. The archive remains the existing full-depth scalar representation.

Remove Background remains a layer operation. Selecting saved alpha or Quick Mask disables it and invalidates a captured RGB removal result, preventing a scalar edit from falling through to underlying artwork.

The complete affected group passed 38 tests at `61bd0dde`; six compatibility checks passed separately at `e9292fd4`. The final target-safety change passed eight background-removal tests at `c5e360ea`. These are separate exact-source results. The two earlier compile failures remain recorded with zero executed tests.

The genuine `c5e360ea` desktop build passed. Native editing and an independent Node reader checked 480,000 samples per operation: Invert at strength 0.375 through the original soft selection and radius-2 Grow Alpha matched exact float bits. Quick Mask Unsharp Mask at radius 3, amount 1.5 and threshold 0 differed by at most 8.940696716308594e-7 against a 5e-6 bound. Actual Undo/Redo and a fresh-process native save/reopen matched exactly; artwork, profiles and other channels were preserved. Remove Background was visibly disabled for Quick Mask. This establishes the bounded software workflow, not human acceptance or the larger Craft filter catalog.

The build and examples are in `dist/omadesign-channel-filter-families-qa-c5e360ea`. Exact receipts, source identities, independent equations, outputs and screenshots are retained under the shared handoff `implementation/craft-channel-filter-families`. No user desktop, settings or documents were changed. The earlier recorded QA process was absent on recovery; only the two private test app processes were launched and stopped.

Advanced filter options, calculations, color/spot channels, interchange and full parity remain open. No app-version, schema or dependency change is involved.
