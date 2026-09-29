# Preserving measurements across native-driver recovery

The first paired matrices produced valid authoring, CPU-render and command-line AI measurements. Native AI later exposed a driver race: inference completion removed progress content and moved the dialog's Cancel button between pointer press and release. The missed cancellation left the modal open; the assumed reopen gesture could then close that old modal through its backdrop. The missing-control assertion correctly failed the process. Those failures and successful superseded native runs remain historical evidence.

The recovery replaces all eight native AI runs per host: two workflows, two repetitions and both production versions. Both versions use identical corrected native-driver sources. The corrected protocol uses hovered atomic semantic-button clicks, held coordinate-slider gestures, and Escape cancellation with explicit modal-state assertions. It does not claim Cancel-button coverage. Original production revisions, model/runtime/input identities, and all other workload measurements remain separate from this driver change.

[continue_paired.py](continue_paired.py) separates preparation from execution. Preparation requires a stopped, sealed parent manifest and a new output directory. It copies ordinary files, never hardlinks or edits the parent. Passed non-replaced runs keep their exact receipts and output bytes. Failed, partial and superseded runs and profiles move only inside the new copy to `excluded-attempts/<parent-manifest-sha>/<side>/<case>-<repetition>/`. Execution starts only missing or explicitly replaced cases, in the original alternating order, with fresh profiles. The original frozen runner still supplies workload arguments, process timing, network isolation, hardware-descriptor checks and functional validation.

```sh
python3 continue_paired.py prepare \
  --parent ORIGINAL_OUTPUT --output NEW_DERIVED_OUTPUT \
  --runner run_paired.py --native-overrides HOST_NATIVE_OVERRIDES.json

python3 continue_paired.py execute \
  --output NEW_DERIVED_OUTPUT \
  --baseline-bin ORIGINAL_BIN --candidate-bin CANDIDATE_BIN \
  --inputs ORIGINAL_INPUTS --runtime PINNED_RUNTIME \
  --candidate-build COMPLETED_CANDIDATE_BUILD.json \
  --display-env PRIVATE_DISPLAY.json --swaymsg SWAYMSG \
  --expected-drm-driver DRIVER
```

These are command templates, not execution receipts. `prepare` never launches a workload. Passing `--native-overrides` always replaces all native AI cases on both sides; it cannot silently replace only a favorable sample. A failed continuation must be sealed and used as a new parent rather than resumed by editing its existing measurements.

The override JSON has `schema_version: 1`, the exact emitted `protocol`, and `sides.baseline` / `sides.candidate`. Each side contains `background_removal_qa-native` and `upscale_qa-native`, each with:

- `path` and `identity: {bytes, sha256}` for the actual executable.
- `production_revision` matching that side's original application revision.
- `build_receipt: {path, identity, artifact_key}` pointing to a completed build receipt.
- `harness_sources`, mapping relative source names to `{path, identity}`.

Each native build receipt must contain `success: true`, `production_commit`, `architecture`, `artifacts`, nonempty successful `steps` with `exit_code: 0`, and the exact `harness_sources` identity map. The baseline rebuild additionally retains its source/library proof showing that the original application code was unchanged; the report audits that proof. Both sides' declared native-driver source identities must match. Build receipts and all declared sources are copied into the derived output, so a later audit does not depend on the original host's absolute paths.

The native result must match the override's `qa_input_protocol`, with `cancel_input: "Escape key"` and `cancel_button_covered: false`. Background removal also requires `matte_radius_changed: true` and an integer `matte_radius > 12`, matching the driver's assertion that its slider gesture changed the default setting. These checks supplement the original undo/redo, document preservation, saved reopen, export and window-size assertions.

The manifest's `continuation` field contains the parent manifest and recursive file-inventory identities, copied runner and continuation-tool identities, the parent's status, retained origins, excluded attempts, and new attempt origins. Every selected entry has an `origin`. Retained origins name the parent receipt and exact artifact inventory. Recovered origins name the collector hash, incremented attempt index, receipt/output identities and optional native override/protocol. Original receipt timestamps and commands are never relabeled as new measurements.

`validate_continuation(output)` is a read-only audit entry point. It verifies selected origin rows against retained/new-attempt records, rehashes their receipt/output inventories, checks excluded run/profile mappings, and validates copied native build/source evidence. It does not execute processes or resolve original remote source paths. The report independently checks this lineage and the original build/input/runtime identities.

`success` describes the selected complete matrix only. `all_attempts_passed` remains false when any ancestor or recovered attempt failed. The report must distinguish the 108 selected runs from failed and superseded attempts, and label the native AI protocol cohort separately from the original authoring/CPU/CLI cohort. Per-case process timing excludes continuation inventory checks, which occur between processes; recovery occurred later in time and does not establish identical thermal or cache conditions across cohorts.

[test_continue_paired.py](test_continue_paired.py) uses synthetic files and a fake runner to check immutability, tamper rejection, native-cohort replacement, missing-case execution, attempt numbering, profile-only exclusions and protocol metadata. It launches no application, compositor or benchmark workload.
