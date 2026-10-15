use super::{
    Document, Layer, LayerKind, MAX_LAYERS, Pixels, PsdExport, Rect, dimensions,
    encode_blend, warn,
};
use crate::color::Blend;
use std::collections::HashSet;
use std::io::Write;
use std::sync::Arc;
use crate::{resources::{self, Allocation}, deep_color::storage};

struct Record {
    rect: Rect,
    name: String,
    blend: [u8; 4],
    opacity: u8,
    flags: u8,
    section: u32,
    mask_rect: Option<Rect>,
    channels: Vec<(i16, Arc<Allocation<u8>>)>,
    overlay: Option<super::effects::Overlay>,
    type_data: Option<Vec<u8>>,
}

fn u16(out: &mut impl Write, value: u16) -> Result<(), String> {out.write_all(&value.to_be_bytes()).map_err(|e| e.to_string())}
fn u32(out: &mut impl Write, value: u32) -> Result<(), String> {out.write_all(&value.to_be_bytes()).map_err(|e| e.to_string())}
fn i32(out: &mut impl Write, value: i32) -> Result<(), String> {out.write_all(&value.to_be_bytes()).map_err(|e| e.to_string())}
fn rect(out: &mut impl Write, value: Rect) -> Result<(), String> {
    for v in [value.top, value.left, value.bottom, value.right] {i32(out, v)?;} Ok(())
}
fn length(out: &mut impl Write, n: usize, large: bool) -> Result<(), String> {
    if large {out.write_all(&(n as u64).to_be_bytes()).map_err(|e| e.to_string())?;} else {u32(out, u32::try_from(n).map_err(|_| "Photoshop section exceeds PSD's 4 GiB limit; export PSB")?)?;} Ok(())
}
fn section(out: &mut impl Write, data: &[u8], large: bool) -> Result<(), String> {
    length(out, data.len(), large)?;out.write_all(data).map_err(|e| e.to_string())
}
fn tagged(out: &mut impl Write, key: &[u8; 4], data: &[u8]) -> Result<(), String> {
    out.write_all(b"8BIM").map_err(|e| e.to_string())?;
    out.write_all(key).map_err(|e| e.to_string())?;
    length(out, data.len() + data.len() % 2, false)?;
    out.write_all(data).map_err(|e| e.to_string())?;
    if !data.len().is_multiple_of(2) {out.write_all(&[0]).map_err(|e| e.to_string())?;} Ok(())
}
fn zip_channel(data: &[u8]) -> Result<Arc<Allocation<u8>>, String> {
    let _codec = resources::global().reserve_now(64 * 1024, &storage::check)?;
    let mut out = storage::Builder::new()?;out.extend(&[0, 2])?;
    let mut zip = flate2::write::ZlibEncoder::new(out, flate2::Compression::fast());
    zip.write_all(data).map_err(|e| format!("Could not encode Photoshop channel: {e}"))?;
    zip.finish().map_err(|e| format!("Could not finish Photoshop channel: {e}"))?.finish()
}

/// Export a standards-compliant RGB/8 PSD or PSB, with a compatibility composite.
/// Vectors and native filters are rendered per layer; groups and pixel masks stay separate.
pub fn encode(doc: &Document, large: bool) -> Result<PsdExport, String> {
    if doc.layers.iter().any(|l| {
        l.kind.shapes().is_some_and(|ss| {
            ss.len() > 1
                && ss
                    .iter()
                    .any(|s| matches!(s.geom, crate::geom::Geom::Text(_)))
        })
    }) {
        let mut prepared = doc.clone();
        prepared.layers.clear();
        for layer in &doc.layers {
            if let Some(shapes) = layer.kind.shapes().filter(|ss| {
                ss.len() > 1
                    && ss
                        .iter()
                        .any(|s| matches!(s.geom, crate::geom::Geom::Text(_)))
            }) {
                let mut group = layer.clone();
                group.kind = LayerKind::Vector { shapes: vec![] };
                group.is_group = true;
                group.pass_through = false;
                for shape in shapes {
                    let mut child = Layer::vector(&shape.name);
                    child.parent = Some(group.id);
                    child.kind = LayerKind::Vector {
                        shapes: vec![shape.clone()],
                    };
                    prepared.layers.push(child);
                }
                prepared.layers.push(group);
            } else {
                prepared.layers.push(layer.clone());
            }
        }
        return encode(&prepared, large);
    }
    if !doc.mask_sources().is_empty() {
        if doc.layers.iter().any(|l| {
            l.kind.pixels().is_some_and(|p| p.native.is_some())
                || l.mask.as_ref().is_some_and(|p| p.native.is_some())
        }) {
            return Err("High-depth artwork masks cannot be flattened by the 8-bit compatibility renderer. Export .oma to retain original samples and editable artwork masks.".into());
        }
        let prepared = crate::compositor::raster_export_document(doc)?;
        let mut result = encode(&prepared, large)?;
        result.warnings.push("Live artwork masks were rendered into their affected layers or groups. Save .oma or export SVG to retain editable mask sources.".into());
        return Ok(result);
    }
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
    let depth = crate::deep_color::document_depth(doc).bits();
    let mut warnings = Vec::new();
    if doc.layers.iter().any(|l| {
        l.filters.independent()
            || l.kind
                .shapes()
                .is_some_and(|ss| ss.iter().any(|s| s.filters.independent()))
    }) {
        warn(
            &mut warnings,
            "Per-effect blend modes were flattened into each Photoshop pixel layer against transparency. The compatibility composite preserves the complete document backdrop; edit native effects in the .oma project.",
        );
    }
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
            type_data: None,
        };
        add_pixels(
            &mut record,
            &Pixels::from_rgba(w, h, pixels.into_raw()).ok_or("Invalid paper")?,
            depth,
            &doc.color,
        )?;
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
        depth,
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
            type_data: None,
        });
    }
    let mut layer_info = storage::Builder::new()?;
    u16(&mut layer_info, (-(records.len() as i16)) as u16)?;
    for record in &records {
        write_record(&mut layer_info, record, large)?;
    }
    for record in &records {
        for (_, channel) in &record.channels {
            layer_info.extend(channel)?;
        }
    }
    if layer_info.len() % 2 != 0 {layer_info.extend(&[0])?;}
    let layer_info = layer_info.finish()?;
    drop(records);
    let mut out = storage::Encoded::new()?;
    out.write_all(b"8BPS").map_err(|e| e.to_string())?;
    u16(&mut out, if large { 2 } else { 1 })?;
    out.write_all(&[0; 6]).map_err(|e| e.to_string())?;
    u16(&mut out, if doc.color.model == crate::deep_color::cms::Model::Cmyk {5} else {4})?;
    u32(&mut out, h)?;
    u32(&mut out, w)?;
    u16(&mut out, depth)?;
    u16(&mut out, if doc.color.model == crate::deep_color::cms::Model::Cmyk {4} else {3})?;
    u32(&mut out, 0)?; // RGB color-mode data.
    let mut resources = Vec::new();
    resources.extend_from_slice(b"8BIM");
    u16(&mut resources, 1005)?; // ResolutionInfo.
    resources.extend_from_slice(&[0, 0]);
    u32(&mut resources, 16)?;
    let dpi = if doc.dpi.is_finite() {
        doc.dpi.clamp(1.0, 65_535.0)
    } else {
        72.0
    };
    for _ in 0..2 {
        u32(&mut resources, (dpi * 65536.0).round() as u32)?;
        u16(&mut resources, 1)?;
        u16(&mut resources, 1)?;
    }
    if !doc.color.icc_profile.is_empty() {
        crate::deep_color::cms::validate_profile(&doc.color.icc_profile, doc.color.model)?;
        resources.extend_from_slice(b"8BIM");
        u16(&mut resources, 1039)?;
        resources.extend_from_slice(&[0, 0]);
        section(&mut resources, &doc.color.icc_profile, false)?;
        if doc.color.icc_profile.len() % 2 != 0 {
            resources.push(0);
        }
    }
    section(&mut out, &resources, false)?;
    let mask_size = layer_info.len().checked_add(if large {12} else {8}).ok_or("Photoshop layer section size overflows")?;
    length(&mut out, mask_size, large)?;
    section(&mut out, &layer_info, large)?;
    u32(&mut out, 0)?;
    drop(layer_info);
    let merged = if depth == 8 && doc.color.model == crate::deep_color::cms::Model::Rgb && doc.color.icc_profile.is_empty() {
        let png = crate::compositor::export_png(doc, 1)?;
        let image = image::load_from_memory(&png)
            .map_err(|e| e.to_string())?
            .into_rgba8();
        Pixels::from_rgba(w, h, image.into_raw()).ok_or("Invalid merged image")?
    } else {
        crate::deep_color::composite(doc, 1)?
    };
    u16(&mut out, 0)?;
    let merged = merged.in_document_color(&doc.color)?;
    for ch in 0..if doc.color.model == crate::deep_color::cms::Model::Cmyk {5} else {4} {
        out.write_all(&plane(&merged, ch, depth, doc.color.model == crate::deep_color::cms::Model::Rgb)?).map_err(|e| e.to_string())?;
    }
    Ok(PsdExport {
        bytes: {let output = Arc::try_unwrap(out.finish()?).map_err(|_| "Shared Photoshop encoded storage")?; let (bytes, _memory) = output.into_parts(); bytes},
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
    sample_depth: u16,
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
                type_data: None,
            });
            collect(
                doc,
                Some(layer.id),
                depth + 1,
                records,
                visited,
                used,
                warnings,
                sample_depth,
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
            opacity: ((layer.opacity
                * if layer.is_group && !layer.filters.active() {
                    layer.fill_opacity
                } else {
                    1.
                })
            .clamp(0.0, 1.0)
                * 255.0)
                .round() as u8,
            flags: 8 | if layer.visible { 0 } else { 2 } | u8::from(layer.locked),
            section: if layer.is_group { 1 } else { 0 },
            mask_rect: None,
            channels: Vec::new(),
            overlay: super::effects::overlay(&layer.filters),
            type_data: None,
        };
        if let Some(shape) = layer
            .kind
            .shapes()
            .filter(|ss| ss.len() == 1)
            .and_then(|ss| ss.first())
            .filter(|s| matches!(s.geom, crate::geom::Geom::Text(_)))
        {
            record.type_data = Some(super::type_layer::encode(
                shape,
                layer.psd_type.as_ref(),
                warnings,
            )?);
            record.opacity =
                (record.opacity as f32 * shape.opacity * shape.fill_opacity).round() as u8;
        }
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
                add_mask(&mut record, &pixels, bounds, sample_depth)?;
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
            record.rect = bounds;
            add_pixels(&mut record, &pixels, sample_depth, &doc.color)?;
            if let Some(mask) = mask {
                add_mask(&mut record, &mask, bounds, sample_depth)?;
            }
        }
        records.push(record);
    }
    Ok(())
}

fn plane(pixels: &Pixels, component: usize, depth: u16, white_matte: bool) -> Result<Arc<Allocation<u8>>, String> {
    let count = dimensions(pixels.w, pixels.h)?.checked_mul(depth as usize / 8).ok_or("Photoshop channel byte size overflows")?;
    let mut out = resources::global().allocate_now::<u8, _>(count, &storage::check)?;
    for (i, bytes) in out.as_mut_slice().chunks_exact_mut(depth as usize / 8).enumerate() {
        let sample = pixels.sample(i);
        let value = if let Some(inks) = pixels.native.as_ref().and_then(|n| n.cmyk.as_ref()) {if component == 4 {inks[i*5+4]} else {1. - inks[i*5+component]}} else if white_matte && component < 3 {sample[component] * sample[3] + (1. - sample[3])} else {sample[component]};
        match depth {8 => bytes[0] = crate::deep_color::byte(value),16 => bytes.copy_from_slice(&((value.clamp(0., 1.) * 65535.).round() as u16).to_be_bytes()),_ => bytes.copy_from_slice(&value.to_be_bytes())}
    } Ok(Arc::new(out))
}

fn add_pixels(record: &mut Record, pixels: &Pixels, depth: u16, color: &crate::deep_color::cms::DocumentColor) -> Result<(), String> {
    let pixels = pixels.in_document_color(color)?;
    let cmyk = color.model == crate::deep_color::cms::Model::Cmyk;
    for &ch in if cmyk {&[-1i16, 0, 1, 2, 3][..]} else {&[-1i16, 0, 1, 2][..]} {
        record.channels.push((
            ch,
            zip_channel(&plane(
                &pixels,
                if ch == -1 {
                    if cmyk { 4 } else { 3 }
                } else {
                    ch as usize
                },
                depth,
                false,
            )?)?,
        ));
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
    } else if mask.native.is_some() {
        mask.resized(w, h)?
    } else {
        let image = image::RgbaImage::from_raw(mask.w, mask.h, mask.data.to_vec())
            .ok_or("Invalid native group mask")?;
        let resized = image::imageops::resize(&image, w, h, image::imageops::FilterType::Triangle);
        Pixels::from_rgba(w, h, resized.into_raw()).ok_or("Invalid resized group mask")?
    };
    Ok((pixels, bounds))
}

fn add_mask(record: &mut Record, mask: &Pixels, bounds: Rect, depth: u16) -> Result<(), String> {
    if mask.data.len() != dimensions(mask.w, mask.h)? * 4 {
        return Err("Invalid native pixel mask buffer".into());
    }
    record.mask_rect = Some(bounds);
    let mut values = storage::Builder::with_capacity(dimensions(mask.w, mask.h)?.checked_mul(depth as usize / 8).ok_or("Photoshop mask byte size overflows")?)?;
    for i in 0..mask.w as usize * mask.h as usize {
        let p = mask.sample(i);
        let value = ((p[0] * 0.2126 + p[1] * 0.7152 + p[2] * 0.0722) * p[3]).clamp(0., 1.);
        match depth {
            8 => values.extend(&[crate::deep_color::byte(value)])?,
            16 => values.extend(&((value * 65535.).round() as u16).to_be_bytes())?,
            _ => values.extend(&value.to_be_bytes())?,
        };
    }
    record.channels.push((-2, zip_channel(&values.finish()?)?));
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
        ..
    } = &layer.kind
    {
        let no_resize = size.x.abs() <= 0.5 && size.y.abs() <= 0.5
            || (size.x - pixels.w as f32).abs() < 0.001 && (size.y - pixels.h as f32).abs() < 0.001;
        if no_resize
            && layer.fill_opacity >= 1.
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
    if matches!(&layer.kind, LayerKind::Vector { shapes } if !shapes.iter().all(|s|matches!(s.geom,crate::geom::Geom::Text(_))))
    {
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
    single.color = doc.color.clone();
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
    let pixels = if doc
        .layers
        .iter()
        .any(|l| l.kind.pixels().is_some_and(|p| p.native.is_some()))
    {
        crate::deep_color::composite(&single, 1)?
    } else {
        let png = crate::compositor::export_png(&single, 1)?;
        let rgba = image::load_from_memory(&png)
            .map_err(|e| e.to_string())?
            .into_rgba8();
        Pixels::from_rgba(rgba.width(), rgba.height(), rgba.into_raw())
            .ok_or("Invalid layer render")?
    };
    let w = pixels.w;
    let h = pixels.h;
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

fn write_record(out: &mut impl Write, record: &Record, large: bool) -> Result<(), String> {
    rect(out, record.rect)?;
    u16(out, record.channels.len() as u16)?;
    for (id, data) in &record.channels {
        u16(out, *id as u16)?;
        length(out, data.len(), large)?;
    }
    out.write_all(b"8BIM").map_err(|e| e.to_string())?;
    out.write_all(&record.blend).map_err(|e| e.to_string())?;
    out.write_all(&[record.opacity, 0, record.flags, 0]).map_err(|e| e.to_string())?;
    let mut extra = Vec::new();
    if let Some(bounds) = record.mask_rect {
        u32(&mut extra, 20)?;
        rect(&mut extra, bounds)?;
        extra.extend_from_slice(&[0, 0, 0, 0]);
    } else {
        u32(&mut extra, 0)?;
    }
    u32(&mut extra, 0)?; // Blend ranges.
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
    u32(&mut unicode, units.len() as u32)?;
    for unit in units {
        u16(&mut unicode, unit)?;
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
        u32(&mut divider, record.section)?;
        divider.extend_from_slice(b"8BIM");
        divider.extend_from_slice(&record.blend);
        tagged(&mut extra, b"lsct", &divider)?;
    }
    if let Some(data) = &record.type_data {
        tagged(&mut extra, b"TySh", data)?;
    }
    section(out, &extra, false)
}
