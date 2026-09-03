//! Wayland file drag-and-drop via `wl_data_device`.
//!
//! winit 0.30 never bound the data device, so Hyprland (and every other
//! compositor) refused drops onto the window. File managers advertise
//! `text/uri-list`; we accept that and emit the same `HoveredFile` /
//! `DroppedFile` events X11 already does.

use std::io::Read;
use std::path::PathBuf;

use sctk::compositor::SurfaceData;
use sctk::data_device_manager::data_device::{DataDevice, DataDeviceHandler};
use sctk::data_device_manager::data_offer::{DataOfferHandler, DragOffer};
use sctk::data_device_manager::data_source::DataSourceHandler;
use sctk::reexports::client::protocol::wl_data_device::WlDataDevice;
use sctk::reexports::client::protocol::wl_data_device_manager::DndAction;
use sctk::reexports::client::protocol::wl_data_source::WlDataSource;
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{Connection, Proxy, QueueHandle};

use crate::event::WindowEvent;
use crate::platform_impl::wayland::state::WinitState;
use crate::platform_impl::wayland::{self, WindowId};

const URI_LIST: &str = "text/uri-list";

pub struct DndSession {
    pub window_id: WindowId,
    pub paths: Vec<PathBuf>,
}

impl WinitState {
    fn device_for(&self, wl: &WlDataDevice) -> Option<&DataDevice> {
        self.seats
            .values()
            .filter_map(|s| s.data_device.as_ref())
            .find(|d| d.inner() == wl)
    }

    fn accept_file_offer(offer: &DragOffer) -> bool {
        let mime = offer.with_mime_types(|mimes| {
            mimes.iter().any(|m| m.as_str() == URI_LIST || m.contains("uri-list"))
        });
        if !mime {
            offer.accept_mime_type(offer.serial, None);
            return false;
        }
        offer.accept_mime_type(offer.serial, Some(URI_LIST.into()));
        offer.set_actions(DndAction::Copy, DndAction::Copy);
        true
    }

    fn read_uri_list(offer: &DragOffer) -> Vec<PathBuf> {
        let Ok(mut pipe) = offer.receive(URI_LIST.to_string()) else {
            return Vec::new();
        };
        let mut buf = String::new();
        if pipe.read_to_string(&mut buf).is_err() {
            return Vec::new();
        }
        parse_uri_list(&buf)
    }
}

impl DataDeviceHandler for WinitState {
    fn enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        data_device: &WlDataDevice,
        _x: f64,
        _y: f64,
        surface: &WlSurface,
    ) {
        let parent = surface
            .data::<SurfaceData>()
            .and_then(|d| d.parent_surface().cloned())
            .unwrap_or_else(|| surface.clone());
        let window_id = wayland::make_wid(&parent);
        if !self.windows.get_mut().contains_key(&window_id) {
            return;
        }

        let Some(offer) = self.device_for(data_device).and_then(|d| d.data().drag_offer()) else {
            return;
        };
        if !Self::accept_file_offer(&offer) {
            self.dnd = None;
            return;
        }
        let paths = Self::read_uri_list(&offer);
        for path in &paths {
            self.events_sink.push_window_event(WindowEvent::HoveredFile(path.clone()), window_id);
        }
        if paths.is_empty() {
            // Still a valid target (copy cursor) even if we could not peek the list yet.
            self.events_sink.push_window_event(
                WindowEvent::HoveredFile(PathBuf::from("file")),
                window_id,
            );
        }
        self.dnd = Some(DndSession { window_id, paths });
        self.dispatched_events = true;
    }

    fn leave(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _data_device: &WlDataDevice) {
        if let Some(session) = self.dnd.take() {
            self.events_sink
                .push_window_event(WindowEvent::HoveredFileCancelled, session.window_id);
            self.dispatched_events = true;
        }
    }

    fn motion(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _data_device: &WlDataDevice,
        _x: f64,
        _y: f64,
    ) {
    }

    fn selection(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _data_device: &WlDataDevice,
    ) {
    }

    fn drop_performed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        data_device: &WlDataDevice,
    ) {
        let Some(offer) = self.device_for(data_device).and_then(|d| d.data().drag_offer()) else {
            return;
        };
        let mut paths = Self::read_uri_list(&offer);
        if paths.is_empty() {
            if let Some(session) = self.dnd.as_ref() {
                paths = session.paths.clone();
            }
        }
        let window_id = self.dnd.as_ref().map(|s| s.window_id).unwrap_or_else(|| {
            wayland::make_wid(&offer.surface)
        });
        for path in paths {
            self.events_sink.push_window_event(WindowEvent::DroppedFile(path), window_id);
        }
        offer.finish();
        offer.destroy();
        self.dnd = None;
        self.dispatched_events = true;
    }
}

impl DataOfferHandler for WinitState {
    fn source_actions(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        offer: &mut DragOffer,
        _actions: DndAction,
    ) {
        offer.set_actions(DndAction::Copy, DndAction::Copy);
    }

    fn selected_action(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _offer: &mut DragOffer,
        _actions: DndAction,
    ) {
    }
}

impl DataSourceHandler for WinitState {
    fn accept_mime(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &WlDataSource,
        _mime: Option<String>,
    ) {
    }

    fn send_request(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &WlDataSource,
        _mime: String,
        _fd: sctk::data_device_manager::WritePipe,
    ) {
    }

    fn cancelled(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &WlDataSource,
    ) {
    }

    fn dnd_dropped(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &WlDataSource,
    ) {
    }

    fn dnd_finished(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &WlDataSource,
    ) {
    }

    fn action(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _source: &WlDataSource,
        _action: DndAction,
    ) {
    }
}

fn parse_uri_list(data: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for raw in data.split(|c| c == '\n' || c == '\r') {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let path = if let Some(rest) = line.strip_prefix("file://") {
            file_uri_to_path(rest)
        } else if line.starts_with('/') {
            Some(PathBuf::from(percent_decode(line)))
        } else {
            None
        };
        if let Some(p) = path {
            out.push(p);
        }
    }
    out
}

fn file_uri_to_path(rest: &str) -> Option<PathBuf> {
    let decoded = percent_decode(rest);
    let path = if decoded.starts_with('/') {
        decoded
    } else {
        // file://hostname/path
        let slash = decoded.find('/')?;
        decoded[slash..].to_string()
    };
    Some(PathBuf::from(path))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(b) = u8::from_str_radix(hex, 16) {
                    out.push(b);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_file_uri_list() {
        let list = "file:///home/me/mark.svg\r\nfile:///tmp/foo%20bar.png\r\n";
        let paths = parse_uri_list(list);
        assert_eq!(paths[0], PathBuf::from("/home/me/mark.svg"));
        assert_eq!(paths[1], PathBuf::from("/tmp/foo bar.png"));
    }
}
