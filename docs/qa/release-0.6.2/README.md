# 0.6.2 release acceptance

Michael confirmed human QA passed and explicitly authorized finishing and shipping #62, #67 and #83 on September 30, 2026. The accepted September 12 0.5.0 build (`69c35f6cc3f4397a883798172a0d654cb05289e4715d0678db7822c823e6791d`) and its evidence remain preserved.

## Native input

An isolated Sway Wayland session on the M1 Pro runs the production Studio UI through `clipboard_qa`, with real fcitx5/Rime input. No changes were made to Michael's desktop input settings. The runtime candidate window follows the transformed canvas caret; composing `nihao` and selecting the first candidate inserts `你好`. Escape cancels composition. Ctrl+C copies CJK text to the actual Wayland clipboard, and Ctrl+V inserts `粘贴測試 🎨`. Undo returns to the start of the typing session; Ctrl+Y restores it; Ctrl+S saves the document.

- [Native event results](wayland-ime-results.json)
- [Rime composition and candidate window](rime-preedit.png)
- [Committed canvas text](rime-commit.png)

The same real fcitx5/Rime commit, copy and save also pass in Pixel and Layout: [results](wayland-persona-results.json), [Pixel](pixel-ime.png), [Layout](layout-ime.png). The isolated display keeps a virtual keyboard connected before opening each app window, so the compositor sends the initial keyboard focus/IME activation.

`issue83_probe` exercises the real Studio UI in Design, Pixel and Layout, including IME activation and Unicode insertion. This injected-event check is separate from the real Wayland/fcitx test. Ten targeted IME/clipboard tests pass, covering draft isolation, undo, cancellation, focus transfer, pointer interruption, surrounding deletion, transformed caret output, and delayed clipboard delivery across composition. The QA helper fails fast if its CJK font is missing; `QA_CJK_FONT` can select an installed font.

Full local library suite: **977 passed, 0 failed, 8 ignored**. Two old ellipsis test failures depended on the machine's default font; their fixtures now use the bundled EBGaramond font. Product ellipsis behavior is unchanged.

## Cloud review

The authenticated browser test uses the real Clerk development instance and deployed Convex development services. It creates all five stamps, highlight and brush strokes, a reverse-drag rectangle and a CJK comment pin; replies; resolves/reopens; reloads; switches snapshot versions; deletes a mobile stamp; and verifies no mobile horizontal overflow. [Browser results](browser-results.json) report nine persisted marks and no browser errors.

The native Studio review window loads the same browser-created stamps, strokes, rectangle, pin and reply on their original flat export: [native review](native-cloud-review.png).

The production Convex schema and functions deploy successfully. New geometry remains backward compatible with existing pin/rectangle rows. The site suite passes **21 tests**, including geometry bounds and point budgets, anonymity, role isolation, immediate revocation, thread deletion and immutable snapshot binding.

Local access to `clerk.omadesign.app` is intercepted by Spectrum Security Shield (plain HTTP redirects to `block.charter-prod.hosted.cujo.io`). The development browser test is therefore recorded separately from production delivery checks.

## Packaging and runtime

Desktop Cargo builds, tests and portable packages run locally. Both final archives pass the GLIBC 2.35 ceiling, including their bundled runtime libraries.

| Archive | SHA-256 |
| --- | --- |
| ARM64 | `e88169b848360b58032aeba546c516db71f629f1e782e72d181445a03cdd0121` |
| x86_64 | `b10319176e9bdd3a99e656956b15ab9b856affb198bb8976637f19c69924b066` |

- [ARM64 receipt](arm64/receipt.json): exact archive installed into a fresh prefix, offline version/inspect, CJK document reopen, and production editor capture with an observed Asahi GPU descriptor.
- [AMD physical x86_64 receipt](hpeliteclient/receipt.json): 13 checks passed.
- [Intel physical x86_64 receipt](intelpro/receipt.json): 13 checks passed.

The x86_64 checks install the exact archive into a fresh prefix, verify binary/runtime hashes, inspect/convert/save/reopen documents offline, and capture the shipped editor with observed AMD/Intel GPU descriptors. Retained supplemental helpers separately exercise real model inference with the archive's installed ONNX Runtime. Those helper checks are not represented as current production UI inference. Existing installations and source fixtures remain unchanged.

The final browser and native fixtures were deleted from Convex development storage, their paired devices were revoked, and the two disposable Clerk users were deleted. The temporary internal cleanup function was removed from the development deployment.

The Blacksmith dashboard at `https://app.blacksmith.sh/michaelmonetized/runs/jobs?instance=github` reports that personal GitHub accounts are unsupported. This explains the queued site and Ship jobs with runner ID 0. Hosted workflows now skip unsupported personal repositories; this skip is not a test pass. Local site tests, typecheck and production build passed; the Vercel Git preview is READY. Production delivery uses a locally built Vercel prebuilt artifact. No GitHub-hosted compute or paid cache/artifact storage was used.
