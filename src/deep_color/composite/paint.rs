//! Unrounded authored paint sampling inside the precise compositor.
use crate::{
    document::Fill,
    geom::{Bounds, Pt},
    gradient::{Gradient, GradientKind, GradientStop, Spread},
    resources::{self, CancellationCheck, Lease},
};
use std::sync::Arc;

pub(super) enum Paint {
    Solid([f32; 4]),
    Gradient {
        gradient: Gradient,
        bounds: Bounds,
        inverse_rotation: tiny_skia::Transform,
        _memory: Arc<Lease>,
    },
}
impl Paint {
    /// Prepare an existing authored paint without mutating its stops or profile.
    /// Args: fill, shape geometry bounds/rotation, document color and cancellation. Returns float paint or a precise refusal.
    pub(super) fn new(
        fill: &Fill,
        bounds: Bounds,
        rotation: f32,
        color: &crate::deep_color::cms::DocumentColor,
        cancel: &dyn CancellationCheck,
    ) -> Result<Self, String> {
        cancel.check()?;
        if let Fill::Solid(value) = fill {
            let rgb = [
                value.r as f32 / 255.,
                value.g as f32 / 255.,
                value.b as f32 / 255.,
            ];
            let rgb = if color.icc_profile.is_empty() {
                rgb
            } else {
                let converted = crate::deep_color::cms::transform(
                    &rgb,
                    crate::deep_color::cms::Model::Rgb,
                    &color.icc_profile,
                    false,
                    color.intent,
                )?;
                [converted[0], converted[1], converted[2]]
            };
            return Ok(Self::Solid([rgb[0], rgb[1], rgb[2], value.a as f32 / 255.]));
        }
        if fill.is_none() {
            return Ok(Self::Solid([0.; 4]));
        }
        if let Fill::Gradient(gradient) = fill {
            return Self::from_gradient(gradient, bounds, rotation, color, cancel);
        }
        let (kind, from, to, c0, c1) = match fill {
            Fill::Linear { from, to, c0, c1 } => (GradientKind::Linear, *from, *to, *c0, *c1),
            Fill::Radial { c0, c1 } => (GradientKind::Radial, [0.5, 0.5], [1., 0.5], *c0, *c1),
            _ => return Err("Native vector patterns require a float paint renderer".into()),
        };
        Self::prepare(
            Gradient {
                kind,
                from,
                to,
                stops: Vec::new(),
                radial_axis: None,
                spread: Spread::Pad,
                focal: [0., 0.],
                inner_radius: 0.,
            },
            &[
                GradientStop {
                    offset: 0.,
                    color: c0,
                },
                GradientStop {
                    offset: 1.,
                    color: c1,
                },
            ],
            bounds,
            rotation,
            color,
            cancel,
        )
    }

    /// Prepare borrowed stroke or fill fields with a fallible admitted stop copy.
    /// Args: saved gradient, shape bounds/rotation, document color and cancellation. Returns normalized native paint.
    pub(super) fn from_gradient(
        source: &Gradient,
        bounds: Bounds,
        rotation: f32,
        color: &crate::deep_color::cms::DocumentColor,
        cancel: &dyn CancellationCheck,
    ) -> Result<Self, String> {
        Self::prepare(
            Gradient {
                kind: source.kind,
                from: source.from,
                to: source.to,
                stops: Vec::new(),
                radial_axis: source.radial_axis,
                spread: source.spread,
                focal: source.focal,
                inner_radius: source.inner_radius,
            },
            &source.stops,
            bounds,
            rotation,
            color,
            cancel,
        )
    }

    fn prepare(
        field: Gradient,
        stops: &[GradientStop],
        bounds: Bounds,
        rotation: f32,
        color: &crate::deep_color::cms::DocumentColor,
        cancel: &dyn CancellationCheck,
    ) -> Result<Self, String> {
        cancel.check()?;
        let bytes = stops
            .len()
            .max(4)
            .checked_mul(2)
            .ok_or("Native gradient storage overflow")?
            .checked_mul(std::mem::size_of::<crate::gradient::GradientStop>())
            .ok_or("Native gradient storage overflow")?;
        let memory = resources::global().reserve_now(bytes as u64, cancel)?;
        let mut gradient = field;
        gradient
            .stops
            .try_reserve_exact(stops.len().max(2))
            .map_err(|_| "Not enough memory for native gradient stops")?;
        gradient.stops.extend_from_slice(stops);
        if !matches!(gradient.kind, GradientKind::Linear | GradientKind::Radial) {
            return Err(
                "Native vector shape-distance and conic gradients require a float paint renderer"
                    .into(),
            );
        }
        if !color.icc_profile.is_empty() {
            return Err("Profiled native vector gradients require a float paint renderer with batched ICC sampling".into());
        }
        if gradient
            .from
            .into_iter()
            .chain(gradient.to)
            .chain(gradient.focal)
            .chain(gradient.radial_axis.into_iter().flatten())
            .chain([gradient.inner_radius])
            .any(|v| !v.is_finite())
            || gradient.stops.iter().any(|s| !s.offset.is_finite())
        {
            return Err("Native gradient has nonfinite field or stop coordinates".into());
        }
        let (a, b) = gradient.endpoints(bounds);
        if gradient.kind == GradientKind::Linear && (b - a).length_sq() == 0.
            || gradient.kind == GradientKind::Radial
                && gradient.radial_transform(bounds).invert().is_none()
        {
            return Err("Native gradient has a singular field transform".into());
        }
        gradient.normalize();
        memory.resize_now(
            (gradient.stops.capacity() * std::mem::size_of::<crate::gradient::GradientStop>())
                as u64,
            cancel,
        )?;
        let center = bounds.center();
        let inverse_rotation =
            tiny_skia::Transform::from_rotate_at(rotation.to_degrees(), center.x, center.y)
                .invert()
                .ok_or("Native gradient rotation is singular")?;
        Ok(Self::Gradient {
            gradient,
            bounds,
            inverse_rotation,
            _memory: memory,
        })
    }
    /// Sample normalized premultiplied stop interpolation without integer rounding.
    /// Args: world point. Returns straight authored RGBA or an invalid-field error.
    pub(super) fn sample(&self, point: Pt) -> Result<[f32; 4], String> {
        match self {
            Self::Solid(value) => Ok(*value),
            Self::Gradient {
                gradient,
                bounds,
                inverse_rotation,
                ..
            } => {
                let mut p = tiny_skia::Point::from_xy(point.x, point.y);
                inverse_rotation.map_point(&mut p);
                let t = gradient.position(*bounds, Pt::new(p.x, p.y));
                if !t.is_finite() {
                    return Err("Native gradient field is undefined at the sample point".into());
                }
                Ok(sample_stops(gradient, t))
            }
        }
    }
}
/// Interpolate authored stops in premultiplied sRGB. Args: existing gradient and field position. Returns straight float RGBA.
fn sample_stops(gradient: &Gradient, position: f32) -> [f32; 4] {
    let t = match gradient.spread {
        Spread::Pad => position.clamp(0., 1.),
        Spread::Repeat => position.rem_euclid(1.),
        Spread::Reflect => 1. - (position.rem_euclid(2.) - 1.).abs(),
    } as f64;
    let rgba = |c: crate::color::Rgba| {
        [
            c.r as f32 / 255.,
            c.g as f32 / 255.,
            c.b as f32 / 255.,
            c.a as f32 / 255.,
        ]
    };
    let first = &gradient.stops[0];
    if t < (first.offset as f64) {
        return rgba(first.color);
    }
    for pair in gradient.stops.windows(2) {
        if t < pair[1].offset as f64 {
            let amount = ((t - pair[0].offset as f64)
                / (pair[1].offset as f64 - pair[0].offset as f64))
                .clamp(0., 1.);
            let a = pair[0].color;
            let b = pair[1].color;
            let alpha = (a.a as f64 * (1. - amount) + b.a as f64 * amount) / 255.;
            let channel = |x: u8, y: u8| {
                if alpha == 0. {
                    0.
                } else {
                    ((x as f64 * a.a as f64 * (1. - amount) + y as f64 * b.a as f64 * amount)
                        / (255. * 255. * alpha)) as f32
                }
            };
            return [
                channel(a.r, b.r),
                channel(a.g, b.g),
                channel(a.b, b.b),
                alpha as f32,
            ];
        }
    }
    rgba(gradient.stops.last().unwrap().color)
}
