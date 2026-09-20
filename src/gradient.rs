//! Editable multi-stop gradients shared by fills, strokes and portable exports.
use crate::color::Rgba;
use crate::document::Shape;
use crate::geom::{Bounds, Geom, Pt};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::VecDeque, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GradientKind {
    Linear,
    Radial,
    Shape,
    Conic,
}
impl GradientKind {
    pub const ALL: [Self; 4] = [Self::Linear, Self::Radial, Self::Shape, Self::Conic];
    pub fn name(self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::Radial => "Radial",
            Self::Shape => "Shape",
            Self::Conic => "Conic",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    pub offset: f32,
    pub color: Rgba,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Gradient {
    pub kind: GradientKind,
    /// Endpoints in object bounding-box coordinates. For radial/conic, `from` is the centre.
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub stops: Vec<GradientStop>,
}
impl Gradient {
    pub fn new(kind: GradientKind, first: Rgba, last: Rgba) -> Self {
        Self {
            kind,
            from: if kind == GradientKind::Linear {
                [0., 0.5]
            } else {
                [0.5, 0.5]
            },
            to: [1., 0.5],
            stops: vec![
                GradientStop {
                    offset: 0.,
                    color: first,
                },
                GradientStop {
                    offset: 1.,
                    color: last,
                },
            ],
        }
    }
    pub fn normalize(&mut self) {
        self.stops.retain(|s| s.offset.is_finite());
        for stop in &mut self.stops {
            stop.offset = stop.offset.clamp(0., 1.);
        }
        self.stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        if self.stops.is_empty() {
            self.stops.push(GradientStop {
                offset: 0.,
                color: Rgba::BLACK,
            });
        }
        if self.stops.len() == 1 {
            self.stops.push(GradientStop {
                offset: 1.,
                color: self.stops[0].color,
            });
        }
    }
    pub fn sample(&self, position: f32) -> Rgba {
        let Some(first) = self.stops.first() else {
            return Rgba::TRANSPARENT;
        };
        let position = position.clamp(0., 1.);
        if position < first.offset {
            return first.color;
        }
        for pair in self.stops.windows(2) {
            if position < pair[1].offset {
                let t = ((position - pair[0].offset) / (pair[1].offset - pair[0].offset).max(1e-9))
                    .clamp(0., 1.);
                let a = pair[0].color;
                let b = pair[1].color;
                let alpha = a.a as f32 * (1. - t) + b.a as f32 * t;
                let channel = |x: u8, y: u8| {
                    if alpha <= 0. {
                        0
                    } else {
                        ((x as f32 * a.a as f32 * (1. - t) + y as f32 * b.a as f32 * t) / alpha)
                            .round() as u8
                    }
                };
                return Rgba::new(
                    channel(a.r, b.r),
                    channel(a.g, b.g),
                    channel(a.b, b.b),
                    alpha.round() as u8,
                );
            }
        }
        self.stops.last().unwrap().color
    }
    pub fn endpoints(&self, bounds: Bounds) -> (Pt, Pt) {
        let point = |v: [f32; 2]| {
            bounds.min
                + Pt::new(
                    v[0] * bounds.width().max(1e-3),
                    v[1] * bounds.height().max(1e-3),
                )
        };
        (point(self.from), point(self.to))
    }
    pub fn angle(&self, bounds: Bounds) -> f32 {
        let (a, b) = self.endpoints(bounds);
        (b.y - a.y).atan2(b.x - a.x).to_degrees().rem_euclid(360.)
    }
    pub fn set_angle(&mut self, degrees: f32, bounds: Bounds) {
        let (a, b) = self.endpoints(bounds);
        let radius = (b - a).length().max(1.);
        let d = Pt::new(degrees.to_radians().cos(), degrees.to_radians().sin()) * radius;
        if self.kind == GradientKind::Linear {
            let center = (a + b) * 0.5;
            self.from = [
                (center.x - d.x * 0.5 - bounds.min.x) / bounds.width().max(1e-3),
                (center.y - d.y * 0.5 - bounds.min.y) / bounds.height().max(1e-3),
            ];
            self.to = [
                (center.x + d.x * 0.5 - bounds.min.x) / bounds.width().max(1e-3),
                (center.y + d.y * 0.5 - bounds.min.y) / bounds.height().max(1e-3),
            ];
        } else {
            self.to = [
                (a.x + d.x - bounds.min.x) / bounds.width().max(1e-3),
                (a.y + d.y - bounds.min.y) / bounds.height().max(1e-3),
            ];
        }
    }
    pub fn position(&self, bounds: Bounds, point: Pt) -> f32 {
        let (a, b) = self.endpoints(bounds);
        let d = b - a;
        let p = point - a;
        match self.kind {
            GradientKind::Linear => (p.x * d.x + p.y * d.y) / (d.x * d.x + d.y * d.y).max(1e-9),
            GradientKind::Radial | GradientKind::Shape => p.length() / d.length().max(1e-6),
            GradientKind::Conic => {
                ((p.y.atan2(p.x) - d.y.atan2(d.x)) / std::f32::consts::TAU).rem_euclid(1.)
            }
        }
    }
    pub fn skia(&self, bounds: Bounds) -> tiny_skia::Shader<'static> {
        let mut normalized = self.clone();
        normalized.normalize();
        let stops = normalized
            .stops
            .iter()
            .map(|s| tiny_skia::GradientStop::new(s.offset, s.color.to_skia()))
            .collect();
        let (a, b) = self.endpoints(bounds);
        let a = tiny_skia::Point::from_xy(a.x, a.y);
        let b = tiny_skia::Point::from_xy(b.x, b.y);
        let shader = match self.kind {
            GradientKind::Linear => tiny_skia::LinearGradient::new(
                a,
                b,
                stops,
                tiny_skia::SpreadMode::Pad,
                tiny_skia::Transform::identity(),
            ),
            _ => tiny_skia::RadialGradient::new(
                a,
                0.,
                a,
                ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt().max(1e-3),
                stops,
                tiny_skia::SpreadMode::Pad,
                tiny_skia::Transform::identity(),
            ),
        };
        shader.unwrap_or(tiny_skia::Shader::SolidColor(self.sample(0.).to_skia()))
    }
}

/// Shape gradients use distance to the actual silhouette (including holes), not its bounding box.
/// A two-pass distance transform keeps this linear in pixel count even for complex paths.
type Texture = (Arc<tiny_skia::Pixmap>, Bounds);
struct TextureEntry {
    gradient: Gradient,
    geom: Geom,
    corners: [f32; 4],
    w: u32,
    h: u32,
    tile: Bounds,
    texture: Texture,
}
thread_local! { static TEXTURES: RefCell<VecDeque<TextureEntry>> = const { RefCell::new(VecDeque::new()) }; }
pub fn texture(gradient: &Gradient, shape: &Shape, padding: f32, scale: f32) -> Option<Texture> {
    if !matches!(gradient.kind, GradientKind::Conic | GradientKind::Shape) {
        return None;
    }
    let geom = &shape.geom;
    let bounds = geom.bbox();
    let tile = bounds.inflate(padding.max(1.));
    // Resolution follows the actual canvas/export transform, with bounded memory.
    let longest = tile.width().max(tile.height()).max(1.);
    let resolution = ((longest * scale.max(0.1)).ceil().clamp(16., 4096.) as u32)
        .next_power_of_two()
        .min(4096);
    let scale = resolution as f32 / longest;
    let w = (tile.width() * scale).ceil().max(2.) as u32;
    let h = (tile.height() * scale).ceil().max(2.) as u32;
    if let Some(cached) = TEXTURES.with(|entries| {
        let mut entries = entries.borrow_mut();
        let index = entries.iter().position(|e| {
            e.gradient == *gradient
                && e.geom == *geom
                && e.corners == shape.corners
                && e.w == w
                && e.h == h
                && e.tile == tile
        })?;
        let entry = entries.remove(index)?;
        let texture = entry.texture.clone();
        entries.push_front(entry);
        Some(texture)
    }) {
        return Some(cached);
    }
    let mut pm = tiny_skia::Pixmap::new(w, h)?;
    let mut field = if gradient.kind == GradientKind::Shape {
        vec![0.; (w * h) as usize]
    } else {
        Vec::new()
    };
    if gradient.kind == GradientKind::Shape {
        let mut mask = tiny_skia::Mask::new(w, h)?;
        if let Some(path) = {
            let mut local = shape.clone();
            local.rotation = 0.;
            local.get_cached_path(96)
        } {
            let xf = tiny_skia::Transform::from_row(
                w as f32 / tile.width(),
                0.,
                0.,
                h as f32 / tile.height(),
                -tile.min.x * w as f32 / tile.width(),
                -tile.min.y * h as f32 / tile.height(),
            );
            let rule = if matches!(geom, Geom::Poly { winding: true, .. }) {
                tiny_skia::FillRule::Winding
            } else {
                tiny_skia::FillRule::EvenOdd
            };
            mask.fill_path(&path, rule, true, xf);
        }
        for (index, (distance, &value)) in field.iter_mut().zip(mask.data()).enumerate() {
            let border = index % w as usize == 0
                || index % w as usize == w as usize - 1
                || index < w as usize
                || index >= (h as usize - 1) * w as usize;
            *distance = if value > 127 && !border { 1e6_f32 } else { 0. };
        }
        let stride = w as usize;
        for y in 1..h as usize {
            for x in 1..w as usize - 1 {
                let i = y * stride + x;
                field[i] = field[i]
                    .min(field[i - 1] + 1.)
                    .min(field[i - stride] + 1.)
                    .min(field[i - stride - 1] + std::f32::consts::SQRT_2)
                    .min(field[i - stride + 1] + std::f32::consts::SQRT_2);
            }
        }
        for y in (0..h as usize - 1).rev() {
            for x in (1..w as usize - 1).rev() {
                let i = y * stride + x;
                field[i] = field[i]
                    .min(field[i + 1] + 1.)
                    .min(field[i + stride] + 1.)
                    .min(field[i + stride - 1] + std::f32::consts::SQRT_2)
                    .min(field[i + stride + 1] + std::f32::consts::SQRT_2);
            }
        }
        let maximum = field.iter().copied().fold(0., f32::max).max(1.);
        for distance in &mut field {
            *distance = 1. - *distance / maximum;
        }
    }
    let mut normalized = gradient.clone();
    normalized.normalize();
    for (i, pixel) in pm.pixels_mut().iter_mut().enumerate() {
        let p = Pt::new(
            tile.min.x + ((i % w as usize) as f32 + 0.5) / w as f32 * tile.width(),
            tile.min.y + ((i / w as usize) as f32 + 0.5) / h as f32 * tile.height(),
        );
        let c = normalized.sample(if gradient.kind == GradientKind::Shape {
            field[i]
        } else {
            gradient.position(bounds, p)
        });
        *pixel = tiny_skia::ColorU8::from_rgba(c.r, c.g, c.b, c.a).premultiply();
    }
    let texture = (Arc::new(pm), tile);
    TEXTURES.with(|entries| {
        let mut entries = entries.borrow_mut();
        entries.push_front(TextureEntry {
            gradient: gradient.clone(),
            geom: geom.clone(),
            corners: shape.corners,
            w,
            h,
            tile,
            texture: texture.clone(),
        });
        let mut bytes = 0;
        entries.retain(|entry| {
            bytes += entry.w as usize * entry.h as usize * 4;
            bytes <= 64 * 1024 * 1024
        });
        entries.truncate(24);
    });
    Some(texture)
}

pub fn texture_shader<'a>(texture: &'a (Arc<tiny_skia::Pixmap>, Bounds)) -> tiny_skia::Shader<'a> {
    let (pixels, b) = texture;
    tiny_skia::Pattern::new(
        pixels.as_ref().as_ref(),
        tiny_skia::SpreadMode::Pad,
        tiny_skia::FilterQuality::Bilinear,
        1.,
        tiny_skia::Transform::from_row(
            b.width() / pixels.width() as f32,
            0.,
            0.,
            b.height() / pixels.height() as f32,
            b.min.x,
            b.min.y,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multi_stop_sampling_preserves_alpha_and_hard_edges() {
        let mut g = Gradient::new(GradientKind::Linear, Rgba::BLACK, Rgba::WHITE);
        g.stops.insert(
            1,
            GradientStop {
                offset: 0.5,
                color: Rgba::new(255, 0, 0, 128),
            },
        );
        assert_eq!(g.sample(0.5), Rgba::new(255, 0, 0, 128));
        assert_eq!(g.sample(1.), Rgba::WHITE);
        let c = g.sample(0.75);
        assert_eq!(c.a, 192);
        assert!(c.r > 250 && c.g > 160);
        g.stops.insert(
            2,
            GradientStop {
                offset: 0.5,
                color: Rgba::BLACK,
            },
        );
        assert_eq!(g.sample(0.5), Rgba::BLACK);
    }
    #[test]
    fn gradient_roundtrip_and_legacy_stroke_default() {
        let g = Gradient::new(GradientKind::Conic, Rgba::TRANSPARENT, Rgba::WHITE);
        let fill = crate::document::Fill::Gradient(g.clone());
        assert_eq!(
            serde_json::from_str::<crate::document::Fill>(&serde_json::to_string(&fill).unwrap())
                .unwrap(),
            fill
        );
        let stroke:crate::document::Stroke=serde_json::from_str(r#"{"color":{"r":1,"g":2,"b":3,"a":255},"width":2.0,"cap":"Round","join":"Round","dash":null}"#).unwrap();
        assert!(stroke.gradient.is_none());
    }
    #[test]
    fn angle_and_conic_quadrants() {
        let b = Bounds::from_min_size(Pt::ZERO, Pt::new(200., 100.));
        let mut g = Gradient::new(GradientKind::Linear, Rgba::BLACK, Rgba::WHITE);
        g.set_angle(90., b);
        assert!((g.angle(b) - 90.).abs() < 0.01);
        g.kind = GradientKind::Conic;
        g.from = [0.5, 0.5];
        g.to = [1., 0.5];
        assert!((g.position(b, Pt::new(100., 100.)) - 0.25).abs() < 0.01);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::document::{Document, Fill, Shape, Stroke, Style};
    fn gradient(kind: GradientKind) -> Gradient {
        let mut g = Gradient::new(
            kind,
            Rgba::new(235, 40, 10, 200),
            Rgba::new(10, 50, 230, 255),
        );
        g.stops.insert(
            1,
            GradientStop {
                offset: 0.37,
                color: Rgba::new(30, 220, 90, 128),
            },
        );
        g
    }
    #[test]
    fn all_gradients_and_gradient_strokes_match_rendered_svg() {
        for kind in GradientKind::ALL {
            let mut doc = Document::new("Gradient export", 128., 112., 72.);
            doc.transparent = true;
            let mut shape = Shape::new(
                Geom::Rect {
                    origin: Pt::new(20., 18.),
                    size: Pt::new(88., 76.),
                    radius: 7.,
                },
                Style {
                    fill: Fill::Gradient(gradient(kind)),
                    stroke: Some(Stroke {
                        gradient: Some(gradient(kind)),
                        width: 8.,
                        ..Stroke::default()
                    }),
                },
            );
            shape.opacity = 0.8;
            doc.layers[1].kind.shapes_mut().unwrap().push(shape);
            let actual = crate::compositor::render_export(&doc, 1).unwrap();
            let source = crate::svg::export(&doc).unwrap();
            let tree = usvg::Tree::from_str(&source, &usvg::Options::default()).unwrap();
            let mut expected = tiny_skia::Pixmap::new(actual.width(), actual.height()).unwrap();
            resvg::render(
                &tree,
                tiny_skia::Transform::identity(),
                &mut expected.as_mut(),
            );
            let mean = actual
                .data()
                .iter()
                .zip(expected.data())
                .map(|(a, b)| a.abs_diff(*b) as f64)
                .sum::<f64>()
                / actual.data().len() as f64;
            assert!(mean < 2.0, "{kind:?} SVG pixels diverged: mean {mean}");
            assert!(
                source.contains("stroke=\"url(#"),
                "gradient stroke must export as gradient paint"
            );
        }
    }
    #[test]
    fn shape_texture_cache_keeps_corners_and_uses_render_scale() {
        let mut shape = Shape::new(
            Geom::Rect {
                origin: Pt::ZERO,
                size: Pt::new(90., 70.),
                radius: 0.,
            },
            Style::default(),
        );
        let g = gradient(GradientKind::Shape);
        let first = texture(&g, &shape, 0., 1.).unwrap();
        let second = texture(&g, &shape, 0., 1.).unwrap();
        assert!(Arc::ptr_eq(&first.0, &second.0));
        shape.corners = [30., 0., 0., 0.];
        let corners = texture(&g, &shape, 0., 1.).unwrap();
        assert!(!Arc::ptr_eq(&first.0, &corners.0));
        assert_ne!(first.0.data(), corners.0.data());
        let large = texture(&g, &shape, 0., 4.).unwrap();
        assert!(large.0.width() > corners.0.width());
    }
    #[test]
    fn swapping_fill_and_stroke_keeps_all_stops_and_undo() {
        let mut studio = crate::app::Studio::new();
        studio.doc = Document::new("Gradient swap", 100., 100., 72.);
        let style = Style {
            fill: Fill::Gradient(gradient(GradientKind::Conic)),
            stroke: Some(Stroke {
                gradient: Some(gradient(GradientKind::Radial)),
                ..Stroke::default()
            }),
        };
        let shape = Shape::new(
            Geom::Rect {
                origin: Pt::ZERO,
                size: Pt::new(30., 30.),
                radius: 0.,
            },
            style.clone(),
        );
        let id = shape.id;
        studio.doc.layers[1].kind.shapes_mut().unwrap().push(shape);
        studio.selection = vec![(1, id)];
        studio.style = style.clone();
        studio.swap_fill_stroke();
        let swapped = &studio.doc.find_shape(1, id).unwrap().style;
        assert_eq!(
            swapped.fill.gradient(),
            style.stroke.as_ref().unwrap().gradient
        );
        assert_eq!(
            swapped.stroke.as_ref().unwrap().gradient,
            style.fill.gradient()
        );
        studio.undo();
        assert_eq!(studio.doc.find_shape(1, id).unwrap().style, style);
    }
}
