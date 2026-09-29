# Output acceptance

All 18 same-host before/after saved-document pairs preserve their common authored state under the current schema. Each authoring document adds exactly 596 `fill_opacity:1.0` fields, 596 `blend_interior:false` fields and one `text_wrap_above_only:false` field. The additive proof verifies those declarations against the pinned candidate source, initializes only absent reference-side defaults, and then checks a global fresh-ID bijection and every reference. Seed IDs, other values/types, array order and opaque image bytes remain protected; no unknown field or candidate value is removed.

**The authoring images are not pixel-identical.** All six host/repetition comparisons change the same nine isolated pixels of 1200×900 (0.000833%), with maximum RGB delta 15/255 and unchanged alpha. Each version repeats exactly, and its authoring image is identical across hosts. This is a quantified small render difference with preserved authored data, not a claim of bit-identical rendering or universal visual equivalence. Its cause has not been established.

All 114 within-version image repeatability pairs are exact. All 108 same-host AI/native export comparisons are exact before/after. Cross-architecture AI images have maximum channel delta 2/255; those differences occur in both versions, while each host’s before/after outputs stay exact. They are pre-existing architecture differences in this comparison, not new candidate-only changes.

The full audit covers 380 image pairs and 60 document pairs. Raw document byte/ID-only failures and every pixel difference remain visible in [summary.json](summary.json); successful current-default proofs do not overwrite those findings. [The additive proof](schema_defaults_equivalence.py) is separate from [strict fresh-ID equivalence](document_id_equivalence.py).

Of the 60 saved-document pairs, 15 match bytes, 11 more pass the strict ID-only proof, and 18 more pass the additive-default/ID proof. The remaining 16 pairs are native AI documents compared across architectures; they are not declared equivalent. Each host’s baseline/candidate native documents passed the separate common-authored-state proof above.

The selected 108 processes passed their checks. Failed/superseded native attempts remain preserved in continuation lineage. The recovered native protocol tests Escape cancellation and does not cover the Cancel button. [Complete measurements](README.md) preserve timing regressions and cohort differences.
