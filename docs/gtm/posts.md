# Launch posts

## X / Twitter

Your Mac had Affinity. Your Linux has omadesign.

Native studio: design, paint, photograph. No Electron. Type you can type into. Theme from ~/.config. Runs on Asahi.

```
curl -fsSL https://raw.githubusercontent.com/michaelmonetized/omadesign/master/scripts/install-remote.sh | sh
```

https://github.com/michaelmonetized/omadesign

---

I got tired of “Linux creative suites” that are a browser with extra steps.

omadesign is Rust, local aarch64 builds, glibc 2.35. Drag a box to zoom. Click T and actually type. Phosphor Light in the well. Catppuccin if that’s what your desktop is.

---

Asahi Omarchy on an M1 Pro should not be a second-class citizen for design tools.

omadesign ships an aarch64 tarball linked against glibc 2.35. Same binary family as x86_64. No GitHub Actions tax.

## Hacker News

**Title:** Show HN: omadesign – native Linux studio (design / paint / photo), no Electron

**Text:**

I needed somewhere to draw logos, paint, and grade photos after leaving macOS. Existing options were either vintage GTK, a web app, or “we’ll port it later.”

omadesign is a Rust 2024 / eframe / tiny-skia desktop. Three personas (Design, Pixel, Photo) share a layer stack. Type is live (caret, Character studio, OpenType). The zoom tool marquee-zooms. UI colours and the UI font come from the Omarchy/desktop config, not a baked orange theme.

Binaries are built on the machine and uploaded to GitHub Releases, zig-linked to glibc 2.35 so they run on Asahi as well as Ubuntu 22.04+.

```
curl -fsSL https://raw.githubusercontent.com/michaelmonetized/omadesign/master/scripts/install-remote.sh | sh
```

MIT. https://github.com/michaelmonetized/omadesign

## Reddit (r/rust, r/linux, r/graphic_design, r/AsahiLinux)

**Title:** omadesign – native Linux design / paint / photo studio (Rust, no Electron)

Short version of the HN text. Include the curl line and the hero still.

## GitHub about

**Description:** Native Linux studio for design, paint, and photograph.

**Topics:** rust linux design illustration photography asahi omarchy vector raster

**Website:** the Vercel URL once live.
