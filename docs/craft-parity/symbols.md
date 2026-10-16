# Vector symbols

This layer uses the existing component definition/instance model. It does not introduce a second symbol document or a new serialized schema.

Symbols contains actual definition thumbnails and native selection/drag responses. New creates a transparent, unclipped definition wrapper around adjacent top-level vector artwork on one layer, or uses an eligible existing frame. Place and canvas drops insert linked instances. Edit selects the original definition; ordinary edits propagate through existing component synchronization. Break Link retains editable instance artwork. Remove Definition clears links while keeping every instance's artwork, with one Undo step. Rename uses the captured definition name to avoid overwriting a concurrent edit.

Creation across layers or from roots inside another frame remains explicit follow-on work, along with external symbol libraries, redefinition and symbolism tools. Invalid, locked, hidden and stale targets must preserve the document. This source layer requires its own focused and native qualification; source authoring alone does not close the full symbol-parity item.

## Instance commands

Duplicate makes a separate definition beside the source artwork. It remaps the definition's family and slots, including exposed property bindings, so later edits remain independent. The copied artwork is visible on the canvas under the current component-backed symbol model; this is not a separate hidden library.

Select All Instances selects visible, unlocked instance roots of the chosen definition without changing the document or history, and reports skipped hidden or locked roots. Reset and Break Link act on the whole concrete instance selection in one transaction. Selecting both an instance and one of its children does not apply the command twice. Locked, hidden or missing members reject the complete mutation instead of partially editing the remaining instances.

Cross-definition replacement, redefinition from alternate artwork, hidden library-only definitions, external libraries and symbolism tools remain open. Existing automatic definition-to-instance updates continue through the shared component synchronizer.
