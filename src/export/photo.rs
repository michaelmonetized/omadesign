//! Photo export keeps its admitted encoded owner through the caller's final write.
use super::{EncodedBytes, Format, Settings};
use crate::{formats::raw::Cancellation, ml::Progress, photo::PhotoImage, resources};
use image::{DynamicImage, RgbaImage};
use std::io::{Seek, Write};

/// Check ordinary output arithmetic without an arbitrary photo pixel ceiling.
/// Args: immutable photo and requested export settings. Returns codec dimensions or an overflow error.
pub(super) fn dimensions(photo: &PhotoImage, settings: Settings) -> Result<(u32, u32), String> {
    if !(1..=8).contains(&settings.scale) {
        return Err("Choose a render scale between 1× and 8×".into());
    }
    let (w, h) = photo.dimensions();
    let (w, h) = photo.develop.output_dim(w, h);
    let w = w
        .checked_mul(settings.scale)
        .ok_or("Photo width overflows")?;
    let h = h
        .checked_mul(settings.scale)
        .ok_or("Photo height overflows")?;
    let count = u64::from(w)
        .checked_mul(u64::from(h))
        .filter(|&n| n > 0)
        .and_then(|n| n.checked_mul(16))
        .ok_or("Photo sample bytes overflow")?;
    if count > isize::MAX as u64 {
        return Err("Photo samples exceed addressable memory".into());
    }
    if let Some(ai) = settings.ai {
        crate::upscale::dimensions(w, h, ai.factor)
    } else {
        Ok((w, h))
    }
}

struct Writer<'a> {
    encoded: crate::deep_color::storage::Encoded,
    cancel: &'a Cancellation,
}
impl Write for Writer<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.cancel.check().map_err(std::io::Error::other)?;
        self.encoded.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.encoded.flush()
    }
}
impl Seek for Writer<'_> {
    fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
        self.cancel.check().map_err(std::io::Error::other)?;
        self.encoded.seek(position)
    }
}

/// Encode photo pixels while retaining the original shared allocation lease.
/// Args: immutable photo, export settings, progress and owning cancellation token. Returns admitted output and notes.
pub(super) fn encode_admitted(
    photo: &PhotoImage,
    settings: Settings,
    progress: &dyn Progress,
    cancel: &Cancellation,
) -> Result<(EncodedBytes, Vec<String>), String> {
    if !settings.format.raster() {
        return Err("Export the developed photo as PNG, TIFF or JPEG".into());
    }
    let (w, h) = dimensions(photo, settings)?;
    cancel.check()?;
    progress.check()?;
    if settings.ai.is_none() && settings.scale == 1 {
        let bytes = photo.export_bytes_cancellable(settings.format.extension(), cancel)?;
        progress.check()?;
        return Ok((bytes.into_allocation()?, vec![]));
    }
    let (sw, sh) = photo.dimensions();
    let demand = (u64::from(w) * u64::from(h))
        .checked_mul(32)
        .and_then(|n| n.checked_add((u64::from(sw) * u64::from(sh)).checked_mul(16)?))
        .and_then(|n| n.checked_add(64 * 1024 * 1024))
        .ok_or("Photo export memory demand overflow")?;
    let _workspace = resources::global().reserve(demand, cancel)?;
    let mut raster = if settings.ai.is_none() && photo.raw.is_some() {
        let input = photo.export_bytes_cancellable("png", cancel)?;
        image::load_from_memory(&input).map_err(|e| e.to_string())?
    } else {
        let out = photo.render_full_cancellable(cancel)?;
        let image = out.image;
        DynamicImage::ImageRgba8(
            RgbaImage::from_raw(image.w, image.h, image.data).ok_or("Invalid developed photo")?,
        )
    };
    cancel.check()?;
    if settings.scale > 1 {
        raster = raster.resize_exact(
            raster
                .width()
                .checked_mul(settings.scale)
                .ok_or("Photo width overflows")?,
            raster
                .height()
                .checked_mul(settings.scale)
                .ok_or("Photo height overflows")?,
            image::imageops::FilterType::CatmullRom,
        );
    }
    cancel.check()?;
    if let Some(ai) = settings.ai {
        raster =
            DynamicImage::ImageRgba8(crate::upscale::upscale(&raster.into_rgba8(), ai, progress)?);
    }
    progress.report("Encoding export", 0, 1);
    let mut output = Writer {
        encoded: crate::deep_color::storage::Encoded::new()?,
        cancel,
    };
    if settings.format == Format::Jpeg {
        let mut rgba = raster.into_rgba8();
        for (i, pixel) in rgba.pixels_mut().enumerate() {
            if i % 4096 == 0 {
                cancel.check()?;
            }
            let alpha = u32::from(pixel[3]);
            for channel in &mut pixel.0[..3] {
                *channel = ((u32::from(*channel) * alpha + 127) / 255 + 255 - alpha) as u8;
            }
            pixel[3] = 255;
        }
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 95)
            .encode_image(&DynamicImage::ImageRgba8(rgba).into_rgb8())
            .map_err(|e| e.to_string())?;
    } else {
        raster
            .write_to(
                &mut output,
                if settings.format == Format::Png {
                    image::ImageFormat::Png
                } else {
                    image::ImageFormat::Tiff
                },
            )
            .map_err(|e| e.to_string())?;
    }
    cancel.check()?;
    progress.check()?;
    Ok((output.encoded.finish()?, vec![]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn photo() -> PhotoImage {
        PhotoImage::from_raw(
            "Output owner".into(),
            crate::formats::raw::RawImage {
                width: 17,
                height: 11,
                pixels: (0..17 * 11 * 3)
                    .map(|n| (n * 113 % 65536) as u16)
                    .collect::<Vec<_>>()
                    .into(),
                memory: None,
                metadata: Default::default(),
                warnings: vec![],
            },
        )
        .unwrap()
    }

    #[test]
    fn admitted_photo_output_keeps_exact_codec_pixels_and_final_clone_lease() {
        let photo = photo();
        for format in [Format::Png, Format::Tiff] {
            let expected = photo.export_bytes(format.extension()).unwrap();
            let (output, _) = encode_admitted(
                &photo,
                Settings {
                    format,
                    ..Default::default()
                },
                &crate::ml::NoProgress,
                &Cancellation::default(),
            )
            .unwrap();
            let expected_pixels = image::load_from_memory(&expected).unwrap().to_rgba16();
            assert_eq!(
                image::load_from_memory(output.as_slice())
                    .unwrap()
                    .to_rgba16(),
                expected_pixels
            );
            let weak = Arc::downgrade(output.lease());
            let clone = output.clone();
            assert!(Arc::ptr_eq(&output, &clone));
            assert!(output.lease().bytes() >= output.bytes());
            drop(output);
            assert!(weak.upgrade().is_some());
            drop(clone);
            assert!(weak.upgrade().is_none());
        }
    }

    #[test]
    fn admitted_scaled_raw_output_retains_u16_and_rejects_cancelled_job() {
        let photo = photo();
        let settings = Settings {
            format: Format::Tiff,
            scale: 2,
            ai: None,
        };
        let (output, _) = encode_admitted(
            &photo,
            settings,
            &crate::ml::NoProgress,
            &Cancellation::default(),
        )
        .unwrap();
        let image = image::load_from_memory(output.as_slice()).unwrap();
        assert_eq!((image.width(), image.height()), (34, 22));
        assert_eq!(image.color(), image::ColorType::Rgba16);
        let cancel = Cancellation::default();
        cancel.cancel();
        assert!(
            encode_admitted(&photo, settings, &crate::ml::NoProgress, &cancel)
                .unwrap_err()
                .contains("cancelled")
        );
    }

    #[test]
    fn ordinary_photo_dimensions_use_checked_demand_instead_of_ai_ceiling() {
        let mut photo = photo();
        photo.raw = Some(Arc::new(crate::formats::raw::RawImage {
            width: 32768,
            height: 16385,
            pixels: vec![0u16; 3].into(),
            memory: None,
            metadata: Default::default(),
            warnings: vec![],
        }));
        assert_eq!(
            dimensions(&photo, Settings::default()).unwrap(),
            (32768, 16385)
        );
        assert!(
            dimensions(
                &photo,
                Settings {
                    scale: 0,
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            dimensions(
                &photo,
                Settings {
                    ai: Some(Default::default()),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}
