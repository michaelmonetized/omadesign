//! Selection samples stay at their authored precision until an actual byte output.
use crate::{
    deep_color::storage,
    resources::{self, Allocation, CancellationCheck},
};
use std::sync::Arc;

pub trait Sample: Copy + Default + PartialOrd {
    fn raw(self) -> f64;
    fn average(value: f64) -> Self;
    /// Read unit selection coverage.
    /// Args: none. Returns the original float or the exact normalized byte value.
    fn unit(self) -> f32;
}
impl Sample for u8 {
    fn unit(self) -> f32 {
        f32::from(self) / 255.
    }
    fn raw(self) -> f64 {
        f64::from(self)
    }
    fn average(value: f64) -> Self {
        value.round() as u8
    }
}
impl Sample for f32 {
    fn unit(self) -> f32 {
        self
    }
    fn raw(self) -> f64 {
        f64::from(self)
    }
    fn average(value: f64) -> Self {
        value as f32
    }
}

/// Promote an incoming byte mask once without making it authoritative again.
/// Args: original bytes and cancellation. Returns admitted float coverage.
pub fn from_bytes(
    values: &[u8],
    cancel: &dyn CancellationCheck,
) -> Result<Arc<Allocation<f32>>, String> {
    let mut out = resources::global().allocate_now::<f32, _>(values.len(), cancel)?;
    for (src, dst) in values
        .chunks(16384)
        .zip(out.as_mut_slice().chunks_mut(16384))
    {
        cancel.check()?;
        for (a, b) in src.iter().zip(dst) {
            *b = a.unit();
        }
    }
    Ok(Arc::new(out))
}
/// Quantize only an actual byte output, never a selection edit or state transition.
/// Args: original unit floats and cancellation. Returns an admitted byte output.
pub fn bytes(
    values: &[f32],
    cancel: &dyn CancellationCheck,
) -> Result<Arc<Allocation<u8>>, String> {
    validate(values)?;
    let mut out = resources::global().allocate_now::<u8, _>(values.len(), cancel)?;
    for (src, dst) in values
        .chunks(16384)
        .zip(out.as_mut_slice().chunks_mut(16384))
    {
        cancel.check()?;
        for (a, b) in src.iter().zip(dst) {
            *b = (*a * 255.).round() as u8;
        }
    }
    Ok(Arc::new(out))
}
/// Reject damaged coverage without clamping accepted authored values.
/// Args: unit samples. Returns an error for nonfinite or out-of-range data.
pub fn validate(values: &[f32]) -> Result<(), String> {
    if values
        .iter()
        .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
    {
        Err("Selection coverage must be finite and between zero and one".into())
    } else {
        Ok(())
    }
}
/// Sample a scalar grid at pixel centers with transparent samples beyond its edge.
/// Args: coverage, grid and pixel coordinate. Returns bilinear float coverage.
pub fn sample(values: &[f32], w: u32, h: u32, x: f32, y: f32) -> f32 {
    if !x.is_finite() || !y.is_finite() {
        return 0.;
    }
    let x = x - 0.5;
    let y = y - 0.5;
    let ix = x.floor();
    let iy = y.floor();
    let fx = x - ix;
    let fy = y - iy;
    let at = |dx, dy| {
        let x = ix + dx as f32;
        let y = iy + dy as f32;
        if x >= 0. && y >= 0. && x < w as f32 && y < h as f32 {
            values[y as usize * w as usize + x as usize]
        } else {
            0.
        }
    };
    if fx == 0. && fy == 0. {
        return at(0, 0);
    }
    let mix = |a: f32, b: f32, t: f32| {
        if t == 0. {
            a
        } else if t == 1. {
            b
        } else {
            a + (b - a) * t
        }
    };
    mix(mix(at(0, 0), at(1, 0), fx), mix(at(0, 1), at(1, 1), fx), fy).clamp(0., 1.)
}
/// Map a complete selection between its actual document coordinate systems.
/// Args: original samples, source and target grids, cancellation. Returns admitted full-depth coverage.
pub fn resample(
    values: &[f32],
    source: crate::app::masking::SelectionSpace,
    target: crate::app::masking::SelectionSpace,
    cancel: &dyn CancellationCheck,
) -> Result<Arc<Allocation<f32>>, String> {
    if values.len() != storage::count(source.w, source.h, 1)?
        || !crate::channels::valid_placement(source.transform)
        || !crate::channels::valid_placement(target.transform)
    {
        return Err("Invalid selection dimensions or placement".into());
    }
    validate(values)?;
    if source == target {
        return resources::global().copy_now(values, cancel).map(Arc::new);
    }
    let transform = source
        .transform
        .invert()
        .ok_or("Selection transform is singular")?
        .pre_concat(target.transform);
    let mut out = resources::global()
        .allocate_now::<f32, _>(storage::count(target.w, target.h, 1)?, cancel)?;
    for y in 0..target.h {
        cancel.check()?;
        for x in 0..target.w {
            let mut p = tiny_skia::Point::from_xy(x as f32 + 0.5, y as f32 + 0.5);
            transform.map_point(&mut p);
            out.as_mut_slice()[(y * target.w + x) as usize] =
                sample(values, source.w, source.h, p.x, p.y);
        }
    }
    Ok(Arc::new(out))
}
/// Combine a newly generated mask with existing full-depth coverage.
/// Args: optional original, incoming byte samples and operation. Returns admitted coverage.
pub fn combine_bytes(
    base: Option<&[f32]>,
    next: &[u8],
    op: crate::paint::PixelCombine,
) -> Result<Arc<Allocation<f32>>, String> {
    let mut out = resources::global().allocate_now::<f32, _>(next.len(), &storage::check)?;
    let base = base.filter(|b| b.len() == next.len());
    for (i, (&v, dst)) in next.iter().zip(out.as_mut_slice()).enumerate() {
        if i % 16384 == 0 {
            storage::check.check()?;
        }
        let b = v.unit();
        *dst = match op {
            crate::paint::PixelCombine::Replace => b,
            crate::paint::PixelCombine::Add => base.map_or(b, |a| a[i].max(b)),
            crate::paint::PixelCombine::Subtract => base.map_or(0., |a| (a[i] - b).max(0.)),
        };
    }
    Ok(Arc::new(out))
}
/// Combine full-depth coverage without changing any source owner.
/// Args: optional original, incoming unit samples and operation. Returns admitted coverage.
pub fn combine(
    base: Option<&[f32]>,
    next: &[f32],
    op: crate::paint::PixelCombine,
) -> Result<Arc<Allocation<f32>>, String> {
    validate(next)?;
    if let Some(base) = base {
        validate(base)?;
    }
    let mut out = resources::global().allocate_now::<f32, _>(next.len(), &storage::check)?;
    let base = base.filter(|b| b.len() == next.len());
    for (i, (&b, dst)) in next.iter().zip(out.as_mut_slice()).enumerate() {
        if i % 16384 == 0 {
            storage::check.check()?;
        }
        *dst = match op {
            crate::paint::PixelCombine::Replace => b,
            crate::paint::PixelCombine::Add => base.map_or(b, |a| a[i].max(b)),
            crate::paint::PixelCombine::Subtract => base.map_or(0., |a| (a[i] - b).max(0.)),
        };
    }
    Ok(Arc::new(out))
}
/// Move scalar coverage through a local inverse mapping without byte resampling.
/// Args: original grid and inverse local-coordinate function. Returns admitted float coverage.
pub fn warp(
    values: &[f32],
    w: u32,
    h: u32,
    inverse: impl Fn(f32, f32) -> (f32, f32),
) -> Result<Arc<Allocation<f32>>, String> {
    if values.len() != storage::count(w, h, 1)? {
        return Err("Selection dimensions changed".into());
    }
    validate(values)?;
    let mut out = resources::global().allocate_now::<f32, _>(values.len(), &storage::check)?;
    for y in 0..h {
        storage::check.check()?;
        for x in 0..w {
            let (sx, sy) = inverse(x as f32 + 0.5, y as f32 + 0.5);
            if !sx.is_finite() || !sy.is_finite() {
                return Err("Selection transform must be finite and invertible".into());
            }
            out.as_mut_slice()[(y * w + x) as usize] = sample(values, w, h, sx, sy);
        }
    }
    Ok(Arc::new(out))
}
/// Store coverage in the existing authoritative native mask pixel format.
/// Args: unit samples and dimensions. Returns RGB luminance coverage with opaque alpha.
pub fn mask_pixels(values: &[f32], w: u32, h: u32) -> Result<crate::document::Pixels, String> {
    if values.len() != storage::count(w, h, 1)? {
        return Err("Selection dimensions changed".into());
    }
    validate(values)?;
    if values
        .iter()
        .all(|v| (f32::from((*v * 255.).round() as u8) / 255.).to_bits() == v.to_bits())
    {
        return crate::document::Pixels::from_byte_allocation(
            w,
            h,
            storage::collect(values.iter().flat_map(|v| {
                let b = (*v * 255.).round() as u8;
                [b, b, b, 255]
            }))?,
        );
    }
    crate::document::Pixels::from_sample_allocation(
        w,
        h,
        crate::deep_color::Depth::F32,
        storage::collect(values.iter().flat_map(|v| [*v, *v, *v, 1.]))?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::masking::SelectionSpace;
    #[test]
    fn craft_selection_precision_mapping_keeps_adjacent_values_and_negative_zero() {
        let values = [-0., f32::from_bits(0x3e800001), 0.75, 1.];
        let source = SelectionSpace {
            w: 4,
            h: 1,
            transform: tiny_skia::Transform::from_translate(3., 5.),
        };
        let a = resample(&values, source, source, &storage::check).unwrap();
        for (a, b) in a.iter().zip(values) {
            assert_eq!(a.to_bits(), b.to_bits());
        }
        let target = SelectionSpace {
            transform: tiny_skia::Transform::from_translate(3.5, 5.),
            ..source
        };
        let mapped = resample(&values, source, target, &storage::check).unwrap();
        for (i, v) in mapped.iter().enumerate() {
            let expected = if i + 1 < values.len() {
                (f64::from(values[i]) + f64::from(values[i + 1])) / 2.
            } else {
                f64::from(values[i]) / 2.
            };
            assert!((f64::from(*v) - expected).abs() < 3e-8);
        }
        let moved = warp(&values, 4, 1, |x, y| (x - 1., y)).unwrap();
        assert_eq!(moved[1].to_bits(), values[0].to_bits());
        assert_eq!(moved[2].to_bits(), values[1].to_bits());
        assert!(validate(&[f32::NAN]).is_err());
        assert!(validate(&[1.0001]).is_err());
        assert!(
            resample(
                &values,
                source,
                SelectionSpace {
                    transform: tiny_skia::Transform::from_scale(0., 1.),
                    ..target
                },
                &storage::check
            )
            .is_err()
        );
        assert!(
            resample(&values, source, target, &|| Err(
                "Actual selection cancellation".into()
            ))
            .is_err()
        );
    }
    #[test]
    fn craft_selection_precision_morphology_and_combine_never_quantize_to_bytes() {
        let a = f32::from_bits(0x3e800001);
        let values = [a, 0.5, 0.75];
        let space = SelectionSpace {
            w: 3,
            h: 1,
            transform: tiny_skia::Transform::identity(),
        };
        let grow = crate::app::pixel_selection::edit_admitted(
            &values,
            space,
            crate::app::pixel_selection::Edit::Grow { radius: 1 },
        )
        .unwrap()
        .unwrap()
        .0;
        assert_eq!(grow.as_slice(), &[0.5, 0.75, 0.75]);
        let feather = crate::app::pixel_selection::edit_admitted(
            &values,
            space,
            crate::app::pixel_selection::Edit::Feather { radius: 1 },
        )
        .unwrap()
        .unwrap()
        .0;
        for (i, v) in feather.iter().enumerate() {
            let expected = values[i.saturating_sub(1)..(i + 2).min(3)]
                .iter()
                .map(|v| f64::from(*v))
                .sum::<f64>()
                / 9.;
            assert!((f64::from(*v) - expected).abs() < 2e-8);
        }
        let subtract = combine(
            Some(&values),
            &[a / 2., 0.1, 0.],
            crate::paint::PixelCombine::Subtract,
        )
        .unwrap();
        assert_eq!(subtract[0].to_bits(), (a - a / 2.).to_bits());
        assert_eq!(subtract[2].to_bits(), values[2].to_bits());
        let bytes = mask_pixels(&[0., 64. / 255., 128. / 255., 1.], 4, 1).unwrap();
        assert!(bytes.native.is_none());
        assert_eq!(
            bytes.data.as_slice(),
            &[
                0, 0, 0, 255, 64, 64, 64, 255, 128, 128, 128, 255, 255, 255, 255, 255
            ]
        );
        let pixels = mask_pixels(&values, 3, 1).unwrap();
        assert!(pixels.native.is_some());
        for (i, v) in values.iter().enumerate() {
            assert_eq!(pixels.sample(i)[0].to_bits(), v.to_bits());
        }
    }
}
