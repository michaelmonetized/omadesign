# Moonlit Reveal — original Omadesign score

`reveal-score.m4a` is a 20-second original synthesized instrumental: restrained
string-like harmonic swells, a rising upper phrase, sparse felt-like bell notes,
and a soft resolving chime. It contains no voice, borrowed melody, sampled
recordings, external loops, or third-party soundfonts. It is a procedural score,
not a recording of a live orchestra.

The composition follows the reveal: quiet moonlit glide (0–4 s), harmonic rise
while the O traces (4–8 s), upper voices and sparkle for the wordmark (7–11 s),
D-major resolution for the logo hold (11–14 s), a gentle recession (14–17 s), and
a fading tail (17–20 s). There is no rhythmic beat competing with the animation.

The archived browser controller is [`audio.ts`](audio.ts):
`new CloudScore()`, `play(timeSeconds): Promise<boolean>`, `pause()`,
`seek(timeSeconds)`, `setMuted(boolean)`, and `dispose()`. It is SSR-safe and
starts muted. Silent autoplay does not fetch the audio. Call `setMuted(false)`
and `play(currentVisualTime)` inside the user's Sound on handler. A rejected
play request resolves to `false`; restore the muted UI in that case. The owner
pauses it when the composition is paused, hidden, or unmounted, and supplies the
visual time after scrubbing or restarting. The score does not loop by itself.

Rebuild from the repository root with Python, numpy, and ffmpeg:

```sh
python artifacts/cloud-reveal-exports/original-film-source/generate-cloud-score.py --output /tmp/omadesign-reveal-score.m4a
# Optional: retain a 48 kHz stereo PCM master outside the web payload.
python artifacts/cloud-reveal-exports/original-film-source/generate-cloud-score.py --output /tmp/omadesign-reveal-score.m4a --master /tmp/omadesign-reveal-score.wav
```

The generator uses a fixed seed and validates finite samples. The source master
has a −4.8 dBFS peak; AAC is stereo, 48 kHz, 160 kb/s. The browser plays at 70%
volume to keep the score gentle. The final 2.15 seconds fade to silence.

Validated export: 20.000 seconds, 407,588 bytes, master RMS −19.24 dBFS. A full
FFmpeg decode completed without errors; measured AAC integrated loudness is
−16.8 LUFS and true peak is −4.8 dBFS. The TypeScript controller passed the site
typecheck and lifecycle checks for SSR, silent autoplay, explicit sound opt-in,
bounded seeking, rejected playback, a pending-play/pause race, and disposal.
