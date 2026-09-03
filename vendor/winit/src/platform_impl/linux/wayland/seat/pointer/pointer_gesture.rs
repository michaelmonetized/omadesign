//! Pointer gestures (`zwp_pointer_gestures_v1`).
//!
//! Backport of winit 0.31 Wayland pinch so trackpad pinch becomes
//! `WindowEvent::PinchGesture`. egui-winit maps that to `Event::Zoom`.

use std::ops::Deref;
use std::sync::Mutex;

use sctk::compositor::SurfaceData;
use sctk::globals::GlobalData;
use sctk::reexports::client::globals::{BindError, GlobalList};
use sctk::reexports::client::{delegate_dispatch, Connection, Dispatch, Proxy, QueueHandle};
use sctk::reexports::protocols::wp::pointer_gestures::zv1::client::zwp_pointer_gesture_pinch_v1::{
    Event, ZwpPointerGesturePinchV1,
};
use sctk::reexports::protocols::wp::pointer_gestures::zv1::client::zwp_pointer_gestures_v1::ZwpPointerGesturesV1;

use crate::event::{TouchPhase, WindowEvent};
use crate::platform_impl::wayland::state::WinitState;
use crate::platform_impl::wayland::{self, DeviceId};

/// Wrapper around the pointer gesture global.
#[derive(Debug)]
pub struct PointerGesturesState {
    pointer_gestures: ZwpPointerGesturesV1,
}

impl PointerGesturesState {
    pub fn new(
        globals: &GlobalList,
        queue_handle: &QueueHandle<WinitState>,
    ) -> Result<Self, BindError> {
        let pointer_gestures = globals.bind(queue_handle, 1..=3, GlobalData)?;
        Ok(Self { pointer_gestures })
    }
}

#[derive(Debug, Default)]
pub struct PointerGestureData {
    inner: Mutex<PointerGestureDataInner>,
}

#[derive(Debug)]
struct PointerGestureDataInner {
    window_id: Option<wayland::WindowId>,
    previous_pinch: f64,
}

impl Default for PointerGestureDataInner {
    fn default() -> Self {
        Self { window_id: None, previous_pinch: 1.0 }
    }
}

impl Deref for PointerGesturesState {
    type Target = ZwpPointerGesturesV1;

    fn deref(&self) -> &Self::Target {
        &self.pointer_gestures
    }
}

impl Dispatch<ZwpPointerGesturesV1, GlobalData, WinitState> for PointerGesturesState {
    fn event(
        _state: &mut WinitState,
        _proxy: &ZwpPointerGesturesV1,
        _event: <ZwpPointerGesturesV1 as Proxy>::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qhandle: &QueueHandle<WinitState>,
    ) {
    }
}

impl Dispatch<ZwpPointerGesturePinchV1, PointerGestureData, WinitState> for PointerGesturesState {
    fn event(
        state: &mut WinitState,
        _proxy: &ZwpPointerGesturePinchV1,
        event: <ZwpPointerGesturePinchV1 as Proxy>::Event,
        data: &PointerGestureData,
        _conn: &Connection,
        _qhandle: &QueueHandle<WinitState>,
    ) {
        let mut inner = data.inner.lock().unwrap();
        let device_id =
            crate::event::DeviceId(crate::platform_impl::DeviceId::Wayland(DeviceId));

        let (window_id, phase, delta) = match event {
            Event::Begin { surface, fingers, .. } => {
                if fingers != 2 {
                    return;
                }
                // Decorations are subsurfaces; pinch there is not a canvas zoom.
                if let Some(data) = surface.data::<SurfaceData>() {
                    if data.parent_surface().is_some() {
                        return;
                    }
                }
                let window_id = wayland::make_wid(&surface);
                inner.window_id = Some(window_id);
                inner.previous_pinch = 1.0;
                (window_id, TouchPhase::Started, 0.0)
            },
            Event::Update { scale: pinch, .. } => {
                let window_id = match inner.window_id {
                    Some(window_id) => window_id,
                    None => return,
                };
                // Wayland scale is absolute from begin. egui-winit does exp(delta),
                // so send ln(new/old) and the canvas gets the true ratio.
                let delta = if inner.previous_pinch > 1e-8 && pinch > 1e-8 {
                    (pinch / inner.previous_pinch).ln()
                } else {
                    0.0
                };
                inner.previous_pinch = pinch;
                (window_id, TouchPhase::Moved, delta)
            },
            Event::End { cancelled, .. } => {
                let window_id = match inner.window_id {
                    Some(window_id) => window_id,
                    None => return,
                };
                *inner = Default::default();
                let phase = if cancelled == 0 { TouchPhase::Ended } else { TouchPhase::Cancelled };
                (window_id, phase, 0.0)
            },
            _ => return,
        };

        state.events_sink.push_window_event(
            WindowEvent::PinchGesture { device_id, delta, phase },
            window_id,
        );
    }
}

delegate_dispatch!(WinitState: [ZwpPointerGesturesV1: GlobalData] => PointerGesturesState);
delegate_dispatch!(WinitState: [ZwpPointerGesturePinchV1: PointerGestureData] => PointerGesturesState);
