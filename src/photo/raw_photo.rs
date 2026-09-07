//! High-precision RAW development. Preview pixels are derived; the decoded
//! 16-bit linear source is shared by full-resolution exports and never edited.
use super::*;
use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct LinearImage {
    pub w: u32,
    pub h: u32,
    pub pixels: Vec<u16>,
}

impl LinearImage {
    fn preview(w: u32, h: u32, pixels: &[u16], max_edge: u32) -> Self {
        let scale = (max_edge as f64 / w.max(h) as f64).min(1.);
        let (pw, ph) = (
            (w as f64 * scale).round().max(1.) as u32,
            (h as f64 * scale).round().max(1.) as u32,
        );
        if (pw, ph) == (w, h) {
            return Self {
                w,
                h,
                pixels: pixels.to_vec(),
            };
        }
        let source = image::ImageBuffer::<image::Rgb<u16>, _>::from_raw(w, h, pixels)
            .expect("validated RAW buffer");
        let resized =
            image::imageops::resize(&source, pw, ph, image::imageops::FilterType::Triangle);
        Self {
            w: pw,
            h: ph,
            pixels: resized.into_raw(),
        }
    }
}

impl PhotoImage {
    pub fn from_raw(name: String, raw: crate::formats::raw::RawImage) -> Self {
        let linear = LinearImage::preview(raw.width, raw.height, &raw.pixels, 1600);
        let baseline = DevelopParams {
            exposure: raw.metadata.baseline_exposure,
            ..Default::default()
        };
        let (w, h, data) = develop_inner::<u8>(
            DevelopSource::linear(linear.w, linear.h, &linear.pixels),
            &baseline,
        );
        let preview = Arc::new(RgbaImage { w, h, data });
        let thumb = Arc::new(preview.downscaled(192));
        Self {
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
        }
    }

    /// Decode on a worker, then restore settings associated with this exact source.
    pub fn load(path: &Path) -> Result<Self, String> {
        let path = path
            .canonicalize()
            .map_err(|e| format!("Could not open photo: {e}"))?;
        let identity = edits::SourceIdentity::read(&path)?;
        let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() > 512 * 1024 * 1024 {
            return Err("Choose a photo smaller than 512 MiB".into());
        }
        let mut bytes = Vec::new();
        file.take(512 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 512 * 1024 * 1024 {
            return Err("Photo exceeds 512 MiB".into());
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        let mut photo = if crate::formats::raw::is_extension(extension) {
            Self::from_raw(name, crate::formats::raw::decode(&bytes)?)
        } else {
            let pixels = crate::formats::decode_image(&bytes)?;
            Self::from_full(
                name,
                RgbaImage {
                    w: pixels.w,
                    h: pixels.h,
                    data: pixels.data,
                },
            )
        };
        if edits::SourceIdentity::read(&path)? != identity {
            return Err("The photo changed while it was opening. Open it again.".into());
        }
        match edits::load(&path, &identity) {
            Ok(Some(params)) => photo.develop = params,
            Ok(None) => {}
            Err(error) => photo
                .notes
                .push(format!("Saved photo settings were not loaded: {error}")),
        }
        photo.source = Some(path);
        photo.source_identity = Some(identity);
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

    pub fn render_preview(&self) -> RgbaImage {
        if let Some(raw) = &self.raw_preview {
            let (w, h, data) = develop_inner::<u8>(
                DevelopSource::linear(raw.w, raw.h, &raw.pixels),
                &self.raw_params(),
            );
            RgbaImage { w, h, data }
        } else {
            develop(&self.preview, &self.develop)
        }
    }

    pub fn render_full(&self) -> RgbaImage {
        if let Some(raw) = &self.raw {
            let (w, h, data) = develop_inner::<u8>(
                DevelopSource::linear(raw.width, raw.height, &raw.pixels),
                &self.raw_params(),
            );
            RgbaImage { w, h, data }
        } else {
            develop(&self.full, &self.develop)
        }
    }

    /// PNG/TIFF retain sixteen-bit channels for RAW; JPEG is an eight-bit delivery image.
    pub fn export_bytes(&self, extension: &str) -> Result<Vec<u8>, String> {
        let extension = extension.to_ascii_lowercase();
        let format = match extension.as_str() {
            "png" => image::ImageFormat::Png,
            "tif" | "tiff" => image::ImageFormat::Tiff,
            "jpg" | "jpeg" => image::ImageFormat::Jpeg,
            _ => return Err("Export the developed photo as PNG, TIFF or JPEG".into()),
        };
        let mut bytes = Cursor::new(Vec::new());
        if let Some(raw) = &self.raw
            && format != image::ImageFormat::Jpeg
        {
            let (w, h, data) = develop_inner::<u16>(
                DevelopSource::linear(raw.width, raw.height, &raw.pixels),
                &self.raw_params(),
            );
            let rgba = image::ImageBuffer::<image::Rgba<u16>, _>::from_raw(w, h, data)
                .ok_or("Invalid developed pixels")?;
            image::DynamicImage::ImageRgba16(rgba)
                .write_to(&mut bytes, format)
                .map_err(|e| e.to_string())?;
        } else {
            let out = self.render_full();
            if format == image::ImageFormat::Jpeg {
                let mut rgb = Vec::with_capacity(out.w as usize * out.h as usize * 3);
                for pixel in out.data.as_chunks::<4>().0 {
                    let alpha = u32::from(pixel[3]);
                    for channel in &pixel[..3] {
                        rgb.push(((u32::from(*channel) * alpha + 127) / 255 + 255 - alpha) as u8);
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
        Ok(bytes.into_inner())
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
