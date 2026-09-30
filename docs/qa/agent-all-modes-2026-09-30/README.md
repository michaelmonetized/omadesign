# Agent disk assets and all workspaces — 2026-09-30

The previous live harness instructed agents not to use filesystem/shell tools for design changes and only exposed vector editing. The installed creation skill described a separate offline CLI workflow. The user-visible result was confirmed in saved thread `18d97e1b3f1ed74f0000000000007a7c`: “The available native tools can draw an editable golfer silhouette, but can’t import or mask a photographic golfer cutout.”

The 35-tool contract permits requested disk assets, exposes native placement and editing across Design, Pixel, Photo, Layout and Motion, and tells agents to discover actual capabilities before claiming a restriction. MCP file reads/imports use worker decoding, then native placement validates revision/session/live-edit permission on the UI thread. ACP advertises and implements text-file reads/writes, with matching active session and write permission checks. Learn remains read-only. Terminal tools remain provider-owned.

The tool set includes file discovery/previews/imports; vector/raster/group layers; embedded image fills; raster transforms; masks, selections, painting and retouching; full Photo adjustments; layout properties/components; advanced native object geometry/style; canvas settings; native commands; animation keys/presets; native snapshots and file exports. Existing codec restrictions remain explicit. This does not claim that every UI command or external-provider feature has been separately exercised.

The creation skill was updated in the repository, the installed Omadesign skill directory, and the active Codex skill directory. Previous installed copies are backed up under `/home/michael/.local/state/omadesign/qa/agent-all-modes-2026-09-30/skill-backups`. Website documentation is generated locally from the same source; no public deployment was requested.

## Native ACP acceptance

A real Codex ACP agent was given a coastal photograph in a directory outside its project, including spaces in the filename. It browsed/previewed that file, developed the original in Photo, exported the developed copy, used the actual photograph as both an embedded Layout image fill and a masked raster, added editable typography, and applied Fade in plus horizontal keys. It inspected time 0 and time 1 and saved OMA and animated SVG. The final release-build native screenshot is `release-native-card.png`. The original disk photo remained unchanged.

The initial debug run completed 30 live operations with no session error (`native-run.json`). The final optimized release run completed 26 live operations in 163 seconds with no session error (`release-native-run.json`). It recovered from three rejected calls after guessing two revisions and an object ID; the native guards rejected the stale/missing targets and the agent re-read current state. The final artifact contains a masked raster, editable photo frame and typography, one developed Photo original, and two animation tracks. Large editable originals are kept in the linked local QA directory rather than duplicated in git.

The installed `~/.local/bin/omadesign-agent-all-modes` build is running on workspace 7 with `/home/michael/Documents/Omadesign Agent Tools Demo.oma`, in an isolated profile. Its live child process executable hash matches the built and installed release binary. `launch.json`, `manifest.json`, and `launched.png` record that check. The original application on workspace 6 was preserved.

## Automated validation

The final focused agent suite passes 42 tests, including disk placement in all five personas, source preservation, native undo/redo, layered/image-fill persistence, masks, selection-constrained painting, raster motion, Photo revision checks, advanced object/canvas/group editing, and actual ACP JSON-RPC text-file delegation. The all-target Cargo check and targeted rustfmt/diff checks pass.

The full library suite retains the two pre-existing text overflow/ellipsis failures already reproduced on the untouched 0.6.1 baseline in the raster-motion QA evidence. The final run is 928 passed, 2 known baseline failures and 7 ignored. See `full-library-tests.log`, `source.json` and the launch manifest.

This branch includes the raster-motion fix that Michael accepted with “tested, feels good” before requesting this extension. The accepted 0.5.0 human QA evidence and unrelated working-tree changes remain untouched. This is a local development build, not a public release.
