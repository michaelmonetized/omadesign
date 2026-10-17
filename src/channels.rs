//! Authoritative scalar channels use the existing admitted native sample storage.
pub(crate) mod curve;
pub(crate) mod fill;
pub mod quick_mask;
use crate::{
    deep_color::storage,
    document::Document,
    resources::{self, Allocation, CancellationCheck},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::Arc};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Plane {
    pub width: u32,
    pub height: u32,
    pub transform: [f32; 6],
    #[serde(with = "crate::deep_color::sample_bytes")]
    pub samples: Arc<Allocation<f32>>,
}
impl Plane {
    /// Validate a scalar grid and placement without reading every sample.
    /// Args: none. Returns an error for damaged dimensions or placement.
    fn validate_grid(&self) -> Result<(), String> {
        if self.samples.len() != storage::count(self.width, self.height, 1)? {
            return Err("Channel dimensions do not match its samples".into());
        }
        if !valid_placement(self.placement()) {
            return Err("Channel placement must be finite and invertible".into());
        }
        Ok(())
    }
    /// Validate all authored coverage at an input or publication boundary.
    /// Args: none. Returns an error for damaged scalar data without changing it.
    pub fn validate(&self) -> Result<(), String> {
        self.validate_grid()?;
        if self
            .samples
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err("Channel coverage must be finite and between zero and one".into());
        }
        Ok(())
    }
    /// Read the stored grid-to-document placement.
    /// Args: none. Returns the existing affine transform without changing data.
    pub fn placement(&self) -> tiny_skia::Transform {
        let [a, b, c, d, e, f] = self.transform;
        tiny_skia::Transform::from_row(a, b, c, d, e, f)
    }
    /// Keep an existing mask's full grid and placement as an admitted scalar plane.
    /// Args: byte coverage, dimensions, placement and cancellation. Returns independent float coverage.
    pub fn from_mask(
        values: &[u8],
        width: u32,
        height: u32,
        transform: tiny_skia::Transform,
        cancel: &dyn CancellationCheck,
    ) -> Result<Self, String> {
        if values.len() != storage::count(width, height, 1)? {
            return Err("Selection dimensions changed".into());
        }
        let mut samples = resources::global().allocate_now::<f32, _>(values.len(), cancel)?;
        for (input, output) in values
            .chunks(16384)
            .zip(samples.as_mut_slice().chunks_mut(16384))
        {
            cancel.check()?;
            for (v, dst) in input.iter().zip(output) {
                *dst = f32::from(*v) / 255.;
            }
        }
        let plane = Self {
            width,
            height,
            transform: [
                transform.sx,
                transform.ky,
                transform.kx,
                transform.sy,
                transform.tx,
                transform.ty,
            ],
            samples: Arc::new(samples),
        };
        plane.validate()?;
        Ok(plane)
    }
    /// Invert the actual full-resolution channel while retaining the source allocation.
    /// Args: cancellation owner. Returns a new plane; no input or history is changed on failure.
    pub fn inverted(&self, cancel: &dyn CancellationCheck) -> Result<Self, String> {
        self.validate()?;
        let mut samples = resources::global().allocate_now::<f32, _>(self.samples.len(), cancel)?;
        for (input, output) in self
            .samples
            .chunks(16384)
            .zip(samples.as_mut_slice().chunks_mut(16384))
        {
            cancel.check()?;
            for (v, dst) in input.iter().zip(output) {
                *dst = 1. - *v;
            }
        }
        Ok(Self {
            samples: Arc::new(samples),
            ..self.clone()
        })
    }
    /// Quantize coverage once for the editor's existing byte-selection operations.
    /// Args: cancellation owner. Returns admitted mask bytes at this plane's exact placement.
    pub fn mask(&self, cancel: &dyn CancellationCheck) -> Result<Arc<Allocation<u8>>, String> {
        self.validate()?;
        let mut bytes = resources::global().allocate_now::<u8, _>(self.samples.len(), cancel)?;
        for (input, output) in self
            .samples
            .chunks(16384)
            .zip(bytes.as_mut_slice().chunks_mut(16384))
        {
            cancel.check()?;
            for (v, dst) in input.iter().zip(output) {
                *dst = (*v * 255.).round() as u8;
            }
        }
        Ok(Arc::new(bytes))
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Channel {
    pub id: u64,
    pub name: String,
    pub plane: Plane,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    Composite,
    Color(usize),
    Transparency,
    Saved(u64),
    QuickMask,
}
impl Target {
    /// Name a concrete color, transparency or saved channel in its document.
    /// Args: current document. Returns a readable name without changing selection.
    pub fn name(self, doc: &Document) -> String {
        let cmyk = doc.color.model == crate::deep_color::cms::Model::Cmyk;
        match self {
            Self::Composite => if cmyk { "CMYK" } else { "RGB" }.into(),
            Self::Color(i) => if cmyk {
                ["Cyan", "Magenta", "Yellow", "Black"].get(i)
            } else {
                ["Red", "Green", "Blue"].get(i)
            }
            .copied()
            .unwrap_or("Missing channel")
            .into(),
            Self::Transparency => "Transparency".into(),
            Self::QuickMask => "Quick Mask".into(),
            Self::Saved(id) => doc
                .channels
                .iter()
                .find(|c| c.id == id)
                .map_or_else(|| "Missing channel".into(), |c| c.name.clone()),
        }
    }
}
/// Check stored channel identities, names and complete scalar payloads.
/// Args: saved channels. Returns an error before typed mutation or native publication.
pub fn validate(channels: &[Channel]) -> Result<(), String> {
    validate_metadata(channels)?;
    for channel in channels {
        channel.plane.validate()?;
    }
    Ok(())
}

/// Validate channel identities and placement during ordinary artwork edits.
/// Args: already admitted channels. Returns structural errors without rereading every pixel.
pub(crate) fn validate_metadata(channels: &[Channel]) -> Result<(), String> {
    let mut ids = HashSet::new();
    for channel in channels {
        if channel.id == 0 || !ids.insert(channel.id) {
            return Err("Saved channel IDs must be unique and nonzero".into());
        }
        if channel.name.trim().is_empty()
            || channel.name.chars().count() > 64
            || channel.name.chars().any(char::is_control)
        {
            return Err("Channel names need 1–64 printable characters".into());
        }
        channel.plane.validate_grid()?;
    }
    Ok(())
}
/// Choose the first unused ordinary alpha-channel name.
/// Args: saved channels and a printable base. Returns a free name without changing the document.
pub fn next_name(channels: &[Channel], base: &str) -> String {
    (1..=channels.len() + 1)
        .map(|i| format!("{base} {i}"))
        .find(|name| !channels.iter().any(|c| c.name == *name))
        .unwrap()
}
/// Render an actual full-resolution color/transparency plane through the existing compositor.
/// Args: immutable document, concrete channel and cancellation. Returns admitted scalar coverage.
pub fn render(
    doc: &Document,
    target: Target,
    cancel: &dyn CancellationCheck,
) -> Result<Plane, String> {
    render_scaled(doc, target, 1., cancel)
}

/// Render a bounded scalar thumbnail through the same source compositor.
/// Args: immutable source, selected channel, maximum edge and cancellation. Returns preview data only.
pub fn preview(
    doc: &Document,
    target: Target,
    edge: u32,
    cancel: &dyn CancellationCheck,
) -> Result<Plane, String> {
    let factor = (edge.max(1) as f64 / doc.width.max(doc.height).max(1.) as f64).min(1.);
    render_scaled(doc, target, factor, cancel)
}

fn render_scaled(
    doc: &Document,
    target: Target,
    factor: f64,
    cancel: &dyn CancellationCheck,
) -> Result<Plane, String> {
    cancel.check()?;
    if target == Target::QuickMask {
        let mask=doc.quick_mask.as_ref().ok_or("Quick Mask is no longer open")?;
        mask.validate(&doc.channels)?;
        return Ok(mask.channel.plane.clone());
    }
    if let Target::Saved(id) = target {
        let plane = doc
            .channels
            .iter()
            .find(|c| c.id == id)
            .ok_or("That saved channel no longer exists")?
            .plane
            .clone();
        plane.validate()?;
        return Ok(plane);
    }
    let cmyk = doc.color.model == crate::deep_color::cms::Model::Cmyk;
    if let Target::Color(i) = target
        && i >= if cmyk { 4 } else { 3 }
    {
        return Err("That color channel does not exist in this document".into());
    }
    if crate::deep_color::has_native(doc) {
        let pixels = crate::deep_color::composite_scaled_with_cancel(doc, factor, cancel)?;
        let mut values = resources::global()
            .allocate_now::<f32, _>(storage::count(pixels.w, pixels.h, 1)?, cancel)?;
        for (i, dst) in values.as_mut_slice().iter_mut().enumerate() {
            if i % 16384 == 0 {
                cancel.check()?;
            }
            let rgba = pixels.sample(i);
            *dst = match target {
                Target::Transparency => rgba[3],
                Target::Color(c) if cmyk => *pixels
                    .native
                    .as_ref()
                    .and_then(|n| n.cmyk.as_ref())
                    .and_then(|p| p.get(i * 5 + c))
                    .ok_or("Missing authoritative CMYK channel")?,
                Target::Color(c) => rgba[c],
                _ => rgba[0] * 0.2126 + rgba[1] * 0.7152 + rgba[2] * 0.0722,
            }
            .clamp(0., 1.);
        }
        return Ok(Plane {
            width: pixels.w,
            height: pixels.h,
            transform: [1. / factor as f32, 0., 0., 1. / factor as f32, 0., 0.],
            samples: Arc::new(values),
        });
    }
    let size = crate::export::RenderSize::new(doc, factor)?;
    let (w, h) = (size.width, size.height);
    let _surface = resources::global().reserve_now(storage::count(w, h, 4)? as u64, cancel)?;
    let pixels = crate::compositor::render_export_sized(doc, size)?;
    cancel.check()?;
    let mut values =
        resources::global().allocate_now::<f32, _>(storage::count(w, h, 1)?, cancel)?;
    for (i, (pixel, dst)) in pixels
        .pixels()
        .iter()
        .zip(values.as_mut_slice())
        .enumerate()
    {
        if i % 16384 == 0 {
            cancel.check()?;
        }
        let p = pixel.demultiply();
        *dst = match target {
            Target::Transparency => f32::from(p.alpha()) / 255.,
            Target::Color(0) => f32::from(p.red()) / 255.,
            Target::Color(1) => f32::from(p.green()) / 255.,
            Target::Color(2) => f32::from(p.blue()) / 255.,
            _ => {
                (f32::from(p.red()) * 0.2126
                    + f32::from(p.green()) * 0.7152
                    + f32::from(p.blue()) * 0.0722)
                    / 255.
            }
        };
    }
    Ok(Plane {
        width: w,
        height: h,
        transform: [1. / factor as f32, 0., 0., 1. / factor as f32, 0., 0.],
        samples: Arc::new(values),
    })
}

#[cfg(test)]
pub(crate) mod tests;

/// Check a channel or selection placement before passing it to the raster mapper.
/// Args: affine transform. Returns false for singular, nonfinite or nonrepresentable inverses.
pub(crate) fn valid_placement(transform: tiny_skia::Transform) -> bool {
    transform.is_finite()
        && (f64::from(transform.sx) * f64::from(transform.sy)
            - f64::from(transform.kx) * f64::from(transform.ky))
            != 0.
        && transform
            .invert()
            .is_some_and(|inverse| inverse.is_finite())
}
