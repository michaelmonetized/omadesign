# Paired fleet rerun: preparation and reporting

The report is generated only after all three hosts finish the real paired
workloads. There are no placeholder measurements. The intended 0.6.1 candidate
still identifies its package as 0.6.0 before release; use source and executable
hashes to identify the tested revision.

The baseline is the preserved issue #158 application source
`221834bdd8af5aee8554a0e835629cbe2f467558`. Original authoring, CPU-render,
CLI and portable-app measurements use retained binaries. The recovered native
AI cohort uses an explicitly pinned source-equivalent baseline rebuild with
the common corrected QA input protocol described below. It is not the shipped
v0.6.0 tag. The original 2026-09-27 fleet table
is retained separately; it ran on the users' Hyprland desktops. New paired
runs use private GPU-backed Sway displays. Historical and fresh samples must
never be pooled. [Display and collector provenance](environment/README.md)
retains the exact Sway configuration, hardware evidence and excluded preflight
attempts separately from completed timing suites.

## Workload and identities

Each host runs one process at a time. Baseline/candidate order reverses for
successive case/repetition pairs. Per version and host:

- Two native authoring runs, using the original infographic seed, typing tasks
  and accompanying project font bank.
- One CPU canvas profile: four calls for each individual layer, then four
  full-scene calls. The first full-scene call is not a cold-process/cache call.
- Two runs each of three upscaling and two background-removal CLI cases.
- Two runs each of native background removal and native upscaling.
- One portable-application reopen/capture.

This produces 18 processes per version/host and 108 across the fleet. Both
authoring windows are exactly 1440 × 900. Their final recorded canvas sizes are baseline
1020 × 770 and candidate 1014.71875 × 771 on all three hosts, validated without
tolerance. The current character-spacing inspector grid sets a slightly larger
minimum sidebar width; the candidate's final canvas area is 0.389% smaller.
The harness records canvas geometry only at the end of each run. The inspector
can expand during text actions after the initial fit, so these values do not
describe every phase and do not establish equal zoom. Every run's final canvas
dimensions remain in the report; timings are not normalized by area. Scripted
typing, pen and brush locations are world coordinates transformed using the
current view; pan steps are two horizontal and one vertical screen point.
View fitting and floating-point round trips can differ, so artwork equality is
audited from actual exports and documents rather than presumed from action
counts. The separate fixed-size CPU canvas profile uses equal raster geometry.
Authoring artwork exports remain exactly 1200 × 900. Candidate authoring
also establishes pointer hover/focus before input; common measured action counts
stay equal, while candidate coordination includes extra hover/setup calls.
These harness/geometry differences remain explicit in every claim.
The first HP/Intel attempts stopped at the previous geometry assertion and are
retained separately as failed preflight evidence, not pooled into timed results.

The final table selects 108 successful processes; it does not claim every
recorded attempt passed. Some original native AI runs stopped at missing-control
assertions. Their receipts and artifacts remain immutable. A separate
[continuation tool](continue_paired.py) copies the stopped/completed parent,
retains successful authoring/CLI measurements byte-for-byte, and archives failed
or superseded native attempts with exact inventories and parent identities.
The [lineage audit](audit_continuation.py) rechecks those copies and selected-run
origins before any final report is generated.

All eight native AI cases per host are rerun with identical QA helper source on
both versions under `hover-atomic-buttons-held-sliders-escape-cancel-v2`.
Semantic buttons use hover plus atomic clicks; coordinate sliders retain held
press/release frames. Cancellation uses the actual Escape key, and results
explicitly report `cancel_button_covered: false`. Background removal asserts
initial Radius 12 and records a changed final Radius greater than 12. These
native QA measurements form a separate recovery cohort with pinned source,
build and executable overrides; original native timings are not silently mixed
into it. This protocol does not establish Cancel-button coverage.

The native baseline library rebuild uses checkout
`587433037d1df9be5e66b36cc77bc6f44fcd7071`, whose 825 tracked production inputs
match `221834bdd8af5aee8554a0e835629cbe2f467558` exactly. The audit independently
recomputes the Git tree comparison, excluding only `docs/` and the standalone
`src/bin/authoring_qa.rs` and `src/bin/canvas_profile.rs` files. Runtime assets,
library source, manifests, vendor source and build scripts remain included.
The canonical tree SHA-256 is
`7590d7cc3c119079ef0f29ee320e07ace467aa3f22ee282236238f74a8d30dfd`.
This proves source equivalence, not identity with the original executable bytes;
the rebuilt native binaries, compiler and build receipts are separately pinned.

The runner checks the original host manifest's baseline binaries, inputs and
ORT libraries, and a candidate build receipt containing all successful checks,
binaries and runtime libraries required for that host's architecture. It retains
immutable snapshots of both receipts. An architecture readiness receipt may be
captured while the other architecture is still building; it does not assert
global build completion. Before publication, the summarizer requires a completed
successful global build and verifies the readiness receipt's exact source,
compiler, Cargo and profile, every recorded step, and every artifact against it.
The snapshot retains its separate identity and capture scope. Model hashes and
ORT 1.28 CPU are checked for every CLI AI case.
The summarizer rechecks the snapshots, input bytes, final build identities,
all process exits and functional evidence, and identical x86 executable/runtime
bytes between HP and Intel. Each host must match its assigned frozen runner
hash: original HP and Intel runs use [the fdinfo collector](runner-versions/run_paired-fdinfo.py),
while original M1 and all recovered runs use [the Asahi-compatible collector](run_paired.py). Both exact source
identities are pinned. The summarizer requires their complete parsed programs
to be identical except for the `gpu_handles` function. Asahi's kernel lacks DRM
fdinfo on this host, so that function also checks an actual open render-node
character descriptor and resolves its driver through sysfs. It does not infer
GPU use from installed libraries or compositor selection. Actual driver/device
evidence is retained; timing, scheduling, actions and every other function stay
identical. This narrowly checked collector difference does not waive the GPU
requirement or permit arbitrary runner changes.

## Aggregation

After timed workloads finish, collect each completed runner output without
changing its contents into `COLLECTED/<host>/` for `hpeliteclient`, `intelpro`
and `m1pro16`. Run the potentially substantial image/hash audit only when it
cannot contend with timed work:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 summarize_paired.py \
  --root COLLECTED \
  --candidate-build FINAL_BUILD_RECEIPT.json \
  --inputs ORIGINAL_INPUT_DIRECTORY \
  --output NEW_SUMMARY.json

PYTHONDONTWRITEBYTECODE=1 python3 make_report.py \
  --summary NEW_SUMMARY.json \
  --output NEW_REPORT.md
```

The default historical evidence is the adjacent
`fleet-benchmark-2026-09-27/` directory. `--historical` can select another exact
copy. `--repository` identifies the Git checkout used to verify the pinned core
AI source identities. Outputs must be new: prior evidence is never overwritten.

Authoring medians/p95/maxima are recalculated from raw UI, input-to-UI and
UI-completion samples, retaining every phase and individual repetition.
Conventional medians average the middle pair; p95 is nearest rank
`ceil(0.95 × n)`. Old/new duration ratios below 1 report a slower candidate.
Per-run medians/ranges and p95 support accompany task-specific claim candidates.

Whole-process time includes startup/loading/workflow waits/encoding and uses
100 ms polling. Two AI observations produce a mechanical p95 equal to their
maximum; this is not a stable tail estimate. Reopen has one observation per
side. Short AI wall-time ratios can be dominated by polling quantization.

Native AI harnesses retain only per-run UI p95/max, not raw frame durations.
Those values remain explicitly harness-reported and are never pooled or
reconstructed. Their retained helper calculation selects sorted index
`floor(0.95 × n)` with zero-based indexing, distinct from the nearest-rank rule
used for raw authoring samples. They include many idle/workflow frames and do not predict dense
authoring speed. Core AI source equality is checked at the pinned revisions;
unchanged inference code/models/runtime must not be marketed as a new inference
algorithm optimization.

## Output correctness and limits

For both repetitions, compare within-version repeatability, same-host
before/after, and m1pro16 against each x86 host on both versions. Each comparison
includes 19 actual PNG exports/mask outputs and three saved documents. PNGs
are decoded to RGBA; dimensions, exactness, differing-pixel counts, channel
maxima/means and decoded hashes are retained. Native screenshots are checked
for dimensions but not treated as comparable artwork at arbitrary UI states.

Raw document byte differences remain visible. The strict ID helper permits
only a seed-protected bijection of fresh layer/shape IDs, resolving every
modeled reference and retaining all other fields, types and array positions.
Unknown/default fields are not removed to manufacture equality. Other JSON
differences receive counts and paths without embedding artwork/field values.
A separate additive proof verifies the pinned current serde defaults, initializes
only absent reference-side `fill_opacity`, `blend_interior` and
`text_wrap_above_only` fields, then repeats the strict ID/reference comparison.
Raw byte and ID-only results remain unchanged. The [output review](OUTPUT-ACCEPTANCE.md)
records the resulting document proof and quantified pixel differences; the
generator does not silently apply a tolerance or claim all output is unchanged
merely because processes passed their internal assertions.

These are as-used local comparisons, not controlled hardware experiments.
Retain load, memory, pressure, frequency-policy and per-process resource
observations. Occupied swap does not prove paging or establish RAM as the
cause of a slowdown. No metric here measures presented FPS or physical
mouse-to-photon latency. Public claims must name the task, host, statistic and
issue baseline, preserve slowdowns and qualify the two-run variation.
