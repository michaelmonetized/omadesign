//! Analytic pixel areas of the existing native compositor's floating-point paths.
use super::shape_mask::ShapeMask;
use crate::{
    deep_color::storage,
    resources::{self, Allocation, CancellationCheck},
};
use tiny_skia::{Path, PathSegment, Point};

const CURVE_ERROR: f64 = 0.00001;

#[derive(Clone, Copy, Default)]
struct Edge {
    a: [f64; 2],
    b: [f64; 2],
    group: usize,
}

pub(super) struct Clip {
    pub path: std::sync::Arc<Path>,
    pub transform: tiny_skia::Transform,
    pub nonzero: bool,
}
impl Edge {
    /// Locate an edge crossing. Args: scanline coordinate. Returns its unrounded horizontal coordinate.
    fn x(self, y: f64) -> f64 {
        self.a[0] + (y - self.a[1]) * (self.b[0] - self.a[0]) / (self.b[1] - self.a[1])
    }
    /// Inspect an edge direction. Args: none. Returns horizontal change per vertical unit.
    fn slope(self) -> f64 {
        (self.b[0] - self.a[0]) / (self.b[1] - self.a[1])
    }
    /// Orient an edge crossing. Args: none. Returns its signed winding contribution.
    fn winding(self) -> i64 {
        if self.b[1] > self.a[1] { 1 } else { -1 }
    }
}
/// Transform existing path geometry. Args: source point and device transform. Returns a finite double coordinate.
fn point(mut p: Point, transform: tiny_skia::Transform) -> Result<[f64; 2], String> {
    transform.map_point(&mut p);
    if !p.x.is_finite() || !p.y.is_finite() {
        return Err("Native vector path has nonfinite geometry".into());
    }
    Ok([p.x as f64, p.y as f64])
}
/// Split a curve leg. Args: endpoints. Returns their midpoint without integer quantization.
fn midpoint(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]
}
/// Bound curve flattening. Args: control point and chord. Returns perpendicular distance in device pixels.
fn deviation(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let length = d[0].hypot(d[1]);
    if length == 0. {
        (p[0] - a[0]).hypot(p[1] - a[1])
    } else {
        ((p[0] - a[0]) * d[1] - (p[1] - a[1]) * d[0]).abs() / length
    }
}
/// Retain a nonhorizontal boundary. Args: admitted builder, endpoints and cancellation. Returns after appending or an admission error.
fn edge(
    out: &mut storage::Builder<Edge>,
    a: [f64; 2],
    b: [f64; 2],
    check: &dyn CancellationCheck,
) -> Result<(), String> {
    check.check()?;
    if a[1] != b[1] {
        out.extend(&[Edge { a, b, group: 0 }])?;
    }
    Ok(())
}
/// Subdivide a native cubic. Args: admitted builder, controls, depth and cancellation. Returns bounded-error line geometry.
fn cubic(
    out: &mut storage::Builder<Edge>,
    p: [[f64; 2]; 4],
    depth: usize,
    check: &dyn CancellationCheck,
) -> Result<(), String> {
    check.check()?;
    let chord = [p[3][0] - p[0][0], p[3][1] - p[0][1]];
    let length_sq = chord[0] * chord[0] + chord[1] * chord[1];
    let ordered = [p[1], p[2]].into_iter().all(|point| {
        let projection = (point[0] - p[0][0]) * chord[0] + (point[1] - p[0][1]) * chord[1];
        projection >= 0. && projection <= length_sq
    });
    if ordered && deviation(p[1], p[0], p[3]).max(deviation(p[2], p[0], p[3])) <= CURVE_ERROR {
        return edge(out, p[0], p[3], check);
    }
    if depth >= 26 {
        return Err("Native vector curve exceeds the precise coverage subdivision budget".into());
    }
    let a = midpoint(p[0], p[1]);
    let b = midpoint(p[1], p[2]);
    let c = midpoint(p[2], p[3]);
    let d = midpoint(a, b);
    let e = midpoint(b, c);
    let f = midpoint(d, e);
    cubic(out, [p[0], a, d, f], depth + 1, check)?;
    cubic(out, [f, e, c, p[3]], depth + 1, check)
}
/// Collect closed float fill boundaries. Args: existing path, device transform and cancellation. Returns admitted edges.
fn edges(
    path: &Path,
    transform: tiny_skia::Transform,
    check: &dyn CancellationCheck,
) -> Result<std::sync::Arc<Allocation<Edge>>, String> {
    let mut out = storage::Builder::new()?;
    let mut first = None;
    let mut last = None;
    for segment in path.segments() {
        check.check()?;
        match segment {
            PathSegment::MoveTo(p) => {
                if let (Some(a), Some(b)) = (last, first) {
                    edge(&mut out, a, b, check)?;
                }
                let p = point(p, transform)?;
                first = Some(p);
                last = Some(p);
            }
            PathSegment::LineTo(p) => {
                let p = point(p, transform)?;
                if let Some(a) = last {
                    edge(&mut out, a, p, check)?;
                }
                last = Some(p);
            }
            PathSegment::QuadTo(control, end) => {
                let c = point(control, transform)?;
                let b = point(end, transform)?;
                if let Some(a) = last {
                    cubic(
                        &mut out,
                        [
                            a,
                            [(a[0] + 2. * c[0]) / 3., (a[1] + 2. * c[1]) / 3.],
                            [(b[0] + 2. * c[0]) / 3., (b[1] + 2. * c[1]) / 3.],
                            b,
                        ],
                        0,
                        check,
                    )?;
                }
                last = Some(b);
            }
            PathSegment::CubicTo(b, c, d) => {
                let b = point(b, transform)?;
                let c = point(c, transform)?;
                let d = point(d, transform)?;
                if let Some(a) = last {
                    cubic(&mut out, [a, b, c, d], 0, check)?;
                }
                last = Some(d);
            }
            PathSegment::Close => {
                if let (Some(a), Some(b)) = (last, first) {
                    edge(&mut out, a, b, check)?;
                }
                last = first;
            }
        }
    }
    if let (Some(a), Some(b)) = (last, first) {
        edge(&mut out, a, b, check)?;
    }
    out.finish()
}
/// Integrate a clipped linear crossing. Args: endpoint offsets and strip height. Returns exact trapezoid area.
fn clamped_integral(a: f64, b: f64, height: f64) -> f64 {
    let primitive = |v: f64| {
        if v <= 0. {
            0.
        } else if v < 1. {
            0.5 * v * v
        } else {
            v - 0.5
        }
    };
    if a == b {
        a.clamp(0., 1.) * height
    } else {
        (primitive(b) - primitive(a)) / (b - a) * height
    }
}

/// Integrate vector coverage without a byte mask or fixed subpixel sample grid.
/// Args: device-space path, fill rule, output extent, cancellation and row consumer.
/// Returns after complete analytic line coverage; curves are flattened within1e-5 device pixels.
pub(super) fn rows(
    path: &Path,
    transform: tiny_skia::Transform,
    nonzero: bool,
    width: u32,
    height: u32,
    check: &dyn CancellationCheck,
    consume: impl FnMut(u32, &[f64]) -> Result<(), String>,
) -> Result<(), String> {
    rows_clipped(
        path,
        transform,
        nonzero,
        &[],
        tiny_skia::Transform::identity(),
        None,
        width,
        height,
        check,
        consume,
    )
}

/// Intersect all geometric boundaries before integrating native mask weights.
/// Args: original path/placement/rule, frame clips, optional bitmap field, output extent, cancellation and consumer.
/// Returns jointly covered area; masks use degree-two triangle quadrature within each bilinear texel region.
pub(super) fn rows_clipped(
    path: &Path,
    transform: tiny_skia::Transform,
    nonzero: bool,
    clips: &[Option<Clip>],
    clip_view: tiny_skia::Transform,
    mask: Option<&ShapeMask<'_>>,
    width: u32,
    height: u32,
    check: &dyn CancellationCheck,
    mut consume: impl FnMut(u32, &[f64]) -> Result<(), String>,
) -> Result<(), String> {
    check.check()?;
    let groups = 1 + clips.len() + usize::from(mask.is_some());
    if groups > 66 {
        return Err("Native geometric clipping exceeds the validated frame depth".into());
    }
    let mut rules = [false; 66];
    rules[0] = nonzero;
    let initial = edges(path, transform, check)?;
    let edges = if clips.is_empty() && mask.is_none() {
        initial
    } else {
        let mut out = storage::Builder::with_capacity(initial.len())?;
        out.extend(initial.as_slice())?;
        drop(initial);
        for (index, clip) in clips.iter().enumerate() {
            check.check()?;
            let clip = clip.as_ref().ok_or("Missing native frame clip")?;
            rules[index + 1] = clip.nonzero;
            let part = edges(&clip.path, clip_view.pre_concat(clip.transform), check)?;
            if part.is_empty() {
                return Ok(());
            }
            for e in part.iter() {
                out.extend(&[Edge {
                    group: index + 1,
                    ..*e
                }])?;
            }
        }
        if let Some(mask) = mask {
            rules[groups - 1] = true;
            let (w, h) = (mask.pixels.w as f32, mask.pixels.h as f32);
            let map = |x, y| point(Point::from_xy(x, y), mask.transform);
            let corners = [map(0., 0.)?, map(w, 0.)?, map(w, h)?, map(0., h)?];
            for i in 0..4 {
                out.extend(&[Edge {
                    a: corners[i],
                    b: corners[(i + 1) % 4],
                    group: groups - 1,
                }])?;
            }
            for x in 0..mask.pixels.w {
                check.check()?;
                out.extend(&[Edge {
                    a: map(x as f32 + 0.5, 0.)?,
                    b: map(x as f32 + 0.5, h)?,
                    group: usize::MAX,
                }])?;
            }
            for y in 0..mask.pixels.h {
                check.check()?;
                out.extend(&[Edge {
                    a: map(0., y as f32 + 0.5)?,
                    b: map(w, y as f32 + 0.5)?,
                    group: usize::MAX,
                }])?;
            }
        }
        out.finish()?
    };
    if edges.is_empty() {
        return Ok(());
    }
    let top = edges
        .iter()
        .flat_map(|e| [e.a[1], e.b[1]])
        .fold(f64::INFINITY, f64::min)
        .floor()
        .max(0.)
        .min(height as f64) as u32;
    let bottom = edges
        .iter()
        .flat_map(|e| [e.a[1], e.b[1]])
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil()
        .max(0.)
        .min(height as f64) as u32;
    let mut coverage = resources::global().allocate_now::<f64, _>(width as usize, check)?;
    let mut active = resources::global().allocate_now::<Edge, _>(edges.len(), check)?;
    for y in top..bottom {
        check.check()?;
        coverage.as_mut_slice().fill(0.);
        let lo = y as f64;
        let hi = lo + 1.;
        let mut count = 0;
        let mut boundaries = storage::Builder::new()?;
        boundaries.extend(&[lo, hi])?;
        for &e in edges.iter() {
            check.check()?;
            for v in [e.a[1], e.b[1]] {
                if v > lo && v < hi {
                    boundaries.extend(&[v])?;
                }
            }
            if e.a[1] != e.b[1] && e.a[1].min(e.b[1]) < hi && e.a[1].max(e.b[1]) > lo {
                active.as_mut_slice()[count] = e;
                count += 1;
            }
        }
        for i in 0..count {
            check.check()?;
            let a = active[i];
            for b in &active[i + 1..count] {
                let d = a.slope() - b.slope();
                if d != 0. {
                    let v = lo + (b.x(lo) - a.x(lo)) / d;
                    if v > lo
                        && v < hi
                        && v > a.a[1].min(a.b[1]).max(b.a[1].min(b.b[1]))
                        && v < a.a[1].max(a.b[1]).min(b.a[1].max(b.b[1]))
                    {
                        boundaries.extend(&[v])?;
                    }
                }
            }
        }
        let boundaries = boundaries.finish()?;
        let mut boundaries = std::sync::Arc::try_unwrap(boundaries)
            .map_err(|_| "Shared native coverage boundaries")?;
        boundaries.as_mut_slice().sort_unstable_by(f64::total_cmp);
        for band in boundaries.windows(2) {
            check.check()?;
            let (a, b) = (band[0], band[1]);
            if a >= b {
                continue;
            }
            let middle = (a + b) * 0.5;
            active.as_mut_slice()[..count]
                .sort_unstable_by(|a, b| a.x(middle).total_cmp(&b.x(middle)));
            let mut winding = [0i64; 66];
            let inside = |winding: &[i64]| {
                winding.iter().zip(&rules).all(|(&v, &nonzero)| {
                    if nonzero {
                        v != 0
                    } else {
                        v.rem_euclid(2) != 0
                    }
                })
            };
            let mut left: Option<Edge> = None;
            for &edge in &active[..count] {
                if middle < edge.a[1].min(edge.b[1]) || middle >= edge.a[1].max(edge.b[1]) {
                    continue;
                }
                let was = inside(&winding[..groups]);
                if edge.group != usize::MAX {
                    winding[edge.group] += edge.winding();
                }
                let now = inside(&winding[..groups]);
                if was && (!now || mask.is_some()) {
                    let left = left
                        .take()
                        .ok_or("Native coverage winding is inconsistent")?;
                    let start = left.x(a).min(left.x(b)).floor().max(0.).min(width as f64) as usize;
                    let end = edge.x(a).max(edge.x(b)).ceil().max(0.).min(width as f64) as usize;
                    for x in start..end {
                        if x % 4096 == 0 {
                            check.check()?;
                        }
                        coverage.as_mut_slice()[x] += if let Some(mask) = mask {
                            weighted_area(left, edge, a, b, x as f64, mask)
                        } else {
                            clamped_integral(edge.x(a) - x as f64, edge.x(b) - x as f64, b - a)
                                - clamped_integral(
                                    left.x(a) - x as f64,
                                    left.x(b) - x as f64,
                                    b - a,
                                )
                        };
                    }
                }
                if now && (!was || mask.is_some()) {
                    left = Some(edge);
                }
            }
        }
        consume(y, coverage.as_slice())?;
    }
    check.check()
}

/// Integrate a bilinear mask over the actual covered part of one pixel.
/// Args: interval edges, vertical band, pixel column and source sampler. Returns weighted area without a mask average.
fn weighted_area(
    left: Edge,
    right: Edge,
    top: f64,
    bottom: f64,
    x: f64,
    mask: &ShapeMask<'_>,
) -> f64 {
    let mut polygon = [[0.; 2]; 8];
    polygon[..4].copy_from_slice(&[
        [left.x(top), top],
        [right.x(top), top],
        [right.x(bottom), bottom],
        [left.x(bottom), bottom],
    ]);
    let mut count = 4;
    for (boundary, greater) in [(x, true), (x + 1., false)] {
        let mut clipped = [[0.; 2]; 8];
        let mut n = 0;
        if count == 0 {
            return 0.;
        }
        let mut a = polygon[count - 1];
        for b in polygon[..count].iter().copied() {
            let ain = if greater {
                a[0] >= boundary
            } else {
                a[0] <= boundary
            };
            let bin = if greater {
                b[0] >= boundary
            } else {
                b[0] <= boundary
            };
            if ain != bin {
                let t = (boundary - a[0]) / (b[0] - a[0]);
                clipped[n] = [boundary, a[1] + t * (b[1] - a[1])];
                n += 1;
            }
            if bin {
                clipped[n] = b;
                n += 1;
            }
            a = b;
        }
        polygon = clipped;
        count = n;
    }
    let mut total = 0.;
    for i in 1..count.saturating_sub(1) {
        let (a, b, c) = (polygon[0], polygon[i], polygon[i + 1]);
        let area = ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() * 0.5;
        for (wa, wb, wc) in [
            (2. / 3., 1. / 6., 1. / 6.),
            (1. / 6., 2. / 3., 1. / 6.),
            (1. / 6., 1. / 6., 2. / 3.),
        ] {
            total += area / 3.
                * mask.at([
                    a[0] * wa + b[0] * wb + c[0] * wc,
                    a[1] * wa + b[1] * wb + c[1] * wc,
                ]);
        }
    }
    total
}
