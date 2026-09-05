//! One geometry, two outputs: the live canvas and PNG export.

use crate::color::Rgba;
use crate::document::{Document, Fill, Layer, LayerKind, Shape};
use crate::geom::{Geom, Pt};
use crate::motion::Pose;
use std::collections::HashMap;
use tiny_skia::{
    FillRule, GradientStop, LinearGradient, Paint, PathBuilder, Pixmap, PixmapPaint, Point,
    RadialGradient, SpreadMode, Stroke as SkStroke, StrokeDash, Transform,
};

/// Canvas camera. `offset` is in **canvas-widget pixels** (0,0 = top-left of
/// the canvas, not the window). Mixing window coordinates here is what made
/// selection handles sit off the filled shape.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub scale: f32,
    pub offset: Pt,
}

impl Default for View {
    fn default() -> Self {
        Self {
            scale: 1.0,
            offset: Pt::ZERO,
        }
    }
}

impl View {
    pub fn to_screen(self, p: Pt) -> Pt {
        Pt::new(
            p.x * self.scale + self.offset.x,
            p.y * self.scale + self.offset.y,
        )
    }

    pub fn to_world(self, p: Pt) -> Pt {
        Pt::new(
            (p.x - self.offset.x) / self.scale,
            (p.y - self.offset.y) / self.scale,
        )
    }

    /// Window-space pointer → world, given the canvas widget's top-left.
    pub fn pointer_to_world(self, canvas_origin: Pt, pointer: Pt) -> Pt {
        self.to_world(Pt::new(
            pointer.x - canvas_origin.x,
            pointer.y - canvas_origin.y,
        ))
    }

    /// World → window-space, given the canvas widget's top-left.
    pub fn world_to_window(self, canvas_origin: Pt, world: Pt) -> Pt {
        let s = self.to_screen(world);
        Pt::new(s.x + canvas_origin.x, s.y + canvas_origin.y)
    }

    pub fn zoom_at(&mut self, screen: Pt, factor: f32) {
        let world = self.to_world(screen);
        self.scale = (self.scale * factor).clamp(0.02, 64.0);
        self.offset = screen - world * self.scale;
    }

    pub fn fit(&mut self, doc: Pt, viewport: crate::geom::Bounds) {
        if viewport.width() < 1.0 || viewport.height() < 1.0 {
            return;
        }
        self.scale =
            ((viewport.width() / doc.x).min(viewport.height() / doc.y) * 0.90).clamp(0.02, 64.0);
        let scaled = doc * self.scale;
        self.offset = viewport.center() - scaled * 0.5;
    }

    /// Fit `world` into the canvas viewport (canvas-local pixels, origin at 0).
    pub fn zoom_to(&mut self, world: crate::geom::Bounds, viewport: crate::geom::Bounds) {
        let ww = world.width();
        let wh = world.height();
        if ww < 1e-3 || wh < 1e-3 || viewport.width() < 1.0 || viewport.height() < 1.0 {
            return;
        }
        self.scale = (viewport.width() / ww)
            .min(viewport.height() / wh)
            .clamp(0.02, 64.0);
        self.offset = viewport.center() - world.center() * self.scale;
    }

    fn transform(self) -> Transform {
        Transform::from_row(
            self.scale,
            0.0,
            0.0,
            self.scale,
            self.offset.x,
            self.offset.y,
        )
    }
}

const CHECKER_A: Rgba = Rgba {
    r: 0xE9,
    g: 0xE9,
    b: 0xE9,
    a: 255,
};
const CHECKER_B: Rgba = Rgba {
    r: 0xCF,
    g: 0xCF,
    b: 0xCF,
    a: 255,
};

static CANVAS_BG_PACKED: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0x00_22_26_2C);

pub fn set_canvas_bg(r: u8, g: u8, b: u8) {
    CANVAS_BG_PACKED.store(
        ((r as u32) << 16) | ((g as u32) << 8) | b as u32,
        std::sync::atomic::Ordering::Relaxed,
    );
}

fn canvas_bg() -> Rgba {
    let n = CANVAS_BG_PACKED.load(std::sync::atomic::Ordering::Relaxed);
    Rgba::rgb(
        ((n >> 16) & 0xFF) as u8,
        ((n >> 8) & 0xFF) as u8,
        (n & 0xFF) as u8,
    )
}

pub struct Draft<'a> {
    pub preview: Option<&'a Shape>,
    pub brush: Option<(usize, &'a Pixmap, f32)>,
}

impl Draft<'static> {
    pub fn none() -> Self {
        Self {
            preview: None,
            brush: None,
        }
    }
}

pub fn render_view(
    doc: &Document,
    view: View,
    screen_w: u32,
    screen_h: u32,
    draft: Draft<'_>,
) -> Option<Pixmap> {
    render_view_posed(doc, view, screen_w, screen_h, draft, None, None)
}

pub fn render_view_posed(
    doc: &Document,
    view: View,
    screen_w: u32,
    screen_h: u32,
    draft: Draft<'_>,
    motion_t: Option<f32>,
    overrides: Option<&HashMap<u64, Pose>>,
) -> Option<Pixmap> {
    let mut pm = Pixmap::new(screen_w, screen_h)?;
    fill_solid(
        &mut pm,
        0.0,
        0.0,
        screen_w as f32,
        screen_h as f32,
        canvas_bg(),
    );
    draw_plates(&mut pm, doc, view);

    let t = view.transform();
    for (li, layer) in doc.layers.iter().enumerate() {
        if !layer.visible || layer.opacity <= 0.0 {
            continue;
        }
        if is_paper_raster(layer) {
            continue;
        }
        let brush = draft
            .brush
            .and_then(|(bl, buf, flow)| (bl == li).then_some((buf, flow)));
        draw_layer(
            &mut pm,
            layer,
            t,
            brush,
            draft.preview,
            motion_t,
            doc,
            overrides,
        );
    }
    Some(pm)
}

fn render_export(doc: &Document, scale: u32) -> Result<Pixmap, String> {
    let s = scale.clamp(1, 8);
    let w = (doc.width * s as f32).round().max(1.0) as u32;
    let h = (doc.height * s as f32).round().max(1.0) as u32;
    let mut pm = Pixmap::new(w, h).ok_or("could not allocate export pixmap")?;
    draw_export_plates(&mut pm, doc, s as f32);
    let t = Transform::from_scale(s as f32, s as f32);
    for layer in &doc.layers {
        if !layer.visible || layer.opacity <= 0.0 {
            continue;
        }
        if is_paper_raster(layer) {
            continue;
        }
        draw_layer(&mut pm, layer, t, None, None, None, doc, None);
    }
    Ok(pm)
}

pub fn export_png(doc: &Document, scale: u32) -> Result<Vec<u8>, String> {
    render_export(doc, scale)?
        .encode_png()
        .map_err(|e| e.to_string())
}

pub fn export_jpeg(doc: &Document, scale: u32, quality: u8) -> Result<Vec<u8>, String> {
    let pm = render_export(doc, scale)?;
    let mut rgb = Vec::with_capacity(pm.width() as usize * pm.height() as usize * 3);
    for pixel in pm.pixels() {
        let white = 255 - pixel.alpha();
        rgb.extend_from_slice(&[
            pixel.red().saturating_add(white),
            pixel.green().saturating_add(white),
            pixel.blue().saturating_add(white),
        ]);
    }
    let mut buf = Vec::new();
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality.max(1));
    enc.encode(
        &rgb,
        pm.width(),
        pm.height(),
        image::ExtendedColorType::Rgb8,
    )
    .map_err(|e| e.to_string())?;
    Ok(buf)
}

fn draw_layer(
    pm: &mut Pixmap,
    layer: &Layer,
    t: Transform,
    brush: Option<(&Pixmap, f32)>,
    preview: Option<&Shape>,
    motion_t: Option<f32>,
    doc: &Document,
    overrides: Option<&HashMap<u64, Pose>>,
) {
    let filtered = layer.filters.active();
    if layer.mask.is_some() || filtered {
        let Some(mut temp) = Pixmap::new(pm.width(), pm.height()) else {
            return;
        };
        draw_content(
            &mut temp,
            layer,
            t,
            brush,
            preview,
            1.0,
            tiny_skia::BlendMode::SourceOver,
            motion_t,
            doc,
            overrides,
        );
        if let Some(mask) = &layer.mask
            && let Some(mask_pm) = mask.to_pixmap()
        {
            let mut placed = Pixmap::new(pm.width(), pm.height()).unwrap();
            placed.draw_pixmap(
                0,
                0,
                mask_pm.as_ref(),
                &PixmapPaint {
                    quality: tiny_skia::FilterQuality::Bilinear,
                    ..Default::default()
                },
                t,
                None,
            );
            let m = tiny_skia::Mask::from_pixmap(placed.as_ref(), tiny_skia::MaskType::Alpha);
            temp.apply_mask(&m);
        }
        if filtered {
            crate::filter::apply(&mut temp, &layer.filters);
        }
        pm.draw_pixmap(
            0,
            0,
            temp.as_ref(),
            &PixmapPaint {
                opacity: layer.opacity.clamp(0.0, 1.0),
                blend_mode: layer.blend.to_skia(),
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    } else {
        draw_content(
            pm,
            layer,
            t,
            brush,
            preview,
            layer.opacity,
            layer.blend.to_skia(),
            motion_t,
            doc,
            overrides,
        );
    }
}

fn draw_content(
    pm: &mut Pixmap,
    layer: &Layer,
    t: Transform,
    brush: Option<(&Pixmap, f32)>,
    preview: Option<&Shape>,
    opacity: f32,
    blend: tiny_skia::BlendMode,
    motion_t: Option<f32>,
    doc: &Document,
    overrides: Option<&HashMap<u64, Pose>>,
) {
    match &layer.kind {
        LayerKind::Vector { shapes } => {
            for s in shapes {
                if !s.visible {
                    continue;
                }
                let pose = pose_of(s.id, motion_t, doc, overrides);
                draw_shape(pm, s, t, opacity, blend, pose);
            }
            if let Some(p) = preview
                && p.visible
            {
                let pose = pose_of(p.id, motion_t, doc, overrides);
                draw_shape(pm, p, t, opacity * 0.85, blend, pose);
            }
        }
        LayerKind::Raster {
            pixels,
            origin,
            size,
            rotation,
        } => {
            let (ox, oy, dw, dh) = {
                let native_w = pixels.w as f32;
                let native_h = pixels.h as f32;
                let (dw, dh) = if size.x.abs() > 0.5 && size.y.abs() > 0.5 {
                    (size.x, size.y)
                } else {
                    (native_w, native_h)
                };
                (origin.x, origin.y, dw, dh)
            };
            let _ = pixels.with_pm(|src| {
                let sx = if src.width() == 0 {
                    1.0
                } else {
                    dw / src.width() as f32
                };
                let sy = if src.height() == 0 {
                    1.0
                } else {
                    dh / src.height() as f32
                };
                let mut xf = Transform::from_translate(ox, oy).pre_scale(sx, sy);
                if rotation.abs() > 1e-5 {
                    let cx = ox + dw * 0.5;
                    let cy = oy + dh * 0.5;
                    xf = Transform::from_translate(cx, cy)
                        .pre_concat(Transform::from_rotate(rotation.to_degrees()))
                        .pre_concat(Transform::from_translate(-cx, -cy))
                        .pre_concat(xf);
                }
                xf = t.pre_concat(xf);
                pm.draw_pixmap(
                    0,
                    0,
                    src.as_ref(),
                    &PixmapPaint {
                        opacity: opacity.clamp(0.0, 1.0),
                        blend_mode: blend,
                        quality: tiny_skia::FilterQuality::Bilinear,
                    },
                    xf,
                    None,
                );
            });
            if let Some((buf, flow)) = brush {
                pm.draw_pixmap(
                    0,
                    0,
                    buf.as_ref(),
                    &PixmapPaint {
                        opacity: (opacity * flow).clamp(0.0, 1.0),
                        blend_mode: blend,
                        ..Default::default()
                    },
                    t,
                    None,
                );
            }
        }
    }
}

fn pose_of(
    id: u64,
    motion_t: Option<f32>,
    doc: &Document,
    overrides: Option<&HashMap<u64, Pose>>,
) -> Pose {
    if let Some(p) = overrides.and_then(|m| m.get(&id)).copied() {
        return p;
    }
    if let Some(t) = motion_t {
        return doc.motion.pose(id, t);
    }
    Pose::identity()
}

fn draw_shape(
    pm: &mut Pixmap,
    shape: &Shape,
    t: Transform,
    opacity: f32,
    blend: tiny_skia::BlendMode,
    pose: Pose,
) {
    if shape.filters.active() {
        let pad = crate::filter::svg_pad(&shape.filters).ceil().max(8.0);
        let b = shape.world_bbox().inflate(pad);
        let tw = b.width().ceil().max(1.0) as u32;
        let th = b.height().ceil().max(1.0) as u32;
        if let Some(mut temp) = Pixmap::new(tw, th) {
            let local = Transform::from_translate(-b.min.x, -b.min.y);
            draw_shape_inner(
                &mut temp,
                shape,
                local,
                1.0,
                tiny_skia::BlendMode::SourceOver,
                pose,
            );
            crate::filter::apply(&mut temp, &shape.filters);
            let xf = t.pre_concat(Transform::from_translate(b.min.x, b.min.y));
            pm.draw_pixmap(
                0,
                0,
                temp.as_ref(),
                &PixmapPaint {
                    opacity: opacity.clamp(0.0, 1.0),
                    blend_mode: blend,
                    quality: tiny_skia::FilterQuality::Bilinear,
                },
                xf,
                None,
            );
        }
        return;
    }
    draw_shape_inner(pm, shape, t, opacity, blend, pose);
}

fn draw_shape_inner(
    pm: &mut Pixmap,
    shape: &Shape,
    t: Transform,
    opacity: f32,
    blend: tiny_skia::BlendMode,
    pose: Pose,
) {
    let Some(path) = shape.get_cached_path(96) else {
        return;
    };
    let shape_op = pose.opacity.unwrap_or(shape.opacity);
    let op = (opacity * shape_op).clamp(0.0, 1.0);
    let xf = if pose.is_identity() {
        t
    } else {
        t.pre_concat(pose.to_skia(shape.world_bbox().center()))
    };
    if !shape.style.fill.is_none() && shape.geom.is_closed() {
        let mut paint = fill_paint(&shape.style.fill, &shape.geom);
        paint.blend_mode = blend;
        paint.shader.apply_opacity(op);
        let rule = match &shape.geom {
            crate::geom::Geom::Poly { winding: true, .. } => FillRule::Winding,
            _ => FillRule::EvenOdd,
        };
        pm.fill_path(&path, &paint, rule, xf, None);
    }
    if let Some(stroke) = &shape.style.stroke
        && stroke.width > 0.0
    {
        let mut paint = Paint {
            anti_alias: true,
            blend_mode: blend,
            ..Paint::default()
        };
        let mut col = stroke.color.to_skia();
        col.set_alpha(col.alpha() * op);
        paint.set_color(col);
        let mut sk = SkStroke {
            width: stroke.width,
            line_cap: stroke.cap.to_skia(),
            line_join: stroke.join.to_skia(),
            ..SkStroke::default()
        };
        if let Some((on, off)) = stroke.dash {
            sk.dash = StrokeDash::new(vec![on, off], 0.0);
        }
        pm.stroke_path(&path, &paint, &sk, xf, None);
    }
}

fn fill_paint<'a>(fill: &Fill, geom: &Geom) -> Paint<'a> {
    let mut paint = Paint {
        anti_alias: true,
        ..Paint::default()
    };
    match fill {
        Fill::None => {}
        Fill::Solid(c) => paint.set_color(c.to_skia()),
        Fill::Linear { from, to, c0, c1 } => {
            let b = geom.bbox();
            let p0 = Point::from_xy(
                b.min.x + from[0] * b.width(),
                b.min.y + from[1] * b.height(),
            );
            let p1 = Point::from_xy(b.min.x + to[0] * b.width(), b.min.y + to[1] * b.height());
            if let Some(shader) = LinearGradient::new(
                p0,
                p1,
                vec![
                    GradientStop::new(0.0, c0.to_skia()),
                    GradientStop::new(1.0, c1.to_skia()),
                ],
                SpreadMode::Pad,
                Transform::identity(),
            ) {
                paint.shader = shader;
            } else {
                paint.set_color(c0.to_skia());
            }
        }
        Fill::Radial { c0, c1 } => {
            let b = geom.bbox();
            let c = b.center();
            let r = b.width().max(b.height()) * 0.5;
            if let Some(shader) = RadialGradient::new(
                Point::from_xy(c.x, c.y),
                0.0,
                Point::from_xy(c.x, c.y),
                r.max(1.0),
                vec![
                    GradientStop::new(0.0, c0.to_skia()),
                    GradientStop::new(1.0, c1.to_skia()),
                ],
                SpreadMode::Pad,
                Transform::identity(),
            ) {
                paint.shader = shader;
            } else {
                paint.set_color(c0.to_skia());
            }
        }
    }
    paint
}

fn is_paper_raster(layer: &Layer) -> bool {
    let LayerKind::Raster { pixels, size, .. } = &layer.kind else {
        return false;
    };
    if size.x.abs() > 0.5 || size.y.abs() > 0.5 {
        return false;
    }
    let Some(c) = pixels.is_uniform() else {
        return false;
    };
    let paper = c.r > 250 && c.g > 250 && c.b > 250 && c.a > 250;
    if !paper {
        return false;
    }
    true
}

fn paper_color(doc: &Document) -> Rgba {
    if doc.transparent {
        CHECKER_A
    } else {
        Rgba::WHITE
    }
}

fn draw_plates(pm: &mut Pixmap, doc: &Document, view: View) {
    if doc.artboards.is_empty() {
        let origin = view.to_screen(Pt::ZERO);
        let size = Pt::new(doc.width * view.scale, doc.height * view.scale);
        if doc.transparent {
            draw_checker(pm, origin, size);
        } else {
            fill_solid(pm, origin.x, origin.y, size.x, size.y, Rgba::WHITE);
        }
        return;
    }
    let c = paper_color(doc);
    for a in &doc.artboards {
        let corners = a.corners().map(|p| view.to_screen(p));
        fill_quad(pm, corners, c);
    }
}

fn draw_export_plates(pm: &mut Pixmap, doc: &Document, scale: f32) {
    if doc.transparent {
        return;
    }
    if doc.artboards.is_empty() {
        pm.fill(tiny_skia::Color::WHITE);
        return;
    }
    for a in &doc.artboards {
        let corners = a.corners().map(|p| p * scale);
        fill_quad(pm, corners, Rgba::WHITE);
    }
}

fn fill_quad(pm: &mut Pixmap, pts: [Pt; 4], c: Rgba) {
    let mut pb = PathBuilder::new();
    pb.move_to(pts[0].x, pts[0].y);
    pb.line_to(pts[1].x, pts[1].y);
    pb.line_to(pts[2].x, pts[2].y);
    pb.line_to(pts[3].x, pts[3].y);
    pb.close();
    let Some(path) = pb.finish() else {
        return;
    };
    let mut paint = Paint {
        anti_alias: true,
        ..Paint::default()
    };
    paint.set_color(c.to_skia());
    pm.fill_path(
        &path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );
}

fn fill_solid(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, c: Rgba) {
    let Some(rect) = tiny_skia::Rect::from_xywh(x, y, w, h) else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color(c.to_skia());
    pm.fill_rect(rect, &paint, Transform::identity(), None);
}

fn draw_checker(pm: &mut Pixmap, origin: Pt, size: Pt) {
    fill_solid(pm, origin.x, origin.y, size.x, size.y, CHECKER_A);
    let cell = 16.0f32;
    // Zoom can make the artboard millions of pixels wide. Only visit cells
    // intersecting the output, keeping work bounded by the viewport.
    let x0 = ((-origin.x / cell).floor() as i32).max(0);
    let y0 = ((-origin.y / cell).floor() as i32).max(0);
    let x1 = ((pm.width() as f32 - origin.x).min(size.x) / cell).ceil() as i32;
    let y1 = ((pm.height() as f32 - origin.y).min(size.y) / cell).ceil() as i32;
    for gy in y0..y1 {
        for gx in x0..x1 {
            if (gx + gy) % 2 == 0 {
                continue;
            }
            fill_solid(
                pm,
                origin.x + gx as f32 * cell,
                origin.y + gy as f32 * cell,
                cell.min(size.x - gx as f32 * cell),
                cell.min(size.y - gy as f32 * cell),
                CHECKER_B,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Cmd, Document, Shape, Style, apply};

    #[test]
    fn checker_clips_large_artboards_without_changing_pattern() {
        let mut near = Pixmap::new(64, 64).unwrap();
        let mut far = near.clone();
        draw_checker(&mut near, Pt::new(-29.0, -25.0), Pt::new(128.0, 128.0));
        draw_checker(
            &mut far,
            Pt::new(-999_997.0, -999_993.0),
            Pt::new(2_000_000.0, 2_000_000.0),
        );
        assert_eq!(near.data(), far.data());
    }

    #[test]
    fn gradient_fill_respects_shape_and_layer_opacity() {
        let mut shape = Shape::new(
            Geom::Rect {
                origin: Pt::ZERO,
                size: Pt::new(20.0, 20.0),
                radius: 0.0,
            },
            Style {
                fill: Fill::Linear {
                    from: [0.0, 0.0],
                    to: [1.0, 0.0],
                    c0: Rgba::rgb(255, 0, 0),
                    c1: Rgba::rgb(0, 0, 255),
                },
                stroke: None,
            },
        );
        shape.opacity = 0.5;
        let mut pm = Pixmap::new(20, 20).unwrap();
        draw_shape(
            &mut pm,
            &shape,
            Transform::identity(),
            0.5,
            tiny_skia::BlendMode::SourceOver,
            Pose::identity(),
        );
        assert!((pm.pixel(10, 10).unwrap().alpha() as i32 - 64).abs() <= 1);
    }
    use crate::geom::Geom;

    #[test]
    fn posed_draw_moves_pixels() {
        use crate::document::{Cmd, Shape, Style, apply};
        use crate::geom::Geom;
        use crate::motion::{Ease, Prop};
        let mut doc = Document::new("t", 80.0, 80.0, 72.0);
        let shape = Shape::new(
            Geom::Rect {
                origin: crate::geom::Pt::new(10.0, 10.0),
                size: crate::geom::Pt::new(20.0, 20.0),
                radius: 0.0,
            },
            Style::default(),
        );
        let id = shape.id;
        apply(&mut doc, &Cmd::AddShape { layer: 1, shape });
        doc.motion.set_key(id, Prop::X, 0.0, 0.0, Ease::Linear);
        doc.motion.set_key(id, Prop::X, 1.0, 30.0, Ease::Linear);
        let rest = render_view(&doc, View::default(), 80, 80, Draft::none()).unwrap();
        let posed = render_view_posed(
            &doc,
            View::default(),
            80,
            80,
            Draft::none(),
            Some(1.0),
            None,
        )
        .unwrap();
        assert_ne!(rest.data(), posed.data(), "a keyed translate must redraw");
    }

    #[test]
    fn artboard_plate_follows_origin() {
        let mut doc = Document::new("t", 80.0, 80.0, 72.0);
        doc.width = 200.0;
        doc.artboards[0].origin = Pt::new(40.0, 0.0);
        doc.artboards[0].size = Pt::new(40.0, 80.0);
        let pm = render_view(&doc, View::default(), 80, 80, Draft::none()).unwrap();
        let left = pm.pixel(10, 40).unwrap();
        let right = pm.pixel(60, 40).unwrap();
        let bg = canvas_bg();
        assert!(
            (left.red() as i32 - bg.r as i32).abs() < 8,
            "left of the board must be pasteboard, got {} {} {}",
            left.red(),
            left.green(),
            left.blue()
        );
        assert!(
            right.red() > 240 && right.green() > 240 && right.blue() > 240,
            "plate must sit on the artboard, got {} {} {}",
            right.red(),
            right.green(),
            right.blue()
        );
    }

    #[test]
    fn export_images_preserve_dimensions_and_transparency() {
        let mut doc = Document::new("t", 64.0, 48.0, 72.0);
        apply(
            &mut doc,
            &Cmd::AddShape {
                layer: 1,
                shape: Shape::new(
                    Geom::Rect {
                        origin: Pt::new(8.0, 8.0),
                        size: Pt::new(24.0, 16.0),
                        radius: 2.0,
                    },
                    Style::default(),
                ),
            },
        );
        let bytes = export_png(&doc, 1).unwrap();
        assert!(bytes.starts_with(b"\x89PNG"));
        let img = image::load_from_memory(&bytes).unwrap();
        assert_eq!(img.width(), 64);
        assert_eq!(img.height(), 48);
        let jpeg = image::load_from_memory(&export_jpeg(&doc, 2, 90).unwrap()).unwrap();
        assert_eq!((jpeg.width(), jpeg.height()), (128, 96));
        doc.transparent = true;
        doc.layers[1].kind.shapes_mut().unwrap()[0].opacity = 0.5;
        let png = image::load_from_memory(&export_png(&doc, 1).unwrap())
            .unwrap()
            .to_rgba8();
        assert_eq!(
            png.get_pixel(0, 0).0[3],
            0,
            "the editor checkerboard is not artwork"
        );
        assert!((png.get_pixel(16, 16).0[3] as i32 - 128).abs() <= 1);
        let jpeg = image::load_from_memory(&export_jpeg(&doc, 1, 100).unwrap())
            .unwrap()
            .to_rgb8();
        assert!(jpeg.get_pixel(0, 0).0.iter().all(|&channel| channel > 245));
    }

    #[test]
    fn pointer_and_overlay_share_canvas_space() {
        let v = View {
            scale: 2.0,
            offset: Pt::new(10.0, 20.0),
        };
        let canvas_origin = Pt::new(80.0, 48.0);
        let world = Pt::new(15.0, 25.0);
        let pix = v.to_screen(world);
        assert!((pix.x - 40.0).abs() < 1e-4 && (pix.y - 70.0).abs() < 1e-4);
        let win = v.world_to_window(canvas_origin, world);
        assert!((win.x - 120.0).abs() < 1e-4 && (win.y - 118.0).abs() < 1e-4);
        let back = v.pointer_to_world(canvas_origin, win);
        assert!((back.x - world.x).abs() < 1e-4 && (back.y - world.y).abs() < 1e-4);
    }

    #[test]
    fn fit_does_not_bake_in_a_window_origin() {
        let mut v = View::default();
        v.fit(
            Pt::new(200.0, 100.0),
            crate::geom::Bounds {
                min: Pt::ZERO,
                max: Pt::new(400.0, 300.0),
            },
        );
        let o = v.to_screen(Pt::ZERO);
        assert!(o.x >= 0.0 && o.x < 400.0, "origin x {o:?} left the canvas");
        assert!(o.y >= 0.0 && o.y < 300.0, "origin y {o:?} left the canvas");
    }

    #[test]
    fn zoom_to_fits_the_box_in_the_viewport() {
        let mut v = View::default();
        let world = crate::geom::Bounds {
            min: Pt::new(100.0, 50.0),
            max: Pt::new(200.0, 150.0),
        };
        let vp = crate::geom::Bounds {
            min: Pt::ZERO,
            max: Pt::new(400.0, 400.0),
        };
        v.zoom_to(world, vp);
        assert!((v.scale - 4.0).abs() < 1e-4, "scale {}", v.scale);
        let c = v.to_screen(world.center());
        assert!((c.x - 200.0).abs() < 1e-3 && (c.y - 200.0).abs() < 1e-3);
        let tl = v.to_screen(world.min);
        let br = v.to_screen(world.max);
        assert!((br.x - tl.x - 400.0).abs() < 1e-3);
        assert!((br.y - tl.y - 400.0).abs() < 1e-3);
    }

    #[test]
    fn pinch_keeps_the_point_under_the_cursor() {
        let mut v = View {
            scale: 1.0,
            offset: crate::geom::Pt::new(10.0, 20.0),
        };
        let screen = crate::geom::Pt::new(80.0, 40.0);
        let world = v.to_world(screen);
        // Wayland pinch is an absolute scale from begin; we feed the ratio.
        v.zoom_at(screen, 1.25);
        v.zoom_at(screen, 0.8);
        let back = v.to_world(screen);
        assert!((back.x - world.x).abs() < 1e-4 && (back.y - world.y).abs() < 1e-4);
        assert!((v.scale - 1.0).abs() < 1e-4);
    }
}
