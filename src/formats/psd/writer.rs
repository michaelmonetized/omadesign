use super::{
    Document, Layer, LayerKind, MAX_DECODED_BYTES, MAX_LAYERS, Pixels, PsdExport, Rect, dimensions,
    encode_blend, warn,
};
use crate::color::Blend;
use std::collections::HashSet;
use std::io::Write;

struct Record {
    rect: Rect,
    name: String,
    blend: [u8; 4],
    opacity: u8,
    flags: u8,
    section: u32,
    mask_rect: Option<Rect>,
    channels: Vec<(i16, Vec<u8>)>,
    overlay: Option<super::effects::Overlay>,
}

fn u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}
fn u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}
fn i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_be_bytes());
}
fn rect(out: &mut Vec<u8>, value: Rect) {
    for v in [value.top, value.left, value.bottom, value.right] {
        i32(out, v);
    }
}
fn length(out: &mut Vec<u8>, n: usize, large: bool) -> Result<(), String> {
    if large {
        out.extend_from_slice(&(n as u64).to_be_bytes());
    } else {
        u32(
            out,
            u32::try_from(n)
                .map_err(|_| "Photoshop section exceeds PSD's 4 GiB limit; export PSB")?,
        );
    }
    Ok(())
}
fn section(out: &mut Vec<u8>, data: &[u8], large: bool) -> Result<(), String> {
    length(out, data.len(), large)?;
    out.extend_from_slice(data);
    Ok(())
}
fn tagged(out: &mut Vec<u8>, key: &[u8; 4], data: &[u8]) -> Result<(), String> {
    out.extend_from_slice(b"8BIM");
    out.extend_from_slice(key);
    // Layer-record tagged lengths include their even-byte padding. There is no
    // separate inter-block pad (global tagged blocks use a different rule).
    length(out, data.len() + data.len() % 2, false)?;
    out.extend_from_slice(data);
    if !data.len().is_multiple_of(2) {
        out.push(0);
    }
    Ok(())
}
fn zip_channel(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = vec![0, 2];
    let mut zip = flate2::write::ZlibEncoder::new(&mut out, flate2::Compression::fast());
    zip.write_all(data)
        .map_err(|e| format!("Could not encode Photoshop channel: {e}"))?;
    zip.finish()
        .map_err(|e| format!("Could not finish Photoshop channel: {e}"))?;
    Ok(out)
}

/// Export a standards-compliant RGB/8 PSD or PSB, with a compatibility composite.
/// Vectors and native filters are rendered per layer; groups and pixel masks stay separate.
pub fn encode(doc: &Document, large: bool) -> Result<PsdExport, String> {
    if !doc.width.is_finite() || !doc.height.is_finite() || doc.width < 1.0 || doc.height < 1.0 {
        return Err("Photoshop export needs a finite, nonempty canvas".into());
    }
    let w = doc.width.round() as u32;
    let h = doc.height.round() as u32;
    let edge_limit = if large { 300_000 } else { 30_000 };
    if w > edge_limit || h > edge_limit {
        return Err(format!(
            "{} supports canvas edges up to {edge_limit} pixels",
            if large { "PSB" } else { "PSD" }
        ));
    }
    dimensions(w, h)?;
    if doc.layers.len() > MAX_LAYERS {
        return Err("Photoshop export exceeds the 8192 layer limit".into());
    }
    let mut warnings = Vec::new();
    let mut records = Vec::new();
    let mut visited = HashSet::new();
    let mut used = 0usize;
    if !doc.transparent {
        let mut paper = Document::new("Paper", 1.0, 1.0, doc.dpi);
        paper.width = doc.width;
        paper.height = doc.height;
        paper.artboards = doc.artboards.clone();
        paper.layers.clear();
        let png = crate::compositor::export_png(&paper, 1)?;
        let pixels = image::load_from_memory(&png)
            .map_err(|e| format!("Could not create Photoshop paper layer: {e}"))?
            .into_rgba8();
        let mut record = Record {
            rect: Rect {
                top: 0,
                left: 0,
                bottom: h as i32,
                right: w as i32,
            },
            name: "Document paper".into(),
            blend: *b"norm",
            opacity: 255,
            flags: 9,
            section: 0,
            mask_rect: None,
            channels: Vec::new(),
            overlay: None,
        };
        add_pixels(&mut record, pixels.as_raw())?;
        records.push(record);
        used = dimensions(w, h)? * 8;
        warn(
            &mut warnings,
            "Document paper was exported as a separate background pixel layer.",
        );
    }
    collect(
        doc,
        None,
        0,
        &mut records,
        &mut visited,
        &mut used,
        &mut warnings,
    )?;
    if visited.len() != doc.layers.len() {
        return Err("Cannot export Photoshop: layer groups contain duplicate IDs, missing parents, or cycles".into());
    }
    if records.len() > MAX_LAYERS {
        return Err("Photoshop groups and layers exceed the 8192 record export limit".into());
    }
    // A layerless document still gets a layer so readers identify merged transparency.
    if records.is_empty() {
        records.push(Record {
            rect: Rect::default(),
            name: "Layer 1".into(),
            blend: *b"norm",
            opacity: 255,
            flags: 8,
            section: 0,
            mask_rect: None,
            channels: Vec::new(),
            overlay: None,
        });
    }
    let mut layer_info = Vec::new();
    u16(&mut layer_info, (-(records.len() as i16)) as u16);
    for record in &records {
        write_record(&mut layer_info, record, large)?;
    }
    for record in &records {
        for (_, channel) in &record.channels {
            layer_info.extend_from_slice(channel);
        }
    }
    if layer_info.len() % 2 != 0 {
        layer_info.push(0);
    }
    let mut layer_mask = Vec::new();
    section(&mut layer_mask, &layer_info, large)?;
    u32(&mut layer_mask, 0); // Global mask.
    let mut out = Vec::new();
    out.extend_from_slice(b"8BPS");
    u16(&mut out, if large { 2 } else { 1 });
    out.extend_from_slice(&[0; 6]);
    u16(&mut out, 4);
    u32(&mut out, h);
    u32(&mut out, w);
    u16(&mut out, 8);
    u16(&mut out, 3);
    u32(&mut out, 0); // RGB color-mode data.
    let mut resources = Vec::new();
    resources.extend_from_slice(b"8BIM");
    u16(&mut resources, 1005); // ResolutionInfo.
    resources.extend_from_slice(&[0, 0]);
    u32(&mut resources, 16);
    let dpi = if doc.dpi.is_finite() {
        doc.dpi.clamp(1.0, 65_535.0)
    } else {
        72.0
    };
    for _ in 0..2 {
        u32(&mut resources, (dpi * 65536.0).round() as u32);
        u16(&mut resources, 1);
        u16(&mut resources, 1);
    }
    section(&mut out, &resources, false)?;
    section(&mut out, &layer_mask, large)?;
    let png = crate::compositor::export_png(doc, 1)?;
    let merged = image::load_from_memory(&png)
        .map_err(|e| format!("Could not create Photoshop composite: {e}"))?
        .into_rgba8();
    if merged.width() != w || merged.height() != h {
        return Err("Photoshop composite dimensions do not match the canvas".into());
    }
    u16(&mut out, 2);
    let mut zipper = flate2::write::ZlibEncoder::new(&mut out, flate2::Compression::fast());
    let data = merged.as_raw();
    for ch in 0..4 {
        let plane: Vec<u8> = data.as_chunks::<4>().0.iter().map(|p| p[ch]).collect();
        zipper
            .write_all(&plane)
            .map_err(|e| format!("Could not encode Photoshop composite: {e}"))?;
    }
    zipper
        .finish()
        .map_err(|e| format!("Could not finish Photoshop composite: {e}"))?;
    Ok(PsdExport {
        bytes: out,
        warnings,
    })
}

fn collect(
    doc: &Document,
    parent: Option<u64>,
    depth: usize,
    records: &mut Vec<Record>,
    visited: &mut HashSet<u64>,
    used: &mut usize,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    if depth > 64 {
        return Err("Photoshop export exceeds the 64 level group nesting limit".into());
    }
    for layer in doc.layers.iter().filter(|layer| layer.parent == parent) {
        if !visited.insert(layer.id) {
            return Err("Duplicate Photoshop layer ID or group cycle".into());
        }
        if layer.is_group {
            records.push(Record {
                rect: Rect::default(),
                name: "</Layer group>".into(),
                blend: *b"norm",
                opacity: 255,
                flags: 8,
                section: 3,
                mask_rect: None,
                channels: Vec::new(),
                overlay: None,
            });
            collect(
                doc,
                Some(layer.id),
                depth + 1,
                records,
                visited,
                used,
                warnings,
            )?;
        }
        let mut record = Record {
            rect: Rect::default(),
            name: layer.name.clone(),
            blend: if layer.is_group && layer.pass_through {
                *b"pass"
            } else {
                *encode_blend(layer.blend)
            },
            opacity: (layer.opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
            flags: 8 | if layer.visible { 0 } else { 2 } | u8::from(layer.locked),
            section: if layer.is_group { 1 } else { 0 },
            mask_rect: None,
            channels: Vec::new(),
            overlay: super::effects::overlay(&layer.filters),
        };
        if layer.is_group {
            if layer.filters.active() && record.overlay.is_none() {
                warn(
                    warnings,
                    "Filters on groups are not encoded as Photoshop effects; the compatibility composite retains their appearance.",
                );
            }
            if let Some(mask) = &layer.mask {
                let (pixels, bounds) = placed_group_mask(layer, mask, warnings)?;
                *used = used
                    .checked_add(pixels.data.len())
                    .ok_or("Photoshop mask export size overflow")?;
                if *used > MAX_DECODED_BYTES {
                    return Err(
                        "Photoshop export masks exceed the 512 MiB working size limit".into(),
                    );
                }
                add_mask(&mut record, &pixels, bounds)?;
            }
        } else if !matches!(&layer.kind, LayerKind::Vector { shapes } if shapes.is_empty())
            || layer.mask.is_some()
            || layer.filters.active()
        {
            let (pixels, bounds, mask) =
                layer_pixels(doc, layer, record.overlay.is_some(), warnings)?;
            let amount = pixels
                .data
                .len()
                .checked_mul(2)
                .ok_or("Photoshop export layer size overflow")?;
            *used = used
                .checked_add(amount)
                .ok_or("Photoshop export size overflow")?;
            if *used > MAX_DECODED_BYTES {
                return Err("Photoshop export layers exceed the 512 MiB working size limit".into());
            }
            record.rect = bounds;
            add_pixels(&mut record, &pixels.data)?;
            if let Some(mask) = mask {
                add_mask(&mut record, &mask, bounds)?;
            }
        }
        records.push(record);
    }
    Ok(())
}

fn add_pixels(record: &mut Record, rgba: &[u8]) -> Result<(), String> {
    for ch in [-1i16, 0, 1, 2] {
        let component = if ch == -1 { 3 } else { ch as usize };
        let data: Vec<u8> = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[component])
            .collect();
        record.channels.push((ch, zip_channel(&data)?));
    }
    Ok(())
}

fn placed_group_mask(
    layer: &Layer,
    mask: &Pixels,
    warnings: &mut Vec<String>,
) -> Result<(Pixels, Rect), String> {
    if mask.w == 0 || mask.h == 0 || mask.data.len() != dimensions(mask.w, mask.h)? * 4 {
        return Err("Invalid native group mask buffer".into());
    }
    let placed = layer.mask_size.x > 0.0 && layer.mask_size.y > 0.0;
    let origin = if placed {
        layer.mask_origin
    } else {
        crate::geom::Pt::ZERO
    };
    let size = if placed {
        layer.mask_size
    } else {
        crate::geom::Pt::new(mask.w as f32, mask.h as f32)
    };
    if !origin.x.is_finite()
        || !origin.y.is_finite()
        || !size.x.is_finite()
        || !size.y.is_finite()
        || origin.x.abs() > 1_000_000.0
        || origin.y.abs() > 1_000_000.0
        || size.x > 300_000.0
        || size.y > 300_000.0
    {
        return Err("Invalid Photoshop group mask position or size".into());
    }
    let w = size.x.round().max(1.0) as u32;
    let h = size.y.round().max(1.0) as u32;
    dimensions(w, h)?;
    let left = origin.x.round() as i32;
    let top = origin.y.round() as i32;
    let bounds = Rect {
        left,
        top,
        right: left + w as i32,
        bottom: top + h as i32,
    };
    if origin.x != left as f32 || origin.y != top as f32 || size.x != w as f32 || size.y != h as f32
    {
        warn(
            warnings,
            "Fractional group mask bounds were rounded to Photoshop pixel coordinates.",
        );
    }
    let pixels = if (mask.w, mask.h) == (w, h) {
        mask.clone()
    } else {
        let image = image::RgbaImage::from_raw(mask.w, mask.h, mask.data.clone())
            .ok_or("Invalid native group mask")?;
        let resized = image::imageops::resize(&image, w, h, image::imageops::FilterType::Triangle);
        Pixels::from_rgba(w, h, resized.into_raw()).ok_or("Invalid resized group mask")?
    };
    Ok((pixels, bounds))
}

fn add_mask(record: &mut Record, mask: &Pixels, bounds: Rect) -> Result<(), String> {
    if mask.data.len() != dimensions(mask.w, mask.h)? * 4 {
        return Err("Invalid native pixel mask buffer".into());
    }
    record.mask_rect = Some(bounds);
    // The compositor uses premultiplied luminance; match it for colored masks too.
    let values: Vec<u8> = mask
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            let luminance = (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32)
                * p[3] as f32
                / 255.0;
            luminance.round() as u8
        })
        .collect();
    record.channels.push((-2, zip_channel(&values)?));
    Ok(())
}

fn layer_pixels(
    doc: &Document,
    layer: &Layer,
    preserve_overlay: bool,
    warnings: &mut Vec<String>,
) -> Result<(Pixels, Rect, Option<Pixels>), String> {
    if let LayerKind::Raster {
        pixels,
        origin,
        size,
        rotation,
    } = &layer.kind
    {
        let no_resize = size.x.abs() <= 0.5 && size.y.abs() <= 0.5
            || (size.x - pixels.w as f32).abs() < 0.001 && (size.y - pixels.h as f32).abs() < 0.001;
        if no_resize
            && rotation.abs() < 0.00001
            && (origin.x - origin.x.round()).abs() < 0.001
            && (origin.y - origin.y.round()).abs() < 0.001
            && (!layer.filters.active() || preserve_overlay)
        {
            if !origin.x.is_finite()
                || !origin.y.is_finite()
                || origin.x.abs() > 1_000_000.0
                || origin.y.abs() > 1_000_000.0
            {
                return Err("Invalid Photoshop layer position".into());
            }
            if pixels.w == 0
                || pixels.h == 0
                || pixels.data.len() != dimensions(pixels.w, pixels.h)? * 4
            {
                return Err("Invalid native raster buffer".into());
            }
            let left = origin.x.round() as i32;
            let top = origin.y.round() as i32;
            let bounds = Rect {
                top,
                left,
                bottom: top + pixels.h as i32,
                right: left + pixels.w as i32,
            };
            if let Some(mask) = &layer.mask {
                if mask.w == pixels.w && mask.h == pixels.h {
                    return Ok((pixels.clone(), bounds, Some(mask.clone())));
                }
            } else {
                return Ok((pixels.clone(), bounds, None));
            }
        }
    }
    if matches!(layer.kind, LayerKind::Vector { .. }) {
        warn(
            warnings,
            "Vector artwork and text are rasterized individually in Photoshop export; save .oma to retain native editing.",
        );
    } else {
        warn(
            warnings,
            "Transformed pixels, mismatched masks, and native filters are rendered into their Photoshop layer pixels.",
        );
    }
    let mut single = Document::new("Photoshop layer", 1.0, 1.0, doc.dpi);
    single.width = doc.width;
    single.height = doc.height;
    single.artboards = doc.artboards.clone();
    single.transparent = true;
    let mut isolated = layer.clone();
    isolated.parent = None;
    isolated.visible = true;
    isolated.opacity = 1.0;
    isolated.blend = Blend::Normal;
    if preserve_overlay {
        isolated.filters = Default::default();
    }
    single.layers = vec![isolated];
    let png = crate::compositor::export_png(&single, 1)?;
    let rgba = image::load_from_memory(&png)
        .map_err(|e| format!("Could not render Photoshop layer: {e}"))?
        .into_rgba8();
    let w = rgba.width();
    let h = rgba.height();
    let pixels =
        Pixels::from_rgba(w, h, rgba.into_raw()).ok_or("Invalid rendered Photoshop layer")?;
    Ok((
        pixels,
        Rect {
            top: 0,
            left: 0,
            bottom: h as i32,
            right: w as i32,
        },
        None,
    ))
}

fn write_record(out: &mut Vec<u8>, record: &Record, large: bool) -> Result<(), String> {
    rect(out, record.rect);
    u16(out, record.channels.len() as u16);
    for (id, data) in &record.channels {
        u16(out, *id as u16);
        length(out, data.len(), large)?;
    }
    out.extend_from_slice(b"8BIM");
    out.extend_from_slice(&record.blend);
    out.extend_from_slice(&[record.opacity, 0, record.flags, 0]);
    let mut extra = Vec::new();
    if let Some(bounds) = record.mask_rect {
        u32(&mut extra, 20);
        rect(&mut extra, bounds);
        extra.extend_from_slice(&[0, 0, 0, 0]);
    } else {
        u32(&mut extra, 0);
    }
    u32(&mut extra, 0); // Blend ranges.
    let legacy: Vec<u8> = record
        .name
        .chars()
        .take(255)
        .map(|c| if c.is_ascii() { c as u8 } else { b'?' })
        .collect();
    extra.push(legacy.len() as u8);
    extra.extend_from_slice(&legacy);
    extra.resize(extra.len() + (4 - (legacy.len() + 1) % 4) % 4, 0);
    let units: Vec<u16> = record.name.encode_utf16().take(32_768).collect();
    let mut unicode = Vec::new();
    u32(&mut unicode, units.len() as u32);
    for unit in units {
        u16(&mut unicode, unit);
    }
    tagged(&mut extra, b"luni", &unicode)?;
    if record.flags & 1 != 0 {
        tagged(&mut extra, b"lspf", &7u32.to_be_bytes())?;
    }
    if let Some(overlay) = &record.overlay {
        let (legacy, modern) = super::effects::encode(overlay);
        tagged(&mut extra, b"lrFX", &legacy)?;
        tagged(&mut extra, b"lfx2", &modern)?;
    }
    if record.section != 0 {
        let mut divider = Vec::new();
        u32(&mut divider, record.section);
        divider.extend_from_slice(b"8BIM");
        divider.extend_from_slice(&record.blend);
        tagged(&mut extra, b"lsct", &divider)?;
    }
    section(out, &extra, false)
}
