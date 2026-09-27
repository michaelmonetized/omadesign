# Review regression followup

The original native screen recording remains the end-to-end proof for the inspector, range edits, history and save/reopen flow. These focused regressions cover the additional review cases:

- Pointer click/re-entry and drag clear pending insertion features.
- Clipboard formatting materializes effective run features and legacy ligature, tabular-figure and small-cap defaults before pasting into different defaults.
- A 100,000-character uniform selection produces one style interval; a single batch overlay preserves adjacent destination styles.
- Native clipboard ownership carries the rich payload beside ordinary text. Identical text copied by another application has no rich provenance and cannot reuse earlier formatting.
- Delayed paste is rejected after the destination caret, text session or tab changes.

Unit tests isolate the clipboard transport from the desktop. `text_clipboard_qa --native` separately exercises the production Wayland transport with actual `wl-paste` reads and an independent `wl-copy` owner. It creates no window. The native receipt records which formats were actually offered and the assertions that passed.

The #151 source is checked on its own PR base, with the upstream text-schema media-generator correction. Character metrics and manual pair positions are added by the separate #152 followup.
