//! Still-image export pipeline, shared by the dialog and real-model QA.
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
    Tiff,
    Svg,
    Psd,
    Psb,
    Pdf,
    Ora,
}
impl Format {
    pub const ALL: [Self; 8] = [
        Self::Png,
        Self::Jpeg,
        Self::Tiff,
        Self::Svg,
        Self::Psd,
        Self::Psb,
        Self::Pdf,
        Self::Ora,
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
    let (w, h) = if let Source::Document(doc) = source {
        // Match compositor rounding for fractional document dimensions.
        (
            (doc.width * settings.scale as f32).round().max(1.) as u32,
            (doc.height * settings.scale as f32).round().max(1.) as u32,
        )
    } else {
        upscale::dimensions(w, h, settings.scale as f64)?
    };
    upscale::dimensions(w, h, settings.ai.map_or(1., |s| s.factor))
}

pub fn encode(
    source: &Source,
    settings: Settings,
    progress: &dyn Progress,
) -> Result<(Vec<u8>, Vec<String>), String> {
    dimensions(source, settings)?;
    progress.check()?;
    progress.report("Rendering export", 0, 1);
    if !settings.format.raster() {
        if settings.ai.is_some() {
            return Err("AI upscaling is available for PNG, TIFF and JPEG".into());
        }
        let Source::Document(doc) = source else {
            return Err("Photos export as PNG, TIFF or JPEG".into());
        };
        let output = match settings.format {
            Format::Svg => (crate::svg::export(doc)?.into_bytes(), vec![]),
            Format::Psd | Format::Psb => {
                let out = crate::formats::psd::encode(doc, settings.format == Format::Psb)?;
                (out.bytes, out.warnings)
            }
            Format::Pdf => crate::formats::pdf::write(doc)?,
            Format::Ora => crate::formats::openraster::write(doc)?,
            _ => unreachable!(),
        };
        progress.check()?;
        return Ok(output);
    }
    if let Source::Photo(photo) = source
        && settings.ai.is_none()
        && settings.scale == 1
    {
        let bytes = photo.export_bytes(settings.format.extension())?;
        progress.check()?;
        return Ok((bytes, vec![]));
    }
    let mut raster = match source {
        Source::Photo(photo) => {
            let raster = if settings.ai.is_none() && photo.raw.is_some() {
                image::load_from_memory(&photo.export_bytes("png")?).map_err(|e| e.to_string())?
            } else {
                let out = photo.render_full();
                DynamicImage::ImageRgba8(
                    RgbaImage::from_raw(out.w, out.h, out.data).ok_or("Invalid developed photo")?,
                )
            };
            if settings.scale > 1 {
                raster.resize_exact(
                    raster.width() * settings.scale,
                    raster.height() * settings.scale,
                    image::imageops::FilterType::CatmullRom,
                )
            } else {
                raster
            }
        }
        Source::Document(doc) => {
            // The compositor returns premultiplied RGBA; models and encoders
            // require straight RGB, especially around translucent artwork.
            let pm = crate::compositor::render_export(doc, settings.scale)?;
            let bytes = pm
                .pixels()
                .iter()
                .flat_map(|p| {
                    let p = p.demultiply();
                    [p.red(), p.green(), p.blue(), p.alpha()]
                })
                .collect();
            DynamicImage::ImageRgba8(RgbaImage::from_raw(pm.width(), pm.height(), bytes).unwrap())
        }
    };
    progress.check()?;
    if let Some(settings) = settings.ai {
        raster =
            DynamicImage::ImageRgba8(upscale::upscale(&raster.into_rgba8(), settings, progress)?);
    }
    progress.report("Encoding export", 0, 1);
    let mut bytes = std::io::Cursor::new(Vec::new());
    if settings.format == Format::Jpeg {
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
    Ok((bytes.into_inner(), vec![]))
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
    let (bytes, notes) = encode(source, settings, progress)?;
    write_atomic(path, &bytes, progress)?;
    Ok(notes)
}

fn write_atomic(path: &Path, bytes: &[u8], progress: &dyn Progress) -> Result<(), String> {
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
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
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
        std::fs::rename(&temp, path).map_err(|e| e.to_string())
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
