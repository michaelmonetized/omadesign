# Contributing to omadesign

Build and contribute to the native Linux app.

## How to work on it

```sh
git clone https://github.com/michaelmonetized/omadesign.git
cd omadesign
cargo test
cargo run --release --bin omadesign
```

Rust 2024. Install Git, stable Rust, a C/C++ compiler and pkg-config. The C++
compiler builds the bundled RAW decoder; no installed LibRaw is needed.
`cargo` is the toolchain. No GTK app, no Electron. The website ships on
Blacksmith (`bun scripts/ship.mts`). Website checks and deployment may run on
Blacksmith. Desktop Cargo tests, compilation and packaging always run locally,
including both ARM64 and x86_64 builds. Do not use GitHub-hosted runners, paid
GitHub cache/artifact storage, or Blacksmith for Rust builds.

### Layout

```
src/
  geom.rs         points, bounds, Bézier, hit testing     (no UI)
  document.rs     layers, shapes, command history
  layout.rs       frames, auto-stack, constraints
  cloud.rs        opt-in sync, comments, showcase publish
  compositor.rs   tiny-skia renderer + PNG/JPEG export
  paint.rs        brush, erase, smudge, clone, fill, wand
  photo.rs        develop pipeline + histograms
  trace.rs        raster to vector (threshold and color)
  boolean.rs      union / subtract / intersect / xor
  text.rs         rustybuzz OpenType + glyph outlines
  tools.rs        personas, tools, shortcut table
  app.rs          studio state and document commands
  app/
    tabs.rs       document ownership and tab switching
    recovery.rs   background recovery snapshots
    shortcuts.rs  keyboard commands
    photo_session.rs  photo selection, previews and textures
  ui/             chrome, canvas, studios, photo, welcome
    jobs.rs       background asset, icon and font requests
assets/phosphor/  Phosphor Light (MIT)
docs/             manual, project status, contributing
site/             landing page (TanStack Start), showcase, compete
convex/           optional Convex schema for cloud
packages/schema/  shared Layout and cloud JSON schemas
scripts/          local release + curl installer
```

Mutations go through `Cmd` + `History`. Tests cover geometry, boolean, paint, develop, project round-trip, SVG, export, type, zoom, place, and trace.

Keep file and network work outside the frame loop. Tab switches transfer document state instead of cloning it. Rendering caches are derived from document data and invalidated when that data changes; they do not belong in saved projects.

### Project conventions

- **Complete tools.** Implement the advertised behavior; live text must remain editable.
- **No hardcoded UI colors.** Chrome reads the Omarchy / `~/.config` theme. Fallback is Catppuccin Mocha, used only when no theme is on disk.
- **Icons are Phosphor Light.** Use the glyphs in `src/ui/icons.rs`.
- **UI font is the desktop font.** Resolve it through Omarchy and fontconfig.
- **Deep modules.** `geom` and `text` have no egui types. Tests share the same seams.
- **Local builds.** `./scripts/release.sh` zig-links glibc 2.35 for aarch64 and x86_64. Run Cargo tests and build release packages locally. Blacksmith is reserved for the website; GitHub-hosted compute and paid GitHub services are prohibited.

### Pull requests

1. `cargo test` is green.
2. If you touched UI, say how you verified it (run the app; there is no browser here).
3. If you added a command, it has an undo.
4. Do not bump the version unless you are cutting a release.

### Every QA pass

Finish each pass with a dated entry in `CHANGELOG.md`: give it a memorable title,
explain what feels different, and name the bugs that went away. Keep it readable
by someone testing the app, with verification and known limits stated honestly.

Push the tested changes to a branch and open or update its GitHub pull request.
Then build and reinstall the same revision for human testing:

```sh
cargo build --release --bin omadesign
./scripts/install.sh
```

The installer also works inside a release tarball. It replaces the binary
atomically, so an open session can finish safely. Relaunch omadesign before
testing the new build; an already running window still uses the previous one.

Use `./scripts/install.sh --launch -- document.oma` to install a missing or older
build and open it. An installation at the same or a newer version opens directly;
missing desktop, MIME, runtime, documentation, skill, or starter-plugin files trigger
a repair first. `--prefix DIRECTORY` works with this mode too. The bundled installer
compares the installed app with the binary beside it and works without a network.

For the latest published release, use:

```sh
curl -fsSL https://omadesign.app/install | sh -s -- --launch -- document.oma
```

Launch mode checks the latest stable release, installs it when needed, and forwards
the app arguments. It keeps a newer installed build. If the release check fails,
an existing complete installation still opens. `OMADESIGN_INSTALL_PREFIX` selects
an isolated destination; a relative path is resolved from the caller's directory.
The normal command without `--launch` still reinstalls the requested release.

OmaStore targets `omadesign-install` in the small installer release archive.
That entry checks the latest version on every launch. It installs a missing app,
opens a current app directly, and offers Update or Later for an older complete
installation. Later opens the installed version; Update runs full native setup
before opening. An explicit `omadesign-install --launch` accepts the update.

The prompt uses terminal input or desktop notification actions, with Zenity and
KDialog fallbacks. If no prompt can be shown, the existing app opens and the
installer prints the update command. OmaStore owns `omadesign-install`; the native
installer owns `omadesign` and its setup files. Removing the store entry removes
its installer shortcut and leaves the native installation intact. Neither
installer overwrites an OmaStore-owned command.

Run `sh scripts/package-installer.sh` to build both installer archives without a
Rust build. Each includes native setup for its matching release, so missing
setup files can be repaired without replacing existing plugin edits. Newer
releases use their own setup script. Normal releases include the archives
through `scripts/release.sh`. Run
`bash scripts/test-installers.sh` for the installer checks; their files stay in
the project's ignored `.artifacts` directory.

### Releasing

On the build machine:

```sh
# bump version in Cargo.toml
./scripts/release.sh
git tag vX.Y.Z
gh release create vX.Y.Z dist/omadesign-X.Y.Z-*.tar.gz* dist/omadesign-installer-X.Y.Z-*.tar.gz*
```

Refuse to ship if `objdump -T` shows GLIBC newer than 2.35.

### License

MIT. Phosphor Light is MIT (see `assets/phosphor/LICENSE-MIT`).

## Contributing Lua plugins

Plugins are versioned Lua bundles in `plugins/`. Start with `plugins/studio-starter`
and follow the [API and contribution guide](plugins.md). Include licensed assets,
a README and reproducible save/reopen/Undo checks with your pull request.
