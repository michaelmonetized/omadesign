use super::*;
use crate::raster::{Kind, Settings};

pub(crate) fn settings(kind: Kind) -> Settings {
    let mut s = Settings::new(kind);
    match kind {
        Kind::ChromaKey => s.color = crate::color::Rgba::rgb(255, 255, 255),
        Kind::BrightnessContrast => s.values[0] = 0.1,
        Kind::Exposure => s.values[0] = 1.,
        Kind::Levels => s.values[2] = 0.75,
        Kind::HueSaturation => s.values[2] = 0.25,
        Kind::ColorBalance => s.values[..3].copy_from_slice(&[0.2, -0.1, 0.05]),
        Kind::Temperature => s.values[..2].copy_from_slice(&[0.2, 0.1]),
        _ => {}
    }
    s
}
fn plane(values: &[f32], w: u32) -> Plane {
    Plane {
        width: w,
        height: values.len() as u32 / w,
        transform: [0., 2., -3., 0., 10., 20.],
        samples: storage::collect(values.iter().copied()).unwrap(),
    }
}
fn filtered(source: &Plane, kind: Kind, values: &[f32]) -> Plane {
    let mut s = Settings::new(kind);
    s.values[..values.len()].copy_from_slice(values);
    apply(source, &Filter::from_raster(&s), None, &storage::check).unwrap()
}
fn close(actual: &[f32], expected: &[f32], tolerance: f32) {
    assert_eq!(actual.len(), expected.len());
    for (i, (a, b)) in actual.iter().zip(expected).enumerate() {
        assert!((a - b).abs() <= tolerance, "sample {i}: {a} != {b}");
    }
}

#[test]
fn craft_channel_filter_families_all_thirty_keep_float_coverage_and_selection_owners() {
    let values: Vec<_> = (0..35)
        .map(|i| {
            if i == 0 {
                -0.
            } else {
                f32::from_bits((i as f32 / 35.).to_bits() + 1)
            }
        })
        .collect();
    let source = plane(&values, 7);
    let bits: Vec<_> = source.samples.iter().map(|v| v.to_bits()).collect();
    let selection: Vec<_> = (0..35)
        .map(|i| {
            if i % 3 == 0 {
                0.
            } else if i % 3 == 1 {
                0.33333334
            } else {
                1.
            }
        })
        .collect();
    let choices = Filter::choices();
    assert_eq!(choices.len(), Kind::ALL.len());
    let names: std::collections::HashSet<_> = choices.iter().map(Filter::name).collect();
    assert_eq!(names.len(), 30);
    let pool = resources::global();
    let before = pool.usage().0;
    for kind in Kind::ALL {
        let mut settings = settings(kind);
        settings.strength = 0.375;
        settings.validate().unwrap();
        let filter = Filter::from_raster(&settings);
        filter.validate().unwrap();
        let output = apply(&source, &filter, Some(&selection), &storage::check)
            .unwrap_or_else(|e| panic!("{kind:?}: {e}"));
        output.validate().unwrap();
        assert_eq!(output.transform, source.transform);
        for i in (0..35).step_by(3) {
            assert_eq!(
                output.samples[i].to_bits(),
                bits[i],
                "{kind:?} selection outside"
            );
        }
        let owner = Arc::downgrade(output.samples.lease());
        let alias = output.samples.clone();
        drop(output);
        assert!(owner.upgrade().is_some());
        drop(alias);
        assert!(owner.upgrade().is_none());
        let identity = filter.with_strength(0.);
        let output = apply(&source, &identity, None, &storage::check).unwrap();
        assert!(Arc::ptr_eq(&output.samples, &source.samples));
    }
    assert_eq!(
        source
            .samples
            .iter()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>(),
        bits
    );
    assert_eq!(pool.usage().0, before);
}

#[test]
fn craft_channel_filter_families_morphology_motion_and_mosaic_match_independent_windows() {
    let values = [
        0.1, 0.8, 0.3, 0.5, 0.9, 0.7, 0.2, 0.6, 0.4, 0.1, 0.3, 0.9, 0.2, 0.8, 0.5,
    ];
    let source = plane(&values, 5);
    for grow in [false, true] {
        let result = filtered(
            &source,
            if grow {
                Kind::GrowAlpha
            } else {
                Kind::ShrinkAlpha
            },
            &[1.],
        );
        let mut expected = vec![];
        for y in 0..3i32 {
            for x in 0..5i32 {
                let mut best = if grow { 0f32 } else { 1f32 };
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let v = values
                            [(y + dy).clamp(0, 2) as usize * 5 + (x + dx).clamp(0, 4) as usize];
                        best = if grow { best.max(v) } else { best.min(v) };
                    }
                }
                expected.push(best);
            }
        }
        close(&result.samples, &expected, 0.);
    }
    let motion = filtered(&source, Kind::MotionBlur, &[3., 0.]);
    let mut expected = vec![];
    for y in 0..3 {
        for x in 0..5 {
            expected.push(
                (-1..=1)
                    .map(|d| f64::from(values[y * 5 + (x as i32 + d).clamp(0, 4) as usize]))
                    .sum::<f64>() as f32
                    / 3.,
            );
        }
    }
    close(&motion.samples, &expected, 1e-7);
    let mosaic = filtered(&source, Kind::Pixelate, &[2.]);
    let mut expected = vec![];
    for y in 0..3 {
        for x in 0..5 {
            let (x0, y0) = (x / 2 * 2, y / 2 * 2);
            let mut sum = 0f64;
            let mut n = 0;
            for sy in y0..(y0 + 2).min(3) {
                for sx in x0..(x0 + 2).min(5) {
                    sum += f64::from(values[sy * 5 + sx]);
                    n += 1;
                }
            }
            expected.push((sum / n as f64) as f32);
        }
    }
    close(&mosaic.samples, &expected, 1e-7);
}

#[test]
fn craft_channel_filter_families_tonal_alpha_and_strength_match_explicit_equations() {
    let source = plane(&[0., 0.25, 0.5, 0.75, 1.], 5);
    close(
        &filtered(&source, Kind::AlphaThreshold, &[0.5]).samples,
        &[0., 0., 1., 1., 1.],
        0.,
    );
    close(
        &filtered(&source, Kind::Posterize, &[3.]).samples,
        &[0., 0.5, 0.5, 1., 1.],
        0.,
    );
    close(
        &filtered(&source, Kind::Solarize, &[0.5]).samples,
        &[0., 0.25, 0.5, 0.25, 0.],
        0.,
    );
    let mut key = Settings::new(Kind::ChromaKey);
    key.color = crate::color::Rgba::rgb(255, 255, 255);
    close(
        &apply(&source, &Filter::from_raster(&key), None, &storage::check)
            .unwrap()
            .samples,
        &[0., 0.25, 0.5, 0.75, 0.],
        0.,
    );
    let balance = filtered(&source, Kind::ColorBalance, &[0.2, -0.1, 0.05]);
    let expected: Vec<_> = source
        .samples
        .iter()
        .map(|v| (*v - 0.012695).clamp(0., 1.))
        .collect();
    close(&balance.samples, &expected, 1e-7);
    let mut inverse = Settings::new(Kind::Invert);
    inverse.strength = 0.375;
    let mask = [0., 0.33333334, 1., 0.5, 0.25];
    let result = apply(
        &source,
        &Filter::from_raster(&inverse),
        Some(&mask),
        &storage::check,
    )
    .unwrap();
    for i in 0..5 {
        let before = source.samples[i];
        let expected = before + ((1. - before) - before) * (0.375 * mask[i]);
        assert_eq!(result.samples[i].to_bits(), expected.to_bits());
    }
    for kind in [Kind::Grayscale, Kind::Vibrance, Kind::HueSaturation] {
        let source = plane(&[-0., f32::from_bits(0x3e800001), 0.5, 1.], 4);
        let out = filtered(&source, kind, &[]);
        assert_eq!(
            out.samples.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            source
                .samples
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn craft_channel_filter_families_unsharp_and_convolution_match_independent_kernels() {
    let source = plane(
        &[
            0., 0.1, 0.9, 0.4, 1., 0.3, 0.7, 0.2, 0.6, 0.5, 1., 0.2, 0.8, 0.3, 0.,
        ],
        5,
    );
    let mut blur = source.samples.to_vec();
    for _ in 0..3 {
        for vertical in [false, true] {
            let old = blur.clone();
            for y in 0..3 {
                for x in 0..5 {
                    let mut sum = 0f64;
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
                        sum += f64::from(old[sy * 5 + sx]);
                    }
                    blur[y * 5 + x] = (sum / 3.) as f32;
                }
            }
        }
    }
    let unsharp = filtered(&source, Kind::UnsharpMask, &[1., 0.75, 0.025]);
    let expected: Vec<_> = source
        .samples
        .iter()
        .zip(&blur)
        .map(|(v, b)| {
            if (v - b).abs() >= 0.025 {
                (v + (v - b) * 0.75).clamp(0., 1.)
            } else {
                *v
            }
        })
        .collect();
    close(&unsharp.samples, &expected, 5e-7);
    for (kind, kernel) in [
        (
            Kind::EdgeDetect,
            [-1., -1., -1., -1., 8., -1., -1., -1., -1.],
        ),
        (Kind::Emboss, [-2., -1., 0., -1., 0., 1., 0., 1., 2.]),
    ] {
        let out = filtered(&source, kind, &[0.5]);
        let mut expected = vec![];
        for y in 0..3i32 {
            for x in 0..5i32 {
                let mut sum = 0f64;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        sum += f64::from(
                            source.samples
                                [(y + dy).clamp(0, 2) as usize * 5 + (x + dx).clamp(0, 4) as usize],
                        ) * f64::from(kernel[((dy + 1) * 3 + dx + 1) as usize]);
                    }
                }
                expected.push(
                    (if kind == Kind::Emboss {
                        0.5 + sum * 0.5
                    } else {
                        sum.abs() * 0.5
                    })
                    .clamp(0., 1.) as f32,
                );
            }
        }
        close(&out.samples, &expected, 4e-7);
    }
}

#[test]
fn craft_channel_filter_families_mid_kernel_cancellation_releases_owned_working_buffers() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let source = resources::global()
        .copy_now(&vec![[0.25, 0.5, 0.75, 1.]; 1024 * 128], &storage::check)
        .unwrap();
    let pool = resources::global();
    let before = pool.usage().0;
    let pending = pool.pending_bytes();
    let observed = AtomicBool::new(false);
    let cancel = || {
        if rayon::current_thread_index().is_some() {
            observed.store(true, Ordering::SeqCst);
            Err("requested filter stop".into())
        } else {
            Ok(())
        }
    };
    let err = crate::raster::apply_float_with_cancel(
        &source,
        1024,
        128,
        &Settings::new(Kind::GaussianBlur),
        None,
        &cancel,
    )
    .unwrap_err();
    assert!(err.contains("requested filter stop"));
    assert!(observed.load(Ordering::SeqCst));
    assert_eq!(pool.usage().0, before);
    assert_eq!(pool.pending_bytes(), pending);
    assert!(source.iter().all(|v| *v == [0.25, 0.5, 0.75, 1.]));
}

#[test]
fn craft_channel_filter_families_motion_keeps_constant_coverage_exact() {
    for value in [1., 0.125] {
        let source = plane(&[value; 32], 8);
        for length in [12., 37., 100.] {
            let output = filtered(&source, Kind::MotionBlur, &[length, 0.]);
            assert!(
                output
                    .samples
                    .iter()
                    .all(|v| v.to_bits() == value.to_bits()),
                "length {length}"
            );
        }
    }
}

#[test]
fn craft_channel_filter_families_rgb_strength_keeps_legacy_clamping() {
    let source = [
        [-0., 0.25, 0.75, 0.],
        [0.2, 0.8, 0.3, 0.4],
        [0.4, 0.1, 0.9, 1.],
        [0.6, 0.3, 0.7, 0.5],
    ];
    for kind in Kind::ALL {
        let mut settings = Settings::new(kind);
        let full = crate::raster::apply_float(&source, 2, 2, &settings, None).unwrap();
        settings.strength = 1.5;
        let above = crate::raster::apply_float(&source, 2, 2, &settings, None).unwrap();
        assert_eq!(
            above
                .iter()
                .flatten()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            full.iter()
                .flatten()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            "{kind:?}"
        );
        settings.strength = -0.5;
        let below = crate::raster::apply_float(&source, 2, 2, &settings, None).unwrap();
        assert_eq!(
            below
                .iter()
                .flatten()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            source
                .iter()
                .flatten()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            "{kind:?}"
        );
    }
}
