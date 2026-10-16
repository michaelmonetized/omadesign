# Live native editing

Open **File → Live collaboration…**. Push a project to cloud first, then choose **Start live edits for this project**. Give the session ID to another project editor. They choose **Join in a new document**. Joining opens a separate tab and preserves their current document.

The service must include the `live` Convex functions and the updated `/api/cloud` handler. Browser authoring is separate from this native workflow. Reviewers cannot join or download live editable state; every join and exchange rechecks the existing cloud membership and device credential.

Each client merges properties against stable native object IDs. Independent objects and properties survive concurrent edits. Moves retain the same object identity. Competing values are ordered by observed logical time and a stable actor ID, and remain in the saved history. Undo withdraws only this client's transaction; it cannot reverse another client's writes. Redo restores the original transaction priority. A concurrent move wins an unseen deletion; a deletion made after observing the move still applies.

**Work offline** pauses exchanges. Continue editing, save the `.oma`, then choose **Reconnect**. The saved file contains pending changes and local undo history. Network failures retain the same queue and retry with bounded backoff. A lost acknowledgement safely resends the same operation ID. Reopening on the same device resumes its actor while a local file lock prevents two processes from writing that actor at once. Another device or a simultaneously opened copy forks a fresh actor and preserves its unsent visible work.

Cursors, editor names and selected objects appear while the editor is connected. Presence expires after 30 seconds without a heartbeat. Background tabs pause exchanges; opening the tab resumes them.

Conflicting parent cycles are detached. Children added to a deleted layer are recovered into a surviving vector layer or a deterministic recovery layer. Missing design-variable bindings are detached while retaining their appearance. Cyclic or missing artwork-mask references are detached while retaining source and target artwork. These repairs are reported as conflicts and become durable on the next authored edit.

Pixel buffers travel as bounded, lossless compressed data; a standard blank Layout canvas fits the live seed. Each pixel buffer merges as one property. Live seeds are limited to 500 KB, and individual live edits to 190 KB. Large pixel documents should use the existing source/snapshot transfer. The native status retains the pending queue and explains oversized edits; saving locally retains the full artwork. The service pages its durable log rather than truncating history.

## Qualification

Focused native tests cover independent edits, disconnect/reconnect, same-property conflicts, author-owned undo/redo, stable moves, orphan recovery, enum changes, parent cycles, mask/variable conflicts and `.oma` save/reopen. Service tests cover authorization, forged actors and undo, idempotent retry, sequence validation, revocation and presence expiry.

The ignored native HTTP acceptance test launches two separate native test processes against an isolated local Convex deployment through the same request handler used by `/api/cloud`. It saves and reopens offline `.oma` files, reconnects both clients, undoes and redoes each client's changes, and compares independently parsed final snapshots. It does not open desktop windows. Hosted deployment and two visible native desktop windows remain separate qualification gates.

Session quotas use small transactional metadata. Existing sessions are counted one seed per transaction before the first new session; interrupted initialization resumes automatically through the native HTTP route. The 100-session limit remains enforced during concurrent creates.
