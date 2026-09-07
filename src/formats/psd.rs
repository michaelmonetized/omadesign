//! Layered PSD/PSB interchange, based on Adobe's Photoshop File Formats Specification.
//!
//! https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/
//! Parsing is bounded and never invokes a shell or an external converter.

mod codec;
mod effects;

use crate::color::Blend;
use crate::document::{Document, Layer, LayerKind, Pixels};
use crate::geom::Pt;

pub struct PsdImport {
    pub document: Document,
    pub warnings: Vec<String>,
}

pub struct PsdExport {
    pub bytes: Vec<u8>,
    pub warnings: Vec<String>,
}

const MAX_PIXELS: usize = 64 * 1024 * 1024;
const MAX_DECODED_BYTES: usize = 512 * 1024 * 1024;
const MAX_LAYERS: usize = 8192;

fn warn(warnings: &mut Vec<String>, text: impl Into<String>) {
    let text = text.into();
    if !warnings.contains(&text) {
        warnings.push(text);
    }
}

fn dimensions(w: u32, h: u32) -> Result<usize, String> {
    let pixels = (w as usize)
        .checked_mul(h as usize)
        .ok_or("Photoshop dimensions overflow")?;
    if pixels > MAX_PIXELS {
        return Err("Photoshop image exceeds the 64 megapixel import/export limit".into());
    }
    Ok(pixels)
}

#[derive(Clone, Copy, Debug, Default)]
struct Rect {
    top: i32,
    left: i32,
    bottom: i32,
    right: i32,
}

impl Rect {
    fn size(self) -> Result<(u32, u32), String> {
        let w = i64::from(self.right) - i64::from(self.left);
        let h = i64::from(self.bottom) - i64::from(self.top);
        if !(0..=300_000).contains(&w) || !(0..=300_000).contains(&h) {
            return Err("Invalid Photoshop layer rectangle".into());
        }
        dimensions(w as u32, h as u32)?;
        Ok((w as u32, h as u32))
    }
}

fn decode_blend(key: [u8; 4], warnings: &mut Vec<String>) -> Blend {
    match &key {
        b"norm" | b"pass" => Blend::Normal,
        b"mul " => Blend::Multiply,
        b"scrn" => Blend::Screen,
        b"over" => Blend::Overlay,
        b"dark" => Blend::Darken,
        b"lite" => Blend::Lighten,
        b"div " => Blend::ColorDodge,
        b"idiv" => Blend::ColorBurn,
        b"hLit" => Blend::HardLight,
        b"sLit" => Blend::SoftLight,
        b"diff" => Blend::Difference,
        b"smud" => Blend::Exclusion,
        b"hue " => Blend::Hue,
        b"sat " => Blend::Saturation,
        b"colr" => Blend::Color,
        b"lum " => Blend::Luminosity,
        _ => {
            warn(
                warnings,
                format!(
                    "Blend mode '{}' is displayed as Normal.",
                    String::from_utf8_lossy(&key)
                ),
            );
            Blend::Normal
        }
    }
}

fn encode_blend(blend: Blend) -> &'static [u8; 4] {
    match blend {
        Blend::Normal => b"norm",
        Blend::Multiply => b"mul ",
        Blend::Screen => b"scrn",
        Blend::Overlay => b"over",
        Blend::Darken => b"dark",
        Blend::Lighten => b"lite",
        Blend::ColorDodge => b"div ",
        Blend::ColorBurn => b"idiv",
        Blend::HardLight => b"hLit",
        Blend::SoftLight => b"sLit",
        Blend::Difference => b"diff",
        Blend::Exclusion => b"smud",
        Blend::Hue => b"hue ",
        Blend::Saturation => b"sat ",
        Blend::Color => b"colr",
        Blend::Luminosity => b"lum ",
    }
}

/// Read saved raster channels and reconstruct the Photoshop layer tree.
pub fn decode(bytes: &[u8], name: &str) -> Result<PsdImport, String> {
    let parsed = codec::parse(bytes)?;
    // Account for independent group masks, whose canvas dimensions may be much
    // larger than the (often empty) group record rectangle.
    let mut decoded_bytes = 0usize;
    for record in &parsed.layers {
        let (w, h) = record.rect.size()?;
        let rgba_bytes = dimensions(w, h)?
            .checked_mul(4)
            .ok_or("Photoshop layer size overflow")?;
        let mask_bytes = if record.mask.is_some() {
            let mask_pixels = if record.section == 1 || record.section == 2 {
                dimensions(parsed.width, parsed.height)?
            } else {
                dimensions(w, h)?
            };
            mask_pixels
                .checked_mul(4)
                .ok_or("Photoshop mask size overflow")?
        } else {
            0
        };
        let channel_bytes: usize = record.pixels.iter().map(|(_, data)| data.len()).sum();
        decoded_bytes = decoded_bytes
            .checked_add(rgba_bytes)
            .and_then(|n| n.checked_add(mask_bytes))
            .and_then(|n| n.checked_add(channel_bytes))
            .ok_or("Photoshop document size overflow")?;
        if decoded_bytes > MAX_DECODED_BYTES {
            return Err(
                "Photoshop layers and masks exceed the 512 MiB decoded import limit".into(),
            );
        }
    }
    let mut warnings = parsed.warnings;
    // Avoid allocating a disposable full-canvas background before loading layers.
    let mut doc = Document::new(name, 1.0, 1.0, parsed.dpi);
    doc.width = parsed.width as f32;
    doc.height = parsed.height as f32;
    doc.transparent = true;
    doc.artboards = crate::document::Artboard::tiled(1, doc.width, doc.height);
    doc.layers.clear();
    let mut stack: Vec<Layer> = Vec::new();
    let mut clipping: Vec<u64> = Vec::new();
    for record in parsed.layers {
        if record.section == 3 {
            if stack.len() >= 64 {
                return Err("Photoshop groups exceed the 64 level nesting limit".into());
            }
            let mut group = Layer::group("Group");
            group.parent = stack.last().map(|g| g.id);
            stack.push(group);
            continue;
        }
        let mut layer = if record.section == 1 || record.section == 2 {
            let mut group = stack
                .pop()
                .ok_or("Photoshop group end has no matching start")?;
            group.name = record.name.clone();
            group.pass_through = &record.blend == b"pass";
            group
        } else {
            let (w, h) = record.rect.size()?;
            if w == 0 || h == 0 {
                let mut layer = Layer::vector(&record.name);
                layer.parent = stack.last().map(|g| g.id);
                layer
            } else {
                let rgba = rgba_channels(w, h, parsed.mode, |id| {
                    record
                        .pixels
                        .iter()
                        .find(|(key, _)| *key == id)
                        .map(|(_, data)| data.as_slice())
                })?;
                let pixels =
                    Pixels::from_rgba(w, h, rgba).ok_or("Invalid Photoshop layer pixels")?;
                let mut layer = Layer::placed_raster(
                    &record.name,
                    pixels,
                    Pt::new(record.rect.left as f32, record.rect.top as f32),
                    Pt::new(w as f32, h as f32),
                );
                layer.parent = stack.last().map(|g| g.id);
                layer
            }
        };
        layer.visible = record.flags & 2 == 0;
        layer.locked = record.flags & 1 != 0;
        layer.opacity = record.opacity as f32 / 255.0;
        layer.blend = decode_blend(record.blend, &mut warnings);
        layer.filters = record.filters.clone();
        let mask_entry = record.pixels.iter().find(|(id, _)| *id == -2);
        if let (Some(mask), Some((_, values))) = (&record.mask, mask_entry) {
            if mask.flags & 2 == 0 {
                let bounds = if layer.is_group {
                    Rect {
                        top: 0,
                        left: 0,
                        bottom: parsed.height as i32,
                        right: parsed.width as i32,
                    }
                } else {
                    record.rect
                };
                layer.mask = Some(convert_mask(mask, values, bounds, record.rect)?);
            } else {
                warn(
                    &mut warnings,
                    "Disabled Photoshop pixel masks are not attached to imported layers.",
                );
            }
        }
        if record.clipping {
            clipping.push(layer.id);
        }
        doc.layers.push(layer);
    }
    if !stack.is_empty() {
        return Err("Photoshop group start has no matching end".into());
    }
    if let Some(channels) = parsed.merged {
        let color_count = codec::color_channels(parsed.mode);
        let rgba = rgba_channels(parsed.width, parsed.height, parsed.mode, |id| {
            let idx = if id == -1 { color_count } else { id as usize };
            channels.get(idx).map(Vec::as_slice)
        })?;
        let pixels = Pixels::from_rgba(parsed.width, parsed.height, rgba)
            .ok_or("Invalid Photoshop merged pixels")?;
        doc.layers.push(Layer::placed_raster(
            "Merged image",
            pixels,
            Pt::ZERO,
            Pt::new(parsed.width as f32, parsed.height as f32),
        ));
        warn(
            &mut warnings,
            "This Photoshop file contains only a merged image; no separate layers were saved.",
        );
    }
    if !clipping.is_empty() {
        bake_clipping(&mut doc, &clipping, &mut warnings)?;
    }
    Ok(PsdImport {
        document: doc,
        warnings,
    })
}

fn rgba_channels<'a>(
    w: u32,
    h: u32,
    mode: u16,
    channel: impl Fn(i16) -> Option<&'a [u8]>,
) -> Result<Vec<u8>, String> {
    let n = dimensions(w, h)?;
    let r = channel(0);
    let g = channel(1);
    let b = channel(2);
    let k = channel(3);
    let alpha = channel(-1);
    for samples in [r, g, b, k, alpha].into_iter().flatten() {
        if samples.len() != n {
            return Err("Photoshop channel dimensions do not match the layer".into());
        }
    }
    let mut rgba = vec![0u8; n * 4];
    for (i, pixel) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        if let Some(red) = r {
            let green = g.map_or(red[i], |data| data[i]);
            let blue = b.map_or(red[i], |data| data[i]);
            if mode == 4 {
                // PSD CMYK channel values are inverted ink coverage.
                let black = k.map_or(255, |data| data[i]) as u16;
                pixel[..3].copy_from_slice(&[
                    (red[i] as u16 * black / 255) as u8,
                    (green as u16 * black / 255) as u8,
                    (blue as u16 * black / 255) as u8,
                ]);
            } else {
                pixel[..3].copy_from_slice(&[red[i], green, blue]);
            }
            pixel[3] = alpha.map_or(255, |data| data[i]);
        }
    }
    Ok(rgba)
}

fn convert_mask(
    mask: &codec::Mask,
    values: &[u8],
    bounds: Rect,
    layer_bounds: Rect,
) -> Result<Pixels, String> {
    let (w, h) = bounds.size()?;
    let (mw, mh) = mask.rect.size()?;
    if values.len() != dimensions(mw, mh)? {
        return Err("Photoshop mask size mismatch".into());
    }
    if w == 0 || h == 0 {
        return Ok(Pixels::new(1, 1));
    }
    let mut data = vec![0u8; dimensions(w, h)? * 4];
    let offset_x = i64::from(mask.rect.left)
        + if mask.flags & 1 != 0 {
            i64::from(layer_bounds.left)
        } else {
            0
        };
    let offset_y = i64::from(mask.rect.top)
        + if mask.flags & 1 != 0 {
            i64::from(layer_bounds.top)
        } else {
            0
        };
    for y in 0..h {
        for x in 0..w {
            let mx = i64::from(bounds.left) + i64::from(x) - offset_x;
            let my = i64::from(bounds.top) + i64::from(y) - offset_y;
            let mut value = if mx >= 0 && my >= 0 && mx < i64::from(mw) && my < i64::from(mh) {
                values[my as usize * mw as usize + mx as usize]
            } else {
                mask.background
            };
            if mask.flags & 4 != 0 {
                value = 255 - value;
            }
            value = 255 - (((255 - value) as u16 * mask.density as u16 + 127) / 255) as u8;
            let p = (y as usize * w as usize + x as usize) * 4;
            data[p..p + 4].copy_from_slice(&[value, value, value, 255]);
        }
    }
    Pixels::from_rgba(w, h, data).ok_or_else(|| "Invalid Photoshop mask".into())
}

fn bake_clipping(
    doc: &mut Document,
    ids: &[u64],
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    warn(
        warnings,
        "Photoshop clipping relationships were converted to pixel masks; they no longer follow edits to the base layer.",
    );
    for idx in 0..doc.layers.len() {
        if !ids.contains(&doc.layers[idx].id) {
            continue;
        }
        let parent = doc.layers[idx].parent;
        let base = (0..idx)
            .rev()
            .find(|i| doc.layers[*i].parent == parent && !ids.contains(&doc.layers[*i].id));
        let Some(base_idx) = base else {
            warn(
                warnings,
                "A Photoshop clipping layer has no base layer; its clipping is not applied.",
            );
            continue;
        };
        if doc.layers[base_idx].is_group || doc.layers[idx].is_group {
            warn(
                warnings,
                "Clipping involving Photoshop groups is not applied.",
            );
            continue;
        }
        let base_layer = &doc.layers[base_idx];
        let base_pixels = base_layer.kind.pixels();
        let base_origin = base_layer
            .kind
            .raster_xform()
            .map(|(origin, _, _)| origin)
            .unwrap_or(Pt::ZERO);
        let layer = &doc.layers[idx];
        let Some(pixels) = layer.kind.pixels() else {
            continue;
        };
        let (w, h) = (pixels.w, pixels.h);
        let origin = layer
            .kind
            .raster_xform()
            .map(|(origin, _, _)| origin)
            .unwrap_or(Pt::ZERO);
        let mut mask = layer.mask.clone().unwrap_or_else(|| {
            let mut mask = Pixels::new(w, h);
            mask.data.fill(255);
            mask
        });
        for y in 0..h {
            for x in 0..w {
                let bx = x as i64 + origin.x as i64 - base_origin.x as i64;
                let by = y as i64 + origin.y as i64 - base_origin.y as i64;
                let alpha = base_pixels
                    .filter(|p| bx >= 0 && by >= 0 && bx < p.w as i64 && by < p.h as i64)
                    .map_or(0, |p| {
                        let i = (by as usize * p.w as usize + bx as usize) * 4;
                        let pixel_alpha = p.data[i + 3] as u16;
                        let mask_value = base_layer
                            .mask
                            .as_ref()
                            .and_then(|m| m.data.get(i))
                            .copied()
                            .unwrap_or(255) as u16;
                        (pixel_alpha * mask_value / 255) as u8
                    });
                let i = (y as usize * w as usize + x as usize) * 4;
                let value = (mask.data[i] as u16 * alpha as u16 / 255) as u8;
                mask.data[i..i + 4].copy_from_slice(&[value, value, value, 255]);
            }
        }
        mask.touch();
        doc.layers[idx].mask = Some(mask);
    }
    Ok(())
}

mod writer;
pub use writer::encode;

#[cfg(test)]
mod tests {
    use super::*;

    const INDEPENDENT_PSD: &[u8] = include_bytes!("psd/fixtures/independent-rle.psd");
    const INDEPENDENT_PSB: &[u8] = include_bytes!("psd/fixtures/independent-rle.psb");

    #[test]
    fn independent_rle_files_preserve_group_layers_offsets_and_masks() {
        for bytes in [INDEPENDENT_PSD, INDEPENDENT_PSB] {
            let result = decode(bytes, "Independent").unwrap();
            let doc = result.document;
            assert_eq!((doc.width, doc.height), (4.0, 3.0));
            assert_eq!(doc.layers.len(), 3);
            assert_eq!(doc.layers[0].name, "Base");
            let pixels = doc.layers[0].kind.pixels().unwrap();
            assert!(
                pixels
                    .data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|p| *p == [25, 50, 100, 255])
            );
            let group = &doc.layers[2];
            assert_eq!(group.name, "Design 🎨");
            assert!(group.is_group);
            assert!(!group.pass_through);
            assert_eq!(group.blend, Blend::Multiply);
            assert_eq!(group.opacity, 192.0 / 255.0);
            let child = &doc.layers[1];
            assert_eq!(child.parent, Some(group.id));
            assert!(!child.visible);
            assert_eq!(child.opacity, 128.0 / 255.0);
            assert_eq!(child.kind.raster_xform().unwrap().0, Pt::new(1.0, -1.0));
            assert!(
                child
                    .kind
                    .pixels()
                    .unwrap()
                    .data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|p| *p == [210, 40, 60, 128])
            );
            assert_eq!(
                child
                    .mask
                    .as_ref()
                    .unwrap()
                    .data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|p| p[0])
                    .collect::<Vec<_>>(),
                [0, 85, 170, 255]
            );
        }
    }

    #[test]
    fn export_reimports_layer_tree_without_losing_pixel_editability() {
        let doc = decode(INDEPENDENT_PSD, "Roundtrip").unwrap().document;
        for large in [false, true] {
            let exported = encode(&doc, large).unwrap();
            let loaded = decode(&exported.bytes, "Roundtrip").unwrap().document;
            assert_eq!(doc.layers.len(), loaded.layers.len());
            for (old, new) in doc.layers.iter().zip(&loaded.layers) {
                assert_eq!(old.name, new.name);
                assert_eq!(old.visible, new.visible);
                assert_eq!(old.opacity, new.opacity);
                assert_eq!(old.blend, new.blend);
                assert_eq!(old.is_group, new.is_group);
                assert_eq!(
                    old.kind.pixels().map(|p| &p.data),
                    new.kind.pixels().map(|p| &p.data)
                );
                assert_eq!(
                    old.mask.as_ref().map(|p| &p.data),
                    new.mask.as_ref().map(|p| &p.data)
                );
            }
            assert_eq!(loaded.layers[1].parent, Some(loaded.layers[2].id));
            let original = crate::compositor::export_png(&doc, 1).unwrap();
            assert_eq!(original, crate::compositor::export_png(&loaded, 1).unwrap());
        }
    }

    #[test]
    fn malformed_psd_lengths_rectangles_and_group_boundaries_are_rejected() {
        for end in 0..26 {
            assert!(decode(&INDEPENDENT_PSD[..end], "Broken").is_err());
        }
        let mut invalid_size = INDEPENDENT_PSD.to_vec();
        invalid_size[18..22].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(decode(&invalid_size, "Oversized").is_err());
        let mut invalid_length = INDEPENDENT_PSD.to_vec();
        invalid_length[26..30].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(decode(&invalid_length, "Oversized section").is_err());
        let divider_offset = INDEPENDENT_PSD
            .windows(4)
            .position(|b| b == b"lsct")
            .unwrap()
            + 8;
        let mut unmatched = INDEPENDENT_PSD.to_vec();
        unmatched[divider_offset..divider_offset + 4].copy_from_slice(&1u32.to_be_bytes());
        assert!(decode(&unmatched, "Unmatched group").is_err());
    }

    #[test]
    fn color_overlay_roundtrip_keeps_original_pixels_and_editable_filter() {
        let mut doc = decode(INDEPENDENT_PSD, "Color overlay").unwrap().document;
        doc.layers[0]
            .filters
            .items
            .push(crate::filter::Fx::ColorMatrix {
                values: [
                    0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                    0.0, 0.0, 1.0, 0.0,
                ],
            });
        let exported = encode(&doc, false).unwrap();
        assert!(exported.bytes.windows(4).any(|key| key == b"lfx2"));
        let imported = decode(&exported.bytes, "Color overlay").unwrap().document;
        assert_eq!(doc.layers[0].filters, imported.layers[0].filters);
        assert_eq!(
            doc.layers[0].kind.pixels().unwrap().data,
            imported.layers[0].kind.pixels().unwrap().data
        );
        assert_eq!(
            crate::compositor::export_png(&doc, 1).unwrap(),
            crate::compositor::export_png(&imported, 1).unwrap()
        );
    }

    #[test]
    fn independent_real_mask_header_precedes_variable_parameters() {
        let bytes = include_bytes!("psd/fixtures/independent-real-mask.psd");
        let imported = decode(bytes, "Mixed masks").unwrap();
        assert_eq!(imported.document.layers.len(), 1);
        let mask = imported.document.layers[0].mask.as_ref().unwrap();
        assert_eq!((mask.w, mask.h), (3, 2));
        assert_eq!(
            mask.data
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| p[0])
                .collect::<Vec<_>>(),
            [127, 152, 177, 202, 227, 255]
        );
        assert!(imported.warnings.iter().any(|w| w.contains("feathering")));
    }

    #[test]
    fn cropped_group_mask_retains_its_document_position_on_export() {
        let mut doc = decode(INDEPENDENT_PSD, "Placed group mask")
            .unwrap()
            .document;
        doc.layers[1].visible = true;
        doc.layers[1].mask = None;
        doc.layers[1]
            .kind
            .set_raster_xform(Pt::ZERO, Pt::new(2.0, 2.0), 0.0);
        doc.layers[2].mask = Pixels::from_rgba(1, 2, vec![255, 255, 255, 255, 128, 128, 128, 255]);
        doc.layers[2].mask_origin = Pt::new(1.0, 0.0);
        doc.layers[2].mask_size = Pt::new(1.0, 2.0);
        let expected = crate::compositor::export_png(&doc, 1).unwrap();
        let bytes = encode(&doc, false).unwrap().bytes;
        let imported = decode(&bytes, "Placed group mask").unwrap().document;
        assert_eq!(
            expected,
            crate::compositor::export_png(&imported, 1).unwrap()
        );
    }
}
