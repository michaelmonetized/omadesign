//! Geometry primitives. No UI types. Callers and tests share this seam.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
}

impl Pt {
    pub const ZERO: Pt = Pt { x: 0.0, y: 0.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn splat(v: f32) -> Self {
        Self { x: v, y: v }
    }

    pub fn length(self) -> f32 {
        self.length_sq().sqrt()
    }

    pub fn length_sq(self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    pub fn normalized(self) -> Self {
        let l = self.length();
        if l < 1e-9 { Self::ZERO } else { self / l }
    }

    pub fn dot(self, o: Self) -> f32 {
        self.x * o.x + self.y * o.y
    }

    pub fn cross(self, o: Self) -> f32 {
        self.x * o.y - self.y * o.x
    }

    pub fn perp(self) -> Self {
        Self {
            x: -self.y,
            y: self.x,
        }
    }

    pub fn lerp(self, o: Self, t: f32) -> Self {
        self + (o - self) * t
    }

    pub fn rotate(self, angle: f32) -> Self {
        let (s, c) = angle.sin_cos();
        Self {
            x: self.x * c - self.y * s,
            y: self.x * s + self.y * c,
        }
    }

    pub fn rotate_about(self, origin: Self, angle: f32) -> Self {
        origin + (self - origin).rotate(angle)
    }

    pub fn min(self, o: Self) -> Self {
        Self {
            x: self.x.min(o.x),
            y: self.y.min(o.y),
        }
    }

    pub fn max(self, o: Self) -> Self {
        Self {
            x: self.x.max(o.x),
            y: self.y.max(o.y),
        }
    }

    pub fn abs(self) -> Self {
        Self {
            x: self.x.abs(),
            y: self.y.abs(),
        }
    }

    pub fn to_array(self) -> [f32; 2] {
        [self.x, self.y]
    }

    pub fn from_array(a: [f32; 2]) -> Self {
        Self { x: a[0], y: a[1] }
    }
}

impl std::ops::Add for Pt {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl std::ops::AddAssign for Pt {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl std::ops::Sub for Pt {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl std::ops::Neg for Pt {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
        }
    }
}

impl std::ops::Mul<f32> for Pt {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl std::ops::Div<f32> for Pt {
    type Output = Self;
    fn div(self, rhs: f32) -> Self {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub min: Pt,
    pub max: Pt,
}

impl Bounds {
    pub fn from_pt(p: Pt) -> Self {
        Self { min: p, max: p }
    }

    pub fn from_min_size(min: Pt, size: Pt) -> Self {
        Self {
            min,
            max: min + size,
        }
    }

    pub fn from_center_size(center: Pt, size: Pt) -> Self {
        let h = size * 0.5;
        Self {
            min: center - h,
            max: center + h,
        }
    }

    pub fn union_pt(&mut self, p: Pt) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }

    pub fn union(&self, o: Self) -> Self {
        Self {
            min: self.min.min(o.min),
            max: self.max.max(o.max),
        }
    }

    pub fn width(self) -> f32 {
        (self.max.x - self.min.x).max(0.0)
    }

    pub fn height(self) -> f32 {
        (self.max.y - self.min.y).max(0.0)
    }

    pub fn size(self) -> Pt {
        Pt::new(self.width(), self.height())
    }

    pub fn center(self) -> Pt {
        (self.min + self.max) * 0.5
    }

    pub fn contains(self, p: Pt) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }

    pub fn inflate(self, pad: f32) -> Self {
        Self {
            min: Pt::new(self.min.x - pad, self.min.y - pad),
            max: Pt::new(self.max.x + pad, self.max.y + pad),
        }
    }

    pub fn intersects(self, o: Self) -> bool {
        self.min.x <= o.max.x
            && self.max.x >= o.min.x
            && self.min.y <= o.max.y
            && self.max.y >= o.min.y
    }

    pub fn is_empty(self) -> bool {
        self.width() < 1e-4 || self.height() < 1e-4
    }

    /// Map a point from this box into `dst` (used for resize).
    pub fn map_pt(self, p: Pt, dst: Self) -> Pt {
        let sx = if self.width() < 1e-6 {
            1.0
        } else {
            dst.width() / self.width()
        };
        let sy = if self.height() < 1e-6 {
            1.0
        } else {
            dst.height() / self.height()
        };
        Pt::new(
            dst.min.x + (p.x - self.min.x) * sx,
            dst.min.y + (p.y - self.min.y) * sy,
        )
    }

    pub fn handle(self, i: usize) -> Pt {
        match i {
            0 => self.min,
            1 => Pt::new(self.max.x, self.min.y),
            2 => self.max,
            3 => Pt::new(self.min.x, self.max.y),
            4 => Pt::new(self.center().x, self.min.y),
            5 => Pt::new(self.max.x, self.center().y),
            6 => Pt::new(self.center().x, self.max.y),
            7 => Pt::new(self.min.x, self.center().y),
            _ => self.center(),
        }
    }

    pub fn rotate_handle(self) -> Pt {
        Pt::new(self.center().x, self.min.y - 28.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub pt: Pt,
    pub h_in: Pt,
    pub h_out: Pt,
    /// Corner rounding at this node. Used by Node-mode widgets and path tessellation.
    #[serde(default)]
    pub radius: f32,
}

/// Screen-pixel drag needed before a pen click becomes a smooth point.
pub const PEN_DRAG_PX: f32 = 3.0;

/// Snap a vector to 45° increments.
pub fn constrain_45(delta: Pt) -> Pt {
    let ang = delta.y.atan2(delta.x);
    let snapped = (ang / std::f32::consts::FRAC_PI_4).round() * std::f32::consts::FRAC_PI_4;
    let len = delta.length();
    Pt::new(len * snapped.cos(), len * snapped.sin())
}

/// Apply pen click-drag to the just-placed anchor.
/// Under `PEN_DRAG_PX` screen pixels this stays a corner — click places a point.
pub fn apply_pen_smooth(anchor: &mut Anchor, drag: Pt, scale: f32, alt: bool, shift: bool) {
    let d = if shift { constrain_45(drag) } else { drag };
    if d.length() * scale.max(0.01) < PEN_DRAG_PX {
        anchor.h_in = Pt::ZERO;
        anchor.h_out = Pt::ZERO;
        return;
    }
    if alt {
        anchor.h_out = d;
    } else {
        anchor.h_out = d;
        anchor.h_in = -d;
    }
}

impl Anchor {
    pub fn corner(pt: Pt) -> Self {
        Self {
            pt,
            h_in: Pt::ZERO,
            h_out: Pt::ZERO,
            radius: 0.0,
        }
    }

    pub fn smooth(pt: Pt, drag: Pt) -> Self {
        Self {
            pt,
            h_in: -drag,
            h_out: drag,
            radius: 0.0,
        }
    }

    pub fn is_corner(&self) -> bool {
        self.h_in.length_sq() < 0.25 && self.h_out.length_sq() < 0.25
    }

    pub fn make_corner(&mut self) {
        self.h_in = Pt::ZERO;
        self.h_out = Pt::ZERO;
    }

    pub fn make_smooth(&mut self) {
        if self.is_corner() {
            self.h_out = Pt::new(24.0, 0.0);
            self.h_in = Pt::new(-24.0, 0.0);
        } else {
            let mag = ((self.h_in.length() + self.h_out.length()) * 0.5).max(12.0);
            let dir = if self.h_out.length_sq() > 1e-4 {
                self.h_out.normalized()
            } else if self.h_in.length_sq() > 1e-4 {
                (-self.h_in).normalized()
            } else {
                Pt::new(1.0, 0.0)
            };
            self.h_out = dir * mag;
            self.h_in = -dir * mag;
        }
    }
}

const CUBIC_STEPS: usize = 16;

pub fn flatten_cubic(p0: Pt, c1: Pt, c2: Pt, p1: Pt, out: &mut Vec<Pt>) {
    for i in 1..=CUBIC_STEPS {
        let t = i as f32 / CUBIC_STEPS as f32;
        out.push(eval_cubic(p0, c1, c2, p1, t));
    }
}

pub fn eval_cubic(p0: Pt, c1: Pt, c2: Pt, p1: Pt, t: f32) -> Pt {
    let u = 1.0 - t;
    p0 * (u * u * u) + c1 * (3.0 * u * u * t) + c2 * (3.0 * u * t * t) + p1 * (t * t * t)
}

/// Split a cubic at `t` via de Casteljau. Returns (left, right) control points.
pub fn split_cubic(p0: Pt, c1: Pt, c2: Pt, p1: Pt, t: f32) -> ([Pt; 4], [Pt; 4]) {
    let a = p0.lerp(c1, t);
    let b = c1.lerp(c2, t);
    let c = c2.lerp(p1, t);
    let d = a.lerp(b, t);
    let e = b.lerp(c, t);
    let f = d.lerp(e, t);
    ([p0, a, d, f], [f, e, c, p1])
}

pub fn seg_dist(p: Pt, a: Pt, b: Pt) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_sq();
    let t = if len_sq < 1e-9 {
        0.0
    } else {
        ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0)
    };
    (p - (a + ab * t)).length()
}

pub fn point_in_poly(p: Pt, pts: &[Pt]) -> bool {
    let n = pts.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        if (a.y > p.y) != (b.y > p.y) {
            let x_int = a.x + (p.y - a.y) / (b.y - a.y + 1e-12) * (b.x - a.x);
            if p.x < x_int {
                inside = !inside;
            }
        }
    }
    inside
}

pub fn poly_area(pts: &[Pt]) -> f32 {
    let n = pts.len();
    if n < 3 {
        return 0.0;
    }
    let mut a = 0.0;
    for i in 0..n {
        let p = pts[i];
        let q = pts[(i + 1) % n];
        a += p.x * q.y - q.x * p.y;
    }
    a * 0.5
}

fn ellipse_pts(center: Pt, radii: Pt, segs: usize) -> Vec<Pt> {
    let n = segs.max(3);
    (0..n)
        .map(|i| {
            let a = std::f32::consts::TAU * i as f32 / n as f32;
            Pt::new(center.x + radii.x * a.cos(), center.y + radii.y * a.sin())
        })
        .collect()
}

fn rounded_rect(origin: Pt, size: Pt, radius: f32) -> Vec<Pt> {
    rounded_rect_corners(origin, size, [radius; 4])
}

/// Per-corner radii: TL, TR, BR, BL.
pub fn rounded_rect_corners(origin: Pt, size: Pt, radii: [f32; 4]) -> Vec<Pt> {
    let x0 = origin.x.min(origin.x + size.x);
    let x1 = origin.x.max(origin.x + size.x);
    let y0 = origin.y.min(origin.y + size.y);
    let y1 = origin.y.max(origin.y + size.y);
    let w = (x1 - x0).max(0.0);
    let h = (y1 - y0).max(0.0);
    let mut r = radii;
    for rr in &mut r {
        *rr = rr.max(0.0).min(w * 0.5).min(h * 0.5);
    }
    if r[0] + r[1] > w {
        let s = w / (r[0] + r[1]).max(1e-6);
        r[0] *= s;
        r[1] *= s;
    }
    if r[3] + r[2] > w {
        let s = w / (r[3] + r[2]).max(1e-6);
        r[3] *= s;
        r[2] *= s;
    }
    if r[0] + r[3] > h {
        let s = h / (r[0] + r[3]).max(1e-6);
        r[0] *= s;
        r[3] *= s;
    }
    if r[1] + r[2] > h {
        let s = h / (r[1] + r[2]).max(1e-6);
        r[1] *= s;
        r[2] *= s;
    }
    if r.iter().all(|x| *x < 0.5) {
        return vec![
            Pt::new(x0, y0),
            Pt::new(x1, y0),
            Pt::new(x1, y1),
            Pt::new(x0, y1),
        ];
    }
    let mut pts = Vec::new();
    let corners = [
        (Pt::new(x1 - r[1], y0 + r[1]), r[1], 1.5, 2.0),
        (Pt::new(x1 - r[2], y1 - r[2]), r[2], 0.0, 0.5),
        (Pt::new(x0 + r[3], y1 - r[3]), r[3], 0.5, 1.0),
        (Pt::new(x0 + r[0], y0 + r[0]), r[0], 1.0, 1.5),
    ];
    for (c, rad, a0, a1) in corners {
        if rad < 0.5 {
            pts.push(c);
            continue;
        }
        for i in 0..=6 {
            let t = i as f32 / 6.0;
            let a = (a0 + (a1 - a0) * t) * std::f32::consts::PI;
            pts.push(Pt::new(c.x + rad * a.cos(), c.y + rad * a.sin()));
        }
    }
    pts
}

/// Widget positions for on-canvas corner rounding (TL, TR, BR, BL).
pub fn corner_widgets(origin: Pt, size: Pt) -> [Pt; 4] {
    let x0 = origin.x;
    let y0 = origin.y;
    let x1 = origin.x + size.x;
    let y1 = origin.y + size.y;
    let inset = 14.0f32
        .min(size.x.abs() * 0.25)
        .min(size.y.abs() * 0.25)
        .max(6.0);
    [
        Pt::new(x0 + inset.copysign(size.x), y0 + inset.copysign(size.y)),
        Pt::new(x1 - inset.copysign(size.x), y0 + inset.copysign(size.y)),
        Pt::new(x1 - inset.copysign(size.x), y1 - inset.copysign(size.y)),
        Pt::new(x0 + inset.copysign(size.x), y1 - inset.copysign(size.y)),
    ]
}

fn polygon_pts(center: Pt, radii: Pt, sides: u32) -> Vec<Pt> {
    let n = sides.max(3);
    (0..n)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * i as f32 / n as f32;
            Pt::new(center.x + radii.x * a.cos(), center.y + radii.y * a.sin())
        })
        .collect()
}

fn star_pts(center: Pt, outer: Pt, inner: f32, points: u32) -> Vec<Pt> {
    let n = points.max(3);
    let mut pts = Vec::with_capacity((n * 2) as usize);
    for i in 0..n * 2 {
        let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * i as f32 / (n * 2) as f32;
        let r = if i % 2 == 0 {
            outer
        } else {
            Pt::new(outer.x * inner, outer.y * inner)
        };
        pts.push(Pt::new(center.x + r.x * a.cos(), center.y + r.y * a.sin()));
    }
    pts
}

fn path_pts(anchors: &[Anchor], closed: bool) -> Vec<Pt> {
    if anchors.is_empty() {
        return vec![];
    }
    if anchors.len() == 1 {
        return vec![anchors[0].pt];
    }
    let n = anchors.len();
    let mut pts = Vec::new();
    let segs = if closed { n } else { n - 1 };
    for i in 0..segs {
        let (a_pt, a_out) = filleted_out(anchors, closed, i);
        let (b_pt, b_in) = filleted_in(anchors, closed, (i + 1) % n);
        if i == 0 {
            pts.push(a_pt);
        }
        if a_out.length_sq() < 0.01 && b_in.length_sq() < 0.01 {
            if (b_pt - *pts.last().unwrap_or(&a_pt)).length_sq() > 0.01 {
                pts.push(b_pt);
            }
        } else {
            flatten_cubic(a_pt, a_pt + a_out, b_pt + b_in, b_pt, &mut pts);
        }
    }
    pts
}

fn filleted_out(anchors: &[Anchor], closed: bool, i: usize) -> (Pt, Pt) {
    let a = &anchors[i];
    if a.radius < 0.5 || !a.is_corner() {
        return (a.pt, a.h_out);
    }
    let n = anchors.len();
    let prev = if i == 0 {
        if closed {
            anchors[n - 1].pt
        } else {
            return (a.pt, a.h_out);
        }
    } else {
        anchors[i - 1].pt
    };
    let next = anchors[(i + 1) % n].pt;
    let (p1, p2) = fillet_pts(prev, a.pt, next, a.radius);
    (p1, p2 - p1)
}

fn filleted_in(anchors: &[Anchor], closed: bool, i: usize) -> (Pt, Pt) {
    let a = &anchors[i];
    if a.radius < 0.5 || !a.is_corner() {
        return (a.pt, a.h_in);
    }
    let n = anchors.len();
    let prev = if i == 0 {
        if closed {
            anchors[n - 1].pt
        } else {
            return (a.pt, a.h_in);
        }
    } else {
        anchors[i - 1].pt
    };
    let next = if i + 1 >= n {
        if closed {
            anchors[0].pt
        } else {
            return (a.pt, a.h_in);
        }
    } else {
        anchors[i + 1].pt
    };
    let (p1, p2) = fillet_pts(prev, a.pt, next, a.radius);
    (p2, p1 - p2)
}

fn fillet_pts(prev: Pt, corner: Pt, next: Pt, radius: f32) -> (Pt, Pt) {
    let vin = corner - prev;
    let vout = next - corner;
    let lin = vin.length().max(1e-6);
    let lout = vout.length().max(1e-6);
    let r = radius.min(lin * 0.45).min(lout * 0.45);
    (corner - vin * (r / lin), corner + vout * (r / lout))
}

/// SVG `d` that keeps cubics (`C`) instead of flattening to lines.
pub fn path_svg_d(anchors: &[Anchor], closed: bool) -> String {
    if anchors.is_empty() {
        return String::new();
    }
    let mut d = format!("M {:.3} {:.3}", anchors[0].pt.x, anchors[0].pt.y);
    let n = anchors.len();
    let segs = if closed { n } else { n.saturating_sub(1) };
    for i in 0..segs {
        let (a_pt, a_out) = filleted_out(anchors, closed, i % n);
        let (b_pt, b_in) = filleted_in(anchors, closed, (i + 1) % n);
        if i == 0 && (a_pt - anchors[0].pt).length_sq() > 0.01 {
            d = format!("M {:.3} {:.3}", a_pt.x, a_pt.y);
        }
        let seg = (b_pt - a_pt).length().max(1.0);
        let eps = 1.0_f32.max(seg * 0.02);
        if a_out.length() < eps && b_in.length() < eps {
            d.push_str(&format!(" L {:.3} {:.3}", b_pt.x, b_pt.y));
        } else {
            let c1 = a_pt + a_out;
            let c2 = b_pt + b_in;
            d.push_str(&format!(
                " C {:.3} {:.3} {:.3} {:.3} {:.3} {:.3}",
                c1.x, c1.y, c2.x, c2.y, b_pt.x, b_pt.y
            ));
        }
    }
    if closed {
        d.push_str(" Z");
    }
    d
}

fn default_true() -> bool {
    true
}

/// Live type: the string stays editable. Contours are a shaped cache.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TypeRun {
    pub origin: Pt,
    pub content: String,
    pub px: f32,
    pub tracking: f32,
    /// Line height in px. `0` means auto (`px * 1.2`).
    #[serde(default)]
    pub leading: f32,
    /// Absolute font path. Empty picks the first system sans.
    #[serde(default)]
    pub font: String,
    #[serde(default = "default_true")]
    pub kern: bool,
    #[serde(default = "default_true")]
    pub liga: bool,
    #[serde(default)]
    pub tnum: bool,
    #[serde(default)]
    pub smcp: bool,
    #[serde(skip)]
    pub contours: Vec<Vec<Pt>>,
}

impl Default for TypeRun {
    fn default() -> Self {
        Self {
            origin: Pt::ZERO,
            content: String::new(),
            px: 72.0,
            tracking: 0.0,
            leading: 0.0,
            font: String::new(),
            kern: true,
            liga: true,
            tnum: false,
            smcp: false,
            contours: vec![],
        }
    }
}

impl TypeRun {
    pub fn line_height(&self) -> f32 {
        if self.leading > 0.5 {
            self.leading
        } else {
            self.px.max(1.0) * 1.2
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Geom {
    Rect {
        origin: Pt,
        size: Pt,
        radius: f32,
    },
    Ellipse {
        center: Pt,
        radii: Pt,
    },
    Polygon {
        center: Pt,
        radii: Pt,
        sides: u32,
    },
    Star {
        center: Pt,
        outer: Pt,
        inner: f32,
        points: u32,
    },
    Line {
        a: Pt,
        b: Pt,
    },
    Path {
        anchors: Vec<Anchor>,
        closed: bool,
    },
    Text(TypeRun),
    Poly {
        contours: Vec<Vec<Pt>>,
        /// True = nonzero winding (fonts, SVG default). False = even-odd.
        #[serde(default)]
        winding: bool,
    },
}

impl Geom {
    pub fn contours(&self, segs: usize) -> Vec<Vec<Pt>> {
        match self {
            Geom::Rect {
                origin,
                size,
                radius,
            } => vec![rounded_rect(*origin, *size, *radius)],
            Geom::Ellipse { center, radii } => vec![ellipse_pts(*center, *radii, segs)],
            Geom::Polygon {
                center,
                radii,
                sides,
            } => vec![polygon_pts(*center, *radii, *sides)],
            Geom::Star {
                center,
                outer,
                inner,
                points,
            } => vec![star_pts(*center, *outer, *inner, *points)],
            Geom::Line { a, b } => vec![vec![*a, *b]],
            Geom::Path { anchors, closed } => {
                let pts = path_pts(anchors, *closed);
                if pts.len() < 2 { vec![] } else { vec![pts] }
            }
            Geom::Text(t) => t.contours.clone(),
            Geom::Poly { contours, .. } => contours.clone(),
        }
    }

    pub fn is_closed(&self) -> bool {
        match self {
            Geom::Line { .. } => false,
            Geom::Path { closed, .. } => *closed,
            _ => true,
        }
    }

    pub fn bbox(&self) -> Bounds {
        match self {
            Geom::Rect { origin, size, .. } => {
                return Bounds {
                    min: origin.min(*origin + *size),
                    max: origin.max(*origin + *size),
                };
            }
            Geom::Ellipse { center, radii } => {
                return Bounds {
                    min: *center - radii.abs(),
                    max: *center + radii.abs(),
                };
            }
            Geom::Line { a, b } => {
                return Bounds {
                    min: a.min(*b),
                    max: a.max(*b),
                };
            }
            _ => {}
        }
        let generated;
        let cs = match self {
            Geom::Text(t) => &t.contours,
            Geom::Poly { contours, .. } => contours,
            _ => {
                generated = self.contours(32);
                &generated
            }
        };
        let mut b = None;
        for c in cs {
            for p in c {
                match &mut b {
                    None => b = Some(Bounds::from_pt(*p)),
                    Some(bb) => bb.union_pt(*p),
                }
            }
        }
        if let Geom::Path { anchors, .. } = self {
            for a in anchors {
                match &mut b {
                    None => b = Some(Bounds::from_pt(a.pt)),
                    Some(bb) => {
                        bb.union_pt(a.pt);
                        bb.union_pt(a.pt + a.h_in);
                        bb.union_pt(a.pt + a.h_out);
                    }
                }
            }
        }
        if let Geom::Text(t) = self
            && b.as_ref().map(|bb| bb.is_empty()).unwrap_or(true)
        {
            let h = t.px.max(8.0);
            let w = (t.px * 0.45).max(12.0);
            let n = t.content.split('\n').count().max(1) as f32;
            return Bounds::from_min_size(
                Pt::new(t.origin.x, t.origin.y - t.px),
                Pt::new(w, h * n.max(1.0)),
            );
        }
        b.unwrap_or(Bounds::from_pt(Pt::ZERO))
    }

    pub fn translate(&mut self, d: Pt) {
        match self {
            Geom::Rect { origin, .. } => *origin += d,
            Geom::Ellipse { center, .. }
            | Geom::Polygon { center, .. }
            | Geom::Star { center, .. } => *center += d,
            Geom::Line { a, b } => {
                *a += d;
                *b += d;
            }
            Geom::Path { anchors, .. } => {
                for a in anchors {
                    a.pt += d;
                }
            }
            Geom::Text(t) => {
                t.origin += d;
                for c in &mut t.contours {
                    for p in c {
                        *p += d;
                    }
                }
            }
            Geom::Poly { contours, .. } => {
                for c in contours {
                    for p in c {
                        *p += d;
                    }
                }
            }
        }
    }

    pub fn map_into(&mut self, src: Bounds, dst: Bounds) {
        let sx = if src.width() < 1e-6 {
            1.0
        } else {
            dst.width() / src.width()
        };
        let sy = if src.height() < 1e-6 {
            1.0
        } else {
            dst.height() / src.height()
        };
        match self {
            Geom::Rect { origin, size, .. } => {
                *origin = src.map_pt(*origin, dst);
                size.x *= sx;
                size.y *= sy;
            }
            Geom::Ellipse { center, radii } => {
                *center = src.map_pt(*center, dst);
                radii.x *= sx;
                radii.y *= sy;
            }
            Geom::Polygon { center, radii, .. } => {
                *center = src.map_pt(*center, dst);
                radii.x *= sx;
                radii.y *= sy;
            }
            Geom::Star { center, outer, .. } => {
                *center = src.map_pt(*center, dst);
                outer.x *= sx;
                outer.y *= sy;
            }
            Geom::Line { a, b } => {
                *a = src.map_pt(*a, dst);
                *b = src.map_pt(*b, dst);
            }
            Geom::Path { anchors, .. } => {
                for a in anchors {
                    a.pt = src.map_pt(a.pt, dst);
                    a.h_in = Pt::new(a.h_in.x * sx, a.h_in.y * sy);
                    a.h_out = Pt::new(a.h_out.x * sx, a.h_out.y * sy);
                }
            }
            Geom::Text(t) => {
                t.origin = src.map_pt(t.origin, dst);
                t.px *= sy;
                t.tracking *= sx;
                t.leading *= sy;
                for c in &mut t.contours {
                    for p in c {
                        *p = src.map_pt(*p, dst);
                    }
                }
            }
            Geom::Poly { contours, .. } => {
                for c in contours {
                    for p in c {
                        *p = src.map_pt(*p, dst);
                    }
                }
            }
        }
    }

    /// Mirror across the bbox centre. Does not use inverted Bounds (width/height
    /// clamp to 0 and would collapse the object to a line).
    pub fn flip(&mut self, horizontal: bool) {
        let c = self.bbox().center();
        self.flip_about(c, horizontal);
    }

    pub fn flip_about(&mut self, c: Pt, horizontal: bool) {
        let fp = |p: Pt| {
            if horizontal {
                Pt::new(2.0 * c.x - p.x, p.y)
            } else {
                Pt::new(p.x, 2.0 * c.y - p.y)
            }
        };
        let fv = |v: Pt| {
            if horizontal {
                Pt::new(-v.x, v.y)
            } else {
                Pt::new(v.x, -v.y)
            }
        };
        match self {
            Geom::Rect { origin, size, .. } => {
                let a = fp(*origin);
                let b = fp(*origin + *size);
                let min = a.min(b);
                let max = a.max(b);
                *origin = min;
                *size = max - min;
            }
            Geom::Ellipse { center, .. }
            | Geom::Polygon { center, .. }
            | Geom::Star { center, .. } => {
                *center = fp(*center);
            }
            Geom::Line { a, b } => {
                *a = fp(*a);
                *b = fp(*b);
            }
            Geom::Path { anchors, .. } => {
                for a in anchors {
                    a.pt = fp(a.pt);
                    a.h_in = fv(a.h_in);
                    a.h_out = fv(a.h_out);
                }
            }
            Geom::Text(t) => {
                t.origin = fp(t.origin);
                for contour in &mut t.contours {
                    for p in contour {
                        *p = fp(*p);
                    }
                }
            }
            Geom::Poly { contours, .. } => {
                for contour in contours {
                    for p in contour {
                        *p = fp(*p);
                    }
                }
            }
        }
    }

    pub fn rotate_about(&mut self, origin: Pt, angle: f32) {
        let rot = |p: &mut Pt| *p = p.rotate_about(origin, angle);
        match self {
            Geom::Rect {
                origin: o, size, ..
            } => {
                // Keep the primitive. Callers add the angle to Shape.rotation.
                let c = *o + *size * 0.5;
                *o += c.rotate_about(origin, angle) - c;
            }
            Geom::Ellipse { center, .. } => rot(center),
            Geom::Polygon { center, .. } | Geom::Star { center, .. } => rot(center),
            Geom::Line { a, b } => {
                rot(a);
                rot(b);
            }
            Geom::Path { anchors, .. } => {
                for a in anchors {
                    a.pt = a.pt.rotate_about(origin, angle);
                    a.h_in = a.h_in.rotate(angle);
                    a.h_out = a.h_out.rotate(angle);
                }
            }
            Geom::Text(t) => {
                rot(&mut t.origin);
                for c in &mut t.contours {
                    for p in c {
                        rot(p);
                    }
                }
            }
            Geom::Poly { contours, .. } => {
                for c in contours {
                    for p in c {
                        rot(p);
                    }
                }
            }
        }
    }

    pub fn contains(&self, p: Pt) -> bool {
        if let Geom::Text(_) = self {
            return self.bbox().inflate(4.0).contains(p);
        }
        if !self.is_closed() {
            return false;
        }
        let mut inside = false;
        for pts in self.contours(64) {
            if point_in_poly(p, &pts) {
                inside = !inside;
            }
        }
        inside
    }

    pub fn dist_to_outline(&self, p: Pt) -> f32 {
        let mut best = f32::INFINITY;
        let closed = self.is_closed();
        for pts in self.contours(64) {
            let n = pts.len();
            if n < 2 {
                continue;
            }
            let segs = if closed { n } else { n - 1 };
            for i in 0..segs {
                best = best.min(seg_dist(p, pts[i], pts[(i + 1) % n]));
            }
        }
        best
    }

    /// Exact-ish conversion so the node tool can edit any vector.
    pub fn to_path(&self) -> Geom {
        match self {
            Geom::Path { .. } => self.clone(),
            Geom::Line { a, b } => Geom::Path {
                anchors: vec![Anchor::corner(*a), Anchor::corner(*b)],
                closed: false,
            },
            Geom::Rect {
                origin,
                size,
                radius,
            } => {
                let mut anchors = vec![
                    Anchor::corner(*origin),
                    Anchor::corner(Pt::new(origin.x + size.x, origin.y)),
                    Anchor::corner(Pt::new(origin.x + size.x, origin.y + size.y)),
                    Anchor::corner(Pt::new(origin.x, origin.y + size.y)),
                ];
                if *radius >= 0.5 {
                    for a in &mut anchors {
                        a.radius = *radius;
                    }
                }
                Geom::Path {
                    anchors,
                    closed: true,
                }
            }
            Geom::Ellipse { center, radii } => {
                let k = 0.552_284_8;
                let rx = radii.x;
                let ry = radii.y;
                let c = *center;
                Geom::Path {
                    anchors: vec![
                        Anchor {
                            pt: Pt::new(c.x + rx, c.y),
                            h_in: Pt::new(0.0, -k * ry),
                            h_out: Pt::new(0.0, k * ry),
                            radius: 0.0,
                        },
                        Anchor {
                            pt: Pt::new(c.x, c.y + ry),
                            h_in: Pt::new(k * rx, 0.0),
                            h_out: Pt::new(-k * rx, 0.0),
                            radius: 0.0,
                        },
                        Anchor {
                            pt: Pt::new(c.x - rx, c.y),
                            h_in: Pt::new(0.0, k * ry),
                            h_out: Pt::new(0.0, -k * ry),
                            radius: 0.0,
                        },
                        Anchor {
                            pt: Pt::new(c.x, c.y - ry),
                            h_in: Pt::new(-k * rx, 0.0),
                            h_out: Pt::new(k * rx, 0.0),
                            radius: 0.0,
                        },
                    ],
                    closed: true,
                }
            }
            other => {
                let closed = other.is_closed();
                let Some(pts) = other.contours(24).into_iter().next() else {
                    return Geom::Path {
                        anchors: vec![],
                        closed,
                    };
                };
                let mut pts = pts;
                if closed && pts.len() > 1 && (pts[0] - *pts.last().unwrap()).length() < 0.5 {
                    pts.pop();
                }
                Geom::Path {
                    anchors: pts.into_iter().map(Anchor::corner).collect(),
                    closed,
                }
            }
        }
    }

    pub fn kind_name(&self) -> &'static str {
        match self {
            Geom::Rect { .. } => "Rectangle",
            Geom::Ellipse { .. } => "Ellipse",
            Geom::Polygon { .. } => "Polygon",
            Geom::Star { .. } => "Star",
            Geom::Line { .. } => "Line",
            Geom::Path { closed: true, .. } => "Path",
            Geom::Path { .. } => "Curve",
            Geom::Text(_) => "Text",
            Geom::Poly { .. } => "Shape",
        }
    }
}

/// Closest point on a path segment, with parameter t in [0, 1] along the cubic.
pub fn closest_on_path(anchors: &[Anchor], closed: bool, p: Pt) -> Option<(usize, f32, f32)> {
    if anchors.len() < 2 {
        return None;
    }
    let segs = if closed {
        anchors.len()
    } else {
        anchors.len() - 1
    };
    let mut best = (f32::INFINITY, 0usize, 0.0);
    for i in 0..segs {
        let a = &anchors[i % anchors.len()];
        let b = &anchors[(i + 1) % anchors.len()];
        for s in 0..=CUBIC_STEPS {
            let t = s as f32 / CUBIC_STEPS as f32;
            let q = eval_cubic(a.pt, a.pt + a.h_out, b.pt + b.h_in, b.pt, t);
            let d = (q - p).length();
            if d < best.0 {
                best = (d, i, t);
            }
        }
    }
    Some((best.1, best.2, best.0))
}

/// Break at `index`. Closed → one open path. Open (not an end) → two paths.
pub fn break_path(
    anchors: &[Anchor],
    closed: bool,
    index: usize,
) -> Option<(Vec<Anchor>, Option<Vec<Anchor>>)> {
    if anchors.len() < 2 || index >= anchors.len() {
        return None;
    }
    if closed {
        let mut a = anchors.to_vec();
        a.rotate_left(index);
        Some((a, None))
    } else {
        if index == 0 || index + 1 >= anchors.len() {
            return None;
        }
        let left = anchors[..=index].to_vec();
        let right = anchors[index..].to_vec();
        Some((left, Some(right)))
    }
}

pub fn reverse_anchors(anchors: &mut [Anchor]) {
    anchors.reverse();
    for a in anchors.iter_mut() {
        std::mem::swap(&mut a.h_in, &mut a.h_out);
    }
}

pub fn insert_anchor(anchors: &mut Vec<Anchor>, closed: bool, p: Pt, slack: f32) -> Option<usize> {
    let (seg, t, dist) = closest_on_path(anchors, closed, p)?;
    if dist > slack {
        return None;
    }
    let a = anchors[seg % anchors.len()];
    let b = anchors[(seg + 1) % anchors.len()];
    let (left, right) = split_cubic(a.pt, a.pt + a.h_out, b.pt + b.h_in, b.pt, t);
    let n = anchors.len();
    anchors[seg % n].h_out = left[1] - left[0];
    let new = Anchor {
        pt: left[3],
        h_in: left[2] - left[3],
        h_out: right[1] - right[0],
        radius: 0.0,
    };
    let next = (seg + 1) % anchors.len();
    if next == 0 && closed {
        anchors[0].h_in = right[2] - right[3];
        anchors.push(new);
        Some(anchors.len() - 1)
    } else {
        anchors[next].h_in = right[2] - right[3];
        anchors.insert(next, new);
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_contains_center_not_outside() {
        let g = Geom::Rect {
            origin: Pt::new(0.0, 0.0),
            size: Pt::new(10.0, 10.0),
            radius: 0.0,
        };
        assert!(g.contains(Pt::new(5.0, 5.0)));
        assert!(!g.contains(Pt::new(20.0, 5.0)));
    }

    #[test]
    fn ellipse_bbox_and_area() {
        let g = Geom::Ellipse {
            center: Pt::new(0.0, 0.0),
            radii: Pt::new(10.0, 5.0),
        };
        let b = g.bbox();
        assert!((b.width() - 20.0).abs() < 0.2);
        assert!((b.height() - 10.0).abs() < 0.2);
        assert!(g.contains(Pt::ZERO));
        assert!(!g.contains(Pt::new(11.0, 0.0)));
    }

    #[test]
    fn map_into_scales_rect() {
        let mut g = Geom::Rect {
            origin: Pt::new(0.0, 0.0),
            size: Pt::new(10.0, 10.0),
            radius: 0.0,
        };
        let src = g.bbox();
        let dst = Bounds::from_min_size(Pt::new(0.0, 0.0), Pt::new(20.0, 10.0));
        g.map_into(src, dst);
        let b = g.bbox();
        assert!((b.width() - 20.0).abs() < 0.01);
        assert!((b.height() - 10.0).abs() < 0.01);
    }

    #[test]
    fn split_cubic_joins() {
        let p0 = Pt::new(0.0, 0.0);
        let c1 = Pt::new(0.0, 10.0);
        let c2 = Pt::new(10.0, 10.0);
        let p1 = Pt::new(10.0, 0.0);
        let (l, r) = split_cubic(p0, c1, c2, p1, 0.5);
        assert!((l[3].x - r[0].x).abs() < 1e-5);
        assert!((l[3].y - r[0].y).abs() < 1e-5);
    }

    #[test]
    fn ellipse_to_path_is_closed_four() {
        let g = Geom::Ellipse {
            center: Pt::new(10.0, 10.0),
            radii: Pt::new(8.0, 4.0),
        };
        let Geom::Path { anchors, closed } = g.to_path() else {
            panic!("path");
        };
        assert!(closed);
        assert_eq!(anchors.len(), 4);
    }

    #[test]
    fn break_open_path_splits() {
        let a: Vec<Anchor> = (0..4)
            .map(|i| Anchor::corner(Pt::new(i as f32 * 10.0, 0.0)))
            .collect();
        let (l, r) = break_path(&a, false, 1).unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(r.unwrap().len(), 3);
    }

    #[test]
    fn star_has_ten_vertices() {
        let g = Geom::Star {
            center: Pt::ZERO,
            outer: Pt::splat(20.0),
            inner: 0.4,
            points: 5,
        };
        assert_eq!(g.contours(8)[0].len(), 10);
    }

    #[test]
    fn poly_area_square() {
        let pts = vec![
            Pt::new(0.0, 0.0),
            Pt::new(10.0, 0.0),
            Pt::new(10.0, 10.0),
            Pt::new(0.0, 10.0),
        ];
        assert!((poly_area(&pts) - 100.0).abs() < 1e-3);
    }

    #[test]
    fn pen_click_stays_corner_under_threshold() {
        let mut a = Anchor::corner(Pt::new(10.0, 10.0));
        apply_pen_smooth(&mut a, Pt::new(1.0, 0.0), 1.0, false, false);
        assert!(a.is_corner(), "1px drag at 100% zoom is a corner");
        apply_pen_smooth(&mut a, Pt::new(20.0, 0.0), 1.0, false, false);
        assert!(!a.is_corner(), "20px drag is a smooth point");
        assert!((a.h_out.x - 20.0).abs() < 0.01);
        assert!((a.h_in.x + 20.0).abs() < 0.01);
    }

    #[test]
    fn pen_alt_breaks_handle_symmetry() {
        let mut a = Anchor::corner(Pt::ZERO);
        apply_pen_smooth(&mut a, Pt::new(30.0, 0.0), 1.0, true, false);
        assert!((a.h_out.x - 30.0).abs() < 0.01);
        assert!(a.h_in.length_sq() < 0.01);
    }

    #[test]
    fn path_svg_emits_cubic() {
        let anchors = vec![
            Anchor::corner(Pt::new(0.0, 0.0)),
            Anchor::smooth(Pt::new(40.0, 0.0), Pt::new(10.0, 8.0)),
        ];
        let d = path_svg_d(&anchors, false);
        assert!(d.contains('C') || d.contains(" C"), "{d}");
        assert!(!d.contains(" L ") || d.starts_with('M'));
    }

    #[test]
    fn path_svg_junk_handles_are_lines() {
        let mut a0 = Anchor::corner(Pt::new(0.0, 0.0));
        a0.h_out = Pt::new(0.3, 0.0);
        let mut a1 = Anchor::corner(Pt::new(80.0, 0.0));
        a1.h_in = Pt::new(-0.4, 0.0);
        let d = path_svg_d(&[a0, a1], false);
        assert!(d.contains(" L "), "{d}");
        assert!(!d.contains(" C "), "{d}");
    }

    #[test]
    fn rotate_keeps_rect_primitive() {
        let mut g = Geom::Rect {
            origin: Pt::new(10.0, 10.0),
            size: Pt::new(40.0, 20.0),
            radius: 6.0,
        };
        g.rotate_about(Pt::new(30.0, 20.0), 0.4);
        match g {
            Geom::Rect { radius, .. } => assert!((radius - 6.0).abs() < 1e-4),
            other => panic!("baked away: {other:?}"),
        }
    }

    #[test]
    fn per_corner_radii_change_outline() {
        let sharp = rounded_rect_corners(Pt::ZERO, Pt::new(80.0, 40.0), [0.0; 4]);
        let round = rounded_rect_corners(Pt::ZERO, Pt::new(80.0, 40.0), [12.0, 0.0, 0.0, 0.0]);
        assert!(round.len() > sharp.len());
    }

    #[test]
    fn flip_h_keeps_path_width() {
        let mut g = Geom::Path {
            anchors: vec![
                Anchor::corner(Pt::new(10.0, 10.0)),
                Anchor::corner(Pt::new(90.0, 10.0)),
                Anchor::corner(Pt::new(90.0, 50.0)),
                Anchor::corner(Pt::new(10.0, 50.0)),
            ],
            closed: false,
        };
        let w = g.bbox().width();
        g.flip(true);
        assert!(
            (g.bbox().width() - w).abs() < 0.01,
            "width {}",
            g.bbox().width()
        );
        let Geom::Path { anchors, .. } = &g else {
            panic!("path")
        };
        assert!((anchors[0].pt.x - 90.0).abs() < 0.01);
        assert!((anchors[1].pt.x - 10.0).abs() < 0.01);
    }

    #[test]
    fn inverted_bounds_width_is_zero_so_flip_must_not_use_it() {
        let b = Bounds {
            min: Pt::new(80.0, 10.0),
            max: Pt::new(10.0, 50.0),
        };
        assert!(b.width() < 1e-3);
    }
}
