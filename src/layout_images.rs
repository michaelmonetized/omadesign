//! Self-contained image fills with bounded decoding and a shared image cache.
//! Cloning an editable design shares encoded image bytes; undo and component
//! instances do not duplicate photographs on every geometry edit.

use crate::document::Shape;
use crate::geom::{Bounds, Pt};
use base64::Engine;
use image::ImageDecoder;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use tiny_skia::{FillRule, FilterQuality, Mask, Pixmap, PixmapPaint, Transform};

const MAX_PIXELS: u64 = 64_000_000;
const MAX_EDGE: u32 = 16_384;
const MAX_SOURCE_BYTES: usize = 96 * 1024 * 1024;
const MAX_CACHE_BYTES: usize = 256 * 1024 * 1024;
const MAX_CACHE_ENTRIES: usize = 12;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageFit {
    #[default]
    Cover,
    Contain,
    Stretch,
}

impl ImageFit {
    pub fn name(self) -> &'static str {
        match self {
            Self::Cover => "Fill",
            Self::Contain => "Fit",
            Self::Stretch => "Stretch",
        }
    }
}

fn centered() -> Pt {
    Pt::splat(0.5)
}
fn serialize_data<S: Serializer>(data: &Arc<str>, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(data)
}
fn deserialize_data<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Arc<str>, D::Error> {
    let data = String::deserialize(deserializer)?;
    if data.len() > MAX_SOURCE_BYTES * 4 / 3 + 4 {
        return Err(serde::de::Error::custom(
            "Image fill exceeds the supported encoded size",
        ));
    }
    Ok(data.into())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageFill {
    /// PNG bytes encoded as base64; no external file paths or network requests.
    #[serde(
        serialize_with = "serialize_data",
        deserialize_with = "deserialize_data"
    )]
    pub data: Arc<str>,
    #[serde(default)]
    pub fit: ImageFit,
    /// Normalized crop focal point; (0.5,0.5) centers the image.
    #[serde(default = "centered")]
    pub focal: Pt,
}

impl PartialEq for ImageFill {
    fn eq(&self, other: &Self) -> bool {
        self.fit == other.fit
            && self.focal == other.focal
            && (Arc::ptr_eq(&self.data, &other.data) || self.data == other.data)
    }
}

struct Cached {
    source: Weak<str>,
    value: Result<Arc<Pixmap>, String>,
    bytes: usize,
    touched: u64,
}
#[derive(Default)]
struct ImageCache {
    entries: HashMap<usize, Cached>,
    tick: u64,
    bytes: usize,
}
impl ImageCache {
    fn insert(&mut self, data: &Arc<str>, value: Result<Arc<Pixmap>, String>) {
        // An expired weak source can never be used again; reclaim it promptly.
        self.entries
            .retain(|_, entry| entry.source.strong_count() > 0);
        self.bytes = self.entries.values().map(|e| e.bytes).sum();
        let bytes = value.as_ref().map_or(0, |p| p.data().len());
        if bytes > MAX_CACHE_BYTES {
            return;
        }
        let key = Arc::as_ptr(data) as *const () as usize;
        if let Some(old) = self.entries.remove(&key) {
            self.bytes -= old.bytes;
        }
        while self.bytes + bytes > MAX_CACHE_BYTES || self.entries.len() >= MAX_CACHE_ENTRIES {
            let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.touched)
                .map(|(id, _)| *id)
            else {
                break;
            };
            if let Some(old) = self.entries.remove(&oldest) {
                self.bytes -= old.bytes;
            }
        }
        self.tick = self.tick.wrapping_add(1);
        self.bytes += bytes;
        self.entries.insert(
            key,
            Cached {
                source: Arc::downgrade(data),
                value,
                bytes,
                touched: self.tick,
            },
        );
    }

    fn get(&mut self, data: &Arc<str>) -> Option<Result<Arc<Pixmap>, String>> {
        let key = Arc::as_ptr(data) as *const () as usize;
        let entry = self.entries.get_mut(&key)?;
        let source = entry.source.upgrade()?;
        if !Arc::ptr_eq(&source, data) {
            return None;
        }
        self.tick = self.tick.wrapping_add(1);
        entry.touched = self.tick;
        Some(entry.value.clone())
    }
}

fn cache() -> &'static Mutex<ImageCache> {
    static CACHE: OnceLock<Mutex<ImageCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(ImageCache::default()))
}

fn decode(bytes: &[u8], png_only: bool) -> Result<image::DynamicImage, String> {
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err("Choose an image smaller than 96 MiB".into());
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| format!("Cannot read image: {e}"))?;
    if png_only && reader.format() != Some(image::ImageFormat::Png) {
        return Err("Stored image fills must contain PNG data".into());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_EDGE);
    limits.max_image_height = Some(MAX_EDGE);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| format!("Cannot decode image: {e}"))?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err("Image fills support up to 64 megapixels".into());
    }
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut decoded = image::DynamicImage::from_decoder(decoder)
        .map_err(|e| format!("Cannot decode image: {e}"))?;
    decoded.apply_orientation(orientation);
    Ok(decoded)
}

/// Import JPEG, PNG, WebP, GIF (first frame), BMP or TIFF and embed normalized
/// PNG data, honoring camera orientation. The resulting .oma needs no sidecar.
pub fn load(path: &Path) -> Result<ImageFill, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("Cannot open image: {e}"))?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_SOURCE_BYTES as u64 {
        return Err("Choose an image smaller than 96 MiB".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_SOURCE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read image: {e}"))?;
    from_bytes(&bytes)
}

pub fn from_bytes(bytes: &[u8]) -> Result<ImageFill, String> {
    let decoded = decode(bytes, false)?;
    let rgba = decoded.to_rgba8();
    let mut encoded = Cursor::new(Vec::new());
    decoded
        .write_to(&mut encoded, image::ImageFormat::Png)
        .map_err(|e| format!("Cannot prepare image: {e}"))?;
    if encoded.get_ref().len() > MAX_SOURCE_BYTES {
        return Err("The embedded image exceeds 96 MiB; use a smaller image".into());
    }
    let fill = ImageFill {
        data: base64::engine::general_purpose::STANDARD
            .encode(encoded.into_inner())
            .into(),
        fit: ImageFit::Cover,
        focal: centered(),
    };
    let pm = crate::color::rgba_to_pixmap(rgba.width(), rgba.height(), &rgba)
        .ok_or("Cannot allocate image pixels")?;
    cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(&fill.data, Ok(Arc::new(pm)));
    Ok(fill)
}

/// Cached by immutable shared source identity; zooming, reflowing or editing a
/// crop never re-hashes or re-decodes the image's encoded bytes.
pub fn pixmap(fill: &ImageFill) -> Result<Arc<Pixmap>, String> {
    if let Some(value) = cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&fill.data)
    {
        return value;
    }
    let value = (|| {
        if fill.data.len() > MAX_SOURCE_BYTES * 4 / 3 + 4 {
            return Err("Stored image exceeds the supported size".into());
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(fill.data.as_ref())
            .map_err(|e| format!("Invalid embedded image: {e}"))?;
        let rgba = decode(&bytes, true)?.to_rgba8();
        crate::color::rgba_to_pixmap(rgba.width(), rgba.height(), &rgba)
            .map(Arc::new)
            .ok_or_else(|| "Cannot allocate image pixels".into())
    })();
    cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(&fill.data, value.clone());
    value
}

pub fn placement(bounds: Bounds, image_size: Pt, fit: ImageFit, focal: Pt) -> Bounds {
    if fit == ImageFit::Stretch {
        return bounds;
    }
    let sx = bounds.width().max(1.0) / image_size.x.max(1.0);
    let sy = bounds.height().max(1.0) / image_size.y.max(1.0);
    let scale = if fit == ImageFit::Cover {
        sx.max(sy)
    } else {
        sx.min(sy)
    };
    let size = image_size * scale;
    let focal = Pt::new(
        if focal.x.is_finite() {
            focal.x.clamp(0.0, 1.0)
        } else {
            0.5
        },
        if focal.y.is_finite() {
            focal.y.clamp(0.0, 1.0)
        } else {
            0.5
        },
    );
    Bounds::from_min_size(
        Pt::new(
            bounds.min.x + (bounds.width() - size.x) * focal.x,
            bounds.min.y + (bounds.height() - size.y) * focal.y,
        ),
        size,
    )
}

/// Paint an image inside the shape's actual rounded/rotated outline. `transform`
/// contains ancestor/view transforms, while the shape's own rotation is applied
/// here. Call between the shape's background fill and its stroke.
pub fn draw(
    pm: &mut Pixmap,
    shape: &Shape,
    transform: Transform,
    opacity: f32,
    mask: Option<&Mask>,
) {
    draw_with_blend(
        pm,
        shape,
        transform,
        opacity,
        tiny_skia::BlendMode::SourceOver,
        mask,
    );
}

pub fn draw_with_blend(
    pm: &mut Pixmap,
    shape: &Shape,
    transform: Transform,
    opacity: f32,
    blend: tiny_skia::BlendMode,
    mask: Option<&Mask>,
) {
    let Some(fill) = &shape.layout.image else {
        return;
    };
    let Ok(image) = pixmap(fill) else {
        return;
    };
    let bounds = shape.geom.bbox();
    let target = placement(
        bounds,
        Pt::new(image.width() as f32, image.height() as f32),
        fill.fit,
        fill.focal,
    );
    if !target.width().is_finite()
        || !target.height().is_finite()
        || target.width() <= 0.0
        || target.height() <= 0.0
    {
        return;
    }
    let Some(path) = shape.get_cached_path(96) else {
        return;
    };
    let Some(mut clip) = mask.cloned().or_else(|| Mask::new(pm.width(), pm.height())) else {
        return;
    };
    let rule = if matches!(shape.geom, crate::geom::Geom::Poly { winding: true, .. }) {
        FillRule::Winding
    } else {
        FillRule::EvenOdd
    };
    if mask.is_some() {
        clip.intersect_path(&path, rule, true, transform);
    } else {
        clip.fill_path(&path, rule, true, transform);
    }
    let centre = bounds.center();
    let image_transform = transform
        .pre_concat(Transform::from_rotate_at(
            shape.rotation.to_degrees(),
            centre.x,
            centre.y,
        ))
        .pre_translate(target.min.x, target.min.y)
        .pre_scale(
            target.width() / image.width() as f32,
            target.height() / image.height() as f32,
        );
    pm.draw_pixmap(
        0,
        0,
        image.as_ref().as_ref(),
        &PixmapPaint {
            opacity: opacity.clamp(0.0, 1.0),
            blend_mode: blend,
            quality: FilterQuality::Bilinear,
        },
        image_transform,
        Some(&clip),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image() -> ImageFill {
        let mut pm = Pixmap::new(4, 2).unwrap();
        for (i, p) in pm.pixels_mut().iter_mut().enumerate() {
            *p = tiny_skia::PremultipliedColorU8::from_rgba(
                if i % 4 < 2 { 255 } else { 0 },
                0,
                if i % 4 >= 2 { 255 } else { 0 },
                255,
            )
            .unwrap();
        }
        from_bytes(&pm.encode_png().unwrap()).unwrap()
    }

    #[test]
    fn image_fill_round_trip_preserves_pixels_and_clones_share_storage() {
        let fill = image();
        let cloned = fill.clone();
        assert!(Arc::ptr_eq(&fill.data, &cloned.data));
        assert!(Arc::ptr_eq(
            &pixmap(&fill).unwrap(),
            &pixmap(&cloned).unwrap()
        ));
        let reopened: ImageFill =
            serde_json::from_str(&serde_json::to_string(&fill).unwrap()).unwrap();
        assert_eq!(
            pixmap(&fill).unwrap().data(),
            pixmap(&reopened).unwrap().data()
        );
        assert_eq!(fill, reopened);
    }

    #[test]
    fn cover_and_contain_use_focal_point_and_preserve_aspect_ratio() {
        let frame = Bounds::from_min_size(Pt::new(10.0, 20.0), Pt::new(100.0, 100.0));
        let cover = placement(
            frame,
            Pt::new(200.0, 100.0),
            ImageFit::Cover,
            Pt::new(1.0, 0.5),
        );
        assert_eq!(cover.min.x, -90.0);
        assert_eq!(cover.size(), Pt::new(200.0, 100.0));
        let fit = placement(frame, Pt::new(200.0, 100.0), ImageFit::Contain, centered());
        assert_eq!(fit.min, Pt::new(10.0, 45.0));
        assert_eq!(fit.size(), Pt::new(100.0, 50.0));
    }

    #[test]
    fn draw_clips_image_to_rounded_frame_and_honors_crop_focus() {
        let mut shape = crate::layout::make_frame(Pt::new(10.0, 10.0), Pt::new(40.0, 40.0));
        shape.corners = [10.0; 4];
        let mut fill = image();
        fill.focal = Pt::new(1.0, 0.5);
        shape.layout.image = Some(fill);
        let mut pm = Pixmap::new(60, 60).unwrap();
        draw(&mut pm, &shape, Transform::identity(), 1.0, None);
        assert_eq!(pm.pixel(30, 30).unwrap().blue(), 255);
        assert_eq!(pm.pixel(10, 10).unwrap().alpha(), 0);
        assert_eq!(pm.pixel(5, 30).unwrap().alpha(), 0);
    }

    #[test]
    fn corrupt_embedded_image_reports_error_and_cache_is_bounded() {
        let bad = ImageFill {
            data: "not a PNG".into(),
            fit: ImageFit::Cover,
            focal: centered(),
        };
        assert!(pixmap(&bad).is_err());
        let mut cache = ImageCache::default();
        let sources: Vec<Arc<str>> = (0..30).map(|i| format!("{i}").into()).collect();
        for data in &sources {
            cache.insert(data, Err("invalid".into()));
        }
        assert!(cache.entries.len() <= MAX_CACHE_ENTRIES);
        assert!(cache.bytes <= MAX_CACHE_BYTES);
    }
}
