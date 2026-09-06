# Website artwork sources

These examples supply the four studio artwork cards on the website. They contain
no stock photography, client work, third-party advertising or embedded interface
mockups.

| Example | Source and native processing                                                                                                                                                                     | Website artwork            |
| ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------- |
| Design  | The studio's original `block-party` vector template, with US spelling. Editable shapes and text in `design.oma`.                                                                                 | `design.webp`, 1600 × 1000 |
| Motion  | The original `after-hours` vector template, with seven editable Draw stroke animation channels. Editable project in `motion.oma`; native animated SVG exported alongside the still.              | `motion.webp`, 1600 × 1000 |
| Pixel   | An original iris painting generated for this project with the built-in OpenAI image generation tool on September 5, 2026. Imported as a raster layer and exported through the native compositor. | `pixel.webp`, 1586 × 992   |
| Photo   | An original fictional coastal landscape generated with the same tool and date. Developed with the native Photo engine, then imported and exported through the native compositor.                 | `photo.webp`, 1586 × 992   |

The generated PNG originals retain their original metadata. The complete prompts
and generation mode are in [prompts.json](prompts.json). The Pixel example is an
imported generated painting; it is not a claim that someone painted every mark in
the application. The Photo image is a generated landscape, not a photograph of a
documented location.

The Photo treatment uses exposure `0.06`, highlights `-0.12`, shadows `0.08` and
vibrance `1.035`; all remaining controls use the native defaults. The original is
kept intact. The replay helper contains these settings explicitly. The resulting
raster project contains the developed pixels; the PNG original and helper preserve
the reproducible starting point and treatment.

## Regenerate

From the repository root, after building the native release library:

```sh
cargo build --release --lib
rustc --edition 2024 -C lto=thin -C embed-bitcode=yes examples/site-showcase/render.rs \
  --extern omadesign=target/release/libomadesign.rlib -L dependency=target/release/deps \
  -o /tmp/omadesign-showcase
/tmp/omadesign-showcase
python examples/site-showcase/encode-webp.py
```

The last step requires Pillow and changes encoding only. The vector artwork uses
lossless WebP; the two detailed raster images use quality 90. No cropping,
compositing, painting or color changes happen in that encoding step.

Native raster `.oma` and PNG intermediates are written into
`omadesign-site-showcase` in the operating system's temporary directory. They are
reproducible and deliberately excluded from the source tree because the two pixel
buffers alone add about 28 MB. Original generated PNGs, the small editable vector
projects and the regeneration source are kept here. Native vector SVG/PNG exports,
including the animated SVG, are in `examples/site-showcase/exports`. Only the four
website WebPs and the three logo SVGs are in `site/public/media/showcase`.

## Approved wordmark

The website's `wordmark-on-dark.svg` and `wordmark-on-light.svg` preserve the full
composition from `media/logo-4-refined.svg`: the blurred green OMA maze, the exact
six letter paths and dot rectangle, the approved open S, and the original shadows
and view box. Only the solid background is removed; the light variant uses an ink
foreground so the wordmark remains readable. `favicon.svg` uses the same approved
d path, centered in a square view box; its foreground responds to the color scheme.
The original user logo files are unchanged.

## Template library capture

The landing page also uses an unedited native library capture at 1600 × 880,
encoded to lossless WebP. This viewport shows the first six template cards.
Recreate it with `omadesign --shot templates --size 1600x880 --out /tmp/templates.png`,
then encode that PNG to `site/public/media/studio/templates.webp`.

## Native panel recordings

The five continuous studio clips use real native UI input and viewport capture.
See [recordings.md](recordings.md) for provenance, chapter timings, dependencies
and the isolated recording and publishing commands.
