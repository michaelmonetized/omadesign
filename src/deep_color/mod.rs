//! Native document samples. RGBA8 is a display cache; authored samples remain authoritative.
mod composite;
pub mod retouch;
pub mod storage;
pub mod surface;
pub use composite::{blend_pixel, composite};

use crate::document::Pixels;
use crate::resources::{self, Allocation, CancellationCheck, Lease};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Depth {
    #[default]
    U8,
    U16,
    F32,
}
impl Depth {
    /// Photoshop sample width.
    /// Args: self. Returns bits per component.
    pub fn bits(self) -> u16 {
        match self {
            Self::U8 => 8,
            Self::U16 => 16,
            Self::F32 => 32,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NativePixels {
    pub depth: Depth,
    #[serde(with = "sample_bytes")]
    pub samples: Arc<Allocation<f32>>,
}

impl Pixels {
    /// Construct authoritative RGBA samples and their display cache.
    /// Args: dimensions, depth and straight RGBA samples. Returns validated pixels or an error.
    pub fn from_samples(w: u32, h: u32, depth: Depth, samples: Vec<f32>) -> Result<Self, String> {
        Self::from_sample_allocation(w, h, depth, storage::adopt(samples)?)
    }

    /// Transfer an admitted RAW development without copying or counting samples twice.
    /// Args: dimensions, depth, owned straight samples, exclusive lease and cancellation owner.
    /// Returns authoritative pixels and an independently admitted display cache.
    pub fn from_admitted_samples<C: CancellationCheck + ?Sized>(
        w: u32,
        h: u32,
        depth: Depth,
        samples: Vec<f32>,
        memory: Arc<Lease>,
        cancel: &C,
    ) -> Result<Self, String> {
        cancel.check()?;
        let samples = resources::global().adopt(samples, memory)?;
        Self::from_sample_allocation_checked(w, h, depth, samples, cancel)
    }

    pub fn from_sample_allocation(
        w: u32,
        h: u32,
        depth: Depth,
        samples: Arc<Allocation<f32>>,
    ) -> Result<Self, String> {
        Self::from_sample_allocation_checked(w, h, depth, samples, &storage::check)
    }
    fn from_sample_allocation_checked<C: CancellationCheck + ?Sized>(
        w: u32,
        h: u32,
        depth: Depth,
        samples: Arc<Allocation<f32>>,
        cancel: &C,
    ) -> Result<Self, String> {
        let count = storage::count(w, h, 4)?;
        if samples.len() != count || samples.iter().any(|s| !s.is_finite()) {
            return Err("Invalid native pixel samples".into());
        }
        let mut data = resources::global().allocate_now::<u8, _>(count, cancel)?;
        for (dst, src) in data
            .as_mut_slice()
            .chunks_mut(65536)
            .zip(samples.chunks(65536))
        {
            cancel.check()?;
            for (dst, &src) in dst.iter_mut().zip(src) {
                *dst = byte(src);
            }
        }
        let mut out = Self::from_byte_allocation(w, h, Arc::new(data))?;
        out.native = Some(NativePixels { depth, samples });
        Ok(out)
    }

    /// Share authoritative samples without a history-sized copy.
    /// Args: self. Returns immutable straight RGBA storage, admitting conversion for a byte image.
    pub fn samples(&self) -> Result<Arc<Allocation<f32>>, String> {
        if let Some(native) = &self.native {
            return Ok(native.samples.clone());
        }
        let mut samples =
            resources::global().allocate_now::<f32, _>(self.data.len(), &storage::check)?;
        for (dst, &src) in samples.as_mut_slice().iter_mut().zip(self.data.iter()) {
            *dst = src as f32 / 255.;
        }
        Ok(Arc::new(samples))
    }
    /// Admit an independent authoring copy before editing shared samples.
    /// Args: cancellation owner. Returns mutable owned samples or preserves the existing image on failure.
    pub fn copy_samples<C: CancellationCheck + ?Sized>(
        &self,
        cancel: &C,
    ) -> Result<Allocation<f32>, String> {
        self.samples()?.try_clone_now(cancel)
    }

    /// Read one authored pixel.
    /// Args: zero-based pixel index. Returns straight RGBA or transparent outside the image.
    pub fn sample(&self, i: usize) -> [f32; 4] {
        if let Some(n) = &self.native {
            n.samples
                .get(i * 4..i * 4 + 4)
                .and_then(|v| v.try_into().ok())
                .unwrap_or([0.; 4])
        } else {
            self.data
                .get(i * 4..i * 4 + 4)
                .map(|v| {
                    [
                        v[0] as f32 / 255.,
                        v[1] as f32 / 255.,
                        v[2] as f32 / 255.,
                        v[3] as f32 / 255.,
                    ]
                })
                .unwrap_or([0.; 4])
        }
    }

    /// Replace an edited pixel buffer while retaining its precision.
    /// Args: straight samples. Returns an error for malformed or nonfinite samples.
    pub fn set_samples(&mut self, samples: Vec<f32>) -> Result<(), String> {
        self.set_sample_allocation(storage::adopt(samples)?)
    }
    pub fn set_sample_allocation(&mut self, samples: Arc<Allocation<f32>>) -> Result<(), String> {
        let depth = self.native.as_ref().map_or(Depth::F32, |n| n.depth);
        let mut out = Self::from_sample_allocation(self.w, self.h, depth, samples)?;
        out.version = self.version.wrapping_add(1);
        *self = out;
        Ok(())
    }

    /// Apply a mask in authored precision.
    /// Args: a matching luminance mask. Returns a new image with multiplied alpha.
    pub fn masked(&self, mask: &Pixels) -> Result<Self, String> {
        if (self.w, self.h) != (mask.w, mask.h) {
            return Err("Mask dimensions do not match pixels".into());
        }
        let mut samples = self.copy_samples(&storage::check)?;
        for (i, p) in samples.as_mut_slice().chunks_exact_mut(4).enumerate() {
            let m = mask.sample(i);
            p[3] *= ((m[0] * 0.2126 + m[1] * 0.7152 + m[2] * 0.0722) * m[3]).clamp(0., 1.);
        }
        Self::from_sample_allocation(
            self.w,
            self.h,
            self.native.as_ref().map_or(Depth::F32, |n| n.depth),
            Arc::new(samples),
        )
    }

    /// Apply a native raster filter.
    /// Args: filter settings and optional byte coverage selection. Returns a full-precision edited buffer.
    pub fn filtered(
        &self,
        settings: &crate::raster::Settings,
        selection: Option<&[u8]>,
    ) -> Result<Self, String> {
        let samples = self.samples()?;
        let source = samples.as_slice().as_chunks::<4>().0;
        let filtered = crate::raster::apply_float(source, self.w, self.h, settings, selection)?;
        let mut output =
            resources::global().allocate_now::<f32, _>(samples.len(), &storage::check)?;
        for (dst, src) in output
            .as_mut_slice()
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(filtered.iter())
        {
            *dst = *src;
        }
        Self::from_sample_allocation(
            self.w,
            self.h,
            self.native.as_ref().map_or(Depth::F32, |n| n.depth),
            Arc::new(output),
        )
    }
}

pub(crate) fn byte(v: f32) -> u8 {
    (v.clamp(0., 1.) * 255.).round() as u8
}

mod sample_bytes {
    use super::*;
    use base64::Engine;
    use serde::{Deserializer, Serializer};
    use std::io::{Read, Write};
    pub fn serialize<S: Serializer>(
        samples: &Arc<Allocation<f32>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let writer = storage::Builder::<u8>::new().map_err(serde::ser::Error::custom)?;
        let mut encoder =
            zstd::stream::write::Encoder::new(writer, 3).map_err(serde::ser::Error::custom)?;
        for chunk in samples.chunks(1024) {
            let mut bytes = [0u8; 4096];
            for (value, dst) in chunk.iter().zip(bytes.chunks_exact_mut(4)) {
                dst.copy_from_slice(&value.to_le_bytes());
            }
            encoder
                .write_all(&bytes[..chunk.len() * 4])
                .map_err(serde::ser::Error::custom)?;
        }
        let packed = encoder
            .finish()
            .map_err(serde::ser::Error::custom)?
            .finish()
            .map_err(serde::ser::Error::custom)?;
        let length = packed
            .len()
            .checked_add(2)
            .and_then(|n| (n / 3).checked_mul(4))
            .ok_or_else(|| serde::ser::Error::custom("Encoded samples overflow"))?;
        let _memory = resources::global()
            .reserve_now(length as u64, &storage::check)
            .map_err(serde::ser::Error::custom)?;
        serializer
            .serialize_str(&base64::engine::general_purpose::STANDARD.encode(packed.as_slice()))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Arc<Allocation<f32>>, D::Error> {
        struct Samples;
        impl<'de> serde::de::Visitor<'de> for Samples {
            type Value = Arc<Allocation<f32>>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a base64 sample string")
            }
            fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<Self::Value, E> {
                decode(text)
            }
        }
        deserializer.deserialize_str(Samples)
    }
    fn decode<E: serde::de::Error>(text: &str) -> Result<Arc<Allocation<f32>>, E> {
        let base64 = base64::read::DecoderReader::new(
            text.as_bytes(),
            &base64::engine::general_purpose::STANDARD,
        );
        let mut decoder =
            zstd::stream::read::Decoder::new(base64).map_err(serde::de::Error::custom)?;
        let _window = resources::global()
            .reserve_now(128 * 1024 * 1024, &storage::check)
            .map_err(serde::de::Error::custom)?;
        decoder
            .window_log_max(27)
            .map_err(serde::de::Error::custom)?;
        let mut samples = storage::Builder::new().map_err(serde::de::Error::custom)?;
        let mut bytes = [0u8; 16384];
        let mut carried = 0;
        loop {
            let read = decoder
                .read(&mut bytes[carried..])
                .map_err(serde::de::Error::custom)?;
            if read == 0 {
                if carried != 0 {
                    return Err(serde::de::Error::custom("Truncated native sample"));
                }
                break;
            }
            let length = carried + read;
            let complete = length / 4 * 4;
            let mut values = [0f32; 4096];
            for (dst, src) in values.iter_mut().zip(bytes[..complete].chunks_exact(4)) {
                *dst = f32::from_le_bytes(src.try_into().unwrap());
                if !dst.is_finite() {
                    return Err(serde::de::Error::custom("Nonfinite native sample"));
                }
            }
            samples
                .extend(&values[..complete / 4])
                .map_err(serde::de::Error::custom)?;
            carried = length - complete;
            bytes.copy_within(complete..length, 0);
        }
        samples.finish().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests;

impl Pixels {
    /// Pad or crop a native grid without resampling.
    /// Args: new dimensions and old-grid offset. Returns a buffer preserving all copied samples.
    pub fn reframed(&self, w: u32, h: u32, dx: i64, dy: i64) -> Result<Self, String> {
        let count = (w as usize)
            .checked_mul(h as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or("Native reframe exceeds working limit")?;
        let mut out = resources::global().allocate_now::<f32, _>(count, &storage::check)?;
        for y in 0..self.h {
            let ny = y as i64 + dy;
            if ny < 0 || ny >= h as i64 {
                continue;
            }
            for x in 0..self.w {
                let nx = x as i64 + dx;
                if nx < 0 || nx >= w as i64 {
                    continue;
                }
                let i = (ny as usize * w as usize + nx as usize) * 4;
                out.as_mut_slice()[i..i + 4]
                    .copy_from_slice(&self.sample(y as usize * self.w as usize + x as usize));
            }
        }
        Self::from_sample_allocation(
            w,
            h,
            self.native.as_ref().map_or(Depth::U8, |n| n.depth),
            Arc::new(out),
        )
    }

    /// Paint an overlay into authored samples.
    /// Args: matching straight RGBA overlay and opacity. Returns an error for mismatched dimensions.
    pub fn paint_overlay(&mut self, overlay: &Pixels, opacity: f32) -> Result<(), String> {
        if (self.w, self.h) != (overlay.w, overlay.h) {
            return Err("Paint overlay dimensions mismatch".into());
        }
        let samples = storage::collect((0..self.w as usize * self.h as usize).flat_map(|i| {
            blend_pixel(
                self.sample(i),
                overlay.sample(i),
                crate::color::Blend::Normal,
                opacity,
            )
        }))?;
        self.set_sample_allocation(samples)
    }
}

impl Pixels {
    /// Validate authoritative samples after project loading.
    /// Args: self. Returns an error for inconsistent dimensions, excessive buffers or nonfinite values.
    pub fn validate_native(&self) -> Result<(), String> {
        let Some(native) = &self.native else {
            return Ok(());
        };
        let count = (self.w as usize)
            .checked_mul(self.h as usize)
            .filter(|&n| n > 0)
            .ok_or("Invalid native dimensions")?;
        if native.samples.len() != count * 4 || native.samples.iter().any(|s| !s.is_finite()) {
            return Err("Invalid native sample dimensions or values".into());
        }
        Ok(())
    }
    /// Read a display paint color in the untagged RGB working space.
    /// Args: brush color. Returns straight floating-point RGBA.
    pub fn paint_color(&self, color: crate::color::Rgba) -> Result<[f32; 4], String> {
        Ok([
            color.r as f32 / 255.,
            color.g as f32 / 255.,
            color.b as f32 / 255.,
            color.a as f32 / 255.,
        ])
    }
    /// Resample authored samples with premultiplied bilinear interpolation.
    /// Args: output dimensions. Returns native samples without an RGBA8 intermediate.
    pub fn resized(&self, w: u32, h: u32) -> Result<Self, String> {
        let count = (w as usize)
            .checked_mul(h as usize)
            .filter(|&n| n > 0)
            .ok_or("Resampled pixels exceed working limit")?;
        let samples = storage::collect((0..count).flat_map(|i| {
            composite::sample(
                self,
                ((i % w as usize) as f32 + 0.5) * self.w as f32 / w as f32,
                ((i / w as usize) as f32 + 0.5) * self.h as f32 / h as f32,
            )
        }))?;
        Self::from_sample_allocation(
            w,
            h,
            self.native.as_ref().map_or(Depth::U8, |n| n.depth),
            samples,
        )
    }
}

/// Sample authored pixels with premultiplied bilinear interpolation.
/// Args: pixels and source-grid coordinates. Returns straight RGBA.
pub fn sample_at(pixels: &Pixels, x: f32, y: f32) -> [f32; 4] {
    composite::sample(pixels, x, y)
}

/// Visit every authored raster or mask in the document.
/// Args: document. Returns layer pixels, layer masks and shape masks without copying them.
pub fn buffers(doc: &crate::document::Document) -> impl Iterator<Item = &Pixels> {
    doc.layers.iter().flat_map(|l| {
        l.kind.pixels().into_iter().chain(l.mask.iter()).chain(
            l.kind
                .shapes()
                .into_iter()
                .flatten()
                .filter_map(|s| s.mask.as_ref()),
        )
    })
}

/// Check whether an export needs the precise rendering route.
/// Args: document. Returns true when any authored pixel or mask has native samples.
pub fn has_native(doc: &crate::document::Document) -> bool {
    buffers(doc).any(|p| p.native.is_some())
}

/// Find the greatest authored sample width, including masks.
/// Args: document. Returns eight, sixteen or floating-point depth.
pub fn document_depth(doc: &crate::document::Document) -> Depth {
    buffers(doc)
        .filter_map(|p| p.native.as_ref().map(|n| n.depth))
        .max_by_key(|d| d.bits())
        .unwrap_or_default()
}
