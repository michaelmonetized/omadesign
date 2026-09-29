# Agent readiness follow-up

[Native failure/recovery recording](agent-recovery.mp4) · [Verification receipt](verification.json) · [Restored draft](restored-draft.png) · [Successful retry](retry-delivered.png)

The follow-up preserves unsent prompts and attachments after ACP startup rejection, process exit, disconnect, document changes, preparation failure, or Stop. If a newer draft exists, recovery keeps both drafts and moves pending paste destinations with the restored prefix.

The composer records TextEdit’s exact Unicode insertion/deletion positions, including repeated characters. Edits before a pending paste move its destination; edits into its destination cancel that paste without replacing the user's new text. Attachment completions still retain their original FIFO order.

The app-owned `.omadesign/agent-attachments` cache uses 0700 directories and 0600 files, including copied-object previews. Permissions apply at creation before writing bytes. Opening Agent upgrades known existing caches on a worker; pasting also hardens the selected project's existing cache. Symlinks and hardlinks are refused without following them, and externally referenced files retain their permissions and contents. A cache-local wildcard `.gitignore` excludes the entire cache even if the project becomes a Git repository later. The application does not rewrite Git history or remove already tracked user files.

## Regression evidence

[The initial three failing regressions](red-tests.log) reproduced the audited bugs on `13affc8f97492ebf18da4c41271826b1e56b84a2` with tests added: permissive cache, lost startup draft, and misplaced delayed paste. The behavior fix is `6ce220a7042babf6ba4d3a9f46266ed87881133f`; the final recorded source `c832f3240d77a268134943fa2148410ac46505d3` additionally corrects the status label after recovery and asserts that label in both startup-failure cases.

[29 agent tests](final-agent-tests.log) and [5 composer tests](final-composer-tests.log) pass. They cover real subprocess startup exit and initialization rejection, disconnect/document changes, typed concurrent drafts, exact Unicode/repeated-character edits through TextEdit, destination cancellation, legacy cache hardening, native text/image/object payloads and previews, external-file preservation, link refusal, and a normal `git add .` in a fresh repository.

Those 34 tests ran on `6ce220a7`. After the status-only correction, [all five readiness regressions](final-status-tests.log) pass on `c832f324`, as does [the all-target check](final-all-targets.log).

Builds ran locally with `--locked --offline -j2` under `/tmp/omadesign-readiness-cargo.lock`. Stable test/recorder executables were copied before releasing that lock. The recorded source and executable hashes are retained in the verification receipt.

## Native proof

The final **16-second, 1600×900 recording contains 160 frames at 10 fps**, with zero unresolved targets or errors. Full FFmpeg decode passed. The [receipt](verification.json) records the source/binary/movie hashes, migrated 0700/0600 cache modes, `git check-ignore` output and normal `git add .` exclusion, plus unchanged 0644 external-reference permissions. [Provenance](provenance.json) retains product-source and evidence hashes.

The native recovery replay pastes actual text and PNG clipboard MIME payloads into the Agent composer, sends two attachments to an ACP process which deliberately fails startup, types the next draft while connecting, verifies both drafts and attachment identities survive, prefixes the recovered brief, and retries successfully through a second real ACP subprocess. The local deterministic peer exercises the production transport; named third-party provider services were not exercised.

The [restored draft](restored-draft.json) and [sent conversation](conversation.json) have the same two attachment IDs. The [received ACP payload](acp-received.json) contains the complete recovered/edited request, one embedded text resource and one PNG image. The synthetic older cache begins at 0755/0644 and ends at 0700/0600 without losing its existing file.

The pending-worker edit cases use deterministic workers and the real egui TextEdit in focused regression tests; the movie covers native clipboard ingestion and startup failure/retry. The recorder captures the native WGPU framebuffer with pointer/key input at 10 fps. This is workflow evidence, not a rendering-performance benchmark. The original canvas stays unchanged; its `.oma` reference artifact is saved by the recorder.
