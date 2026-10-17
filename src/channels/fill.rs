//! Connected scalar edits keep original float comparisons and admitted scratch.
use super::Plane;
use crate::resources::{Allocation, CancellationCheck};
use std::sync::Arc;

/// Find a four-connected scalar region through the actual soft selection.
/// Args: native plane, local seed, byte-scale tolerance, optional mask and cancellation.
/// Returns admitted coverage; no original sample or layer is changed.
pub(crate) fn region(
    source: &Plane,
    seed: (u32, u32),
    tolerance: f32,
    selection: Option<&[f32]>,
    cancel: &dyn CancellationCheck,
) -> Result<Arc<Allocation<f32>>, String> {
    source.validate()?;
    cancel.check()?;
    let n = source.samples.len();
    if n > u32::MAX as usize || seed.0 >= source.width || seed.1 >= source.height {
        return Err("Channel fill seed is outside its grid".into());
    }
    if !tolerance.is_finite() || !(0.0..=255.0).contains(&tolerance) {
        return Err("Channel fill tolerance must be between zero and 255".into());
    }
    if selection.is_some_and(|mask| mask.len() != n || crate::selection_coverage::validate(mask).is_err()) {
        return Err("Channel fill selection dimensions changed".into());
    }
    let pool = crate::resources::global();
    let mut seen = pool.allocate_now::<u8, _>(n, cancel)?;
    let start = (seed.1 * source.width + seed.0) as usize;
    if selection.is_some_and(|mask| mask[start] == 0.) {
        return crate::deep_color::storage::collect(std::iter::repeat_n(0.,n));
    }
    let mut queue = pool.allocate_now::<u32, _>(n, cancel)?;
    let value = source.samples[start];
    let threshold = tolerance / 255.;
    seen.as_mut_slice()[start] = 255;
    queue.as_mut_slice()[0] = start as u32;
    let mut head = 0;
    let mut tail = 1;
    while head < tail {
        if head % 16384 == 0 {
            cancel.check()?;
        }
        let i = queue[head] as usize;
        head += 1;
        let x = i % source.width as usize;
        for next in [
            if x > 0 { Some(i - 1) } else { None },
            if x + 1 < source.width as usize {
                Some(i + 1)
            } else {
                None
            },
            i.checked_sub(source.width as usize),
            i.checked_add(source.width as usize).filter(|v| *v < n),
        ]
        .into_iter()
        .flatten()
        {
            if seen[next] != 0 {
                continue;
            }
            seen.as_mut_slice()[next] = 1;
            if selection.is_none_or(|mask| mask[next] > 0.)
                && (source.samples[next] - value).abs() <= threshold
            {
                seen.as_mut_slice()[next] = 255;
                queue.as_mut_slice()[tail] = next as u32;
                tail += 1;
            }
        }
    }
    drop(queue);let mut output=pool.allocate_now::<f32,_>(n,cancel)?;
    for (i,(v,dst)) in seen.iter().zip(output.as_mut_slice()).enumerate(){if i%16384==0{cancel.check()?;}*dst=if *v==255{selection.map_or(1.,|mask|mask[i])}else{0.};}
    Ok(Arc::new(output))
}

/// Fill one connected native region with a scalar value and opacity.
/// Args: source, seed, tolerance, fill value, opacity, optional selection and cancellation.
/// Returns an immutable admitted result preserving every sample outside the region.
pub(crate) fn apply(
    source: &Plane,
    seed: (u32, u32),
    tolerance: f32,
    value: f32,
    opacity: f32,
    selection: Option<&[f32]>,
    cancel: &dyn CancellationCheck,
) -> Result<Plane, String> {
    if [value, opacity]
        .into_iter()
        .any(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
    {
        return Err("Channel fill value and opacity must be between zero and one".into());
    }
    let coverage = region(source, seed, tolerance, selection, cancel)?;
    let mut output = crate::resources::global().copy_now(&source.samples, cancel)?;
    for (chunk, out) in coverage
        .chunks(16384)
        .zip(output.as_mut_slice().chunks_mut(16384))
    {
        cancel.check()?;
        for (weight, dst) in chunk.iter().zip(out) {
            if *weight > 0. {
                let amount = *weight * opacity;
                *dst = (*dst + (value - *dst) * amount).clamp(0., 1.);
            }
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
    use crate::deep_color::storage;
    #[test]
    fn craft_scalar_fill_uses_native_connected_values_soft_selection_and_exact_outside_bits() {
        let source = Plane {
            width: 5,
            height: 2,
            transform: [1., 0., 0., 1., 0., 0.],
            samples: storage::collect(
                [
                    0.25,
                    0.25,
                    f32::from_bits(0x3e800001),
                    0.75,
                    -0.,
                    0.25,
                    0.25,
                    f32::from_bits(0x3e800001),
                    0.75,
                    -0.,
                ]
                .into_iter(),
            )
            .unwrap(),
        };
        let mask = [1., 128./255., 1., 1., 1., 0., 128./255., 1., 1., 1.];
        let out = apply(&source, (0, 0), 0., 1., 0.5, Some(&mask), &storage::check).unwrap();
        assert_eq!(out.samples[0], 0.625);
        let partial: f32 = 0.25 + (1. - 0.25) * (128. / 255. * 0.5);
        assert_eq!(out.samples[1].to_bits(), partial.to_bits());
        assert_eq!(out.samples[6].to_bits(), partial.to_bits());
        for i in [2, 3, 4, 5, 7, 8, 9] {
            assert_eq!(out.samples[i].to_bits(), source.samples[i].to_bits());
        }
        let near = region(&source, (0, 0), 1., None, &storage::check).unwrap();
        assert_eq!(near.as_slice(), &[1., 1., 1., 0., 0., 1., 1., 1., 0., 0.]);
        let alias = out.clone();
        let owner = Arc::downgrade(out.samples.lease());
        drop(out);
        assert!(owner.upgrade().is_some());
        drop(alias);
        assert!(owner.upgrade().is_none());
        assert!(
            apply(&source, (0, 0), 0., 1., 1., None, &|| Err(
                "Cancelled".into()
            ))
            .is_err()
        );
        assert!(region(&source, (5, 0), 0., None, &storage::check).is_err());
        assert!(region(&source, (0, 0), f32::NAN, None, &storage::check).is_err());
    }
}
