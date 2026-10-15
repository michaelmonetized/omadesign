//! Editable source-relative masks and scene-linear local adjustments.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LocalMask {
    Radial {
        center: [f32; 2],
        radius: [f32; 2],
        angle: f32,
        feather: f32,
    },
    Gradient {
        start: [f32; 2],
        end: [f32; 2],
    },
    Brush {
        strokes: Vec<BrushStroke>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrushStroke {
    pub points: Vec<[f32; 2]>,
    pub radius: f32,
    pub hardness: f32,
    pub flow: f32,
    pub erase: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LocalAdjustment {
    pub name: String,
    pub enabled: bool,
    pub mask: LocalMask,
    pub invert: bool,
    pub opacity: f32,
    pub exposure: f32,
    pub temperature: f32,
    pub tint: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub highlights: f32,
    pub shadows: f32,
}

impl Default for LocalAdjustment {
    fn default() -> Self {
        Self {
            name: "Local adjustment".into(),
            enabled: true,
            mask: LocalMask::Radial {
                center: [0.5, 0.5],
                radius: [0.25, 0.25],
                angle: 0.,
                feather: 0.5,
            },
            invert: false,
            opacity: 1.,
            exposure: 0.,
            temperature: 0.,
            tint: 0.,
            contrast: 1.,
            saturation: 1.,
            highlights: 0.,
            shadows: 0.,
        }
    }
}

fn bounded(value: f32, low: f32, high: f32) -> bool {
    value.is_finite() && (low..=high).contains(&value)
}
fn point_valid(point: &[f32; 2]) -> bool {
    point.iter().all(|&v| bounded(v, 0., 1.))
}

impl LocalMask {
    /// Evaluate editable geometry in the corrected, uncropped image.
    /// Args: normalized image point and width/height aspect. Returns mask coverage from zero to one.
    pub fn coverage(&self, point: [f32; 2], aspect: f32) -> f32 {
        match self {
            Self::Radial {
                center,
                radius,
                angle,
                feather,
            } => {
                let (sin, cos) = angle.to_radians().sin_cos();
                let (x, y) = ((point[0] - center[0]) * aspect, point[1] - center[1]);
                let d = (((x * cos + y * sin) / (radius[0] * aspect)).powi(2)
                    + ((-x * sin + y * cos) / radius[1]).powi(2))
                .sqrt();
                if *feather <= 0. {
                    return if d <= 1. { 1. } else { 0. };
                }
                smooth(((1. - d) / feather).clamp(0., 1.))
            }
            Self::Gradient { start, end } => {
                let delta = [(end[0] - start[0]) * aspect, end[1] - start[1]];
                let norm = delta[0] * delta[0] + delta[1] * delta[1];
                if norm <= 1e-12 {
                    return 0.;
                }
                smooth(
                    (((point[0] - start[0]) * aspect * delta[0]
                        + (point[1] - start[1]) * delta[1])
                        / norm)
                        .clamp(0., 1.),
                )
            }
            Self::Brush { strokes } => {
                let mut coverage = 0.;
                for stroke in strokes {
                    let distance = stroke
                        .points
                        .windows(2)
                        .map(|p| segment_distance(point, p[0], p[1], aspect))
                        .fold(
                            stroke
                                .points
                                .first()
                                .map_or(f32::INFINITY, |p| distance(point, *p, aspect)),
                            f32::min,
                        );
                    let radius = stroke.radius;
                    let weight = if stroke.hardness >= 1. {
                        if distance <= radius { 1. } else { 0. }
                    } else {
                        smooth(
                            ((radius - distance) / (radius * (1. - stroke.hardness))).clamp(0., 1.),
                        )
                    } * stroke.flow;
                    coverage = if stroke.erase {
                        coverage * (1. - weight)
                    } else {
                        coverage + (1. - coverage) * weight
                    };
                }
                coverage
            }
        }
    }

    pub fn valid(&self) -> bool {
        match self {
            Self::Radial {
                center,
                radius,
                angle,
                feather,
            } => {
                point_valid(center)
                    && radius.iter().all(|&v| bounded(v, 0.001, 2.))
                    && bounded(*angle, -180., 180.)
                    && bounded(*feather, 0., 1.)
            }
            Self::Gradient { start, end } => {
                point_valid(start) && point_valid(end) && distance(*start, *end, 1.) > 0.0001
            }
            Self::Brush { strokes } => {
                strokes.len() <= 1024
                    && strokes.iter().map(|s| s.points.len()).sum::<usize>() <= 32768
                    && strokes.iter().all(|s| {
                        !s.points.is_empty()
                            && s.points.iter().all(point_valid)
                            && bounded(s.radius, 0.001, 1.)
                            && bounded(s.hardness, 0., 1.)
                            && bounded(s.flow, 0., 1.)
                    })
            }
        }
    }
}

fn smooth(x: f32) -> f32 {
    x * x * (3. - 2. * x)
}
fn distance(a: [f32; 2], b: [f32; 2], aspect: f32) -> f32 {
    (((a[0] - b[0]) * aspect).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// Measure a round brush stroke in image-height units.
/// Args: normalized point, segment endpoints, width/height aspect. Returns shortest image-space distance.
fn segment_distance(p: [f32; 2], a: [f32; 2], b: [f32; 2], aspect: f32) -> f32 {
    let delta = [(b[0] - a[0]) * aspect, b[1] - a[1]];
    let norm = delta[0] * delta[0] + delta[1] * delta[1];
    let t = if norm <= 1e-12 {
        0.
    } else {
        (((p[0] - a[0]) * aspect * delta[0] + (p[1] - a[1]) * delta[1]) / norm).clamp(0., 1.)
    };
    distance(
        p,
        [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])],
        aspect,
    )
}

impl LocalAdjustment {
    /// Validate an adjustment from a sidecar, catalog or agent patch.
    /// Args: none. Returns whether all settings and mask geometry are bounded.
    pub fn valid(&self) -> bool {
        !self.name.is_empty()
            && self.name.len() <= 256
            && !self.name.chars().any(char::is_control)
            && self.mask.valid()
            && bounded(self.opacity, 0., 1.)
            && bounded(self.exposure, -8., 8.)
            && bounded(self.temperature, -100., 100.)
            && bounded(self.tint, -100., 100.)
            && bounded(self.contrast, 0., 4.)
            && bounded(self.saturation, 0., 2.)
            && bounded(self.highlights, -1., 1.)
            && bounded(self.shadows, -1., 1.)
    }

    /// Blend local light and color in scene-linear RGB.
    /// Args: linear RGB, normalized corrected point and source aspect. Returns adjusted linear RGB without display clipping.
    pub fn apply(&self, rgb: [f32; 3], point: [f32; 2], aspect: f32) -> [f32; 3] {
        if !self.enabled {
            return rgb;
        }
        let coverage = self.mask.coverage(point, aspect);
        let weight = self.opacity * if self.invert { 1. - coverage } else { coverage };
        if weight <= 0. {
            return rgb;
        }
        let luma = super::pixel_luma(rgb[0], rgb[1], rgb[2]).clamp(0., 1.);
        let gain = 2f32.powf(self.exposure)
            * (1. + self.highlights * luma * luma * 0.9 + self.shadows * (1. - luma).powi(2) * 0.9)
                .max(0.);
        let t = self.temperature / 100.;
        let tint = self.tint / 100.;
        let wb = [
            1. + t * 0.22 + tint * 0.08,
            1. - t * 0.05 - tint * 0.18,
            1. - t * 0.22 + tint * 0.08,
        ];
        let adjusted = std::array::from_fn::<_, 3, _>(|c| {
            ((rgb[c] * gain * wb[c] - 0.18) * self.contrast + 0.18).max(0.)
        });
        let luma = super::pixel_luma(adjusted[0], adjusted[1], adjusted[2]);
        std::array::from_fn(|c| {
            rgb[c] + ((luma + (adjusted[c] - luma) * self.saturation).max(0.) - rgb[c]) * weight
        })
    }
}

/// Convert a viewer point through saved crop and rotation into editable mask space.
/// Args: output UV and development settings. Returns normalized corrected, uncropped UV.
pub fn source_uv(uv: [f32; 2], params: &super::DevelopParams) -> [f32; 2] {
    let c = params.crop.unwrap_or([0., 0., 1., 1.]);
    let [x, y] = [c[0] + uv[0] * (c[2] - c[0]), c[1] + uv[1] * (c[3] - c[1])];
    match params.rotate {
        90 => [y, 1. - x],
        180 => [1. - x, 1. - y],
        270 => [1. - y, x],
        _ => [x, y],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::photo::{DevelopParams, PhotoImage};

    #[test]
    fn mask_geometry_and_brush_erase_are_resolution_independent() {
        let radial = LocalAdjustment::default().mask;
        assert_eq!(radial.coverage([0.5, 0.5], 1.), 1.);
        assert_eq!(radial.coverage([0., 0.], 1.), 0.);
        let gradient = LocalMask::Gradient {
            start: [0.25, 0.5],
            end: [0.75, 0.5],
        };
        assert_eq!(gradient.coverage([0.5, 0.5], 2.), 0.5);
        let stroke = BrushStroke {
            points: vec![[0.25, 0.5], [0.75, 0.5]],
            radius: 0.1,
            hardness: 1.,
            flow: 1.,
            erase: false,
        };
        let mut brush = LocalMask::Brush {
            strokes: vec![stroke.clone()],
        };
        assert_eq!(brush.coverage([0.5, 0.5], 2.), 1.);
        assert_eq!(brush.coverage([0.5, 0.7], 2.), 0.);
        if let LocalMask::Brush { strokes } = &mut brush {
            strokes.push(BrushStroke {
                points: vec![[0.5, 0.5]],
                erase: true,
                ..stroke
            });
        }
        assert_eq!(brush.coverage([0.5, 0.5], 2.), 0.);
        assert!(
            source_uv(
                [0.5, 0.25],
                &DevelopParams {
                    rotate: 90,
                    crop: Some([0.2, 0.1, 0.8, 0.9]),
                    ..Default::default()
                }
            )
            .iter()
            .zip([0.3, 0.5])
            .all(|(a, b)| (a - b).abs() < 1e-6)
        );
    }

    #[test]
    fn local_exposure_has_an_independent_linear_reference_and_invert() {
        let mut adjustment = LocalAdjustment {
            exposure: 1.,
            ..Default::default()
        };
        let input = [0.1, 0.2, 0.3];
        let actual = adjustment.apply(input, [0.5, 0.5], 1.);
        for c in 0..3 {
            assert!((actual[c] - input[c] * 2.).abs() < 1e-6);
        }
        assert_eq!(adjustment.apply(input, [0., 0.], 1.), input);
        adjustment.invert = true;
        assert_eq!(adjustment.apply(input, [0.5, 0.5], 1.), input);
        assert!(adjustment.apply(input, [0., 0.], 1.)[0] > input[0]);
    }

    #[test]
    fn local_camera_edits_reopen_and_agree_at_multiple_sizes_with_export() {
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".artifacts")
            .join(format!(
                "photo-local-{}-{}",
                std::process::id(),
                crate::document::next_id()
            ));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("camera.dng");
        let original = include_bytes!("../formats/raw/synthetic.dng");
        std::fs::write(&path, original).unwrap();
        let mut photo = PhotoImage::load(&path).unwrap();
        photo.develop.local_adjustments = vec![
            LocalAdjustment {
                exposure: 0.5,
                temperature: 15.,
                mask: LocalMask::Gradient {
                    start: [0., 0.],
                    end: [1., 1.],
                },
                ..Default::default()
            },
            LocalAdjustment {
                exposure: -0.3,
                mask: LocalMask::Radial {
                    center: [0.5, 0.5],
                    radius: [0.4, 0.4],
                    angle: 15.,
                    feather: 0.8,
                },
                ..Default::default()
            },
        ];
        photo.develop.optics.profile.vignette = [0.25, 0.];
        photo.develop.optics.profile.distortion = [-0.04, 0., 0.];
        let settings = crate::photo::edits::save(
            &path,
            photo.source_identity.as_ref().unwrap(),
            &photo.develop,
        )
        .unwrap();
        let reopened = PhotoImage::load(&settings).unwrap();
        assert_eq!(reopened.develop, photo.develop);
        let full = photo.render_full();
        for edge in [16, 32, 64] {
            let preview = photo.render_thumbnail(edge);
            let reference = full.resized(preview.w, preview.h);
            let mean = preview
                .data
                .chunks_exact(4)
                .zip(reference.data.chunks_exact(4))
                .flat_map(|(a, b)| {
                    a[..3]
                        .iter()
                        .zip(&b[..3])
                        .map(|(&a, &b)| a.abs_diff(b) as u64)
                        .collect::<Vec<_>>()
                })
                .sum::<u64>() as f64
                / (preview.w * preview.h * 3) as f64;
            assert!(mean < 4., "{edge}px preview/export mean error {mean}");
        }
        for ext in ["png", "tiff"] {
            let decoded = image::load_from_memory(&reopened.export_bytes(ext).unwrap()).unwrap();
            assert_eq!(decoded.color(), image::ColorType::Rgba16);
            assert_eq!((decoded.width(), decoded.height()), (full.w, full.h));
            for (&a, &b) in decoded.to_rgba16().as_raw().iter().zip(&full.data) {
                assert!(((u32::from(a) + 128) / 257).abs_diff(u32::from(b)) <= 1);
            }
        }
        assert_eq!(std::fs::read(&path).unwrap(), original);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn local_sidecars_default_old_documents_and_reject_invalid_masks() {
        let mut json = serde_json::to_value(DevelopParams::default()).unwrap();
        json.as_object_mut().unwrap().remove("local_adjustments");
        assert!(
            serde_json::from_value::<DevelopParams>(json)
                .unwrap()
                .local_adjustments
                .is_empty()
        );
        let mut p = DevelopParams::default();
        p.local_adjustments.push(LocalAdjustment {
            mask: LocalMask::Gradient {
                start: [0.5, 0.5],
                end: [0.5, 0.5],
            },
            ..Default::default()
        });
        assert!(crate::photo::edits::validate(&p).is_err());
        p.local_adjustments = vec![LocalAdjustment::default(); 33];
        assert!(crate::photo::edits::validate(&p).is_err());
    }
}
