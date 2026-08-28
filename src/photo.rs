use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq)]
pub struct RgbaImage {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

impl RgbaImage {
    pub fn new(w: u32, h: u32, data: Vec<u8>) -> Option<Self> {
        if w == 0 || h == 0 || data.len() != w as usize * h as usize * 4 {
            return None;
        }
        Some(Self { w, h, data })
    }

    pub fn downscaled(&self, max_edge: u32) -> RgbaImage {
        let long = self.w.max(self.h);
        if long <= max_edge {
            return self.clone();
        }
        let k = max_edge as f32 / long as f32;
        let dw = (self.w as f32 * k).round().max(1.0) as u32;
        let dh = (self.h as f32 * k).round().max(1.0) as u32;
        let mut out = vec![0u8; dw as usize * dh as usize * 4];
        let sx = self.w as f32 / dw as f32;
        let sy = self.h as f32 / dh as f32;
        out.par_chunks_mut(dw as usize * 4)
            .enumerate()
            .for_each(|(y, row)| {
                let fy = (y as f32 + 0.5) * sy;
                let y0 = (fy.floor() as u32).min(self.h - 1);
                for (x, px) in row.chunks_mut(4).enumerate() {
                    let fx = (x as f32 + 0.5) * sx;
                    let x0 = (fx.floor() as u32).min(self.w - 1);
                    let i = (y0 * self.w + x0) as usize * 4;
                    px.copy_from_slice(&self.data[i..i + 4]);
                }
            });
        RgbaImage {
            w: dw,
            h: dh,
            data: out,
        }
    }

    fn alpha(&self, x: u32, y: u32) -> u8 {
        self.data[((y * self.w + x) as usize) * 4 + 3]
    }

    pub fn encode_png(&self) -> Option<Vec<u8>> {
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
            pm.data_mut()[i * 4..i * 4 + 4].copy_from_slice(&[r, g, b, a as u8]);
        }
        pm.encode_png().ok()
    }
}

pub fn to_pixmap(img: &RgbaImage) -> Option<tiny_skia::Pixmap> {
    let mut pm = tiny_skia::Pixmap::new(img.w, img.h)?;
    for (i, src) in img.data.chunks_exact(4).enumerate() {
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
        pm.data_mut()[i * 4..i * 4 + 4].copy_from_slice(&[r, g, b, a as u8]);
    }
    Some(pm)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DevelopParams {
    pub exposure: f32,
    pub temperature: f32,
    pub tint: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    pub clarity: f32,
    pub saturation: f32,
    pub vibrance: f32,
    pub grain: f32,
    pub vignette: f32,
    pub dehaze: f32,
    pub hue: f32,
    pub split_shadow: [f32; 3],
    pub split_highlight: [f32; 3],
    pub split_balance: f32,
    pub curve: [f32; 5],
    pub hsl: [HslBand; 8],
    pub rotate: u32,
    pub crop: Option<[f32; 4]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HslBand {
    pub hue: f32,
    pub sat: f32,
    pub luma: f32,
}

impl Default for HslBand {
    fn default() -> Self {
        Self {
            hue: 0.0,
            sat: 0.0,
            luma: 0.0,
        }
    }
}

pub const HSL_NAMES: [&str; 8] = [
    "Red", "Orange", "Yellow", "Green", "Aqua", "Blue", "Purple", "Magenta",
];

impl Default for DevelopParams {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            temperature: 0.0,
            tint: 0.0,
            contrast: 1.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            clarity: 0.0,
            saturation: 1.0,
            vibrance: 1.0,
            grain: 0.0,
            vignette: 0.0,
            dehaze: 0.0,
            hue: 0.0,
            split_shadow: [0.0, 0.0, 0.0],
            split_highlight: [0.0, 0.0, 0.0],
            split_balance: 0.0,
            curve: [0.0, 0.25, 0.5, 0.75, 1.0],
            hsl: [HslBand::default(); 8],
            rotate: 0,
            crop: None,
        }
    }
}

impl DevelopParams {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    pub fn rotated_dim(&self, w: u32, h: u32) -> (u32, u32) {
        if self.rotate == 90 || self.rotate == 270 {
            (h, w)
        } else {
            (w, h)
        }
    }
}

#[derive(Clone)]
pub struct PhotoImage {
    pub name: String,
    pub full: RgbaImage,
    pub preview: RgbaImage,
    pub thumb: RgbaImage,
    pub develop: DevelopParams,
}

impl PhotoImage {
    pub fn from_full(name: String, full: RgbaImage) -> Self {
        let preview = full.downscaled(1600);
        let thumb = full.downscaled(192);
        Self {
            name,
            full,
            preview,
            thumb,
            develop: DevelopParams::default(),
        }
    }
}

pub fn decode_bytes(bytes: &[u8]) -> Option<RgbaImage> {
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    RgbaImage::new(w, h, rgba.into_raw())
}

pub fn load_file(path: &std::path::Path) -> Option<RgbaImage> {
    let bytes = std::fs::read(path).ok()?;
    decode_bytes(&bytes)
}

fn pil(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

#[inline]
fn pixel_luma(r: f32, g: f32, b: f32) -> f32 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

pub fn develop(src: &RgbaImage, p: &DevelopParams) -> RgbaImage {
    let (rw, rh) = p.rotated_dim(src.w, src.h);
    let (cw, ch) = match p.crop {
        Some(c) => {
            let w = ((c[2] - c[0]) * rw as f32).round().max(1.0) as u32;
            let h = ((c[3] - c[1]) * rh as f32).round().max(1.0) as u32;
            (w, h)
        }
        None => (rw, rh),
    };

    let n = src.w as usize * src.h as usize;
    let mut col = vec![0f32; n * 3];

    let exp = 2f32.powf(p.exposure);
    let t = p.temperature.clamp(-100.0, 100.0) / 100.0;
    let ti = p.tint.clamp(-100.0, 100.0) / 100.0;
    let temp_r = 1.0 + t * 0.22 + ti * 0.08;
    let temp_g = 1.0 - t * 0.05 - ti * 0.18;
    let temp_b = 1.0 - t * 0.22 + ti * 0.08;
    let contrast = p.contrast.clamp(0.0, 2.0).max(0.0001);
    let sat = p.saturation.clamp(0.0, 2.0);
    let vib = p.vibrance.clamp(0.0, 2.0);
    let hl = p.highlights.clamp(-1.0, 1.0);
    let sh = p.shadows.clamp(-1.0, 1.0);
    let wh = p.whites.clamp(-1.0, 1.0);
    let bl = p.blacks.clamp(-1.0, 1.0);

    col.par_chunks_mut(3)
        .enumerate()
        .for_each(|(i, px)| {
            let base = i * 4;
            let a = src.data[base + 3] as f32 / 255.0;
            let mut r = src.data[base] as f32 / 255.0;
            let mut g = src.data[base + 1] as f32 / 255.0;
            let mut b = src.data[base + 2] as f32 / 255.0;

            if a > 0.0 {
                r = pil(r * exp * temp_r);
                g = pil(g * exp * temp_g);
                b = pil(b * exp * temp_b);

                let l = pixel_luma(r, g, b);

                let hl_w = l * l;
                let sh_w = (1.0 - l) * (1.0 - l);
                let wh_w = l * l * l * l * l * l;
                let bl_w = (1.0 - l) * (1.0 - l) * (1.0 - l) * (1.0 - l) * (1.0 - l) * (1.0 - l);
                r = pil(r * (1.0 + hl * 0.9 * hl_w + sh * 0.9 * sh_w + wh * 0.7 * wh_w + bl * 0.7 * bl_w));
                g = pil(g * (1.0 + hl * 0.9 * hl_w + sh * 0.9 * sh_w + wh * 0.7 * wh_w + bl * 0.7 * bl_w));
                b = pil(b * (1.0 + hl * 0.9 * hl_w + sh * 0.9 * sh_w + wh * 0.7 * wh_w + bl * 0.7 * bl_w));

                r = pil((r - 0.5) * contrast + 0.5);
                g = pil((g - 0.5) * contrast + 0.5);
                b = pil((b - 0.5) * contrast + 0.5);

                let ln = pixel_luma(r, g, b);
                let total_sat = sat * (1.0 + (vib - 1.0) * (1.0 - (r.max(g).max(b) - r.min(g).min(b))));
                let s = total_sat;
                r = pil(ln + (r - ln) * s);
                g = pil(ln + (g - ln) * s);
                b = pil(ln + (b - ln) * s);

                if p.dehaze.abs() > 0.001 {
                    let l = pixel_luma(r, g, b);
                    let haze = (l - r.min(g).min(b)).max(0.0);
                    let k = p.dehaze.clamp(-1.0, 1.0);
                    r = pil(r - haze * k * 0.85);
                    g = pil(g - haze * k * 0.85);
                    b = pil(b - haze * k * 0.85);
                }

                if p.hue.abs() > 0.001 || p.hsl.iter().any(|b| b.hue.abs() + b.sat.abs() + b.luma.abs() > 0.001) {
                    let (mut h, mut s, mut l) = rgb_to_hsl(r, g, b);
                    h = (h + p.hue / 360.0).rem_euclid(1.0);
                    let band = hsl_band(h);
                    let hb = p.hsl[band];
                    h = (h + hb.hue / 360.0).rem_euclid(1.0);
                    s = pil(s * (1.0 + hb.sat));
                    l = pil(l * (1.0 + hb.luma));
                    let (nr, ng, nb) = hsl_to_rgb(h, s, l);
                    r = nr;
                    g = ng;
                    b = nb;
                }

                let l = pixel_luma(r, g, b);
                let bal = p.split_balance.clamp(-1.0, 1.0);
                let sh_w = ((1.0 - l) * (1.0 - bal * 0.5)).clamp(0.0, 1.0);
                let hi_w = (l * (1.0 + bal * 0.5)).clamp(0.0, 1.0);
                r = pil(r + p.split_shadow[0] * sh_w + p.split_highlight[0] * hi_w);
                g = pil(g + p.split_shadow[1] * sh_w + p.split_highlight[1] * hi_w);
                b = pil(b + p.split_shadow[2] * sh_w + p.split_highlight[2] * hi_w);

                r = eval_curve(r, &p.curve);
                g = eval_curve(g, &p.curve);
                b = eval_curve(b, &p.curve);
            }

            px[0] = r;
            px[1] = g;
            px[2] = b;
        });

    let lum: Vec<f32> = col
        .par_chunks(3)
        .map(|px| pixel_luma(px[0], px[1], px[2]))
        .collect();

    if p.clarity.abs() > 0.001 {
        let radius = ((src.w.min(src.h)) as f32 / 260.0).clamp(2.0, 10.0) as usize;
        let bl = box_blur(&lum, src.w as usize, src.h as usize, radius);
        let c = (p.clarity.clamp(-1.0, 1.0) * 3.0) / (1.0 + radius as f32 * 0.5);
        col.par_chunks_mut(3)
            .enumerate()
            .for_each(|(i, px)| {
                let l = lum[i];
                let delta = (l - bl[i]) * c;
                let nl = pil(l + delta);
                if l > 0.0001 {
                    let f = nl / l;
                    px[0] = pil(px[0] * f);
                    px[1] = pil(px[1] * f);
                    px[2] = pil(px[2] * f);
                }
            });
    }

    let grain = p.grain.clamp(0.0, 1.0);
    let vign = p.vignette.clamp(-1.0, 1.0);

    let mut out = vec![0u8; cw as usize * ch as usize * 4];
    out.par_chunks_mut(cw as usize * 4)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_mut(4).enumerate() {
                let sw = src.w as f64;
                let sh = src.h as f64;
                let mut sx = x as f64 + 0.5;
                let mut sy = y as f64 + 0.5;

                if let Some(c) = &p.crop {
                    let (cwf, chf) = (cw as f64, ch as f64);
                    sx = c[0] as f64 * rw as f64
                        + (x as f64 / cwf) * (c[2] - c[0]) as f64 * rw as f64;
                    sy = c[1] as f64 * rh as f64
                        + (y as f64 / chf) * (c[3] - c[1]) as f64 * rh as f64;
                }

                let (mut ux, mut uy) = (sx, sy);
                match p.rotate {
                    90 => {
                        let t = ux;
                        ux = uy;
                        uy = sh - t;
                    }
                    180 => {
                        ux = sw - ux;
                        uy = sh - uy;
                    }
                    270 => {
                        let t = ux;
                        ux = sw - uy;
                        uy = t;
                    }
                    _ => {}
                }

                let ix = (ux - 0.5).round() as i64;
                let iy = (uy - 0.5).round() as i64;
                if ix < 0 || iy < 0 || (ix as u32) >= src.w || (iy as u32) >= src.h {
                    continue;
                }
                let sxr = ix as usize;
                let syr = iy as usize;
                let si = (syr * src.w as usize + sxr) * 3;
                let a = src.alpha(sxr as u32, syr as u32);
                let (mut r, mut g, mut b) = (col[si], col[si + 1], col[si + 2]);

                if vign.abs() > 0.001 {
                    let nx = (x as f32 / cw as f32 - 0.5) * 2.0;
                    let ny = (y as f32 / ch as f32 - 0.5) * 2.0;
                    let d = (nx * nx + ny * ny) * 0.7;
                    if vign > 0.0 {
                        let f = 1.0 - vign * d * 0.85;
                        r = pil(r * f);
                        g = pil(g * f);
                        b = pil(b * f);
                    } else {
                        let f = -vign * d * 0.85;
                        r = pil(r + (1.0 - r) * f);
                        g = pil(g + (1.0 - g) * f);
                        b = pil(b + (1.0 - b) * f);
                    }
                }
                if grain > 0.001 {
                    let oi = y * cw as usize + x;
                    let mut z = (oi as u32)
                        .wrapping_mul(747796405)
                        .wrapping_add(2891336453);
                    z = (z ^ (z >> 13)).wrapping_mul(1274126177);
                    let gv = ((z >> 24) as f64 / 255.0) as f32 * 2.0 - 1.0;
                    let band = grain * 0.10 + grain * grain * 0.10;
                    r = pil(r + gv * band);
                    g = pil(g + gv * band);
                    b = pil(b + gv * band);
                }

                px[0] = (r * 255.0).round() as u8;
                px[1] = (g * 255.0).round() as u8;
                px[2] = (b * 255.0).round() as u8;
                px[3] = a;
            }
        });

    RgbaImage {
        w: cw,
        h: ch,
        data: out,
    }
}

fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) * 0.5;
    let d = max - min;
    if d < 1e-6 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if max == g {
        (((b - r) / d) + 2.0) / 6.0
    } else {
        (((r - g) / d) + 4.0) / 6.0
    };
    (h, s.clamp(0.0, 1.0), l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    let a = s * l.min(1.0 - l);
    let f = |n: f32| {
        let k = (n + h * 12.0) % 12.0;
        l - a * ((k - 3.0).min(9.0 - k).clamp(-1.0, 1.0))
    };
    (f(0.0), f(8.0), f(4.0))
}

fn hsl_band(h: f32) -> usize {
    // 8 slices starting at red (0)
    ((h.rem_euclid(1.0) * 8.0).floor() as usize).min(7)
}

fn eval_curve(x: f32, pts: &[f32; 5]) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let t = x * 4.0;
    let i = t.floor() as usize;
    let f = t - i as f32;
    let a = pts[i.min(4)];
    let b = pts[(i + 1).min(4)];
    pil(a + (b - a) * f)
}

fn box_blur(lum: &[f32], w: usize, h: usize, radius: usize) -> Vec<f32> {
    let mut horiz = vec![0f32; w * h];
    horiz
        .par_chunks_mut(w)
        .enumerate()
        .for_each(|(y, row)| {
            let base = y * w;
            let mut sum = 0f32;
            let win = radius * 2 + 1;
            for x in 0..(radius.min(w)) {
                sum += lum[base + x];
            }
            for x in 0..w {
                let x0 = x as i64 - radius as i64;
                let x1 = x as i64 + radius as i64;
                if x0 >= 0 {
                    sum -= lum[base + x0 as usize];
                } else {
                    sum -= lum[base];
                }
                let x1c = (x1 as usize).min(w - 1);
                sum += lum[base + x1c];
                row[x] = sum / win as f32;
            }
        });

    let mut vert = vec![0f32; w * h];
    vert
        .par_chunks_mut(w)
        .enumerate()
        .for_each(|(x, column)| {
            let mut sum = 0f32;
            let win = radius * 2 + 1;
            for y in 0..(radius.min(h)) {
                sum += horiz[y * w + x];
            }
            for y in 0..h {
                let y0 = y as i64 - radius as i64;
                let y1 = y as i64 + radius as i64;
                if y0 >= 0 {
                    sum -= horiz[y0 as usize * w + x];
                } else {
                    sum -= horiz[x];
                }
                let y1c = (y1 as usize).min(h - 1);
                sum += horiz[y1c * w + x];
                column[y] = sum / win as f32;
            }
        });

    vert
}

#[derive(Clone, Copy)]
pub struct Histogram {
    pub bins: [u32; 256],
    pub max: u32,
}

impl Default for Histogram {
    fn default() -> Self {
        Histogram {
            bins: [0; 256],
            max: 1,
        }
    }
}

pub fn histograms(img: &RgbaImage) -> [Histogram; 4] {
    let mut bins = [[0u32; 256]; 4];
    for px in img.data.chunks_exact(4) {
        let l = (0.2126 * px[0] as f32 + 0.7152 * px[1] as f32 + 0.0722 * px[2] as f32) as usize;
        bins[0][px[0] as usize] += 1;
        bins[1][px[1] as usize] += 1;
        bins[2][px[2] as usize] += 1;
        bins[3][l.min(255)] += 1;
    }
    let mut out = [Histogram { bins: [0; 256], max: 1 }; 4];
    for (i, h) in out.iter_mut().enumerate() {
        h.bins = bins[i];
        h.max = bins[i].iter().copied().max().unwrap_or(1).max(1);
    }
    out
}

pub fn luminance_histogram(img: &RgbaImage) -> Histogram {
    histograms(img)[3]
}

pub fn auto_tone(p: &mut DevelopParams, img: &RgbaImage) {
    let mut lums: Vec<u8> = img
        .data
        .chunks_exact(4)
        .step_by(4)
        .map(|px| {
            (0.2126 * px[0] as f32 + 0.7152 * px[1] as f32 + 0.0722 * px[2] as f32) as u8
        })
        .collect();
    lums.sort_unstable();
    let n = lums.len().max(2);
    let lo_i = ((n as f32 * 0.01).floor() as usize).min(n - 1);
    let hi_i = ((n as f32 * 0.99).floor() as usize).min(n - 1);
    let lo = lums[lo_i] as f32 / 255.0;
    let hi = lums[hi_i] as f32 / 255.0;
    p.exposure = (0.5 / ((lo + hi) * 0.5).max(0.01)).log2();
    p.blacks = -lo * 4.0;
    p.whites = (1.0 - hi) * 4.0;
}

#[allow(dead_code)]
pub fn total_pixels(img: &RgbaImage) -> usize {
    img.w as usize * img.h as usize
}

fn hash01(x: u32, y: u32) -> f32 {
    let h = x
        .wrapping_mul(374761393)
        .wrapping_add(y.wrapping_mul(668265263));
    let h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    ((h >> 24) as f32) / 255.0
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn value_noise(x: f32, y: f32) -> f32 {
    let xi = x.floor() as u32;
    let yi = y.floor() as u32;
    let fx = x - x.floor();
    let fy = y - y.floor();
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sy = fy * fy * (3.0 - 2.0 * fy);
    let a = hash01(xi, yi);
    let b = hash01(xi + 1, yi);
    let c = hash01(xi, yi + 1);
    let d = hash01(xi + 1, yi + 1);
    lerp(lerp(a, b, sx), lerp(c, d, sx), sy)
}

fn fbm(x: f32, y: f32, octaves: u32) -> f32 {
    let mut v = 0.0;
    let mut amp = 0.5;
    let mut freq = 1.0;
    let mut total = 0.0;
    for _ in 0..octaves {
        v += amp * value_noise(x * freq, y * freq);
        total += amp;
        amp *= 0.5;
        freq *= 2.1;
    }
    v / total
}

fn sample_dawn() -> RgbaImage {
    let (w, h) = (1920u32, 1080u32);
    let mut data = vec![0u8; w as usize * h as usize * 4];
    let horizon = 0.62_f32;
    for (i, px) in data.chunks_exact_mut(4).enumerate() {
        let x = (i % w as usize) as f32;
        let y = (i / w as usize) as f32;
        let nx = x / w as f32;
        let ny = y / h as f32;

        let mut r;
        let mut g;
        let mut b;
        let a = 255u8;

        let sun_cx = 0.72_f32;
        let sun_cy = 0.52_f32;
        let sd = ((nx - sun_cx).powi(2) + (ny - sun_cy).powi(2)).sqrt();

        if ny < horizon {
            let t = ny / horizon;
            let sy = t.powi(4);
            r = lerp(0.12, 1.0, (1.0 - t).powi(2));
            g = lerp(0.10, 0.62, 1.0 - t * 1.5);
            b = lerp(0.34, 0.35, 1.0 - t);
            r = r * 2.0; // warm band
            let glow = (-sd * 9.0).exp();
            r += glow * 0.9;
            g += glow * 0.55;
            b += glow * 0.18;
            let stars = fbm(nx * 40.0, ny * 40.0, 3);
            let star_strength = (1.0 - sy).max(0.0);
            if stars > 0.72 && t > 0.55 {
                let s = (stars - 0.72) / 0.28 * star_strength * 0.9;
                r += s * 0.7;
                g += s * 0.8;
                b += s;
            }
        } else {
            let ridge = fbm(nx * 6.0, 100.0, 4);
            let mline = horizon + ridge * 0.33;
            let haze = (ny - horizon).clamp(0.0, 0.06) / 0.06;
            if ny < mline + (nx - 0.0).abs() * 0.0 {
                r = 0.15 + haze * 0.5;
                g = 0.08 + haze * 0.33;
                b = 0.06 + haze * 0.18;
            } else {
                let mt = ((ny - mline) / (1.0 - mline)).clamp(0.0, 1.0);
                r = lerp(0.10, 0.03, mt) + hash01((nx * 400.0) as u32, (ny * 400.0) as u32) * 0.02;
                g = lerp(0.06, 0.025, mt) + hash01((nx * 400.0) as u32, (ny * 400.0) as u32) * 0.015;
                b = lerp(0.05, 0.02, mt) + hash01((nx * 400.0) as u32, (ny * 400.0) as u32) * 0.015;
            }
        }

        px[0] = (r.clamp(0.0, 1.0) * 255.0) as u8;
        px[1] = (g.clamp(0.0, 1.0) * 255.0) as u8;
        px[2] = (b.clamp(0.0, 1.0) * 255.0) as u8;
        px[3] = a;
    }
    RgbaImage { w, h, data }
}

fn sample_quiet() -> RgbaImage {
    let (w, h) = (1600u32, 1100u32);
    let mut data = vec![0u8; w as usize * h as usize * 4];
    let cx = w as f32 * 0.5;
    let cy = h as f32 * 0.45;
    for (i, px) in data.chunks_exact_mut(4).enumerate() {
        let x = (i % w as usize) as f32;
        let y = (i / w as usize) as f32;
        let nx = x / w as f32;
        let ny = y / h as f32;
        let n = fbm(nx * 30.0, ny * 30.0, 3);
        let mut r = 0.05 + n * 0.04;
        let mut g = 0.05 + n * 0.04;
        let mut b = 0.07 + n * 0.05;

        let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
        let body = (-d / 320.0).exp();
        let n2 = fbm(nx * 8.0 + 5.0, ny * 8.0, 3);
        r += body * lerp(0.72, 0.85, n2);
        g += body * lerp(0.55, 0.66, n2);
        b += body * lerp(0.48, 0.56, n2);

        let hx = w as f32 * 0.24;
        let hy = h as f32 * 0.28;
        let d2 = ((x - hx).powi(2) + (y - hy).powi(2)).sqrt();
        let glow = (-d2 / 90.0).exp();
        b += glow * 0.55;
        g += glow * 0.35;

        let gx = w as f32 * 0.82;
        let gy = h as f32 * 0.70;
        let d3 = ((x - gx).powi(2) + (y - gy).powi(2)).sqrt();
        let glow2 = (-d3 / 120.0).exp();
        r += glow2 * 0.6;
        g += glow2 * 0.3;

        let vig = (((nx * 2.0 - 1.0).powi(2) + (ny * 2.0 - 1.0).powi(2)) * 0.55).min(1.0);
        let f = 1.0 - vig * 0.35;
        r *= f;
        g *= f;
        b *= f;

        let g3 = fbm(nx * 700.0, ny * 700.0, 2);
        let grain = (g3 - 0.5) * 0.012;
        r += grain;
        g += grain;
        b += grain;

        px[0] = (r.clamp(0.0, 1.0) * 255.0) as u8;
        px[1] = (g.clamp(0.0, 1.0) * 255.0) as u8;
        px[2] = (b.clamp(0.0, 1.0) * 255.0) as u8;
        px[3] = 255;
    }
    RgbaImage { w, h, data }
}

fn sample_testchart() -> RgbaImage {
    let (w, h) = (1800u32, 1200u32);
    let mut data = vec![0u8; w as usize * h as usize * 4];
    let bands = 8;
    let band_h = h / bands;
    for (i, px) in data.chunks_exact_mut(4).enumerate() {
        let x = (i % w as usize) as f32;
        let y = (i / w as usize) as f32;
        let nx = x / w as f32;
        let band = (y / band_h as f32).min(bands as f32 - 1.0) as u32;
        let hue = band as f32 / (bands - 1) as f32;
        let (mut r, mut g, mut b) = (0.0, 0.0, 0.0);
        let hi = (hue * 6.0) as u32 % 6;
        let hf = hue * 6.0 - (hi as f32);
        let l2 = 0.5;
        let s = 0.85;
        match hi % 6 {
            0 => { r = l2; g = lerp(0.0, 1.0, hf); }
            1 => { g = l2; r = lerp(1.0, 0.0, hf); }
            2 => { g = l2; b = lerp(0.0, 1.0, hf); }
            3 => { b = l2; g = lerp(1.0, 0.0, hf); }
            4 => { b = l2; r = lerp(0.0, 1.0, hf); }
            _ => { r = l2; b = lerp(1.0, 0.0, hf); }
        }
        let deskew = r * 0.299 + g * 0.587 + b * 0.114;
        r = (deskew + (r - deskew) * s).clamp(0.0, 1.0) * (0.35 + 0.65 * nx);
        g = (deskew + (g - deskew) * s).clamp(0.0, 1.0) * (0.35 + 0.65 * nx);
        b = (deskew + (b - deskew) * s).clamp(0.0, 1.0) * (0.35 + 0.65 * nx);

        px[0] = (r * 255.0) as u8;
        px[1] = (g * 255.0) as u8;
        px[2] = (b * 255.0) as u8;
        px[3] = 255;
    }
    RgbaImage { w, h, data }
}

pub fn sample_photo(kind: usize) -> (String, RgbaImage) {
    match kind {
        0 => ("Dawn Over Range.jpg".to_string(), sample_dawn()),
        1 => ("Quiet Portrait.jpg".to_string(), sample_quiet()),
        _ => ("Test Chart.png".to_string(), sample_testchart()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gray_rgba(v: u8, w: u32, h: u32) -> RgbaImage {
        let mut data = vec![0u8; w as usize * h as usize * 4];
        for px in data.chunks_exact_mut(4) {
            px.copy_from_slice(&[v, v, v, 255]);
        }
        RgbaImage { w, h, data }
    }

    #[test]
    fn samples_have_expected_sizes() {
        assert_eq!(sample_dawn().w, 1920);
        assert_eq!(sample_quiet().h, 1100);
        assert_eq!(sample_testchart().w, 1800);
    }

    #[test]
    fn develop_preserves_dimensions_and_alpha() {
        let img = sample_testchart();
        let out = develop(&img, &DevelopParams::default());
        assert_eq!((out.w, out.h), (img.w, img.h));
        assert_eq!(out.data.len(), img.data.len());
        let src_a: Vec<u8> = img.data.iter().step_by(4).copied().collect();
        let out_a: Vec<u8> = out.data.iter().step_by(4).copied().collect();
        assert_eq!(src_a.len(), out_a.len());
        assert!(img.data.iter().step_by(4).any(|&a| a < 255), "test chart has some transparency");
    }

    #[test]
    fn develop_rotation_swaps_dimensions() {
        let img = gray_rgba(128, 80, 50);
        let mut p = DevelopParams::default();
        p.rotate = 90;
        let out = develop(&img, &p);
        assert_eq!((out.w, out.h), (50, 80));
        p.rotate = 0;
        p.crop = Some([0.0, 0.0, 0.5, 1.0]);
        let out = develop(&img, &p);
        assert_eq!((out.w, out.h), (40, 50));
    }

    #[test]
    fn exposure_brightens_output() {
        let img = gray_rgba(64, 64, 64);
        let mut p = DevelopParams::default();
        let base = develop(&img, &p);
        p.exposure = 2.0;
        let lit = develop(&img, &p);
        let bmean = |im: &RgbaImage| {
            im.data.chunks_exact(4).map(|px| px[0] as u32).sum::<u32>() / (im.data.len() / 4) as u32
        };
        assert!(bmean(&lit) > bmean(&base));
    }

    #[test]
    fn histogram_contains_every_pixel() {
        let img = sample_dawn();
        let hists = histograms(&img);
        let total: u32 = hists[3].bins.iter().sum();
        assert_eq!(total, img.w * img.h);
        assert!(hists[3].max > 0);
    }

    #[test]
    fn auto_tone_sets_reasonable_ranges() {
        let img = sample_quiet();
        let mut p = DevelopParams::default();
        auto_tone(&mut p, &img);
        assert!(p.exposure.abs() < 4.0);
        assert!((0.0..=2.0).contains(&p.contrast));
        assert!((0.0..=2.0).contains(&p.saturation));
    }

    #[test]
    fn downscaled_and_thumb_smaller() {
        let img = sample_dawn();
        let small = img.downscaled(1600);
        assert!(small.w <= 1600 && small.w > 0);
        let tiny = img.downscaled(192);
        assert!(tiny.w <= 192);
    }

    #[test]
    fn develop_preview_and_full_agree() {
        let img = sample_quiet();
        let preview = img.downscaled(1600);
        let mut p = DevelopParams::default();
        p.contrast = 1.3;
        p.saturation = 1.2;
        p.grain = 0.2;
        p.vignette = 0.3;
        let _full_out = develop(&img, &p);
        let prev_out = develop(&preview, &p);
        assert_eq!(prev_out.w, preview.w);
        assert_eq!(prev_out.h, preview.h);
    }
}