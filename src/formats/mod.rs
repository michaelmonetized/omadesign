//! Layered interchange formats. Conversion notes are part of the imported document.
pub mod affinity;
pub mod cli;
pub mod idml;
pub mod illustrator;
pub mod openraster;
pub mod pdf;
pub mod psd;
pub mod raw;
pub mod svg;
pub mod xcf;

pub fn decode_image(bytes: &[u8]) -> Result<crate::document::Pixels, String> {
    use crate::{
        deep_color::{Depth, storage},
        document::Pixels,
        resources,
    };
    use image::{DynamicImage, ImageDecoder};
    let _input = resources::global().reserve_now(bytes.len() as u64, &storage::check)?;
    if let Some(pixels) = crate::deep_color::tiff::decode(bytes)? {return Ok(pixels);}
    let _codec = resources::global().reserve_now(16 * 1024 * 1024, &storage::check)?;
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(16 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let (w, h) = decoder.dimensions();
    let count = storage::count(w, h, 4)?;
    let memory = resources::global().reserve_now(decoder.total_bytes(), &storage::check)?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(decoder.total_bytes().saturating_add(16 * 1024 * 1024));
    decoder.set_limits(limits).map_err(|e| e.to_string())?;
    let profile = decoder.icc_profile().map_err(|e| e.to_string())?.unwrap_or_default();
    let decoded =
        DynamicImage::from_decoder(decoder).map_err(|e| format!("Could not decode image: {e}"))?;
    let native = matches!(
        decoded.color(),
        image::ColorType::L16
            | image::ColorType::La16
            | image::ColorType::Rgb16
            | image::ColorType::Rgba16
            | image::ColorType::Rgb32F
            | image::ColorType::Rgba32F
    );
    let mut pixels = if native {
        let depth = if matches!(
            decoded.color(),
            image::ColorType::Rgb32F | image::ColorType::Rgba32F
        ) {
            Depth::F32
        } else {
            Depth::U16
        };
        let mut rgba = resources::global().allocate_now::<f32, _>(count, &storage::check)?;
        macro_rules! convert {
            ($image:expr,$channels:expr,$scale:expr) => {{
                for (src, dst) in $image
                    .as_raw()
                    .chunks_exact($channels)
                    .zip(rgba.as_mut_slice().chunks_exact_mut(4))
                {
                    let r = src[0] as f32 * $scale;
                    let (g, b) = if $channels < 3 {
                        (r, r)
                    } else {
                        (src[1] as f32 * $scale, src[2] as f32 * $scale)
                    };
                    let a = if $channels == 2 {
                        src[1] as f32 * $scale
                    } else if $channels == 4 {
                        src[3] as f32 * $scale
                    } else {
                        1.
                    };
                    dst.copy_from_slice(&[r, g, b, a]);
                }
            }};
        }
        match decoded {
            DynamicImage::ImageLuma16(image) => convert!(image, 1, 1. / 65535.),
            DynamicImage::ImageLumaA16(image) => convert!(image, 2, 1. / 65535.),
            DynamicImage::ImageRgb16(image) => convert!(image, 3, 1. / 65535.),
            DynamicImage::ImageRgba16(image) => convert!(image, 4, 1. / 65535.),
            DynamicImage::ImageRgb32F(image) => convert!(image, 3, 1.),
            DynamicImage::ImageRgba32F(image) => convert!(image, 4, 1.),
            _ => unreachable!(),
        }
        Pixels::from_sample_allocation(w, h, depth, std::sync::Arc::new(rgba))
    } else if let DynamicImage::ImageRgba8(image) = decoded {
        let bytes = image.into_raw();
        memory.resize_now(bytes.capacity() as u64, &storage::check)?;
        Pixels::from_byte_allocation(w, h, resources::global().adopt(bytes, memory)?)
    } else {
        let mut rgba = resources::global().allocate_now::<u8, _>(count, &storage::check)?;
        macro_rules! convert {
            ($image:expr,$channels:expr) => {{
                for (src, dst) in $image
                    .as_raw()
                    .chunks_exact($channels)
                    .zip(rgba.as_mut_slice().chunks_exact_mut(4))
                {
                    let r = src[0];
                    let (g, b) = if $channels < 3 {
                        (r, r)
                    } else {
                        (src[1], src[2])
                    };
                    let a = if $channels == 2 { src[1] } else { 255 };
                    dst.copy_from_slice(&[r, g, b, a]);
                }
            }};
        }
        match decoded {
            DynamicImage::ImageLuma8(image) => convert!(image, 1),
            DynamicImage::ImageLumaA8(image) => convert!(image, 2),
            DynamicImage::ImageRgb8(image) => convert!(image, 3),
            _ => return Err("Unsupported decoded image samples".into()),
        }
        Pixels::from_byte_allocation(w, h, std::sync::Arc::new(rgba))
    }?;
    if !profile.is_empty() {pixels.assign_rgb_profile(profile,1)?;}
    Ok(pixels)
}

pub fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    let temp = parent.join(format!(
        ".omadesign-export-{}-{}.tmp",
        std::process::id(),
        crate::document::next_id()
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        std::fs::rename(&temp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}
