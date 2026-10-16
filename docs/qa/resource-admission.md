# Initialized storage and pending admission

The shared pool keeps every reservation in its existing total capacity limit.
Host admission additionally charges all pending bytes plus the existing headroom
against a fresh `MemAvailable` observation. Initialization and that observation
use the admission mutex, so a reservation cannot gain initialized credit between
a stale host observation and the admission decision.

Only `Allocation` construction after filling/copying a Vec, exclusive adoption
of its actual initialized length, or a completed checked `write_at` grants credit. Its real capacity is a shrink
floor; spare capacity stays pending. A private owner token prevents binding one
lease to two live allocations. Arc aliases share the same buffer, lease and credit.
There is no byte-count-only credit API. Native opaque decoder estimates, RAW's
legacy sample reservation, and unfinished legacy codec builders remain pending.
An owned output `write_at` admits both old and replacement capacities before
growth. It credits only bytes actually initialized by copying or zero-filling;
unwritten spare capacity remains pending. Photo's seekable output uses this owner.

An Allocation drop locks admission, frees its Vec, removes initialized credit,
and then drops its lease. A separately retained lease remains a pending
reservation after the buffer dies. `into_parts` keeps the full lease and capacity
floor while restoring credit to pending for the plain Vec transfer. A subsequent
exclusive adoption restores credit from that Vec's actual length. No samples are
copied and no reserved bytes are released by the transfer.

Point adjustments and framing are evaluated directly into the developed output.
Only clarity retains the source-sized neighborhood buffers it requires. RAW
PNG16 writes bounded big-endian rows; TIFF borrows the admitted u16 input. The
shared Photo export adopts its encoded output and retains it through atomic file
write. Native `.oma` save uses `project::encode_admitted` and retains its owner
through write, sync, rename and failure rollback. String/Vec compatibility
wrappers and other conservative builders remain outside aggregate route proof.

This source cut adds deterministic partial-capacity, concurrent initialization /
host-observation, alias, adoption, copy, cancellation, shrink and pressure checks.
It awaits the coherent native/Photo consumer execution. It does not establish a
physical RSS ceiling, qualify a camera mode, change native serialization or raise
the shared pool or host budget. Other processes can still change available memory.
