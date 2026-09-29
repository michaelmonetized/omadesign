# Native AI driver validation

The final QA protocol passes background removal and upscaling against both ARM product libraries. These four runs are **functional diagnostics, not benchmark measurements**: library compilation could overlap. Their retained UI and elapsed-time fields must not be used for performance claims. Final measured native reruns belong to the separately recorded matched continuation cohort.

The candidate product source remains `8ee01b4ea08e0dac9891694b82dd3f4f377a5cf2`. The baseline is issue reference `221834bdd8af5aee8554a0e835629cbe2f467558`, not shipped 0.6.0. Its library was rebuilt at `587433037d1df9be5e66b36cc77bc6f44fcd7071`; the production-source equivalence proof is independently checked by the report auditor. Only QA input/validation code changed for this protocol.

## Why the original driver failed

The [trace excerpt](trace-excerpt.log) preserves original trace line numbers. A candidate diagnostic exited 101; its binary, original source, instrumented source and full trace hashes are in [trace-receipt.json](trace-receipt.json).

1. Frame 45 pressed the actual Cancel widget at `(272.4, 741.9)` (`id_91D6`). During that UI update the footer moved upward 32 pixels.
2. Frame 46 released at the old position. `clicked=None`; the removal modal remained open.
3. The driver assumed cancellation succeeded. Its later editor “Remove Background” click instead hit the still-open modal's backdrop (`id_C032`), dismissing it.
4. The global readiness check accepts an absent removal session, and “Mask” also exists in the sidebar. The driver advanced without a dialog and eventually failed to find Radius.

The disappearing progress row in the product's busy/ready layout explains the observed footer movement; the exact asynchronous job state was not instrumented. See [diagnosis.json](diagnosis.json). This evidence does not establish a newly introduced candidate regression or validate the moving Cancel button.

## Final input protocol and coverage

`hover-atomic-buttons-held-sliders-escape-cancel-v2` uses real egui pointer and keyboard events. Semantic buttons receive a hover pass, refreshed label coordinates, then press/release in one input batch. The Radius slider retains a held press across a UI pass because atomic button clicks do not change its drag value. Cancellation uses the actual Escape key. **Cancel-button coverage is explicitly false.**

Exact modal identity and unique control labels are checked around opening, cancellation and reopening. Asynchronous export completion also waits for modal closure. There are no direct product-state shortcuts for these clicks and no blind input retries. Existing document preservation, history, export, undo/redo and save/reopen assertions remain.

| ARM diagnostic | Result | Retained functional evidence |
| --- | --- | --- |
| Baseline background removal | Passed | [result](baseline-background-result.json): preserved pixels, cancellation, one undo step, undo/redo, save/reopen, Radius changed |
| Candidate background removal | Passed | [result](candidate-background-result.json): same checks |
| Baseline upscaling | Passed | [result](baseline-upscale-result.json): upscale-first/cutout, cancellation, no partial canceled export, undo/redo, save/reopen, 4× document export, persisted 2× photo copy, custom 2.5× photo |
| Candidate upscaling | Passed | [result](candidate-upscale-result.json): same checks |

Both background runs assert initial Radius 12 and report final Radius **59**. The candidate's `04-refined.png` was visually inspected and shows 59 px. Decoded mask comparison found:

- Candidate versus baseline with the corrected protocol: exact RGBA equality.
- Candidate Radius 59 versus the earlier same-candidate Radius 12 diagnostic: 62,474 changed pixels, maximum channel difference 175.
- Corrected ARM candidate versus previously passing original-driver Intel and HP candidate masks: eight changed pixels, maximum channel difference 1. Cross-architecture output is not described as exactly equal.

The [mask receipt](matte-validation.json) retains dimensions, decoded RGBA hashes and exact difference counts. These comparisons were completed before continuation timing began; this publication step reads only the existing small receipts.

## Identities and evidence handling

[source-receipt.json](source-receipt.json) pins all five helper source files, including unchanged entry points/pipelines. The two changed files are:

| Source | SHA-256 |
| --- | --- |
| `src/bin/background_removal_qa/native.rs` | `ef45748b158fe4f9c49d3bee9b2ac670c68185b55e9ed0d6913b31b1ed4a55bc` |
| `src/bin/upscale_qa/native.rs` | `3693e664be2bb853abf82b41070f22b77dd8ef9903e1bcc4d3790626b44daa64` |

[ARM builds](aarch64-build-receipt.json), [x86 baseline builds](x86_64-baseline-build-receipt.json) and [x86 candidate builds](x86_64-candidate-build-receipt.json) retain exact executable/dependency hashes and direct-rustc commands. They use optimization level 3, one codegen unit and LTO disabled. Successful product-library rebuild commands are retained [separately](baseline-library-build.json). The four diagnostic proofs here are ARM; x86 build success alone is not presented as a native functional run.

[receipt.json](receipt.json) records original and published evidence hashes. Absolute local paths are replaced with named placeholders; timings, assertions, result values and source/build hashes are unchanged. Original failed/provisional attempts remain in the artifact archive and are excluded from the matched final native measurements, not relabeled as passes. This directory does not constitute human desktop QA.

After measurement, the two checked-in QA files were normalized with rustfmt. The [format proof](source-format-proof.json) records both sets of hashes and verifies that formatting the exact archived measured sources produces the checked-in files byte-for-byte. All benchmark source/build receipts retain their original identities. The final formatted files pass the all-target check and targeted rustfmt check.
