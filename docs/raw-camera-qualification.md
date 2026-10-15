# RAW camera qualification

PHOTO-03 qualifies an exact source file, camera and capture mode. Recognizing an
extension, using the same camera body, or decoding another compression mode does
not qualify a case. Camera JPEG looks are outside this contract.

The [manifest](qa/raw-camera-manifest.json) inventories every one of the 36
extensions in `src/formats/raw.rs::EXTENSIONS` and the format guide. It includes
80 candidate cases: 61 independently pinned public source URLs/hashes and 19
cases awaiting a licensed source. The inventory names concrete capture axes and
records every remaining axis as unqualified. It is not an exhaustive camera
support list. The M50 RAW case has verified identity and reference pins ready for
application checks. No camera case has passed those checks yet.

## Provenance

Fixture metadata comes from the existing shared LightCraft checkout at
`0da998f24dfc03069d49867f8877af2bf7c51f94`, specifically
[`xtask/src/main.rs`](https://github.com/storytold/lightcraft/blob/0da998f24dfc03069d49867f8877af2bf7c51f94/xtask/src/main.rs)
and
[`crates/raw/tests/cr3_corpus.rs`](https://github.com/storytold/lightcraft/blob/0da998f24dfc03069d49867f8877af2bf7c51f94/crates/raw/tests/cr3_corpus.rs).
The pinned source declares these fixtures CC0. Its MIT license was read before
reuse; the manifest retains its attribution and complete permission notice.
Only URLs, source hashes and attributed oracle metadata were reused in the repo.
Camera images and reference executables stay in private qualification storage.
Acquisition must match the named URL, license and digest before using the source.

Eight CR3 cases carry DNGLab 0.8.0 black-box sensor FNV-1a metadata and full sensor
geometry. Unprepared cases retain the upstream macOS release **archive** SHA-256;
M50 RAW now pins the actual official Linux aarch64 executable. An archive hash is
neither the executable hash nor the sensor-file SHA-256. These samples include
masked borders. Photo exposes oriented, white-balanced, demosaiced RGB, so those
sensor hashes cannot validate its developed pixels. The test-only native sensor
route now returns unpacked integer mosaic samples including masked borders. A
reference counts only after an independently generated, SHA-256-pinned same-plane
file is actually compared; this API alone establishes no camera pass.
LightCraft's own regression hashes and embedded JPEG agreement were not promoted
to independent oracles. The M50 C-RAW archive's 3:2 label conflicts with the pinned
4000x4000 shot crop; that exception is recorded without inferring native geometry.

The parent supplied HEAD-only receipts for the exact M50 nice URL (28,349,772
bytes) and iPhone 12 Pro data URL (29,195,592 bytes). M50 was subsequently acquired
and hash-verified; iPhone remains a HEAD-only receipt. The bare M50 `/4657` URL
returns HTML; acquisition uses the complete pinned nice URL. The iPhone data URL
redirects to `/download/data/`.

The original PHOTO-03 exploration, original implementation brief, audit acceptance
and `progress.json` alignment were read. The aligned scope is all advertised RAW
families/modes, including phone and high-resolution captures. `PHOTO-03.json` is
absent and was not invented. Existing three-file DNG/CR3/RAF statements and
PHOTO-01/02 files/receipts are unchanged; their missing source/reference identities
have not been recreated from new decoder output.

## Evidence contract

Each case pins source URL/license/SHA-256 and known camera, firmware, compression,
CFA, depth, sensor/active/output geometry, orientation, crop, WB and profile fields.
Archive camera labels remain candidate descriptions until in-file identity is
verified. Unknown values stay `null`. Missing required identity, source, reference,
executable hash, toolchain recipe/platform or successful comparisons fails a
required run. Firmware and optional crop/opcode/frame descriptions may remain
unknown without inventing a value. Non-exposed source tags still require an
independently documented tag inspection; the native assertions cover the actual
model/make, output geometry, orientation and WB.
When present, `identity.crop` records `[left, top, width, height]` in source sensor
pixels; it does not silently replace a decoder's measured output geometry.

Reference kinds have separate meanings:

| Kind | Required behavior |
| --- | --- |
| `libraw_integration` | Separately invoked, pinned system/upstream LibRaw. Checks integration; it is not an independent implementation. |
| `independent_render` | A separate decoder implementation with a pinned normalization recipe and the same oriented linear sRGB RGB16 or display sRGB RGBA16 representation. |
| `independent_sensor` | Pinned integer sensor mosaic including masked borders; compared only with the native unpacked mosaic, never developed RGB. |
| `display_look` | Optional look evidence only. Cannot satisfy a required correctness kind or count toward successful comparisons. |

Every case requires integration plus an independent kind. Reference files use
little-endian u16 samples without headers; dimensions and representation are
mandatory. `oriented-linear-srgb-rgb16le` compares the actual retained RAW pixels.
`display-srgb-rgba16le` compares the default PNG's independently decoded pixels.
`sensor-u16le-masked` is never compared with either RGB representation. Reference
SHA-256 and generating executable SHA-256 must be pinned independently before the
run; the runner never creates a camera reference from its own output. Record
compiler, architecture, color transforms, WB, crop, extraction program/version and
any measured tolerance in the reference provenance. Zero tolerances currently
mean exact equality; no cross-platform tolerance has been measured or granted.

Successful reference comparisons contain sample/changed counts, normalized
mean/max error and per-channel mean/max. Failed tolerance checks retain those
statistics and fail the case. All selected missing cases are accumulated while
available valid sources continue through the serial runner.

The actual Photo workflow loads an owned copy through `PhotoImage::load`, checks
the retained source identity, reads normal PNG/TIFF exports through the codec
decoder and insists on actual RGBA16, compares their pixels, checks independent
sRGB transfer/framing math, checks preview and full against a shared linear-first
normalization, changes exposure to +0.75 stop with a 90-degree turn and half crop,
saves settings, drops the loaded photo, reopens `.omaphoto`, and checks the edited
TIFF pixels. The full-render `AdmittedImage` owner stays alive through comparison;
RAW samples are accessed as slices. These workflow/mathematical checks do not count
as external decoder comparisons. They cannot turn a missing-reference case green.

Every case receipt includes original source hashes before/after, observed metadata,
reference tool identities, decoder version, test executable hash, exact revision,
Rust toolchain, Cargo.lock hash, OS/architecture, hashed producer hostname, workflow checks and classified
errors. Optional mode explicitly counts `checked` and `skipped`; a checked workflow
with missing evidence remains skipped. Hash corruption or a workflow failure still
fails optional mode. Resource samples report Linux process RSS/high-water bytes, live host availability
and the same shared pool at route milestones. The process lifetime high-water
includes qualification/reference work; it is not an isolated decoder peak. A case
fails if admitted buffers remain above its starting baseline after its last owners
drop. The codec validation reader uses checked byte admission and retains its
sample lease; it does not impose the codec default memory ceiling. Unsupported OS
counters stay null. These measurements imply no native-window, device or physical
proof. Active cancellation has a separate exact-source test and receipt.

## APIs and coordinator registration

The integrated test-only registration in `src/formats/raw.rs` is:

```rust
#[cfg(test)]
#[path = "raw/qualification.rs"]
mod qualification;
```

The integrated cut registers the test module and adds a native integer mosaic
reference path; it does not change the app/schema version. The runner expects `RawImage.pixels` slice API,
`PhotoImage::load(&Path) -> Result<PhotoImage, String>`,
`render_full() -> Result<AdmittedImage, String>` where the owner dereferences to
`RgbaImage`; `render_preview()` and `render_thumbnail()` also return a fallible
`AdmittedImage`. `export_to()` and `photo::edits::save()` retain their ordinary path
contracts. Linear preview samples share `Arc<Allocation<u16>>`; each original
display/thumbnail Arc retains independent admission through its final clone.
It makes no `PhotoImage::from_raw` call and does not construct `RawImage`, so the
new memory lease and fallible constructor require no special test bypass.

Rust test entry points:

```text
formats::raw::qualification::tests::raw_camera_contract_
formats::raw::qualification::raw_camera_corpus   (ignored real corpus entry)
```

Environment for the corpus entry:

| Variable | Meaning |
| --- | --- |
| `OMA_RAW_MANIFEST` | Optional absolute manifest path; defaults to the checked-in contract. |
| `OMA_RAW_MODE` | `required` (default) or explicitly `optional`. |
| `OMA_RAW_CORPUS` | Read-only corpus root: `raw/FILENAME`, `references/FILENAME`. |
| `OMA_RAW_OUTPUT` | New absolute directory under `/home` with a canonical existing parent; refuses an existing path. |
| `OMA_RAW_CASE` | Optional exact case ID. Unset processes every manifest case serially. Unknown IDs fail. |
| `OMA_RAW_CASE_BINDINGS` | Optional JSON object for that one case: `source`, `references`, `executables`. |
| `OMA_RAW_RUNNER_SHA256` | Required independently pinned SHA-256 of the compiled libtest executable. |
| `OMA_RAW_REVISION` | Required full 40-character integrated source commit. |

The Node driver sets these variables and validates them again against its own
receipt. A zero-test Cargo filter cannot pass because the required native receipt
will be absent. The ignored annotation protects ordinary tests from an accidental
real-corpus run; invoking this entry explicitly cannot silently skip a required
case.

## Commands, for the coordinator to schedule

The narrow Node self-test has run. Build and camera receipts must be read at their
exact committed revisions; queued/interrupted logs are not passes. Use the existing config at
`/home/michael/Projects/omadesign/.artifacts/craft-gap-stack/build-config.json`.
Every build/test goes through its shared global heavy lock and canonical target:

```sh
node /home/michael/Projects/omadesign/.artifacts/craft-gap-stack/tooling/c4a4ce84171cd41c0db544efbb1c99c57e4bd26c/scripts/serialized-cargo.mjs \
  /home/michael/Projects/omadesign/.artifacts/craft-gap-stack/build-config.json \
  omadesign FULL_COMMIT test --locked --lib -- \
  formats::raw::qualification::tests::raw_camera_contract_ --test-threads=1
```

Use the executable path and SHA-256 from the ordinary focused run's canonical
Cargo receipt. The shared helper owns Cargo JSON formatting and hashes the test
executable before releasing its lock. Do not submit another build merely to obtain
a binding receipt, and do not pass a custom message format. Verify those live bytes
against that exact source/tree/lockfile receipt, then supply the hash on the actual
run of the same commit/toolchain. A changed executable fails before decoding.

Metadata planning performs no acquisition, compiler invocation or decoding:

```sh
node scripts/raw-qualification.mjs plan --corpus /home/michael/RAW-CORPUS
node scripts/raw-qualification.mjs plan --case cr3-canon-m50-raw \
  --corpus /home/michael/RAW-CORPUS --bindings /home/michael/RAW-BINDINGS.json
node scripts/raw-qualification.mjs self-test
```

Bindings preserve cached/public/private file paths in place:

```json
{
  "schema_version": 1,
  "cases": {
    "cr3-canon-m50-raw": {
      "source": "/home/michael/RAW-CORPUS/cached-original.CR3",
      "references": {"libraw-integration": "/home/michael/RAW-CORPUS/reference.rgb16le"},
      "executables": {"libraw-integration": "/home/michael/TOOLS/dcraw_emu"}
    }
  }
}
```

This example supplies only one kind; it intentionally cannot satisfy the CR3
independent-sensor gate. Do not fill its missing reference by hashing native RGB.

After source/reference/identity pins and runner hash have been independently
established, prepare an existing canonical `.qa-runs` parent in this checkout and
request a new result directory:

```sh
node scripts/raw-qualification.mjs run --mode required \
  --config /home/michael/Projects/omadesign/.artifacts/craft-gap-stack/build-config.json \
  --revision FULL_COMMIT --runner-sha256 LIBTEST_SHA256 \
  --corpus /home/michael/RAW-CORPUS --bindings /home/michael/RAW-BINDINGS.json \
  --output .qa-runs/raw-required-NEW
```

The driver invokes exactly the serialized helper route, once per available case:
`test --locked --lib -- formats::raw::qualification::raw_camera_corpus --ignored
--exact --nocapture --test-threads=1`. It still records missing cases and continues.
Use `--mode optional` only for an explicitly partial developer report. `--case ID`
selects a narrower report; its result never qualifies the other 79 cases.

Future acquisition is explicit and serial, with the same shared `heavy.lock`:

```sh
node scripts/raw-qualification.mjs acquire \
  --config /home/michael/Projects/omadesign/.artifacts/craft-gap-stack/build-config.json \
  --corpus /home/michael/RAW-CORPUS --case cr3-canon-m50-raw
```

There is no archive unpacking or external decoder/reference command in the Node
driver. It streams one pinned CC0 HTTPS source into its own uniquely named partial
file, checks size/hash, links the completed file without replacing existing paths,
and removes only its own partial. Existing mismatched files are retained and fail.
Reference acquisition/generation remains a separately scheduled parent action.
Never pass the internal lock marker yourself.

The 512 MiB per-source and 1 GiB temporary budgets are acquisition/test scheduling
limits, **not product RAW size or pixel caps**. Larger phone/high-resolution cases
need parent resource scheduling. A case keeps at most one source copy and one
export on disk; the runner checks a conservative export allowance before writing.
It removes only its exact owned source/export/settings paths and retains compact
JSON/log receipts. Original files, their sidecars and cached/public fixtures are
never deleted. No resource measurement runs concurrently with a compiler here.

## Remaining evidence

Seventy-nine cases still lack complete pinned references and verified in-file
identity. Nineteen cases also lack source URLs and
source hashes. The 61 public source pins establish provenance, not local corpus
presence or decoding success. The pinned CC0 Canon M50 RAW and official
hash-verified DNGLab 0.8.0 executable are locally acquired. Its canonical sensor
reference matches the attributed DNGLab FNV exactly, and its separate LibRaw
integration reference is pinned at 6024x4020. Full sensor CMP1 tags confirm 14-bit
encoding at 6288x4056; decoded samples use 16-bit storage. DNGLab reports an active
rectangle of 6022x4018 and a 6000x4000 shot inset, which remain distinct from the
measured LibRaw output. Original source hashes before/after match. These are tool
and reference receipts; application comparison, settings reopen and active
cancellation have not passed. The remaining sources are not claimed present.
The eight DNGLab metadata pins alone cannot close the native sensor-plane
comparison. Historical private fixtures/reference bundles need recovery and rights
review before binding or sharing.

Contract tests cover missing corpora, hash changes, missing references, zero
comparisons, future/invalid schemas, wrong sensor planes, optional skips, safe
outputs and an original generated 64x48 DNG through the real Photo workflow. They
passed at `6492b77debebc9628f433d42236b9a860fb953f9`: 103 passed, zero failed,
three ignored. The first final link failed with `Input/output error`; one
authorized identical retry linked and passed. Both receipts are preserved.
Actual synthetic AHD cancellation stopped in 39.6 ms and the immediate next
64x48 decode succeeded. The strict corpus, private camera fixture and real M50
cancellation tests were ignored. Targeted Rust formatting and Node syntax checks
passed. The authorized narrow Node self-test
passed: `{"command":"self-test","status":"passed","contract_mutations":10,"camera_decodes":0}`.
It also checks missing/changed file pins, reference corruption, zero required runs
and optional skip accounting using four original bytes, then removes its own
scratch files. It downloads nothing and runs no compiler or decoder.
Native resource/cancellation/precision implementation is under exact-commit
qualification in PRs #259, #261, #263 and #264. Actual corpus/reference execution
and active cancellation proof remain pending. Precise Design placement waits for
the full qualified native/color dependency; no partial native cut bypass is allowed.
