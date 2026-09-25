//! Liquid-glass lens for the Apple glass effect.
//!
//! Opaque pixels are the glass. The color already in the pixmap is the
//! backdrop the glass refracts. Pixels with zero alpha stay as they are.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tiny_skia::Pixmap;

/// Liquid glass lens.
///
/// A saved file may omit fields. Missing fields come back as the built-in
/// defaults. An older file that still stores `scale` and `frequency` still
/// opens; those fields are ignored.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppleGlass {
    pub ior: f32,
    pub chromatic: f32,
    pub edge_width: f32,
    pub spec_power: f32,
    pub spec_intensity: f32,
    pub fresnel_intensity: f32,
    pub blur: f32,
    pub split: f32,
    pub split_angle: f32,
    pub falloff: f32,
}

impl AppleGlass {
    pub const DEFAULT: Self = Self {
        ior: 2.0,
        chromatic: 0.13,
        edge_width: 200.0,
        spec_power: 55.61,
        spec_intensity: 0.96,
        fresnel_intensity: 1.2,
        blur: 1.2,
        split: 0.45,
        split_angle: 130.0,
        falloff: 168.0,
    };
}

impl Default for AppleGlass {
    fn default() -> Self {
        Self::DEFAULT
    }
}

const BLUR_TAPS: usize = 16;
const GOLDEN: f32 = 2.39996323;
const FAR: f64 = 1.0e12;

/// Paint the liquid-glass lens into a pixmap.
///
/// `pm` is the layer or shape picture. `glass` supplies the lens settings.
/// Coverage of half or more is the glass. The pixmap is both the shape and
/// the backdrop. Pixels with zero alpha are not touched, and pixels that are
/// touched keep the coverage they already had.
pub(super) fn apply(pm: &mut Pixmap, glass: &AppleGlass) {
    let w = pm.width() as usize;
    let h = pm.height() as usize;
    if w == 0 || h == 0 {
        return;
    }
    let src = pm.data().to_vec();
    if src.chunks_exact(4).all(|px| px[3] == 0) {
        return;
    }
    let glass = finite_glass(glass);
    let spread = glass.blur.abs() * (1.0 + glass.split.abs());
    let reach = glass.edge_width.abs().max(1.0) + glass.falloff.abs() + spread + 2.0;
    let field = distance_field(&src, w, h, reach);
    let split_px = glass.blur * glass.split;
    let lens = Lens {
        src: &src,
        w: w as i32,
        h: h as i32,
        field: &field,
        glass,
        light: light_from_corner(w as f32, h as f32),
        inset: glass.blur + split_px,
        split_px,
    };
    pm.data_mut()
        .par_chunks_mut(w * 4)
        .enumerate()
        .for_each(|(y, row)| {
            for x in 0..w {
                let src_i = (y * w + x) * 4;
                let src_a = src[src_i + 3];
                if src_a == 0 {
                    continue;
                }
                let color = lens.shade(x as f32 + 0.5, y as f32 + 0.5);
                if !(color[3] > 1.0e-3) {
                    continue;
                }
                let cover = src_a as f32 / 255.0 / color[3];
                let i = x * 4;
                row[i] = to_byte(color[0] * cover);
                row[i + 1] = to_byte(color[1] * cover);
                row[i + 2] = to_byte(color[2] * cover);
                row[i + 3] = src_a;
            }
        });
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

fn finite_glass(glass: &AppleGlass) -> AppleGlass {
    let base = AppleGlass::DEFAULT;
    AppleGlass {
        ior: finite_or(glass.ior, base.ior),
        chromatic: finite_or(glass.chromatic, base.chromatic),
        edge_width: finite_or(glass.edge_width, base.edge_width),
        spec_power: finite_or(glass.spec_power, base.spec_power),
        spec_intensity: finite_or(glass.spec_intensity, base.spec_intensity),
        fresnel_intensity: finite_or(glass.fresnel_intensity, base.fresnel_intensity),
        blur: finite_or(glass.blur, base.blur),
        split: finite_or(glass.split, base.split),
        split_angle: finite_or(glass.split_angle, base.split_angle),
        falloff: finite_or(glass.falloff, base.falloff),
    }
}

/// Light 30% in from the top-left of the pixmap.
fn light_from_corner(w: f32, h: f32) -> [f32; 3] {
    let mouse_x = w * 0.3;
    let mouse_y = h * 0.3;
    let mut dx = w * 0.5 - mouse_x;
    let mut dy = h * 0.5 - mouse_y;
    dy -= 0.1;
    let len = (dx * dx + dy * dy).sqrt();
    let scale = len.max(1.0) / len.max(1.0e-6);
    dx *= scale;
    dy *= scale;
    let along = (dx * dx + dy * dy).sqrt();
    normalize3([dx, dy, along])
}

fn to_byte(value: f32) -> u8 {
    if !value.is_finite() {
        return 0;
    }
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Dome height from the rim to the flat middle.
///
/// `t` is 0 on the edge and 1 at `edge_width`. Returns a height from 0 to 1.
fn surface_height(t: f32) -> f32 {
    let s = 1.0 - t;
    let s4 = s * s * s * s;
    (1.0 - s4).max(0.0).powf(0.25)
}

/// Snell shift in slope units.
///
/// `sin_i` is the sine of the incoming angle, `slope` is the surface slope,
/// and `n` is the index of refraction. Returns the refracted slope minus `slope`.
fn refract_disp(sin_i: f32, slope: f32, n: f32) -> f32 {
    let n = if n.abs() < 1.0e-4 { 1.0e-3 } else { n };
    let sin_r = (sin_i / n).clamp(-0.9999, 0.9999);
    sin_r * (1.0 - sin_r * sin_r).sqrt().recip() - slope
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len <= 1.0e-8 {
        return [0.0, 0.0, 1.0];
    }
    [v[0] / len, v[1] / len, v[2] / len]
}

fn reflect3(incident: [f32; 3], normal: [f32; 3]) -> [f32; 3] {
    let d = normal[0] * incident[0] + normal[1] * incident[1] + normal[2] * incident[2];
    [
        incident[0] - 2.0 * d * normal[0],
        incident[1] - 2.0 * d * normal[1],
        incident[2] - 2.0 * d * normal[2],
    ]
}

struct Field {
    w: usize,
    h: usize,
    pix: Vec<[f32; 3]>,
}

impl Field {
    fn sample(&self, px: f32, py: f32) -> [f32; 3] {
        if self.w == 0 || self.h == 0 {
            return [0.0, 0.0, 0.0];
        }
        let px = if px.is_finite() { px } else { 0.0 };
        let py = if py.is_finite() { py } else { 0.0 };
        let px = px.clamp(0.0, (self.w - 1) as f32);
        let py = py.clamp(0.0, (self.h - 1) as f32);
        let x0 = px.floor() as usize;
        let y0 = py.floor() as usize;
        let x1 = (x0 + 1).min(self.w - 1);
        let y1 = (y0 + 1).min(self.h - 1);
        let tx = px - x0 as f32;
        let ty = py - y0 as f32;
        let p00 = self.pix[y0 * self.w + x0];
        let p10 = self.pix[y0 * self.w + x1];
        let p01 = self.pix[y1 * self.w + x0];
        let p11 = self.pix[y1 * self.w + x1];
        let mut out = [0.0; 3];
        for c in 0..3 {
            let top = p00[c] + (p10[c] - p00[c]) * tx;
            let bot = p01[c] + (p11[c] - p01[c]) * tx;
            out[c] = top + (bot - top) * ty;
        }
        out
    }
}

/// Signed distance from alpha. Inside the opaque region is positive.
///
/// The search is capped at `reach` pixels, about an edge width plus the
/// falloff and the blur spread. Farther than that the lens is already flat.
fn distance_field(src: &[u8], w: usize, h: usize, reach: f32) -> Field {
    let pw = w + 2;
    let ph = h + 2;
    let mut inside = vec![FAR; pw * ph];
    let mut outside = vec![0.0; pw * ph];
    for y in 0..h {
        for x in 0..w {
            if src[(y * w + x) * 4 + 3] >= 128 {
                let i = (y + 1) * pw + (x + 1);
                inside[i] = 0.0;
                outside[i] = FAR;
            }
        }
    }
    edt_2d(&mut inside, pw, ph);
    edt_2d(&mut outside, pw, ph);
    let reach = if reach.is_finite() {
        reach.max(1.0)
    } else {
        1.0
    };
    let mut raw = vec![0.0f32; pw * ph];
    for i in 0..pw * ph {
        let d = outside[i].sqrt() - inside[i].sqrt();
        raw[i] = if d.is_finite() {
            (d as f32).clamp(-reach, reach)
        } else {
            0.0
        };
    }
    let mut pix = vec![[0.0; 3]; w * h];
    for y in 0..h {
        for x in 0..w {
            let at = |sx: usize, sy: usize| raw[sy * pw + sx];
            let px = x + 1;
            let py = y + 1;
            let sd = at(px, py);
            let gx = at(px + 1, py) - at(px - 1, py);
            let gy = at(px, py + 1) - at(px, py - 1);
            let len = (gx * gx + gy * gy).sqrt();
            let (nx, ny) = if len > 1.0e-6 {
                (gx / len, gy / len)
            } else {
                (0.0, 0.0)
            };
            pix[y * w + x] = [sd, nx, ny];
        }
    }
    Field { w, h, pix }
}

fn edt_2d(grid: &mut [f64], w: usize, h: usize) {
    if w == 0 || h == 0 {
        return;
    }
    let n = w.max(h);
    let mut v = vec![0i32; n];
    let mut z = vec![0.0f64; n + 1];
    let mut line = vec![0.0f64; n];
    let mut dest = vec![0.0f64; n];
    for x in 0..w {
        for y in 0..h {
            line[y] = grid[y * w + x];
        }
        edt_1d(&line[..h], &mut dest[..h], &mut v[..h], &mut z[..h + 1]);
        for y in 0..h {
            grid[y * w + x] = dest[y];
        }
    }
    for y in 0..h {
        line[..w].copy_from_slice(&grid[y * w..(y + 1) * w]);
        edt_1d(&line[..w], &mut dest[..w], &mut v[..w], &mut z[..w + 1]);
        grid[y * w..(y + 1) * w].copy_from_slice(&dest[..w]);
    }
}

fn edt_1d(f: &[f64], d: &mut [f64], v: &mut [i32], z: &mut [f64]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        k += 1;
        let s = loop {
            k -= 1;
            let r = v[k] as usize;
            let qf = q as f64;
            let rf = r as f64;
            let s = (f[q] - f[r] + qf * qf - rf * rf) / (2.0 * (qf - rf));
            if k == 0 || !(s <= z[k]) {
                break s;
            }
        };
        k += 1;
        v[k] = q as i32;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    k = 0;
    for q in 0..n {
        while k + 1 < n && z[k + 1] < q as f64 {
            k += 1;
        }
        let r = v[k];
        let rf = r as f64;
        let qf = q as f64;
        d[q] = (qf - rf) * (qf - rf) + f[r as usize];
    }
}

struct Lens<'a> {
    src: &'a [u8],
    w: i32,
    h: i32,
    field: &'a Field,
    glass: AppleGlass,
    light: [f32; 3],
    inset: f32,
    split_px: f32,
}

impl Lens<'_> {
    fn shade(&self, x: f32, y: f32) -> [f32; 4] {
        let sd = self.sample_field(x, y)[0];
        let falloff = self.glass.falloff.max(1.0e-3);
        let amount = 1.0 - ((sd - self.inset) / falloff).clamp(0.0, 1.0);
        let blur = self.glass.blur.max(0.0) * amount;
        let split = self.split_px * amount;
        if self.split_px <= 0.0 {
            return self.lens_blurred(x, y, blur);
        }
        let angle = self.glass.split_angle.to_radians();
        let off_x = angle.cos() * split;
        let off_y = angle.sin() * split;
        let red = self.lens_blurred(x + off_x, y + off_y, blur);
        let green = self.lens_blurred(x, y, blur);
        let blue = self.lens_blurred(x - off_x, y - off_y, blur);
        [red[0], green[1], blue[2], red[3].max(green[3]).max(blue[3])]
    }

    fn lens_blurred(&self, x: f32, y: f32, radius: f32) -> [f32; 4] {
        if self.glass.blur <= 0.0 || radius <= 0.0 {
            return self.lens_color(x, y);
        }
        let mut sum = self.lens_color(x, y);
        for i in 0..BLUR_TAPS {
            let fi = i as f32 + 0.5;
            let angle = fi * GOLDEN;
            let r = (fi / BLUR_TAPS as f32).sqrt() * radius;
            let sample = self.lens_color(x + angle.cos() * r, y + angle.sin() * r);
            sum[0] += sample[0];
            sum[1] += sample[1];
            sum[2] += sample[2];
            sum[3] += sample[3];
        }
        let scale = 1.0 / (BLUR_TAPS as f32 + 1.0);
        [
            sum[0] * scale,
            sum[1] * scale,
            sum[2] * scale,
            sum[3] * scale,
        ]
    }

    fn lens_color(&self, x: f32, y: f32) -> [f32; 4] {
        let sdf = self.sample_field(x, y);
        let sd = sdf[0] - self.inset;
        let edge = smoothstep(-1.0, 1.0, sd);
        let ew = self.glass.edge_width.max(1.0);
        let t = (sd / ew).clamp(0.0, 1.0);
        let gx = sdf[1] + 1.0e-6;
        let gy = sdf[2] + 1.0e-6;
        let glen = (gx * gx + gy * gy).sqrt().max(1.0e-8);
        let border_x = -gx / glen;
        let border_y = -gy / glen;
        let delta = 0.001;
        let h1 = surface_height((t - delta).clamp(0.0, 1.0));
        let h2 = surface_height((t + delta).clamp(0.0, 1.0));
        let slope = (h2 - h1) * (0.5 / delta);
        let sin_i = slope * (1.0 + slope * slope).sqrt().recip();
        let disp_g = refract_disp(sin_i, slope, self.glass.ior) * ew;
        let refracted = if self.glass.chromatic == 0.0 {
            self.sample_bg(x + border_x * disp_g, y + border_y * disp_g)
        } else {
            let disp_r = refract_disp(sin_i, slope, self.glass.ior - self.glass.chromatic) * ew;
            let disp_b = refract_disp(sin_i, slope, self.glass.ior + self.glass.chromatic) * ew;
            [
                self.sample_bg(x + border_x * disp_r, y + border_y * disp_r)[0],
                self.sample_bg(x + border_x * disp_g, y + border_y * disp_g)[1],
                self.sample_bg(x + border_x * disp_b, y + border_y * disp_b)[2],
            ]
        };
        let normal = normalize3([-slope * border_x, -slope * border_y, 1.0]);
        let reflected = reflect3([-self.light[0], -self.light[1], -self.light[2]], normal);
        let spec = reflected[2].max(0.0).powf(self.glass.spec_power.max(0.0));
        let fresnel = (1.0 - normal[2].clamp(-1.0, 1.0)).max(0.0).powf(3.0);
        let add = spec * self.glass.spec_intensity + fresnel * self.glass.fresnel_intensity;
        [
            (refracted[0] + add) * edge,
            (refracted[1] + add) * edge,
            (refracted[2] + add) * edge,
            edge,
        ]
    }

    fn sample_field(&self, x: f32, y: f32) -> [f32; 3] {
        self.field.sample(x - 0.5, y - 0.5)
    }

    fn sample_bg(&self, x: f32, y: f32) -> [f32; 3] {
        let x = if x.is_finite() { x } else { 0.0 };
        let y = if y.is_finite() { y } else { 0.0 };
        let px = (x - 0.5).clamp(0.0, (self.w - 1) as f32);
        let py = (y - 0.5).clamp(0.0, (self.h - 1) as f32);
        let x0 = px.floor() as i32;
        let y0 = py.floor() as i32;
        let x1 = (x0 + 1).min(self.w - 1);
        let y1 = (y0 + 1).min(self.h - 1);
        let tx = px - x0 as f32;
        let ty = py - y0 as f32;
        let p00 = self.texel(x0, y0);
        let p10 = self.texel(x1, y0);
        let p01 = self.texel(x0, y1);
        let p11 = self.texel(x1, y1);
        let mut out = [0.0; 3];
        for c in 0..3 {
            let top = p00[c] + (p10[c] - p00[c]) * tx;
            let bot = p01[c] + (p11[c] - p01[c]) * tx;
            out[c] = top + (bot - top) * ty;
        }
        out
    }

    fn texel(&self, x: i32, y: i32) -> [f32; 3] {
        let i = ((y as usize * self.w as usize) + x as usize) * 4;
        [
            self.src[i] as f32 / 255.0,
            self.src[i + 1] as f32 / 255.0,
            self.src[i + 2] as f32 / 255.0,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_is_positive_inside_and_points_inward() {
        let w = 32;
        let h = 32;
        let mut src = vec![0u8; w * h * 4];
        for y in 8..24 {
            for x in 8..24 {
                src[(y * w + x) * 4 + 3] = 255;
            }
        }
        let field = distance_field(&src, w, h, 64.0);
        let center = field.pix[16 * w + 16];
        let corner = field.pix[0];
        let right = field.pix[16 * w + 22];
        let top = field.pix[8 * w + 16];
        assert!(
            (center[0] - 8.0).abs() < 2.0,
            "center distance {}",
            center[0]
        );
        assert!(corner[0] < 0.0, "corner {}", corner[0]);
        assert!(right[0] > 0.0, "right distance {}", right[0]);
        assert!(right[1] < -0.5, "right gradient {}", right[1]);
        assert!(top[2] > 0.5, "top gradient {}", top[2]);
    }
}
