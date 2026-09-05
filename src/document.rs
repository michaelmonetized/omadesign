//! Hybrid document: vector and raster layers, command history, hit testing.

use crate::color::{Blend, Rgba};
use crate::geom::{Bounds, Geom, Pt};
use serde::{Deserialize, Deserializer, Serialize};
use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Selection id for a raster layer treated as an object (placed image).
pub const RASTER_ID: u64 = 0;

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

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Shape {
    pub id: u64,
    pub name: String,
    pub geom: Geom,
    pub style: Style,
    pub rotation: f32,
    pub opacity: f32,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub filters: crate::filter::FilterStack,
    /// Per-corner radii (TL, TR, BR, BL). All zero → use `Geom::Rect.radius`.
    #[serde(default)]
    pub corners: [f32; 4],
    #[serde(skip)]
    cached_path: RefCell<Option<Arc<CachedPath>>>,
}

#[derive(Debug)]
struct CachedPath {
    geom: Geom,
    rotation: f32,
    corners: [f32; 4],
    segments: usize,
    path: Option<Arc<tiny_skia::Path>>,
}

impl PartialEq for Shape {
    fn eq(&self, other: &Self) -> bool {
        // Rendering a shape does not change its document value.
        self.id == other.id
            && self.name == other.name
            && self.geom == other.geom
            && self.style == other.style
            && self.rotation == other.rotation
            && self.opacity == other.opacity
            && self.visible == other.visible
            && self.locked == other.locked
            && self.filters == other.filters
            && self.corners == other.corners
    }
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
            visible: true,
            locked: false,
            filters: crate::filter::FilterStack::default(),
            corners: [0.0; 4],
            cached_path: RefCell::new(None),
        }
    }

    pub fn effective_corners(&self) -> [f32; 4] {
        if self.corners.iter().any(|c| *c > 0.05) {
            return self.corners;
        }
        if let Geom::Rect { radius, .. } = self.geom {
            [radius; 4]
        } else {
            [0.0; 4]
        }
    }

    pub fn world_contours(&self, segs: usize) -> Vec<Vec<Pt>> {
        let mut cs = if let Geom::Rect { origin, size, .. } = self.geom {
            vec![crate::geom::rounded_rect_corners(
                origin,
                size,
                self.effective_corners(),
            )]
        } else {
            self.geom.contours(segs)
        };
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
        if self.rotation.abs() <= 1e-5 && !matches!(self.geom, Geom::Path { .. }) {
            return self.geom.bbox();
        }
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

    pub fn get_cached_path(&self, segs: usize) -> Option<Arc<tiny_skia::Path>> {
        if let Some(cached) = self.cached_path.borrow().as_ref()
            && cached.segments == segs
            && cached.rotation == self.rotation
            && cached.corners == self.corners
            && cached.geom == self.geom
        {
            return cached.path.clone();
        }
        let mut pb = tiny_skia::PathBuilder::new();
        for contour in self.world_contours(segs) {
            if contour.len() < 2 {
                continue;
            }
            pb.move_to(contour[0].x, contour[0].y);
            for p in &contour[1..] {
                pb.line_to(p.x, p.y);
            }
            if self.geom.is_closed() {
                pb.close();
            }
        }
        let path = pb.finish().map(Arc::new);
        *self.cached_path.borrow_mut() = Some(Arc::new(CachedPath {
            geom: self.geom.clone(),
            rotation: self.rotation,
            corners: self.corners,
            segments: segs,
            path: path.clone(),
        }));
        path
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Pixels {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
    #[serde(skip)]
    pub version: u64,
    #[serde(skip)]
    pub(crate) cached_pm: RefCell<Option<(u64, tiny_skia::Pixmap)>>,
    #[serde(skip)]
    pub(crate) cached_uniform: RefCell<Option<(u64, Option<Rgba>)>>,
}

impl Clone for Pixels {
    fn clone(&self) -> Self {
        Self {
            w: self.w,
            h: self.h,
            data: self.data.clone(),
            version: self.version,
            cached_pm: RefCell::new(None),
            cached_uniform: RefCell::new(*self.cached_uniform.borrow()),
        }
    }
}

impl Pixels {
    pub fn new(w: u32, h: u32) -> Self {
        Self {
            w: w.max(1),
            h: h.max(1),
            data: vec![0u8; w.max(1) as usize * h.max(1) as usize * 4],
            version: 0,
            cached_pm: RefCell::new(None),
            cached_uniform: RefCell::new(Some((0, Some(Rgba::TRANSPARENT)))),
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
            cached_pm: RefCell::new(None),
            cached_uniform: RefCell::new(None),
        })
    }

    pub fn touch(&mut self) {
        self.version = self.version.wrapping_add(1);
        self.cached_pm.borrow_mut().take();
        self.cached_uniform.borrow_mut().take();
    }

    fn build_pm(&self) -> Option<tiny_skia::Pixmap> {
        crate::color::rgba_to_pixmap(self.w, self.h, &self.data)
    }

    fn ensure_pm(&self) {
        let mut slot = self.cached_pm.borrow_mut();
        if let Some((v, pm)) = slot.as_ref()
            && *v == self.version
            && pm.width() == self.w
            && pm.height() == self.h
        {
            return;
        }
        *slot = self.build_pm().map(|pm| (self.version, pm));
    }

    pub fn with_pm<R>(&self, f: impl FnOnce(&tiny_skia::Pixmap) -> R) -> Option<R> {
        self.ensure_pm();
        let slot = self.cached_pm.borrow();
        slot.as_ref().map(|(_, pm)| f(pm))
    }

    pub fn to_pixmap(&self) -> Option<tiny_skia::Pixmap> {
        self.with_pm(|pm| pm.clone())
    }

    /// Every pixel has alpha 0.
    pub fn is_invisible(&self) -> bool {
        self.data.is_empty() || self.data.chunks_exact(4).all(|p| p[3] == 0)
    }

    /// `Some` when every pixel is the same rgba.
    pub fn is_uniform(&self) -> Option<Rgba> {
        if let Some((version, color)) = *self.cached_uniform.borrow()
            && version == self.version
        {
            return color;
        }
        let color = self.data.chunks_exact(4).next().and_then(|first| {
            self.data
                .chunks_exact(4)
                .all(|pixel| pixel == first)
                .then(|| Rgba::new(first[0], first[1], first[2], first[3]))
        });
        *self.cached_uniform.borrow_mut() = Some((self.version, color));
        color
    }

    pub fn from_pixmap(pm: &tiny_skia::Pixmap) -> Self {
        let mut data = vec![0u8; pm.data().len()];
        for (dst, src) in data.chunks_exact_mut(4).zip(pm.pixels()) {
            let color = src.demultiply();
            dst.copy_from_slice(&[color.red(), color.green(), color.blue(), color.alpha()]);
        }
        Self {
            w: pm.width(),
            h: pm.height(),
            data,
            version: 1,
            cached_pm: RefCell::new(Some((1, pm.clone()))),
            cached_uniform: RefCell::new(None),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LayerKind {
    Vector {
        shapes: Vec<Shape>,
    },
    Raster {
        pixels: Pixels,
        /// Display origin. `(0,0)` + zero size = full-buffer paint layer.
        #[serde(default)]
        origin: Pt,
        /// Display size. Zero means native `pixels.w/h`.
        #[serde(default)]
        size: Pt,
        #[serde(default)]
        rotation: f32,
    },
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
            LayerKind::Raster { pixels, .. } => Some(pixels),
            _ => None,
        }
    }

    pub fn pixels(&self) -> Option<&Pixels> {
        match self {
            LayerKind::Raster { pixels, .. } => Some(pixels),
            _ => None,
        }
    }

    pub fn is_placed_raster(&self) -> bool {
        match self {
            LayerKind::Raster { size, origin, .. } => {
                size.x.abs() > 0.5 && size.y.abs() > 0.5
                    || origin.x.abs() > 0.5
                    || origin.y.abs() > 0.5
            }
            _ => false,
        }
    }

    pub fn raster_xform(&self) -> Option<(Pt, Pt, f32)> {
        match self {
            LayerKind::Raster {
                pixels,
                origin,
                size,
                rotation,
            } => {
                let sz = if size.x.abs() > 0.5 && size.y.abs() > 0.5 {
                    *size
                } else {
                    Pt::new(pixels.w as f32, pixels.h as f32)
                };
                Some((*origin, sz, *rotation))
            }
            _ => None,
        }
    }

    pub fn raster_bounds(&self) -> Option<Bounds> {
        let (origin, size, rotation) = self.raster_xform()?;
        let b = Bounds::from_min_size(origin, size);
        if rotation.abs() > 1e-5 {
            let c = b.center();
            let mut out = Bounds::from_pt(b.min.rotate_about(c, rotation));
            for p in [Pt::new(b.max.x, b.min.y), b.max, Pt::new(b.min.x, b.max.y)] {
                out.union_pt(p.rotate_about(c, rotation));
            }
            Some(out)
        } else {
            Some(b)
        }
    }

    pub fn set_raster_xform(&mut self, origin: Pt, size: Pt, rotation: f32) {
        if let LayerKind::Raster {
            origin: o,
            size: s,
            rotation: r,
            ..
        } = self
        {
            *o = origin;
            *s = size;
            *r = rotation;
        }
    }

    pub fn raster_contains(&self, p: Pt) -> bool {
        let LayerKind::Raster {
            pixels,
            origin,
            size,
            rotation,
        } = self
        else {
            return false;
        };
        let sz = if size.x.abs() > 0.5 && size.y.abs() > 0.5 {
            *size
        } else {
            Pt::new(pixels.w as f32, pixels.h as f32)
        };
        if sz.x.abs() < 1.0 || sz.y.abs() < 1.0 {
            return false;
        }
        let c = *origin + sz * 0.5;
        let q = if rotation.abs() > 1e-5 {
            p.rotate_about(c, -rotation)
        } else {
            p
        };
        let local = q - *origin;
        if local.x < 0.0 || local.y < 0.0 || local.x > sz.x || local.y > sz.y {
            return false;
        }
        let px = ((local.x / sz.x) * pixels.w as f32).floor() as i32;
        let py = ((local.y / sz.y) * pixels.h as f32).floor() as i32;
        if px < 0 || py < 0 || px >= pixels.w as i32 || py >= pixels.h as i32 {
            return false;
        }
        let i = ((py as u32 * pixels.w + px as u32) * 4) as usize;
        pixels.data.get(i + 3).copied().unwrap_or(0) > 8
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
    #[serde(default)]
    pub filters: crate::filter::FilterStack,
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
            filters: crate::filter::FilterStack::default(),
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
                origin: Pt::ZERO,
                size: Pt::ZERO,
                rotation: 0.0,
            },
            filters: crate::filter::FilterStack::default(),
        }
    }

    pub fn placed_raster(name: impl Into<String>, pixels: Pixels, origin: Pt, size: Pt) -> Self {
        Self {
            id: next_id(),
            name: name.into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            blend: Blend::Normal,
            mask: None,
            kind: LayerKind::Raster {
                pixels,
                origin,
                size,
                rotation: 0.0,
            },
            filters: crate::filter::FilterStack::default(),
        }
    }

    pub fn find(&self, id: u64) -> Option<&Shape> {
        self.kind.shapes()?.iter().find(|s| s.id == id)
    }

    pub fn find_mut(&mut self, id: u64) -> Option<&mut Shape> {
        self.kind.shapes_mut()?.iter_mut().find(|s| s.id == id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Guide {
    pub vertical: bool,
    pub pos: f32,
}

/// Ruler labels are converted from document pixels; artwork stays in pixel space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RulerUnit {
    #[default]
    Pixels,
    Millimeters,
    Centimeters,
    Inches,
    Points,
}

impl RulerUnit {
    pub const ALL: [Self; 5] = [
        Self::Pixels,
        Self::Millimeters,
        Self::Centimeters,
        Self::Inches,
        Self::Points,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Pixels => "Pixels (px)",
            Self::Millimeters => "Millimeters (mm)",
            Self::Centimeters => "Centimeters (cm)",
            Self::Inches => "Inches (in)",
            Self::Points => "Points (pt)",
        }
    }

    pub fn suffix(self) -> &'static str {
        match self {
            Self::Pixels => "px",
            Self::Millimeters => "mm",
            Self::Centimeters => "cm",
            Self::Inches => "in",
            Self::Points => "pt",
        }
    }

    pub fn pixels_per_unit(self, dpi: f32) -> f32 {
        let dpi = if dpi.is_finite() && dpi > 0.0 {
            dpi
        } else {
            72.0
        };
        match self {
            Self::Pixels => 1.0,
            Self::Millimeters => dpi / 25.4,
            Self::Centimeters => dpi / 2.54,
            Self::Inches => dpi,
            Self::Points => dpi / 72.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RulerSettings {
    pub origin: Pt,
    pub unit: RulerUnit,
    pub guides_visible: bool,
}

impl Default for RulerSettings {
    fn default() -> Self {
        Self {
            origin: Pt::ZERO,
            unit: RulerUnit::Pixels,
            guides_visible: true,
        }
    }
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Artboard {
    pub id: u64,
    pub name: String,
    pub origin: Pt,
    pub size: Pt,
    #[serde(default)]
    pub rotation: f32,
}

impl Artboard {
    pub fn new(index: usize, origin: Pt, size: Pt) -> Self {
        Self {
            id: next_id(),
            name: format!("Artboard {}", index + 1),
            origin,
            size,
            rotation: 0.0,
        }
    }

    pub fn blank() -> Self {
        Self {
            id: 0,
            name: String::new(),
            origin: Pt::ZERO,
            size: Pt::ZERO,
            rotation: 0.0,
        }
    }

    pub fn tiled(count: u32, page_w: f32, page_h: f32) -> Vec<Self> {
        let n = count.max(1);
        (0..n)
            .map(|i| {
                Self::new(
                    i as usize,
                    Pt::new((page_w + 48.0) * i as f32, 0.0),
                    Pt::new(page_w, page_h),
                )
            })
            .collect()
    }

    pub fn local_bounds(&self) -> Bounds {
        Bounds::from_min_size(self.origin, self.size)
    }

    pub fn center(&self) -> Pt {
        self.origin + self.size * 0.5
    }

    pub fn corners(&self) -> [Pt; 4] {
        let o = self.origin;
        let s = self.size;
        let pts = [o, Pt::new(o.x + s.x, o.y), o + s, Pt::new(o.x, o.y + s.y)];
        let c = self.center();
        if self.rotation.abs() < 1e-5 {
            pts
        } else {
            pts.map(|p| p.rotate_about(c, self.rotation))
        }
    }

    pub fn handle_pts(&self) -> [Pt; 8] {
        let b = self.local_bounds();
        let c = self.center();
        let mut hs = [Pt::ZERO; 8];
        for i in 0..8 {
            let p = b.handle(i);
            hs[i] = if self.rotation.abs() < 1e-5 {
                p
            } else {
                p.rotate_about(c, self.rotation)
            };
        }
        hs
    }

    pub fn rotate_handle_pt(&self) -> Pt {
        let rh = self.local_bounds().rotate_handle();
        if self.rotation.abs() < 1e-5 {
            rh
        } else {
            rh.rotate_about(self.center(), self.rotation)
        }
    }

    /// Artboards sit on the page. Rotation snaps to 0 / 90 / 180 / 270.
    pub fn snap_rotation(rad: f32) -> f32 {
        let deg = rad.to_degrees();
        let wrapped = (deg + 180.0).rem_euclid(360.0) - 180.0;
        let snapped = (wrapped / 90.0).round() * 90.0;
        snapped.to_radians()
    }

    pub fn bounds(&self) -> Bounds {
        let b = self.local_bounds();
        if self.rotation.abs() < 1e-5 {
            return b;
        }
        let c = self.center();
        let mut out = Bounds::from_pt(b.min.rotate_about(c, self.rotation));
        for p in [Pt::new(b.max.x, b.min.y), b.max, Pt::new(b.min.x, b.max.y)] {
            out.union_pt(p.rotate_about(c, self.rotation));
        }
        out
    }

    pub fn contains(&self, p: Pt) -> bool {
        let c = self.origin + self.size * 0.5;
        let q = if self.rotation.abs() > 1e-5 {
            p.rotate_about(c, -self.rotation)
        } else {
            p
        };
        let d = q - self.origin;
        d.x >= 0.0 && d.y >= 0.0 && d.x <= self.size.x && d.y <= self.size.y
    }

    pub fn area(&self) -> f32 {
        self.size.x.abs() * self.size.y.abs()
    }

    /// Distance to the frame. Inside: nearest edge. Outside: distance to rect.
    pub fn edge_dist(&self, p: Pt) -> f32 {
        let b = self.bounds();
        if b.is_empty() {
            return f32::MAX;
        }
        let dx = if p.x < b.min.x {
            b.min.x - p.x
        } else if p.x > b.max.x {
            p.x - b.max.x
        } else {
            0.0
        };
        let dy = if p.y < b.min.y {
            b.min.y - p.y
        } else if p.y > b.max.y {
            p.y - b.max.y
        } else {
            0.0
        };
        if dx > 0.0 && dy > 0.0 {
            (dx * dx + dy * dy).sqrt()
        } else if dx > 0.0 {
            dx
        } else if dy > 0.0 {
            dy
        } else {
            (p.x - b.min.x)
                .min(b.max.x - p.x)
                .min(p.y - b.min.y)
                .min(b.max.y - p.y)
        }
    }
}

fn deserialize_artboards<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Artboard>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Count(u32),
        List(Vec<Artboard>),
    }
    match Raw::deserialize(d)? {
        Raw::List(v) if !v.is_empty() => Ok(v),
        Raw::List(_) => Ok(vec![Artboard::blank()]),
        Raw::Count(n) => Ok((0..n.max(1)).map(|_| Artboard::blank()).collect()),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    pub name: String,
    pub width: f32,
    pub height: f32,
    pub dpi: f32,
    pub layers: Vec<Layer>,
    pub guides: Vec<Guide>,
    #[serde(default)]
    pub ruler: RulerSettings,
    pub grid: Grid,
    #[serde(default)]
    pub transparent: bool,
    #[serde(default, deserialize_with = "deserialize_artboards")]
    pub artboards: Vec<Artboard>,
    #[serde(default)]
    pub show_bleed: bool,
    #[serde(default)]
    pub show_safe: bool,
    #[serde(default)]
    pub bleed: f32,
    #[serde(default)]
    pub motion: crate::motion::Motion,
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
        let page_w = width.max(1.0);
        let page_h = height.max(1.0);
        let art = artboards.max(1);
        let boards = Artboard::tiled(art, page_w, page_h);
        let total_w = if art > 1 {
            page_w * art as f32 + 48.0 * (art as f32 - 1.0)
        } else {
            page_w
        };
        let w = total_w.round().max(1.0) as u32;
        let h = page_h.round().max(1.0) as u32;
        let mut doc = Self {
            name: name.into(),
            width: total_w,
            height: page_h,
            dpi,
            layers: vec![Layer::raster("Background", w, h), Layer::vector("Layer 1")],
            guides: vec![],
            ruler: RulerSettings::default(),
            grid: Grid::default(),
            transparent,
            artboards: boards,
            show_bleed,
            show_safe,
            bleed: 36.0, // 0.125" at 300dpi or 0.5" at 72dpi ~ 36px
            motion: crate::motion::Motion::default(),
        };
        // Fill background white when not transparent
        if !transparent && let Some(px) = doc.layers[0].kind.pixels_mut() {
            let white = crate::color::Rgba::WHITE;
            for chunk in px.data.chunks_mut(4) {
                chunk[0] = white.r;
                chunk[1] = white.g;
                chunk[2] = white.b;
                chunk[3] = 255;
            }
            px.touch();
        }
        if show_bleed {
            // Outer bleed rect guides
            let b = doc.bleed;
            doc.guides.push(Guide {
                vertical: true,
                pos: -b,
            });
            doc.guides.push(Guide {
                vertical: true,
                pos: doc.width + b,
            });
            doc.guides.push(Guide {
                vertical: false,
                pos: -b,
            });
            doc.guides.push(Guide {
                vertical: false,
                pos: doc.height + b,
            });
        }
        doc
    }

    pub fn migrate_artboards(&mut self) {
        if self.artboards.is_empty() {
            self.artboards = vec![Artboard::new(
                0,
                Pt::ZERO,
                Pt::new(self.width.max(1.0), self.height.max(1.0)),
            )];
            return;
        }
        if self
            .artboards
            .iter()
            .all(|a| a.size.x < 1.0 && a.size.y < 1.0)
        {
            let n = self.artboards.len().max(1) as u32;
            let gutter = 48.0;
            let page_w = if n > 1 {
                (self.width - gutter * (n as f32 - 1.0)) / n as f32
            } else {
                self.width
            };
            self.artboards = Artboard::tiled(n, page_w.max(1.0), self.height.max(1.0));
        }
        for (i, a) in self.artboards.iter_mut().enumerate() {
            if a.id == 0 {
                a.id = next_id();
            }
            if a.name.trim().is_empty() {
                a.name = format!("Artboard {}", i + 1);
            }
        }
    }

    pub fn unique_artboard_name(&self, wanted: &str) -> String {
        let base = wanted.trim();
        let base = if base.is_empty() { "Artboard" } else { base };
        if !self.artboards.iter().any(|a| a.name == base) {
            return base.to_string();
        }
        for n in 2..10_000 {
            let cand = format!("{base} {n}");
            if !self.artboards.iter().any(|a| a.name == cand) {
                return cand;
            }
        }
        format!("{base} {}", next_id())
    }

    pub fn artboard_hit(&self, p: Pt, slack: f32) -> Option<u64> {
        let mut best_edge: Option<(f32, f32, u64)> = None;
        let mut best_in: Option<(f32, u64)> = None;
        for a in &self.artboards {
            let area = a.area().max(1.0);
            let d = a.edge_dist(p);
            if d <= slack {
                let better = best_edge
                    .is_none_or(|(bd, ba, _)| d + 0.5 < bd || ((d - bd).abs() <= 0.5 && area < ba));
                if better {
                    best_edge = Some((d, area, a.id));
                }
            }
            if a.contains(p) {
                let better = best_in.is_none_or(|(ba, _)| area < ba);
                if better {
                    best_in = Some((area, a.id));
                }
            }
        }
        best_edge
            .map(|(_, _, id)| id)
            .or_else(|| best_in.map(|(_, id)| id))
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
                    if !shape.visible || shape.locked {
                        continue;
                    }
                    if shape.contains_world(p) || shape.dist_world(p) <= stroke_slack {
                        return Some((li, shape.id));
                    }
                }
            }
            if layer.kind.is_placed_raster() && layer.kind.raster_contains(p) {
                return Some((li, RASTER_ID));
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
                    if !shape.visible || shape.locked {
                        continue;
                    }
                    if shape.world_bbox().intersects(r) {
                        out.push((li, shape.id));
                    }
                }
            }
            if layer.kind.is_placed_raster()
                && let Some(b) = layer.kind.raster_bounds()
                && b.intersects(r)
            {
                out.push((li, RASTER_ID));
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
        for a in &self.artboards {
            max = max.max(a.id);
        }
        bump_id(max);
        self.migrate_artboards();
    }
}

#[derive(Clone, Debug)]
pub enum Cmd {
    Batch(Vec<Cmd>),
    SetLayerMask {
        index: usize,
        before: Option<Pixels>,
        after: Option<Pixels>,
    },
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
    ReorderShape {
        layer: usize,
        from: usize,
        to: usize,
    },
    SetGeoms {
        items: Vec<(usize, u64, Geom, Geom, f32, f32)>,
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
    InsertGuide {
        index: usize,
        guide: Guide,
    },
    SetGuides {
        before: Vec<Guide>,
        after: Vec<Guide>,
    },
    SetRuler {
        before: RulerSettings,
        after: RulerSettings,
    },
    SetMotion {
        before: crate::motion::Motion,
        after: crate::motion::Motion,
    },
    SetFilters {
        index: usize,
        before: crate::filter::FilterStack,
        after: crate::filter::FilterStack,
    },
    SetShapeFilters {
        layer: usize,
        id: u64,
        before: crate::filter::FilterStack,
        after: crate::filter::FilterStack,
    },
    SetShapeMeta {
        layer: usize,
        id: u64,
        name: String,
        visible: bool,
        locked: bool,
        before: (String, bool, bool),
    },
    SetRasterXform {
        layer: usize,
        before: (Pt, Pt, f32),
        after: (Pt, Pt, f32),
    },
    SetArtboards {
        before: Vec<Artboard>,
        after: Vec<Artboard>,
    },
    SetCorners {
        layer: usize,
        id: u64,
        before: [f32; 4],
        after: [f32; 4],
        radius_before: f32,
        radius_after: f32,
    },
}

const MAX_HISTORY: usize = 200;

#[derive(Default, Clone)]
pub struct History {
    undo: Vec<Cmd>,
    redo: Vec<Cmd>,
}

impl History {
    pub fn push(&mut self, cmd: Cmd) {
        if let Some(prev) = self.undo.last_mut()
            && coalesce(prev, &cmd)
        {
            self.redo.clear();
            return;
        }
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

fn coalesce(prev: &mut Cmd, next: &Cmd) -> bool {
    match (prev, next) {
        (
            Cmd::SetStyle {
                layer, id, after, ..
            },
            Cmd::SetStyle {
                layer: l2,
                id: i2,
                after: a2,
                ..
            },
        ) if *layer == *l2 && *id == *i2 => {
            *after = a2.clone();
            true
        }
        (
            Cmd::SetOpacity {
                layer, id, after, ..
            },
            Cmd::SetOpacity {
                layer: l2,
                id: i2,
                after: a2,
                ..
            },
        ) if *layer == *l2 && *id == *i2 => {
            *after = *a2;
            true
        }
        (
            Cmd::SetLayerMeta {
                index,
                name,
                visible,
                locked,
                opacity,
                blend,
                ..
            },
            Cmd::SetLayerMeta {
                index: i2,
                name: n2,
                visible: v2,
                locked: k2,
                opacity: o2,
                blend: b2,
                ..
            },
        ) if *index == *i2 => {
            *name = n2.clone();
            *visible = *v2;
            *locked = *k2;
            *opacity = *o2;
            *blend = *b2;
            true
        }
        (Cmd::SetMotion { after, .. }, Cmd::SetMotion { after: a2, .. }) => {
            *after = a2.clone();
            true
        }
        (
            Cmd::SetFilters { index, after, .. },
            Cmd::SetFilters {
                index: i2,
                after: a2,
                ..
            },
        ) if *index == *i2 => {
            *after = a2.clone();
            true
        }
        (
            Cmd::SetShapeFilters {
                layer, id, after, ..
            },
            Cmd::SetShapeFilters {
                layer: l2,
                id: i2,
                after: a2,
                ..
            },
        ) if *layer == *l2 && *id == *i2 => {
            *after = a2.clone();
            true
        }
        (
            Cmd::SetRasterXform { layer, after, .. },
            Cmd::SetRasterXform {
                layer: l2,
                after: a2,
                ..
            },
        ) if *layer == *l2 => {
            *after = *a2;
            true
        }
        (Cmd::SetArtboards { after, .. }, Cmd::SetArtboards { after: a2, .. }) => {
            *after = a2.clone();
            true
        }
        (
            Cmd::SetCorners {
                layer,
                id,
                after,
                radius_after,
                ..
            },
            Cmd::SetCorners {
                layer: l2,
                id: i2,
                after: a2,
                radius_after: r2,
                ..
            },
        ) if *layer == *l2 && *id == *i2 => {
            *after = *a2;
            *radius_after = *r2;
            true
        }
        (
            Cmd::SetShapeMeta {
                layer,
                id,
                name,
                visible,
                locked,
                ..
            },
            Cmd::SetShapeMeta {
                layer: l2,
                id: i2,
                name: n2,
                visible: v2,
                locked: k2,
                ..
            },
        ) if *layer == *l2 && *id == *i2 => {
            *name = n2.clone();
            *visible = *v2;
            *locked = *k2;
            true
        }
        _ => false,
    }
}

fn invert_cmd(cmd: Cmd) -> Cmd {
    match cmd {
        Cmd::SetLayerMask {
            index,
            before,
            after,
        } => Cmd::SetLayerMask {
            index,
            before: after,
            after: before,
        },
        Cmd::Batch(commands) => Cmd::Batch(commands.into_iter().rev().map(invert_cmd).collect()),
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
        Cmd::ReorderShape { layer, from, to } => Cmd::ReorderShape {
            layer,
            from: to,
            to: from,
        },
        Cmd::SetGeoms { items } => Cmd::SetGeoms {
            items: items
                .into_iter()
                .map(|(layer, id, before, after, rb, ra)| (layer, id, after, before, ra, rb))
                .collect(),
        },
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
        Cmd::RemoveGuide { index, guide } => Cmd::InsertGuide { index, guide },
        Cmd::InsertGuide { index, guide } => Cmd::RemoveGuide { index, guide },
        Cmd::SetGuides { before, after } => Cmd::SetGuides {
            before: after,
            after: before,
        },
        Cmd::SetRuler { before, after } => Cmd::SetRuler {
            before: after,
            after: before,
        },
        Cmd::RestoreShapes { layer, shapes } => Cmd::RemoveShapes { layer, shapes },
        Cmd::SetMotion { before, after } => Cmd::SetMotion {
            before: after,
            after: before,
        },
        Cmd::SetFilters {
            index,
            before,
            after,
        } => Cmd::SetFilters {
            index,
            before: after,
            after: before,
        },
        Cmd::SetShapeFilters {
            layer,
            id,
            before,
            after,
        } => Cmd::SetShapeFilters {
            layer,
            id,
            before: after,
            after: before,
        },
        Cmd::SetShapeMeta {
            layer,
            id,
            name,
            visible,
            locked,
            before,
        } => Cmd::SetShapeMeta {
            layer,
            id,
            name: before.0.clone(),
            visible: before.1,
            locked: before.2,
            before: (name, visible, locked),
        },
        Cmd::SetRasterXform {
            layer,
            before,
            after,
        } => Cmd::SetRasterXform {
            layer,
            before: after,
            after: before,
        },
        Cmd::SetArtboards { before, after } => Cmd::SetArtboards {
            before: after,
            after: before,
        },
        Cmd::SetCorners {
            layer,
            id,
            before,
            after,
            radius_before,
            radius_after,
        } => Cmd::SetCorners {
            layer,
            id,
            before: after,
            after: before,
            radius_before: radius_after,
            radius_after: radius_before,
        },
    }
}

pub fn apply(doc: &mut Document, cmd: &Cmd) {
    match cmd {
        Cmd::SetLayerMask { index, after, .. } => {
            if let Some(layer) = doc.layers.get_mut(*index) {
                layer.mask = after.clone();
            }
        }
        Cmd::Batch(commands) => {
            for command in commands {
                apply(doc, command);
            }
        }
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
            doc.motion.drop_shapes(&ids);
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
            layer, id, after, ..
        } => {
            if let Some(s) = doc.find_shape_mut(*layer, *id) {
                s.style = after.clone();
            }
        }
        Cmd::SetOpacity {
            layer, id, after, ..
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
        Cmd::ReorderShape { layer, from, to } => {
            if let Some(vs) = doc.layers.get_mut(*layer).and_then(|l| l.kind.shapes_mut())
                && *from < vs.len()
            {
                let shape = vs.remove(*from);
                let t = (*to).min(vs.len());
                vs.insert(t, shape);
            }
        }
        Cmd::SetGeoms { items } => {
            for (layer, id, _, after, _, rot_after) in items {
                if let Some(s) = doc.find_shape_mut(*layer, *id) {
                    s.geom = after.clone();
                    s.rotation = *rot_after;
                }
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
            layer, mask, after, ..
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
        Cmd::SetMotion { after, .. } => {
            doc.motion = after.clone();
        }
        Cmd::SetFilters { index, after, .. } => {
            if let Some(l) = doc.layers.get_mut(*index) {
                l.filters = after.clone();
            }
        }
        Cmd::SetShapeFilters {
            layer, id, after, ..
        } => {
            if let Some(s) = doc.find_shape_mut(*layer, *id) {
                s.filters = after.clone();
            }
        }
        Cmd::SetShapeMeta {
            layer,
            id,
            name,
            visible,
            locked,
            ..
        } => {
            if let Some(s) = doc.find_shape_mut(*layer, *id) {
                s.name = name.clone();
                s.visible = *visible;
                s.locked = *locked;
            }
        }
        Cmd::SetRasterXform { layer, after, .. } => {
            if let Some(l) = doc.layers.get_mut(*layer) {
                l.kind.set_raster_xform(after.0, after.1, after.2);
            }
        }
        Cmd::SetArtboards { after, .. } => {
            doc.artboards = after.clone();
        }
        Cmd::SetCorners {
            layer,
            id,
            after,
            radius_after,
            ..
        } => {
            if let Some(s) = doc.find_shape_mut(*layer, *id) {
                s.corners = *after;
                if let Geom::Rect { radius, .. } = &mut s.geom {
                    *radius = *radius_after;
                }
            }
        }
        Cmd::InsertGuide { index, guide } => {
            doc.guides.insert((*index).min(doc.guides.len()), *guide)
        }
        Cmd::SetGuides { after, .. } => doc.guides.clone_from(after),
        Cmd::SetRuler { after, .. } => doc.ruler = *after,
        Cmd::AddGuide { guide } => doc.guides.push(*guide),
        Cmd::RemoveGuide { index, guide } => {
            if *index < doc.guides.len() {
                doc.guides.remove(*index);
            } else if let Some(i) = doc
                .guides
                .iter()
                .rposition(|g| g.vertical == guide.vertical && (g.pos - guide.pos).abs() < 0.01)
            {
                doc.guides.remove(i);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_roundtrip_preserves_partial_alpha_after_cache_rebuild() {
        let rgba = vec![255, 0, 0, 128, 0, 255, 0, 64, 12, 34, 56, 255];
        let pixels = Pixels::from_rgba(3, 1, rgba.clone()).unwrap();
        let pm = pixels.to_pixmap().unwrap();
        assert_eq!(&pm.data()[..8], &[128, 0, 0, 128, 0, 64, 0, 64]);
        let mut restored = Pixels::from_pixmap(&pm);
        assert_eq!(restored.data, rgba);
        restored.touch();
        assert_eq!(restored.to_pixmap().unwrap().data(), pm.data());
    }

    #[test]
    fn raster_uniform_cache_follows_edits_and_clones_without_render_cache() {
        let mut pixels = Pixels::new(2, 1);
        assert_eq!(pixels.is_uniform(), Some(Rgba::TRANSPARENT));
        pixels.data[..4].copy_from_slice(&[255, 0, 0, 255]);
        pixels.touch();
        assert_eq!(pixels.is_uniform(), None);
        pixels.data[4..].copy_from_slice(&[255, 0, 0, 255]);
        pixels.touch();
        pixels.to_pixmap().unwrap();
        assert_eq!(pixels.is_uniform(), Some(Rgba::rgb(255, 0, 0)));
        let cloned = pixels.clone();
        assert_eq!(cloned.is_uniform(), pixels.is_uniform());
        assert!(cloned.cached_pm.borrow().is_none());
        assert_eq!(
            cloned.to_pixmap().unwrap().data(),
            pixels.to_pixmap().unwrap().data()
        );
    }

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
        let cmd = Cmd::AddShape { layer: 1, shape: s };
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
        apply(&mut doc, &Cmd::AddShape { layer: 1, shape: s });
        assert_eq!(doc.hit_test(Pt::new(20.0, 20.0), 2.0), Some((1, id)));
        assert_eq!(doc.hit_test(Pt::new(90.0, 90.0), 2.0), None);
    }

    #[test]
    fn path_cache_follows_geom() {
        let mut s = Shape::new(
            Geom::Rect {
                origin: Pt::new(0.0, 0.0),
                size: Pt::new(10.0, 10.0),
                radius: 0.0,
            },
            Style::default(),
        );
        let a = s.get_cached_path(8).unwrap();
        s.geom.translate(Pt::new(40.0, 0.0));
        let b = s.get_cached_path(8).unwrap();
        assert_ne!(a.bounds(), b.bounds(), "moved shape must rebuild its path");
    }

    #[test]
    fn path_cache_tracks_interior_points_and_resolution() {
        let contours = vec![vec![
            Pt::ZERO,
            Pt::new(10.0, 0.0),
            Pt::new(5.0, 5.0),
            Pt::new(0.0, 10.0),
        ]];
        for geom in [
            Geom::Poly {
                contours: contours.clone(),
                winding: false,
            },
            Geom::Text(crate::geom::TypeRun {
                contours,
                ..Default::default()
            }),
        ] {
            let mut shape = Shape::new(geom, Style::default());
            let unrendered = shape.clone();
            let original = shape.get_cached_path(32).unwrap();
            assert_eq!(shape, unrendered);
            assert!(Arc::ptr_eq(&original, &shape.get_cached_path(32).unwrap()));
            match &mut shape.geom {
                Geom::Poly { contours, .. } => contours[0][2].x = 7.0,
                Geom::Text(t) => t.contours[0][2].x = 7.0,
                _ => unreachable!(),
            }
            let edited = shape.get_cached_path(32).unwrap();
            assert_eq!(original.bounds(), edited.bounds());
            assert_ne!(original.points(), edited.points());
        }
        let ellipse = Shape::new(
            Geom::Ellipse {
                center: Pt::ZERO,
                radii: Pt::new(10.0, 10.0),
            },
            Style::default(),
        );
        assert!(
            ellipse.get_cached_path(64).unwrap().points().len()
                > ellipse.get_cached_path(8).unwrap().points().len()
        );
    }

    #[test]
    fn artboard_rotation_snaps_to_cardinals() {
        let q = std::f32::consts::FRAC_PI_2;
        let s = |d: f32| Artboard::snap_rotation(d.to_radians()).to_degrees().round();
        assert_eq!(s(10.0), 0.0);
        assert_eq!(s(44.0), 0.0);
        assert_eq!(s(46.0), 90.0);
        assert_eq!(s(90.0), 90.0);
        assert_eq!(s(135.0), 180.0);
        assert_eq!(s(-10.0), 0.0);
        assert_eq!(s(-46.0), -90.0);
        let r = Artboard::snap_rotation(q * 0.4);
        assert!((r - 0.0).abs() < 1e-5);
        let r = Artboard::snap_rotation(q * 0.6);
        assert!((r - q).abs() < 1e-4);
    }

    #[test]
    fn artboard_count_migrates() {
        let json = r#"{"name":"t","width":200,"height":100,"dpi":72,"layers":[],"guides":[],"grid":{"visible":false,"snap":true,"size":8,"subdivisions":1},"artboards":3}"#;
        let mut doc: Document = serde_json::from_str(json).unwrap();
        doc.migrate_artboards();
        assert_eq!(doc.artboards.len(), 3);
        assert!(doc.artboards[0].size.x > 1.0);
        assert_eq!(doc.artboards[0].name, "Artboard 1");
        assert!(doc.artboards[0].id != 0);
    }

    #[test]
    fn placed_raster_hit_test() {
        let mut doc = Document::new("t", 200.0, 100.0, 72.0);
        let mut data = vec![0u8; 8 * 8 * 4];
        for px in data.chunks_mut(4) {
            px[0] = 255;
            px[3] = 255;
        }
        let pixels = Pixels::from_rgba(8, 8, data).unwrap();
        doc.layers.push(Layer::placed_raster(
            "photo",
            pixels,
            Pt::new(40.0, 20.0),
            Pt::new(80.0, 80.0),
        ));
        let li = doc.layers.len() - 1;
        assert_eq!(
            doc.hit_test(Pt::new(50.0, 30.0), 2.0),
            Some((li, RASTER_ID))
        );
        assert_eq!(doc.hit_test(Pt::new(5.0, 5.0), 2.0), None);
    }

    #[test]
    fn artboard_hit_prefers_smaller_and_edges() {
        let mut doc = Document::new("t", 1920.0, 1080.0, 72.0);
        doc.artboards = vec![
            Artboard {
                id: 1,
                name: "A".into(),
                origin: Pt::ZERO,
                size: Pt::new(1920.0, 1080.0),
                rotation: 0.0,
            },
            Artboard {
                id: 2,
                name: "B".into(),
                origin: Pt::new(-8.0, -32.0),
                size: Pt::new(3052.0, 1140.0),
                rotation: 0.0,
            },
        ];
        // Centre of A is inside both; smaller A wins.
        assert_eq!(doc.artboard_hit(Pt::new(960.0, 540.0), 8.0), Some(1));
        // A's top edge, even though B covers it.
        assert_eq!(doc.artboard_hit(Pt::new(100.0, 0.0), 8.0), Some(1));
        // Only B's margin.
        assert_eq!(doc.artboard_hit(Pt::new(2500.0, 500.0), 8.0), Some(2));
    }
}
