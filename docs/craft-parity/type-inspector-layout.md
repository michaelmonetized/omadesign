# Area-text Character controls

The actual desktop Type workflow exposed an area-frame Character pane showing only width. Width and height used two full-width labelled controls in one unbounded horizontal row. The first took the row; the second label wrapped in the remaining space and pushed the other controls below the viewport.

The fields now use two bounded columns. Existing values, ranges, speeds, labels and typed frame edits remain. Their accepted native responses also feed the existing field-history collector, so typing a dimension stays one Undo operation. Insets and the optional line-limit number use that same existing collector.

Source `ae5f5a60` passed all four focused checks: real dock widths 256/288/420, window resizing, native dimension typing as one Undo, exact native/image reopen, and the two existing keyboard frame routes. The first `4117e896` attempt stopped before tests because a formatting filename banner entered the fixture; that receipt remains retained.

The genuine desktop binary passed actual width/height edits, separate Undo/Redo, a 1600×1000 → 1200×800 → 1600×1000 window cycle and separate-process cold reopen. Independent JSON/byte inspection found only the intended width, height and derived wrap-width changes. Native saved files match exactly after Undo/Redo, resizing and cold reopen. Evidence: `implementation/craft-type-inspector-layout.json`. The private test apps stopped; user windows/settings were untouched. This is software qualification, not human acceptance.

A separate native gap remains: keyboard “Resize panel” updates one group while another stacked group can retain a larger width, so their shared column does not shrink. That is the next fix. Broader Character/Type and full Craft parity remain open. No model, shaping, schema, dependency or app-version change.
