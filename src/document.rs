use eframe::egui::{pos2, Color32, Pos2, Rect, Stroke, Vec2};
use std::sync::atomic::{AtomicU64, Ordering};

pub type ShapeId = u64;

static SHAPE_ID: AtomicU64 = AtomicU64::new(1);

pub fn next_shape_id() -> ShapeId {
    SHAPE_ID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Anchor {
    pub pt: Pos2,
    pub h_in: Vec2,
    pub h_out: Vec2,
}

impl Anchor {
    pub fn corner(pt: Pos2) -> Self {
        Self {
            pt,
            h_in: Vec2::ZERO,
            h_out: Vec2::ZERO,
        }
    }

    pub fn smooth(pt: Pos2, drag: Vec2) -> Self {
        Self {
            pt,
            h_in: -drag,
            h_out: drag,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Geometry {
    Rect { origin: Pos2, size: Vec2 },
    Ellipse { center: Pos2, radii: Vec2 },
    Polyline { points: Vec<Pos2>, closed: bool },
    Path {
        anchors: Vec<Anchor>,
        closed: bool,
    },
    Text {
        subpaths: Vec<Vec<Pos2>>,
        content: String,
        px: f32,
    },
    MultiPolygon { contours: Vec<Vec<Pos2>> },
}

const CUBIC_STEPS: usize = 16;

fn flatten_cubic(p0: Pos2, c1: Pos2, c2: Pos2, p1: Pos2, out: &mut Vec<Pos2>) {
    for i in 1..=CUBIC_STEPS {
        let t = i as f32 / CUBIC_STEPS as f32;
        let a = (1.0 - t) * (1.0 - t) * (1.0 - t);
        let b = 3.0 * (1.0 - t) * (1.0 - t) * t;
        let d = 3.0 * (1.0 - t) * t * t;
        let e = t * t * t;
        out.push(pos2(
            a * p0.x + b * c1.x + d * c2.x + e * p1.x,
            a * p0.y + b * c1.y + d * c2.y + e * p1.y,
        ));
    }
}

impl Geometry {
    pub fn contours(&self, segments: usize) -> Vec<Vec<Pos2>> {
        match self {
            Geometry::Rect { origin, size } => vec![vec![
                pos2(origin.x, origin.y),
                pos2(origin.x + size.x, origin.y),
                pos2(origin.x + size.x, origin.y + size.y),
                pos2(origin.x, origin.y + size.y),
            ]],
            Geometry::Ellipse { center, radii } => vec![(0..segments.max(3))
                .map(|i| {
                    let a = std::f32::consts::TAU * i as f32 / segments as f32;
                    pos2(center.x + radii.x * a.cos(), center.y + radii.y * a.sin())
                })
                .collect()],
            Geometry::Polyline { points, .. } => vec![points.clone()],
            Geometry::Path { anchors, closed } => {
                if anchors.len() < 2 {
                    return vec![];
                }
                let mut pts = vec![anchors[0].pt];
                let segs = if *closed { anchors.len() } else { anchors.len() - 1 };
                for i in 0..segs {
                    let a = &anchors[i % anchors.len()];
                    let b = &anchors[(i + 1) % anchors.len()];
                    flatten_cubic(a.pt, a.pt + a.h_out, b.pt + b.h_in, b.pt, &mut pts);
                }
                vec![pts]
            }
            Geometry::Text { subpaths, .. } => subpaths.clone(),
            Geometry::MultiPolygon { contours } => contours.clone(),
        }
    }

    pub fn translate(&mut self, d: Vec2) {
        match self {
            Geometry::Rect { origin, .. } => {
                *origin += d;
            }
            Geometry::Ellipse { center, .. } => {
                *center += d;
            }
            Geometry::Polyline { points, .. } => {
                for p in points.iter_mut() {
                    *p += d;
                }
            }
            Geometry::Path { anchors, .. } => {
                for a in anchors.iter_mut() {
                    a.pt += d;
                }
            }
            Geometry::Text { subpaths, .. } => {
                for sp in subpaths.iter_mut() {
                    for p in sp.iter_mut() {
                        *p += d;
                    }
                }
            }
            Geometry::MultiPolygon { contours } => {
                for c in contours.iter_mut() {
                    for p in c.iter_mut() {
                        *p += d;
                    }
                }
            }
        };
    }

    pub fn bbox(&self) -> Rect {
        let first = |pts: &[Pos2]| Rect::from_min_max(pts[0], pts[0]);
        match self {
            Geometry::Rect { origin, size } => Rect::from_min_size(*origin, *size),
            Geometry::Ellipse { center, radii } => Rect::from_min_max(
                pos2(center.x - radii.x, center.y - radii.y),
                pos2(center.x + radii.x, center.y + radii.y),
            ),
            Geometry::Polyline { points, .. } => {
                let mut r = first(points);
                for p in points.iter().skip(1) {
                    r.extend_with(*p);
                }
                r
            }
            Geometry::Path { anchors, .. } => {
                let mut r = Rect::from_min_max(anchors[0].pt, anchors[0].pt);
                for a in anchors {
                    r.extend_with(a.pt);
                    r.extend_with(a.pt + a.h_out);
                    r.extend_with(a.pt + a.h_in);
                }
                r
            }
            Geometry::Text { subpaths, .. } => {
                let mut r = first(&subpaths[0]);
                for sp in subpaths {
                    for p in sp {
                        r.extend_with(*p);
                    }
                }
                r
            }
            Geometry::MultiPolygon { contours } => {
                let mut r = first(&contours[0]);
                for c in contours {
                    for p in c {
                        r.extend_with(*p);
                    }
                }
                r
            }
        }
    }

    pub fn is_closed_outline(&self) -> bool {
        match self {
            Geometry::Polyline { closed, .. } => *closed,
            Geometry::Path { closed, .. } => *closed,
            _ => true,
        }
    }

    pub fn contains(&self, p: Pos2) -> bool {
        if !self.is_closed_outline() {
            return false;
        }
        for pts in self.contours(96) {
            let n = pts.len();
            if n < 3 {
                continue;
            }
            let mut inside = false;
            for i in 0..n {
                let a = pts[i];
                let b = pts[(i + 1) % n];
                if (a.y > p.y) != (b.y > p.y) {
                    let x_int = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
                    if p.x < x_int {
                        inside = !inside;
                    }
                }
            }
            if inside {
                return true;
            }
        }
        false
    }

    pub fn dist_to_outline(&self, p: Pos2) -> f32 {
        let mut best = f32::INFINITY;
        let closed = self.is_closed_outline();
        for pts in self.contours(96) {
            let n = pts.len();
            if n < 2 {
                continue;
            }
            let segs = if closed { n } else { n - 1 };
            for i in 0..segs {
                let a = pts[i];
                let b = pts[(i + 1) % n];
                best = best.min(seg_dist(p, a, b));
            }
        }
        best
    }
}

fn seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let ap = p - a;
    let len_sq = ab.length_sq();
    let t = if len_sq < 1e-9 {
        0.0
    } else {
        (ap.dot(ab) / len_sq).clamp(0.0, 1.0)
    };
    (p - (a + ab * t)).length()
}

#[derive(Clone, Debug, PartialEq)]
pub enum Fill {
    None,
    Solid(Color32),
    Linear {
        from: [f32; 2],
        to: [f32; 2],
        c0: Color32,
        c1: Color32,
    },
}

impl Fill {
    pub fn is_none(&self) -> bool {
        matches!(self, Fill::None)
    }
}

#[derive(Clone, Debug)]
pub struct Style {
    pub fill: Fill,
    pub stroke: Option<Stroke>,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            fill: Fill::Solid(Color32::from_rgb(0x4F, 0x8C, 0xFF)),
            stroke: Some(Stroke::new(2.0, Color32::from_rgb(0x1B, 0x24, 0x33))),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Shape {
    pub id: ShapeId,
    pub geom: Geometry,
    pub style: Style,
}

#[derive(Clone)]
pub struct VectorLayer {
    pub shapes: Vec<Shape>,
}

#[derive(Clone)]
pub struct RasterLayer {
    pub pixmap: tiny_skia::Pixmap,
    pub version: u64,
}

impl RasterLayer {
    pub fn new(w: u32, h: u32) -> Option<Self> {
        Some(Self {
            pixmap: tiny_skia::Pixmap::new(w, h)?,
            version: 0,
        })
    }

    pub fn touch(&mut self) {
        self.version += 1;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerBlend {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    Difference,
    Exclusion,
    HardLight,
    SoftLight,
}

impl LayerBlend {
    pub const ALL: [LayerBlend; 10] = [
        LayerBlend::Normal,
        LayerBlend::Multiply,
        LayerBlend::Screen,
        LayerBlend::Overlay,
        LayerBlend::Darken,
        LayerBlend::Lighten,
        LayerBlend::Difference,
        LayerBlend::Exclusion,
        LayerBlend::HardLight,
        LayerBlend::SoftLight,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            LayerBlend::Normal => "Normal",
            LayerBlend::Multiply => "Multiply",
            LayerBlend::Screen => "Screen",
            LayerBlend::Overlay => "Overlay",
            LayerBlend::Darken => "Darken",
            LayerBlend::Lighten => "Lighten",
            LayerBlend::Difference => "Difference",
            LayerBlend::Exclusion => "Exclusion",
            LayerBlend::HardLight => "Hard Light",
            LayerBlend::SoftLight => "Soft Light",
        }
    }

    pub fn to_skia(self) -> tiny_skia::BlendMode {
        use tiny_skia::BlendMode as B;
        match self {
            LayerBlend::Normal => B::SourceOver,
            LayerBlend::Multiply => B::Multiply,
            LayerBlend::Screen => B::Screen,
            LayerBlend::Overlay => B::Overlay,
            LayerBlend::Darken => B::Darken,
            LayerBlend::Lighten => B::Lighten,
            LayerBlend::Difference => B::Difference,
            LayerBlend::Exclusion => B::Exclusion,
            LayerBlend::HardLight => B::HardLight,
            LayerBlend::SoftLight => B::SoftLight,
        }
    }
}

#[derive(Clone)]
pub enum LayerKind {
    Vector(VectorLayer),
    Raster(RasterLayer),
}

impl LayerKind {
    pub fn tag(&self) -> &'static str {
        match self {
            LayerKind::Vector(_) => "V",
            LayerKind::Raster(_) => "PX",
        }
    }

    pub fn vector_shapes_mut(&mut self) -> Option<&mut Vec<Shape>> {
        match self {
            LayerKind::Vector(v) => Some(&mut v.shapes),
            _ => None,
        }
    }

    pub fn raster_mut(&mut self) -> Option<&mut RasterLayer> {
        match self {
            LayerKind::Raster(r) => Some(r),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct Layer {
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub opacity: f32,
    pub blend: LayerBlend,
    pub mask: Option<RasterLayer>,
    pub kind: LayerKind,
}

impl Layer {
    pub fn find_shape_by_id(&self, id: u64) -> Option<&Shape> {
        match &self.kind {
            LayerKind::Vector(v) => v.shapes.iter().find(|s| s.id == id),
            _ => None,
        }
    }

    pub fn find_shape_by_id_mut(&mut self, id: u64) -> Option<&mut Shape> {
        match &mut self.kind {
            LayerKind::Vector(v) => v.shapes.iter_mut().find(|s| s.id == id),
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn vector_len(&self) -> usize {
        match &self.kind {
            LayerKind::Vector(v) => v.shapes.len(),
            _ => 0,
        }
    }

    #[cfg(test)]
    pub fn kind_vector(&self) -> Option<&Vec<Shape>> {
        match &self.kind {
            LayerKind::Vector(v) => Some(&v.shapes),
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn raster(&self) -> Option<&RasterLayer> {
        match &self.kind {
            LayerKind::Raster(r) => Some(r),
            _ => None,
        }
    }
}

pub struct Document {
    pub width: f32,
    pub height: f32,
    pub layers: Vec<Layer>,
}

impl Document {
    pub fn size(&self) -> Vec2 {
        Vec2::new(self.width, self.height)
    }

    pub fn new(width: f32, height: f32) -> Self {
        let px_w = width.max(1.0) as u32;
        let px_h = height.max(1.0) as u32;
        Self {
            width,
            height,
            layers: vec![
                Layer {
                    name: "Pixel 1".into(),
                    visible: true,
                    locked: false,
                    opacity: 1.0,
                    blend: LayerBlend::Normal,
                    mask: None,
                    kind: LayerKind::Raster(RasterLayer::new(px_w, px_h).unwrap()),
                },
                Layer {
                    name: "Vector 1".into(),
                    visible: true,
                    locked: false,
                    opacity: 1.0,
                    blend: LayerBlend::Normal,
                    mask: None,
                    kind: LayerKind::Vector(VectorLayer { shapes: vec![] }),
                },
            ],
        }
    }

    pub fn find_shape_mut(&mut self, layer_idx: usize, id: ShapeId) -> Option<&mut Shape> {
        self.layers
            .get_mut(layer_idx)?
            .kind
            .vector_shapes_mut()?
            .iter_mut()
            .find(|s| s.id == id)
    }

    pub fn hit_test(&self, p: Pos2, stroke_slack: f32) -> Option<(usize, ShapeId)> {
        for (li, layer) in self.layers.iter().enumerate().rev() {
            if !layer.visible || layer.locked {
                continue;
            }
            if let LayerKind::Vector(v) = &layer.kind {
                for shape in v.shapes.iter().rev() {
                    if shape.geom.contains(p) || shape.geom.dist_to_outline(p) <= stroke_slack {
                        return Some((li, shape.id));
                    }
                }
            }
        }
        None
    }
}

pub mod history {
    use super::{Geometry, Layer, Shape};
    use eframe::egui::Vec2;

    #[derive(Clone)]
    pub enum Cmd {
        AddShape { layer: usize, shape: Shape },
        AddShapes { layer: usize, shapes: Vec<Shape> },
        RemoveShapes { layer: usize, shapes: Vec<Shape> },
        TranslateShape { layer: usize, id: u64, delta: Vec2 },
        SetGeometry {
            layer: usize,
            id: u64,
            before: Geometry,
            after: Geometry,
        },
        SetStyle {
            layer: usize,
            id: u64,
            before: super::Style,
            after: super::Style,
        },
        AddLayer { index: usize, layer: Layer },
        RemoveLayer { index: usize, layer: Layer },
        BrushStroke {
            layer: usize,
            before: Vec<u8>,
            after: Vec<u8>,
        },
    }

    impl Cmd {
        pub fn invert(self) -> Cmd {
            match self {
                Cmd::AddShape { layer, shape } => Cmd::RemoveShapes {
                    layer,
                    shapes: vec![shape],
                },
                Cmd::AddShapes { layer, shapes } => Cmd::RemoveShapes { layer, shapes },
                Cmd::RemoveShapes { layer, shapes } => Cmd::AddShapes { layer, shapes },
                Cmd::TranslateShape { layer, id, delta } => Cmd::TranslateShape {
                    layer,
                    id,
                    delta: -delta,
                },
                Cmd::SetGeometry {
                    layer,
                    id,
                    before,
                    after,
                } => Cmd::SetGeometry {
                    layer,
                    id,
                    before: after,
                    after: before,
                },
                Cmd::SetStyle {
                    layer,
                    id,
                    before,
                    after,
                } => Cmd::SetStyle {
                    layer,
                    id,
                    before: after,
                    after: before,
                },
                Cmd::AddLayer { index, layer } => Cmd::RemoveLayer { index, layer },
                Cmd::RemoveLayer { index, layer } => Cmd::AddLayer { index, layer },
                Cmd::BrushStroke {
                    layer,
                    before,
                    after,
                } => Cmd::BrushStroke {
                    layer,
                    before: after,
                    after: before,
                },
            }
        }
    }

    const MAX_HISTORY: usize = 40;

    #[derive(Default)]
    pub struct History {
        undo_stack: Vec<Cmd>,
        redo_stack: Vec<Cmd>,
    }

    impl History {
        pub fn push(&mut self, cmd: Cmd) {
            self.undo_stack.push(cmd);
            self.redo_stack.clear();
            if self.undo_stack.len() > MAX_HISTORY {
                self.undo_stack.remove(0);
            }
        }

        pub fn can_undo(&self) -> bool {
            !self.undo_stack.is_empty()
        }

        pub fn can_redo(&self) -> bool {
            !self.redo_stack.is_empty()
        }

        pub fn undo(&mut self) -> Option<Cmd> {
            let inv = self.undo_stack.pop()?.invert();
            self.redo_stack.push(inv.clone());
            Some(inv)
        }

        pub fn redo(&mut self) -> Option<Cmd> {
            let inv = self.redo_stack.pop()?.invert();
            self.undo_stack.push(inv.clone());
            Some(inv)
        }

        pub fn clear(&mut self) {
            self.undo_stack.clear();
            self.redo_stack.clear();
        }
    }
}

pub fn apply_cmd(doc: &mut Document, cmd: &history::Cmd) {
    match cmd {
        history::Cmd::AddShape { layer, shape } => {
            if let Some(vs) = doc.layers.get_mut(*layer).and_then(|l| l.kind.vector_shapes_mut()) {
                vs.push(shape.clone());
            }
        }
        history::Cmd::AddShapes { layer, shapes } => {
            if let Some(vs) = doc.layers.get_mut(*layer).and_then(|l| l.kind.vector_shapes_mut()) {
                vs.extend(shapes.iter().cloned());
            }
        }
        history::Cmd::RemoveShapes { layer, shapes } => {
            let ids: Vec<u64> = shapes.iter().map(|s| s.id).collect();
            if let Some(vs) = doc.layers.get_mut(*layer).and_then(|l| l.kind.vector_shapes_mut()) {
                vs.retain(|s| !ids.contains(&s.id));
            }
        }
        history::Cmd::TranslateShape { layer, id, delta } => {
            if let Some(s) = doc.find_shape_mut(*layer, *id) {
                s.geom.translate(*delta);
            }
        }
        history::Cmd::SetGeometry {
            layer,
            id,
            after,
            ..
        } => {
            if let Some(s) = doc.find_shape_mut(*layer, *id) {
                s.geom = after.clone();
            }
        }
        history::Cmd::SetStyle {
            layer,
            id,
            after,
            ..
        } => {
            if let Some(s) = doc.find_shape_mut(*layer, *id) {
                s.style = after.clone();
            }
        }
        history::Cmd::AddLayer { index, layer } => {
            let idx = (*index).min(doc.layers.len());
            doc.layers.insert(idx, layer.clone());
        }
        history::Cmd::RemoveLayer { index, .. } => {
            if *index < doc.layers.len() {
                doc.layers.remove(*index);
            }
        }
        history::Cmd::BrushStroke { layer, after, .. } => {
            if let Some(r) = doc.layers.get_mut(*layer).and_then(|l| l.kind.raster_mut()) {
                if r.pixmap.data().len() == after.len() {
                    r.pixmap.data_mut().copy_from_slice(after);
                    r.touch();
                }
            }
        }
    }
}
