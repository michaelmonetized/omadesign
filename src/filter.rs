//! SVG filter effects on a layer. Rasterised for the canvas, emitted as `<filter>` on export.

use crate::color::Rgba;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tiny_skia::Pixmap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FilterStack {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub items: Vec<Fx>,
}

fn default_true() -> bool {
    true
}

impl Default for FilterStack {
    fn default() -> Self {
        Self {
            enabled: true,
            items: vec![],
        }
    }
}

impl FilterStack {
    pub fn is_empty(&self) -> bool {
        !self.enabled || self.items.is_empty()
    }

    pub fn active(&self) -> bool {
        self.enabled && !self.items.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Fx {
    Blur {
        std: f32,
    },
    Shadow {
        dx: f32,
        dy: f32,
        blur: f32,
        color: Rgba,
    },
    InnerShadow {
        dx: f32,
        dy: f32,
        blur: f32,
        color: Rgba,
    },
    Offset {
        dx: f32,
        dy: f32,
    },
    Morphology {
        erode: bool,
        radius: f32,
    },
    Saturate {
        amount: f32,
    },
    HueRotate {
        degrees: f32,
    },
    Brightness {
        amount: f32,
    },
    Contrast {
        amount: f32,
    },
    Invert {
        amount: f32,
    },
    ColorMatrix {
        values: [f32; 20],
    },
    Turbulence {
        fractal: bool,
        base: f32,
        octaves: u32,
        seed: u32,
    },
    Displacement {
        scale: f32,
        x_ch: u8,
        y_ch: u8,
    },
    /// Frosted refraction: noise drives an feDisplacementMap.
    AppleGlass {
        scale: f32,
        frequency: f32,
    },
}

impl Fx {
    pub fn name(&self) -> &'static str {
        match self {
            Fx::Blur { .. } => "Gaussian blur",
            Fx::Shadow { .. } => "Drop shadow",
            Fx::InnerShadow { .. } => "Inner shadow",
            Fx::Offset { .. } => "Offset",
            Fx::Morphology { erode: true, .. } => "Erode",
            Fx::Morphology { .. } => "Dilate",
            Fx::Saturate { .. } => "Saturate",
            Fx::HueRotate { .. } => "Hue rotate",
            Fx::Brightness { .. } => "Brightness",
            Fx::Contrast { .. } => "Contrast",
            Fx::Invert { .. } => "Invert",
            Fx::ColorMatrix { .. } => "Color matrix",
            Fx::Turbulence { .. } => "Turbulence",
            Fx::Displacement { .. } => "Displacement",
            Fx::AppleGlass { .. } => "Apple glass",
        }
    }

    pub fn catalog() -> &'static [(&'static str, fn() -> Fx)] {
        &[
            ("Gaussian blur", || Fx::Blur { std: 8.0 }),
            ("Drop shadow", || Fx::Shadow {
                dx: 6.0,
                dy: 8.0,
                blur: 10.0,
                color: Rgba::new(0, 0, 0, 160),
            }),
            ("Inner shadow", || Fx::InnerShadow {
                dx: 3.0,
                dy: 4.0,
                blur: 6.0,
                color: Rgba::new(0, 0, 0, 180),
            }),
            ("Offset", || Fx::Offset { dx: 8.0, dy: 8.0 }),
            ("Dilate", || Fx::Morphology {
                erode: false,
                radius: 2.0,
            }),
            ("Erode", || Fx::Morphology {
                erode: true,
                radius: 2.0,
            }),
            ("Saturate", || Fx::Saturate { amount: 0.0 }),
            ("Hue rotate", || Fx::HueRotate { degrees: 45.0 }),
            ("Brightness", || Fx::Brightness { amount: 1.2 }),
            ("Contrast", || Fx::Contrast { amount: 1.2 }),
            ("Invert", || Fx::Invert { amount: 1.0 }),
            ("Color matrix", || Fx::ColorMatrix {
                values: identity_matrix(),
            }),
            ("Turbulence", || Fx::Turbulence {
                fractal: true,
                base: 0.04,
                octaves: 3,
                seed: 1,
            }),
            ("Displacement", || Fx::Displacement {
                scale: 20.0,
                x_ch: 0,
                y_ch: 1,
            }),
            ("Apple glass", || Fx::AppleGlass {
                scale: 18.0,
                frequency: 0.04,
            }),
        ]
    }
}

fn identity_matrix() -> [f32; 20] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        1.0, 0.0,
    ]
}

pub fn apply(pm: &mut Pixmap, stack: &FilterStack) {
    if stack.is_empty() {
        return;
    }
    for fx in &stack.items {
        apply_one(pm, fx);
    }
}

fn apply_one(pm: &mut Pixmap, fx: &Fx) {
    match *fx {
        Fx::Blur { std } => blur(pm, std.max(0.0)),
        Fx::Shadow {
            dx,
            dy,
            blur: b,
            color,
        } => drop_shadow(pm, dx, dy, b, color),
        Fx::InnerShadow {
            dx,
            dy,
            blur: b,
            color,
        } => inner_shadow(pm, dx, dy, b, color),
        Fx::Offset { dx, dy } => offset(pm, dx, dy),
        Fx::Morphology { erode, radius } => morphology(pm, erode, radius.max(0.0)),
        Fx::Saturate { amount } => color_matrix(pm, &saturate_matrix(amount)),
        Fx::HueRotate { degrees } => color_matrix(pm, &hue_matrix(degrees)),
        Fx::Brightness { amount } => color_matrix(pm, &brightness_matrix(amount)),
        Fx::Contrast { amount } => color_matrix(pm, &contrast_matrix(amount)),
        Fx::Invert { amount } => color_matrix(pm, &invert_matrix(amount.clamp(0.0, 1.0))),
        Fx::ColorMatrix { values } => color_matrix(pm, &values),
        Fx::Turbulence {
            fractal,
            base,
            octaves,
            seed,
        } => turbulence(pm, fractal, base.max(0.001), octaves.max(1), seed),
        Fx::Displacement { scale, x_ch, y_ch } => displacement(pm, scale, x_ch.min(3), y_ch.min(3)),
        Fx::AppleGlass { scale, frequency } => apple_glass(pm, scale, frequency),
    }
}

fn blur(pm: &mut Pixmap, sigma: f32) {
    if sigma < 0.15 {
        return;
    }
    let r = (sigma * 1.35).round().max(1.0) as i32;
    box_blur(pm, r);
    box_blur(pm, (r - 1).max(1));
}

fn box_blur(pm: &mut Pixmap, radius: i32) {
    let r = radius.max(1);
    let n = 2 * r + 1;
    let w = pm.width() as i32;
    let h = pm.height() as i32;
    let src = pm.data().to_vec();
    let mut tmp = vec![0u8; src.len()];
    // Horizontal.
    for y in 0..h {
        for c in 0..4 {
            let mut acc = 0i32;
            for k in -r..=r {
                acc += sample(&src, w, h, k, y, c) as i32;
            }
            for x in 0..w {
                tmp[idx(w, x, y, c)] = (acc / n) as u8;
                acc += sample(&src, w, h, x + r + 1, y, c) as i32;
                acc -= sample(&src, w, h, x - r, y, c) as i32;
            }
        }
    }
    // Vertical.
    let mut out = vec![0u8; src.len()];
    for x in 0..w {
        for c in 0..4 {
            let mut acc = 0i32;
            for k in -r..=r {
                acc += sample(&tmp, w, h, x, k, c) as i32;
            }
            for y in 0..h {
                out[idx(w, x, y, c)] = (acc / n) as u8;
                acc += sample(&tmp, w, h, x, y + r + 1, c) as i32;
                acc -= sample(&tmp, w, h, x, y - r, c) as i32;
            }
        }
    }
    pm.data_mut().copy_from_slice(&out);
}

fn sample(data: &[u8], w: i32, h: i32, x: i32, y: i32, c: i32) -> u8 {
    if x < 0 || y < 0 || x >= w || y >= h {
        0
    } else {
        data[idx(w, x, y, c)]
    }
}

fn idx(w: i32, x: i32, y: i32, c: i32) -> usize {
    ((y * w + x) * 4 + c) as usize
}

fn offset(pm: &mut Pixmap, dx: f32, dy: f32) {
    let w = pm.width() as i32;
    let h = pm.height() as i32;
    let ox = dx.round() as i32;
    let oy = dy.round() as i32;
    if ox == 0 && oy == 0 {
        return;
    }
    let src = pm.data().to_vec();
    let mut out = vec![0u8; src.len()];
    for y in 0..h {
        for x in 0..w {
            let sx = x - ox;
            let sy = y - oy;
            if sx >= 0 && sy >= 0 && sx < w && sy < h {
                let di = idx(w, x, y, 0);
                let si = idx(w, sx, sy, 0);
                out[di..di + 4].copy_from_slice(&src[si..si + 4]);
            }
        }
    }
    pm.data_mut().copy_from_slice(&out);
}

fn drop_shadow(pm: &mut Pixmap, dx: f32, dy: f32, sigma: f32, color: Rgba) {
    let src = pm.clone();
    tint_alpha(pm, color);
    blur(pm, sigma);
    offset(pm, dx, dy);
    blit_over(pm, &src);
}

fn inner_shadow(pm: &mut Pixmap, dx: f32, dy: f32, sigma: f32, color: Rgba) {
    let src = pm.clone();
    invert_alpha(pm);
    offset(pm, dx, dy);
    blur(pm, sigma);
    tint_alpha(pm, color);
    clip_to_alpha(pm, &src);
    let shadow = pm.clone();
    pm.data_mut().copy_from_slice(src.data());
    blit_over(pm, &shadow);
}

fn tint_alpha(pm: &mut Pixmap, color: Rgba) {
    let cr = color.r as f32 / 255.0;
    let cg = color.g as f32 / 255.0;
    let cb = color.b as f32 / 255.0;
    let ca = color.a as f32 / 255.0;
    let data = pm.data_mut();
    for px in data.chunks_exact_mut(4) {
        let a = (px[3] as f32 / 255.0) * ca;
        px[0] = (cr * a * 255.0).round() as u8;
        px[1] = (cg * a * 255.0).round() as u8;
        px[2] = (cb * a * 255.0).round() as u8;
        px[3] = (a * 255.0).round() as u8;
    }
}

fn invert_alpha(pm: &mut Pixmap) {
    let data = pm.data_mut();
    for px in data.chunks_exact_mut(4) {
        px[3] = 255u8.saturating_sub(px[3]);
        let a = px[3] as f32 / 255.0;
        px[0] = (px[0] as f32 * a) as u8;
        px[1] = (px[1] as f32 * a) as u8;
        px[2] = (px[2] as f32 * a) as u8;
    }
}

fn clip_to_alpha(pm: &mut Pixmap, mask: &Pixmap) {
    let m = mask.data();
    let data = pm.data_mut();
    for (i, px) in data.chunks_exact_mut(4).enumerate() {
        let ma = m[i * 4 + 3] as f32 / 255.0;
        px[0] = (px[0] as f32 * ma) as u8;
        px[1] = (px[1] as f32 * ma) as u8;
        px[2] = (px[2] as f32 * ma) as u8;
        px[3] = (px[3] as f32 * ma) as u8;
    }
}

fn blit_over(dst: &mut Pixmap, src: &Pixmap) {
    let s = src.data();
    let d = dst.data_mut();
    for (di, si) in d.chunks_exact_mut(4).zip(s.chunks_exact(4)) {
        let sa = si[3] as f32 / 255.0;
        let da = di[3] as f32 / 255.0;
        let out_a = sa + da * (1.0 - sa);
        if out_a < 1e-6 {
            di.copy_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        for c in 0..3 {
            let sc = si[c] as f32;
            let dc = di[c] as f32;
            di[c] = (sc + dc * (1.0 - sa)).round().clamp(0.0, 255.0) as u8;
        }
        di[3] = (out_a * 255.0).round() as u8;
    }
}

fn morphology(pm: &mut Pixmap, erode: bool, radius: f32) {
    let r = radius.round().clamp(0.0, 64.0) as i32;
    if r <= 0 {
        return;
    }
    let w = pm.width() as usize;
    let h = pm.height() as usize;
    if w == 0 || h == 0 {
        return;
    }
    let src = pm.data().to_vec();
    let Some((bx0, by0, bx1, by1)) = opaque_bounds(&src, w, h) else {
        return;
    };
    let x0 = bx0.saturating_sub(r as usize);
    let y0 = by0.saturating_sub(r as usize);
    let x1 = (bx1 + r as usize).min(w - 1);
    let y1 = (by1 + r as usize).min(h - 1);
    let hy0 = y0.saturating_sub(r as usize);
    let hy1 = (y1 + r as usize).min(h - 1);
    let horizontal = extremum_axis(&src, w, h, x0, x1, hy0, hy1, r, erode, true);
    let winners = extremum_axis(&horizontal, w, h, x0, x1, y0, y1, r, erode, false);
    let mut out = src.clone();
    for y in y0..=y1 {
        for x in x0..=x1 {
            let i = (y * w + x) * 4;
            if erode {
                write_erode(&mut out[i..i + 4], &src[i..i + 4], winners[i + 3]);
            } else {
                out[i..i + 4].copy_from_slice(&winners[i..i + 4]);
            }
        }
    }
    pm.data_mut().copy_from_slice(&out);
}

fn opaque_bounds(src: &[u8], w: usize, h: usize) -> Option<(usize, usize, usize, usize)> {
    let mut min_x = w;
    let mut min_y = h;
    let mut max_x = 0usize;
    let mut max_y = 0usize;
    let mut any = false;
    for y in 0..h {
        for x in 0..w {
            if src[(y * w + x) * 4 + 3] > 0 {
                any = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    any.then_some((min_x, min_y, max_x, max_y))
}

fn extremum_axis(
    src: &[u8],
    w: usize,
    h: usize,
    x0: usize,
    x1: usize,
    y0: usize,
    y1: usize,
    r: i32,
    erode: bool,
    horizontal: bool,
) -> Vec<u8> {
    let mut out = src.to_vec();
    if horizontal {
        let rows: Vec<(usize, Vec<u8>)> = (y0..=y1)
            .into_par_iter()
            .map(|y| {
                let mut row = vec![0u8; (x1 - x0 + 1) * 4];
                for (n, x) in (x0..=x1).enumerate() {
                    let px = nearest_extremum(src, w, h, x as i32, y as i32, r, erode, true);
                    row[n * 4..n * 4 + 4].copy_from_slice(&px);
                }
                (y, row)
            })
            .collect();
        for (y, row) in rows {
            for (n, x) in (x0..=x1).enumerate() {
                let i = (y * w + x) * 4;
                out[i..i + 4].copy_from_slice(&row[n * 4..n * 4 + 4]);
            }
        }
    } else {
        let cols: Vec<(usize, Vec<u8>)> = (x0..=x1)
            .into_par_iter()
            .map(|x| {
                let mut col = vec![0u8; (y1 - y0 + 1) * 4];
                for (n, y) in (y0..=y1).enumerate() {
                    let px = nearest_extremum(src, w, h, x as i32, y as i32, r, erode, false);
                    col[n * 4..n * 4 + 4].copy_from_slice(&px);
                }
                (x, col)
            })
            .collect();
        for (x, col) in cols {
            for (n, y) in (y0..=y1).enumerate() {
                let i = (y * w + x) * 4;
                out[i..i + 4].copy_from_slice(&col[n * 4..n * 4 + 4]);
            }
        }
    }
    out
}

fn nearest_extremum(
    src: &[u8],
    w: usize,
    h: usize,
    x: i32,
    y: i32,
    r: i32,
    erode: bool,
    horizontal: bool,
) -> [u8; 4] {
    let mut best = pixel_at(src, w, h, x, y);
    let mut best_a = best[3];
    for d in 1..=r {
        for sign in [-1, 1] {
            let (sx, sy) = if horizontal {
                (x + sign * d, y)
            } else {
                (x, y + sign * d)
            };
            let px = pixel_at(src, w, h, sx, sy);
            let better = if erode {
                px[3] < best_a
            } else {
                px[3] > best_a
            };
            if better {
                best = px;
                best_a = px[3];
            }
        }
    }
    best
}

fn pixel_at(src: &[u8], w: usize, h: usize, x: i32, y: i32) -> [u8; 4] {
    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
        return [0, 0, 0, 0];
    }
    let i = (y as usize * w + x as usize) * 4;
    [src[i], src[i + 1], src[i + 2], src[i + 3]]
}

fn write_erode(dst: &mut [u8], src: &[u8], new_a: u8) {
    let old_a = src[3];
    if new_a >= old_a {
        dst.copy_from_slice(&src[..4]);
    } else if new_a == 0 || old_a == 0 {
        dst.fill(0);
    } else {
        for c in 0..3 {
            dst[c] = ((src[c] as u16 * new_a as u16) / old_a as u16) as u8;
        }
        dst[3] = new_a;
    }
}

fn color_matrix(pm: &mut Pixmap, m: &[f32; 20]) {
    let data = pm.data_mut();
    for px in data.chunks_exact_mut(4) {
        let a = px[3] as f32 / 255.0;
        if a < 1e-6 {
            continue;
        }
        let r = (px[0] as f32 / 255.0) / a.max(1e-6);
        let g = (px[1] as f32 / 255.0) / a.max(1e-6);
        let b = (px[2] as f32 / 255.0) / a.max(1e-6);
        let nr = m[0] * r + m[1] * g + m[2] * b + m[3] * a + m[4];
        let ng = m[5] * r + m[6] * g + m[7] * b + m[8] * a + m[9];
        let nb = m[10] * r + m[11] * g + m[12] * b + m[13] * a + m[14];
        let na = (m[15] * r + m[16] * g + m[17] * b + m[18] * a + m[19]).clamp(0.0, 1.0);
        px[0] = (nr.clamp(0.0, 1.0) * na * 255.0).round() as u8;
        px[1] = (ng.clamp(0.0, 1.0) * na * 255.0).round() as u8;
        px[2] = (nb.clamp(0.0, 1.0) * na * 255.0).round() as u8;
        px[3] = (na * 255.0).round() as u8;
    }
}

fn saturate_matrix(s: f32) -> [f32; 20] {
    // SVG feColorMatrix saturate
    let s = s.max(0.0);
    let ir = 0.213;
    let ig = 0.715;
    let ib = 0.072;
    [
        ir * (1.0 - s) + s,
        ig * (1.0 - s),
        ib * (1.0 - s),
        0.0,
        0.0,
        ir * (1.0 - s),
        ig * (1.0 - s) + s,
        ib * (1.0 - s),
        0.0,
        0.0,
        ir * (1.0 - s),
        ig * (1.0 - s),
        ib * (1.0 - s) + s,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
    ]
}

fn hue_matrix(deg: f32) -> [f32; 20] {
    let a = deg.to_radians();
    let c = a.cos();
    let s = a.sin();
    [
        0.213 + 0.787 * c - 0.213 * s,
        0.715 - 0.715 * c - 0.715 * s,
        0.072 - 0.072 * c + 0.928 * s,
        0.0,
        0.0,
        0.213 - 0.213 * c + 0.143 * s,
        0.715 + 0.285 * c + 0.140 * s,
        0.072 - 0.072 * c - 0.283 * s,
        0.0,
        0.0,
        0.213 - 0.213 * c - 0.787 * s,
        0.715 - 0.715 * c + 0.715 * s,
        0.072 + 0.928 * c + 0.072 * s,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
    ]
}

fn brightness_matrix(a: f32) -> [f32; 20] {
    [
        a, 0.0, 0.0, 0.0, 0.0, 0.0, a, 0.0, 0.0, 0.0, 0.0, 0.0, a, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        0.0,
    ]
}

fn contrast_matrix(a: f32) -> [f32; 20] {
    let t = 0.5 * (1.0 - a);
    [
        a, 0.0, 0.0, 0.0, t, 0.0, a, 0.0, 0.0, t, 0.0, 0.0, a, 0.0, t, 0.0, 0.0, 0.0, 1.0, 0.0,
    ]
}

fn invert_matrix(t: f32) -> [f32; 20] {
    let s = 1.0 - 2.0 * t;
    let o = t;
    [
        s, 0.0, 0.0, 0.0, o, 0.0, s, 0.0, 0.0, o, 0.0, 0.0, s, 0.0, o, 0.0, 0.0, 0.0, 1.0, 0.0,
    ]
}

fn turbulence(pm: &mut Pixmap, fractal: bool, base: f32, octaves: u32, seed: u32) {
    let w = pm.width();
    let h = pm.height();
    let data = pm.data_mut();
    for y in 0..h {
        for x in 0..w {
            let mut amp = 1.0f32;
            let mut freq = base;
            let mut v = 0.0f32;
            let mut max = 0.0;
            for o in 0..octaves {
                let n = noise(x as f32 * freq, y as f32 * freq, seed.wrapping_add(o * 19));
                let t = if fractal { n * 0.5 + 0.5 } else { n.abs() };
                v += t * amp;
                max += amp;
                amp *= 0.5;
                freq *= 2.0;
            }
            let g = (v / max.max(1e-6) * 255.0).round().clamp(0.0, 255.0) as u8;
            let i = ((y * w + x) * 4) as usize;
            data[i] = g;
            data[i + 1] = g;
            data[i + 2] = g;
            data[i + 3] = 255;
        }
    }
}

fn noise(x: f32, y: f32, seed: u32) -> f32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sy = fy * fy * (3.0 - 2.0 * fy);
    let n00 = hash(x0, y0, seed);
    let n10 = hash(x0 + 1, y0, seed);
    let n01 = hash(x0, y0 + 1, seed);
    let n11 = hash(x0 + 1, y0 + 1, seed);
    let nx0 = n00 + (n10 - n00) * sx;
    let nx1 = n01 + (n11 - n01) * sx;
    nx0 + (nx1 - nx0) * sy
}

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut n = (x as u32)
        .wrapping_mul(374761393)
        .wrapping_add((y as u32).wrapping_mul(668265263))
        .wrapping_add(seed.wrapping_mul(1274126177));
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    n = n ^ (n >> 16);
    (n as f32 / u32::MAX as f32) * 2.0 - 1.0
}

fn displacement(pm: &mut Pixmap, scale: f32, x_ch: u8, y_ch: u8) {
    let w = pm.width() as i32;
    let h = pm.height() as i32;
    let src = pm.data().to_vec();
    let mut out = vec![0u8; src.len()];
    let xc = x_ch.min(3) as i32;
    let yc = y_ch.min(3) as i32;
    for y in 0..h {
        for x in 0..w {
            let i = idx(w, x, y, 0);
            let dx = (src[i + xc as usize] as f32 / 255.0 - 0.5) * scale;
            let dy = (src[i + yc as usize] as f32 / 255.0 - 0.5) * scale;
            let sx = (x as f32 + dx).round() as i32;
            let sy = (y as f32 + dy).round() as i32;
            if sx >= 0 && sy >= 0 && sx < w && sy < h {
                let si = idx(w, sx, sy, 0);
                out[i..i + 4].copy_from_slice(&src[si..si + 4]);
            }
        }
    }
    pm.data_mut().copy_from_slice(&out);
}

fn apple_glass(pm: &mut Pixmap, scale: f32, frequency: f32) {
    if scale.abs() < 0.05 {
        return;
    }
    let w = pm.width() as i32;
    let h = pm.height() as i32;
    let src = pm.data().to_vec();
    let mut out = vec![0u8; src.len()];
    let frequency = frequency.max(0.001);
    for y in 0..h {
        for x in 0..w {
            let nx = (x as f32 * frequency).floor() as i32;
            let ny = (y as f32 * frequency).floor() as i32;
            let dx = hash(nx, ny, 2) * scale;
            let dy = hash(nx, ny, 9) * scale;
            let sx = (x as f32 + dx).round() as i32;
            let sy = (y as f32 + dy).round() as i32;
            let i = idx(w, x, y, 0);
            if sx >= 0 && sy >= 0 && sx < w && sy < h {
                let si = idx(w, sx, sy, 0);
                out[i..i + 4].copy_from_slice(&src[si..si + 4]);
            }
        }
    }
    pm.data_mut().copy_from_slice(&out);
}

/// Extra user-space padding a stack needs so blur/offset is not clipped.
pub fn svg_pad(stack: &FilterStack) -> f32 {
    if stack.is_empty() {
        return 0.0;
    }
    let mut pad = 8.0f32;
    for fx in &stack.items {
        match fx {
            Fx::Blur { std } => pad += std.max(0.0) * 4.0,
            Fx::Shadow { dx, dy, blur, .. } | Fx::InnerShadow { dx, dy, blur, .. } => {
                pad += blur.max(0.0) * 4.0 + dx.abs() + dy.abs();
            }
            Fx::Offset { dx, dy } => pad += dx.abs() + dy.abs(),
            Fx::Morphology { radius, .. } => pad += radius.max(0.0),
            Fx::Displacement { scale, .. } | Fx::AppleGlass { scale, .. } => pad += scale.abs(),
            _ => {}
        }
    }
    pad
}

/// SVG `<filter>` for this stack. `region` is `[x, y, width, height]` in user space.
pub fn svg_filter(id: &str, stack: &FilterStack, region: [f32; 4]) -> Option<String> {
    if stack.is_empty() {
        return None;
    }
    let mut body = String::new();
    let mut last = "SourceGraphic".to_string();
    let mut n = 0u32;
    let mut next = || {
        n += 1;
        format!("a{n}")
    };
    for fx in &stack.items {
        let out = next();
        match fx {
            Fx::Blur { std } => {
                body.push_str(&format!(
                    "<feGaussianBlur in=\"{last}\" stdDeviation=\"{:.3}\" result=\"{out}\"/>\n",
                    std.max(0.0)
                ));
            }
            Fx::Shadow {
                dx,
                dy,
                blur,
                color,
            } => {
                // SVG 1.1 chain. feDropShadow is SVG 2; many Linux viewers drop the
                // whole filter when they see it, and you get the raw shape.
                let blur_id = next();
                let off = next();
                let flood = next();
                let sh = next();
                body.push_str(&format!(
                    "<feGaussianBlur in=\"{last}\" stdDeviation=\"{:.3}\" result=\"{blur_id}\"/>\n\
                     <feOffset in=\"{blur_id}\" dx=\"{dx:.2}\" dy=\"{dy:.2}\" result=\"{off}\"/>\n\
                     <feFlood flood-color=\"#{:02X}{:02X}{:02X}\" flood-opacity=\"{:.3}\" result=\"{flood}\"/>\n\
                     <feComposite in=\"{flood}\" in2=\"{off}\" operator=\"in\" result=\"{sh}\"/>\n\
                     <feComposite in=\"{last}\" in2=\"{sh}\" operator=\"over\" result=\"{out}\"/>\n",
                    blur.max(0.0),
                    color.r,
                    color.g,
                    color.b,
                    color.a as f32 / 255.0
                ));
            }
            Fx::InnerShadow {
                dx,
                dy,
                blur,
                color,
            } => {
                let a = next();
                let b = next();
                let c = next();
                body.push_str(&format!(
                    "<feOffset in=\"{last}\" dx=\"{dx:.2}\" dy=\"{dy:.2}\" result=\"{a}\"/>\n\
                     <feGaussianBlur in=\"{a}\" stdDeviation=\"{:.3}\" result=\"{b}\"/>\n\
                     <feComposite in=\"{b}\" in2=\"{last}\" operator=\"arithmetic\" k2=\"-1\" k3=\"1\" result=\"{c}\"/>\n\
                     <feFlood flood-color=\"{}\" flood-opacity=\"{:.3}\"/>\n\
                     <feComposite in2=\"{c}\" operator=\"in\" result=\"{out}\"/>\n\
                     <feComposite in=\"{out}\" in2=\"{last}\" operator=\"over\" result=\"{out}\"/>\n",
                    blur.max(0.0),
                    format!("#{:02X}{:02X}{:02X}", color.r, color.g, color.b),
                    color.a as f32 / 255.0
                ));
            }
            Fx::Offset { dx, dy } => {
                body.push_str(&format!(
                    "<feOffset in=\"{last}\" dx=\"{dx:.2}\" dy=\"{dy:.2}\" result=\"{out}\"/>\n"
                ));
            }
            Fx::Morphology { erode, radius } => {
                let op = if *erode { "erode" } else { "dilate" };
                body.push_str(&format!(
                    "<feMorphology in=\"{last}\" operator=\"{op}\" radius=\"{:.2}\" result=\"{out}\"/>\n",
                    radius.max(0.0)
                ));
            }
            Fx::Saturate { amount } => {
                body.push_str(&format!(
                    "<feColorMatrix in=\"{last}\" type=\"saturate\" values=\"{amount:.3}\" result=\"{out}\"/>\n"
                ));
            }
            Fx::HueRotate { degrees } => {
                body.push_str(&format!(
                    "<feColorMatrix in=\"{last}\" type=\"hueRotate\" values=\"{degrees:.2}\" result=\"{out}\"/>\n"
                ));
            }
            Fx::Brightness { amount } => {
                body.push_str(&format!(
                    "<feComponentTransfer in=\"{last}\" result=\"{out}\"><feFuncR type=\"linear\" slope=\"{amount:.3}\"/><feFuncG type=\"linear\" slope=\"{amount:.3}\"/><feFuncB type=\"linear\" slope=\"{amount:.3}\"/></feComponentTransfer>\n"
                ));
            }
            Fx::Contrast { amount } => {
                let t = 0.5 * (1.0 - amount);
                body.push_str(&format!(
                    "<feComponentTransfer in=\"{last}\" result=\"{out}\"><feFuncR type=\"linear\" slope=\"{amount:.3}\" intercept=\"{t:.3}\"/><feFuncG type=\"linear\" slope=\"{amount:.3}\" intercept=\"{t:.3}\"/><feFuncB type=\"linear\" slope=\"{amount:.3}\" intercept=\"{t:.3}\"/></feComponentTransfer>\n"
                ));
            }
            Fx::Invert { amount } => {
                let s = 1.0 - 2.0 * amount;
                body.push_str(&format!(
                    "<feComponentTransfer in=\"{last}\" result=\"{out}\"><feFuncR type=\"linear\" slope=\"{s:.3}\" intercept=\"{amount:.3}\"/><feFuncG type=\"linear\" slope=\"{s:.3}\" intercept=\"{amount:.3}\"/><feFuncB type=\"linear\" slope=\"{s:.3}\" intercept=\"{amount:.3}\"/></feComponentTransfer>\n"
                ));
            }
            Fx::ColorMatrix { values } => {
                let v: String = values.iter().map(|x| format!("{x:.4} ")).collect();
                body.push_str(&format!(
                    "<feColorMatrix in=\"{last}\" type=\"matrix\" values=\"{v}\" result=\"{out}\"/>\n"
                ));
            }
            Fx::Turbulence {
                fractal,
                base,
                octaves,
                seed,
            } => {
                let ty = if *fractal {
                    "fractalNoise"
                } else {
                    "turbulence"
                };
                body.push_str(&format!(
                    "<feTurbulence type=\"{ty}\" baseFrequency=\"{base:.4}\" numOctaves=\"{octaves}\" seed=\"{seed}\" result=\"{out}\"/>\n"
                ));
            }
            Fx::Displacement { scale, x_ch, y_ch } => {
                let ch = |c: u8| match c {
                    1 => "G",
                    2 => "B",
                    3 => "A",
                    _ => "R",
                };
                body.push_str(&format!(
                    "<feDisplacementMap in=\"{last}\" in2=\"{last}\" scale=\"{scale:.2}\" xChannelSelector=\"{}\" yChannelSelector=\"{}\" result=\"{out}\"/>\n",
                    ch(*x_ch),
                    ch(*y_ch)
                ));
            }
            Fx::AppleGlass { scale, frequency } => {
                let map = format!("{id}-glass");
                body.push_str(&format!(
                    "<feTurbulence type=\"fractalNoise\" baseFrequency=\"{frequency:.4}\" numOctaves=\"2\" seed=\"2\" result=\"{map}\"/>\n"
                ));
                body.push_str(&format!(
                    "<feDisplacementMap in=\"{last}\" in2=\"{map}\" scale=\"{scale:.2}\" xChannelSelector=\"R\" yChannelSelector=\"G\" result=\"{out}\"/>\n"
                ));
            }
        }
        last = out;
    }
    let [x, y, w, h] = region;
    Some(format!(
        "<filter id=\"{id}\" filterUnits=\"userSpaceOnUse\" primitiveUnits=\"userSpaceOnUse\" x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" color-interpolation-filters=\"sRGB\">\n{body}</filter>\n"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(r: u8, g: u8, b: u8, a: u8) -> Pixmap {
        let mut pm = Pixmap::new(16, 16).unwrap();
        for px in pm.data_mut().chunks_exact_mut(4) {
            let af = a as f32 / 255.0;
            px[0] = (r as f32 * af) as u8;
            px[1] = (g as f32 * af) as u8;
            px[2] = (b as f32 * af) as u8;
            px[3] = a;
        }
        pm
    }

    #[test]
    fn blur_changes_pixels() {
        let mut pm = Pixmap::new(32, 32).unwrap();
        pm.data_mut().fill(0);
        let i = ((8 * 32 + 8) * 4) as usize;
        pm.data_mut()[i..i + 4].copy_from_slice(&[255, 0, 0, 255]);
        let before = pm.data().to_vec();
        blur(&mut pm, 3.0);
        assert_ne!(pm.data(), before.as_slice());
        assert!(pm.data()[3] > 0 || pm.data()[((8 * 32 + 9) * 4) + 3] > 0);
    }

    #[test]
    fn apple_glass_is_a_displacement_of_turbulence() {
        let svg = svg_filter(
            "glass",
            &FilterStack {
                enabled: true,
                items: vec![Fx::AppleGlass {
                    scale: 12.0,
                    frequency: 0.05,
                }],
            },
            [0.0, 0.0, 32.0, 32.0],
        )
        .unwrap();
        assert!(svg.contains("<feTurbulence"));
        assert!(svg.contains("<feDisplacementMap"));
        assert!(svg.contains("in2=\"glass-glass\""));
        let mut pm = solid(20, 40, 200, 255);
        pm.data_mut()[0..4].copy_from_slice(&[255, 0, 0, 255]);
        let before = pm.data().to_vec();
        apple_glass(&mut pm, 8.0, 0.2);
        assert_ne!(pm.data(), before.as_slice());
    }

    #[test]
    fn saturate_zero_is_grey() {
        let mut pm = solid(255, 0, 0, 255);
        color_matrix(&mut pm, &saturate_matrix(0.0));
        let p = &pm.data()[0..4];
        assert!((p[0] as i32 - p[1] as i32).abs() < 8);
    }

    #[test]
    fn svg_emits_primitives() {
        let mut s = FilterStack::default();
        s.items.push(Fx::Blur { std: 4.0 });
        s.items.push(Fx::Shadow {
            dx: 2.0,
            dy: 3.0,
            blur: 5.0,
            color: Rgba::new(0, 0, 0, 128),
        });
        let svg = svg_filter("fx1", &s, [-40.0, -40.0, 180.0, 180.0]).unwrap();
        assert!(svg.contains("feGaussianBlur"));
        assert!(svg.contains("feOffset"));
        assert!(svg.contains("feFlood"));
        assert!(svg.contains("feComposite"));
        assert!(!svg.contains("feDropShadow"));
        assert!(svg.contains("userSpaceOnUse"));
    }

    fn cutout(w: u32, h: u32, x0: u32, y0: u32, x1: u32, y1: u32) -> Pixmap {
        let mut pm = Pixmap::new(w, h).unwrap();
        for y in y0..y1 {
            for x in x0..x1 {
                let i = ((y * w + x) * 4) as usize;
                pm.data_mut()[i..i + 4].copy_from_slice(&[200, 10, 20, 255]);
            }
        }
        pm
    }

    fn px(pm: &Pixmap, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * pm.width() + x) * 4) as usize;
        pm.data()[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn dilate_grows_a_hard_cutout_without_moving_its_interior() {
        let mut pm = cutout(32, 32, 8, 8, 24, 24);
        morphology(&mut pm, false, 2.0);
        assert_eq!(px(&pm, 16, 16), [200, 10, 20, 255]);
        assert_eq!(px(&pm, 6, 16), [200, 10, 20, 255]);
        assert_eq!(px(&pm, 5, 16), [0, 0, 0, 0]);
        assert_eq!(px(&pm, 0, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn erode_shrinks_a_hard_cutout_and_leaves_the_middle_put() {
        let mut pm = cutout(32, 32, 8, 8, 24, 24);
        morphology(&mut pm, true, 2.0);
        assert_eq!(px(&pm, 16, 16), [200, 10, 20, 255]);
        assert_eq!(px(&pm, 8, 16), [0, 0, 0, 0]);
        assert_eq!(px(&pm, 9, 16), [0, 0, 0, 0]);
        assert_eq!(px(&pm, 11, 16), [200, 10, 20, 255]);
    }

    #[test]
    fn erode_does_not_blank_a_solid_image() {
        let mut pm = solid(0, 180, 40, 255);
        morphology(&mut pm, true, 1.0);
        assert_eq!(px(&pm, 8, 8), [0, 180, 40, 255]);
        assert_eq!(px(&pm, 0, 0)[3], 0);
    }

    #[test]
    fn morphology_on_a_large_cutout_stays_responsive() {
        let mut pm = cutout(480, 320, 40, 30, 440, 290);
        let started = std::time::Instant::now();
        morphology(&mut pm, false, 12.0);
        morphology(&mut pm, true, 12.0);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "morphology took {:?}",
            started.elapsed()
        );
        assert_eq!(px(&pm, 240, 160), [200, 10, 20, 255]);
    }
}
