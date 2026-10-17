//! Scalar curves evaluate original float coverage without a byte LUT.
use super::Plane;
use crate::{deep_color::storage, resources::CancellationCheck};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Curve {
    Points(Vec<[f32; 2]>),
    Drawn(Box<[f32; 256]>),
}
impl Default for Curve {
    fn default() -> Self {
        Self::Points(vec![[0., 0.], [1., 1.]])
    }
}
pub(crate) struct Evaluator<'a> {
    curve: &'a Curve,
    second: [f64; 19],
    identity: bool,
}
impl Curve {
    /// Validate the actual curve and solve its natural cubic segments once.
    /// Args: none. Returns an evaluator or a readable error before sample allocation.
    pub(crate) fn prepare(&self) -> Result<Evaluator<'_>, String> {
        let mut second = [0.; 19];
        let identity = match self {
            Self::Points(points) => {
                if !(2..=19).contains(&points.len()) {
                    return Err("Use between 2 and 19 curve points".into());
                }
                if points
                    .iter()
                    .flatten()
                    .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
                {
                    return Err("Curve points must be finite and between 0 and 255".into());
                }
                if points.windows(2).any(|w| w[0][0] >= w[1][0]) {
                    return Err("Curve inputs must stay in increasing order".into());
                }
                let mut upper = [0.; 19];
                let mut rhs = [0.; 19];
                let n = points.len();
                for i in 1..n - 1 {
                    let h0 = f64::from(points[i][0]) - f64::from(points[i - 1][0]);
                    let h1 = f64::from(points[i + 1][0]) - f64::from(points[i][0]);
                    let left = h0 / 6.;
                    let diagonal = (h0 + h1) / 3.;
                    let right = h1 / 6.;
                    let slope = (f64::from(points[i + 1][1]) - f64::from(points[i][1])) / h1
                        - (f64::from(points[i][1]) - f64::from(points[i - 1][1])) / h0;
                    let denominator = diagonal - left * upper[i - 1];
                    upper[i] = right / denominator;
                    rhs[i] = (slope - left * rhs[i - 1]) / denominator;
                }
                for i in (1..n - 1).rev() {
                    second[i] = rhs[i] - upper[i] * second[i + 1];
                }
                points.first() == Some(&[0., 0.])
                    && points.last() == Some(&[1., 1.])
                    && points.iter().all(|p| p[0] == p[1])
            }
            Self::Drawn(values) => {
                if values
                    .iter()
                    .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
                {
                    return Err("Drawn curve values must be finite and between 0 and 255".into());
                }
                values
                    .iter()
                    .enumerate()
                    .all(|(i, v)| *v == i as f32 / 255.)
            }
        };
        Ok(Evaluator {
            curve: self,
            second,
            identity,
        })
    }
    /// Convert point curves to the same 256-entry drawing used by the graph.
    /// Args: none. Returns float output entries without changing source pixels.
    pub(crate) fn drawn(&self) -> Result<Self, String> {
        let evaluator = self.prepare()?;
        if matches!(self, Self::Drawn(_)) {
            return Ok(self.clone());
        }
        Ok(Self::Drawn(Box::new(std::array::from_fn(|i| {
            evaluator.sample(i as f32 / 255.)
        }))))
    }
    /// Fit a drawn curve with at most 19 natural-cubic control points.
    /// Args: none. Returns a bounded approximation; the drawing itself stays unchanged.
    pub(crate) fn fitted(&self) -> Result<Self, String> {
        let Self::Drawn(values) = self else {
            return Ok(self.clone());
        };
        self.prepare()?;
        let mut indices = vec![0, 255];
        while indices.len() < 19 {
            let curve = Self::Points(
                indices
                    .iter()
                    .map(|i| [*i as f32 / 255., values[*i]])
                    .collect(),
            );
            let eval = curve.prepare()?;
            let error = (1..255)
                .filter(|i| !indices.contains(i))
                .map(|i| (i, (eval.sample(i as f32 / 255.) - values[i]).abs()))
                .max_by(|a, b| a.1.total_cmp(&b.1));
            let Some((i, error)) = error else { break };
            if error <= 0.25 / 255. {
                break;
            }
            indices.push(i);
            indices.sort_unstable();
        }
        Ok(Self::Points(
            indices
                .into_iter()
                .map(|i| [i as f32 / 255., values[i]])
                .collect(),
        ))
    }
    /// Draw a continuous segment into the 256-entry float curve.
    /// Args: normalized graph endpoints. Returns whether actual drawing values changed.
    pub(crate) fn pencil_line(&mut self, from: [f32; 2], to: [f32; 2]) -> Result<bool, String> {
        if from
            .into_iter()
            .chain(to)
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
        {
            return Err("Curve drawing must stay inside the graph".into());
        }
        if !matches!(self, Self::Drawn(_)) {
            *self = self.drawn()?;
        }
        let Self::Drawn(values) = self else {
            unreachable!()
        };
        let mut changed = false;
        let first = (from[0].min(to[0]) * 255.).round() as usize;
        let last = (from[0].max(to[0]) * 255.).round() as usize;
        for i in first..=last {
            let t = if from[0] == to[0] {
                1.
            } else {
                ((i as f32 / 255. - from[0]) / (to[0] - from[0])).clamp(0., 1.)
            };
            let value = from[1] + (to[1] - from[1]) * t;
            changed |= values[i] != value;
            values[i] = value;
        }
        Ok(changed)
    }
    /// Smooth the actual drawing while preserving its endpoints.
    /// Args: none. Returns a new drawing; does not change any channel.
    pub(crate) fn smooth(&self) -> Result<Self, String> {
        let Self::Drawn(before) = self.drawn()? else {
            unreachable!()
        };
        let mut after = before.clone();
        for i in 1..255 {
            after[i] = before[i - 1] * 0.25 + before[i] * 0.5 + before[i + 1] * 0.25;
        }
        Ok(Self::Drawn(after))
    }
}
impl Evaluator<'_> {
    /// Evaluate one original scalar sample at native precision.
    /// Args: finite unit input. Returns unit output, constant beyond the first and last points.
    pub(crate) fn sample(&self, input: f32) -> f32 {
        if self.identity {
            return input;
        }
        match self.curve {
            Curve::Drawn(values) => {
                let at = f64::from(input.clamp(0., 1.)) * 255.;
                let i = (at.floor() as usize).min(255);
                let t = at - i as f64;
                (f64::from(values[i])
                    + (f64::from(values[(i + 1).min(255)]) - f64::from(values[i])) * t)
                    as f32
            }
            Curve::Points(p) => {
                if input <= p[0][0] {
                    return p[0][1];
                }
                if input >= p[p.len() - 1][0] {
                    return p[p.len() - 1][1];
                }
                let upper = p.partition_point(|p| p[0] < input);
                if p[upper][0] == input {
                    return p[upper][1];
                }
                let i = upper - 1;
                let x0 = f64::from(p[i][0]);
                let x1 = f64::from(p[i + 1][0]);
                let h = x1 - x0;
                let a = (x1 - f64::from(input)) / h;
                let b = (f64::from(input) - x0) / h;
                (a * f64::from(p[i][1])
                    + b * f64::from(p[i + 1][1])
                    + ((a * a * a - a) * self.second[i] + (b * b * b - b) * self.second[i + 1])
                        * h
                        * h
                        / 6.) as f32
            }
        }
        .clamp(0., 1.)
    }
}
/// Apply a validated curve to immutable float coverage and optional soft selection.
/// Args: full source plane, curve, same-grid byte mask and cancellation. Returns an admitted plane or leaves every source unchanged.
pub(crate) fn apply(
    source: &Plane,
    curve: &Curve,
    selection: Option<&[u8]>,
    cancel: &dyn CancellationCheck,
) -> Result<Plane, String> {
    source.validate()?;
    let evaluator = curve.prepare()?;
    cancel.check()?;
    if selection.is_some_and(|mask| mask.len() != source.samples.len()) {
        return Err("Channel curve selection dimensions changed".into());
    }
    if evaluator.identity {
        return Ok(source.clone());
    }
    let mut output =
        crate::resources::global().allocate_now::<f32, _>(source.samples.len(), cancel)?;
    for (index, (chunk, out)) in source
        .samples
        .chunks(16384)
        .zip(output.as_mut_slice().chunks_mut(16384))
        .enumerate()
    {
        cancel.check()?;
        for (j, (before, dst)) in chunk.iter().zip(out).enumerate() {
            let i = index * 16384 + j;
            let weight = selection.map_or(1., |mask| f32::from(mask[i]) / 255.);
            let mapped = evaluator.sample(*before);
            *dst = if weight == 0. || mapped.to_bits() == before.to_bits() {
                *before
            } else {
                (*before + (mapped - *before) * weight).clamp(0., 1.)
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
    fn plane(values: &[f32]) -> Plane {
        Plane {
            width: values.len() as u32,
            height: 1,
            transform: [1., 0., 0., 1., 0., 0.],
            samples: storage::collect(values.iter().copied()).unwrap(),
        }
    }
    #[test]
    fn craft_channel_curves_natural_cubic_matches_analytic_segments_at_float_inputs() {
        let curve = Curve::Points(vec![[0., 0.], [0.5, 1.], [1., 0.]]);
        let eval = curve.prepare().unwrap();
        for i in 0..=1024 {
            let x = i as f32 / 1024.;
            let z = if x <= 0.5 { x } else { 1. - x };
            let expected = 3. * z - 4. * z * z * z;
            assert!((eval.sample(x) - expected).abs() < 2e-7, "{x}");
        }
        assert_eq!(eval.sample(0.5), 1.);
        let line = Curve::Points(vec![[0.25, 0.1], [0.75, 0.9]]);
        let e = line.prepare().unwrap();
        assert_eq!(e.sample(0.), 0.1);
        assert_eq!(e.sample(1.), 0.9);
        assert!((e.sample(0.5) - 0.5).abs() < 1e-7);
        assert_ne!(
            e.sample(f32::from_bits(0x3f000000)).to_bits(),
            e.sample(f32::from_bits(0x3f000001)).to_bits()
        );
    }
    #[test]
    fn craft_channel_curves_identity_masks_and_aliases_preserve_original_bits() {
        let source = plane(&[-0., f32::from_bits(0x3e800001), 0.5, 1.]);
        let unchanged = apply(&source, &Curve::default(), None, &storage::check).unwrap();
        assert!(Arc::ptr_eq(&source.samples, &unchanged.samples));
        let inverse = Curve::Points(vec![[0., 1.], [1., 0.]]);
        let mask = [0, 128, 255, 0];
        let output = apply(&source, &inverse, Some(&mask), &storage::check).unwrap();
        assert_eq!(output.samples[0].to_bits(), (-0.0f32).to_bits());
        assert_eq!(output.samples[3], 1.);
        let x = source.samples[1];
        assert_eq!(
            output.samples[1].to_bits(),
            (x + (1. - x - x) * (128. / 255.)).to_bits()
        );
        assert_eq!(output.samples[2], 0.5);
        let owner = Arc::downgrade(output.samples.lease());
        let alias = output.clone();
        drop(output);
        assert!(owner.upgrade().is_some());
        drop(alias);
        assert!(owner.upgrade().is_none());
        assert_eq!(source.samples[1].to_bits(), 0x3e800001);
    }
    #[test]
    fn craft_channel_curves_drawing_smoothing_and_fitting_keep_endpoints_and_native_steps() {
        let mut curve = Curve::default().drawn().unwrap();
        assert!(curve.pencil_line([0.25, 0.75], [0.75, 0.25]).unwrap());
        let Curve::Drawn(values) = &curve else {
            unreachable!()
        };
        assert_eq!(values[0], 0.);
        assert_eq!(values[255], 1.);
        assert!((values[128] - (1. - 128. / 255.)).abs() < 1e-7);
        let smooth = curve.smooth().unwrap();
        let Curve::Drawn(after) = &smooth else {
            unreachable!()
        };
        assert_eq!(after[0], values[0]);
        assert_eq!(after[255], values[255]);
        assert_eq!(
            after[128],
            values[127] * 0.25 + values[128] * 0.5 + values[129] * 0.25
        );
        let fitted = Curve::default().drawn().unwrap().fitted().unwrap();
        let Curve::Points(points) = &fitted else {
            unreachable!()
        };
        assert_eq!(points, &vec![[0., 0.], [1., 1.]]);
        assert_eq!(
            fitted
                .prepare()
                .unwrap()
                .sample(f32::from_bits(0x3e800001))
                .to_bits(),
            0x3e800001
        );
    }
    #[test]
    fn craft_channel_curves_invalid_and_cancelled_work_keeps_source_owner() {
        for points in [
            vec![],
            vec![[0., 0.]],
            vec![[0.5, 0.], [0.5, 1.]],
            vec![[0.75, 0.], [0.25, 1.]],
            vec![[0., f32::NAN], [1., 1.]],
            vec![[0., 0.], [1., 1.1]],
        ] {
            assert!(Curve::Points(points).prepare().is_err());
        }
        let source = plane(&[0.25, 0.75]);
        let before = source.samples.clone();
        let inverse = Curve::Points(vec![[0., 1.], [1., 0.]]);
        assert!(apply(&source, &inverse, Some(&[255]), &storage::check).is_err());
        assert!(apply(&source, &inverse, None, &|| Err("Cancelled".into())).is_err());
        assert!(Arc::ptr_eq(&source.samples, &before));
        assert_eq!(source.samples.as_slice(), &[0.25, 0.75]);
    }
}
