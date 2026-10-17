# Pixel command menus

Pixel mode's Image menu now opens the implemented tonal adjustments, and Filter opens the implemented key/transparency, blur, detail, stylize and distortion commands. These choices call the same raster or scalar dialogs as the Adjustments pane. Saved alpha and Quick Mask keep their captured full-depth route; artwork and layer masks keep their existing route. Curves remains available for its implemented scalar targets. Remove Background remains layer-only.

The pane and top menus share one category chooser. Existing labels, parameters, preview workers, Apply/Cancel, history and archive code remain. Invalid, locked and busy targets disable command choices. Panel shortcuts in Image are explicitly labelled as panels. No unsupported reference menu labels were added as pretend commands.

The four existing raster-dialog checks passed at `5de63125`; that cohort's four new menu fixtures failed before command activation. A later fixture-only attempt at `71b2c7c5` identified the pinned native submenu labels' arrow suffix. All four corrected menu tests passed at `a45b3355` in 33.71 seconds: every existing choice opens and cancels through the actual menu, and Image adjustments apply independently to artwork, a layer mask, saved alpha and Quick Mask with Undo/Redo and native reopen. Invalid, locked and busy targets remain disabled. These are separate exact-source results.

The genuine `a45b3355` desktop build passed. Actual Image-menu soft-selected Invert at strength 0.375 and Filter-menu radius-2 Grow Alpha matched independent calculations bit-for-bit across 480,000 samples each. Actual Undo/Redo and a fresh-process native save/reopen matched exactly, with artwork, profiles and other channels unchanged. The first desktop attempt applied whole-plane Invert correctly; its batched selection-load/menu input did not establish a soft selection. That output and diagnosis remain separate from the corrected run.

Run `dist/omadesign-pixel-command-menus-qa-a45b3355/run` for the isolated QA build. Exact receipts, screenshots, saved files and independent readers are retained under the shared `implementation/craft-pixel-command-menus` handoff. Private test desktops were stopped; no user desktop or document was changed.

Full Craft menus, RGB Curves and full parity remain open. No dependency, schema or app-version change.
