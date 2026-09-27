# Review regression followup

The original 48-second native recording remains the end-to-end proof for character controls, shortcuts, history, save/reopen and exported results. Followup regressions cover:

- A metric change on just the second character of `fi` splits that affected ligature while the adjacent uniform `fi` remains a ligature. Tracking, kerning, baseline shift and both scales are checked against real EB Garamond glyphs.
- Optical kerning keeps positive and negative tracking additive; per-glyph positions and total widths are checked numerically.
- Ranged HTML tracking adds the paragraph letter-spacing CSS variable.
- Native rich-text copy materializes effective metric defaults and preserves interior manual-pair positions, including character-index rebasing. Legacy #151 clipboard payloads default to no manual pairs.

`metrics-tests.log` covers the first shaping fixes; `clipboard-tests.log` and `clipboard-text-tests.log` cover the later clipboard integration. The native OS transport, actual app Copy/async Paste, external identical-text replacement, and undo/redo are recorded in the #151 supplemental receipt.
