# Native ACP harness and selection dialogs — 2026-09-27

## Source and migration

Work is in the primary `/home/michael/Projects/omadesign` checkout on
`feat/live-design-agent-selection`, based directly on `origin/master`
`a4d5b2faa2df976bfad164658a3732fd058d78d6` (0.6.0).

The initial implementation was mistakenly made on `preview/cloud-dreamscape`
at `f4754fc` (0.5.8), missing 48 upstream commits. After the discrepancy was
identified, only the two requested features were ported onto current master.
The original preview branch and unrelated artwork were preserved. A source
archive, patch, and file manifest are retained at
`/home/michael/Documents/Omadesign-QA-20260927-acp-migration/`.

Conflict resolution preserves upstream's library-button and zoom placement,
screen color picker, animation export, selection add/subtract behavior, and
arrow-key selection movement. The existing inspector's fixed resize, feather,
and distort buttons now use the same configurable selection menu as the top bar.

## Delivered behavior

- A docked native conversation panel connects to local ACP agents over stdio.
- Native MCP tools inspect, create, style, select, and remove editable objects;
  rendered canvas snapshots let the agent inspect its composition.
- Each accepted operation appears in the editor and uses its command history.
  Revision checks protect concurrent manual edits; document changes disconnect
  the session before queued tools can modify another canvas.
- Explicit provider permission choices, immediate cancellation of queued native
  writes, saved transcripts, session reconnect, and read-only native canvas mode.
- Pixel Select offers Move, Resize, Grow, Shrink, Feather, and Reshape dialogs
  with asynchronous previews, Apply/Cancel, and correct soft selection coverage.

## Automated validation on 0.6.0

- `cargo test --lib --locked --offline`: **672 passed, 0 failed, 5 ignored**.
- `cargo check --all-targets --locked --offline`: passed.
- Native application, `agent_qa`, and `capture_studios` build offline.
- New agent/selection modules pass `rustfmt --check`; `git diff --check` passes.
  Existing upstream formatting outside this work was preserved.
- Tests cover native object creation, text, gradients, frame parenting, history,
  saved documents, revision conflicts, locked objects, read-only sessions,
  bounded context, socket authentication, ACP permissions/cancellation, document
  isolation, prompt entry, and keeping Stop visible in constrained windows.

Provider initialization succeeded against Codex ACP 1.13.1, Claude ACP 0.81.2,
Gemini CLI 0.61.0, and OpenCode 1.18.32. Only Codex has been exercised through a
complete live-design turn; the other providers' checks are handshakes, not full
design or authentication tests. Handshake evidence is retained in
`target/qa/agent-harness-2026-09-27/provider-handshakes.json`.

The earlier Codex run on 0.5.8 produced 12 editable objects through 14 live edits,
then resumed the saved session and changed only `& FORM` to `& FIELD`. Those
captures remain under `target/qa/agent-harness-2026-09-27/{create,resume}` and are
historical evidence, not validation of the current base.

## Native selection verification on 0.6.0

The WGPU/Wayland `pixel-selection` run recorded **660 frames at 30 fps** with
**0 unresolved input targets**. It drew an ellipse selection, opened all six
Select dialogs, applied Move/Resize/Grow/Shrink/Feather, and cancelled Reshape.
The capture confirms the upstream asset-library and bottom zoom placement.
Evidence is under `target/qa/pixel-selection-current-2026-09-27/`.

This run preceded the inspector shortcut consolidation. After that small routing
change, the 10 focused selection tests passed, all targets checked, and all three
native binaries rebuilt successfully.

## Live ACP verification on the final 0.6.0 build

A fresh Codex session completed **14 native edits**, producing **12 editable
objects** in 127 seconds with no errors. The agent inspected snapshots and
corrected the title spacing. Fifteen native WGPU screenshots show the progression.

A separate process restored the saved session and document, changed the title
from FERN & FORM to FERN & FIELD with **one native edit**, then inspected another
snapshot. It finished in 28 seconds without errors. A structural comparison of
both saved `.oma` files found exactly one changed value: the title's text content.
The session ID and original transcript prefix were preserved.

Current evidence is in `target/qa/agent-harness-current-2026-09-27/`:

- `create/` and `resume/`: editable documents, PNG exports, native screenshots,
  transcripts, and result JSON.
- `resume-verification.json`: exact document diff and session/history checks.
- `edit-progression.mp4`: a 15-second sequence of the actual native edit captures,
  paced at one screenshot per second; this is an edited progression, not realtime.
- `build-manifest.json` and the validation logs: local executable hashes and checks.

Final local debug application SHA-256:
`a4e8ef82ad303eee9ce972050faa5990c4728e57d0cc90c979a2e9325607083e`.

No installed application or public release was replaced. The accepted 0.5.0
human-QA evidence in `AGENTS.md` remains unchanged.
