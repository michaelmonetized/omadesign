# RAW camera qualification

PHOTO-03 qualifies an exact source file, camera and capture mode. Recognizing an
extension, using the same camera body, or decoding another compression mode does
not qualify a case. Camera JPEG looks are outside this contract.

The [manifest](qa/raw-camera-manifest.json) inventories every one of the 36
extensions in `src/formats/raw.rs::EXTENSIONS` and the format guide. It includes
80 candidate cases: 61 independently pinned public source URLs/hashes and 19
cases awaiting a licensed source. The inventory names concrete capture axes and
records every remaining axis as unqualified. It is not an exhaustive camera
support list. The exact M50 RAW case passed its strict workflow at
`b3041919d443dad35ec3387b76a13e275fb41db9`. The other 79 cases remain
unqualified. X2D 100C references are prepared; reference preparation does not
qualify application decoding, placement or export.

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
need parent resource scheduling. The approved `coverage-6565` exception retains
a 3 GiB aggregate allowance and at least 6 GiB free after reserving that full
allowance. Its source still must fit 512 MiB.

The workflow counts the original, reference files, explicitly listed compact
evidence and owned source/export/settings files. It checks each source-copy
chunk and each shared export write, includes the codec output bound before
encoding, and rejects links or unrecognized scheduled exceptions.
`OMA_RAW_SCHEDULED_CASE_STORAGE` points to a private JSON descriptor containing
`case`, `source_sha256`, `temporary_bytes`, `minimum_free_bytes` and `files`. Only
the exact approved X2D source and 3/6 GiB values can enlarge the default allowance.
This descriptor grants test storage, never product support or a camera pass.
A case keeps at most one source copy and one export on disk.
It removes only its exact owned source/export/settings paths and retains compact
JSON/log receipts. Original files, their sidecars and cached/public fixtures are
never deleted. No resource measurement runs concurrently with a compiler here.

## Exact receipts and remaining evidence

The original optics/local/catalog stack retains its 87-test receipt at
`984245418d82bd6c396d879e89837cbbaafa7828`. The RAW affected cohort passed
103 tests at `6492b77debebc9628f433d42236b9a860fb953f9`; its original linker I/O
failure and authorized identical retry are both preserved. These receipts remain
bound to their original commits.

At `b3041919d443dad35ec3387b76a13e275fb41db9`, the pinned Canon M50 RAW strict
workflow passed one case. Full sensor samples matched DNGLab exactly (25,504,128
samples); developed RGB matched the LibRaw integration reference exactly
(72,649,440 samples). Independently decoded PNG16 and TIFF16 pixels matched.
Normalized preview/full maximum error was 0.001960779; edited settings save/reopen
maximum transfer error was 0.000007728. Original source hashes before/after matched.
The sampled process-lifetime high-water was 876,068,864 bytes; it includes
reference and workflow work and is not an isolated decoder peak.

The same commit passed 25 resource/cancellation/preset tests, including actual
M50 native cancellation in 283.503 ms, immediate distinct-source next-open in
4022.665 ms, preserved current edits/source, no stale publication, and isolated
preset persistence/reopen. Its test executable hash was
`b115c23686e4b4dd35214dbe947a26ca79a5e35c59c554f99fca8deeef1b1330`.
FNV-1a64 `62261f0ba81cfcd2` is over each little-endian sensor u16 byte,
including masked borders; it is not a per-u16 word xor. Native qualification
compares sensor samples with the pinned file; it does not itself compute that FNV.
Suite's reviewed pure-Node consumer retains this exact producer and one qualified
row, leaving 79 unqualified and the full candidate false. It does not replay a
camera job or promote the receipt to later source.

RAW-to-Design software consumer checks passed 7/7 at
`56c9805fe465cf58f329209a4cf2040e734a35c1`. They exercise actual placement
handlers, stale source/destination refusal, cancellation, undo, independent scalar
OETF, F32 native storage, exact project reopen, actual destination ICC/model/intent,
float TIFF and explicit conversion before PNG16 export. Direct CMYK TIFF preserves
ink bits exactly. The float compositor's `1-(1-ink)` arithmetic is measured
separately: maximum 2.98023224e-8, within the 5.96046448e-8 bound. The earlier
6/1 result at `198a5013` is preserved as an overly strict composite comparison.
These calibration/handler checks do not prove 102 MP placement or a native window.

The exact CC0 X2D 100C source was acquired at 213,311,488 bytes, SHA-256
`dcc5a4abe3498e6f25e89bb491995fd12c3b669d9277c71b45b49210d3e56280`.
Scheduled reference preparation passed 1/1 at
`8d5a6f7a9e82916620746b8fb0af826147f9c993`. DNGLab's 11904x8842 sensor plane
matches attributed byte-FNV `c07a267c0bc60a5f`; the separately generated LibRaw
RGB16 reference is 11664x8750 (102.06 MP). Sensor/reference SHA-256, executable
hashes, command flags and original before/after hashes are retained. PNM is
converted directly to canonical little-endian samples without another full PNM
file. Actual case files used 1,036,187,252 bytes. Measured tool peaks were
437,072 KiB for DNGLab and 1,229,888 KiB for LibRaw. A shared 2,818,572,288-byte
reservation covered the serial tools and bounded streaming process; child address
space was limited to 2.5 GiB. These are reference-process measurements, not
application decode/placement acceptance or a product memory limit.

X2D's independent EXIF model, LibRaw camera label and internal normalized model
remain distinct. Its source default crop `[128,96,11656,8742]` does not replace
the measured LibRaw output. The independent active-area value is absent. Standard
TIFF tag inspection also finds no ActiveArea tag, DNGVersion or OpcodeList1/2/3;
MakerNotes are not interpreted as DNG opcodes. `active_dimensions` remains null,
and required preflight refuses this case rather than inventing an active area.
At `c8093a50fca4dc38104d7c7a4f1f22bf2bdf881b`, the real Photo load returned
11664x8750 with 306,180,000 linear u16 samples. Process-lifetime sampled high
water was 1,905,360,896 bytes; retained admission was 673,613,632 bytes and returned
to zero after final owners dropped. Source hashes were unchanged. The affected
cohort was 11 passed / 1 failed: a second open while retaining the current photo
was refused at the generic 3,982 MiB native request, before sensor stage 8. The
1.12 ms cancellation of that admission wait is not active decode proof; next-open
failed safely and current edits/source stayed intact. The full failed receipt is
preserved. Application precision, settings reopen, native placement/export and
active cancellation for this X2D source remain unproven.

The follow-up demand repair recognizes only `unpacked_load_raw()` with ordinary
three-color Bayer, single-frame, unit-aspect, non-Fuji layout, normal ushort pitch
and at most 32 MiB metadata. With the adapter's fixed AHD settings, LibRaw 0.22.2
`unpack.cpp`, `raw2image.cpp`, `ahd_demosaic.cpp` and `mem_image.cpp` bound the
simultaneously live padded ushort sensor, padded four-channel ushort image and
RGB16 output. The request is their sum plus 64 MiB for metadata/tile scratch, or
two RGB16 outputs during the later native-to-Rust copy, whichever is greater.
Other decoders/layouts retain the generic estimate. No vendor source, shared-pool
policy or host-pressure test changes. The token records actual requests, including
refusals, so the real fixture check must prove which bound was used. This repair
still requires exact-source execution.

Nineteen cases still lack licensed source URLs/hashes. Other missing inputs,
references and mode identities remain explicit required failures. No body, firmware,
phone HDR mode, proprietary compression mode or other combination is inferred from
a representative fixture. Historical private fixtures need recovery and rights
review before binding or sharing. No camera-JPEG look match is claimed.

The complete qualified native/color dependency was consumed before the placement
adapter. The later shared admitted encoded-output/common integration is a separate
qualification stream; legacy bare-Vec encoder helpers remain outside aggregate
output-lifetime proof. The new scheduled workflow write guard and current high-res
case pins are source changes awaiting their coherent affected checks. No additional
Rust build is run just to refresh Suite pins. No installed/native-window/device,
physical, human or all-camera acceptance is claimed by these headless receipts.
