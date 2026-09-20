//! Destructive raster filters, evaluated from immutable source pixels.
//! RGB is straight-alpha at the boundary; spatial mixing uses premultiplied alpha.
use crate::color::Rgba;
use rayon::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bank {
    Filters,
    Effects,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    ChromaKey,
    BrightnessContrast,
    Exposure,
    Levels,
    HueSaturation,
    Vibrance,
    ColorBalance,
    Temperature,
    Grayscale,
    Sepia,
    Invert,
    Threshold,
    Posterize,
    AlphaThreshold,
    FeatherAlpha,
    GrowAlpha,
    ShrinkAlpha,
    GaussianBlur,
    MotionBlur,
    Sharpen,
    UnsharpMask,
    EdgeDetect,
    Emboss,
    Pixelate,
    Noise,
    Vignette,
    Halftone,
    Solarize,
    Swirl,
    Ripple,
}

impl Kind {
    pub const ALL: [Self; 30] = [
        Self::ChromaKey,
        Self::BrightnessContrast,
        Self::Exposure,
        Self::Levels,
        Self::HueSaturation,
        Self::Vibrance,
        Self::ColorBalance,
        Self::Temperature,
        Self::Grayscale,
        Self::Sepia,
        Self::Invert,
        Self::Threshold,
        Self::Posterize,
        Self::AlphaThreshold,
        Self::FeatherAlpha,
        Self::GrowAlpha,
        Self::ShrinkAlpha,
        Self::GaussianBlur,
        Self::MotionBlur,
        Self::Sharpen,
        Self::UnsharpMask,
        Self::EdgeDetect,
        Self::Emboss,
        Self::Pixelate,
        Self::Noise,
        Self::Vignette,
        Self::Halftone,
        Self::Solarize,
        Self::Swirl,
        Self::Ripple,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::ChromaKey => "Chroma key",
            Self::BrightnessContrast => "Brightness / contrast",
            Self::Exposure => "Exposure / gamma",
            Self::Levels => "Levels",
            Self::HueSaturation => "Hue / saturation",
            Self::Vibrance => "Vibrance",
            Self::ColorBalance => "Color balance",
            Self::Temperature => "Temperature / tint",
            Self::Grayscale => "Grayscale",
            Self::Sepia => "Sepia",
            Self::Invert => "Invert",
            Self::Threshold => "Threshold",
            Self::Posterize => "Posterize",
            Self::AlphaThreshold => "Alpha threshold",
            Self::FeatherAlpha => "Feather alpha",
            Self::GrowAlpha => "Grow alpha",
            Self::ShrinkAlpha => "Shrink alpha",
            Self::GaussianBlur => "Gaussian blur",
            Self::MotionBlur => "Motion blur",
            Self::Sharpen => "Sharpen",
            Self::UnsharpMask => "Unsharp mask",
            Self::EdgeDetect => "Find edges",
            Self::Emboss => "Emboss",
            Self::Pixelate => "Pixelate",
            Self::Noise => "Film grain",
            Self::Vignette => "Vignette",
            Self::Halftone => "Halftone",
            Self::Solarize => "Solarize",
            Self::Swirl => "Swirl",
            Self::Ripple => "Ripple",
        }
    }
    pub fn bank(self) -> Bank {
        if matches!(
            self,
            Self::GaussianBlur
                | Self::MotionBlur
                | Self::Sharpen
                | Self::UnsharpMask
                | Self::EdgeDetect
                | Self::Emboss
                | Self::Pixelate
                | Self::Noise
                | Self::Vignette
                | Self::Halftone
                | Self::Solarize
                | Self::Swirl
                | Self::Ripple
        ) {
            Bank::Effects
        } else {
            Bank::Filters
        }
    }
    pub fn category(self) -> &'static str {
        match self {
            Self::ChromaKey
            | Self::AlphaThreshold
            | Self::FeatherAlpha
            | Self::GrowAlpha
            | Self::ShrinkAlpha => "Key & transparency",
            Self::GaussianBlur | Self::MotionBlur => "Blur",
            Self::Sharpen | Self::UnsharpMask | Self::EdgeDetect | Self::Emboss => "Detail",
            Self::Pixelate | Self::Noise | Self::Vignette | Self::Halftone | Self::Solarize => {
                "Stylize"
            }
            Self::Swirl | Self::Ripple => "Distort",
            _ => "Color & tone",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::ChromaKey => {
                "Remove a sampled background color with soft edges and spill cleanup."
            }
            Self::Levels => "Set black and white points, then adjust midtone gamma.",
            Self::FeatherAlpha => "Soften the alpha channel without blurring the image colors.",
            Self::GrowAlpha => "Expand opaque edges; borrow nearby color for transparent pixels.",
            Self::ShrinkAlpha => "Contract opaque edges to clean a cutout.",
            Self::UnsharpMask => "Increase local contrast around details while preserving alpha.",
            Self::Noise => "Add repeatable monochrome grain without changing transparency.",
            _ => "Preview the result, then Apply to commit one undoable edit.",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub kind: Kind,
    pub values: [f32; 5],
    pub color: Rgba,
    pub strength: f32,
}

impl Settings {
    pub fn new(kind: Kind) -> Self {
        let values = match kind {
            Kind::ChromaKey => [0.08, 0.04, 0.0, 0.65, 0.0],
            Kind::Exposure => [0.0, 1.0, 0.0, 0.0, 0.0],
            Kind::Levels => [0.0, 1.0, 1.0, 0.0, 0.0],
            Kind::HueSaturation => [0.0, 0.0, 0.0, 0.0, 0.0],
            Kind::Vibrance => [0.35, 0.0, 0.0, 0.0, 0.0],
            Kind::Threshold | Kind::AlphaThreshold | Kind::Solarize => [0.5, 0.0, 0.0, 0.0, 0.0],
            Kind::Posterize => [5.0, 0.0, 0.0, 0.0, 0.0],
            Kind::GaussianBlur | Kind::FeatherAlpha => [4.0, 0.0, 0.0, 0.0, 0.0],
            Kind::GrowAlpha | Kind::ShrinkAlpha => [2.0, 0.0, 0.0, 0.0, 0.0],
            Kind::MotionBlur => [12.0, 0.0, 0.0, 0.0, 0.0],
            Kind::Sharpen => [0.6, 0.0, 0.0, 0.0, 0.0],
            Kind::UnsharpMask => [3.0, 1.0, 0.02, 0.0, 0.0],
            Kind::EdgeDetect | Kind::Emboss => [1.0, 0.0, 0.0, 0.0, 0.0],
            Kind::Pixelate | Kind::Halftone => [12.0, 0.0, 0.0, 0.0, 0.0],
            Kind::Noise => [0.12, 0.0, 0.0, 0.0, 0.0],
            Kind::Vignette => [0.65, 0.3, 0.0, 0.0, 0.0],
            Kind::Swirl => [90.0, 0.8, 0.0, 0.0, 0.0],
            Kind::Ripple => [8.0, 40.0, 0.0, 0.0, 0.0],
            _ => [0.0; 5],
        };
        Self {
            kind,
            values,
            color: Rgba::rgb(0, 255, 0),
            strength: 1.0,
        }
    }
    /// Pixel distances track source resolution when rendering a smaller preview.
    pub fn scaled(&self, scale: f32) -> Self {
        let mut out = self.clone();
        match out.kind {
            Kind::GaussianBlur | Kind::FeatherAlpha | Kind::MotionBlur | Kind::UnsharpMask => {
                out.values[0] *= scale
            }
            Kind::GrowAlpha | Kind::ShrinkAlpha | Kind::Pixelate | Kind::Halftone => {
                out.values[0] = (out.values[0] * scale).max(1.0)
            }
            Kind::Ripple => {
                out.values[0] *= scale;
                out.values[1] *= scale;
            }
            _ => {}
        }
        out
    }
}

fn byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}
fn luma(c: &[f32; 4]) -> f32 {
    c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722
}
fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a).max(0.00001)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
fn chroma(c: &[f32; 4]) -> [f32; 2] {
    let y = c[0] * 0.299 + c[1] * 0.587 + c[2] * 0.114;
    [(c[2] - y) * 0.565, (c[0] - y) * 0.713]
}
fn key_pixel(mut c: [f32; 4], s: &Settings) -> [f32; 4] {
    let key = [
        s.color.r as f32 / 255.0,
        s.color.g as f32 / 255.0,
        s.color.b as f32 / 255.0,
        1.0,
    ];
    let uv = chroma(&c);
    let kuv = chroma(&key);
    // A neutral key needs luminance discrimination: white must not also key black.
    let key_chroma = (kuv[0] * kuv[0] + kuv[1] * kuv[1]).sqrt();
    let neutral_weight = 1.0 - smooth(0.05, 0.20, key_chroma);
    let distance = ((uv[0] - kuv[0]).powi(2)
        + (uv[1] - kuv[1]).powi(2)
        + (luma(&c) - luma(&key)).powi(2) * neutral_weight)
        .sqrt();
    let falloff = (s.values[1] * (1.0 - s.values[2].clamp(0.0, 1.0))).max(0.00001);
    let keep = smooth(s.values[0], s.values[0] + falloff, distance);
    c[3] *= keep;
    // Subtract only the excess of the key's dominant color, never replace alpha.
    let dominant = if key[1] > key[0] && key[1] >= key[2] {
        1
    } else if key[2] > key[0] {
        2
    } else {
        0
    };
    let a = (dominant + 1) % 3;
    let b = (dominant + 2) % 3;
    let excess = (c[dominant] - c[a].max(c[b])).max(0.0);
    let near_key = 1.0
        - smooth(
            s.values[0] + falloff,
            s.values[0] + falloff + 0.25,
            distance,
        );
    c[dominant] -= excess * s.values[3].clamp(0.0, 1.0) * near_key;
    c
}

/// Selection values are coverage (0..255). No selection means the entire buffer.
/// A malformed source or selection is rejected instead of partially editing it.
pub fn apply(
    source: &[u8],
    w: u32,
    h: u32,
    settings: &Settings,
    selection: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    let count = (w as usize)
        .checked_mul(h as usize)
        .ok_or("Image is too large")?;
    if count == 0 || count.checked_mul(4) != Some(source.len()) {
        return Err("Invalid image dimensions".into());
    }
    if selection.is_some_and(|m| m.len() != count) {
        return Err("Selection dimensions no longer match this layer".into());
    }
    let src: Vec<[f32; 4]> = source
        .par_chunks_exact(4)
        .map(|p| {
            [
                p[0] as f32 / 255.0,
                p[1] as f32 / 255.0,
                p[2] as f32 / 255.0,
                p[3] as f32 / 255.0,
            ]
        })
        .collect();
    let p = settings.values;
    let mut out = match settings.kind {
        Kind::GaussianBlur => blur(&src, w, h, p[0]),
        Kind::FeatherAlpha => {
            let blurred = blur(&src, w, h, p[0]);
            src.par_iter()
                .zip(blurred.par_iter())
                .map(|(a, b)| {
                    // Newly revealed pixels need nearby visible color, never hidden RGB.
                    if a[3] <= 0.00001 {
                        *b
                    } else {
                        [a[0], a[1], a[2], b[3]]
                    }
                })
                .collect()
        }
        Kind::GrowAlpha | Kind::ShrinkAlpha => morphology(
            &src,
            w,
            h,
            p[0].round().clamp(1.0, 32.0) as i32,
            settings.kind == Kind::GrowAlpha,
        ),
        Kind::MotionBlur => motion_blur(&src, w, h, p[0], p[1]),
        Kind::Sharpen | Kind::UnsharpMask => {
            let blurred = blur(
                &src,
                w,
                h,
                if settings.kind == Kind::Sharpen {
                    1.0
                } else {
                    p[0]
                },
            );
            src.par_iter()
                .zip(blurred.par_iter())
                .map(|(c, b)| {
                    let amount = if settings.kind == Kind::Sharpen {
                        p[0] * 3.0
                    } else {
                        p[1]
                    };
                    let threshold = if settings.kind == Kind::Sharpen {
                        0.0
                    } else {
                        p[2]
                    };
                    let mut v = *c;
                    for ch in 0..3 {
                        if (c[ch] - b[ch]).abs() >= threshold {
                            v[ch] = c[ch] + (c[ch] - b[ch]) * amount;
                        }
                    }
                    v
                })
                .collect()
        }
        Kind::EdgeDetect | Kind::Emboss => convolution(&src, w, h, settings),
        Kind::Pixelate | Kind::Halftone => pixelate(
            &src,
            w,
            h,
            p[0].round().max(1.0) as u32,
            settings.kind == Kind::Halftone,
        ),
        Kind::Swirl | Kind::Ripple => distort(&src, w, h, settings),
        _ => src
            .par_iter()
            .enumerate()
            .map(|(i, c)| {
                let mut v = *c;
                let y = luma(c);
                match settings.kind {
                    Kind::ChromaKey => v = key_pixel(v, settings),
                    Kind::BrightnessContrast => {
                        for ch in 0..3 {
                            v[ch] = (c[ch] - 0.5) * 2.0f32.powf(p[1] * 2.0) + 0.5 + p[0];
                        }
                    }
                    Kind::Exposure => {
                        for ch in 0..3 {
                            v[ch] = (c[ch] * 2.0f32.powf(p[0]))
                                .max(0.0)
                                .powf(1.0 / p[1].max(0.05));
                        }
                    }
                    Kind::Levels => {
                        for ch in 0..3 {
                            v[ch] = ((c[ch] - p[0]) / (p[1] - p[0]).max(0.001))
                                .clamp(0.0, 1.0)
                                .powf(1.0 / p[2].max(0.05));
                        }
                    }
                    Kind::HueSaturation => {
                        let mut hsv = rgb_to_hsv(*c);
                        hsv[0] = (hsv[0] + p[0] / 360.0).rem_euclid(1.0);
                        hsv[1] = (hsv[1] * (1.0 + p[1])).clamp(0.0, 1.0);
                        let rgb = hsv_to_rgb(hsv);
                        for ch in 0..3 {
                            v[ch] = if p[2] >= 0.0 {
                                rgb[ch] + (1.0 - rgb[ch]) * p[2]
                            } else {
                                rgb[ch] * (1.0 + p[2])
                            };
                        }
                    }
                    Kind::Vibrance => {
                        let max = c[0].max(c[1]).max(c[2]);
                        let min = c[0].min(c[1]).min(c[2]);
                        let amount = 1.0 + p[0] * (1.0 - (max - min));
                        for ch in 0..3 {
                            v[ch] = y + (c[ch] - y) * amount;
                        }
                    }
                    Kind::ColorBalance => {
                        for ch in 0..3 {
                            v[ch] += p[ch] * 0.5;
                        }
                    }
                    Kind::Temperature => {
                        v[0] += p[0] * 0.2 + p[1] * 0.05;
                        v[1] -= p[1] * 0.15;
                        v[2] -= p[0] * 0.2 - p[1] * 0.05;
                    }
                    Kind::Grayscale => v[..3].fill(y),
                    Kind::Sepia => {
                        v[0] = c[0] * 0.393 + c[1] * 0.769 + c[2] * 0.189;
                        v[1] = c[0] * 0.349 + c[1] * 0.686 + c[2] * 0.168;
                        v[2] = c[0] * 0.272 + c[1] * 0.534 + c[2] * 0.131;
                    }
                    Kind::Invert => {
                        for ch in 0..3 {
                            v[ch] = 1.0 - c[ch];
                        }
                    }
                    Kind::Threshold => v[..3].fill(if y >= p[0] { 1.0 } else { 0.0 }),
                    Kind::Posterize => {
                        let steps = (p[0].round().max(2.0) - 1.0).max(1.0);
                        for ch in 0..3 {
                            v[ch] = (c[ch] * steps).round() / steps;
                        }
                    }
                    Kind::AlphaThreshold => {
                        v[3] = if c[3] >= p[0] && c[3] > 0.0 { 1.0 } else { 0.0 }
                    }
                    Kind::Noise => {
                        let n = (hash(i as u32) as f32 / u32::MAX as f32 - 0.5) * p[0];
                        for ch in 0..3 {
                            v[ch] += n;
                        }
                    }
                    Kind::Vignette => {
                        let x = ((i % w as usize) as f32 + 0.5) / w as f32 * 2.0 - 1.0;
                        let y = ((i / w as usize) as f32 + 0.5) / h as f32 * 2.0 - 1.0;
                        let amount = smooth(p[1], 1.4143, (x * x + y * y).sqrt()) * p[0];
                        for ch in 0..3 {
                            v[ch] *= 1.0 - amount;
                        }
                    }
                    Kind::Solarize => {
                        for ch in 0..3 {
                            if c[ch] > p[0] {
                                v[ch] = 1.0 - c[ch];
                            }
                        }
                    }
                    _ => {}
                }
                v
            })
            .collect(),
    };
    // Interpolate premultiplied colors so alpha-changing filters never create dark fringes.
    let strength = settings.strength.clamp(0.0, 1.0);
    out.par_iter_mut().enumerate().for_each(|(i, pixel)| {
        let amount = strength * selection.map_or(1.0, |mask| mask[i] as f32 / 255.0);
        if amount <= 0.0 {
            *pixel = src[i];
            return;
        }
        if amount >= 1.0 {
            return;
        }
        let original = src[i];
        let alpha = original[3] * (1.0 - amount) + pixel[3] * amount;
        for ch in 0..3 {
            pixel[ch] = if alpha > 0.00001 {
                (original[ch] * original[3] * (1.0 - amount)
                    + pixel[ch].clamp(0.0, 1.0) * pixel[3] * amount)
                    / alpha
            } else {
                original[ch]
            };
        }
        pixel[3] = alpha;
    });
    Ok(out
        .par_iter()
        .flat_map_iter(|p| p.iter().map(|&v| byte(v)))
        .collect())
}

fn hash(mut n: u32) -> u32 {
    n ^= 0x9e3779b9;
    n ^= n >> 16;
    n = n.wrapping_mul(0x7feb352d);
    n ^= n >> 15;
    n = n.wrapping_mul(0x846ca68b);
    n ^ (n >> 16)
}
fn rgb_to_hsv(c: [f32; 4]) -> [f32; 3] {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    let d = max - min;
    let hue = if d < 0.00001 {
        0.0
    } else if max == c[0] {
        ((c[1] - c[2]) / d).rem_euclid(6.0)
    } else if max == c[1] {
        (c[2] - c[0]) / d + 2.0
    } else {
        (c[0] - c[1]) / d + 4.0
    };
    [hue / 6.0, if max > 0.0 { d / max } else { 0.0 }, max]
}
fn hsv_to_rgb(c: [f32; 3]) -> [f32; 3] {
    let h = c[0] * 6.0;
    let x = c[2] * (1.0 - c[1]);
    let y = c[2] * (1.0 - (h - h.floor()) * c[1]);
    let z = c[2] * (1.0 - (1.0 - (h - h.floor())) * c[1]);
    match h.floor() as i32 % 6 {
        0 => [c[2], z, x],
        1 => [y, c[2], x],
        2 => [x, c[2], z],
        3 => [x, y, c[2]],
        4 => [z, x, c[2]],
        _ => [c[2], x, y],
    }
}
fn premul(c: [f32; 4]) -> [f32; 4] {
    [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]]
}
fn unpremul(mut c: [f32; 4]) -> [f32; 4] {
    if c[3] > 0.00001 {
        for ch in 0..3 {
            c[ch] /= c[3];
        }
    }
    c
}
fn at(src: &[[f32; 4]], w: u32, h: u32, x: i32, y: i32) -> [f32; 4] {
    src[y.clamp(0, h as i32 - 1) as usize * w as usize + x.clamp(0, w as i32 - 1) as usize]
}
fn sample(src: &[[f32; 4]], w: u32, h: u32, x: f32, y: f32) -> [f32; 4] {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let fx = x - x.floor();
    let fy = y - y.floor();
    let mut out = [0.0; 4];
    for (dx, dy, weight) in [
        (0, 0, (1.0 - fx) * (1.0 - fy)),
        (1, 0, fx * (1.0 - fy)),
        (0, 1, (1.0 - fx) * fy),
        (1, 1, fx * fy),
    ] {
        let p = premul(at(src, w, h, ix + dx, iy + dy));
        for ch in 0..4 {
            out[ch] += p[ch] * weight;
        }
    }
    unpremul(out)
}
fn blur(src: &[[f32; 4]], w: u32, h: u32, radius: f32) -> Vec<[f32; 4]> {
    let input: Vec<_> = src.par_iter().copied().map(premul).collect();
    blur_channels(&input, w, h, radius)
        .into_par_iter()
        .map(unpremul)
        .collect()
}
fn blur_channels(src: &[[f32; 4]], w: u32, h: u32, radius: f32) -> Vec<[f32; 4]> {
    if radius < 0.1 {
        return src.to_vec();
    }
    let r = (radius * 0.58).round().clamp(1.0, 128.0) as i32;
    let mut out = src.to_vec();
    // Three separable box passes approximate a Gaussian in linear time.
    for _ in 0..3 {
        let mut tmp = vec![[0.0; 4]; out.len()];
        tmp.par_chunks_mut(w as usize)
            .enumerate()
            .for_each(|(y, row)| {
                let mut sum = [0.0; 4];
                for dx in -r..=r {
                    let p = at(&out, w, h, dx, y as i32);
                    for ch in 0..4 {
                        sum[ch] += p[ch];
                    }
                }
                for (x, p) in row.iter_mut().enumerate() {
                    for ch in 0..4 {
                        p[ch] = sum[ch] / (2 * r + 1) as f32;
                    }
                    let a = at(&out, w, h, x as i32 - r, y as i32);
                    let b = at(&out, w, h, x as i32 + r + 1, y as i32);
                    for ch in 0..4 {
                        sum[ch] += b[ch] - a[ch];
                    }
                }
            });
        // Transpose to keep vertical sliding-window passes parallel and contiguous.
        let mut transposed = vec![[0.0; 4]; out.len()];
        transposed
            .par_chunks_mut(h as usize)
            .enumerate()
            .for_each(|(x, col)| {
                let mut sum = [0.0; 4];
                for dy in -r..=r {
                    let p = at(&tmp, w, h, x as i32, dy);
                    for ch in 0..4 {
                        sum[ch] += p[ch];
                    }
                }
                for (y, p) in col.iter_mut().enumerate() {
                    for ch in 0..4 {
                        p[ch] = sum[ch] / (2 * r + 1) as f32;
                    }
                    let a = at(&tmp, w, h, x as i32, y as i32 - r);
                    let b = at(&tmp, w, h, x as i32, y as i32 + r + 1);
                    for ch in 0..4 {
                        sum[ch] += b[ch] - a[ch];
                    }
                }
            });
        out.par_chunks_mut(w as usize)
            .enumerate()
            .for_each(|(y, row)| {
                for (x, p) in row.iter_mut().enumerate() {
                    *p = transposed[x * h as usize + y];
                }
            });
    }
    out
}
fn morphology(src: &[[f32; 4]], w: u32, h: u32, r: i32, grow: bool) -> Vec<[f32; 4]> {
    let mut out = src.to_vec();
    // Separable square structuring element. Keep the winning pixel's RGB when growing.
    for vertical in [false, true] {
        let input = out;
        out = input
            .par_iter()
            .enumerate()
            .map(|(i, p)| {
                let x = (i % w as usize) as i32;
                let y = (i / w as usize) as i32;
                let mut best = *p;
                for d in -r..=r {
                    let c = at(
                        &input,
                        w,
                        h,
                        x + if vertical { 0 } else { d },
                        y + if vertical { d } else { 0 },
                    );
                    if (grow && c[3] > best[3]) || (!grow && c[3] < best[3]) {
                        best = c;
                    }
                }
                if grow {
                    best
                } else {
                    [p[0], p[1], p[2], best[3]]
                }
            })
            .collect();
    }
    out
}
fn motion_blur(src: &[[f32; 4]], w: u32, h: u32, length: f32, angle: f32) -> Vec<[f32; 4]> {
    let n = length.round().clamp(1.0, 128.0) as u32;
    let theta = angle.to_radians();
    src.par_iter()
        .enumerate()
        .map(|(i, _)| {
            let x = (i % w as usize) as f32;
            let y = (i / w as usize) as f32;
            let mut sum = [0.0; 4];
            for step in 0..n {
                let d = step as f32 - (n - 1) as f32 * 0.5;
                let p = premul(sample(src, w, h, x + d * theta.cos(), y + d * theta.sin()));
                for ch in 0..4 {
                    sum[ch] += p[ch] / n as f32;
                }
            }
            unpremul(sum)
        })
        .collect()
}
fn convolution(src: &[[f32; 4]], w: u32, h: u32, s: &Settings) -> Vec<[f32; 4]> {
    src.par_iter()
        .enumerate()
        .map(|(i, c)| {
            let x = (i % w as usize) as i32;
            let y = (i / w as usize) as i32;
            let mut v = [0.0, 0.0, 0.0, c[3]];
            let kernel = if s.kind == Kind::Emboss {
                [-2.0, -1.0, 0.0, -1.0, 0.0, 1.0, 0.0, 1.0, 2.0]
            } else {
                [-1.0, -1.0, -1.0, -1.0, 8.0, -1.0, -1.0, -1.0, -1.0]
            };
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let p = at(src, w, h, x + dx, y + dy);
                    let weight = kernel[((dy + 1) * 3 + dx + 1) as usize];
                    for ch in 0..3 {
                        v[ch] += p[ch] * weight;
                    }
                }
            }
            for ch in 0..3 {
                v[ch] = if s.kind == Kind::Emboss {
                    0.5 + v[ch] * s.values[0]
                } else {
                    v[ch].abs() * s.values[0]
                };
            }
            v
        })
        .collect()
}
fn pixelate(src: &[[f32; 4]], w: u32, h: u32, size: u32, halftone: bool) -> Vec<[f32; 4]> {
    let bw = w.div_ceil(size);
    let bh = h.div_ceil(size);
    let cells: Vec<[f32; 4]> = (0..bw * bh)
        .into_par_iter()
        .map(|i| {
            let x = i % bw * size;
            let y = i / bw * size;
            let mut sum = [0.0; 4];
            let mut n = 0;
            for sy in y..(y + size).min(h) {
                for sx in x..(x + size).min(w) {
                    let p = premul(src[(sy * w + sx) as usize]);
                    for ch in 0..4 {
                        sum[ch] += p[ch];
                    }
                    n += 1;
                }
            }
            for ch in 0..4 {
                sum[ch] /= n as f32;
            }
            unpremul(sum)
        })
        .collect();
    src.par_iter()
        .enumerate()
        .map(|(i, c)| {
            let x = i as u32 % w;
            let y = i as u32 / w;
            let cell = cells[(y / size * bw + x / size) as usize];
            if !halftone {
                return cell;
            }
            let dx = (x % size) as f32 + 0.5 - size as f32 * 0.5;
            let dy = (y % size) as f32 + 0.5 - size as f32 * 0.5;
            let radius = (1.0 - luma(&cell)).sqrt() * size as f32 * 0.7072;
            let v = smooth(radius - 0.5, radius + 0.5, (dx * dx + dy * dy).sqrt());
            [v, v, v, c[3]]
        })
        .collect()
}
fn distort(src: &[[f32; 4]], w: u32, h: u32, s: &Settings) -> Vec<[f32; 4]> {
    src.par_iter()
        .enumerate()
        .map(|(i, c)| {
            let x = (i % w as usize) as f32;
            let y = (i / w as usize) as f32;
            let cx = (w - 1) as f32 * 0.5;
            let cy = (h - 1) as f32 * 0.5;
            let dx = x - cx;
            let dy = y - cy;
            let r = (dx * dx + dy * dy).sqrt();
            if s.kind == Kind::Swirl {
                let radius = w.min(h) as f32 * 0.5 * s.values[1];
                if r >= radius || radius < 0.01 {
                    return *c;
                }
                let theta = s.values[0].to_radians() * (1.0 - r / radius).powi(2);
                let (sin, cos) = theta.sin_cos();
                sample(
                    src,
                    w,
                    h,
                    cx + dx * cos - dy * sin,
                    cy + dx * sin + dy * cos,
                )
            } else {
                let d = (r / s.values[1].max(1.0) * std::f32::consts::TAU).sin() * s.values[0];
                let scale = if r > 0.01 { (r + d) / r } else { 1.0 };
                sample(src, w, h, cx + dx * scale, cy + dy * scale)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn key_removes_green_preserves_subject_and_existing_alpha() {
        let source = [0, 255, 0, 255, 240, 30, 20, 128, 0, 0, 0, 0];
        let out = apply(&source, 3, 1, &Settings::new(Kind::ChromaKey), None).unwrap();
        assert_eq!(out[3], 0);
        assert_eq!(&out[4..8], &source[4..8]);
        assert_eq!(out[11], 0);
    }
    #[test]
    fn key_falloff_is_soft_and_hardness_tightens_it() {
        let source = [60, 170, 70, 255];
        let mut s = Settings::new(Kind::ChromaKey);
        s.values = [0.05, 0.6, 0.0, 0.0, 0.0];
        let soft = apply(&source, 1, 1, &s, None).unwrap()[3];
        assert!(soft > 0 && soft < 255);
        s.values[2] = 1.0;
        assert_eq!(apply(&source, 1, 1, &s, None).unwrap()[3], 255);
    }
    #[test]
    fn selection_limits_key_and_strength_preserves_partial_alpha() {
        let source = [0, 255, 0, 128, 0, 255, 0, 255];
        let s = Settings::new(Kind::ChromaKey);
        let out = apply(&source, 2, 1, &s, Some(&[0, 128])).unwrap();
        assert_eq!(&out[..4], &source[..4]);
        assert!((out[7] as i32 - 127).abs() <= 1);
        assert!(apply(&source, 2, 1, &s, Some(&[255])).is_err());
    }
    #[test]
    fn blur_does_not_bleed_hidden_rgb_into_opaque_edges() {
        let source = [255, 0, 0, 255, 0, 255, 0, 0, 0, 255, 0, 0];
        let mut s = Settings::new(Kind::GaussianBlur);
        s.values[0] = 2.0;
        let out = apply(&source, 3, 1, &s, None).unwrap();
        for p in out.chunks_exact(4) {
            if p[3] > 0 {
                assert_eq!(p[0], 255);
                assert_eq!(p[1], 0);
            }
        }
    }
    #[test]
    fn banks_are_complete_finite_and_preserve_buffer_size() {
        let source: Vec<u8> = (0..64)
            .flat_map(|i| {
                [
                    (i * 3) as u8,
                    (i * 2) as u8,
                    i as u8,
                    if i % 3 == 0 { 0 } else { 180 },
                ]
            })
            .collect();
        for kind in Kind::ALL {
            let out = apply(&source, 8, 8, &Settings::new(kind), None).unwrap();
            assert_eq!(out.len(), source.len(), "{}", kind.name());
        }
    }
    #[test]
    fn zero_strength_is_exact_identity_for_every_effect() {
        let source = [
            255, 0, 0, 0, 20, 70, 180, 123, 100, 40, 3, 255, 255, 255, 255, 255,
        ];
        for kind in Kind::ALL {
            let mut s = Settings::new(kind);
            s.strength = 0.0;
            assert_eq!(apply(&source, 2, 2, &s, None).unwrap(), source);
        }
    }
    #[test]
    fn neutral_key_distinguishes_white_from_black() {
        let source = [255, 255, 255, 255, 0, 0, 0, 255, 255, 0, 0, 255];
        let mut settings = Settings::new(Kind::ChromaKey);
        settings.color = Rgba::WHITE;
        let out = apply(&source, 3, 1, &settings, None).unwrap();
        assert_eq!(out[3], 0);
        assert_eq!(out[7], 255);
        assert_eq!(out[11], 255);
    }
    #[test]
    fn feathered_alpha_borrows_visible_color_instead_of_hidden_rgb() {
        let source = [255, 0, 0, 255, 0, 255, 0, 0, 0, 255, 0, 0];
        let out = apply(&source, 3, 1, &Settings::new(Kind::FeatherAlpha), None).unwrap();
        assert!(out[7] > 0);
        assert_eq!(out[4], 255);
        assert_eq!(out[5], 0);
    }
    #[test]
    fn sampled_lime_defaults_preserve_neutral_clothing_skin_hair_and_beard() {
        let mut settings = Settings::new(Kind::ChromaKey);
        settings.color = Rgba::rgb(132, 181, 89);
        let source = [
            132, 181, 89, 255, // sampled screen
            94, 138, 63, 255, // screen shadow
            225, 221, 217, 255, // neutral fabric
            255, 255, 255, 255, // white glasses
            208, 151, 134, 255, // skin
            141, 123, 99, 255, // warm hair
            93, 86, 74, 255, // neutral beard
        ];
        let out = apply(&source, 7, 1, &settings, None).unwrap();
        assert_eq!(out[3], 0);
        assert_eq!(out[7], 0);
        assert_eq!(&out[8..], &source[8..]);
    }
}
