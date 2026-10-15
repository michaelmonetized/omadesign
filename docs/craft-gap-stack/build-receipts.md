# Locked build receipts

The shared target cache reuses executable paths across commits. A wrapper that hashes an executable after the build helper exits may hash the next job's binary. `scripts/serialized-cargo.mjs` now captures Cargo's executable artifact records and streams their SHA-256 hashes while the global build lock remains held.

The invocation is unchanged:

```sh
node scripts/serialized-cargo.mjs CONFIG REPOSITORY FULL_COMMIT test --locked --lib -- FILTER --test-threads=1
```

Omatainer has a binary test target; omit `--lib`. Use the appropriate package/target for EditBay. Both `--target-dir PATH` and `--target-dir=PATH` are refused. The helper owns Cargo's message format for `test` and `build`; it preserves ordinary test output and rendered diagnostics.

After Cargo exits, the helper hashes only the executable paths emitted by Cargo, including cached artifacts. It writes an immutable JSON file at:

```text
CONFIG.scratch/receipts/REPOSITORY/FULL_COMMIT/RECEIPT_SHA256.json
```

Standard error prints `Cargo receipt: ABSOLUTE_PATH` after the file has been written and synced. The filename is the SHA-256 of the complete JSON bytes, including their final newline. Receipt creation and executable hashing finish before the next queued job can change the checkout or shared target.

Schema 1 records the exact revision and source tree, clean checkout state before/after Cargo, original and executed Cargo arguments, command exit code/signal, helper/module/config hashes, timestamps, executable path/hash/size and Cargo target/profile/cache metadata. `receipt_errors` reports binding failures. A missing executable or changed canonical checkout makes the helper fail even if Cargo succeeded. Cargo failures retain their exit code and receive a failure receipt.

An empty artifact list means no executable was bound; the helper never searches the cache for a plausible binary. A `--no-run` build has no test execution proof. Likewise, an artifact record alone does not show that a fixture ran or passed. Qualification must separately validate test counts, fixture/reference bytes, required checks and evidence tiers.

Use the printed receipt for that invocation. Existing receipts without a binary binding remain historical or unavailable for checks requiring one; hashing today's reused path cannot repair them. No product rerun is required solely to install this helper change. Queued jobs invoking the central helper load the updated inner script when they acquire the lock. Consumers invoking an isolated older helper must update both `serialized-cargo.mjs` and `cargo-receipt.mjs`, or use the central path.

Six focused Node tests use real Git/flock and a tiny fake Cargo process. They cover cached artifacts and streamed output, queued executable replacement, build-only arguments, absent artifact events, failed Cargo, missing executables and both forbidden target-directory forms. They create no Rust build, target cache or application process, and remove their own fixtures.
