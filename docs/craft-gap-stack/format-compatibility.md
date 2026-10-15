# Integrated document versions

These gates are implemented on the SVG integration candidate. They require integrated runtime qualification before a release. The app release number and document format number are separate.

| Minimum document version | Persisted feature | First planned milestone |
| --- | --- | --- |
| 14 | Affine/rich type and per-span font, size or paint | 0.6.8 |
| 15 | Gradient affine axis, focal point, inner radius or spread | 0.6.9 |
| 16 | Vector patterns and shared SVG definition references | 0.6.10 |
| 17 | Editable SVG filter graphs | 0.6.11 |
| 23 | Publishing styles, masters, links and page metadata | 0.6.26 |

The writer takes the highest requirement across every layer, shape and character span. A pattern used by one text range therefore requires version16 even though range styling itself requires14. Filter graphs require17 regardless of layer order. Pattern sources in text ranges use the same pooled definitions as shape fills.

Plain documents still write version5. Existing versions1–13 retain their normalization paths. Known new features declared below their required version are rejected before source expansion or pixel decompression. This candidate reads versions1–17 and23. It refuses reserved versions18–22 and versions above23 because their models have not been integrated. Raising the highest version does not imply support for missing formats between the implemented gates.

Nonempty publishing metadata and paragraph or character style references require version23 even when the text and paint need only14–17. Empty publishing defaults retain the existing document version. Publishing source and mixed-font runtime acceptance remain separate from these compatibility checks.

Compatibility checks cover mixed-feature precedence, underdeclared gradient/filter documents, plain legacy input and shared pattern definitions across native text and shape fills. Source and independent renderer acceptance remain separate. In particular, RGBA8 SVG evaluation does not establish float color fidelity, and animated affine gradients retain their own integration gate.

The PDF integration branch includes the independently qualified vector and embedded-font exporter. That source merge has not been qualified with these text and paint models. Retained SVG filter graphs and rich source text currently return an explicit error rather than silently losing effects or range paint. Advanced gradients use the existing named appearance fallback until the PDF adapter preserves their affine field. These boundaries must be replaced and tested before the integrated PDF milestone is accepted; the standalone PDF receipts do not qualify the combined source.
