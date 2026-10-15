//! High-precision RAW development. Preview pixels are derived; the decoded
//! 16-bit linear source is shared by full-resolution exports and never edited.
use super::*;
use crate::formats::raw::{Cancellation, resources};
use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::Arc;

#[derive(Debug)]
pub struct AdmittedImage {
    pub image: RgbaImage,
    pub(crate) memory: Arc<resources::Lease>,
}
impl AdmittedImage {
    /// Admit an already bounded derived preview into retained ownership.
    /// Args: validated RGBA preview. Returns its owner or an immediate pressure error.
    pub(crate) fn from_raster(image: RgbaImage) -> Result<Self, String> {
        if image.w == 0
            || image.h == 0
            || u64::from(image.w) * u64::from(image.h) * 4 != image.data.len() as u64
        {
            return Err("Invalid photo preview".into());
        }
        let memory = resources::global()
            .reserve_now(image.data.capacity() as u64, &Cancellation::default())?;
        Ok(Self { image, memory })
    }
}

#[derive(Debug)]
pub struct AdmittedSamples {
    pub width: u32,
    pub height: u32,
    pub samples: Vec<f32>,
    pub memory: Arc<resources::Lease>,
}
impl std::ops::Deref for AdmittedImage {
    type Target = RgbaImage;
    fn deref(&self) -> &Self::Target {
        &self.image
    }
}
impl PartialEq for AdmittedImage {
    fn eq(&self, other: &Self) -> bool {
        self.image == other.image
    }
}
impl PartialEq<RgbaImage> for AdmittedImage {
    fn eq(&self, other: &RgbaImage) -> bool {
        self.image == *other
    }
}
impl PartialEq<AdmittedImage> for RgbaImage {
    fn eq(&self, other: &AdmittedImage) -> bool {
        *self == other.image
    }
}

#[derive(Debug)]
pub struct EncodedPhoto {
    bytes: Vec<u8>,
    _memory: Arc<resources::Lease>,
}
impl std::ops::Deref for EncodedPhoto {
    type Target = Vec<u8>;
    fn deref(&self) -> &Self::Target {
        &self.bytes
    }
}
impl EncodedPhoto {
    /// Transfer encoded output into the shared immutable owner without copying or reserving again.
    /// Args: self. Returns the same byte allocation with its exclusive live lease.
    pub(crate) fn into_allocation(self) -> Result<Arc<resources::Allocation<u8>>, String> {
        resources::global().adopt(self.bytes, self._memory)
    }
    pub(crate) fn into_vec(self) -> Vec<u8> {
        self.bytes
    }
}

struct BoundedWriter<'a> {
    inner: Cursor<Vec<u8>>,
    limit: u64,
    progress: &'a dyn crate::ml::Progress,
}
impl std::io::Write for BoundedWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.progress.check().map_err(std::io::Error::other)?;
        let end = self
            .inner
            .position()
            .checked_add(bytes.len() as u64)
            .filter(|end| *end <= self.limit)
            .ok_or_else(|| {
                std::io::Error::other("Photo encoder exceeded its admitted output size")
            })?;
        let buffer = self.inner.get_mut();
        if end as usize > buffer.capacity() {
            buffer
                .try_reserve_exact(end as usize - buffer.len())
                .map_err(std::io::Error::other)?;
        }
        std::io::Write::write(&mut self.inner, bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl std::io::Seek for BoundedWriter<'_> {
    fn seek(&mut self, from: std::io::SeekFrom) -> std::io::Result<u64> {
        let position = std::io::Seek::seek(&mut self.inner, from)?;
        if position > self.limit {
            return Err(std::io::Error::other(
                "Photo encoder seek exceeded its admitted output size",
            ));
        }
        Ok(position)
    }
}

/// Account development's source-sized float buffers and output before allocating.
/// Args: source/output dimensions, output channel bytes, saved parameters and owner. Returns an operation lease.
fn development_memory(
    source: (u32, u32),
    output: (u32, u32),
    channel_bytes: u64,
    params: &DevelopParams,
    cancel: &Cancellation,
    now: bool,
) -> Result<Arc<resources::Lease>, String> {
    let count = u64::from(source.0)
        .checked_mul(u64::from(source.1))
        .ok_or("Photo dimensions overflow")?;
    let output = u64::from(output.0)
        .checked_mul(u64::from(output.1))
        .ok_or("Photo output dimensions overflow")?;
    let per_source = 12
        + if now { 6 } else { 0 }
        + if params.optics.active() { 4 } else { 0 }
        + if params.clarity.abs() > 0.001 { 12 } else { 0 };
    let demand = count
        .checked_mul(per_source)
        .and_then(|n| n.checked_add(output.checked_mul(4 * channel_bytes)?))
        .and_then(|n| {
            n.checked_add(if now {
                8 * 1024 * 1024
            } else {
                96 * 1024 * 1024
            })
        })
        .ok_or("Photo development memory demand overflow")?;
    if now {
        resources::global().reserve_now(demand, cancel)
    } else {
        resources::global().reserve(demand, cancel)
    }
}

#[derive(Clone, Debug)]
pub struct LinearImage {
    pub w: u32,
    pub h: u32,
    pub pixels: Arc<resources::Allocation<u16>>,
}

impl LinearImage {
    /// Resize linear samples using row-bounded separable triangle filtering.
    /// Args: validated source samples, preview edge and job owner. Returns RGB16 without a source-sized float intermediate.
    fn preview_cancellable(
        w: u32,
        h: u32,
        pixels: &[u16],
        max_edge: u32,
        cancel: &Cancellation,
    ) -> Result<Self, String> {
        cancel.check()?;
        if w == 0 || h == 0 || u64::from(w) * u64::from(h) * 3 != pixels.len() as u64 {
            return Err("Invalid linear preview source".into());
        }
        let scale = (max_edge.max(1) as f64 / w.max(h) as f64).min(1.);
        let (pw, ph) = (
            (w as f64 * scale).round().max(1.) as u32,
            (h as f64 * scale).round().max(1.) as u32,
        );
        let demand = u64::from(pw) * u64::from(ph) * 6
            + u64::from(w) * (12 * rayon::current_num_threads() as u64 + 32)
            + 8 * 1024 * 1024;
        let memory = resources::global().reserve_now(demand, cancel)?;
        let mut output = photo_buffer(pw as usize * ph as usize * 3)?;
        if (pw, ph) == (w, h) {
            output.copy_from_slice(pixels);
            memory.resize_now(output.capacity() as u64 * 2, cancel)?;
            return Ok(Self {
                w,
                h,
                pixels: resources::global().adopt(output, memory)?,
            });
        }
        let horizontal: Vec<_> = (0..pw).map(|x| triangle_weights(w, pw, x)).collect();
        output
            .par_chunks_mut(pw as usize * 3)
            .enumerate()
            .try_for_each(|(y, row)| {
                cancel.check()?;
                let (top, weights) = triangle_weights(h, ph, y as u32);
                let mut vertical: Vec<f32> = photo_buffer(w as usize * 3)?;
                for x in 0..w as usize {
                    if x % 1024 == 0 {
                        cancel.check()?;
                    }
                    for (offset, weight) in weights.iter().enumerate() {
                        let at = ((top + offset) * w as usize + x) * 3;
                        for c in 0..3 {
                            vertical[x * 3 + c] += f32::from(pixels[at + c]) * weight;
                        }
                    }
                }
                for (target, (left, weights)) in row.chunks_exact_mut(3).zip(&horizontal) {
                    for c in 0..3 {
                        let value = weights
                            .iter()
                            .enumerate()
                            .map(|(offset, weight)| vertical[(left + offset) * 3 + c] * weight)
                            .sum::<f32>();
                        target[c] = value.round().clamp(0., 65535.) as u16;
                    }
                }
                Ok::<_, String>(())
            })?;
        memory.resize_now(output.capacity() as u64 * 2, cancel)?;
        Ok(Self {
            w: pw,
            h: ph,
            pixels: resources::global().adopt(output, memory)?,
        })
    }
}

fn triangle_weights(source: u32, target: u32, index: u32) -> (usize, Vec<f32>) {
    let ratio = source as f32 / target as f32;
    let radius = ratio.max(1.);
    let center = (index as f32 + 0.5) * ratio;
    let start = ((center - radius).floor() as i64).clamp(0, i64::from(source) - 1) as usize;
    let end = ((center + radius).ceil() as i64).clamp(start as i64 + 1, i64::from(source)) as usize;
    let center = center - 0.5;
    let mut weights: Vec<_> = (start..end)
        .map(|i| (1. - ((i as f32 - center) / radius).abs()).max(0.))
        .collect();
    let sum = weights.iter().sum::<f32>();
    for weight in &mut weights {
        *weight /= sum;
    }
    (start, weights)
}

impl PhotoImage {
    pub fn from_raw(name: String, raw: crate::formats::raw::RawImage) -> Result<Self, String> {
        Self::from_raw_cancellable(name, raw, &Cancellation::default())
    }
    fn from_raw_cancellable(
        name: String,
        mut raw: crate::formats::raw::RawImage,
        cancel: &Cancellation,
    ) -> Result<Self, String> {
        raw.admit_memory(cancel)?;
        let linear =
            LinearImage::preview_cancellable(raw.width, raw.height, &raw.pixels, 1600, cancel)?;
        let preview_memory = resources::global().reserve(
            u64::from(linear.w) * u64::from(linear.h) * 20 + 8 * 1024 * 1024,
            cancel,
        )?;
        let baseline = DevelopParams {
            exposure: raw.metadata.baseline_exposure,
            ..Default::default()
        };
        let (w, h, data) = develop_checked::<u8>(
            DevelopSource::linear(linear.w, linear.h, &linear.pixels),
            &baseline,
            cancel,
        )?;
        let preview = Arc::new(RgbaImage { w, h, data });
        preview_memory.resize_now(preview.data.capacity() as u64, cancel)?;
        resources::global().retain(&preview, preview_memory);
        let thumb_memory = resources::global().reserve_now(192 * 192 * 4, cancel)?;
        let thumb = Arc::new(preview.downscaled(192));
        thumb_memory.resize_now(thumb.data.capacity() as u64, cancel)?;
        resources::global().retain(&thumb, thumb_memory);
        Ok(Self {
            name,
            full: preview.clone(),
            preview,
            thumb,
            develop: DevelopParams::default(),
            notes: raw.warnings.clone(),
            raw: Some(Arc::new(raw)),
            raw_preview: Some(Arc::new(linear)),
            source: None,
            source_identity: None,
            settings_path: None,
        })
    }

    /// Decode on a worker, then restore settings associated with this exact source.
    pub fn load(path: &Path) -> Result<Self, String> {
        Self::load_cancellable(path, &crate::formats::raw::Cancellation::default())
    }

    /// Open a source and sidecar while honoring the owning job cancellation.
    /// Args: original/settings path and token. Returns the restored photo only while its job remains current.
    pub fn load_cancellable(
        path: &Path,
        cancel: &crate::formats::raw::Cancellation,
    ) -> Result<Self, String> {
        cancel.check()?;
        let settings = if edits::is_sidecar(path) {
            Some(
                path.canonicalize()
                    .map_err(|error| format!("Could not open photo settings: {error}"))?,
            )
        } else {
            None
        };
        let source = if let Some(settings) = &settings {
            edits::source_path(settings)?
        } else {
            path.to_owned()
        };
        let path = source
            .canonicalize()
            .map_err(|e| format!("Could not open photo: {e}"))?;
        let identity = edits::SourceIdentity::read(&path)?;
        let explicit = settings
            .as_ref()
            .map(|settings| edits::load_file(settings, &identity))
            .transpose()?;
        let mut file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        let is_raw = crate::formats::raw::is_extension(
            path.extension().and_then(|s| s.to_str()).unwrap_or(""),
        );
        if !is_raw && file.metadata().map_err(|e| e.to_string())?.len() > 512 * 1024 * 1024 {
            return Err("Choose a photo smaller than 512 MiB".into());
        }
        let input_memory =
            crate::formats::raw::resources::global().reserve(identity.size_bytes, cancel)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(identity.size_bytes as usize)
            .map_err(|_| "Not enough memory to read the photo")?;
        let mut block = [0u8; 256 * 1024];
        loop {
            cancel.check()?;
            let count = file.read(&mut block).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            if bytes.len() as u64 + count as u64 > identity.size_bytes {
                return Err("The photo changed while it was opening. Open it again.".into());
            }
            bytes.extend_from_slice(&block[..count]);
        }
        if bytes.len() as u64 != identity.size_bytes {
            return Err("The photo changed while it was opening. Open it again.".into());
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        let mut photo = if crate::formats::raw::is_extension(extension) {
            Self::from_raw_cancellable(
                name,
                crate::formats::raw::decode_reserved(&bytes, cancel, &input_memory)?,
                cancel,
            )?
        } else {
            let pixels = crate::formats::decode_image(&bytes)?;
            Self::from_full(
                name,
                RgbaImage {
                    w: pixels.w,
                    h: pixels.h,
                    data: pixels.data.to_vec(),
                },
            )
        };
        cancel.check()?;
        if edits::SourceIdentity::read(&path)? != identity {
            return Err("The photo changed while it was opening. Open it again.".into());
        }
        if let Some(params) = explicit {
            photo.develop = params;
        } else {
            match edits::load(&path, &identity) {
                Ok(Some(params)) => photo.develop = params,
                Ok(None) => {}
                Err(error) => photo
                    .notes
                    .push(format!("Saved photo settings were not loaded: {error}")),
            }
        }
        photo.settings_path = Some(settings.unwrap_or_else(|| edits::sidecar_path(&path)));
        photo.source = Some(path);
        photo.source_identity = Some(identity);
        cancel.check()?;
        Ok(photo)
    }

    pub fn dimensions(&self) -> (u32, u32) {
        self.raw
            .as_ref()
            .map_or((self.full.w, self.full.h), |r| (r.width, r.height))
    }

    fn raw_params(&self) -> DevelopParams {
        let mut params = self.develop.clone();
        if let Some(raw) = &self.raw {
            params.exposure += raw.metadata.baseline_exposure;
        }
        params
    }

    pub fn render_preview(&self) -> Result<AdmittedImage, String> {
        let cancel = Cancellation::default();
        let params = self.raw_params();
        let dims = self
            .raw_preview
            .as_ref()
            .map_or((self.preview.w, self.preview.h), |raw| (raw.w, raw.h));
        let memory = development_memory(
            dims,
            params.output_dim(dims.0, dims.1),
            1,
            &params,
            &cancel,
            true,
        )?;
        let (w, h, data) = if let Some(raw) = &self.raw_preview {
            develop_checked::<u8>(
                DevelopSource::linear(raw.w, raw.h, &raw.pixels),
                &params,
                &cancel,
            )?
        } else {
            develop_checked::<u8>(DevelopSource::rgba(&self.preview), &params, &cancel)?
        };
        memory.resize_now(data.capacity() as u64, &cancel)?;
        Ok(AdmittedImage {
            image: RgbaImage { w, h, data },
            memory,
        })
    }

    /// Render a small edited preview with retained, nonblocking memory ownership.
    /// Args: maximum edge. Returns derived RGBA8 or an immediate pressure error; original samples stay unchanged.
    pub fn render_thumbnail(&self, max_edge: u32) -> Result<AdmittedImage, String> {
        let cancel = Cancellation::default();
        let params = self.raw_params();
        let source = self
            .raw_preview
            .as_ref()
            .map_or((self.preview.w, self.preview.h), |raw| (raw.w, raw.h));
        let scale = (max_edge.max(1) as f64 / f64::from(source.0.max(source.1))).min(1.);
        let dims = (
            (f64::from(source.0) * scale).round().max(1.) as u32,
            (f64::from(source.1) * scale).round().max(1.) as u32,
        );
        let memory = development_memory(
            dims,
            params.output_dim(dims.0, dims.1),
            1,
            &params,
            &cancel,
            true,
        )?;
        let (w, h, data) = if let Some(raw) = &self.raw_preview {
            let linear =
                LinearImage::preview_cancellable(raw.w, raw.h, &raw.pixels, max_edge, &cancel)?;
            develop_checked::<u8>(
                DevelopSource::linear(linear.w, linear.h, &linear.pixels),
                &params,
                &cancel,
            )?
        } else {
            let preview = self.preview.downscaled(max_edge.max(1));
            develop_checked::<u8>(DevelopSource::rgba(&preview), &params, &cancel)?
        };
        memory.resize_now(data.capacity() as u64, &cancel)?;
        Ok(AdmittedImage {
            image: RgbaImage { w, h, data },
            memory,
        })
    }

    pub fn render_full(&self) -> Result<AdmittedImage, String> {
        self.render_full_cancellable(&Cancellation::default())
    }

    /// Develop authoritative straight encoded sRGB samples without integer quantization.
    /// Args: current placement/export job token. Returns F32 RGBA, dimensions and its live resource lease.
    pub fn developed_f32(&self, cancel: &Cancellation) -> Result<AdmittedSamples, String> {
        let params = self.raw_params();
        let dimensions = self.dimensions();
        let memory = development_memory(
            dimensions,
            params.output_dim(dimensions.0, dimensions.1),
            4,
            &params,
            cancel,
            false,
        )?;
        let (width, height, samples) = if let Some(raw) = &self.raw {
            develop_checked::<f32>(
                DevelopSource::linear(raw.width, raw.height, &raw.pixels),
                &params,
                cancel,
            )?
        } else {
            develop_checked::<f32>(DevelopSource::rgba(&self.full), &params, cancel)?
        };
        memory.resize(samples.capacity() as u64 * 4, cancel)?;
        Ok(AdmittedSamples {
            width,
            height,
            samples,
            memory,
        })
    }

    /// Develop source-sized pixels while retaining their actual resource ownership.
    /// Args: current detail/export job token. Returns straight RGBA8 for display and its live output/cache lease.
    pub fn render_full_cancellable(&self, cancel: &Cancellation) -> Result<AdmittedImage, String> {
        let params = self.raw_params();
        let dimensions = self.dimensions();
        let memory = development_memory(
            dimensions,
            params.output_dim(dimensions.0, dimensions.1),
            1,
            &params,
            cancel,
            false,
        )?;
        let (w, h, data) = if let Some(raw) = &self.raw {
            develop_checked::<u8>(
                DevelopSource::linear(raw.width, raw.height, &raw.pixels),
                &params,
                cancel,
            )?
        } else {
            develop_checked::<u8>(DevelopSource::rgba(&self.full), &params, cancel)?
        };
        memory.resize(data.capacity() as u64, cancel)?;
        Ok(AdmittedImage {
            image: RgbaImage { w, h, data },
            memory,
        })
    }

    /// PNG/TIFF retain sixteen-bit channels for RAW; JPEG is an eight-bit delivery image.
    pub fn export_bytes(&self, extension: &str) -> Result<EncodedPhoto, String> {
        self.export_bytes_cancellable(extension, &Cancellation::default())
    }

    /// Encode full precision camera development within shared buffer admission.
    /// Args: delivery extension and owning operation token. Returns encoded bytes with their allocation lease.
    pub fn export_bytes_cancellable(
        &self,
        extension: &str,
        cancel: &Cancellation,
    ) -> Result<EncodedPhoto, String> {
        let extension = extension.to_ascii_lowercase();
        let format = match extension.as_str() {
            "png" => image::ImageFormat::Png,
            "tif" | "tiff" => image::ImageFormat::Tiff,
            "jpg" | "jpeg" => image::ImageFormat::Jpeg,
            _ => return Err("Export the developed photo as PNG, TIFF or JPEG".into()),
        };
        let params = self.raw_params();
        let dimensions = self.dimensions();
        let output = params.output_dim(dimensions.0, dimensions.1);
        let count = u64::from(output.0)
            .checked_mul(u64::from(output.1))
            .ok_or("Photo output dimensions overflow")?;
        let limit = count
            .checked_mul(9)
            .and_then(|n| n.checked_add(1024 * 1024))
            .ok_or("Photo encoded size overflow")?;
        let memory = development_memory(dimensions, output, 2, &params, cancel, false)?;
        memory.resize(
            memory
                .bytes()
                .checked_add(limit.checked_mul(2).ok_or("Photo encoded size overflow")?)
                .ok_or("Photo memory demand overflow")?,
            cancel,
        )?;
        let mut bytes = BoundedWriter {
            inner: Cursor::new(Vec::new()),
            limit,
            progress: cancel,
        };
        if let Some(raw) = &self.raw
            && format != image::ImageFormat::Jpeg
        {
            let (w, h, data) = develop_checked::<u16>(
                DevelopSource::linear(raw.width, raw.height, &raw.pixels),
                &params,
                cancel,
            )?;
            let rgba = image::ImageBuffer::<image::Rgba<u16>, _>::from_raw(w, h, data)
                .ok_or("Invalid developed pixels")?;
            image::DynamicImage::ImageRgba16(rgba)
                .write_to(&mut bytes, format)
                .map_err(|e| e.to_string())?;
        } else {
            let (w, h, data) = if let Some(raw) = &self.raw {
                develop_checked::<u8>(
                    DevelopSource::linear(raw.width, raw.height, &raw.pixels),
                    &params,
                    cancel,
                )?
            } else {
                develop_checked::<u8>(DevelopSource::rgba(&self.full), &params, cancel)?
            };
            let out = RgbaImage { w, h, data };
            if format == image::ImageFormat::Jpeg {
                let mut rgb = photo_buffer(out.w as usize * out.h as usize * 3)?;
                for (index, (pixel, target)) in out
                    .data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(rgb.chunks_exact_mut(3))
                    .enumerate()
                {
                    if index % 4096 == 0 {
                        cancel.check()?;
                    }
                    let alpha = u32::from(pixel[3]);
                    for (channel, out) in pixel[..3].iter().zip(target) {
                        *out = ((u32::from(*channel) * alpha + 127) / 255 + 255 - alpha) as u8;
                    }
                }
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 95)
                    .encode(&rgb, out.w, out.h, image::ExtendedColorType::Rgb8)
                    .map_err(|e| e.to_string())?;
            } else {
                let rgba = image::RgbaImage::from_raw(out.w, out.h, out.data)
                    .ok_or("Invalid photo pixels")?;
                image::DynamicImage::ImageRgba8(rgba)
                    .write_to(&mut bytes, format)
                    .map_err(|e| e.to_string())?;
            }
        }
        cancel.check()?;
        let bytes = bytes.inner.into_inner();
        memory.resize(bytes.capacity() as u64, cancel)?;
        Ok(EncodedPhoto {
            bytes,
            _memory: memory,
        })
    }

    pub fn export_to(&self, path: &Path) -> Result<(), String> {
        if let Some(source) = &self.source
            && path.canonicalize().is_ok_and(|output| output == *source)
        {
            return Err("Choose a separate export file to preserve the camera original".into());
        }
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        crate::formats::write_atomic(path, &self.export_bytes(extension)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detached_raw_preview_levels_remain_admitted_after_photo_and_source_drop() {
        let raw =
            crate::formats::raw::decode(include_bytes!("../formats/raw/synthetic.dng")).unwrap();
        let source = Arc::downgrade(&raw.pixels);
        let photo = PhotoImage::from_raw("detached previews".into(), raw).unwrap();
        let linear = photo.raw_preview.as_ref().unwrap().clone();
        let samples = linear.pixels.clone();
        let preview = photo.preview.clone();
        let thumb = photo.thumb.clone();
        let linear_lease = Arc::downgrade(samples.lease());
        let preview_lease = Arc::downgrade(&resources::global().retained(&preview).unwrap());
        let thumb_lease = Arc::downgrade(&resources::global().retained(&thumb).unwrap());
        drop(photo);
        resources::global().usage();
        assert!(source.upgrade().is_none());
        assert_eq!(linear_lease.upgrade().unwrap().bytes(), samples.bytes());
        assert_eq!(
            preview_lease.upgrade().unwrap().bytes(),
            preview.data.capacity() as u64
        );
        assert_eq!(
            thumb_lease.upgrade().unwrap().bytes(),
            thumb.data.capacity() as u64
        );
        drop(linear);
        assert!(linear_lease.upgrade().is_some());
        drop(samples);
        drop(preview);
        drop(thumb);
        resources::global().usage();
        assert!(linear_lease.upgrade().is_none());
        assert!(preview_lease.upgrade().is_none());
        assert!(thumb_lease.upgrade().is_none());
    }

    #[test]
    fn queued_thumbnail_retains_admission_until_its_last_consumer_drops() {
        let photo = PhotoImage::from_full(
            "queued".into(),
            RgbaImage {
                w: 64,
                h: 48,
                data: vec![100; 64 * 48 * 4],
            },
        );
        let thumbnail = photo.render_thumbnail(32).unwrap();
        let lease = Arc::downgrade(&thumbnail.memory);
        let (tx, rx) = std::sync::mpsc::sync_channel(2);
        tx.send(thumbnail).unwrap();
        drop(photo);
        assert!(lease.upgrade().is_some());
        let received = rx.recv().unwrap();
        assert_eq!((received.w, received.h), (32, 24));
        assert_eq!(
            received.memory.bytes(),
            received.image.data.capacity() as u64
        );
        drop(received);
        assert!(lease.upgrade().is_none());
    }

    #[test]
    fn bounded_linear_preview_matches_triangle_reference_at_multiple_sizes() {
        for (w, h, edge) in [(257, 193, 73), (193, 257, 111), (73, 51, 99)] {
            let pixels: Vec<u16> = (0..w * h * 3)
                .map(|i| ((i * 193 + 17) % 65536) as u16)
                .collect();
            let preview =
                LinearImage::preview_cancellable(w, h, &pixels, edge, &Cancellation::default())
                    .unwrap();
            let input = image::ImageBuffer::<image::Rgb<u16>, _>::from_raw(w, h, pixels.as_slice())
                .unwrap();
            let reference = image::imageops::resize(
                &input,
                preview.w,
                preview.h,
                image::imageops::FilterType::Triangle,
            );
            assert!(
                preview
                    .pixels
                    .iter()
                    .zip(reference.as_raw())
                    .all(|(a, b)| a.abs_diff(*b) <= 1)
            );
        }
        let cancel = Cancellation::default();
        cancel.cancel();
        assert!(LinearImage::preview_cancellable(1, 1, &[1, 2, 3], 1, &cancel).is_err());
    }

    #[test]
    fn f32_development_keeps_transfer_precision_straight_alpha_and_immutable_source() {
        let values = Arc::new(vec![10000, 10001, 10002, 20000, 20001, 20002]);
        let raw = crate::formats::raw::RawImage {
            width: 2,
            height: 1,
            pixels: values.clone(),
            memory: None,
            metadata: Default::default(),
            warnings: vec![],
        };
        let photo = PhotoImage::from_raw("linear".into(), raw).unwrap();
        let native = photo.developed_f32(&Cancellation::default()).unwrap();
        assert_eq!((native.width, native.height), (2, 1));
        for (pixel, source) in native.samples.chunks_exact(4).zip(values.chunks_exact(3)) {
            for c in 0..3 {
                let x = f64::from(source[c]) / 65535.;
                let expected = if x <= 0.0031308 {
                    12.92 * x
                } else {
                    1.055 * x.powf(1. / 2.4) - 0.055
                };
                assert!((f64::from(pixel[c]) - expected).abs() < 0.000001);
            }
            assert_eq!(pixel[3], 1.);
        }
        assert!(native.samples[1] > native.samples[0]);
        assert!(
            native
                .samples
                .iter()
                .any(|sample| (sample - (sample * 65535.).round() / 65535.).abs() > 0.000001)
        );
        assert!(Arc::ptr_eq(&photo.raw.as_ref().unwrap().pixels, &values));
        assert_eq!(
            values.as_slice(),
            [10000, 10001, 10002, 20000, 20001, 20002]
        );
        let full = Arc::new(photo.render_full().unwrap());
        let lease = Arc::downgrade(&full.memory);
        let cache = full.clone();
        drop(full);
        assert!(lease.upgrade().is_some());
        drop(cache);
        assert!(lease.upgrade().is_none());
    }

    #[test]
    fn bounded_photo_writer_rejects_excess_before_allocation_and_obeys_cancel() {
        use std::io::Write;
        let cancel = Cancellation::default();
        let mut writer = BoundedWriter {
            inner: Cursor::new(Vec::new()),
            limit: 16,
            progress: &cancel,
        };
        assert!(writer.write_all(&[0u8; 17]).is_err());
        assert_eq!(writer.inner.get_ref().capacity(), 0);
        writer.write_all(&[1u8; 8]).unwrap();
        cancel.cancel();
        assert!(writer.write_all(&[2u8; 8]).is_err());
        assert_eq!(writer.inner.into_inner(), vec![1; 8]);
    }

    #[test]
    fn raw_exposure_uses_linear_samples_and_preserves_sub_byte_detail() {
        let pixels = [10000, 10001, 10002, 20000, 20001, 20002];
        let source = DevelopSource::linear(2, 1, &pixels);
        let params = DevelopParams {
            exposure: 1.,
            ..Default::default()
        };
        let (_, _, out) = develop_inner::<u16>(source, &params);
        assert!(
            out[1] > out[0] && out[2] > out[1],
            "16-bit output lost low-order source detail"
        );
        assert_eq!(
            out[0],
            (srgb_encode(20000. / 65535.) * 65535.).round() as u16
        );
        let (_, _, baseline) = develop_inner::<u16>(source, &DevelopParams::default());
        assert_eq!(
            baseline[0],
            (srgb_encode(10000. / 65535.) * 65535.).round() as u16
        );
        assert_eq!(pixels, [10000, 10001, 10002, 20000, 20001, 20002]);
    }

    #[test]
    fn sixteen_bit_crop_rotation_uses_source_dimensions() {
        let pixels = [
            1000, 1000, 1000, 2000, 2000, 2000, 3000, 3000, 3000, 4000, 4000, 4000, 5000, 5000,
            5000, 6000, 6000, 6000,
        ];
        let params = DevelopParams {
            rotate: 90,
            crop: Some([0., 0., 1., 0.5]),
            ..Default::default()
        };
        let (w, h, pixels) = develop_inner::<u16>(DevelopSource::linear(3, 2, &pixels), &params);
        assert_eq!((w, h), (2, 2));
        assert!(pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 65535));
    }
}
