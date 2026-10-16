//! Still-image export pipeline, shared by the dialog and real-model QA.
#[cfg(test)]
mod ownership_tests;
mod photo;
pub mod target;

/// Immutable encoded bytes whose shared admission lives through the final consumer.
pub type EncodedBytes = std::sync::Arc<crate::resources::Allocation<u8>>;
use crate::{document::Document, ml::Progress, photo::PhotoImage, upscale};
use image::{DynamicImage, RgbaImage};
use std::{io::Write, path::Path};

#[derive(Clone)]
pub enum Source {
    Document(Document),
    Photo(PhotoImage),
}
impl Source {
    pub fn dimensions(&self) -> (u32, u32) {
        match self {
            Self::Document(doc) => (
                doc.width.round().max(1.) as u32,
                doc.height.round().max(1.) as u32,
            ),
            Self::Photo(photo) => {
                let (w, h) = photo.dimensions();
                photo.develop.output_dim(w, h)
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Format {
    Png,
    Jpeg,
    Tiff,
    Svg,
    Psd,
    Psb,
    Pdf,
    Ora,
    Idml,
}
impl Format {
    pub const ALL: [Self; 9] = [
        Self::Png,
        Self::Jpeg,
        Self::Tiff,
        Self::Svg,
        Self::Psd,
        Self::Psb,
        Self::Pdf,
        Self::Ora,
        Self::Idml,
    ];
    pub fn raster(self) -> bool {
        matches!(self, Self::Png | Self::Jpeg | Self::Tiff)
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Tiff => "tif",
            Self::Svg => "svg",
            Self::Psd => "psd",
            Self::Psb => "psb",
            Self::Pdf => "pdf",
            Self::Ora => "ora",
            Self::Idml => "idml",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Tiff => "TIFF",
            Self::Svg => "SVG · vector",
            Self::Psd => "PSD · layered",
            Self::Psb => "PSB · layered",
            Self::Pdf => "PDF · layered",
            Self::Ora => "OpenRaster · layered",
            Self::Idml => "IDML · publishing",
        }
    }
}
#[derive(Clone, Copy)]
pub struct Settings {
    pub format: Format,
    pub scale: u32,
    pub ai: Option<upscale::Settings>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            format: Format::Png,
            scale: 1,
            ai: None,
        }
    }
}
pub fn dimensions(source: &Source, settings: Settings) -> Result<(u32, u32), String> {
    let (w, h) = source.dimensions();
    if !settings.format.raster() {
        return Ok((w, h));
    }
    if !(1..=8).contains(&settings.scale) {
        return Err("Choose a render scale between 1× and 8×".into());
    }
    if let Source::Photo(photo) = source {
        return photo::dimensions(photo, settings);
    }
    let (w, h) = if let Source::Document(doc) = source {
        // Match compositor rounding for fractional document dimensions.
        (
            (doc.width * settings.scale as f32).round().max(1.) as u32,
            (doc.height * settings.scale as f32).round().max(1.) as u32,
        )
    } else {
        upscale::dimensions(w, h, settings.scale as f64)?
    };
    if matches!(source, Source::Document(doc) if crate::deep_color::has_native(doc))
        && settings.ai.is_none()
    {
        crate::deep_color::storage::count(w, h, 4)?;
        return Ok((w, h));
    }
    upscale::dimensions(w, h, settings.ai.map_or(1., |s| s.factor))
}

/// Export legacy transferred bytes without a retained admission owner.
/// Args: source, settings and progress. Returns unleased bytes; use encode_admitted for retained output.
pub fn encode(
    source: &Source,
    settings: Settings,
    progress: &dyn Progress,
) -> Result<(Vec<u8>, Vec<String>), String> {
    let (bytes, notes) = encode_admitted(source, settings, progress)?;
    let bytes = std::sync::Arc::try_unwrap(bytes).map_err(|_| "Shared legacy export output")?;
    let (bytes, _memory) = bytes.into_parts();
    Ok((bytes, notes))
}

/// Encode output while retaining its shared allocation through the consumer's final write.
/// Args: immutable source, export settings and cancellation-aware progress. Returns admitted bytes and notes.
pub fn encode_admitted(
    source: &Source,
    settings: Settings,
    progress: &dyn Progress,
) -> Result<(EncodedBytes, Vec<String>), String> {
    crate::formats::raw::Cancellation::with_progress(progress, |cancel| {
        encode_inner_admitted(source, settings, progress, cancel)
    })
}
fn encode_inner_admitted(
    source: &Source,
    settings: Settings,
    progress: &dyn Progress,
    cancel: &crate::formats::raw::Cancellation,
) -> Result<(EncodedBytes, Vec<String>), String> {
    dimensions(source, settings)?;
    progress.check()?;
    cancel.check()?;
    if let Source::Photo(photo) = source {
        return photo::encode_admitted(photo, settings, progress, cancel);
    }
    progress.report("Rendering export", 0, 1);
    if !settings.format.raster() {
        if let Source::Document(doc) = source
            && matches!(
                settings.format,
                Format::Svg | Format::Pdf | Format::Ora | Format::Idml
            )
            && crate::deep_color::has_native(doc)
        {
            return Err("This exchange route cannot preserve native sample precision yet. Export .oma, PSD, PSB, TIFF or 16-bit RGB PNG, or convert an explicit document copy to 8-bit.".into());
        }
        if settings.ai.is_some() {
            return Err("AI upscaling is available for PNG, TIFF and JPEG".into());
        }
        let Source::Document(doc) = source else {
            return Err("Photos export as PNG, TIFF or JPEG".into());
        };
        if matches!(settings.format, Format::Psd | Format::Psb) {
            let output = crate::formats::psd::encode_admitted(doc, settings.format == Format::Psb)?;
            progress.check()?;
            cancel.check()?;
            return Ok((output.bytes, output.warnings));
        }
        let output = match settings.format {
            Format::Svg => (crate::svg::export(doc)?.into_bytes(), vec![]),
            Format::Pdf => crate::formats::pdf::write(doc)?,
            Format::Ora => crate::formats::openraster::write(doc)?,
            Format::Idml => crate::formats::idml::write(doc)?,
            _ => unreachable!(),
        };
        progress.check()?;
        cancel.check()?;
        return Ok((crate::deep_color::storage::adopt(output.0)?, output.1));
    }
    if let Source::Document(doc) = source
        && doc.color.model == crate::deep_color::cms::Model::Cmyk
    {
        if settings.format != Format::Tiff || settings.ai.is_some() {
            return Err("CMYK authoring exports as PSD, PSB or TIFF. Convert the document to RGB explicitly for PNG/JPEG.".into());
        }
        let mut pixels = crate::deep_color::composite_with_cancel(doc, settings.scale, cancel)?;
        pixels.native.as_mut().unwrap().depth = crate::deep_color::document_depth(doc);
        let bytes = crate::deep_color::tiff::encode_admitted(&pixels)?;
        progress.check()?;
        cancel.check()?;
        return Ok((bytes, vec![]));
    }
    let _native_memory;
    let _ai_memory;
    let mut raster = match source {
        Source::Photo(_) => unreachable!("Photo sources dispatch before document encoding"),
        Source::Document(doc) if crate::deep_color::has_native(doc) => {
            if settings.ai.is_some() || settings.format == Format::Jpeg {
                return Err("This export would reduce native document precision. Export PNG, TIFF or PSD without AI upscaling.".into());
            }
            let mut pixels = crate::deep_color::composite_with_cancel(doc, settings.scale, cancel)?;
            let depth = crate::deep_color::document_depth(doc);
            if depth == crate::deep_color::Depth::F32 && settings.format != Format::Tiff {
                return Err(
                    "Floating-point export requires TIFF or PSD to retain float samples".into(),
                );
            }
            let (w, h) = (pixels.w, pixels.h);
            let samples = pixels
                .native
                .take()
                .ok_or("Missing native export samples")?
                .samples;
            drop(pixels);
            if depth == crate::deep_color::Depth::F32 {
                let samples = std::sync::Arc::try_unwrap(samples)
                    .map_err(|_| "Native export unexpectedly shares working samples")?;
                let (data, memory) = samples.into_parts();
                _native_memory = Some(memory);
                DynamicImage::ImageRgba32F(
                    image::ImageBuffer::from_raw(w, h, data).ok_or("Invalid float export")?,
                )
            } else {
                let mut data = crate::resources::global()
                    .allocate_now::<u16, _>(samples.len(), &crate::deep_color::storage::check)?;
                for (dst, &src) in data.as_mut_slice().iter_mut().zip(samples.iter()) {
                    *dst = (src.clamp(0., 1.) * 65535.).round() as u16;
                }
                let (data, memory) = data.into_parts();
                _native_memory = Some(memory);
                DynamicImage::ImageRgba16(
                    image::ImageBuffer::from_raw(w, h, data).ok_or("Invalid native export")?,
                )
            }
        }
        Source::Document(doc) => {
            // The compositor returns premultiplied RGBA; models and encoders
            // require straight RGB, especially around translucent artwork.
            let (w, h) = dimensions(source, settings)?;
            let _surface_memory = crate::resources::global().reserve_now(
                crate::deep_color::storage::count(w, h, 4)? as u64,
                &crate::deep_color::storage::check,
            )?;
            let pm = crate::compositor::render_export(doc, settings.scale)?;
            let bytes = crate::deep_color::storage::collect(pm.pixels().iter().flat_map(|p| {
                let p = p.demultiply();
                [p.red(), p.green(), p.blue(), p.alpha()]
            }))?;
            let bytes =
                std::sync::Arc::try_unwrap(bytes).map_err(|_| "Shared export raster storage")?;
            let (bytes, memory) = bytes.into_parts();
            _native_memory = Some(memory);
            DynamicImage::ImageRgba8(
                RgbaImage::from_raw(pm.width(), pm.height(), bytes)
                    .ok_or("Invalid export raster")?,
            )
        }
    };
    progress.check()?;
    if let Some(settings) = settings.ai {
        let (w, h) = upscale::dimensions(raster.width(), raster.height(), settings.factor)?;
        _ai_memory = Some(crate::resources::global().reserve_now(
            crate::deep_color::storage::count(w, h, 4)? as u64,
            &crate::deep_color::storage::check,
        )?);
        raster =
            DynamicImage::ImageRgba8(upscale::upscale(&raster.into_rgba8(), settings, progress)?);
    }
    progress.report("Encoding export", 0, 1);
    let mut bytes = EncodedWriter {
        encoded: crate::deep_color::storage::Encoded::new()?,
        progress,
        cancel,
    };
    if settings.format == Format::Jpeg {
        let _rgb_memory = crate::resources::global().reserve_now(
            crate::deep_color::storage::count(raster.width(), raster.height(), 3)? as u64,
            &crate::deep_color::storage::check,
        )?;
        let mut rgba = raster.into_rgba8();
        for p in rgba.pixels_mut() {
            let a = p[3] as u32;
            for c in 0..3 {
                p[c] = ((p[c] as u32 * a + 127) / 255 + 255 - a) as u8;
            }
            p[3] = 255;
        }
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 95)
            .encode_image(&DynamicImage::ImageRgba8(rgba).into_rgb8())
            .map_err(|e| e.to_string())?;
    } else if let Source::Document(doc) = source
        && crate::deep_color::has_native(doc)
    {
        use image::ImageEncoder;
        let _profile_memory = crate::resources::global().reserve_now(
            doc.color.icc_profile.len() as u64,
            &crate::deep_color::storage::check,
        )?;
        if settings.format == Format::Png {
            let mut encoder = image::codecs::png::PngEncoder::new(&mut bytes);
            if !doc.color.icc_profile.is_empty() {
                encoder
                    .set_icc_profile(doc.color.icc_profile.clone())
                    .map_err(|e| e.to_string())?;
            }
            encoder
                .write_image(
                    raster.as_bytes(),
                    raster.width(),
                    raster.height(),
                    raster.color().into(),
                )
                .map_err(|e| e.to_string())?;
        } else {
            crate::deep_color::tiff::write_rgb(&mut bytes, &raster, &doc.color.icc_profile)?;
        }
    } else {
        raster
            .write_to(
                &mut bytes,
                if settings.format == Format::Png {
                    image::ImageFormat::Png
                } else {
                    image::ImageFormat::Tiff
                },
            )
            .map_err(|e| e.to_string())?;
    }
    progress.check()?;
    cancel.check()?;
    Ok((bytes.encoded.finish()?, vec![]))
}

pub fn save(
    source: &Source,
    settings: Settings,
    path: &Path,
    progress: &dyn Progress,
) -> Result<Vec<String>, String> {
    if let Source::Photo(photo) = source {
        for original in [photo.source.as_ref(), photo.settings_path.as_ref()]
            .into_iter()
            .flatten()
        {
            if path == original
                || path
                    .canonicalize()
                    .is_ok_and(|p| original.canonicalize().is_ok_and(|o| p == o))
            {
                return Err(
                    "Choose a separate export file to preserve the original photo and its settings"
                        .into(),
                );
            }
        }
    }
    let (bytes, notes) = encode_admitted(source, settings, progress)?;
    write_atomic(path, bytes.as_slice(), progress)?;
    Ok(notes)
}

struct EncodedWriter<'a> {
    encoded: crate::deep_color::storage::Encoded,
    progress: &'a dyn Progress,
    cancel: &'a crate::formats::raw::Cancellation,
}
impl Write for EncodedWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.progress.check().map_err(std::io::Error::other)?;
        self.cancel.check().map_err(std::io::Error::other)?;
        self.encoded.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.encoded.flush()
    }
}
impl std::io::Seek for EncodedWriter<'_> {
    fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
        self.progress.check().map_err(std::io::Error::other)?;
        self.cancel.check().map_err(std::io::Error::other)?;
        std::io::Seek::seek(&mut self.encoded, position)
    }
}

pub(crate) fn write_atomic(
    path: &Path,
    bytes: &[u8],
    progress: &dyn Progress,
) -> Result<(), String> {
    progress.check()?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temp = parent.join(format!(
        ".omadesign-upscale-{}-{}.tmp",
        std::process::id(),
        crate::document::next_id()
    ));
    write_atomic_temp(path, &temp, bytes, progress)
}

/// Commit an export using a temporary file owned by this operation.
/// Args: destination, temporary path, bytes and progress. Returns without deleting any pre-existing file on creation failure.
fn write_atomic_temp(
    path: &Path,
    temp: &Path,
    bytes: &[u8],
    progress: &dyn Progress,
) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(temp)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        for (i, chunk) in bytes.chunks(256 * 1024).enumerate() {
            progress.check()?;
            file.write_all(chunk).map_err(|e| e.to_string())?;
            progress.report(
                "Saving export",
                (i * 256 * 1024 + chunk.len()).min(bytes.len()),
                bytes.len(),
            );
        }
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        progress.check()?;
        std::fs::rename(temp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ml::NoProgress;
    #[test]
    fn cancelled_write_preserves_existing_file_and_leaves_no_partial() {
        struct Cancel;
        impl Progress for Cancel {
            fn report(&self, _: &str, _: usize, _: usize) {}
            fn cancelled(&self) -> bool {
                true
            }
        }
        let root =
            std::env::temp_dir().join(format!("oma-upscale-write-{}", crate::document::next_id()));
        std::fs::create_dir_all(&root).unwrap();
        let p = root.join("output.png");
        std::fs::write(&p, b"original").unwrap();
        assert!(write_atomic(&p, b"new", &Cancel).is_err());
        assert_eq!(std::fs::read(&p).unwrap(), b"original");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn vector_formats_reject_ai_and_raster_scale_is_exact() {
        let source = Source::Document(Document::new("export", 31., 17., 96.));
        let settings = Settings {
            format: Format::Svg,
            ai: Some(Default::default()),
            ..Default::default()
        };
        assert!(encode(&source, settings, &NoProgress).is_err());
        let settings = Settings {
            scale: 3,
            ..Default::default()
        };
        let (bytes, _) = encode(&source, settings, &NoProgress).unwrap();
        let image = image::load_from_memory(&bytes).unwrap();
        assert_eq!((image.width(), image.height()), (93, 51));
    }
}
