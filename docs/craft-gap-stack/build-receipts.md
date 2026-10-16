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

`queue_helper_sha256` is the additional schema-1 hash of `scripts/build-queue.mjs`. Older receipts lack that field and retain their original qualification boundary. The hash identifies queue code available to the helper; it does not prove that an outside caller used the queue. In particular, a legacy outer `flock` can still invoke the updated `--inside` helper directly. No old receipt or no-run result becomes FIFO or execution proof.

An empty artifact list means no executable was bound; the helper never searches the cache for a plausible binary. A `--no-run` build has no test execution proof. Likewise, an artifact record alone does not show that a fixture ran or passed. Qualification must separately validate test counts, fixture/reference bytes, required checks and evidence tiers.

Use the printed receipt for that invocation. Existing receipts without a binary binding remain historical or unavailable for checks requiring one; hashing today's reused path cannot repair them. No product rerun is required solely to install this helper change. Consumers invoking an isolated older helper must update `serialized-cargo.mjs`, `cargo-receipt.mjs` and `build-queue.mjs`, or use the central path.

Future outer helper invocations register FIFO tickets under `CONFIG.lock.queue`. Order means atomic ticket registration order, not process launch time. Only the live head approaches the same global `CONFIG.lock`; existing direct-flock callers continue to share heavy exclusion. **Old queued direct-flock jobs are not retroactively ordered.** They may enter between future tickets. Leave every existing caller and waiter in place.

The queue holds `.lock` only for short registration, head cleanup and removal. Each unique ticket has an exclusive kernel lock retained by an open descriptor. Waiting uses blocking shared flock on the current head, without a scheduler or polling watcher. Released/dead tickets are cleared under the metadata lock. PID is informational: ownership follows the exact open file description, so PID reuse cannot keep a dead ticket alive or discard a live inherited child. SIGINT/SIGTERM cancels waiting submissions; active submissions forward the signal and retain ownership until the child closes. Executable hashing and receipt persistence still finish inside the heavy lock.

Suite's future wrapper can join the queue before its approved single outer flock:

```js
import { withBuildQueue } from './scripts/build-queue.mjs';

await withBuildQueue(config.lock, async ({ ticketFd }) => {
  // Spawn the existing outer flock with stdio: ['inherit', 'inherit', 'inherit', ticketFd].
  // Under that flock, run helper --inside no-run, then the actual Node adapter.
  // Forward cancellation to the owned child and await its close before returning.
}, { signal });
```

The exported signature is `withBuildQueue(lockPath, run, { signal } = {})`; it returns the awaited callback result. It never acquires the heavy lock. `ticketFd` remains owned by the queue; inherit it as descriptor 3 into the outer locked child, retain it through the adapter and let the queue close its original descriptor. Do not acquire another heavy lock or invoke the outer helper from that callback. A signal aborts queue waiting; the callback owns active-child signal forwarding and must settle only after `close`. If an outer caller dies, the inherited descriptor keeps its active child at the head. Existing Suite submissions remain unchanged.

Run `node --test scripts/serialized-cargo.test.mjs`. Fifteen focused Node tests retain the original six receipt cases and cover FIFO order, waiting/head cancellation, stale ownership with a reused live PID, killed queued/active owners, active cancellation through child close, legacy interoperability, the Suite wrapper contract and early argument rejection. They use owned fixture locks and fake Cargo only, create no Rust build or application process, and remove their fixture directories.
