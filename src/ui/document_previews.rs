//! Bounded, asynchronous live previews. A single worker renders visible documents;
//! unchanged documents keep their texture and closed documents release it.
use crate::app::Studio;
use crate::compositor::{self, Draft, View};
use crate::document::{Document, LayerKind, Pixels};
use crate::geom::{Bounds, Pt};
use eframe::egui::{self, TextureHandle};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const JOB: &str = "document-thumbnail-render";
const CACHE: &str = "document-thumbnail-cache";
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
}

type Rendered = (String, Revision, egui::ColorImage);

pub(super) fn begin(ctx: &egui::Context, studio: &Studio) -> Cache {
    let mut cache = ctx
        .data(|d| d.get_temp::<Cache>(egui::Id::new(CACHE)))
        .unwrap_or_default();
    cache.pending = false;
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
    cache.images.retain(|id, _| {
        (0..studio.tab_count()).any(|i| {
            studio
                .tab_preview_source(i)
                .is_some_and(|(_, open_id, _, _)| id == open_id)
        })
    });
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
        let revision = Revision {
            edited,
            motion: studio.is_motion().then_some(playhead.to_bits()),
            dark: ctx.theme() == egui::Theme::Dark,
        };
        let cached = self.images.get(id);
        self.visible.push(id.to_owned());
        let changed = cached.is_none_or(|preview| preview.revision != revision);
        self.pending |= changed;
        if changed
            && (cached.is_none() || !self.needs_initial)
            && !super::super::jobs::is_running::<Rendered>(ctx, JOB)
        {
            let wait = cached
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
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(CACHE), self));
    }
}

pub(super) fn ready(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<Cache>(egui::Id::new(CACHE)))
        .is_some_and(|cache| !cache.pending)
        && !super::super::jobs::is_running::<Rendered>(ctx, JOB)
}

/// Raster buffers are Vecs, not shared storage. Build a bounded image proxy on
/// the UI thread instead of repeatedly cloning a full-resolution photograph.
fn snapshot(doc: &Document) -> Document {
    let mut out = doc.layout_snapshot();
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
}
