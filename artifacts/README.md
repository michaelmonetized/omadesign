# Cloud design archive

Historical Omadesign cloud artwork, exports, and validation evidence from
September 2026. These files are an asset archive; they do not replace the live
website or participate in the desktop/site build.

## Contents

- `cloud-reveal-exports/omadesign-cloud-1080p.mp4`: the original 20-second film,
  1920 × 1080, 24 fps, H.264 with stereo AAC. Its checksum and historical checks
  are in [the film QA record](../docs/qa/cloud-reveal-2026-09-23.md).
- `cloud-reveal-exports/omadesign-cloud-player.tar.gz`: the original film's
  standalone player, with its own JavaScript, CSS, and synthesized score.
- `cloud-reveal/`: a later built dreamscape player. Both HTML entry points
  currently load that dreamscape bundle; this differs from the original film
  in the tar archive and MP4.
- `cloud-reveal-exports/dreamscape-v2-source/` and `original-film-source/`:
  archived source snapshots. Historical scripts and reports can refer to paths,
  temporary fixtures, ports, and deployment URLs from their original environment;
  they are not current deployment instructions.
- The remaining screenshots and JSON reports document design iterations and
  browser checks performed at the time. They are not fresh production checks.
- [Editable artwork study](../examples/omadesign-cloud.oma),
  [0.6.0 logo](../media/logo-0.6.0.oma), [logo study 3](../media/logo-3.oma),
  and [logo study 4](../media/logo-4.oma) are native `.oma` sources. Raster logo
  exports are under `media/`. The historical `omadesign-cloud.oma` filename
  contains a mixed artwork/layout study, not the source of the cloud film.

## Viewing

Serve `artifacts/cloud-reveal/` over HTTP to view the later dreamscape player:

```sh
python3 -m http.server 5176 --directory artifacts/cloud-reveal
# Open http://localhost:5176/cloud-dreamscape.html
```

For the original film, extract `omadesign-cloud-player.tar.gz` into a separate
folder and serve that folder instead. The movie plays directly in a video player.
The HTML player requests Google Fonts, so typography can fall back when offline.
The later dreamscape texture provenance is recorded in
[the site texture sources](../site/src/components/cloud-reveal/textures/SOURCES.md).

## Import validation — 2026-09-27

All four new `.oma` documents loaded and exported to PNG using the current 0.6.0
native renderer. The 47 PNG/JPEG assets decoded successfully; all new JSON files
parsed. The MP4 decoded fully without errors, has the expected 20-second duration,
and matches its recorded SHA-256. Both HTML entry points resolve their local
script/style files, and the portable archive has safe paths and complete local
HTML dependencies. The browser interaction reports remain historical evidence.

The imported files total approximately 101 MiB, including a 47.2 MiB editable
cloud document. Vite caches, worktree gitlinks, and the byte-identical root
`Untitled.oma` draft are excluded. The original import commit remains preserved
on `feat/live-design-agent-selection`.
