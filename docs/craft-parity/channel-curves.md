# Saved alpha Curves

The Channels pane opens Curves for the concrete saved alpha target. The graph edits 2–19 natural cubic control points or a 256-entry float drawing. Point inputs and outputs use the familiar 0–255 display scale; the actual channel remains float coverage. Arrow keys move the selected point, Page Up/Down changes selection, Insert adds a point, and Delete removes it. Tab reaches the native numeric controls. Draw mode supports continuous strokes and Shift continuation, smoothing and a bounded fit back to points.

Preview uses a proxy with a maximum edge of 512 pixels. Original toggles the displayed proxy. Neither touches artwork or history. Apply runs against the captured full-resolution float plane and pixel selection, then uses the existing channel command for one Undo step. Cancel retires its worker. Document, source or selection changes retire the dialog rather than applying to a new owner. Invalid or unordered points cannot apply.

The Pixel Color pane shows the actual foreground used by the brush (or the current fill tool). Foreground changes update tool settings rather than recoloring a selected object. Vector and text color controls keep their existing typed commands.

The implementation follows the local PhotoCraft reference's natural spline, point/drawing modes, smoothing and constant endpoints. Native evaluation uses original float inputs rather than an eight-bit lookup table. This layer does not implement color-channel Curves, reusable adjustment layers, eyedroppers, presets, curve-file interchange or full PhotoCraft parity. Existing Live/PSD refusal for saved channels remains.

Production source `811a0bdbcc12b1f5eb6f579f90071200e85c5997` passed all eight directly affected Curves tests in 27.10 seconds, including actual native graph/numeric/Apply/Cancel and full-document Undo/reopen. The preceding `f9f2db19` cohort passed 28 tests and failed one drawing test; that failure led to the exact-entry smoothing correction, with assertions preserved. Foreground and existing paint/channel passes remain attached to that earlier receipt rather than becoming a new combined total.

The genuine current desktop also passed actual point input, proxy/Original, drawing/smoothing/fitting, Cancel, Apply, single Undo/Redo and fresh-process load/resave. An independent Node reader checked all 480,000 native adjusted samples against the declared linear curve (maximum absolute error 3.624e-8), unchanged artwork and exact Cancel/Undo/Redo/cold files. Native Ctrl+V accepted an alpha hex foreground without changing the project or inserting an Undo step. This is software/native UI evidence, not human acceptance.

Run [the isolated QA launcher](../../dist/omadesign-channel-curves-qa-811a0bdb/run) to test it. The existing human QA window stays open. Public release/version reconciliation and complete parity remain open.
