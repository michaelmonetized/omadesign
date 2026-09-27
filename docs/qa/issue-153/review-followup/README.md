# Appearance review followup

Source `0c7308cd6ef9ec4c0aeca3ab7679554c6f3c0683` fixes three confirmed appearance regressions. [Five focused checks pass](focused-tests.log); [result and file hashes](result.json) identify the tested source and copied executable.

- Raster-to-Layout-frame conversion now carries Fill opacity and interior grouping alongside object opacity, blend, and filters. The regression checks converted appearance and restoration on Undo. [Review](https://github.com/michaelmonetized/omadesign/pull/162#discussion_r4116751951).
- HTML `drop-shadow` and inset `box-shadow` multiply color alpha by effect opacity. The regression covers an invisible outer shadow and a half-opacity inner shadow with a partially transparent color. [Review](https://github.com/michaelmonetized/omadesign/pull/162#discussion_r4116762206).
- Versionless `omadesign-shapes:` clipboard effects receive the same legacy migration as old projects. Explicit modern effect fields retain their settings, including knockout and independent blend. [Review](https://github.com/michaelmonetized/omadesign/pull/162#discussion_r4116751950).

The [cross-layer PDF finding](https://github.com/michaelmonetized/omadesign/pull/162#discussion_r4116751947) was **disproved by executable comparison**, so PDF production behavior is unchanged. Regular layers intentionally isolate their children in `compositor::draw_layer`: a blue Multiply object/effect over red in the same layer becomes black; the same object/effect in a separate ordinary layer stays blue. Four fixtures cover both combinations. Their exported PDF round trips and actual Poppler `pdftocairo` renders match the native compositor pixel-for-pixel, including the red background. The `.oma`, PDF, compositor reference PNG and Poppler PNG for each case are included here.

These are supplemental file/export checks, not another UI recording or performance benchmark. The passing native screen recording in the parent directory remains unchanged. Tests were run locally from an isolated worktree, using a copied test executable to avoid concurrent relink interference.

Reproduce the focused PDF check with the test executable and `OMA_EFFECT_REVIEW_QA=/absolute/evidence/path`; that opt-in writes fixtures and invokes installed `pdftocairo`. Without the variable, the regression checks the PDF round trip using the application importer and requires no external programs.
