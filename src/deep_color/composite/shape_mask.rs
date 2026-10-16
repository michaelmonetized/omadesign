//! Authoritative luminance and alpha in the existing shape-mask placement.
use crate::{document::Pixels, resources::CancellationCheck};
use tiny_skia::Transform;

pub(super) struct ShapeMask<'a> {
    pub pixels: &'a Pixels,
    pub transform: Transform,
    inverse: [f64; 6],
}

impl<'a> ShapeMask<'a> {
    /// Validate an authored mask before allocating coverage work.
    /// Args: original pixels and cancellation. Returns a named refusal for unsupported color or range.
    pub(super) fn validate(pixels: &Pixels, cancel: &dyn CancellationCheck) -> Result<(), String> {
        let count = crate::deep_color::storage::count(pixels.w, pixels.h, 4)?;
        if pixels.native.is_none() && pixels.data.len() != count {
            return Err("Invalid original shape-mask byte dimensions".into());
        }
        pixels.validate_native()?;
        if pixels
            .native
            .as_ref()
            .is_some_and(|n| n.cmyk.is_some() || !n.icc_profile.is_empty())
        {
            return Err(
                "Profiled or CMYK shape masks need a native color-coverage pipeline".into(),
            );
        }
        if let Some(native) = &pixels.native {
            for chunk in native.samples.chunks(4096) {
                cancel.check()?;
                if chunk.iter().any(|v| !(0. ..=1.).contains(v)) {
                    return Err("Native shape-mask coverage requires samples in the unit interval; original samples are unchanged".into());
                }
            }
        }
        Ok(())
    }

    /// Bind original samples to device coordinates without making a display mask.
    /// Args: validated pixels and shared source-to-device transform. Returns an invertible sampler.
    pub(super) fn new(pixels: &'a Pixels, transform: Transform) -> Result<Self, String> {
        let [a, b, c, d, e, f] = [
            transform.sx,
            transform.ky,
            transform.kx,
            transform.sy,
            transform.tx,
            transform.ty,
        ]
        .map(f64::from);
        let det = a * d - b * c;
        if ![a, b, c, d, e, f, det].iter().all(|v| v.is_finite()) || det == 0. {
            return Err("Native shape-mask placement is nonfinite or singular".into());
        }
        Ok(Self {
            pixels,
            transform,
            inverse: [
                d / det,
                -b / det,
                -c / det,
                a / det,
                (c * f - d * e) / det,
                (b * e - a * f) / det,
            ],
        })
    }

    /// Sample premultiplied luminance directly from authored texels.
    /// Args: device point within the geometric mask support. Returns an unrounded bilinear weight.
    pub(super) fn at(&self, point: [f64; 2]) -> f64 {
        let [a, b, c, d, e, f] = self.inverse;
        let x = (a * point[0] + c * point[1] + e - 0.5).clamp(0., self.pixels.w as f64 - 1.);
        let y = (b * point[0] + d * point[1] + f - 0.5).clamp(0., self.pixels.h as f64 - 1.);
        let (ix, iy) = (x.floor() as usize, y.floor() as usize);
        let (tx, ty) = (x - x.floor(), y - y.floor());
        let mut value = 0.;
        for (dx, wx) in [(0, 1. - tx), (1, tx)] {
            for (dy, wy) in [(0, 1. - ty), (1, ty)] {
                let px = (ix + dx).min(self.pixels.w as usize - 1);
                let py = (iy + dy).min(self.pixels.h as usize - 1);
                let p = self
                    .pixels
                    .sample(py * self.pixels.w as usize + px)
                    .map(f64::from);
                value += (p[0] * 0.2126 + p[1] * 0.7152 + p[2] * 0.0722) * p[3] * wx * wy;
            }
        }
        value
    }
}
