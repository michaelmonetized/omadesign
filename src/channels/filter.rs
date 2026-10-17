//! Scalar filters keep original coverage and blend the selection only after filtering.
use super::Plane;
use crate::{
    deep_color::storage,
    resources::{self, CancellationCheck},
};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Filter {
    GaussianBlur {
        radius: f32,
    },
    Levels {
        black: f32,
        white: f32,
        gamma: f32,
        output_black: f32,
        output_white: f32,
    },
    Threshold {
        level: f32,
    },
    Raster {
        settings: crate::raster::Settings,
        scale: f32,
    },
    Mix {
        filter: Box<Filter>,
        strength: f32,
    },
}
impl Filter {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::GaussianBlur { .. } => "Gaussian blur",
            Self::Levels { .. } => "Levels",
            Self::Threshold { .. } => "Threshold",
            Self::Raster { settings, .. } => settings.kind.name(),
            Self::Mix { filter, .. } => filter.name(),
        }
    }
    /// Borrow the operation independently of its single final strength blend.
    /// Args: none. Returns the base filter without cloning sample data.
    pub(crate) fn base(&self) -> &Self {
        match self {
            Self::Mix { filter, .. } => filter.base(),
            _ => self,
        }
    }
    /// Read the single final blend amount.
    /// Args: none. Returns unit coverage, validated before evaluation.
    pub(crate) fn strength(&self) -> f32 {
        match self {
            Self::Mix { strength, .. } => *strength,
            _ => 1.,
        }
    }
    /// Replace strength without nesting multiple blend operations.
    /// Args: requested unit amount. Returns one base operation and at most one strength wrapper.
    pub(crate) fn with_strength(self, strength: f32) -> Self {
        let filter = self.base().clone();
        if strength == 1. {
            filter
        } else {
            Self::Mix {
                filter: Box::new(filter),
                strength,
            }
        }
    }
    /// Translate an existing raster choice without losing actual filter parameters or strength.
    /// Args: current settings. Returns the original scalar implementations or the shared scalar raster engine.
    pub(crate) fn from_raster(settings: &crate::raster::Settings) -> Self {
        use crate::raster::Kind;
        let p = settings.values;
        let base = match settings.kind {
            Kind::GaussianBlur => Self::GaussianBlur { radius: p[0] },
            Kind::Levels => Self::Levels {
                black: p[0],
                white: p[1],
                gamma: p[2],
                output_black: 0.,
                output_white: 1.,
            },
            Kind::Threshold => Self::Threshold { level: p[0] },
            _ => {
                let mut original = settings.clone();
                original.strength = 1.;
                Self::Raster {
                    settings: original,
                    scale: 1.,
                }
            }
        };
        base.with_strength(settings.strength)
    }
    /// List every existing raster choice with scalar defaults.
    /// Args: none. Returns unique choices, preserving the original three controls first.
    pub(crate) fn choices() -> Vec<Self> {
        use crate::raster::{Kind, Settings};
        let mut out = vec![
            Self::GaussianBlur { radius: 4. },
            Self::from_raster(&Settings::new(Kind::Levels)),
            Self::Threshold { level: 0.5 },
        ];
        out.extend(
            Kind::ALL
                .into_iter()
                .filter(|k| !matches!(k, Kind::GaussianBlur | Kind::Levels | Kind::Threshold))
                .map(|k| Self::from_raster(&Settings::new(k))),
        );
        out
    }
    /// Validate authored controls before allocating any output.
    /// Args: none. Returns an error without changing or clamping loaded settings.
    pub(crate) fn validate(&self) -> Result<(), String> {
        let unit = |v: f32| v.is_finite() && (0. ..=1.).contains(&v);
        let valid = match self {
            Self::GaussianBlur { radius } => radius.is_finite() && (0. ..=100.).contains(radius),
            Self::Levels {
                black,
                white,
                gamma,
                output_black,
                output_white,
            } => {
                unit(*black)
                    && unit(*white)
                    && black < white
                    && gamma.is_finite()
                    && (0.01..=10.).contains(gamma)
                    && unit(*output_black)
                    && unit(*output_white)
            }
            Self::Threshold { level } => unit(*level),
            Self::Raster { settings, scale } => {
                settings.validate().is_ok() && scale.is_finite() && *scale > 0. && *scale <= 1.
            }
            Self::Mix { filter, strength } => unit(*strength) && filter.validate().is_ok(),
        };
        if valid {
            Ok(())
        } else {
            Err("Use finite filter values; the input black point must be below white".into())
        }
    }
    /// Scale only spatial controls for the actual preview grid.
    /// Args: source-to-preview scale. Returns settings without changing the authored values.
    pub(crate) fn scaled(&self, scale: f32) -> Self {
        match self {
            Self::GaussianBlur { radius } => Self::GaussianBlur {
                radius: radius * scale,
            },
            Self::Raster {
                settings,
                scale: old,
            } => Self::Raster {
                settings: settings.clone(),
                scale: old * scale,
            },
            Self::Mix { filter, strength } => filter.scaled(scale).with_strength(*strength),
            _ => self.clone(),
        }
    }
}

/// Filter original full-depth coverage and apply a soft selection once.
/// Args: scalar source, validated filter, optional matching selection and cancellation. Returns the same placement with admitted output.
pub(crate) fn apply(
    source: &Plane,
    filter: &Filter,
    selection: Option<&[f32]>,
    cancel: &(dyn CancellationCheck + Sync),
) -> Result<Plane, String> {
    source.validate()?;
    filter.validate()?;
    if selection.is_some_and(|s| {
        s.len() != source.samples.len() || crate::selection_coverage::validate(s).is_err()
    }) {
        return Err("Channel filter selection dimensions changed".into());
    }
    cancel.check()?;
    let strength = filter.strength();
    let filter = filter.base();
    if strength == 0.
        || matches!(filter, Filter::GaussianBlur { radius } if *radius < 0.1)
        || matches!(filter, Filter::Levels { black, white, gamma, output_black, output_white } if *black == 0. && *white == 1. && *gamma == 1. && *output_black == 0. && *output_white == 1.)
    {
        return Ok(source.clone());
    }
    let rastered = if let Filter::Raster { settings, scale } = filter {
        let mut current = settings.scaled(*scale);
        current.strength = 1.;
        Some(crate::raster::apply_scalar(
            &source.samples,
            source.width,
            source.height,
            &current,
            cancel,
        )?)
    } else {
        None
    };
    let blurred = if let Filter::GaussianBlur { radius } = *filter {
        Some(crate::raster::blur_channels(
            source.samples.as_slice().as_chunks::<1>().0,
            source.width,
            source.height,
            radius,
            cancel,
        )?)
    } else {
        None
    };
    let mut output = resources::global().allocate_now::<f32, _>(source.samples.len(), cancel)?;
    for (block, dst) in output.as_mut_slice().chunks_mut(16384).enumerate() {
        cancel.check()?;
        for (j, out) in dst.iter_mut().enumerate() {
            let i = block * 16384 + j;
            let before = source.samples[i];
            let mapped = match *filter {
                Filter::GaussianBlur { .. } => blurred.as_ref().unwrap()[i][0].clamp(0., 1.),
                Filter::Levels {
                    black,
                    white,
                    gamma,
                    output_black,
                    output_white,
                } => {
                    output_black
                        + ((before - black) / (white - black))
                            .clamp(0., 1.)
                            .powf(1. / gamma)
                            * (output_white - output_black)
                }
                Filter::Raster { .. } => rastered.as_ref().unwrap()[i][0].clamp(0., 1.),
                Filter::Mix { .. } => {
                    unreachable!("Strength wrapper is removed before evaluating samples")
                }
                Filter::Threshold { level } => {
                    if before >= level {
                        1.
                    } else {
                        0.
                    }
                }
            };
            let weight = strength * selection.map_or(1., |s| s[i]);
            *out = if weight == 0. || before.to_bits() == mapped.to_bits() {
                before
            } else if weight == 1. {
                mapped
            } else {
                (before + (mapped - before) * weight).clamp(0., 1.)
            };
        }
    }
    Ok(Plane {
        samples: Arc::new(output),
        ..source.clone()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plane(values: &[f32], width: u32) -> Plane {
        Plane {
            width,
            height: values.len() as u32 / width,
            transform: [0., 2., -3., 0., 10., 20.],
            samples: storage::collect(values.iter().copied()).unwrap(),
        }
    }
    #[test]
    fn craft_channel_filters_levels_threshold_and_float_selection_use_original_samples() {
        let input = plane(&[-0., f32::from_bits(0x3e800001), 0.5, 1.], 4);
        let identity = Filter::Levels {
            black: 0.,
            white: 1.,
            gamma: 1.,
            output_black: 0.,
            output_white: 1.,
        };
        assert!(Arc::ptr_eq(
            &input.samples,
            &apply(&input, &identity, None, &storage::check)
                .unwrap()
                .samples
        ));
        let settings = Filter::Levels {
            black: 0.1,
            white: 0.9,
            gamma: 2.,
            output_black: 0.2,
            output_white: 0.8,
        };
        let mask = [0., f32::from_bits(0x3eaaaaab), 1., 0.5];
        let result = apply(&input, &settings, Some(&mask), &storage::check).unwrap();
        assert_eq!(result.transform, input.transform);
        for i in 0..4 {
            let x = input.samples[i];
            let y = 0.2 + ((x - 0.1) / 0.8).clamp(0., 1.).sqrt() * 0.6;
            let expected = if mask[i] == 0. {
                x
            } else {
                x + (y - x) * mask[i]
            };
            assert!((result.samples[i] - expected).abs() < 1e-7);
        }
        assert_eq!(result.samples[0].to_bits(), (-0.0f32).to_bits());
        let threshold = apply(
            &input,
            &Filter::Threshold {
                level: f32::from_bits(0x3e800001),
            },
            None,
            &storage::check,
        )
        .unwrap();
        assert_eq!(threshold.samples.as_slice(), &[0., 1., 1., 1.]);
        assert_eq!(input.samples[1].to_bits(), 0x3e800001);
    }
    #[test]
    fn craft_channel_filters_scalar_gaussian_matches_independent_box_sum_and_rgba_engine() {
        let input = plane(
            &[
                0., 0.25, 0.75, 1., 0., 1., 0.3, 0.6, 0.9, 0.1, 0., 0.8, 0.4, 0.2, 1.,
            ],
            5,
        );
        let mut oracle = input.samples.to_vec();
        for _ in 0..3 {
            for vertical in [false, true] {
                let old = oracle.clone();
                for y in 0..3 {
                    for x in 0..5 {
                        let mut sum = 0.;
                        for d in -1..=1 {
                            let sx = if vertical {
                                x
                            } else {
                                (x as i32 + d).clamp(0, 4) as usize
                            };
                            let sy = if vertical {
                                (y as i32 + d).clamp(0, 2) as usize
                            } else {
                                y
                            };
                            sum += old[sy * 5 + sx];
                        }
                        oracle[y * 5 + x] = sum / 3.;
                    }
                }
            }
        }
        let result = apply(
            &input,
            &Filter::GaussianBlur { radius: 2. },
            None,
            &storage::check,
        )
        .unwrap();
        for (a, b) in result.samples.iter().zip(&oracle) {
            assert!((a - b).abs() < 3e-7, "{a} != {b}");
        }
        let rgba: Vec<_> = input.samples.iter().map(|v| [*v, *v, *v, 1.]).collect();
        let old = crate::raster::apply_float(
            &rgba,
            5,
            3,
            &crate::raster::Settings::new(crate::raster::Kind::GaussianBlur).scaled(0.5),
            None,
        )
        .unwrap();
        for (a, b) in result.samples.iter().zip(old.iter()) {
            assert_eq!(a.to_bits(), b[0].to_bits());
        }
    }
    #[test]
    fn craft_channel_filters_invalid_cancel_and_final_alias_preserve_source_and_pool() {
        let source = plane(&[0., 0.5, 1., 0.25], 2);
        let settings = Filter::GaussianBlur { radius: 2. };
        assert!(
            apply(
                &source,
                &Filter::Levels {
                    black: 0.8,
                    white: 0.2,
                    gamma: 1.,
                    output_black: 0.,
                    output_white: 1.
                },
                None,
                &storage::check
            )
            .is_err()
        );
        assert!(apply(&source, &settings, Some(&[f32::NAN; 4]), &storage::check).is_err());
        let cancel = || Err("actual filter cancellation".into());
        assert!(
            apply(&source, &settings, None, &cancel)
                .unwrap_err()
                .contains("actual filter cancellation")
        );
        let plane = apply(&source, &settings, None, &storage::check).unwrap();
        let owner = Arc::downgrade(plane.samples.lease());
        let alias = plane.samples.clone();
        drop(plane);
        assert!(owner.upgrade().is_some());
        drop(alias);
        assert!(owner.upgrade().is_none());
        assert_eq!(source.samples.as_slice(), &[0., 0.5, 1., 0.25]);
    }
}

#[cfg(test)]
#[path = "filter_families_tests.rs"]
pub(crate) mod family_tests;
