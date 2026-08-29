//! Hybrid document: vector and raster layers, command history, hit testing.

use crate::color::{Blend, Rgba};
use crate::geom::{Bounds, Geom, Pt};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

pub fn bump_id(seen: u64) {
    NEXT_ID.fetch_max(seen + 1, Ordering::Relaxed);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cap {
    Butt,
    Round,
    Square,
}

impl Cap {
    pub fn name(self) -> &'static str {
        match self {
            Cap::Butt => "Butt",
            Cap::Round => "Round",
            Cap::Square => "Square",
        }
    }
    pub fn to_skia(self) -> tiny_skia::LineCap {
        match self {
            Cap::Butt => tiny_skia::LineCap::Butt,
            Cap::Round => tiny_skia::LineCap::Round,
            Cap::Square => tiny_skia::LineCap::Square,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Join {
    Miter,
    Round,
    Bevel,
}

impl Join {
    pub fn name(self) -> &'static str {
        match self {
            Join::Miter => "Miter",
            Join::Round => "Round",
            Join::Bevel => "Bevel",
        }
    }
    pub fn to_skia(self) -> tiny_skia::LineJoin {
        match self {
            Join::Miter => tiny_skia::LineJoin::Miter,
            Join::Round => tiny_skia::LineJoin::Round,
            Join::Bevel => tiny_skia::LineJoin::Bevel,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Fill {
    None,
    Solid(Rgba),
    Linear {
        from: [f32; 2],
        to: [f32; 2],
        c0: Rgba,
        c1: Rgba,
    },
    Radial {
        c0: Rgba,
        c1: Rgba,
    },
}

impl Fill {
    pub fn is_none(&self) -> bool {
        matches!(self, Fill::None)
    }

    pub fn solid_or(self, fallback: Rgba) -> Rgba {
        match self {
            Fill::Solid(c) => c,
            Fill::Linear { c0, .. } | Fill::Radial { c0, .. } => c0,
            Fill::None => fallback,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub color: Rgba,
    pub width: f32,
    pub cap: Cap,
    pub join: Join,
    pub dash: Option<(f32, f32)>,
}

impl Default for Stroke {
    fn default() -> Self {
        Self {
            color: Rgba::rgb(0x1B, 0x24, 0x33),
            width: 2.0,
            cap: Cap::Round,
            join: Join::Round,
            dash: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Style {
    pub fill: Fill,
    pub stroke: Option<Stroke>,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            fill: Fill::Solid(Rgba::rgb(0x4F, 0x8C, 0xFF)),
            stroke: Some(Stroke::default()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub id: u64,
    pub name: String,
    pub geom: Geom,
    pub style: Style,
    pub rotation: f32,
    pub opacity: f32,
}

impl Shape {
    pub fn new(geom: Geom, style: Style) -> Self {
        let name = geom.kind_name().to_string();
        Self {
            id: next_id(),
            name,
            geom,
            style,
            rotation: 0.0,
            opacity: 1.0,
        }
    }

    pub fn world_contours(&self, segs: usize) -> Vec<Vec<Pt>> {
        let mut cs = self.geom.contours(segs);
        if self.rotation.abs() > 1e-5 {
            let c = self.geom.bbox().center();
            for contour in &mut cs {
                for p in contour {
                    *p = p.rotate_about(c, self.rotation);
                }
            }
        }
        cs
    }

    pub fn world_bbox(&self) -> Bounds {
        let mut b = None;
        for c in self.world_contours(32) {
            for p in c {
                match &mut b {
                    None => b = Some(Bounds::from_pt(p)),
                    Some(bb) => bb.union_pt(p),
                }
            }
        }
        b.unwrap_or_else(|| self.geom.bbox())
    }

    pub fn contains_world(&self, p: Pt) -> bool {
        let q = if self.rotation.abs() > 1e-5 {
            p.rotate_about(self.geom.bbox().center(), -self.rotation)
        } else {
            p
        };
        self.geom.contains(q)
    }

    pub fn dist_world(&self, p: Pt) -> f32 {
        let q = if self.rotation.abs() > 1e-5 {
            p.rotate_about(self.geom.bbox().center(), -self.rotation)
        } else {
            p
        };
        self.geom.dist_to_outline(q)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pixels {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
    #[serde(skip)]
    pub version: u64,
}

impl Pixels {
    pub fn new(w: u32, h: u32) -> Self {
        Self {
            w: w.max(1),
            h: h.max(1),
            data: vec![0u8; w.max(1) as usize * h.max(1) as usize * 4],
            version: 0,
        }
    }

    pub fn from_rgba(w: u32, h: u32, data: Vec<u8>) -> Option<Self> {
        if data.len() != w as usize * h as usize * 4 {
            return None;
        }
        Some(Self {
            w,
            h,
            data,
            version: 1,
        })
    }

    pub fn touch(&mut self) {
        self.version += 1;
    }

    pub fn to_pixmap(&self) -> Option<tiny_skia::Pixmap> {
        let mut pm = tiny_skia::Pixmap::new(self.w, self.h)?;
        for (i, src) in self.data.chunks_exact(4).enumerate() {
            let a = src[3] as u32;
            let (r, g, b) = if a == 0 || a == 255 {
                (src[0], src[1], src[2])
            } else {
                (
                    ((src[0] as u32 * 255 + a - 1) / a).min(255) as u8,
                    ((src[1] as u32 * 255 + a - 1) / a).min(255) as u8,
                    ((src[2] as u32 * 255 + a - 1) / a).min(255) as u8,
                )
            };
            pm.data_mut()[i * 4..i * 4 + 4].copy_from_slice(&[r, g, b, src[3]]);
        }
        Some(pm)
    }

    pub fn from_pixmap(pm: &tiny_skia::Pixmap) -> Self {
        let mut data = vec![0u8; pm.data().len()];
        for (i, src) in pm.data().chunks_exact(4).enumerate() {
            let a = src[3] as u32;
            let (r, g, b) = if a == 0 || a == 255 {
                (src[0], src[1], src[2])
            } else {
                (
                    ((src[0] as u32 * 255) / a).min(255) as u8,
                    ((src[1] as u32 * 255) / a).min(255) as u8,
                    ((src[2] as u32 * 255) / a).min(255) as u8,
                )
            };
            data[i * 4..i * 4 + 4].copy_from_slice(&[r, g, b, src[3]]);
        }
        Self {
            w: pm.width(),
            h: pm.height(),
            data,
            version: 1,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LayerKind {
    Vector { shapes: Vec<Shape> },
    Raster { pixels: Pixels },
}

impl LayerKind {
    pub fn tag(&self) -> &'static str {
        match self {
            LayerKind::Vector { .. } => "V",
            LayerKind::Raster { .. } => "Px",
        }
    }

    pub fn shapes(&self) -> Option<&[Shape]> {
        match self {
            LayerKind::Vector { shapes } => Some(shapes),
            _ => None,
        }
    }

    pub fn shapes_mut(&mut self) -> Option<&mut Vec<Shape>> {
        match self {
            LayerKind::Vector { shapes } => Some(shapes),
            _ => None,
        }
    }

    pub fn pixels_mut(&mut self) -> Option<&mut Pixels> {
        match self {
            LayerKind::Raster { pixels } => Some(pixels),
            _ => None,
        }
    }

    pub fn pixels(&self) -> Option<&Pixels> {
        match self {
            LayerKind::Raster { pixels } => Some(pixels),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Layer {
    pub id: u64,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub opacity: f32,
    pub blend: Blend,
    pub mask: Option<Pixels>,
    pub kind: LayerKind,
}

impl Layer {
    pub fn vector(name: impl Into<String>) -> Self {
        Self {
            id: next_id(),
            name: name.into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            blend: Blend::Normal,
            mask: None,
            kind: LayerKind::Vector { shapes: vec![] },
        }
    }

    pub fn raster(name: impl Into<String>, w: u32, h: u32) -> Self {
        Self {
            id: next_id(),
            name: name.into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            blend: Blend::Normal,
            mask: None,
            kind: LayerKind::Raster {
                pixels: Pixels::new(w, h),
            },
        }
    }

    pub fn find(&self, id: u64) -> Option<&Shape> {
        self.kind.shapes()?.iter().find(|s| s.id == id)
    }

    pub fn find_mut(&mut self, id: u64) -> Option<&mut Shape> {
        self.kind.shapes_mut()?.iter_mut().find(|s| s.id == id)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Guide {
    pub vertical: bool,
    pub pos: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Grid {
    pub visible: bool,
    pub snap: bool,
    pub size: f32,
    pub subdivisions: u32,
}

impl Default for Grid {
    fn default() -> Self {
        Self {
            visible: false,
            snap: true,
            size: 8.0,
            subdivisions: 1,
        }
    }
}

fn default_artboards() -> u32 {
    1
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    pub name: String,
    pub width: f32,
    pub height: f32,
    pub dpi: f32,
    pub layers: Vec<Layer>,
    pub guides: Vec<Guide>,
    pub grid: Grid,
    #[serde(default)]
    pub transparent: bool,
    #[serde(default = "default_artboards")]
    pub artboards: u32,
    #[serde(default)]
    pub show_bleed: bool,
    #[serde(default)]
    pub show_safe: bool,
    #[serde(default)]
    pub bleed: f32,
}

impl Document {
    pub fn new(name: impl Into<String>, width: f32, height: f32, dpi: f32) -> Self {
        Self::new_with_options(name, width, height, dpi, false, 1, false, false)
    }

    pub fn new_with_options(
        name: impl Into<String>,
        width: f32,
        height: f32,
        dpi: f32,
        transparent: bool,
        artboards: u32,
        show_bleed: bool,
        show_safe: bool,
    ) -> Self {
        let w = width.max(1.0) as u32;
        let h = height.max(1.0) as u32;
        let art = artboards.max(1);
        // For multiple artboards we tile them horizontally with a 48px gutter.
        let total_w = if art > 1 {
            w * art + 48 * (art - 1)
        } else {
            w
        };
        let mut doc = Self {
            name: name.into(),
            width: if art > 1 { total_w as f32 } else { width },
            height,
            dpi,
            layers: vec![
                Layer::raster("Background", total_w, h),
                Layer::vector("Layer 1"),
            ],
            guides: vec![],
            grid: Grid::default(),
            transparent,
            artboards: art,
            show_bleed,
            show_safe,
            bleed: 36.0, // 0.125" at 300dpi or 0.5" at 72dpi ~ 36px
        };
        // Fill background white when not transparent
        if !transparent {
            if let Some(px) = doc.layers[0].kind.pixels_mut() {
                let white = crate::color::Rgba::WHITE;
                for chunk in px.data.chunks_mut(4) {
                    chunk[0] = white.r;
                    chunk[1] = white.g;
                    chunk[2] = white.b;
                    chunk[3] = 255;
                }
                px.touch();
            }
        }
        if art > 1 {
            // Vertical artboard separators as guides
            for i in 1..art {
                let x = (w as f32 + 48.0) * i as f32 - 24.0;
                doc.guides.push(Guide { vertical: true, pos: x });
            }
        }
        if show_bleed {
            // Outer bleed rect guides
            let b = doc.bleed;
            doc.guides.push(Guide { vertical: true, pos: -b });
            doc.guides.push(Guide { vertical: true, pos: doc.width + b });
            doc.guides.push(Guide { vertical: false, pos: -b });
            doc.guides.push(Guide { vertical: false, pos: doc.height + b });
        }
        doc
    }

    pub fn size(&self) -> Pt {
        Pt::new(self.width, self.height)
    }

    pub fn find_shape(&self, layer: usize, id: u64) -> Option<&Shape> {
        self.layers.get(layer)?.find(id)
    }

    pub fn find_shape_mut(&mut self, layer: usize, id: u64) -> Option<&mut Shape> {
        self.layers.get_mut(layer)?.find_mut(id)
    }

    pub fn hit_test(&self, p: Pt, stroke_slack: f32) -> Option<(usize, u64)> {
        for (li, layer) in self.layers.iter().enumerate().rev() {
            if !layer.visible || layer.locked {
                continue;
            }
            if let Some(shapes) = layer.kind.shapes() {
                for shape in shapes.iter().rev() {
                    if shape.contains_world(p) || shape.dist_world(p) <= stroke_slack {
                        return Some((li, shape.id));
                    }
                }
            }
        }
        None
    }

    pub fn hits_in_rect(&self, r: Bounds) -> Vec<(usize, u64)> {
        let mut out = vec![];
        for (li, layer) in self.layers.iter().enumerate() {
            if !layer.visible || layer.locked {
                continue;
            }
            if let Some(shapes) = layer.kind.shapes() {
                for shape in shapes {
                    if shape.world_bbox().intersects(r) {
                        out.push((li, shape.id));
                    }
                }
            }
        }
        out
    }

    pub fn ensure_ids(&mut self) {
        let mut max = 0u64;
        for l in &self.layers {
            max = max.max(l.id);
            if let Some(ss) = l.kind.shapes() {
                for s in ss {
                    max = max.max(s.id);
                }
            }
        }
        bump_id(max);
    }
}

#[derive(Clone, Debug)]
pub enum Cmd {
    AddShape {
        layer: usize,
        shape: Shape,
    },
    RestoreShapes {
        layer: usize,
        shapes: Vec<Shape>,
    },
    RemoveShapes {
        layer: usize,
        shapes: Vec<Shape>,
    },
    SetGeom {
        layer: usize,
        id: u64,
        before: Geom,
        after: Geom,
        rot_before: f32,
        rot_after: f32,
    },
    SetStyle {
        layer: usize,
        id: u64,
        before: Style,
        after: Style,
    },
    SetOpacity {
        layer: usize,
        id: u64,
        before: f32,
        after: f32,
    },
    AddLayer {
        index: usize,
        layer: Layer,
    },
    RemoveLayer {
        index: usize,
        layer: Layer,
    },
    ReorderLayer {
        from: usize,
        to: usize,
    },
    SetLayerMeta {
        index: usize,
        name: String,
        visible: bool,
        locked: bool,
        opacity: f32,
        blend: Blend,
        before: (String, bool, bool, f32, Blend),
    },
    Pixels {
        layer: usize,
        mask: bool,
        before: Vec<u8>,
        after: Vec<u8>,
    },
    AddGuide {
        guide: Guide,
    },
    RemoveGuide {
        index: usize,
        guide: Guide,
    },
}

const MAX_HISTORY: usize = 80;

#[derive(Default)]
pub struct History {
    undo: Vec<Cmd>,
    redo: Vec<Cmd>,
}

impl History {
    pub fn push(&mut self, cmd: Cmd) {
        self.undo.push(cmd);
        self.redo.clear();
        if self.undo.len() > MAX_HISTORY {
            self.undo.remove(0);
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> Option<Cmd> {
        let cmd = self.undo.pop()?;
        let inv = invert_cmd(cmd.clone());
        self.redo.push(cmd);
        Some(inv)
    }

    pub fn redo(&mut self) -> Option<Cmd> {
        let cmd = self.redo.pop()?;
        self.undo.push(cmd.clone());
        Some(cmd)
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    pub fn len(&self) -> usize {
        self.undo.len()
    }
}

fn invert_cmd(cmd: Cmd) -> Cmd {
    match cmd {
        Cmd::AddShape { layer, shape } => Cmd::RemoveShapes {
            layer,
            shapes: vec![shape],
        },
        Cmd::RemoveShapes { layer, shapes } => Cmd::RestoreShapes { layer, shapes },
        Cmd::SetGeom {
            layer,
            id,
            before,
            after,
            rot_before,
            rot_after,
        } => Cmd::SetGeom {
            layer,
            id,
            before: after,
            after: before,
            rot_before: rot_after,
            rot_after: rot_before,
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
        Cmd::SetOpacity {
            layer,
            id,
            before,
            after,
        } => Cmd::SetOpacity {
            layer,
            id,
            before: after,
            after: before,
        },
        Cmd::AddLayer { index, layer } => Cmd::RemoveLayer { index, layer },
        Cmd::RemoveLayer { index, layer } => Cmd::AddLayer { index, layer },
        Cmd::ReorderLayer { from, to } => Cmd::ReorderLayer { from: to, to: from },
        Cmd::SetLayerMeta {
            index,
            name,
            visible,
            locked,
            opacity,
            blend,
            before,
        } => Cmd::SetLayerMeta {
            index,
            name: before.0.clone(),
            visible: before.1,
            locked: before.2,
            opacity: before.3,
            blend: before.4,
            before: (name, visible, locked, opacity, blend),
        },
        Cmd::Pixels {
            layer,
            mask,
            before,
            after,
        } => Cmd::Pixels {
            layer,
            mask,
            before: after,
            after: before,
        },
        Cmd::AddGuide { guide } => Cmd::RemoveGuide {
            index: usize::MAX,
            guide,
        },
        Cmd::RemoveGuide { index: _, guide } => Cmd::AddGuide { guide },
        Cmd::RestoreShapes { layer, shapes } => Cmd::RemoveShapes { layer, shapes },
    }
}

pub fn apply(doc: &mut Document, cmd: &Cmd) {
    match cmd {
        Cmd::AddShape { layer, shape } => {
            if let Some(vs) = doc.layers.get_mut(*layer).and_then(|l| l.kind.shapes_mut()) {
                vs.push(shape.clone());
            }
        }
        Cmd::RestoreShapes { layer, shapes } => {
            if let Some(vs) = doc.layers.get_mut(*layer).and_then(|l| l.kind.shapes_mut()) {
                vs.extend(shapes.iter().cloned());
            }
        }
        Cmd::RemoveShapes { layer, shapes } => {
            let ids: Vec<u64> = shapes.iter().map(|s| s.id).collect();
            if let Some(vs) = doc.layers.get_mut(*layer).and_then(|l| l.kind.shapes_mut()) {
                vs.retain(|s| !ids.contains(&s.id));
            }
        }
        Cmd::SetGeom {
            layer,
            id,
            after,
            rot_after,
            ..
        } => {
            if let Some(s) = doc.find_shape_mut(*layer, *id) {
                s.geom = after.clone();
                s.rotation = *rot_after;
            }
        }
        Cmd::SetStyle {
            layer,
            id,
            after,
            ..
        } => {
            if let Some(s) = doc.find_shape_mut(*layer, *id) {
                s.style = after.clone();
            }
        }
        Cmd::SetOpacity {
            layer,
            id,
            after,
            ..
        } => {
            if let Some(s) = doc.find_shape_mut(*layer, *id) {
                s.opacity = *after;
            }
        }
        Cmd::AddLayer { index, layer } => {
            let i = (*index).min(doc.layers.len());
            doc.layers.insert(i, layer.clone());
        }
        Cmd::RemoveLayer { index, .. } => {
            if *index < doc.layers.len() && doc.layers.len() > 1 {
                doc.layers.remove(*index);
            }
        }
        Cmd::ReorderLayer { from, to } => {
            if *from < doc.layers.len() {
                let layer = doc.layers.remove(*from);
                let t = (*to).min(doc.layers.len());
                doc.layers.insert(t, layer);
            }
        }
        Cmd::SetLayerMeta {
            index,
            name,
            visible,
            locked,
            opacity,
            blend,
            ..
        } => {
            if let Some(l) = doc.layers.get_mut(*index) {
                l.name = name.clone();
                l.visible = *visible;
                l.locked = *locked;
                l.opacity = *opacity;
                l.blend = *blend;
            }
        }
        Cmd::Pixels {
            layer,
            mask,
            after,
            ..
        } => {
            let apply_px = |px: &mut Pixels| {
                if px.data.len() == after.len() {
                    px.data.copy_from_slice(after);
                    px.touch();
                }
            };
            if let Some(l) = doc.layers.get_mut(*layer) {
                if *mask {
                    if let Some(m) = l.mask.as_mut() {
                        apply_px(m);
                    }
                } else if let Some(px) = l.kind.pixels_mut() {
                    apply_px(px);
                }
            }
        }
        Cmd::AddGuide { guide } => doc.guides.push(*guide),
        Cmd::RemoveGuide { index, guide } => {
            if *index < doc.guides.len() {
                doc.guides.remove(*index);
            } else if let Some(i) = doc.guides.iter().position(|g| {
                g.vertical == guide.vertical && (g.pos - guide.pos).abs() < 0.01
            }) {
                doc.guides.remove(i);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_undo_shape() {
        let mut doc = Document::new("t", 200.0, 100.0, 72.0);
        let mut hist = History::default();
        let s = Shape::new(
            Geom::Rect {
                origin: Pt::new(10.0, 10.0),
                size: Pt::new(20.0, 20.0),
                radius: 0.0,
            },
            Style::default(),
        );
        let cmd = Cmd::AddShape {
            layer: 1,
            shape: s,
        };
        apply(&mut doc, &cmd);
        hist.push(cmd);
        assert_eq!(doc.layers[1].kind.shapes().unwrap().len(), 1);
        let inv = hist.undo().unwrap();
        apply(&mut doc, &inv);
        assert_eq!(doc.layers[1].kind.shapes().unwrap().len(), 0);
        let redo = hist.redo().unwrap();
        apply(&mut doc, &redo);
        assert_eq!(doc.layers[1].kind.shapes().unwrap().len(), 1);
    }

    #[test]
    fn hit_test_finds_rect() {
        let mut doc = Document::new("t", 200.0, 100.0, 72.0);
        let s = Shape::new(
            Geom::Rect {
                origin: Pt::new(10.0, 10.0),
                size: Pt::new(20.0, 20.0),
                radius: 0.0,
            },
            Style::default(),
        );
        let id = s.id;
        apply(
            &mut doc,
            &Cmd::AddShape {
                layer: 1,
                shape: s,
            },
        );
        assert_eq!(doc.hit_test(Pt::new(20.0, 20.0), 2.0), Some((1, id)));
        assert_eq!(doc.hit_test(Pt::new(90.0, 90.0), 2.0), None);
    }
}
