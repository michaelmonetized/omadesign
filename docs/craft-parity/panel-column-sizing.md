# Shared dock sizing

The native Character-pane check exposed a preference mismatch: the keyboard Panels dialog resized only one stacked group, while the shared workspace derives the column width from the widest group. A smaller request could therefore leave the actual column unchanged.

Resize panel now updates every visible group on the selected dock edge, matching the existing pointer divider behavior. Other edges, hidden groups and separate floating windows remain independent. The existing captured-workspace validation, Apply and real focus request remain.

Source `25229831` passed all six focused panel-dialog checks, including actual native Apply and rendered shared-column geometry, rejected dimensions, stale/cancel handling, preference reopen and independent floating windows.

The genuine desktop binary then passed the original failure case: a right column starting with 320/420-pixel groups became 256/256 after one native dialog action and 420/420 after another. Independent JSON inspection found no other workspace changes; the saved document remained byte-identical. The private test app stopped and user windows/settings were untouched. Evidence: `implementation/craft-panel-column-sizing.json`. This is software qualification, not human acceptance.

No document/history, workspace model, dependency, schema or version change. Full Craft parity remains open. Earlier pointer-divider input attempts were inconclusive and are not qualified by this keyboard correction.
