# Craft gap PR program

The 25 source-confirmed gaps from [the comparison](https://github.com/michaelmonetized/omadesign/pull/219) are assigned once in [program.json](program.json). Each group owns working behavior, focused checks and stacked PRs. The original 28 cuts reserve Omadesign 0.6.4–0.6.31. Michael approved nine more milestones, 0.6.32–0.6.40, after discussing all eight explorations separately. These are source milestones; preparing a number does not publish a release or qualify unfinished work. Omatainer and EditBay keep their existing version schemes.

0.6.4 combines the installed brush fixes (#209) with the mask, SVG and import-feedback stack (#211 → #213 → #215 → #218), preserving the previously qualified source behavior. The coordinator adds consistent stable version preparation and a single shared build lock. No new binary qualification is implied by preparing version metadata.

0.6.5 migrates the existing number field to shared controls and adds shared panel primitives. Existing large coordinates remain unchanged during idle rendering, locked controls cannot alter values, and field responses distinguish drag previews from release commits. The exact source at `516b965e984d9791bbd373a8389b429cd7b6934e` passed six focused tests; shared source hashes match all three apps. PR #223 prepares this milestone without publishing or installing a release. Contextual geometry and task panel integration follow in separate cuts.

Use `node scripts/version.mjs stable 0.6.4` to prepare Cargo.toml, Omadesign's Cargo.lock entry and the Rust notices lock hash together. Invalid, regressive and already-tagged versions fail before writes. Stable preparation leaves the website's published download versions alone; update those only when the corresponding release exists. Existing nightly preparation retains its public padded counter.

Builds run through `node scripts/serialized-cargo.mjs CONFIG REPOSITORY FULL_COMMIT test [FOCUSED_ARGS]`. The local JSON config names one dedicated clean build worktree and an existing target cache per repository, plus shared scratch and lock paths under /home. The helper holds a global flock, checks out only the requested committed source, limits Cargo to two jobs, and removes its temporary directory afterward. Never point the canonical worktree at a user's checkout. Agents must not create extra target caches, run cargo clean, package release archives or launch intrusive desktop/audio tests.

The helper writes [source and executable receipts](build-receipts.md) before releasing that lock. Read the printed receipt path instead of hashing a shared target executable after another job can replace it. The receipt records compilation and command outcome; fixture execution and acceptance remain separate checks.

The [approved follow-on scope](follow-on-scope.md) covers complete keyboard access, all advertised RAW families and modes, full component/grid/prototype authoring, stereo then 2.1/5.1/7.1 mixing, full audio delivery, full video formats and high-depth/HDR, and shared fixture qualification. Run affected fixtures per PR and the full agreed cohort before a public candidate. Undo/clipboard corrections and existing format integration remain within the original milestones. The 0.7.0 release decision follows qualification; no version-only release is needed.

Accepted QA, rollback evidence, private fixtures, original artwork and the delivered client document remain protected.

Trackers: [Omadesign #216](https://github.com/michaelmonetized/omadesign/issues/216), [Omatainer #517](https://github.com/michaelmonetized/omatainer/issues/517), [EditBay #70](https://github.com/michaelmonetized/editbay/issues/70).
