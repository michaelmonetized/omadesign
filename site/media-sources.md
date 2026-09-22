# Website media sources

## RAW development preview

`public/media/studio/raw-preview.webp` is a native Omadesign Photo screenshot,
captured from revision `5bee4aae7432dc87ceaa905762b1b81037740ed9` on September 6,
2026. The 1600 × 1000 lossless WebP preserves the captured pixels.

The displayed Fujifilm X-T30 II photograph is a **CC0-1.0** sample from
[raw.pixls.us](https://raw.pixls.us/getfile.php/5064/nice/Fujifilm%20-%20X-T30%20II%20-%2014bit%2014bit%20compressed%20%283%3A2%29.RAF).
Its source SHA-256 is
`5e2678038377cc7f146ceb7693c08535fa1af162cec705d06b0446c3d3017d47`.
The sample's license was verified against its catalog metadata before publication.
The screenshot contains no private user photograph. Omadesign's UI is part of
the MIT-licensed project.

## Cloud collaboration reveal

`public/media/cloud/reveal.mp4` is Michael’s supplied Grok Imagine video,
already edited in a separate session: `grok-video-dc5e39f6-2774-49c0-ab72-f67dc95afb83-trimmed.mp4`.
The 1280 × 720 H.264/AAC clip is 24.542 seconds. It is remuxed for fast-start
streaming without re-encoding or further trimming. `reveal.webp` is its
12-second frame, used as the poster and reduced-motion fallback.

## Official 0.5.7 mark

`public/media/branding/logo-0.5.7.svg` is a byte-identical copy of Michael's
official artwork at `media/logo.svg`. It replaces the website header/footer
mark and favicon. The original square has substantial empty space above and
below the green circular emblem and lavender wordmark; the header's 16:9
viewport removes only that padding. The artwork's colors and geometry are
unchanged in both site themes.

`logo-0.5.7-social.png` rasterizes the original at 1200 × 1200, viewed through
a 1200 × 630 crop offset 285 pixels from the top. The full emblem and lettering
remain visible. Source/output hashes and crop metadata are in
`public/media/branding/manifest.json`. The existing Cloud announcement is
preserved as a separate, previously supplied film.

## 0.5.7 product film

`public/media/studio/film-0.5.7.{mp4,webm,webp,vtt,json}` replaces the legacy
97-second product film in the homepage's ending film section. The separate
Cloud opening announcement and existing carousel remain unchanged.

The new film is 32 seconds at 1920 × 1080 / 60 fps (1920 frames), with no audio
stream. Its edit follows a 120 BPM grid with cuts on beats one and three, one
second apart. MP4 uses H.264 and WebM uses VP9; the open-codec version is offered
first. The poster is the new native welcome screen at 0.35 seconds. WebVTT cues
describe the edited sequence.

Fresh native WGPU recordings were driven through Omadesign's real UI. The edit
uses selected source ranges, speed ramps, camera crops and captions; the
two-second ending uses the supplied official SVG. The JSON receipt records each
shot, source hashes, raw take action reports, cut frames and output hashes. The
clip is an edited product film, rather than a continuous workflow replay.

The player loads on demand, includes native play/pause/seek controls, keeps the
full 16:9 frame on mobile and pauses when scrolled offscreen. It never autoplays,
including for users who request reduced motion.
