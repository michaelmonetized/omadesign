# Nested-frame canvas selection

Selection guides now follow the same frame placement as the painted artwork. Selecting a rotated group or an object inside it leaves the document untouched. Node points, Bézier controls, corner controls, gradient endpoints and the text caret use that placement too.

Picking reverses ancestor Motion poses and frame rotations in painter order. Hidden, locked and clipped ancestors reject pointer hits. Marquee bounds follow ancestor placement. Static frame handles retain the existing layout constraints; children keep their type, styles and rotation. Motion frame move/resize/rotate gestures key the outer selected target once, with descendants inheriting it.

The correction reuses `compositor::frame_transform` in both frame painting and canvas geometry. It adds no document fields, file feature, dependency, version or alternate renderer. Existing typed commands and gesture history remain the mutation routes.

## Verification

Source-bound tests exercise actual canvas pointer input, guide and caret paint positions, nested static rotations, nonuniform ancestor Motion poses, clipping and lock checks, node/handle editing, gradient placement, frame constraints, one-step Undo/Redo and native save/reopen. Existing canvas and frame-rendering checks remain separate exact receipts.

A private native desktop run uses the retained two-story rich-text document that exposed the bug. Selection-only save matches the original bytes. Group movement retains all IDs, parentage, typography and unrelated artwork, with exact Undo/Redo. The initial native uneven-resize run exposed a child-rotation loss, which is retained as a failure and corrected before final delivery.

This is a bounded canvas correction. Full Craft parity, public release/version reconciliation and human acceptance remain open. Static text content remains editable; this is not a typography or geometry-format migration. Existing Motion scale grips retain their uniform scale behavior.

Final native source `afa58327a98b50ff9a887ef34fcf40e6bf34eed2` preserves both rich-story rotations through the uneven resize. Selection-only bytes match the retained original; native Undo/Redo and a new-process reopen match the resized file exactly. The independent JSON reader also verifies every other type/style field and the unrelated rectangle.

The 53-case group at `73f0a684` passed 51 and failed two new fixture setups; the corrected fixtures plus caret/movement checks passed 5/5 at `bee59a84`. Motion-root handles passed 2/2 at `2da6c511`; the native-discovered rotation correction passed 2/2 at `afa58327`. These are distinct exact-source receipts, not one combined final-head run. Earlier compile refusals and the native resize failure remain in the handoff.
