# Homepage film: your work in motion

The product film near the end of the homepage is a new 32-second silent cut.
It leads with the welcome browser and creation workflows, then moves through
editing, project brand assets, Layout and Motion. The existing opening Cloud
announcement and studio carousel remain in place.

## Direction and timing

- 120 BPM in 4/4: one beat every 0.5 seconds, one bar every two seconds.
- Cuts on beats 1 and 3: 32 one-second shots, 16 complete bars.
- 1920×1080 at 60 fps: 1,920 frames, cuts at frames 0, 60, 120 … 1,860.
- Hard cuts, native pointer/brush/animation movement, accelerated source windows,
  and eased editorial pans/pushes between cuts. No backing track or narration.
- Editorial copy stays brief. Optional captions describe groups of actions over
  two to four seconds instead of trying to narrate every one-second cut.
- The new official logo resolves the final two seconds; it is not an opening hold.

## Actual app footage

`src/bin/capture_studios.rs` records the native WGPU viewport at 1600×900 and
60 fps. Real egui pointer and keyboard events drive the actual `Studio` app loop,
including its asynchronous file work. Capture uses temporary configuration/data
directories and a context-local filesystem catalog root; HOME and the user's
running sessions are unchanged. Only the starting documents are seeded.

The five new welcome takes cover browsing/filtering/selection/nested projects,
Vector templates, Layout templates, Raster sizes, and the Learn/Create prompt
boxes. All five completed with zero input-target or expected-state failures.
Both prompt boxes are cancelled after typing: no external agent is launched,
and no generated result is simulated.

The eight editing takes are fresh recordings of Graphics, Design, Pixel, Chroma,
Photo, Brand kit, Layout and Motion. Selected windows were checked visually
before/after the recorded action. Longer Design/Pixel takes contain unused
actions whose targets were not visible; their exact reports remain in the
edit manifest. The renderer rejects a target failure inside an included window.
The film does not claim to demonstrate those unused stroke/effects/trace/color
actions. A weak Motion Slide-up window was excluded; the ending uses the visibly
colored Draw-stroke/Pop-in result. Layout shows presentation and frame dragging,
not a responsive breakpoint change or a prototype link click.

The capture driver allows frames while a brush is held or animation is playing.
Waiting for thumbnail readiness during those continuous operations would prevent
the subsequent release input from arriving. Normal readiness checks still apply
when no continuous interaction is active. This changes the capture utility only.

## Official mark

`media/logo.svg` is the user-designated source of truth. Its colors and paths are
unchanged. `scripts/prepare-brand-assets.py` copies the square export to the app
icon and derives `assets/omadesign-wordmark.svg` by changing only the root viewport
to trim empty padding. Native Welcome/About preserve its aspect ratio. The site
uses an exact copy for the header, footer and favicon, with a matching social card.
The film ending is derived from the same SVG, with no replacement/generated mark.

## Reproduce and verify

Capture each required scene in a graphical session:

```sh
DISPLAY=:0 WINIT_UNIX_BACKEND=x11 WINIT_X11_SCALE_FACTOR=1 \
  target/aarch64-unknown-linux-gnu/release/capture_studios \
  welcome-browse /path/to/sources --fps 60
```

The other new scene names are `welcome-vector`, `welcome-layout`, `welcome-raster`
and `welcome-agents`; existing scene names are listed above. Each take produces
a video, native inspection frames, a capture metadata file and an action report.
The two-second `brand-ending.mp4` is the official SVG rendered onto its matching
dark background. It is explicitly identified separately from app footage.

The published JSON is also the edit decision list:

```sh
python scripts/cut-homepage-film.py \
  site/public/media/studio/film-0.5.7.json \
  --sources /path/to/sources --work /path/to/edit \
  --output site/public/media/studio/film-0.5.7.mp4
```

The renderer validates source ranges and action reports, writes exactly 60 frames
per shot, checks the final frame count/duration, fully decodes the MP4, and creates
WebM, WebP, WebVTT and a manifest with source/output hashes. Production uses
versioned filenames, WebM with MP4 fallback, native controls, no autoplay and
`preload="none"`; leaving the viewport or hiding the page pauses playback.

Raw takes, interrupted-take evidence, selected-window visual checks and editorial
review frames are retained locally under
`/home/michael/Videos/omadesign/0.5.6-homepage-film/` (the work folder was named
before the official-logo update became the 0.5.7 patch).
Release and browser receipts are under
`/home/michael/Documents/Omadesign-QA-0.5.7-20260921/`.
