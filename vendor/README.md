# vendor

`winit` is 0.30.13 plus:

- Wayland trackpad pinch (`zwp_pointer_gestures_v1`)
- Wayland file drop (`wl_data_device` / `text/uri-list`)

eframe 0.36 still depends on winit 0.30. Upstream pinch-on-Wayland landed in 0.31.
The pinch delta lives in `winit/src/platform_impl/linux/wayland/seat/pointer/pointer_gesture.rs`.
File drop lives in `winit/src/platform_impl/linux/wayland/seat/dnd.rs`.

`egui-winit` is 0.36.1 (upstream commit
`4c1f2fae95475a40e524884ebb298bcb1714b08e`) with one input fix:

- Clipboard commands retain their originating key press before the existing
  Copy/Cut/Paste event, even when the clipboard contains no text. This preserves
  the exact press-time modifiers for application chords such as Ctrl+Shift+V.
- The application dispatches each key/clipboard pair once. Text fields continue
  to receive the original clipboard event and payload.

Sources and published manifests come from the crates.io 0.36.1 package. Its
MIT and Apache-2.0 licenses come from the matching upstream tag. The focused
bridge regression is `cargo test --offline -p egui-winit --lib clipboard_chord_tests`.
