# Craft gap PR program

The 25 source-confirmed gaps from [the comparison](https://github.com/michaelmonetized/omadesign/pull/219) are assigned once in [program.json](program.json). Each group owns working behavior, focused checks and stacked PRs. The 28 proposed cuts reserve Omadesign 0.6.4–0.6.31; they are not published releases or claims of completed work. Omatainer and EditBay keep their existing version schemes.

0.6.4 combines the installed brush fixes (#209) with the mask, SVG and import-feedback stack (#211 → #213 → #215 → #218), preserving the previously qualified source behavior. The coordinator adds consistent stable version preparation and a single shared build lock. No new binary qualification is implied by preparing version metadata.

Use `node scripts/version.mjs stable 0.6.4` to prepare Cargo.toml, Omadesign's Cargo.lock entry and the Rust notices lock hash together. Invalid, regressive and already-tagged versions fail before writes. Stable preparation leaves the website's published download versions alone; update those only when the corresponding release exists. Existing nightly preparation retains its public padded counter.

Builds run through `node scripts/serialized-cargo.mjs CONFIG REPOSITORY FULL_COMMIT test [FOCUSED_ARGS]`. The local JSON config names one dedicated clean build worktree and an existing target cache per repository, plus shared scratch and lock paths under /home. The helper holds a global flock, checks out only the requested committed source, limits Cargo to two jobs, and removes its temporary directory afterward. Never point the canonical worktree at a user's checkout. Agents must not create extra target caches, run cargo clean, package release archives or launch intrusive desktop/audio tests.

The eight listed explorations are research for Michael's alignment discussion. Their findings do not authorize new implementation scope or determine the route to 0.7.0. Accepted QA, rollback evidence, private fixtures, original artwork and the delivered client document remain protected.

Trackers: [Omadesign #216](https://github.com/michaelmonetized/omadesign/issues/216), [Omatainer #517](https://github.com/michaelmonetized/omatainer/issues/517), [EditBay #70](https://github.com/michaelmonetized/editbay/issues/70).
