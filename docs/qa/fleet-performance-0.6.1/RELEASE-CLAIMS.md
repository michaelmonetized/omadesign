# Release claims: measured brushing improvement

Brush-drag UI processing improved by **2.87–3.03× at the median** and **3.81–4.56× at p95** across these three tested machines, versus the retained issue #158 baseline.

| Host | Median ms, baseline → candidate | Median old/new | P95 ms, baseline → candidate | P95 old/new |
| --- | ---: | ---: | ---: | ---: |
| hpeliteclient | 62.344 → 21.193 | 2.94× | 98.328 → 25.804 | 3.81× |
| intelpro | 45.255 → 15.783 | 2.87× | 66.236 → 16.589 | 3.99× |
| m1pro16 | 27.785 → 9.165 | 3.03× | 43.052 → 9.443 | 4.56× |

Typing, pan and zoom median ratios ranged 0.992–1.027× old/new. This is a brushing gain, not a claim that all authoring is 3× faster.

- intelpro, Pan: median increased 0.312 ms, max increased 13.556 ms.
- intelpro, Zoom: max increased 5.198 ms.

HP native upscaling regressed: baseline runs 69.636/68.197 s, candidate 78.226/82.500 s; the whole-workflow median rose from 68.916 to 80.363 s (16.6% longer; 0.86× old/new). This observed workflow regression is retained without assigning a cause. The same runs reported lower aggregate UI p95 (89.78/78.60 → 5.34/5.81 ms), but sampled more UI calls (2678/2717 → 4350/4553); these differing distributions do not establish an isolated interaction-speed ratio. Core AI inference source is unchanged. The recorded inference phase includes preview/apply readiness, rather than an isolated model kernel. Resource observations do not establish whether the longer workflow reflects external load or self-contention.

Other observed increases include HP pen commit: median 40.658 → 49.377 ms and p95 44.189 → 100.087 ms, from only 6 calls per version; Intel 1024-pixel 2× CLI upscaling: 22.084 → 29.615 s median (34.1% longer, two observations per version). Portable reopen/capture has one observation per version: hpeliteclient 2.612 → 3.117 s; intelpro 1.404 → 2.006 s; m1pro16 1.417 → 1.805 s. These sparse observations are retained without assigning a cause; the full tables include smaller increases and individual runs.

Candidate production source: `8ee01b4ea08e0dac9891694b82dd3f4f377a5cf2`. Baseline: `221834bdd8af5aee8554a0e835629cbe2f467558`. The baseline is **not the shipped v0.6.0 tag**, so these results do not support “0.6.1 is 3× faster than 0.6.0.” The tested candidate still reports package 0.6.0 pending release. Both sides use the original fleet release profile with LTO disabled; these [benchmark builds](build-validation/README.md) are not the final packaged 0.6.1 release binaries.

Two authoring runs per version/host; ratios use measured `Studio::ui` CPU wall duration, not displayed FPS or physical input latency. The outer window is unchanged; the final recorded candidate canvas area is 0.389% smaller, with no timing normalization or equal-zoom claim. Other applications, thermals and pressure remained as used. Core AI inference code is unchanged; no AI-algorithm speedup is claimed.

See [complete measurements and variation](README.md), [raw samples](summary.json) and [output acceptance](OUTPUT-ACCEPTANCE.md). Authoring output has nine explicitly documented changed pixels; do not claim pixel identity.
