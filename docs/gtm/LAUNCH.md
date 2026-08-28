# omadesign GTM

Ship date: with v1.0.4. Dark Catppuccin landing, one-line install, no login.

## Positioning

**One line:** A native Linux studio for design, paint, and photograph.

**For:** designers, illustrators, photographers, and artists who left macOS (or never had it) and will not spend a third of their life in a browser.

**Against:** half-ported Affinity, bloated Electron “creative suites”, GIMP-as-the-only-answer.

**Proof:** local aarch64 build that runs on Asahi Omarchy against glibc 2.35. Type you can type into. Pen that adds points. Theme from `~/.config`.

## Channels

| Channel | Asset | First line |
|---|---|---|
| X / Twitter | `posts.md` | Your Mac had Affinity. Your Linux has omadesign. |
| Hacker News | Show HN | Show HN: omadesign – native Linux studio (design / paint / photo), no Electron |
| Reddit r/linux, r/linux_gaming? no, r/opensource, r/rust, r/graphic_design, r/asahi | short + curl | |
| GitHub | README + release notes + topics | |
| Site | omadesign on Vercel | Droppy-shaped, Catppuccin mocha |

## One-liner

```sh
curl -fsSL https://raw.githubusercontent.com/michaelmonetized/omadesign/master/scripts/install-remote.sh | sh
```

## Talking points (stay true)

- Native Rust, eframe + tiny-skia. Not a website.
- Built on this machine. Uploaded. No Actions bill.
- glibc 2.35 so Asahi is a first-class citizen, not an afterthought.
- UI chrome is *your* theme and *your* font.
- Phosphor Light in the well, not letter soup.
- Type is live. Zoom is a box. Pen is a pen.

## Do not say

- “Affinity clone with full Publisher parity.” We are honest about text frames and RAW.
- “AI design assistant.”
- “Works on macOS.” Asahi is Linux.

## Assets in-repo

- `site/public/media/design.jpg` hero UI
- `site/public/media/paint.jpg` pixel persona
- `site/public/media/photo.jpg` develop
- `site/public/media/type.jpg` character studio
- `site/public/media/hero.mp4` six-second reel
- `site/public/media/mark.png` demo export
