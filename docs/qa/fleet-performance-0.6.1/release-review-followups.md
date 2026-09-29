# Agent attachment review follow-ups

The 0.6.1 release review confirmed and fixed two functional issues from [PR #168](https://github.com/michaelmonetized/omadesign/pull/168): attachment metadata could make a saved transcript exceed History's file-size limit, and one failed image preview could reject an otherwise permitted file batch. The transcript writer now bounds the complete serialized file on its worker; preview failures retain the original file reference and valid siblings. Focused tests cover metadata and escaped-text budgets, History reopening, preservation of an existing save on an oversized header, and mixed valid/invalid image batches.

Two separate findings remain follow-up work; this release patch does not claim to fix them:

- [Unreferenced cache payloads](https://github.com/michaelmonetized/omadesign/pull/168#discussion_r4117723632): rejecting a prepared paste at the attachment limit, cancelling an in-flight paste, or removing an unsent attachment can leave application-owned files in `.omadesign/agent-attachments`. Repeated use can grow disk usage. Cleanup must account for every current draft, pending turn and saved-history reference, and must never delete externally referenced files. No broad deletion was added during the release fix.
- [Copied-object summary allocation](https://github.com/michaelmonetized/omadesign/pull/168#discussion_r4117724488): preparing a turn currently reads and parses the complete copied-object document, up to the 100 MiB file cap, then serializes it before retaining a 4,000-character summary. This runs on the attachment worker but can allocate substantially more than the eventual summary. A bounded-prefix implementation is a separate optimization and should preserve file validation and delivery behavior.

Neither finding changes the completed fleet measurement inputs or warrants relabeling their measured results. Neither is a claim that all attachment resource behavior has been optimized.

## Final cache admission correction

The combined release test run exposed an address-dependent cache admission bug after the fleet measurements. A deterministic sequence of 80 aligned keys made the earlier approximate frequency sketch miss 79 objects on the next traversal of a 63-entry cache: a false high count admitted a cold key, evicting the first resident and starting a sequential eviction cascade. Exact aging request counts now retain the expected 63-object subset, including after repeated scene-priority resets. This changes retention decisions, not cached pixel keys or rendered output.

Counts still halve after every 1,024 requests and saturate at 255. Their sum, and therefore the number of retained keys, stays below 2,048. The exact map adds at most approximately 75 KiB of metadata per cache; the existing 64 MiB object-effect, 64 MiB layer/group and 16 MiB independent-appearance pixel budgets remain unchanged. Tab/edit resets retain the map allocation and valid cached pixels. Deterministic collision, aging and many-unique-key tests supplement the unchanged renderer regression asserting that over-budget scenes retain a useful subset.

The published fleet timings remain measurements of the explicitly recorded candidate commit, before this release correction and the attachment/retouch review fixes. They are not new measurements of the final packaged release.
