//! Illustrator placement adapters using the shared admitted pixel storage and sampler.

use crate::{
    deep_color::{self, storage},
    document::Pixels,
    resources::{self, Allocation},
};
use std::sync::Arc;

/// Reflect a source grid without changing its color model or shared history.
/// Args: mutable placed pixels. Returns an admission error before publishing any partial edit.
pub(super) fn reflect(pixels: &mut Pixels) -> Result<(), String> {
    if pixels.native.is_none() {
        let (w, h) = (pixels.w, pixels.h);
        flip_rows(pixels.bytes_mut()?, w, h, 4);
        pixels.touch();
        return Ok(());
    }
    let mut data = pixels.data.try_clone_now(&storage::check)?;
    flip_rows(data.as_mut_slice(), pixels.w, pixels.h, 4);
    let mut native = pixels.native.clone().unwrap();
    let mut samples = native.samples.try_clone_now(&storage::check)?;
    flip_rows(samples.as_mut_slice(), pixels.w, pixels.h, 4);
    native.samples = Arc::new(samples);
    if let Some(inks) = &native.cmyk {
        let mut inks = inks.try_clone_now(&storage::check)?;
        flip_rows(inks.as_mut_slice(), pixels.w, pixels.h, 5);
        native.cmyk = Some(Arc::new(inks));
    }
    let mut reflected = Pixels::from_byte_allocation(pixels.w, pixels.h, Arc::new(data))?;
    reflected.native = Some(native);
    reflected.validate_native()?;
    *pixels = reflected;
    Ok(())
}

/// Reverse rows in an already admitted grid.
/// Args: samples, dimensions and channel count. Returns after swapping complete rows in place.
fn flip_rows<T>(values: &mut [T], w: u32, h: u32, channels: usize) {
    let row = w as usize * channels;
    for y in 0..h as usize / 2 {
        let opposite = h as usize - y - 1;
        for x in 0..row {
            values.swap(y * row + x, opposite * row + x);
        }
    }
}

/// Resample authored color using the existing native bilinear sampler.
/// Args: source pixels, destination dimensions and destination-to-source affine.
/// Returns admitted native pixels retaining depth, profile, intent and CMYKA authoring.
pub(super) fn resample_native(
    pixels: &Pixels,
    w: u32,
    h: u32,
    inverse: tiny_skia::Transform,
) -> Result<Pixels, String> {
    let native = pixels
        .native
        .as_ref()
        .ok_or("Expected native Illustrator pixels")?;
    let (samples, inks) = if let Some(inks) = &native.cmyk {
        let cmy = ink_view(pixels, inks, false)?;
        let black = ink_view(pixels, inks, true)?;
        let mut output = resources::global()
            .allocate_now::<f32, _>(storage::count(w, h, 5)?, &storage::check)?;
        for (i, dst) in output.as_mut_slice().chunks_exact_mut(5).enumerate() {
            let point = source_point(i, w, inverse);
            let c = deep_color::sample_at(&cmy, point.x, point.y);
            let k = deep_color::sample_at(&black, point.x, point.y);
            dst.copy_from_slice(&[c[0], c[1], c[2], k[0], c[3]]);
        }
        let inks = Arc::new(output);
        let samples = deep_color::cms::transform_alpha(
            &inks,
            deep_color::cms::Model::Cmyk,
            &native.icc_profile,
            true,
            native.intent,
        )?;
        (samples, Some(inks))
    } else {
        let mut output = resources::global()
            .allocate_now::<f32, _>(storage::count(w, h, 4)?, &storage::check)?;
        for (i, dst) in output.as_mut_slice().chunks_exact_mut(4).enumerate() {
            let point = source_point(i, w, inverse);
            dst.copy_from_slice(&deep_color::sample_at(pixels, point.x, point.y));
        }
        (Arc::new(output), None)
    };
    let mut out = Pixels::from_sample_allocation(w, h, native.depth, samples)?;
    if inks.is_none() && !native.icc_profile.is_empty() {
        let display = deep_color::cms::transform_alpha(
            &out.native.as_ref().unwrap().samples,
            deep_color::cms::Model::Rgb,
            &native.icc_profile,
            true,
            native.intent,
        )?;
        out.data = storage::collect(display.iter().map(|&v| deep_color::byte(v)))?;
    }
    let retained = out.native.as_mut().unwrap();
    retained.cmyk = inks;
    retained.icc_profile = native.icc_profile.clone();
    retained.intent = native.intent;
    out.validate_native()?;
    Ok(out)
}

/// Map a destination pixel center into its source grid.
/// Args: destination index, row width and inverse affine. Returns source coordinates for the shared sampler.
fn source_point(i: usize, w: u32, inverse: tiny_skia::Transform) -> tiny_skia::Point {
    let mut point =
        tiny_skia::Point::from_xy((i % w as usize) as f32 + 0.5, (i / w as usize) as f32 + 0.5);
    inverse.map_point(&mut point);
    point
}

/// Pack ink channels for the existing RGBA sampler without a color conversion.
/// Args: source pixels, authoritative CMYKA and black-plane selector. Returns an admitted temporary pixel view.
fn ink_view(pixels: &Pixels, inks: &Allocation<f32>, black: bool) -> Result<Pixels, String> {
    let samples = storage::collect(inks.chunks_exact(5).flat_map(|p| {
        if black {
            [p[3], 0., 0., p[4]]
        } else {
            [p[0], p[1], p[2], p[4]]
        }
    }))?;
    Pixels::from_sample_allocation(
        pixels.w,
        pixels.h,
        pixels.native.as_ref().unwrap().depth,
        samples,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deep_color::{Depth, cms};

    #[test]
    fn illustrator_reflection_keeps_shared_native_inks_profile_and_display() {
        let profile = include_bytes!("../psd/fixtures/fogra39l-cc0.icc");
        let original = Pixels::from_cmyk(
            1,
            2,
            Depth::U16,
            vec![0.1, 0.2, 0.3, 0.4, 1., 0.8, 0.7, 0.6, 0.5, 0.25],
            profile.to_vec(),
            0,
        )
        .unwrap();
        let snapshot = serde_json::to_value(&original).unwrap();
        let mut placed = original.clone();
        reflect(&mut placed).unwrap();
        let before = original.native.as_ref().unwrap();
        let after = placed.native.as_ref().unwrap();
        assert_eq!(after.depth, Depth::U16);
        assert_eq!(after.intent, 0);
        assert!(Arc::ptr_eq(&after.icc_profile, &before.icc_profile));
        assert_eq!(&after.samples[..4], &before.samples[4..]);
        assert_eq!(
            &after.cmyk.as_ref().unwrap()[..5],
            &before.cmyk.as_ref().unwrap()[5..]
        );
        assert_eq!(&placed.data[..4], &original.data[4..]);
        assert!(!Arc::ptr_eq(&placed.data, &original.data));
        assert_eq!(serde_json::to_value(&original).unwrap(), snapshot);
        let reopened: Pixels =
            serde_json::from_value(serde_json::to_value(&placed).unwrap()).unwrap();
        reopened.validate_native().unwrap();
        assert_eq!(reopened.native, placed.native);
        let mut bytes = Pixels::from_rgba(1, 2, vec![1, 2, 3, 255, 4, 5, 6, 255]).unwrap();
        let shared = bytes.clone();
        reflect(&mut bytes).unwrap();
        assert_eq!(&bytes.data[..], &[4, 5, 6, 255, 1, 2, 3, 255]);
        assert_eq!(&shared.data[..], &[1, 2, 3, 255, 4, 5, 6, 255]);
    }

    #[test]
    fn illustrator_native_shear_preserves_hdr_inks_and_explicit_intent() {
        let rgb =
            Pixels::from_samples(2, 2, Depth::F32, [1.25, -0.2, 0.1234567, 0.5].repeat(4)).unwrap();
        let mut profiled =
            Pixels::from_samples(2, 2, Depth::U16, [0.1234567, 0.2, 0.7, 0.5].repeat(4)).unwrap();
        profiled
            .assign_rgb_profile(include_bytes!("../psd/fixtures/srgb-cc0.icc").to_vec(), 0)
            .unwrap();
        let cmyk = Pixels::from_cmyk(
            2,
            2,
            Depth::F32,
            [0.1234567, 0.2345678, 0.3456789, 0.456789, 0.7].repeat(4),
            include_bytes!("../psd/fixtures/fogra39l-cc0.icc").to_vec(),
            0,
        )
        .unwrap();
        for original in [rgb, profiled, cmyk] {
            let snapshot = serde_json::to_value(&original).unwrap();
            let inverse = tiny_skia::Transform::from_row(1., 0., -0.5, 1., 0., 0.);
            let placed = resample_native(&original, 3, 2, inverse).unwrap();
            let before = original.native.as_ref().unwrap();
            let after = placed.native.as_ref().unwrap();
            assert_eq!(after.depth, before.depth);
            assert_eq!(after.intent, before.intent);
            assert!(Arc::ptr_eq(&after.icc_profile, &before.icc_profile));
            for (i, p) in after.samples.chunks_exact(4).enumerate() {
                if [2, 3].contains(&i) {
                    assert_eq!(p[3], 0.);
                    if after.cmyk.is_none() {
                        assert_eq!(p, &[0.; 4]);
                    }
                } else {
                    for (actual, expected) in p.iter().zip(before.samples[..4].iter()) {
                        assert!(
                            (actual - expected).abs() < 1e-5,
                            "{i}: {actual} != {expected}"
                        );
                    }
                }
            }
            if let Some(inks) = &after.cmyk {
                assert_eq!(inks.len(), 30);
                for &i in &[0, 1, 4, 5] {
                    for (actual, expected) in inks[i * 5..i * 5 + 5]
                        .iter()
                        .zip(before.cmyk.as_ref().unwrap()[..5].iter())
                    {
                        assert!((actual - expected).abs() < 1e-6);
                    }
                }
                let display = cms::transform_alpha(
                    inks,
                    cms::Model::Cmyk,
                    &after.icc_profile,
                    true,
                    after.intent,
                )
                .unwrap();
                assert_eq!(after.samples.as_slice(), display.as_slice());
            }
            assert!(resample_native(&original, u32::MAX, u32::MAX, inverse).is_err());
            assert_eq!(serde_json::to_value(&original).unwrap(), snapshot);
            let reopened: Pixels =
                serde_json::from_value(serde_json::to_value(&placed).unwrap()).unwrap();
            reopened.validate_native().unwrap();
            assert_eq!(reopened.native, placed.native);
            assert_eq!(reopened.data, placed.data);
        }
    }
}
