# Contributing to omadesign

This is a small native Linux studio. Keep it that way.

## How to work on it

```sh
git clone https://github.com/michaelmonetized/omadesign.git
cd omadesign
cargo test
cargo run --release
```

Rust 2024. `cargo` is the toolchain. No GTK app, no Electron, no GitHub Actions.

### Layout

```
src/
  geom.rs         points, bounds, Bézier, hit testing     (no UI)
  document.rs     layers, shapes, command history
  compositor.rs   tiny-skia renderer + PNG/JPEG export
  paint.rs        brush, erase, smudge, clone, fill, wand
  photo.rs        develop pipeline + histograms
  boolean.rs      union / subtract / intersect / xor
  text.rs         rustybuzz OpenType + glyph outlines
  tools.rs        personas, tools, shortcut table
  app.rs          studio state, commands, keys
  ui/             chrome, canvas, studios, photo, welcome
assets/phosphor/  Phosphor Light (MIT)
docs/             manual, roadmap, GTM
site/             landing page (TanStack Start)
scripts/          local release + curl installer
```

Mutations go through `Cmd` + `History`. Tests cover geometry, boolean, paint, develop, project round-trip, SVG, export, type, and zoom.

### Rules of the house

- **No stubs.** A tool that drops a baked path called “Type” is not type.
- **No hardcoded UI colours.** Chrome reads the Omarchy / `~/.config` theme. Fallback is Catppuccin Mocha, used only when no theme is on disk.
- **Icons are Phosphor Light.** Add a glyph in `src/ui/icons.rs`, do not invent a stick figure.
- **UI font is the desktop font.** Do not bundle Inter “because marketing.”
- **Deep modules.** `geom` and `text` have no egui types. Tests share the same seams.
- **Local builds.** `./scripts/release.sh` zig-links glibc 2.35 for aarch64 and x86_64. Never add a GitHub Actions workflow that bills Microsoft for runners.

### Pull requests

1. `cargo test` is green.
2. If you touched UI, say how you verified it (run the app; there is no browser here).
3. If you added a command, it has an undo.
4. Do not bump the version unless you are cutting a release.

### Releasing

On the build machine:

```sh
# bump version in Cargo.toml
./scripts/release.sh
git tag vX.Y.Z
gh release create vX.Y.Z dist/omadesign-X.Y.Z-*.tar.gz*
```

Refuse to ship if `objdump -T` shows GLIBC newer than 2.35.

### Licence

MIT. Phosphor Light is MIT (see `assets/phosphor/LICENSE-MIT`).
