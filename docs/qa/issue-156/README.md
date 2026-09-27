# Issue 156 native acceptance

[Complete native E2E recording (42 seconds)](issue-156.mp4) · [assertion results](result.json)

Recorded on 2026-09-27 using actual native WGPU viewport frames and real egui pointer/keyboard events. All 19 checks passed: Fit is absent from the title bar and grouped before 100% in the status corner in Design, Layout, Pixel, Motion and Photo; actual-size and Fit behavior work; Agent opens/closes; View → Fit artboard and Ctrl+0 work; and the welcome screen has Learn with AI and no Fit control.

The recording contains 420 frames at 1600×1000, encoded at 10 fps. Capture took 208.84 seconds; playback is deterministic input replay, not a realtime performance measurement. The fixture seeds two shapes and loads the repository photo; edits use native input. Isolated XDG profiles preserve the installed application and accepted 0.5.0 human QA evidence.

Both gradient tests pass for rendering, alpha, distinct colors in dark/light themes and texture caching. All four existing key-HUD tests pass, including passive keyboard ownership and bounded hints across personas/window sizes. Logs are included.

The existing `capture_studios` tool produced refreshed [Design](design.png), [Photo](photo.png) and [welcome](welcome.png) screenshots. Its target lookup now recognizes the Agent icon's stable accessible widget response.

Reproduce locally in a graphical session:

```sh
cargo build --locked --offline --bin inspector_qa --bin capture_studios
DISPLAY=:0 target/debug/inspector_qa 156 /tmp/issue-156-qa
DISPLAY=:0 target/debug/capture_studios design /tmp/issue-156-design --probe
DISPLAY=:0 target/debug/capture_studios photo /tmp/issue-156-photo --probe
DISPLAY=:0 target/debug/capture_studios welcome-agents /tmp/issue-156-welcome --probe
cargo test --lib --locked --offline ui::icons::gradient
cargo test --lib --locked --offline ui::key_hud_tests
```
