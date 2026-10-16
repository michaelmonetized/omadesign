//! Analytic pixel areas of the existing native compositor's floating-point paths.
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
        out.extend(&[Edge { a, b }])?;
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
    mut consume: impl FnMut(u32, &[f64]) -> Result<(), String>,
) -> Result<(), String> {
    check.check()?;
    let edges = edges(path, transform, check)?;
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
            if e.a[1].min(e.b[1]) < hi && e.a[1].max(e.b[1]) > lo {
                active.as_mut_slice()[count] = e;
                count += 1;
                for v in [e.a[1], e.b[1]] {
                    if v > lo && v < hi {
                        boundaries.extend(&[v])?;
                    }
                }
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
            let mut winding = 0i64;
            let mut left = None;
            for &edge in &active[..count] {
                if middle < edge.a[1].min(edge.b[1]) || middle >= edge.a[1].max(edge.b[1]) {
                    continue;
                }
                let was = if nonzero {
                    winding != 0
                } else {
                    winding.rem_euclid(2) != 0
                };
                winding += edge.winding();
                let inside = if nonzero {
                    winding != 0
                } else {
                    winding.rem_euclid(2) != 0
                };
                if !was && inside {
                    left = Some(edge);
                } else if was && !inside {
                    let left = left
                        .take()
                        .ok_or("Native coverage winding is inconsistent")?;
                    let start = left.x(a).min(left.x(b)).floor().max(0.).min(width as f64) as usize;
                    let end = edge.x(a).max(edge.x(b)).ceil().max(0.).min(width as f64) as usize;
                    for x in start..end {
                        if x % 4096 == 0 {
                            check.check()?;
                        }
                        coverage.as_mut_slice()[x] +=
                            clamped_integral(edge.x(a) - x as f64, edge.x(b) - x as f64, b - a)
                                - clamped_integral(
                                    left.x(a) - x as f64,
                                    left.x(b) - x as f64,
                                    b - a,
                                );
                    }
                }
            }
        }
        consume(y, coverage.as_slice())?;
    }
    check.check()
}
