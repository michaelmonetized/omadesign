//! Raster to vector. Binary and colour traces, no extra crates.

use crate::color::Rgba;
use crate::document::Pixels;
use crate::geom::{poly_area, Geom, Pt};
use std::collections::HashMap;

/// Trace knobs. Defaults match a logo on white or a transparent PNG.
#[derive(Clone, Copy, Debug)]
pub struct TraceOpts {
    /// 1 = black/white by luminance. 2..=16 = colour buckets.
    pub colors: u32,
    /// Luminance cutoff when `colors == 1`. Darker pixels become fill.
    pub threshold: f32,
    /// Ramer–Douglas–Peucker epsilon in pixels.
    pub smoothness: f32,
    /// Skip near-white / empty buckets so paper does not become a rectangle.
    pub ignore_white: bool,
    /// Drop contours smaller than this area (px²).
    pub min_area: f32,
}

impl Default for TraceOpts {
    fn default() -> Self {
        Self {
            colors: 1,
            threshold: 0.55,
            smoothness: 1.5,
            ignore_white: true,
            min_area: 8.0,
        }
    }
}

/// One filled path from a trace.
#[derive(Clone, Debug)]
pub struct Traced {
    pub geom: Geom,
    pub color: Rgba,
}

/// Trace `px` into filled polygons.
pub fn trace(px: &Pixels, opts: TraceOpts) -> Vec<Traced> {
    if px.w == 0 || px.h == 0 || px.data.len() < 4 {
        return vec![];
    }
    let (src, scale) = downsample(px, 1600);
    let colors = opts.colors.clamp(1, 16);
    let mut out = if colors <= 1 {
        let mask = mono_mask(&src, opts);
        contours_to_traced(&mask, src.w, src.h, opts, Rgba::rgb(0x11, 0x11, 0x11))
            .into_iter()
            .collect()
    } else {
        color_trace(&src, opts, colors as usize)
    };
    if (scale - 1.0).abs() > 1e-4 {
        for t in &mut out {
            scale_geom(&mut t.geom, scale);
        }
    }
    out
}

struct Raster {
    w: u32,
    h: u32,
    data: Vec<u8>,
}

fn downsample(px: &Pixels, max_edge: u32) -> (Raster, f32) {
    let long = px.w.max(px.h);
    if long <= max_edge {
        return (
            Raster {
                w: px.w,
                h: px.h,
                data: px.data.clone(),
            },
            1.0,
        );
    }
    let k = max_edge as f32 / long as f32;
    let dw = (px.w as f32 * k).round().max(1.0) as u32;
    let dh = (px.h as f32 * k).round().max(1.0) as u32;
    let mut data = vec![0u8; dw as usize * dh as usize * 4];
    let sx = px.w as f32 / dw as f32;
    let sy = px.h as f32 / dh as f32;
    for y in 0..dh {
        let y0 = ((y as f32 + 0.5) * sy).floor() as u32;
        let y0 = y0.min(px.h - 1);
        for x in 0..dw {
            let x0 = ((x as f32 + 0.5) * sx).floor() as u32;
            let x0 = x0.min(px.w - 1);
            let si = ((y0 * px.w + x0) * 4) as usize;
            let di = ((y * dw + x) * 4) as usize;
            data[di..di + 4].copy_from_slice(&px.data[si..si + 4]);
        }
    }
    (Raster { w: dw, h: dh, data }, 1.0 / k)
}

fn luma(r: u8, g: u8, b: u8) -> f32 {
    (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0
}

fn mono_mask(src: &Raster, opts: TraceOpts) -> Vec<u8> {
    let n = src.w as usize * src.h as usize;
    let mut mask = vec![0u8; n];
    let t = opts.threshold.clamp(0.02, 0.98);
    for (i, px) in src.data.chunks_exact(4).enumerate() {
        let a = px[3];
        if a < 16 {
            continue;
        }
        let y = luma(px[0], px[1], px[2]);
        if opts.ignore_white && y > 0.92 && a > 200 {
            continue;
        }
        if y < t || a < 250 && y < t + 0.15 {
            mask[i] = 1;
        }
    }
    mask
}

fn color_trace(src: &Raster, opts: TraceOpts, k: usize) -> Vec<Traced> {
    let (palette, index) = palettize(src, k);
    let n = src.w as usize * src.h as usize;
    let mut out = Vec::new();
    for (ci, color) in palette.iter().enumerate() {
        let y = luma(color.r, color.g, color.b);
        if opts.ignore_white && y > 0.92 {
            continue;
        }
        let mut mask = vec![0u8; n];
        let mut any = false;
        for (i, idx) in index.iter().enumerate() {
            if *idx == ci as u16 {
                mask[i] = 1;
                any = true;
            }
        }
        if !any {
            continue;
        }
        out.extend(contours_to_traced(&mask, src.w, src.h, opts, *color));
    }
    out
}

fn palettize(src: &Raster, k: usize) -> (Vec<Rgba>, Vec<u16>) {
    let mut samples: Vec<[u8; 3]> = Vec::new();
    for px in src.data.chunks_exact(4) {
        if px[3] < 16 {
            continue;
        }
        samples.push([px[0], px[1], px[2]]);
    }
    if samples.is_empty() {
        return (vec![], vec![u16::MAX; src.w as usize * src.h as usize]);
    }
    let palette_rgb = median_cut(&samples, k.max(1));
    let palette: Vec<Rgba> = palette_rgb
        .iter()
        .map(|c| Rgba::rgb(c[0], c[1], c[2]))
        .collect();
    let mut index = vec![u16::MAX; src.w as usize * src.h as usize];
    for (i, px) in src.data.chunks_exact(4).enumerate() {
        if px[3] < 16 {
            continue;
        }
        let mut best = 0usize;
        let mut bd = u32::MAX;
        for (ci, c) in palette_rgb.iter().enumerate() {
            let dr = px[0] as i32 - c[0] as i32;
            let dg = px[1] as i32 - c[1] as i32;
            let db = px[2] as i32 - c[2] as i32;
            let d = (dr * dr + dg * dg + db * db) as u32;
            if d < bd {
                bd = d;
                best = ci;
            }
        }
        index[i] = best as u16;
    }
    (palette, index)
}

fn median_cut(samples: &[[u8; 3]], k: usize) -> Vec<[u8; 3]> {
    struct Bucket {
        pts: Vec<[u8; 3]>,
    }
    let mut buckets = vec![Bucket {
        pts: samples.to_vec(),
    }];
    while buckets.len() < k {
        let (split_i, axis, range) = buckets
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let (ax, r) = channel_range(&b.pts);
                (i, ax, r)
            })
            .max_by_key(|(_, _, r)| *r)
            .unwrap_or((0, 0, 0));
        if range < 2 || buckets[split_i].pts.len() < 2 {
            break;
        }
        let mut pts = std::mem::take(&mut buckets[split_i].pts);
        pts.sort_by_key(|p| p[axis]);
        let mid = pts.len() / 2;
        let right = pts.split_off(mid);
        buckets[split_i].pts = pts;
        buckets.push(Bucket { pts: right });
    }
    buckets
        .iter()
        .filter(|b| !b.pts.is_empty())
        .map(|b| average_rgb(&b.pts))
        .collect()
}

fn channel_range(pts: &[[u8; 3]]) -> (usize, u8) {
    let mut min = [255u8; 3];
    let mut max = [0u8; 3];
    for p in pts {
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    let mut axis = 0;
    let mut range = 0u8;
    for i in 0..3 {
        let r = max[i].saturating_sub(min[i]);
        if r >= range {
            range = r;
            axis = i;
        }
    }
    (axis, range)
}

fn average_rgb(pts: &[[u8; 3]]) -> [u8; 3] {
    let n = pts.len() as u64;
    if n == 0 {
        return [0, 0, 0];
    }
    let mut s = [0u64; 3];
    for p in pts {
        s[0] += p[0] as u64;
        s[1] += p[1] as u64;
        s[2] += p[2] as u64;
    }
    [(s[0] / n) as u8, (s[1] / n) as u8, (s[2] / n) as u8]
}

fn contours_to_traced(
    mask: &[u8],
    w: u32,
    h: u32,
    opts: TraceOpts,
    color: Rgba,
) -> Vec<Traced> {
    let raw = trace_mask(mask, w, h);
    let eps = opts.smoothness.max(0.2);
    let min_a = opts.min_area.max(1.0);
    let mut contours = Vec::new();
    for c in raw {
        let s = rdp_closed(&c, eps);
        if s.len() < 3 {
            continue;
        }
        if poly_area(&s).abs() < min_a {
            continue;
        }
        contours.push(s);
    }
    if contours.is_empty() {
        return vec![];
    }
    vec![Traced {
        geom: Geom::Poly {
            contours,
            winding: false,
        },
        color,
    }]
}

/// Pixel-edge walk. Interior stays on the left so outers are CCW and holes CW.
fn trace_mask(mask: &[u8], w: u32, h: u32) -> Vec<Vec<Pt>> {
    let fg = |x: i32, y: i32| -> bool {
        if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
            return false;
        }
        mask[(y as u32 * w + x as u32) as usize] != 0
    };
    let mut adj: HashMap<(i32, i32), Vec<(i32, i32)>> = HashMap::new();
    let mut push = |a: (i32, i32), b: (i32, i32)| {
        adj.entry(a).or_default().push(b);
    };
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            if !fg(x, y) {
                continue;
            }
            if !fg(x - 1, y) {
                push((x, y), (x, y + 1));
            }
            if !fg(x + 1, y) {
                push((x + 1, y + 1), (x + 1, y));
            }
            if !fg(x, y - 1) {
                push((x + 1, y), (x, y));
            }
            if !fg(x, y + 1) {
                push((x, y + 1), (x + 1, y + 1));
            }
        }
    }
    let mut loops = Vec::new();
    while let Some((start, _)) = adj.iter().find(|(_, v)| !v.is_empty()) {
        let start = *start;
        let mut ring: Vec<(i32, i32)> = vec![start];
        let mut cur = start;
        loop {
            let next = {
                let outs = adj.get_mut(&cur);
                match outs {
                    Some(v) if !v.is_empty() => Some(v.remove(0)),
                    _ => None,
                }
            };
            let Some(next) = next else {
                break;
            };
            if next == start {
                break;
            }
            ring.push(next);
            cur = next;
            if ring.len() > (w as usize + 2) * (h as usize + 2) {
                break;
            }
        }
        if ring.len() >= 4 {
            let mut pts: Vec<Pt> = ring
                .into_iter()
                .map(|(x, y)| Pt::new(x as f32, y as f32))
                .collect();
            collapse_collinear(&mut pts);
            if pts.len() >= 3 {
                loops.push(pts);
            }
        }
    }
    loops
}

fn collapse_collinear(pts: &mut Vec<Pt>) {
    if pts.len() < 4 {
        return;
    }
    let mut out = Vec::with_capacity(pts.len());
    let n = pts.len();
    for i in 0..n {
        let a = pts[(i + n - 1) % n];
        let b = pts[i];
        let c = pts[(i + 1) % n];
        let ab = b - a;
        let bc = c - b;
        let cross = ab.x * bc.y - ab.y * bc.x;
        if cross.abs() > 1e-3 {
            out.push(b);
        }
    }
    if out.len() >= 3 {
        *pts = out;
    }
}

fn rdp_closed(pts: &[Pt], eps: f32) -> Vec<Pt> {
    if pts.len() < 4 {
        return pts.to_vec();
    }
    let mut ring = pts.to_vec();
    ring.push(pts[0]);
    let s = rdp(&ring, eps);
    let mut out = s;
    if out.len() >= 2 && (out[0] - *out.last().unwrap()).length() < 1e-3 {
        out.pop();
    }
    if out.len() < 3 {
        pts.to_vec()
    } else {
        out
    }
}

fn rdp(pts: &[Pt], eps: f32) -> Vec<Pt> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let first = pts[0];
    let last = pts[pts.len() - 1];
    let mut max_d = 0.0f32;
    let mut idx = 0usize;
    for (i, p) in pts.iter().enumerate().skip(1).take(pts.len() - 2) {
        let d = dist_seg(*p, first, last);
        if d > max_d {
            max_d = d;
            idx = i;
        }
    }
    if max_d > eps {
        let left = rdp(&pts[..=idx], eps);
        let right = rdp(&pts[idx..], eps);
        let mut out = left;
        out.pop();
        out.extend(right);
        out
    } else {
        vec![first, last]
    }
}

fn dist_seg(p: Pt, a: Pt, b: Pt) -> f32 {
    let ab = b - a;
    let l2 = ab.length_sq();
    if l2 < 1e-12 {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

fn scale_geom(g: &mut Geom, s: f32) {
    if let Geom::Poly { contours, .. } = g {
        for c in contours {
            for p in c {
                p.x *= s;
                p.y *= s;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Pixels;

    fn solid_rect(w: u32, h: u32, x0: u32, y0: u32, x1: u32, y1: u32, rgb: [u8; 3]) -> Pixels {
        let mut data = vec![255u8; w as usize * h as usize * 4];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                if x >= x0 && x < x1 && y >= y0 && y < y1 {
                    data[i] = rgb[0];
                    data[i + 1] = rgb[1];
                    data[i + 2] = rgb[2];
                    data[i + 3] = 255;
                } else {
                    data[i] = 255;
                    data[i + 1] = 255;
                    data[i + 2] = 255;
                    data[i + 3] = 255;
                }
            }
        }
        Pixels::from_rgba(w, h, data).unwrap()
    }

    #[test]
    fn traces_a_black_square() {
        let px = solid_rect(32, 32, 8, 8, 24, 24, [0, 0, 0]);
        let out = trace(&px, TraceOpts::default());
        assert_eq!(out.len(), 1);
        let Geom::Poly { contours, .. } = &out[0].geom else {
            panic!("poly");
        };
        assert!(!contours.is_empty());
        let b = out[0].geom.bbox();
        assert!((b.min.x - 8.0).abs() < 1.5, "min.x {}", b.min.x);
        assert!((b.min.y - 8.0).abs() < 1.5, "min.y {}", b.min.y);
        assert!((b.max.x - 24.0).abs() < 1.5, "max.x {}", b.max.x);
        assert!((b.max.y - 24.0).abs() < 1.5, "max.y {}", b.max.y);
        let area: f32 = contours.iter().map(|c| poly_area(c).abs()).sum();
        assert!((area - 256.0).abs() < 8.0, "area {area}");
        assert!(
            contours[0].len() <= 6,
            "square should collapse, got {} pts",
            contours[0].len()
        );
    }

    #[test]
    fn ignores_white_paper() {
        let px = solid_rect(16, 16, 0, 0, 16, 16, [255, 255, 255]);
        let out = trace(&px, TraceOpts::default());
        assert!(out.is_empty());
    }

    #[test]
    fn colour_trace_splits_red_and_blue() {
        let mut data = vec![255u8; 32 * 16 * 4];
        for y in 0..16u32 {
            for x in 0..32u32 {
                let i = ((y * 32 + x) * 4) as usize;
                if x < 16 {
                    data[i] = 200;
                    data[i + 1] = 20;
                    data[i + 2] = 20;
                } else {
                    data[i] = 20;
                    data[i + 1] = 20;
                    data[i + 2] = 200;
                }
            }
        }
        let px = Pixels::from_rgba(32, 16, data).unwrap();
        let out = trace(
            &px,
            TraceOpts {
                colors: 2,
                ignore_white: false,
                smoothness: 1.0,
                min_area: 4.0,
                ..TraceOpts::default()
            },
        );
        assert_eq!(out.len(), 2, "expected two colour shapes, got {}", out.len());
        let reds = out.iter().filter(|t| t.color.r > t.color.b).count();
        let blues = out.iter().filter(|t| t.color.b > t.color.r).count();
        assert_eq!(reds, 1);
        assert_eq!(blues, 1);
    }

    #[test]
    fn hole_in_a_ring() {
        let mut data = vec![255u8; 24 * 24 * 4];
        for y in 0..24u32 {
            for x in 0..24u32 {
                let i = ((y * 24 + x) * 4) as usize;
                let on_ring = x >= 4 && x < 20 && y >= 4 && y < 20;
                let in_hole = x >= 9 && x < 15 && y >= 9 && y < 15;
                if on_ring && !in_hole {
                    data[i] = 0;
                    data[i + 1] = 0;
                    data[i + 2] = 0;
                }
            }
        }
        let px = Pixels::from_rgba(24, 24, data).unwrap();
        let out = trace(&px, TraceOpts::default());
        assert_eq!(out.len(), 1);
        let Geom::Poly { contours, .. } = &out[0].geom else {
            panic!("poly");
        };
        assert!(
            contours.len() >= 2,
            "ring should be outer + hole, got {}",
            contours.len()
        );
        let mut areas: Vec<f32> = contours.iter().map(|c| poly_area(c).abs()).collect();
        areas.sort_by(|a, b| b.partial_cmp(a).unwrap());
        assert!(
            areas[0] > areas[1] * 1.5,
            "outer should dwarf the hole, areas {areas:?}"
        );
    }
}
