//! Bounded, asynchronous live previews. A single worker renders visible documents;
//! unchanged documents keep their texture and closed documents release it.
use crate::app::Studio;
use crate::compositor::{self, Draft, View};
use crate::document::{Document, LayerKind, Pixels};
use crate::geom::{Bounds, Pt};
use eframe::egui::{self, TextureHandle};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

const JOB: &str = "document-thumbnail-render";
const CACHE: &str = "document-thumbnail-cache";
const READY: &str = "document-thumbnails-ready";
const SIDE: u32 = 128;
const REFRESH: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, PartialEq)]
struct Revision {
    edited: Instant,
    motion: Option<u32>,
    dark: bool,
}

#[derive(Clone)]
struct Preview {
    revision: Revision,
    texture: TextureHandle,
    updated: Instant,
}

#[derive(Clone, Default)]
pub(super) struct Cache {
    images: HashMap<String, Preview>,
    pending: bool,
    visible: Vec<String>,
    needs_initial: bool,
    interacting: bool,
    playing: bool,
    playback_stopped: bool,
}

type Rendered = (String, Revision, egui::ColorImage);

pub(super) fn begin(ctx: &egui::Context, studio: &Studio) -> Cache {
    let mut cache = ctx
        .data_mut(|d| d.remove_temp::<Cache>(egui::Id::new(CACHE)))
        .unwrap_or_default();
    cache.pending = false;
    let playing = studio.is_motion() && studio.playing;
    cache.playback_stopped = cache.playing && !playing;
    cache.playing = playing;
    // Snapshotting vectors and pixel proxies is UI-thread work. It must not
    // compete with a drag or stroke, even when the stale tab is inactive.
    let (down, released) =
        ctx.input(|input| (input.pointer.any_down(), input.pointer.any_released()));
    cache.interacting = down || released;
    if released {
        // Tabs render before the canvas commits its release event. Snapshot
        // the completed document next frame, after the final brush/drag step.
        ctx.request_repaint();
    }
    if let Some(Ok((id, revision, image))) = super::super::jobs::poll::<Rendered>(ctx, JOB) {
        let texture = if let Some(existing) = cache.images.get_mut(&id) {
            existing.texture.set(image, egui::TextureOptions::LINEAR);
            existing.texture.clone()
        } else {
            ctx.load_texture(
                format!("document-{id}"),
                image,
                egui::TextureOptions::LINEAR,
            )
        };
        cache.images.insert(
            id,
            Preview {
                revision,
                texture,
                updated: Instant::now(),
            },
        );
    }
    let open: HashSet<_> = (0..studio.tab_count())
        .filter_map(|i| studio.tab_preview_source(i).map(|(_, id, _, _)| id))
        .collect();
    cache.images.retain(|id, _| open.contains(id.as_str()));
    cache.needs_initial = cache
        .visible
        .iter()
        .any(|id| !cache.images.contains_key(id));
    cache.visible.clear();
    cache
}

impl Cache {
    pub(super) fn image(
        &mut self,
        ctx: &egui::Context,
        studio: &Studio,
        i: usize,
    ) -> Option<&TextureHandle> {
        let (doc, id, edited, playhead) = studio.tab_preview_source(i)?;
        let motion = if i == studio.active_tab {
            studio.is_motion()
        } else {
            doc.workspace == Some(crate::tools::Persona::Motion)
        };
        let revision = Revision {
            edited,
            motion: motion.then_some(playhead.to_bits()),
            dark: ctx.theme() == egui::Theme::Dark,
        };
        let cached = self.images.get(id);
        self.visible.push(id.to_owned());
        let changed = cached.is_none_or(|preview| preview.revision != revision);
        self.pending |= changed;
        if changed
            && !self.interacting
            // Playback already renders the full canvas. A second posed render
            // for a tiny tab competes for CPU, including native-size effects.
            // Keep the last preview and refresh once playback pauses.
            && !self.playing
            && (cached.is_none() || !self.needs_initial)
            && !super::super::jobs::is_running::<Rendered>(ctx, JOB)
        {
            let wait = cached
                .filter(|_| !self.playback_stopped)
                .map(|preview| REFRESH.saturating_sub(preview.updated.elapsed()))
                .unwrap_or_default();
            if wait.is_zero() {
                let doc = snapshot(doc);
                let id = id.to_owned();
                super::super::jobs::start(ctx, JOB, move || {
                    render(&doc, revision.motion.map(f32::from_bits))
                        .map(|image| (id, revision, image))
                });
            } else {
                ctx.request_repaint_after(wait);
            }
        }
        cached.map(|preview| &preview.texture)
    }

    pub(super) fn store(self, ctx: &egui::Context) {
        // Initial-thumbnail priority is based on the previous visible rows.
        // Scrolling can hide the missing row before it starts; guarantee that
        // stale rows remaining in view get another scheduling opportunity.
        if self.pending
            && !self.interacting
            && !self.playing
            && !super::super::jobs::is_running::<Rendered>(ctx, JOB)
        {
            ctx.request_repaint_after(REFRESH);
        }
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new(READY), !self.pending);
            d.insert_temp(egui::Id::new(CACHE), self);
        });
    }
}

pub(super) fn ready(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(egui::Id::new(READY)))
        .unwrap_or(false)
        && !super::super::jobs::is_running::<Rendered>(ctx, JOB)
}

/// Raster buffers are Vecs, not shared storage. Build a bounded image proxy on
/// the UI thread instead of repeatedly cloning a full-resolution photograph.
fn snapshot(doc: &Document) -> Document {
    let mut out = doc.layout_snapshot_with_shapes(|shape| {
        shape.clone_with_mask(
            shape
                .mask
                .as_ref()
                .map(|mask| proxy_pixels(mask, (256.0 / mask.w.max(mask.h) as f32).min(1.0))),
        )
    });
    out.transparent = doc.transparent;
    out.motion = doc.motion.clone();
    for (source, target) in doc.layers.iter().zip(&mut out.layers) {
        target.pass_through = source.pass_through;
        target.filters = source.filters.clone();
        target.mask_origin = source.mask_origin;
        target.mask_size = source.mask_size;
        let largest_mask = source.mask.as_ref().map_or(1, |mask| mask.w.max(mask.h));
        let ratio = match &source.kind {
            LayerKind::Raster { pixels, .. } => {
                (256.0 / pixels.w.max(pixels.h).max(largest_mask) as f32).min(1.0)
            }
            LayerKind::Vector { .. } => (256.0 / largest_mask as f32).min(1.0),
        };
        target.mask = source.mask.as_ref().map(|mask| proxy_pixels(mask, ratio));
        match &source.kind {
            LayerKind::Raster {
                pixels,
                origin,
                size,
                rotation,
                shear,
            } => {
                target.kind = LayerKind::Raster {
                    pixels: proxy_pixels(pixels, ratio),
                    origin: *origin,
                    size: if size.x.abs() > 0.5 && size.y.abs() > 0.5 {
                        *size
                    } else {
                        Pt::new(pixels.w as f32, pixels.h as f32)
                    },
                    rotation: *rotation,
                    shear: *shear,
                };
            }
            LayerKind::Vector { .. } => {
                if let Some(mask) = &source.mask
                    && (source.mask_size.x <= 0.0 || source.mask_size.y <= 0.0)
                {
                    target.mask_origin = Pt::ZERO;
                    target.mask_size = Pt::new(mask.w as f32, mask.h as f32);
                }
            }
        }
    }
    out
}

fn proxy_pixels(source: &Pixels, ratio: f32) -> Pixels {
    let width = (source.w as f32 * ratio).round().max(1.0) as u32;
    let height = (source.h as f32 * ratio).round().max(1.0) as u32;
    let mut out = Pixels::new(width, height);
    // Four samples per destination pixel bound work by thumbnail size, including
    // masks. Full-resolution data is never copied or scanned for a preview.
    for y in 0..height {
        for x in 0..width {
            let mut channels = [0u16; 4];
            for (ox, oy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                let sx = (((x as f32 + ox) * source.w as f32 / width as f32) as u32)
                    .min(source.w.saturating_sub(1));
                let sy = (((y as f32 + oy) * source.h as f32 / height as f32) as u32)
                    .min(source.h.saturating_sub(1));
                let index = ((sy * source.w + sx) * 4) as usize;
                for (channel, total) in channels.iter_mut().enumerate() {
                    *total += source.data.get(index + channel).copied().unwrap_or(0) as u16;
                }
            }
            let index = ((y * width + x) * 4) as usize;
            for (channel, total) in channels.into_iter().enumerate() {
                out.data[index + channel] = (total / 4) as u8;
            }
        }
    }
    out.touch();
    out
}

fn bounds(doc: &Document) -> Bounds {
    let mut bounds = doc
        .artboards
        .iter()
        .map(|artboard| artboard.bounds())
        .reduce(|a, b| a.union(b))
        .unwrap_or_else(|| Bounds::from_min_size(Pt::ZERO, Pt::new(doc.width, doc.height)));
    // Layout frames can extend outside the initial document size.
    for layer in doc.layers.iter().filter(|layer| layer.visible) {
        if let Some(shapes) = layer.kind.shapes() {
            for shape in shapes
                .iter()
                .filter(|shape| shape.visible && shape.layout.frame)
            {
                bounds = bounds.union(shape.world_bbox());
            }
        }
    }
    bounds
}

fn render(doc: &Document, playhead: Option<f32>) -> Result<egui::ColorImage, String> {
    let bounds = bounds(doc);
    let edge = SIDE as f32;
    let scale =
        ((edge - 8.0) / bounds.width().max(1.0)).min((edge - 8.0) / bounds.height().max(1.0));
    let view = View {
        scale,
        offset: Pt::new(edge * 0.5, edge * 0.5) - bounds.center() * scale,
    };
    let pixels =
        compositor::render_view_posed(doc, view, SIDE, SIDE, Draft::none(), playhead, None)
            .ok_or("Could not render the document preview")?;
    Ok(egui::ColorImage::from_rgba_premultiplied(
        [SIDE as usize, SIDE as usize],
        pixels.data(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;
    use crate::document::{Cmd, Shape, Style};
    use crate::geom::Geom;

    #[test]
    fn thumbnail_changes_with_document_edits_and_preserves_aspect() {
        let mut studio = Studio::new();
        studio.doc = Document::new("wide", 200.0, 100.0, 72.0);
        let before = render(&studio.doc, None).unwrap();
        let shape = Shape::new(
            Geom::Rect {
                origin: Pt::ZERO,
                size: Pt::new(200.0, 100.0),
                radius: 0.0,
            },
            Style {
                fill: crate::document::Fill::Solid(Rgba::rgb(255, 0, 0)),
                stroke: None,
            },
        );
        studio.commit(Cmd::AddShape { layer: 1, shape });
        let after = render(&studio.doc, None).unwrap();
        assert_eq!(after.size, [128, 128]);
        assert_ne!(before.pixels, after.pixels);
        assert_eq!(after.pixels[64 * 128 + 64], egui::Color32::RED);
        assert_eq!(before.pixels[4 * 128 + 64], after.pixels[4 * 128 + 64]);
    }

    #[test]
    fn raster_snapshot_bounds_memory_and_preserves_placement_and_masks() {
        let mut doc = Document::new("Large photo", 2048.0, 1024.0, 72.0);
        doc.layers[0].mask = Some(Pixels::new(2048, 1024));
        let LayerKind::Raster {
            origin,
            size,
            rotation,
            ..
        } = &mut doc.layers[0].kind
        else {
            unreachable!()
        };
        *origin = Pt::new(30.0, 70.0);
        *size = Pt::new(800.0, 400.0);
        *rotation = 0.5;
        let proxy = snapshot(&doc);
        let layer = &proxy.layers[0];
        let LayerKind::Raster {
            pixels,
            origin,
            size,
            rotation,
            ..
        } = &layer.kind
        else {
            unreachable!()
        };
        assert_eq!((pixels.w, pixels.h), (256, 128));
        assert_eq!(pixels.data.len(), 256 * 128 * 4);
        assert_eq!(layer.mask.as_ref().unwrap().data.len(), pixels.data.len());
        assert_eq!(*origin, Pt::new(30.0, 70.0));
        assert_eq!(*size, Pt::new(800.0, 400.0));
        assert_eq!(*rotation, 0.5);
        assert_eq!(
            doc.layers[0].kind.pixels().unwrap().data.len(),
            2048 * 1024 * 4
        );
    }

    #[test]
    fn object_masks_are_bounded_without_changing_the_thumbnail() {
        let mut doc = Document::new("Masked vector", 200.0, 100.0, 72.0);
        let mut shape = Shape::new(
            Geom::Rect {
                origin: Pt::ZERO,
                size: Pt::new(200.0, 100.0),
                radius: 0.0,
            },
            Style {
                fill: crate::document::Fill::Solid(Rgba::rgb(255, 0, 0)),
                stroke: None,
            },
        );
        shape.mask = Pixels::from_rgba(2048, 1024, vec![255; 2048 * 1024 * 4]);
        doc.layers[1].kind.shapes_mut().unwrap().push(shape);
        let proxy = snapshot(&doc);
        let mask = proxy.layers[1].kind.shapes().unwrap()[0]
            .mask
            .as_ref()
            .unwrap();
        assert_eq!((mask.w, mask.h, mask.data.len()), (256, 128, 256 * 128 * 4));
        let original = render(&doc, None).unwrap();
        let thumbnail = render(&proxy, None).unwrap();
        // The compositor's workspace theme is global and other UI tests may
        // change it concurrently. Compare the actual artboard, not its margin.
        assert!((35..93).all(|y| {
            (5..123).all(|x| original.pixels[y * 128 + x] == thumbnail.pixels[y * 128 + x])
        }));
        assert_eq!(
            doc.layers[1].kind.shapes().unwrap()[0]
                .mask
                .as_ref()
                .unwrap()
                .w,
            2048
        );
    }

    #[test]
    fn rotated_gradient_mask_proxy_preserves_placement_and_softness() {
        let mut doc = Document::new("Soft rotated mask", 300.0, 200.0, 72.0);
        let mut shape = Shape::new(
            Geom::Rect {
                origin: Pt::new(60.0, 50.0),
                size: Pt::new(160.0, 80.0),
                radius: 0.0,
            },
            Style {
                fill: crate::document::Fill::Solid(Rgba::rgb(255, 0, 0)),
                stroke: None,
            },
        );
        let mut mask = Pixels::new(1024, 512);
        for (i, pixel) in mask.data.chunks_exact_mut(4).enumerate() {
            let value = ((i % 1024) * 255 / 1023) as u8;
            pixel.copy_from_slice(&[value, value, value, 255]);
        }
        mask.touch();
        shape.mask = Some(mask);
        shape.rotation = 0.37;
        doc.layers[1].kind.shapes_mut().unwrap().push(shape);
        let original = render(&doc, None).unwrap();
        let proxy = render(&snapshot(&doc), None).unwrap();
        let errors: Vec<_> = original
            .pixels
            .iter()
            .zip(&proxy.pixels)
            .enumerate()
            .filter(|(i, _)| (5..123).contains(&(i % 128)) && (25..103).contains(&(i / 128)))
            .map(|(_, pixels)| pixels)
            .flat_map(|(a, b)| a.to_array().into_iter().zip(b.to_array()))
            .map(|(a, b)| a.abs_diff(b) as usize)
            .collect();
        assert!(
            errors.iter().sum::<usize>() as f32 / (errors.len() as f32) < 0.1,
            "proxy must preserve the rotated gradient, including its soft edge"
        );
        assert!(
            errors.iter().filter(|&&error| error > 8).count() < 128,
            "only edge sampling may differ by more than a few channel values"
        );
    }

    #[test]
    fn preview_snapshots_wait_for_pointer_release_and_then_refresh() {
        let ctx = egui::Context::default();
        let mut studio = Studio::new();
        studio.doc = Document::new("Preview", 8.0, 8.0, 72.0);
        let pointer = |pressed| egui::RawInput {
            events: vec![egui::Event::PointerButton {
                pos: egui::pos2(20.0, 20.0),
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        ctx.begin_pass(pointer(true));
        let mut cache = begin(&ctx, &studio);
        assert!(cache.image(&ctx, &studio, 0).is_none());
        assert!(!super::super::super::jobs::is_running::<Rendered>(
            &ctx, JOB
        ));
        cache.store(&ctx);
        assert!(!ready(&ctx));
        ctx.end_pass().textures_delta.clear();

        ctx.begin_pass(pointer(false));
        let mut cache = begin(&ctx, &studio);
        cache.image(&ctx, &studio, 0);
        assert!(!super::super::super::jobs::is_running::<Rendered>(
            &ctx, JOB
        ));
        cache.store(&ctx);
        ctx.end_pass().textures_delta.clear();

        ctx.begin_pass(egui::RawInput::default());
        let mut cache = begin(&ctx, &studio);
        cache.image(&ctx, &studio, 0);
        assert!(super::super::super::jobs::is_running::<Rendered>(&ctx, JOB));
        cache.store(&ctx);
        ctx.end_pass().textures_delta.clear();
    }

    #[test]
    fn cached_previews_move_between_frames_and_closed_tabs_release_them() {
        let ctx = egui::Context::default();
        let mut studio = Studio::new();
        studio.show_welcome = false;
        let (_, id, edited, _) = studio.tab_preview_source(0).unwrap();
        let id = id.to_owned();
        let mut cache = Cache::default();
        cache.images.insert(
            id.clone(),
            Preview {
                revision: Revision {
                    edited,
                    motion: None,
                    dark: ctx.theme() == egui::Theme::Dark,
                },
                texture: ctx.load_texture(
                    "test",
                    egui::ColorImage::new([1, 1], vec![egui::Color32::WHITE]),
                    egui::TextureOptions::LINEAR,
                ),
                updated: Instant::now(),
            },
        );
        let storage = cache.images.get_key_value(&id).unwrap().0.as_ptr();
        cache.store(&ctx);
        let cache = begin(&ctx, &studio);
        assert_eq!(
            cache.images.get_key_value(&id).unwrap().0.as_ptr(),
            storage,
            "frames must not clone every cached tab identity and texture"
        );
        cache.store(&ctx);
        studio.new_tab();
        studio.close_tab(0);
        assert!(begin(&ctx, &studio).images.is_empty());
    }

    #[test]
    fn playing_keeps_the_cached_thumbnail_and_pause_refreshes_immediately() {
        let ctx = egui::Context::default();
        let mut studio = Studio::new();
        studio.doc = Document::new("Motion preview", 8.0, 8.0, 72.0);
        studio.persona = crate::tools::Persona::Motion;
        let (_, id, edited, _) = studio.tab_preview_source(0).unwrap();
        let id = id.to_owned();
        let texture = ctx.load_texture(
            "paused-preview",
            egui::ColorImage::new([1, 1], vec![egui::Color32::RED]),
            egui::TextureOptions::LINEAR,
        );
        let texture_id = texture.id();
        let mut cache = Cache::default();
        cache.images.insert(id.clone(), Preview {
            revision: Revision {
                edited,
                motion: Some(0.0f32.to_bits()),
                dark: ctx.theme() == egui::Theme::Dark,
            },
            texture,
            // Even an updated texture inside the ordinary refresh interval
            // must immediately become eligible after playback stops.
            updated: Instant::now() + REFRESH,
        });
        cache.store(&ctx);
        studio.playing = true;
        for playhead in [0.25, 0.5, 0.75] {
            studio.playhead = playhead;
            let mut cache = begin(&ctx, &studio);
            assert_eq!(cache.image(&ctx, &studio, 0).unwrap().id(), texture_id);
            assert!(!super::super::super::jobs::is_running::<Rendered>(&ctx, JOB));
            cache.store(&ctx);
        }
        studio.playing = false;
        let mut cache = begin(&ctx, &studio);
        assert_eq!(cache.image(&ctx, &studio, 0).unwrap().id(), texture_id);
        assert!(super::super::super::jobs::is_running::<Rendered>(&ctx, JOB));
        cache.store(&ctx);
    }

    #[test]
    fn playing_defers_cold_previews_for_active_and_inactive_tabs() {
        let ctx = egui::Context::default();
        let mut studio = Studio::new();
        studio.doc = Document::new("Inactive preview", 8.0, 8.0, 72.0);
        studio.show_welcome = false;
        studio.new_tab();
        studio.doc = Document::new("Playing preview", 8.0, 8.0, 72.0);
        studio.persona = crate::tools::Persona::Motion;
        studio.playing = true;
        let mut cache = begin(&ctx, &studio);
        for i in 0..studio.tab_count() {
            assert!(cache.image(&ctx, &studio, i).is_none());
        }
        assert!(!super::super::super::jobs::is_running::<Rendered>(&ctx, JOB));
        cache.store(&ctx);
        assert!(!ready(&ctx));

        studio.playing = false;
        let mut cache = begin(&ctx, &studio);
        assert!(cache.image(&ctx, &studio, studio.active_tab).is_none());
        assert!(super::super::super::jobs::is_running::<Rendered>(&ctx, JOB));
        cache.store(&ctx);
    }
}
