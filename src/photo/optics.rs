//! Source-relative optical calibration. Coefficients describe the inverse map
//! from a corrected image into the untouched, already oriented linear source.
use super::DevelopSource;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LensProfile {
    pub name: String,
    pub camera: String,
    pub lens: String,
    pub focal_length: f32,
    pub aperture: f32,
    pub center: [f32; 2],
    pub distortion: [f32; 3],
    pub chromatic_aberration: [f32; 2],
    pub vignette: [f32; 2],
}

impl Default for LensProfile {
    fn default() -> Self {
        Self {
            name: "Manual calibration".into(),
            camera: String::new(),
            lens: String::new(),
            focal_length: 0.,
            aperture: 0.,
            center: [0.5, 0.5],
            distortion: [0.; 3],
            chromatic_aberration: [0.; 2],
            vignette: [0.; 2],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OpticalSettings {
    pub enabled: bool,
    pub profile: LensProfile,
    pub perspective: [f32; 2],
    pub straighten: f32,
    pub scale: f32,
}

impl Default for OpticalSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            profile: LensProfile::default(),
            perspective: [0.; 2],
            straighten: 0.,
            scale: 1.,
        }
    }
}

impl OpticalSettings {
    /// Identify neutral settings for legacy preset compatibility.
    /// Args: none. Returns true when all optical settings are defaults.
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Check calibration values before rendering.
    /// Args: none. Returns true for finite, bounded geometry and gain coefficients.
    pub fn valid(&self) -> bool {
        let within = |v: f32, lo, hi| v.is_finite() && (lo..=hi).contains(&v);
        let p = &self.profile;
        [&p.name, &p.camera, &p.lens]
            .iter()
            .all(|s| s.len() <= 256 && !s.chars().any(char::is_control))
            && within(p.focal_length, 0., 3000.)
            && within(p.aperture, 0., 128.)
            && p.center.iter().all(|&v| within(v, 0., 1.))
            && p.distortion.iter().all(|&v| within(v, -1., 1.))
            && p.chromatic_aberration
                .iter()
                .all(|&v| within(v, -0.05, 0.05))
            && p.vignette.iter().all(|&v| within(v, -4., 4.))
            && self.perspective.iter().all(|&v| within(v, -0.5, 0.5))
            && within(self.straighten, -45., 45.)
            && within(self.scale, 0.25, 4.)
    }

    pub(super) fn active(&self) -> bool {
        self.enabled
            && (self.profile.distortion != [0.; 3]
                || self.profile.chromatic_aberration != [0.; 2]
                || self.profile.vignette != [0.; 2]
                || self.perspective != [0.; 2]
                || self.straighten != 0.
                || self.scale != 1.)
    }

    /// Map a corrected point into source coordinates.
    /// Args: normalized output x/y, source aspect ratio, RGB channel. Returns source UV and radial gain.
    pub fn source_point(&self, uv: [f32; 2], aspect: f32, channel: usize) -> ([f32; 2], f32) {
        let center = self.profile.center;
        let x = (uv[0] - center[0]) * 2. * aspect / self.scale;
        let y = (uv[1] - center[1]) * 2. / self.scale;
        let (sin, cos) = self.straighten.to_radians().sin_cos();
        let (x, y) = (x * cos - y * sin, x * sin + y * cos);
        let denominator = 1. + self.perspective[0] * x + self.perspective[1] * y;
        if denominator <= 0.01 {
            return ([-1.; 2], 0.);
        }
        let (x, y) = (x / denominator, y / denominator);
        let r2 = (x * x + y * y) / (aspect * aspect + 1.);
        let [k1, k2, k3] = self.profile.distortion;
        let ca = match channel {
            0 => self.profile.chromatic_aberration[0],
            2 => self.profile.chromatic_aberration[1],
            _ => 0.,
        };
        let radial = (1. + r2 * (k1 + r2 * (k2 + r2 * k3))) * (1. + ca);
        let gain = 2f32.powf(
            (self.profile.vignette[0] * r2 + self.profile.vignette[1] * r2 * r2).clamp(-8., 8.),
        );
        (
            [
                center[0] + x * radial / (2. * aspect),
                center[1] + y * radial / 2.,
            ],
            gain,
        )
    }

    pub(super) fn sample(&self, src: DevelopSource<'_>, index: usize) -> [f32; 4] {
        let uv = [
            (index % src.w as usize) as f32 / src.w as f32 + 0.5 / src.w as f32,
            (index / src.w as usize) as f32 / src.h as f32 + 0.5 / src.h as f32,
        ];
        let mut out = [0.; 4];
        let mut alpha: f32 = 1.;
        for channel in 0..3 {
            let (point, gain) = self.source_point(uv, src.w as f32 / src.h as f32, channel);
            let pixel = sample_linear(src, point);
            out[channel] = pixel[channel] * gain;
            alpha = alpha.min(pixel[3]);
        }
        out[3] = alpha;
        out
    }
}

/// Bilinearly sample straight linear RGB with premultiplied alpha interpolation.
/// Args: immutable source and normalized source point. Returns linear RGBA, transparent outside the source.
fn sample_linear(src: DevelopSource<'_>, uv: [f32; 2]) -> [f32; 4] {
    if uv.iter().any(|v| !v.is_finite() || !(0. ..=1.).contains(v)) {
        return [0.; 4];
    }
    let x = (uv[0] * src.w as f32 - 0.5).clamp(0., (src.w - 1) as f32);
    let y = (uv[1] * src.h as f32 - 0.5).clamp(0., (src.h - 1) as f32);
    let (x0, y0) = (x.floor() as u32, y.floor() as u32);
    let (x1, y1) = ((x0 + 1).min(src.w - 1), (y0 + 1).min(src.h - 1));
    let (fx, fy) = (x.fract(), y.fract());
    let mut out = [0.; 4];
    for (sx, sy, weight) in [
        (x0, y0, (1. - fx) * (1. - fy)),
        (x1, y0, fx * (1. - fy)),
        (x0, y1, (1. - fx) * fy),
        (x1, y1, fx * fy),
    ] {
        let p = src.pixel((sy * src.w + sx) as usize);
        out[3] += p[3] * weight;
        for c in 0..3 {
            let linear = if src.linear.is_some() {
                p[c]
            } else {
                srgb_decode(p[c])
            };
            out[c] += linear * p[3] * weight;
        }
    }
    if out[3] > 0. {
        for c in 0..3 {
            out[c] /= out[3];
        }
    }
    out
}

pub(super) fn srgb_decode(x: f32) -> f32 {
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::photo::{DevelopParams, PhotoImage, develop_inner};

    #[test]
    fn radial_map_matches_polynomial_and_channel_calibration() {
        let mut settings = OpticalSettings::default();
        settings.profile.distortion = [0.12, -0.03, 0.01];
        settings.profile.chromatic_aberration = [0.015, -0.01];
        settings.profile.vignette = [0.6, 0.2];
        let x: f32 = 0.6;
        let y: f32 = -0.4;
        let radius = (x * x + y * y) / 2.;
        let polynomial = 1. + 0.12 * radius - 0.03 * radius.powi(2) + 0.01 * radius.powi(3);
        for (channel, scale) in [(0, 1.015), (1, 1.), (2, 0.99)] {
            let (uv, gain) = settings.source_point([0.8, 0.3], 1., channel);
            assert!((uv[0] - (0.5 + x * polynomial * scale / 2.)).abs() < 1e-6);
            assert!((uv[1] - (0.5 + y * polynomial * scale / 2.)).abs() < 1e-6);
            assert!((gain - 2f32.powf(0.6 * radius + 0.2 * radius.powi(2))).abs() < 1e-6);
        }
    }

    #[test]
    fn optical_gain_runs_before_transfer_and_does_not_touch_sensor_values() {
        let pixels = [8000u16, 10000, 12000].repeat(16);
        let original = pixels.clone();
        let mut params = DevelopParams::default();
        params.optics.profile.vignette = [1., 0.];
        let (_, _, out) = develop_inner::<u16>(DevelopSource::linear(4, 4, &pixels), &params);
        let r2: f32 = (0.75 * 0.75 + 0.75 * 0.75) / 2.;
        let expected = super::super::srgb_encode(8000. / 65535. * 2f32.powf(r2));
        assert_eq!(out[0], (expected * 65535.).round() as u16);
        assert_eq!(pixels, original);
        params.optics.enabled = false;
        let (_, _, disabled) = develop_inner::<u16>(DevelopSource::linear(4, 4, &pixels), &params);
        assert!(out[0] > disabled[0]);
        assert_eq!(
            disabled[0],
            (super::super::srgb_encode(8000. / 65535.) * 65535.).round() as u16
        );
    }

    #[test]
    fn camera_fixture_saves_reopens_and_exports_sixteen_bit_optical_settings() {
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".artifacts")
            .join(format!(
                "photo-optics-{}-{}",
                std::process::id(),
                crate::document::next_id()
            ));
        std::fs::create_dir_all(&folder).unwrap();
        let source = folder.join("calibration.dng");
        let original = include_bytes!("../formats/raw/synthetic.dng");
        std::fs::write(&source, original).unwrap();
        let mut photo = PhotoImage::load(&source).unwrap();
        let sensor = photo.raw.as_ref().unwrap().pixels.clone();
        photo.develop.optics.profile.camera = "Omadesign Synthetic Bayer".into();
        photo.develop.optics.profile.lens = "Synthetic 50mm calibration".into();
        photo.develop.optics.profile.focal_length = 50.;
        photo.develop.optics.profile.distortion = [-0.1, 0.02, 0.];
        photo.develop.optics.profile.chromatic_aberration = [0.01, -0.01];
        photo.develop.optics.profile.vignette = [0.4, 0.1];
        photo.develop.optics.perspective = [0.08, -0.04];
        photo.develop.optics.straighten = 2.;
        photo.develop.optics.scale = 1.2;
        crate::photo::edits::save(
            &source,
            photo.source_identity.as_ref().unwrap(),
            &photo.develop,
        )
        .unwrap();
        let reopened = PhotoImage::load(&source).unwrap();
        assert_eq!(reopened.develop, photo.develop);
        assert_eq!(photo.raw.as_ref().unwrap().pixels, sensor);
        assert_eq!(
            reopened.render_preview().unwrap(),
            photo.render_full().unwrap()
        );
        for extension in ["png", "tiff"] {
            let decoded =
                image::load_from_memory(&reopened.export_bytes(extension).unwrap()).unwrap();
            assert_eq!((decoded.width(), decoded.height()), (64, 48));
            assert_eq!(decoded.color(), image::ColorType::Rgba16);
            let rgba = decoded.to_rgba16();
            let expected = reopened.render_full().unwrap();
            for (a, b) in rgba.as_raw().iter().zip(&expected.data) {
                assert!(((u32::from(*a) + 128) / 257).abs_diff(u32::from(*b)) <= 1);
            }
        }
        assert_eq!(std::fs::read(&source).unwrap(), original);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn old_development_json_defaults_to_neutral_optics() {
        let mut json = serde_json::to_value(DevelopParams::default()).unwrap();
        json.as_object_mut().unwrap().remove("optics");
        assert_eq!(
            serde_json::from_value::<DevelopParams>(json)
                .unwrap()
                .optics,
            OpticalSettings::default()
        );
        let mut settings = OpticalSettings::default();
        settings.scale = f32::NAN;
        assert!(!settings.valid());
    }
}

#[cfg(test)]
mod resized_camera_tests {
    use crate::photo::{PhotoImage, RgbaImage};

    /// Generate a smooth Bayer calibration frame without borrowing camera assets.
    /// Args: sensor width/height. Returns an uncompressed DNG using the MIT fixture's camera tags.
    fn calibration_dng(w: u32, h: u32) -> Vec<u8> {
        let mut bytes = include_bytes!("../formats/raw/synthetic.dng").to_vec();
        let raw_offset = bytes.len() as u32;
        let count = u16::from_le_bytes(bytes[8..10].try_into().unwrap()) as usize;
        for i in 0..count {
            let at = 10 + i * 12;
            let value = match u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()) {
                256 => Some(w),
                257 | 278 => Some(h),
                273 => Some(raw_offset),
                279 => Some(w * h * 2),
                _ => None,
            };
            if let Some(value) = value {
                bytes[at + 8..at + 12].copy_from_slice(&value.to_le_bytes());
            }
        }
        for y in 0..h {
            for x in 0..w {
                let level = 5000. + 18000. * x as f32 / w as f32 + 8000. * y as f32 / h as f32;
                let gain = match (y % 2, x % 2) {
                    (0, 0) => 0.85,
                    (1, 1) => 0.7,
                    _ => 1.,
                };
                bytes.extend_from_slice(&((level * gain) as u16).to_le_bytes());
            }
        }
        bytes
    }

    fn mean_error(a: &RgbaImage, b: &RgbaImage) -> f64 {
        a.data
            .chunks_exact(4)
            .zip(b.data.chunks_exact(4))
            .map(|(a, b)| {
                a[..3]
                    .iter()
                    .zip(&b[..3])
                    .map(|(&a, &b)| a.abs_diff(b) as u64)
                    .sum::<u64>()
            })
            .sum::<u64>() as f64
            / (a.w * a.h * 3) as f64
    }

    #[test]
    fn reduced_camera_preview_detail_and_exports_agree_after_optical_reopen() {
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".artifacts")
            .join(format!(
                "photo-optics-resized-{}-{}",
                std::process::id(),
                crate::document::next_id()
            ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("synthetic-50mm.dng");
        let original = calibration_dng(1800, 1200);
        std::fs::write(&path, &original).unwrap();
        let mut photo = PhotoImage::load(&path).unwrap();
        assert_eq!(photo.dimensions(), (1800, 1200));
        assert_eq!((photo.preview.w, photo.preview.h), (1600, 1067));
        photo.develop.optics.profile.camera = "Omadesign Synthetic Bayer".into();
        photo.develop.optics.profile.lens = "Synthetic 50mm calibration".into();
        photo.develop.optics.profile.focal_length = 50.;
        photo.develop.optics.profile.distortion = [-0.08, 0.015, 0.];
        photo.develop.optics.profile.chromatic_aberration = [0.012, -0.009];
        photo.develop.optics.profile.vignette = [0.2, 0.05];
        photo.develop.optics.perspective = [0.04, -0.03];
        photo.develop.optics.straighten = 1.5;
        photo.develop.optics.scale = 1.2;
        let settings = crate::photo::edits::save(
            &path,
            photo.source_identity.as_ref().unwrap(),
            &photo.develop,
        )
        .unwrap();
        let photo = PhotoImage::load(&settings).unwrap();
        let full = photo.render_full().unwrap();
        let reduced = photo.render_preview().unwrap();
        assert_ne!((reduced.w, reduced.h), (full.w, full.h));
        assert!(mean_error(&reduced, &full.resized(reduced.w, reduced.h)) < 2.);
        for edge in [400, 800, 1600] {
            let preview = photo.render_thumbnail(edge).unwrap();
            let error = mean_error(&preview, &full.resized(preview.w, preview.h));
            assert!(
                error < 2.,
                "{edge}px optical geometry/CA mean error {error}"
            );
        }
        for extension in ["png", "tiff"] {
            let export = image::load_from_memory(&photo.export_bytes(extension).unwrap()).unwrap();
            assert_eq!((export.width(), export.height()), (1800, 1200));
            assert_eq!(export.color(), image::ColorType::Rgba16);
            for (&a, &b) in export.to_rgba16().as_raw().iter().zip(&full.data) {
                assert!(((u32::from(a) + 128) / 257).abs_diff(u32::from(b)) <= 1);
            }
        }
        assert_eq!(std::fs::read(&path).unwrap(), original);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
