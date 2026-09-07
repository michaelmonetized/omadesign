//! Full-resolution RAW inspection without sending whole camera images to the GPU.
use crate::app::Studio;
use crate::photo::{DevelopParams, RgbaImage};
use eframe::egui::{self, Color32, ColorImage, Rect, TextureHandle, TextureOptions, Ui, Vec2};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

const MAX_TILE_EDGE: u32 = 2048;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Key {
    selected: usize,
    version: u64,
    original: bool,
    source: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Region {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

impl Region {
    fn contains(self, other: Self) -> bool {
        self.x <= other.x
            && self.y <= other.y
            && self.x + self.w >= other.x + other.w
            && self.y + self.h >= other.y + other.h
    }
}

struct Tile {
    region: Region,
    texture: TextureHandle,
}

#[derive(Debug)]
struct Rendered {
    full: Arc<RgbaImage>,
    region: Region,
    pixels: RgbaImage,
}

struct Job {
    key: Key,
    receiver: mpsc::Receiver<Option<Rendered>>,
}

#[derive(Default)]
struct State {
    desired: Option<Key>,
    changed: Option<Instant>,
    full: Option<Arc<RgbaImage>>,
    tile: Option<Tile>,
    job: Option<Job>,
    failed: bool,
}

impl State {
    fn select(&mut self, desired: Option<Key>) {
        if self.desired != desired {
            self.desired = desired;
            self.changed = Some(Instant::now());
            self.full = None;
            self.tile = None;
            self.failed = false;
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.job else { return };
        let result = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => None,
        };
        let key = job.key;
        self.job = None;
        if self.desired != Some(key) {
            return;
        }
        if let Some(rendered) = result {
            let image = ColorImage::from_rgba_unmultiplied(
                [rendered.pixels.w as usize, rendered.pixels.h as usize],
                &rendered.pixels.data,
            );
            self.tile = Some(Tile {
                region: rendered.region,
                texture: ctx.load_texture("photo-detail", image, TextureOptions::LINEAR),
            });
            self.full = Some(rendered.full);
        } else {
            // A failed worker keeps the usable preview and is retried only
            // after selecting or changing the photograph.
            self.failed = true;
        }
    }
}

fn state(ctx: &egui::Context) -> Arc<Mutex<State>> {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Arc<Mutex<State>>>(egui::Id::new("photo-detail-state"))
            .clone()
    })
}

/// Draw after the regular preview, before crop handles or other overlays.
/// The cached CPU image and queued work are each bounded to one photograph.
pub(super) fn draw(ui: &Ui, studio: &Studio, dest: Rect, viewport: Rect, preview_size: Vec2) {
    let state = state(ui.ctx());
    let Ok(mut state) = state.lock() else { return };
    let selected = studio.photo.selected.zip(studio.photo.selected());
    let detail = selected.and_then(|(index, image)| {
        let raw = image.raw.as_ref()?;
        let physical_width = dest.width() * ui.ctx().pixels_per_point();
        if !dest.is_finite()
            || dest.width() <= 0.0
            || dest.height() <= 0.0
            || physical_width <= preview_size.x * 1.05
        {
            return None;
        }
        let dimensions = if studio.photo.show_original {
            image.dimensions()
        } else {
            let (w, h) = image.dimensions();
            image.develop.output_dim(w, h)
        };
        if dimensions.0 as f32 <= preview_size.x && dimensions.1 as f32 <= preview_size.y {
            return None;
        }
        let key = Key {
            selected: index,
            version: studio.photo.sel_version,
            original: studio.photo.show_original,
            source: Arc::as_ptr(raw) as usize,
        };
        Some((key, image, dimensions))
    });
    state.select(detail.as_ref().map(|(key, _, _)| *key));
    state.poll(ui.ctx());
    let Some((key, image, dimensions)) = detail else {
        return;
    };
    let clip = viewport.intersect(ui.clip_rect());
    let Some(visible) = region(dimensions, dest, clip, 0) else {
        return;
    };
    let Some(requested) = region(dimensions, dest, clip, 192) else {
        return;
    };
    let scale = dest.width() * ui.ctx().pixels_per_point() / dimensions.0 as f32;
    let covered = state.tile.as_ref().is_some_and(|tile| {
        adequate(
            tile.region,
            visible,
            requested,
            tile.texture.size()[0] as u32,
            scale,
        )
    });
    if !covered && state.job.is_none() && !state.failed {
        let elapsed = state
            .changed
            .map_or(Duration::MAX, |changed| changed.elapsed());
        if elapsed >= Duration::from_millis(100) {
            let limit = ui
                .input(|input| input.max_texture_side)
                .clamp(1, MAX_TILE_EDGE as usize) as u32;
            let cached = state.full.clone();
            // A RAW PhotoImage only clones bounded previews; sensor pixels are
            // shared through Arc, and all full-resolution development stays here.
            let source = cached.is_none().then(|| image.clone());
            let (tx, receiver) = mpsc::channel();
            let ctx = ui.ctx().clone();
            state.job = Some(Job { key, receiver });
            std::thread::spawn(move || {
                let rendered = std::panic::catch_unwind(|| {
                    let full = cached.unwrap_or_else(|| {
                        let mut photo = source.expect("uncached RAW source");
                        if key.original {
                            photo.develop = DevelopParams::default();
                        }
                        Arc::new(photo.render_full())
                    });
                    let pixels = tile_pixels(&full, requested, limit);
                    Rendered {
                        full,
                        region: requested,
                        pixels,
                    }
                })
                .ok();
                let _ = tx.send(rendered);
                ctx.request_repaint();
            });
        } else {
            ui.ctx()
                .request_repaint_after(Duration::from_millis(100) - elapsed);
        }
    }
    if let Some(tile) = &state.tile {
        let size = egui::vec2(dimensions.0 as f32, dimensions.1 as f32);
        let min =
            dest.min + dest.size() * egui::vec2(tile.region.x as f32, tile.region.y as f32) / size;
        let extent = dest.size() * egui::vec2(tile.region.w as f32, tile.region.h as f32) / size;
        ui.painter().with_clip_rect(clip).image(
            tile.texture.id(),
            Rect::from_min_size(min, extent),
            Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    let limited = state.tile.as_ref().is_some_and(|tile| {
        let samples = tile.region.w as f32 / tile.texture.size()[0] as f32;
        samples > 1.05 && scale * samples > 1.05
    });
    let status = if state.failed {
        Some("Preview (detail failed)")
    } else if !covered {
        Some("Rendering detail…")
    } else if limited {
        Some("Zoom in for full detail")
    } else {
        None
    };
    if let Some(status) = status {
        let galley = ui.painter().layout_no_wrap(
            status.to_owned(),
            egui::FontId::proportional(12.0),
            crate::ui::theme::fg_weak(),
        );
        let position = clip.right_bottom() - galley.size() - egui::vec2(12.0, 12.0);
        let painter = ui.painter().with_clip_rect(clip);
        painter.rect_filled(
            Rect::from_min_size(position, galley.size()).expand(5.0),
            3.0,
            crate::ui::theme::bg_panel(),
        );
        painter.galley(position, galley, Color32::WHITE);
    }
}

fn adequate(tile: Region, visible: Region, requested: Region, tile_width: u32, scale: f32) -> bool {
    tile.contains(visible)
        && (tile == requested || tile.w as f32 / tile_width as f32 <= (1.0 / scale).max(1.0) * 1.05)
}

/// Native screenshots wait for the requested detail instead of capturing an
/// enlarged preview while the worker is still running.
pub(super) fn ready(ctx: &egui::Context) -> bool {
    let state =
        ctx.data(|data| data.get_temp::<Arc<Mutex<State>>>(egui::Id::new("photo-detail-state")));
    state.is_none_or(|state| {
        state.lock().map_or(true, |state| {
            state.desired.is_none() || state.failed || (state.tile.is_some() && state.job.is_none())
        })
    })
}

fn region((w, h): (u32, u32), dest: Rect, clip: Rect, padding: u32) -> Option<Region> {
    let visible = dest.intersect(clip);
    if !visible.is_positive() || w == 0 || h == 0 {
        return None;
    }
    let x0 = (((visible.min.x - dest.min.x) as f64 / dest.width() as f64 * w as f64).floor()
        as u32)
        .saturating_sub(padding);
    let y0 = (((visible.min.y - dest.min.y) as f64 / dest.height() as f64 * h as f64).floor()
        as u32)
        .saturating_sub(padding);
    let x1 = (((visible.max.x - dest.min.x) as f64 / dest.width() as f64 * w as f64).ceil() as u32)
        .saturating_add(padding)
        .min(w);
    let y1 = (((visible.max.y - dest.min.y) as f64 / dest.height() as f64 * h as f64).ceil()
        as u32)
        .saturating_add(padding)
        .min(h);
    (x1 > x0 && y1 > y0).then_some(Region {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    })
}

fn tile_pixels(full: &RgbaImage, region: Region, limit: u32) -> RgbaImage {
    let buffer =
        image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(full.w, full.h, full.data.as_slice())
            .expect("validated developed pixels");
    let view = image::imageops::crop_imm(&buffer, region.x, region.y, region.w, region.h);
    let scale = (limit as f64 / region.w.max(region.h) as f64).min(1.0);
    let w = (region.w as f64 * scale).round().max(1.0) as u32;
    let h = (region.h as f64 * scale).round().max(1.0) as u32;
    if (w, h) == (region.w, region.h) {
        let mut data = Vec::with_capacity(w as usize * h as usize * 4);
        for row in region.y..region.y + region.h {
            let start = (row as usize * full.w as usize + region.x as usize) * 4;
            data.extend_from_slice(&full.data[start..start + w as usize * 4]);
        }
        return RgbaImage { w, h, data };
    }
    let pixels = image::imageops::resize(&*view, w, h, image::imageops::FilterType::Triangle);
    RgbaImage {
        w,
        h,
        data: pixels.into_raw(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_zoom_tile_maps_to_exact_full_resolution_pixels() {
        let full = RgbaImage {
            w: 8,
            h: 4,
            data: (0u8..32).flat_map(|i| [i, i + 1, i + 2, 255]).collect(),
        };
        let dest = Rect::from_min_size(egui::pos2(-2.0, -1.0), egui::vec2(8.0, 4.0));
        let clip = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(3.0, 2.0));
        let visible = region((8, 4), dest, clip, 0).unwrap();
        assert_eq!(
            visible,
            Region {
                x: 2,
                y: 1,
                w: 3,
                h: 2
            }
        );
        let tile = tile_pixels(&full, visible, 2048);
        assert_eq!((tile.w, tile.h), (3, 2));
        assert_eq!(&tile.data[..4], &[10, 11, 12, 255]);
        assert_eq!(&tile.data[20..24], &[20, 21, 22, 255]);
        let scaled = tile_pixels(
            &full,
            Region {
                x: 0,
                y: 0,
                w: 8,
                h: 4,
            },
            4,
        );
        assert_eq!((scaled.w, scaled.h), (4, 2));
    }

    #[test]
    fn stale_detail_worker_cannot_replace_current_photo() {
        let ctx = egui::Context::default();
        let old = Key {
            selected: 0,
            version: 1,
            original: false,
            source: 1,
        };
        let current = Key {
            selected: 1,
            version: 2,
            original: false,
            source: 2,
        };
        let (tx, receiver) = mpsc::channel();
        let mut state = State {
            desired: Some(old),
            job: Some(Job { key: old, receiver }),
            ..Default::default()
        };
        state.select(Some(current));
        tx.send(Some(Rendered {
            full: Arc::new(RgbaImage {
                w: 1,
                h: 1,
                data: vec![0, 0, 0, 255],
            }),
            region: Region {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
            pixels: RgbaImage {
                w: 1,
                h: 1,
                data: vec![0, 0, 0, 255],
            },
        }))
        .unwrap();
        state.poll(&ctx);
        assert!(state.full.is_none() && state.tile.is_none() && state.job.is_none());
        assert!(!state.failed);
    }

    #[test]
    fn magnified_native_and_texture_limited_tiles_do_not_render_repeatedly() {
        let tile = Region {
            x: 0,
            y: 0,
            w: 4096,
            h: 2048,
        };
        let visible = Region {
            x: 100,
            y: 100,
            w: 2000,
            h: 1000,
        };
        assert!(adequate(tile, visible, tile, 2048, 1.0));
        let native = Region {
            x: 0,
            y: 0,
            w: 1024,
            h: 1024,
        };
        let visible = Region {
            x: 100,
            y: 100,
            w: 500,
            h: 500,
        };
        assert!(adequate(native, visible, visible, 1024, 4.0));
        assert!(!adequate(native, visible, visible, 256, 1.0));
    }
}
