# 0.6.2 release acceptance

Michael confirmed human QA passed and explicitly authorized finishing and shipping #62, #67 and #83 on September 30, 2026. The accepted September 12 0.5.0 build (`69c35f6cc3f4397a883798172a0d654cb05289e4715d0678db7822c823e6791d`) and its evidence remain preserved.

## Native input

An isolated Sway Wayland session on the M1 Pro runs the production Studio UI through `clipboard_qa`, with real fcitx5/Rime input. No changes were made to Michael's desktop input settings. The runtime candidate window follows the transformed canvas caret; composing `nihao` and selecting the first candidate inserts `你好`. Escape cancels composition. Ctrl+C copies CJK text to the actual Wayland clipboard, and Ctrl+V inserts `粘贴測試 🎨`. Undo returns to the start of the typing session; Ctrl+Y restores it; Ctrl+S saves the document.

- [Native event results](wayland-ime-results.json)
- [Rime composition and candidate window](rime-preedit.png)
- [Committed canvas text](rime-commit.png)

`issue83_probe` exercises the real Studio UI in Design, Pixel and Layout, including IME activation and Unicode insertion. This injected-event check is separate from the real Wayland/fcitx test. Seven targeted unit regressions cover draft isolation, undo, cancellation, focus transfer, pointer interruption, surrounding deletion and transformed caret output.

Full local library suite: **976 passed, 0 failed, 8 ignored**. Two old ellipsis test failures depended on the machine's default font; their fixtures now use the bundled EBGaramond font. Product ellipsis behavior is unchanged.

## Cloud review

The authenticated browser test uses the real Clerk development instance and deployed Convex development services. It creates all five stamps, highlight and brush strokes, a reverse-drag rectangle and a CJK comment pin; replies; resolves/reopens; reloads; switches snapshot versions; deletes a mobile stamp; and verifies no mobile horizontal overflow. [Browser results](browser-results.json) report nine persisted marks and no browser errors.

The production Convex schema and functions deploy successfully. New geometry remains backward compatible with existing pin/rectangle rows. The site suite passes **21 tests**, including geometry bounds and point budgets, anonymity, role isolation, immediate revocation, thread deletion and immutable snapshot binding.

Local access to `clerk.omadesign.app` is intercepted by Spectrum Security Shield (plain HTTP redirects to `block.charter-prod.hosted.cujo.io`). The development browser test is therefore recorded separately from production delivery checks.

## Packaging and runtime

Desktop Cargo builds, tests and portable packages run locally. Site deployment uses Blacksmith only. Fresh archive hashes and physical-machine receipts are added after the final package run; site CI does not substitute for native runtime evidence.
