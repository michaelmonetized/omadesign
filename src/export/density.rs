//! Physical density tags are written by the existing PNG/JPEG/TIFF codecs.
use crate::resources::CancellationCheck;
use std::{borrow::Cow, io::Write};

/// Validate a codec's representable physical density before rendering.
/// Args: format and positive pixels per inch. Returns an error instead of saturating a density tag.
pub(super) fn validate(format: super::Format, dpi: f64) -> Result<(), String> {
    if !dpi.is_finite() || dpi <= 0. {
        return Err("Output PPI must be positive and finite".into());
    }
    let valid = match format {
        super::Format::Png => (1.0..=u32::MAX as f64).contains(&(dpi / 0.0254).round()),
        super::Format::Jpeg => (1.0..=u16::MAX as f64).contains(&dpi.round()),
        super::Format::Tiff => return crate::deep_color::tiff::density(dpi).map(|_| ()),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err("The requested PPI cannot be represented by this export format".into())
    }
}

/// Encode straight RGB samples with a real PNG pHYs density and the original ICC bytes.
/// Args: current admitted writer, raster, profile, PPI and cancellation. Returns only after codec finalization.
pub(super) fn png<W: Write>(
    writer: &mut W,
    raster: &image::DynamicImage,
    profile: &[u8],
    dpi: f64,
    cancel: &dyn CancellationCheck,
) -> Result<(), String> {
    validate(super::Format::Png, dpi)?;
    cancel.check()?;
    let depth = match raster {
        image::DynamicImage::ImageRgba8(_) => png::BitDepth::Eight,
        image::DynamicImage::ImageRgba16(_) => png::BitDepth::Sixteen,
        _ => return Err("PNG density encoding requires retained 8/16-bit RGBA samples".into()),
    };
    let _metadata = crate::resources::global().reserve_now(profile.len() as u64, cancel)?;
    let mut info = png::Info::with_size(raster.width(), raster.height());
    info.color_type = png::ColorType::Rgba;
    info.bit_depth = depth;
    let ppu = (dpi / 0.0254).round() as u32;
    info.pixel_dims = Some(png::PixelDimensions {
        xppu: ppu,
        yppu: ppu,
        unit: png::Unit::Meter,
    });
    if !profile.is_empty() {
        info.icc_profile = Some(Cow::Borrowed(profile));
    }
    let encoder = png::Encoder::with_info(writer, info).map_err(|e| e.to_string())?;
    let mut header = encoder.write_header().map_err(|e| e.to_string())?;
    {
        let mut stream = header.stream_writer().map_err(|e| e.to_string())?;
        match raster {
            image::DynamicImage::ImageRgba8(image) => {
                for row in image.as_raw().chunks_exact(raster.width() as usize * 4) {
                    cancel.check()?;
                    stream.write_all(row).map_err(|e| e.to_string())?;
                }
            }
            image::DynamicImage::ImageRgba16(image) => {
                let samples = raster.width() as usize * 4;
                let mut row = crate::resources::global().allocate_now::<u8, _>(
                    samples.checked_mul(2).ok_or("PNG row bytes overflow")?,
                    cancel,
                )?;
                for source in image.as_raw().chunks_exact(samples) {
                    cancel.check()?;
                    for (bytes, value) in row.as_mut_slice().chunks_exact_mut(2).zip(source) {
                        bytes.copy_from_slice(&value.to_be_bytes());
                    }
                    stream
                        .write_all(row.as_slice())
                        .map_err(|e| e.to_string())?;
                }
            }
            _ => unreachable!(),
        }
        stream.finish().map_err(|e| e.to_string())?;
    }
    header.finish().map_err(|e| e.to_string())?;
    cancel.check()
}
