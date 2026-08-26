use crate::document::{Document, Fill, LayerBlend, LayerKind, Shape};
use eframe::egui::{pos2, Color32, Pos2, Rect, Stroke, Vec2};

pub const ELLIPSE_SEGMENTS: usize = 96;
const CHECKER_A: Color32 = Color32::from_rgb(0xE9, 0xE9, 0xE9);
const CHECKER_B: Color32 = Color32::from_rgb(0xCF, 0xCF, 0xCF);
const CANVAS_BG: Color32 = Color32::from_rgb(0x24, 0x28, 0x2E);

#[derive(Clone, Copy, Debug)]
pub struct View {
    pub scale: f32,
    pub offset: Vec2,
}

impl Default for View {
    fn default() -> Self {
        Self {
            scale: 1.0,
            offset: Vec2::ZERO,
        }
    }
}

impl View {
    pub fn to_screen(&self, p: Pos2) -> Pos2 {
        pos2(p.x * self.scale + self.offset.x, p.y * self.scale + self.offset.y)
    }

    pub fn to_world(self, p: Pos2) -> Pos2 {
        pos2((p.x - self.offset.x) / self.scale, (p.y - self.offset.y) / self.scale)
    }

    pub fn zoom_at(&mut self, screen_pos: Pos2, factor: f32) {
        let world = self.to_world(screen_pos);
        self.scale = (self.scale * factor).clamp(0.02, 64.0);
        self.offset = screen_pos.to_vec2() - world.to_vec2() * self.scale;
    }

    pub fn fit(&mut self, doc_size: Vec2, viewport: Rect) {
        if viewport.width() < 1.0 || viewport.height() < 1.0 {
            return;
        }
        self.scale = ((viewport.width() / doc_size.x).min(viewport.height() / doc_size.y) * 0.92)
            .clamp(0.01, 64.0);
        let scaled = doc_size * self.scale;
        self.offset = viewport.center().to_vec2() - scaled * 0.5;
    }

    fn transform(&self) -> tiny_skia::Transform {
        tiny_skia::Transform::from_row(self.scale, 0.0, 0.0, self.scale, self.offset.x, self.offset.y)
    }
}

pub struct LayerTexture {
    pub handle: eframe::egui::TextureHandle,
    pub version: u64,
}

pub struct RenderDraft<'a> {
    pub preview_shape: Option<&'a Shape>,
    pub pen_preview: Option<(&'a [crate::document::Anchor], Pos2)>,
    pub brush: Option<(usize, &'a tiny_skia::Pixmap, f32)>,
}

impl<'a> RenderDraft<'a> {
    pub fn none() -> RenderDraft<'static> {
        RenderDraft {
            preview_shape: None,
            pen_preview: None,
            brush: None,
        }
    }
}

pub fn render_view(
    doc: &Document,
    view: View,
    screen_w: u32,
    screen_h: u32,
    draft: RenderDraft<'_>,
) -> Option<tiny_skia::Pixmap> {
    let mut pm = tiny_skia::Pixmap::new(screen_w, screen_h)?;
    fill_solid(&mut pm, 0.0, 0.0, screen_w as f32, screen_h as f32, CANVAS_BG);

    let doc_screen = Rect::from_min_size(view.to_screen(pos2(0.0, 0.0)), doc.size() * view.scale);
    draw_checkerboard(&mut pm, doc_screen);
    stroke_rect(&mut pm, doc_screen, 1.0, Color32::from_rgb(0x55, 0x5A, 0x63));

    let t = view.transform();
    for (li, layer) in doc.layers.iter().enumerate() {
        if !layer.visible || layer.opacity <= 0.0 {
            continue;
        }
        let brush_buf = draft
            .brush
            .and_then(|(bl, buf, flow)| (bl == li).then_some((buf, flow)));
        draw_layer(&mut pm, layer, t, view, brush_buf, draft.preview_shape);
    }
    Some(pm)
}

fn draw_layer(
    pm: &mut tiny_skia::Pixmap,
    layer: &crate::document::Layer,
    t: tiny_skia::Transform,
    view: View,
    brush_buf: Option<(&tiny_skia::Pixmap, f32)>,
    preview: Option<&Shape>,
) {
    if layer.mask.is_some() {
        let mut temp = match tiny_skia::Pixmap::new(pm.width(), pm.height()) {
            Some(p) => p,
            None => return,
        };
        draw_layer_content(&mut temp, layer, t, view, brush_buf, preview, 1.0, LayerBlend::Normal);
        if let Some(mask) = &layer.mask {
            let mut mask_pm = match tiny_skia::Pixmap::new(pm.width(), pm.height()) {
                Some(p) => p,
                None => return,
            };
            mask_pm.draw_pixmap(
                0,
                0,
                mask.pixmap.as_ref(),
                &tiny_skia::PixmapPaint {
                    quality: tiny_skia::FilterQuality::Bilinear,
                    ..Default::default()
                },
                t,
                None,
            );
            let m = tiny_skia::Mask::from_pixmap(mask_pm.as_ref(), tiny_skia::MaskType::Alpha);
                temp.apply_mask(&m);
        }
        pm.draw_pixmap(
            0,
            0,
            temp.as_ref(),
            &tiny_skia::PixmapPaint {
                opacity: layer.opacity.clamp(0.0, 1.0),
                blend_mode: layer.blend.to_skia(),
                ..Default::default()
            },
            tiny_skia::Transform::identity(),
            None,
        );
    } else {
        draw_layer_content(pm, layer, t, view, brush_buf, preview, layer.opacity, layer.blend);
    }
}

fn draw_layer_content(
    pm: &mut tiny_skia::Pixmap,
    layer: &crate::document::Layer,
    t: tiny_skia::Transform,
    view: View,
    brush_buf: Option<(&tiny_skia::Pixmap, f32)>,
    preview: Option<&Shape>,
    opacity: f32,
    blend: LayerBlend,
) {
    if let LayerKind::Vector(v) = &layer.kind {
        for shape in &v.shapes {
            draw_shape(pm, shape, view, opacity, blend, t);
        }
        if let Some(p) = preview {
            draw_shape(pm, p, view, opacity * 0.85, blend, t);
        }
    }
    if let LayerKind::Raster(r) = &layer.kind {
        pm.draw_pixmap(
            0,
            0,
            r.pixmap.as_ref(),
            &tiny_skia::PixmapPaint {
                opacity: opacity.clamp(0.0, 1.0),
                blend_mode: blend.to_skia(),
                quality: tiny_skia::FilterQuality::Bilinear,
            },
            t,
            None,
        );
    }
    if let Some((buf, flow)) = brush_buf {
        pm.draw_pixmap(
            0,
            0,
            buf.as_ref(),
            &tiny_skia::PixmapPaint {
                opacity: flow.clamp(0.0, 1.0) * opacity.clamp(0.0, 1.0),
                blend_mode: blend.to_skia(),
                quality: tiny_skia::FilterQuality::Bilinear,
            },
            t,
            None,
        );
    }
}

fn draw_shape(
    pm: &mut tiny_skia::Pixmap,
    shape: &Shape,
    view: View,
    opacity: f32,
    blend: LayerBlend,
    t: tiny_skia::Transform,
) {
    let contours = shape.geom.contours(ELLIPSE_SEGMENTS);
    if contours.is_empty() {
        return;
    }
    let closed = shape.geom.is_closed_outline();
    let mut pb = tiny_skia::PathBuilder::new();
    let mut any = false;
    for pts in &contours {
        if pts.len() < 2 {
            continue;
        }
        let first = view.to_screen(pts[0]);
        pb.move_to(first.x, first.y);
        for p in pts.iter().skip(1) {
            let s = view.to_screen(*p);
            pb.line_to(s.x, s.y);
        }
        if closed {
            pb.close();
        }
        any = true;
    }
    if !any {
        return;
    }
    let path = match pb.finish() {
        Some(p) => p,
        None => return,
    };
    let opa = opacity.clamp(0.0, 1.0);

    if closed && !matches!(shape.style.fill, Fill::None) {
        if let Some(mut paint) = fill_paint(shape, view, opa) {
            paint.blend_mode = blend.to_skia();
            pm.fill_path(&path, &paint, tiny_skia::FillRule::Winding, t, None);
        }
    }
    if let Some(s) = &shape.style.stroke {
        if s.width > 0.0 {
            let mut paint = Paint {
                anti_alias: true,
                blend_mode: blend.to_skia(),
                ..Paint::default()
            };
            paint.set_color(tiny_skia::Color::from_rgba8(
                s.color.r(),
                s.color.g(),
                s.color.b(),
                (s.color.a() as f32 * opa) as u8,
            ));
            let ts = tiny_skia::Stroke {
                width: (s.width * view.scale).max(0.3),
                line_cap: tiny_skia::LineCap::Round,
                line_join: tiny_skia::LineJoin::Round,
                ..Default::default()
            };
            pm.stroke_path(&path, &paint, &ts, t, None);
        }
    }
}

use tiny_skia::Paint;

fn fill_paint(shape: &Shape, view: View, opa: f32) -> Option<Paint> {
    let mut paint = Paint {
        anti_alias: true,
        ..Paint::default()
    };
    match &shape.style.fill {
        Fill::None => None,
        Fill::Solid(c) => {
            paint.set_color(tiny_skia::Color::from_rgba8(
                c.r(),
                c.g(),
                c.b(),
                (c.a() as f32 * opa) as u8,
            ));
            Some(paint)
        }
        Fill::Linear { from, to, c0, c1 } => {
            let b = shape.geom.bbox();
            let w = b.width().max(1e-3);
            let h = b.height().max(1e-3);
            let abs = |n: [f32; 2]| pos2(b.min.x + n[0] * w, b.min.y + n[1] * h);
            let a0 = view.to_screen(abs(*from));
            let a1 = view.to_screen(abs(*to));
            let c0a = (c0.a() as f32 * opa) as u8;
            let c1a = (c1.a() as f32 * opa) as u8;
            let stops = vec![
                tiny_skia::GradientStop::new(0.0, tiny_skia::Color::from_rgba8(c0.r(), c0.g(), c0.b(), c0a)),
                tiny_skia::GradientStop::new(1.0, tiny_skia::Color::from_rgba8(c1.r(), c1.g(), c1.b(), c1a)),
            ];
            let shader = tiny_skia::LinearGradient::new(
                tiny_skia::Point::from_xy(a0.x, a0.y),
                tiny_skia::Point::from_xy(a1.x, a1.y),
                stops,
                tiny_skia::SpreadMode::Pad,
                tiny_skia::Transform::identity(),
            )?;
            paint.shader = shader;
            Some(paint)
        }
    }
}

fn fill_solid(pm: &mut tiny_skia::Pixmap, x: f32, y: f32, w: f32, h: f32, c: Color32) {
    let mut paint = Paint::default();
    paint.set_color(tiny_skia::Color::from_rgba8(c.r(), c.g(), c.b(), 255));
    if let Some(rect) = tiny_skia::Rect::from_xywh(x, y, w, h) {
        pm.fill_rect(rect, &paint, tiny_skia::Transform::identity(), None);
    }
}

fn stroke_rect(pm: &mut tiny_skia::Pixmap, r: Rect, width: f32, c: Color32) {
    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(r.min.x, r.min.y);
    pb.line_to(r.max.x, r.min.y);
    pb.line_to(r.max.x, r.max.y);
    pb.line_to(r.min.x, r.max.y);
    pb.close();
    if let Some(path) = pb.finish() {
        let mut paint = Paint::default();
        paint.set_color(tiny_skia::Color::from_rgba8(c.r(), c.g(), c.b(), 255));
        let ts = tiny_skia::Stroke {
            width,
            ..Default::default()
        };
        pm.stroke_path(&path, &paint, &ts, tiny_skia::Transform::identity(), None);
    }
}

fn draw_checkerboard(pm: &mut tiny_skia::Pixmap, doc_screen: Rect) {
    if doc_screen.width() < 1.0 || doc_screen.height() < 1.0 {
        return;
    }
    let clip = Rect::from_min_max(
        pos2(0.0, 0.0),
        pos2(pm.width() as f32, pm.height() as f32),
    )
    .intersect(doc_screen);
    let step = (32.0 * (doc_screen.width() / 1280.0).max(0.1)).clamp(6.0, 64.0);
    let cols = (clip.width() / step).ceil() as i32 + 1;
    let rows = (clip.height() / step).ceil() as i32 + 1;
    if cols * rows > 60_000 {
        fill_solid(pm, doc_screen.min.x, doc_screen.min.y, doc_screen.width(), doc_screen.height(), CHECKER_B);
        return;
    }
    let mut paint_b = Paint::default();
    paint_b.set_color(tiny_skia::Color::from_rgba8(CHECKER_B.r(), CHECKER_B.g(), CHECKER_B.b(), 255));
    let mut paint_a = Paint::default();
    paint_a.set_color(tiny_skia::Color::from_rgba8(CHECKER_A.r(), CHECKER_A.g(), CHECKER_A.b(), 255));
    if let Some(rect) = tiny_skia::Rect::from_xywh(
        doc_screen.min.x,
        doc_screen.min.y,
        doc_screen.width(),
        doc_screen.height(),
    ) {
        pm.fill_rect(rect, &paint_b, tiny_skia::Transform::identity(), None);
    }
    let x0 = (clip.left() / step).floor() as i32;
    let y0 = (clip.top() / step).floor() as i32;
    for row in y0..(y0 + rows) {
        for col in x0..(x0 + cols) {
            if (row + col) % 2 != 0 {
                continue;
            }
            let cell = Rect::from_min_size(
                pos2(col as f32 * step, row as f32 * step),
                Vec2::splat(step),
            );
            let c = cell.intersect(doc_screen);
            if c.width() > 0.0 && c.height() > 0.0 {
                if let Some(rect) = tiny_skia::Rect::from_xywh(c.min.x, c.min.y, c.width(), c.height()) {
                    pm.fill_rect(rect, &paint_a, tiny_skia::Transform::identity(), None);
                }
            }
        }
    }
}

pub fn straight_rgba(pm: &tiny_skia::Pixmap) -> Vec<u8> {
    #[allow(clippy::manual_div_ceil)]
    pm.data()
        .chunks_exact(4)
        .flat_map(|px| {
            let a = px[3] as u32;
            if a == 0 {
                [0u8, 0, 0, 0]
            } else {
                [
                    (((px[0] as u32 * 255 + a / 2) / a).min(255)) as u8,
                    (((px[1] as u32 * 255 + a / 2) / a).min(255)) as u8,
                    (((px[2] as u32 * 255 + a / 2) / a).min(255)) as u8,
                    px[3],
                ]
            }
        })
        .collect()
}

pub fn selection_screen_contours(geom: &crate::document::Geometry, view: View) -> Vec<Vec<Pos2>> {
    geom.contours(ELLIPSE_SEGMENTS)
        .iter()
        .map(|c| c.iter().map(|p| view.to_screen(*p)).collect())
        .collect()
}

pub mod export {
    use super::*;
    use crate::document::Document;

    pub fn png_bytes_scaled(doc: &Document, scale: f32) -> Result<Vec<u8>, String> {
        let view = View {
            scale,
            offset: Vec2::ZERO,
        };
        let w = (doc.width * scale).round().max(1.0) as u32;
        let h = (doc.height * scale).round().max(1.0) as u32;
        let pm = render_view(doc, view, w, h, RenderDraft::none())
            .ok_or_else(|| "composite failed".to_string())?;
        pm.encode_png().map_err(|e| format!("png encode: {e}"))
    }

    pub fn png_bytes(doc: &Document) -> Result<Vec<u8>, String> {
        png_bytes_scaled(doc, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Anchor, Geometry, Layer, LayerBlend, LayerKind, Style, VectorLayer};

    #[test]
    fn unpremultiply_restores_straight_alpha() {
        let mut pm = tiny_skia::Pixmap::new(2, 1).unwrap();
        pm.data_mut()[..4].copy_from_slice(&[128, 0, 0, 255]);
        pm.data_mut()[4..8].copy_from_slice(&[64, 64, 64, 128]);
        let out = straight_rgba(&pm);
        assert_eq!(&out[..4], &[128, 0, 0, 255]);
        assert_eq!(&out[4..8], &[128, 128, 128, 128]);
    }

    #[test]
    fn export_png_has_signature_and_composite_draws_fill() {
        let mut doc = Document::new(16.0, 16.0);
        doc.layers.clear();
        doc.layers.push(Layer {
            name: "v".into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            blend: LayerBlend::Normal,
            mask: None,
            kind: LayerKind::Vector(VectorLayer {
                shapes: vec![Shape {
                    id: 1,
                    geom: Geometry::Rect {
                        origin: pos2(0.0, 0.0),
                        size: eframe::egui::Vec2::splat(16.0),
                    },
                    style: Style {
                        fill: Fill::Solid(Color32::from_rgb(200, 30, 30)),
                        stroke: None,
                    },
                }],
            }),
        });
        let pm = render_view(&doc, View::default(), 16, 16, RenderDraft::none()).unwrap();
        let px = &pm.data()[0..4];
        assert_eq!(px[0], 200);
        assert_eq!(px[1], 30);
        assert_eq!(px[2], 30);
        assert_eq!(px[3], 255);

        let bytes = export::png_bytes(&doc).unwrap();
        assert_eq!(&bytes[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn gradient_fill_interpolates() {
        let mut doc = Document::new(32.0, 8.0);
        doc.layers.clear();
        doc.layers.push(Layer {
            name: "g".into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            blend: LayerBlend::Normal,
            mask: None,
            kind: LayerKind::Vector(VectorLayer {
                shapes: vec![Shape {
                    id: 1,
                    geom: Geometry::Rect {
                        origin: pos2(0.0, 0.0),
                        size: eframe::egui::Vec2::new(32.0, 8.0),
                    },
                    style: Style {
                        fill: Fill::Linear {
                            from: [0.0, 0.0],
                            to: [1.0, 0.0],
                            c0: Color32::from_rgb(0, 0, 0),
                            c1: Color32::from_rgb(255, 255, 255),
                        },
                        stroke: None,
                    },
                }],
            }),
        });
        let pm = render_view(&doc, View::default(), 32, 8, RenderDraft::none()).unwrap();
        let left = &pm.data()[(4 * 32 + 2) * 4..(4 * 32 + 2) * 4 + 4];
        let right = &pm.data()[(4 * 32 + 29) * 4..(4 * 32 + 29) * 4 + 4];
        assert!(left[0] < 40, "left should be dark, got {}", left[0]);
        assert!(right[0] > 215, "right should be bright, got {}", right[0]);
    }

    #[test]
    fn mask_limits_layer_alpha() {
        let mut doc = Document::new(16.0, 16.0);
        doc.layers.clear();
        let mut masked = Layer {
            name: "m".into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            blend: LayerBlend::Normal,
            mask: tiny_skia::Pixmap::new(16, 16).map(|pm| crate::document::RasterLayer {
                pixmap: pm,
                version: 0,
            }),
            kind: LayerKind::Vector(VectorLayer {
                shapes: vec![Shape {
                    id: 1,
                    geom: Geometry::Rect {
                        origin: pos2(0.0, 0.0),
                        size: eframe::egui::Vec2::splat(16.0),
                    },
                    style: Style {
                        fill: Fill::Solid(Color32::from_rgb(255, 0, 0)),
                        stroke: None,
                    },
                }],
            }),
        };
        if let Some(m) = masked.mask.as_mut() {
            let mut pb = tiny_skia::PathBuilder::new();
            pb.push_circle(8.0, 8.0, 4.0);
            pb.close();
            let path = pb.finish().unwrap();
            let mut paint = tiny_skia::Paint::default();
            paint.set_color(tiny_skia::Color::from_rgba8(255, 255, 255, 255));
            m.pixmap
                .fill_path(&path, &paint, tiny_skia::FillRule::Winding, tiny_skia::Transform::identity(), None);
        }
        doc.layers.push(masked);
        let pm = render_view(&doc, View::default(), 16, 16, RenderDraft::none()).unwrap();
        let center = &pm.data()[(8 * 16 + 8) * 4..(8 * 16 + 8) * 4 + 4];
        let corner = &pm.data()[(1 * 16 + 1) * 4..(1 * 16 + 1) * 4 + 4];
        assert_eq!(center[3], 255, "mask center should be opaque red");
        assert_eq!(center[0], 255);
        assert_ne!(corner[0], 255, "outside mask should not be red");
        assert_ne!(corner[1], 0, "outside mask should not be red");
    }

    #[test]
    fn bezier_path_flattens_through_handles() {
        let g = Geometry::Path {
            anchors: vec![
                Anchor::corner(pos2(0.0, 0.0)),
                Anchor {
                    pt: pos2(100.0, 0.0),
                    h_in: eframe::egui::Vec2::new(-50.0, 40.0),
                    h_out: eframe::egui::Vec2::new(50.0, 40.0),
                },
            ],
            closed: false,
        };
        let c = &g.contours(96)[0];
        assert_eq!(c[0], pos2(0.0, 0.0));
        assert_eq!(*c.last().unwrap(), pos2(100.0, 0.0));
        let mid = c[c.len() / 2];
        assert!(mid.y > 5.0, "curve should bow through handle direction");
    }
}
