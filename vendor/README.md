# vendor

`winit` is 0.30.13 plus:

- Wayland trackpad pinch (`zwp_pointer_gestures_v1`)
- Wayland file drop (`wl_data_device` / `text/uri-list`)

eframe 0.36 still depends on winit 0.30. Upstream pinch-on-Wayland landed in 0.31.
The pinch delta lives in `winit/src/platform_impl/linux/wayland/seat/pointer/pointer_gesture.rs`.
File drop lives in `winit/src/platform_impl/linux/wayland/seat/dnd.rs`.
