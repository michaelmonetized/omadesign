//! One geometry, two outputs: the live canvas and PNG export.

use crate::color::Rgba;
use crate::document::{Document, Fill, Layer, LayerKind, Shape};
use crate::geom::{Geom, Pt};
use tiny_skia::{
    FillRule, GradientStop, LineCap, LineJoin, LinearGradient, Paint, Path, PathBuilder, Pixmap,
    PixmapPaint, Point, RadialGradient, Shader, SpreadMode, Stroke as SkStroke, StrokeDash,
    Transform,
};

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

    pub fn zoom_at(&mut self, screen: Pt, factor: f32) {
        let world = self.to_world(screen);
        self.scale = (self.scale * factor).clamp(0.02, 64.0);
        self.offset = screen - world * self.scale;
    }

    pub fn fit(&mut self, doc: Pt, viewport: crate::geom::Bounds) {
        if viewport.width() < 1.0 || viewport.height() < 1.0 {
            return;
        }
        self.scale = ((viewport.width() / doc.x).min(viewport.height() / doc.y) * 0.90)
            .clamp(0.02, 64.0);
        let scaled = doc * self.scale;
        self.offset = viewport.center() - scaled * 0.5;
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
const CANVAS_BG: Rgba = Rgba {
    r: 0x22,
    g: 0x26,
    b: 0x2C,
    a: 255,
};

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
    let mut pm = Pixmap::new(screen_w, screen_h)?;
    fill_solid(&mut pm, 0.0, 0.0, screen_w as f32, screen_h as f32, CANVAS_BG);

    let origin = view.to_screen(Pt::ZERO);
    let size = Pt::new(doc.width * view.scale, doc.height * view.scale);
    draw_checker(&mut pm, origin, size);
    stroke_rect(
        &mut pm,
        origin,
        size,
        1.0,
        Rgba::rgb(0x55, 0x5A, 0x63),
    );

    let t = view.transform();
    for (li, layer) in doc.layers.iter().enumerate() {
        if !layer.visible || layer.opacity <= 0.0 {
            continue;
        }
        let brush = draft
            .brush
            .and_then(|(bl, buf, flow)| (bl == li).then_some((buf, flow)));
        draw_layer(&mut pm, layer, t, brush, draft.preview);
    }
    Some(pm)
}

pub fn export_png(doc: &Document, scale: u32) -> Result<Vec<u8>, String> {
    let s = scale.clamp(1, 8);
    let w = (doc.width * s as f32).round().max(1.0) as u32;
    let h = (doc.height * s as f32).round().max(1.0) as u32;
    let mut pm = Pixmap::new(w, h).ok_or("could not allocate export pixmap")?;
    draw_checker(
        &mut pm,
        Pt::ZERO,
        Pt::new(w as f32, h as f32),
    );
    let t = Transform::from_scale(s as f32, s as f32);
    for layer in &doc.layers {
        if !layer.visible || layer.opacity <= 0.0 {
            continue;
        }
        draw_layer(&mut pm, layer, t, None, None);
    }
    pm.encode_png().map_err(|e| e.to_string())
}

pub fn export_jpeg(doc: &Document, scale: u32, quality: u8) -> Result<Vec<u8>, String> {
    let png = export_png(doc, scale)?;
    let img = image::load_from_memory(&png).map_err(|e| e.to_string())?;
    let rgb = img.to_rgb8();
    let mut buf = Vec::new();
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality.max(1));
    enc.encode(
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
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
) {
    if layer.mask.is_some() {
        let Some(mut temp) = Pixmap::new(pm.width(), pm.height()) else {
            return;
        };
        draw_content(&mut temp, layer, t, brush, preview, 1.0, tiny_skia::BlendMode::SourceOver);
        if let Some(mask) = &layer.mask {
            if let Some(mask_pm) = mask.to_pixmap() {
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
) {
    match &layer.kind {
        LayerKind::Vector { shapes } => {
            for s in shapes {
                draw_shape(pm, s, t, opacity, blend);
            }
            if let Some(p) = preview {
                draw_shape(pm, p, t, opacity * 0.85, blend);
            }
        }
        LayerKind::Raster { pixels } => {
            if let Some(src) = pixels.to_pixmap() {
                pm.draw_pixmap(
                    0,
                    0,
                    src.as_ref(),
                    &PixmapPaint {
                        opacity: opacity.clamp(0.0, 1.0),
                        blend_mode: blend,
                        quality: tiny_skia::FilterQuality::Bilinear,
                        ..Default::default()
                    },
                    t,
                    None,
                );
            }
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

fn draw_shape(pm: &mut Pixmap, shape: &Shape, t: Transform, opacity: f32, blend: tiny_skia::BlendMode) {
    let Some(path) = shape_path(shape) else {
        return;
    };
    let op = (opacity * shape.opacity).clamp(0.0, 1.0);
    if !shape.style.fill.is_none() && shape.geom.is_closed() {
        let mut paint = fill_paint(&shape.style.fill, &shape.geom);
        paint.blend_mode = blend;
        let mut c = paint_color(&mut paint);
        if let Some(col) = c.as_mut() {
            col.set_alpha(col.alpha() * op);
            paint.set_color(*col);
        }
        pm.fill_path(&path, &paint, FillRule::EvenOdd, t, None);
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
        pm.stroke_path(&path, &paint, &sk, t, None);
    }
}

fn paint_color(p: &mut Paint) -> Option<tiny_skia::Color> {
    if matches!(p.shader, Shader::SolidColor(_)) {
        if let Shader::SolidColor(c) = p.shader {
            return Some(c);
        }
    }
    None
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

fn shape_path(shape: &Shape) -> Option<Path> {
    let mut pb = PathBuilder::new();
    let mut any = false;
    for contour in shape.world_contours(96) {
        if contour.len() < 2 {
            continue;
        }
        pb.move_to(contour[0].x, contour[0].y);
        for p in contour.iter().skip(1) {
            pb.line_to(p.x, p.y);
        }
        if shape.geom.is_closed() {
            pb.close();
        }
        any = true;
    }
    if any { pb.finish() } else { None }
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
    let cols = (size.x / cell).ceil() as i32;
    let rows = (size.y / cell).ceil() as i32;
    for gy in 0..rows {
        for gx in 0..cols {
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

fn stroke_rect(pm: &mut Pixmap, origin: Pt, size: Pt, width: f32, c: Rgba) {
    let Some(rect) = tiny_skia::Rect::from_xywh(origin.x, origin.y, size.x.max(1.0), size.y.max(1.0))
    else {
        return;
    };
    let path = PathBuilder::from_rect(rect);
    let mut paint = Paint {
        anti_alias: true,
        ..Paint::default()
    };
    paint.set_color(c.to_skia());
    pm.stroke_path(
        &path,
        &paint,
        &SkStroke {
            width,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            ..SkStroke::default()
        },
        Transform::identity(),
        None,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Cmd, Document, Shape, Style, apply};
    use crate::geom::Geom;

    #[test]
    fn export_png_is_valid() {
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
    }
}
