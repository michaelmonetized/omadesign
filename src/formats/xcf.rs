//! GIMP XCF layered raster import.
//!
//! https://developer.gimp.org/core/standards/xcf/
//! Bounded, no GIMP process. Save remains `.oma`. OpenRaster is the export to GIMP.

use crate::color::Blend;
use crate::document::{Artboard, Document, Guide, Layer, Pixels};
use crate::geom::Pt;
use flate2::read::ZlibDecoder;
use std::io::Read;

const LIMIT: u64 = 512 * 1024 * 1024;
const MAX_PIXELS: u64 = 64 * 1024 * 1024;
const MAX_LAYERS: usize = 8192;
const TILE: u32 = 64;

const PROP_END: u32 = 0;
const PROP_COLORMAP: u32 = 1;
const PROP_FLOATING_SELECTION: u32 = 5;
const PROP_OPACITY: u32 = 6;
const PROP_MODE: u32 = 7;
const PROP_VISIBLE: u32 = 8;
const PROP_LOCK_ALPHA: u32 = 10;
const PROP_APPLY_MASK: u32 = 11;
const PROP_OFFSETS: u32 = 15;
const PROP_COMPRESSION: u32 = 17;
const PROP_GUIDES: u32 = 18;
const PROP_RESOLUTION: u32 = 19;
const PROP_TEXT_LAYER_FLAGS: u32 = 26;
const PROP_LOCK_CONTENT: u32 = 28;
const PROP_GROUP_ITEM: u32 = 29;
const PROP_ITEM_PATH: u32 = 30;
const PROP_LOCK_POSITION: u32 = 32;
const PROP_FLOAT_OPACITY: u32 = 33;

pub fn looks_like(bytes: &[u8]) -> bool {
    bytes.starts_with(b"gimp xcf ")
}

pub fn read(bytes: &[u8], name: &str) -> Result<(Document, Vec<String>), String> {
    if bytes.len() as u64 > LIMIT {
        return Err("GIMP document exceeds 512 MiB.".into());
    }
    let mut p = Parser {
        data: bytes,
        pos: 0,
        wide: false,
        version: 0,
        decoded: 0,
        warnings: Vec::new(),
    };
    p.header()?;
    let width = p.u32()?;
    let height = p.u32()?;
    dimensions(width, height)?;
    let base = p.u32()?;
    let precision = if p.version >= 4 { p.u32()? } else { 1 };
    let bpc = bytes_per_channel(precision, &mut p.warnings)?;
    let mut compression = 1u8;
    let mut colormap = Vec::new();
    let mut dpi = 72.0f32;
    let mut guides = Vec::new();
    p.properties(|p, kind, payload| {
        match kind {
            PROP_COLORMAP => colormap = colormap_from(payload)?,
            PROP_COMPRESSION => {
                compression = payload
                    .first()
                    .copied()
                    .ok_or("GIMP compression is missing")?;
            }
            PROP_RESOLUTION => {
                if payload.len() >= 4 {
                    dpi = f32::from_be_bytes(payload[0..4].try_into().unwrap());
                    if !dpi.is_finite() || dpi <= 0.0 {
                        dpi = 72.0;
                    }
                }
            }
            PROP_GUIDES => guides = guides_from(payload)?,
            PROP_FLOATING_SELECTION => p.warn(
                "A floating selection is not imported. Anchor it in GIMP, or save OpenRaster.",
            ),
            _ => {}
        }
        Ok(())
    })?;
    let layers_at = p.pointers()?;
    let _channels = p.pointers()?;
    if layers_at.len() > MAX_LAYERS {
        return Err("GIMP document exceeds 8,192 layers.".into());
    }
    let mut parsed = Vec::new();
    for offset in layers_at {
        parsed.push(p.layer(offset, compression, bpc, base, &colormap)?);
    }
    let mut doc = Document::new(name, width as f32, height as f32, dpi);
    doc.transparent = true;
    doc.artboards = vec![Artboard::new(
        0,
        Pt::ZERO,
        Pt::new(width as f32, height as f32),
    )];
    doc.guides = guides;
    doc.layers = assemble(parsed, &mut p.warnings)?;
    if doc.layers.is_empty() {
        doc.layers.push(Layer::vector("Layer 1"));
    }
    match base {
        0 | 1 | 2 => {}
        other => p.warn(format!(
            "GIMP color type {other} is treated as RGB. Save .oma after checking the result."
        )),
    }
    if compression > 2 {
        return Err("GIMP tile compression is not supported.".into());
    }
    doc.validate_hierarchy()?;
    Ok((doc, p.warnings))
}

fn bytes_per_channel(precision: u32, warnings: &mut Vec<String>) -> Result<u32, String> {
    // GIMP 2.10 numbers, plus the later perceptual variants that keep 8-bit in 0..=2.
    match precision {
        0..=2 => Ok(1),
        100..=102 => Ok(1),
        150..=152 => Ok(1),
        3 | 4 | 6 | 103 | 104 | 106 | 153 | 154 | 156 => {
            warn(
                warnings,
                "GIMP high-bit-depth channels are converted to 8-bit RGBA.",
            );
            Ok(2)
        }
        5 | 7 | 8 | 105 | 107 | 108 | 155 | 157 | 158 => {
            warn(
                warnings,
                "GIMP high-bit-depth channels are converted to 8-bit RGBA.",
            );
            Ok(4)
        }
        _ => {
            warn(
                warnings,
                "Unrecognized GIMP precision is treated as 8-bit. Check the imported pixels.",
            );
            Ok(1)
        }
    }
}

struct Parsed {
    layer: Layer,
    path: Vec<u32>,
    group: bool,
}

fn assemble(parsed: Vec<Parsed>, warnings: &mut Vec<String>) -> Result<Vec<Layer>, String> {
    let mut by_path: std::collections::HashMap<Vec<u32>, u64> = std::collections::HashMap::new();
    for item in &parsed {
        if item.group {
            by_path.insert(item.path.clone(), item.layer.id);
        }
    }
    let mut layers = Vec::with_capacity(parsed.len());
    for mut item in parsed.into_iter().rev() {
        if let Some((last, parent_path)) = item.path.split_last() {
            let _ = last;
            if let Some(parent) = by_path.get(parent_path) {
                item.layer.parent = Some(*parent);
            } else if !item.path.is_empty() {
                warn(
                    warnings,
                    "A GIMP layer refers to a missing group and is imported at the root.",
                );
            }
        }
        layers.push(item.layer);
    }
    Ok(layers)
}

struct Parser<'a> {
    data: &'a [u8],
    pos: usize,
    wide: bool,
    version: u32,
    decoded: u64,
    warnings: Vec<String>,
}

impl<'a> Parser<'a> {
    fn header(&mut self) -> Result<(), String> {
        let magic = self.bytes(14)?;
        let text = std::str::from_utf8(magic).map_err(|_| "Not a GIMP XCF document")?;
        self.version = if text == "gimp xcf file\0" {
            0
        } else if let Some(rest) = text.strip_prefix("gimp xcf v")
            && rest.ends_with('\0')
        {
            rest.trim_end_matches('\0')
                .parse()
                .map_err(|_| "Not a GIMP XCF document")?
        } else {
            return Err("Not a GIMP XCF document".into());
        };
        self.wide = self.version >= 11;
        if self.version > 25 {
            self.warn(format!(
                "GIMP XCF version {} is newer than this reader. Supported layers are imported; check conversion notes.",
                self.version
            ));
        }
        Ok(())
    }

    fn layer(
        &mut self,
        offset: u64,
        compression: u8,
        bpc: u32,
        base: u32,
        colormap: &[[u8; 3]],
    ) -> Result<Parsed, String> {
        self.seek(offset)?;
        let width = self.u32()?;
        let height = self.u32()?;
        dimensions(width, height)?;
        let kind = self.u32()?;
        let name = self.string()?;
        let mut opacity = 1.0f32;
        let mut visible = true;
        let mut locked = false;
        let mut blend = Blend::Normal;
        let mut origin = Pt::ZERO;
        let mut group = false;
        let mut path = Vec::new();
        let mut apply_mask = true;
        let mut text = false;
        let mut pass_through = false;
        self.properties(|p, prop, payload| {
            match prop {
                PROP_OPACITY => {
                    if payload.len() >= 4 {
                        let value = u32::from_be_bytes(payload[0..4].try_into().unwrap());
                        opacity = (value as f32 / 255.0).clamp(0.0, 1.0);
                    }
                }
                PROP_FLOAT_OPACITY => {
                    if payload.len() >= 4 {
                        let value = f32::from_be_bytes(payload[0..4].try_into().unwrap());
                        if value.is_finite() {
                            opacity = value.clamp(0.0, 1.0);
                        }
                    }
                }
                PROP_VISIBLE => {
                    if payload.len() >= 4 {
                        visible = u32::from_be_bytes(payload[0..4].try_into().unwrap()) != 0;
                    }
                }
                PROP_MODE => {
                    if payload.len() >= 4 {
                        let mode = u32::from_be_bytes(payload[0..4].try_into().unwrap());
                        if mode == 61 {
                            pass_through = true;
                        }
                        blend = blend_mode(mode, &mut p.warnings);
                    }
                }
                PROP_OFFSETS => {
                    if payload.len() >= 8 {
                        let x = i32::from_be_bytes(payload[0..4].try_into().unwrap());
                        let y = i32::from_be_bytes(payload[4..8].try_into().unwrap());
                        origin = Pt::new(x as f32, y as f32);
                    }
                }
                PROP_GROUP_ITEM => group = true,
                PROP_ITEM_PATH => {
                    if payload.len() % 4 != 0 {
                        return Err("GIMP group path is not aligned".into());
                    }
                    path = payload
                        .chunks_exact(4)
                        .map(|c| u32::from_be_bytes(c.try_into().unwrap()))
                        .collect();
                }
                PROP_LOCK_CONTENT | PROP_LOCK_POSITION | PROP_LOCK_ALPHA => {
                    if payload.len() >= 4
                        && u32::from_be_bytes(payload[0..4].try_into().unwrap()) != 0
                    {
                        locked = true;
                    }
                }
                PROP_APPLY_MASK => {
                    if payload.len() >= 4 {
                        apply_mask = u32::from_be_bytes(payload[0..4].try_into().unwrap()) != 0;
                    }
                }
                PROP_TEXT_LAYER_FLAGS => text = true,
                PROP_FLOATING_SELECTION => p.warn(
                    "A floating selection is imported as a normal layer. Anchor it in GIMP for a faithful stack.",
                ),
                _ => {}
            }
            Ok(())
        })?;
        let hierarchy = self.pointer()?;
        let mask_at = self.pointer()?;
        let mut layer = if group {
            let mut group_layer = Layer::group(name);
            group_layer.pass_through = pass_through;
            group_layer
        } else {
            let pixels = self.pixels(
                hierarchy,
                width,
                height,
                kind,
                compression,
                bpc,
                base,
                colormap,
            )?;
            Layer::placed_raster(name, pixels, origin, Pt::new(width as f32, height as f32))
        };
        if apply_mask && mask_at != 0 && !group {
            let mask = self.mask(mask_at, width, height, compression, bpc)?;
            apply_layer_mask(layer.kind.pixels_mut().unwrap(), &mask);
        } else if mask_at != 0 && !group {
            self.warn("A disabled GIMP layer mask is not applied.");
        }
        if text {
            self.warn("GIMP text layers import as pixels. Save .oma; they are not live type.");
        }
        layer.visible = visible;
        layer.locked = locked;
        layer.opacity = opacity;
        layer.blend = blend;
        Ok(Parsed { layer, path, group })
    }

    fn pixels(
        &mut self,
        hierarchy: u64,
        width: u32,
        height: u32,
        kind: u32,
        compression: u8,
        bpc: u32,
        base: u32,
        colormap: &[[u8; 3]],
    ) -> Result<Pixels, String> {
        if width == 0 || height == 0 {
            return Pixels::from_rgba(1, 1, vec![0, 0, 0, 0])
                .ok_or_else(|| "Invalid empty GIMP layer".into());
        }
        let components = components(kind)?;
        let bpp = components
            .checked_mul(bpc)
            .ok_or("GIMP layer type overflow")?;
        self.seek(hierarchy)?;
        let hw = self.u32()?;
        let hh = self.u32()?;
        let hbpp = self.u32()?;
        if hw != width || hh != height {
            return Err("GIMP layer hierarchy size does not match the layer".into());
        }
        if hbpp != bpp {
            return Err("GIMP layer hierarchy depth does not match the layer type".into());
        }
        let level = self.pointer()?;
        self.seek(level)?;
        let lw = self.u32()?;
        let lh = self.u32()?;
        let _lbpp = self.u32()?;
        if lw != width || lh != height {
            return Err("GIMP layer level size does not match the layer".into());
        }
        let tiles = self.pointers()?;
        let expected = tile_count(width, height);
        if tiles.len() != expected {
            return Err("GIMP layer tile count does not match its size".into());
        }
        let raw = self.read_tiles(width, height, bpp, compression, &tiles)?;
        rgba_from_raw(&raw, width, height, kind, bpc, base, colormap)
    }

    fn mask(
        &mut self,
        offset: u64,
        width: u32,
        height: u32,
        compression: u8,
        bpc: u32,
    ) -> Result<Vec<u8>, String> {
        self.seek(offset)?;
        let mw = self.u32()?;
        let mh = self.u32()?;
        if mw != width || mh != height {
            return Err("GIMP layer mask size does not match the layer".into());
        }
        let _name = self.string()?;
        self.properties(|_, _, _| Ok(()))?;
        let hierarchy = self.pointer()?;
        self.seek(hierarchy)?;
        let hw = self.u32()?;
        let hh = self.u32()?;
        let hbpp = self.u32()?;
        if hw != width || hh != height {
            return Err("GIMP layer mask hierarchy size does not match the layer".into());
        }
        let level = self.pointer()?;
        self.seek(level)?;
        let _lw = self.u32()?;
        let _lh = self.u32()?;
        let _lbpp = self.u32()?;
        let tiles = self.pointers()?;
        let raw = self.read_tiles(width, height, hbpp.max(bpc), compression, &tiles)?;
        let mut mask = vec![0u8; (width as usize) * (height as usize)];
        let stride = hbpp.max(1) as usize;
        if raw.len() < mask.len().saturating_mul(stride) && stride == 1 {
            if raw.len() != mask.len() {
                return Err("GIMP layer mask pixels are truncated".into());
            }
            return Ok(raw);
        }
        for (i, slot) in mask.iter_mut().enumerate() {
            *slot = raw.get(i * stride).copied().unwrap_or(0);
        }
        Ok(mask)
    }

    fn read_tiles(
        &mut self,
        width: u32,
        height: u32,
        bpp: u32,
        compression: u8,
        tiles: &[u64],
    ) -> Result<Vec<u8>, String> {
        let bpp = bpp as usize;
        let total = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(bpp))
            .ok_or("GIMP layer pixels overflow")?;
        self.account(total as u64)?;
        let mut out = vec![0u8; total];
        let cols = width.div_ceil(TILE);
        for (index, &offset) in tiles.iter().enumerate() {
            let col = (index as u32) % cols;
            let row = (index as u32) / cols;
            let x = col * TILE;
            let y = row * TILE;
            let tw = (width - x).min(TILE);
            let th = (height - y).min(TILE);
            let tile_bytes = (tw as usize)
                .checked_mul(th as usize)
                .and_then(|n| n.checked_mul(bpp))
                .ok_or("GIMP tile overflow")?;
            let next = tiles
                .get(index + 1)
                .copied()
                .unwrap_or(self.data.len() as u64);
            if next < offset {
                return Err("GIMP tile offsets are not increasing".into());
            }
            let stored = (next - offset) as usize;
            self.seek(offset)?;
            let encoded = self.bytes(stored.min(self.data.len().saturating_sub(self.pos)))?;
            let decoded = decode_tile(encoded, tile_bytes, compression, bpp)?;
            if decoded.len() != tile_bytes {
                return Err("GIMP tile decoded to the wrong size".into());
            }
            for ty in 0..th as usize {
                let src = ty * tw as usize * bpp;
                let dest = ((y as usize + ty) * width as usize + x as usize) * bpp;
                out[dest..dest + tw as usize * bpp]
                    .copy_from_slice(&decoded[src..src + tw as usize * bpp]);
            }
        }
        Ok(out)
    }

    fn properties(
        &mut self,
        mut visit: impl FnMut(&mut Self, u32, &'a [u8]) -> Result<(), String>,
    ) -> Result<(), String> {
        loop {
            let kind = self.u32()?;
            let len = self.u32()? as usize;
            if kind == PROP_END {
                if len != 0 {
                    self.skip(len)?;
                }
                return Ok(());
            }
            let payload = self.bytes(len)?;
            visit(self, kind, payload)?;
        }
    }

    fn pointers(&mut self) -> Result<Vec<u64>, String> {
        let mut out = Vec::new();
        loop {
            let ptr = self.pointer()?;
            if ptr == 0 {
                return Ok(out);
            }
            if ptr as usize > self.data.len() {
                return Err("GIMP pointer is outside the file".into());
            }
            if out.len() >= MAX_LAYERS {
                return Err("GIMP document exceeds 8,192 layers.".into());
            }
            out.push(ptr);
        }
    }

    fn pointer(&mut self) -> Result<u64, String> {
        if self.wide {
            Ok(self.u64()?)
        } else {
            Ok(self.u32()? as u64)
        }
    }

    fn string(&mut self) -> Result<String, String> {
        let len = self.u32()? as usize;
        if len == 0 {
            return Ok(String::new());
        }
        let bytes = self.bytes(len)?;
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        String::from_utf8(bytes[..end].to_vec()).map_err(|_| "GIMP layer name is not UTF-8".into())
    }

    fn u32(&mut self) -> Result<u32, String> {
        let bytes = self.bytes(4)?;
        Ok(u32::from_be_bytes(bytes.try_into().unwrap()))
    }

    fn u64(&mut self) -> Result<u64, String> {
        let bytes = self.bytes(8)?;
        Ok(u64::from_be_bytes(bytes.try_into().unwrap()))
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or("GIMP document is truncated")?;
        if end > self.data.len() {
            return Err("GIMP document is truncated".into());
        }
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn skip(&mut self, n: usize) -> Result<(), String> {
        self.bytes(n).map(|_| ())
    }

    fn seek(&mut self, offset: u64) -> Result<(), String> {
        if offset as usize > self.data.len() {
            return Err("GIMP pointer is outside the file".into());
        }
        self.pos = offset as usize;
        Ok(())
    }

    fn account(&mut self, extra: u64) -> Result<(), String> {
        self.decoded = self
            .decoded
            .checked_add(extra)
            .ok_or("GIMP pixels exceed memory limit")?;
        if self.decoded > LIMIT || extra > MAX_PIXELS * 4 {
            return Err("GIMP layer pixels exceed 512 MiB.".into());
        }
        Ok(())
    }

    fn warn(&mut self, text: impl Into<String>) {
        warn(&mut self.warnings, text);
    }
}

fn warn(warnings: &mut Vec<String>, text: impl Into<String>) {
    let text = text.into();
    if !warnings.contains(&text) {
        warnings.push(text);
    }
}

fn dimensions(w: u32, h: u32) -> Result<u64, String> {
    let pixels = u64::from(w)
        .checked_mul(u64::from(h))
        .ok_or("GIMP dimensions overflow")?;
    if pixels > MAX_PIXELS {
        return Err("GIMP image exceeds the 64 megapixel import limit".into());
    }
    if w > 32_768 || h > 32_768 {
        return Err("GIMP image exceeds 32,768 pixels on a side".into());
    }
    Ok(pixels)
}

fn tile_count(width: u32, height: u32) -> usize {
    width.div_ceil(TILE) as usize * height.div_ceil(TILE) as usize
}

fn components(kind: u32) -> Result<u32, String> {
    match kind {
        0 => Ok(3),
        1 => Ok(4),
        2 | 4 => Ok(1),
        3 | 5 => Ok(2),
        _ => Err(format!("Unsupported GIMP layer type {kind}")),
    }
}

fn decode_tile(
    encoded: &[u8],
    tile_bytes: usize,
    compression: u8,
    bpp: usize,
) -> Result<Vec<u8>, String> {
    match compression {
        0 => {
            if encoded.len() < tile_bytes {
                return Err("Uncompressed GIMP tile is truncated".into());
            }
            Ok(encoded[..tile_bytes].to_vec())
        }
        1 => decode_rle(encoded, tile_bytes, bpp),
        2 => {
            let decoder = ZlibDecoder::new(encoded);
            let mut out = Vec::new();
            decoder
                .take(tile_bytes as u64 + 1)
                .read_to_end(&mut out)
                .map_err(|e| format!("GIMP zlib tile is invalid: {e}"))?;
            if out.len() != tile_bytes {
                return Err("GIMP zlib tile decoded to the wrong size".into());
            }
            Ok(out)
        }
        _ => Err("GIMP tile compression is not supported".into()),
    }
}

fn decode_rle(encoded: &[u8], tile_bytes: usize, bpp: usize) -> Result<Vec<u8>, String> {
    if bpp == 0 || tile_bytes % bpp != 0 {
        return Err("GIMP RLE tile depth is invalid".into());
    }
    let plane = tile_bytes / bpp;
    let mut planar = vec![0u8; tile_bytes];
    let mut src = 0usize;
    for channel in 0..bpp {
        let dest = &mut planar[channel * plane..(channel + 1) * plane];
        let mut filled = 0usize;
        while filled < plane {
            let opcode = *encoded.get(src).ok_or("GIMP RLE tile is truncated")?;
            src += 1;
            if opcode <= 126 {
                let count = opcode as usize + 1;
                let value = *encoded.get(src).ok_or("GIMP RLE tile is truncated")?;
                src += 1;
                if filled + count > plane {
                    return Err("GIMP RLE run exceeds the tile".into());
                }
                dest[filled..filled + count].fill(value);
                filled += count;
            } else if opcode == 127 {
                let p = *encoded.get(src).ok_or("GIMP RLE tile is truncated")? as usize;
                let q = *encoded.get(src + 1).ok_or("GIMP RLE tile is truncated")? as usize;
                src += 2;
                let count = p * 256 + q;
                let value = *encoded.get(src).ok_or("GIMP RLE tile is truncated")?;
                src += 1;
                if filled + count > plane {
                    return Err("GIMP RLE run exceeds the tile".into());
                }
                dest[filled..filled + count].fill(value);
                filled += count;
            } else if opcode == 128 {
                let p = *encoded.get(src).ok_or("GIMP RLE tile is truncated")? as usize;
                let q = *encoded.get(src + 1).ok_or("GIMP RLE tile is truncated")? as usize;
                src += 2;
                let count = p * 256 + q;
                if filled + count > plane || src + count > encoded.len() {
                    return Err("GIMP RLE literal exceeds the tile".into());
                }
                dest[filled..filled + count].copy_from_slice(&encoded[src..src + count]);
                src += count;
                filled += count;
            } else {
                let count = 256 - opcode as usize;
                if filled + count > plane || src + count > encoded.len() {
                    return Err("GIMP RLE literal exceeds the tile".into());
                }
                dest[filled..filled + count].copy_from_slice(&encoded[src..src + count]);
                src += count;
                filled += count;
            }
        }
    }
    let mut interleaved = vec![0u8; tile_bytes];
    for i in 0..plane {
        for channel in 0..bpp {
            interleaved[i * bpp + channel] = planar[channel * plane + i];
        }
    }
    Ok(interleaved)
}

fn sample(raw: &[u8], index: usize, bpc: u32) -> u8 {
    match bpc {
        1 => raw.get(index).copied().unwrap_or(0),
        2 => raw.get(index).copied().unwrap_or(0),
        4 => raw.get(index).copied().unwrap_or(0),
        _ => 0,
    }
}

fn rgba_from_raw(
    raw: &[u8],
    width: u32,
    height: u32,
    kind: u32,
    bpc: u32,
    _base: u32,
    colormap: &[[u8; 3]],
) -> Result<Pixels, String> {
    let n = width as usize * height as usize;
    let step = components(kind)? as usize * bpc as usize;
    if raw.len() < n.saturating_mul(step) {
        return Err("GIMP layer pixels are truncated".into());
    }
    let mut rgba = vec![0u8; n * 4];
    for i in 0..n {
        let base = i * step;
        let px = match kind {
            0 => [
                sample(raw, base, bpc),
                sample(raw, base + bpc as usize, bpc),
                sample(raw, base + 2 * bpc as usize, bpc),
                255,
            ],
            1 => [
                sample(raw, base, bpc),
                sample(raw, base + bpc as usize, bpc),
                sample(raw, base + 2 * bpc as usize, bpc),
                sample(raw, base + 3 * bpc as usize, bpc),
            ],
            2 => {
                let g = sample(raw, base, bpc);
                [g, g, g, 255]
            }
            3 => {
                let g = sample(raw, base, bpc);
                [g, g, g, sample(raw, base + bpc as usize, bpc)]
            }
            4 => {
                let index = sample(raw, base, 1) as usize;
                let rgb = colormap.get(index).copied().unwrap_or([0, 0, 0]);
                [rgb[0], rgb[1], rgb[2], 255]
            }
            5 => {
                let index = sample(raw, base, 1) as usize;
                let rgb = colormap.get(index).copied().unwrap_or([0, 0, 0]);
                [rgb[0], rgb[1], rgb[2], sample(raw, base + 1, 1)]
            }
            _ => [0, 0, 0, 0],
        };
        rgba[i * 4..i * 4 + 4].copy_from_slice(&px);
    }
    Pixels::from_rgba(width, height, rgba).ok_or_else(|| "Invalid GIMP pixels".into())
}

fn apply_layer_mask(pixels: &mut Pixels, mask: &[u8]) {
    if mask.len() * 4 != pixels.data.len() {
        return;
    }
    for (i, chunk) in pixels.data.chunks_exact_mut(4).enumerate() {
        chunk[3] = ((u16::from(chunk[3]) * u16::from(mask[i])) / 255) as u8;
    }
}

fn colormap_from(payload: &[u8]) -> Result<Vec<[u8; 3]>, String> {
    if payload.len() < 4 {
        return Err("GIMP colormap is truncated".into());
    }
    let count = u32::from_be_bytes(payload[0..4].try_into().unwrap()) as usize;
    if count > 256 || payload.len() < 4 + count * 3 {
        return Err("GIMP colormap is invalid".into());
    }
    Ok(payload[4..4 + count * 3]
        .chunks_exact(3)
        .map(|c| [c[0], c[1], c[2]])
        .collect())
}

fn guides_from(payload: &[u8]) -> Result<Vec<Guide>, String> {
    let mut guides = Vec::new();
    let mut i = 0;
    while i + 5 <= payload.len() {
        let pos = i32::from_be_bytes(payload[i..i + 4].try_into().unwrap()) as f32;
        let orientation = payload[i + 4];
        i += 5;
        match orientation {
            1 => guides.push(Guide {
                vertical: false,
                pos,
            }),
            2 => guides.push(Guide {
                vertical: true,
                pos,
            }),
            _ => {}
        }
    }
    Ok(guides)
}

fn blend_mode(mode: u32, warnings: &mut Vec<String>) -> Blend {
    match mode {
        0 | 23 | 28 => Blend::Normal,
        3 | 30 => Blend::Multiply,
        4 | 31 => Blend::Screen,
        5 => Blend::Overlay,
        6 | 32 => Blend::Difference,
        9 | 35 => Blend::Darken,
        10 | 36 => Blend::Lighten,
        11 | 37 => Blend::Hue,
        12 | 38 => Blend::Saturation,
        13 | 39 => Blend::Color,
        14 | 40 | 56 => Blend::Luminosity,
        16 | 42 => Blend::ColorDodge,
        17 | 43 => Blend::ColorBurn,
        18 | 44 => Blend::HardLight,
        19 | 45 => Blend::SoftLight,
        52 => Blend::Exclusion,
        61 => Blend::Normal,
        _ => {
            warn(
                warnings,
                format!("GIMP blend mode {mode} is displayed as Normal."),
            );
            Blend::Normal
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write;

    struct Writer {
        data: Vec<u8>,
        wide: bool,
    }

    impl Writer {
        fn new(version: u32) -> Self {
            let mut data = Vec::new();
            if version == 0 {
                data.extend(b"gimp xcf file\0");
            } else {
                data.extend(format!("gimp xcf v{version:03}\0").as_bytes());
            }
            Self {
                data,
                wide: version >= 11,
            }
        }

        fn u32(&mut self, value: u32) {
            self.data.extend(value.to_be_bytes());
        }

        fn pointer_at(&mut self, value: u64) {
            if self.wide {
                self.data.extend(value.to_be_bytes());
            } else {
                self.data.extend((value as u32).to_be_bytes());
            }
        }

        fn string(&mut self, text: &str) {
            if text.is_empty() {
                self.u32(0);
                return;
            }
            self.u32(text.len() as u32 + 1);
            self.data.extend(text.as_bytes());
            self.data.push(0);
        }

        fn prop(&mut self, kind: u32, payload: &[u8]) {
            self.u32(kind);
            self.u32(payload.len() as u32);
            self.data.extend(payload);
        }

        fn end_props(&mut self) {
            self.u32(PROP_END);
            self.u32(0);
        }

        fn patch_pointer(&mut self, at: usize, value: u64) {
            if self.wide {
                self.data[at..at + 8].copy_from_slice(&value.to_be_bytes());
            } else {
                self.data[at..at + 4].copy_from_slice(&(value as u32).to_be_bytes());
            }
        }

        fn pointer_slot(&mut self) -> usize {
            let at = self.data.len();
            self.pointer_at(0);
            at
        }
    }

    fn rgba_layer(
        version: u32,
        name: &str,
        width: u32,
        height: u32,
        pixels: &[u8],
        origin: (i32, i32),
        compression: u8,
        path: &[u32],
        group: bool,
    ) -> Vec<u8> {
        let mut w = Writer::new(version);
        w.u32(width.max(1));
        w.u32(height.max(1));
        w.u32(0);
        if version >= 4 {
            w.u32(1);
        }
        w.prop(PROP_COMPRESSION, &[compression]);
        w.end_props();
        let layer_slot = w.pointer_slot();
        w.pointer_at(0);
        w.pointer_at(0);
        let layer_at = w.data.len() as u64;
        w.patch_pointer(layer_slot, layer_at);
        w.u32(width.max(1));
        w.u32(height.max(1));
        w.u32(1);
        w.string(name);
        let mut offsets = Vec::new();
        offsets.extend(origin.0.to_be_bytes());
        offsets.extend(origin.1.to_be_bytes());
        w.prop(PROP_OFFSETS, &offsets);
        if group {
            w.prop(PROP_GROUP_ITEM, &[]);
        }
        if !path.is_empty() {
            let mut bytes = Vec::new();
            for index in path {
                bytes.extend(index.to_be_bytes());
            }
            w.prop(PROP_ITEM_PATH, &bytes);
        }
        w.end_props();
        let hierarchy_slot = w.pointer_slot();
        w.pointer_at(0);
        if group {
            w.patch_pointer(hierarchy_slot, w.data.len() as u64);
            w.u32(width.max(1));
            w.u32(height.max(1));
            w.u32(4);
            w.pointer_at(0);
            return w.data;
        }
        let hierarchy_at = w.data.len() as u64;
        w.patch_pointer(hierarchy_slot, hierarchy_at);
        w.u32(width);
        w.u32(height);
        w.u32(4);
        let level_slot = w.pointer_slot();
        w.pointer_at(0);
        let level_at = w.data.len() as u64;
        w.patch_pointer(level_slot, level_at);
        w.u32(width);
        w.u32(height);
        w.u32(4);
        let tile_slot = w.pointer_slot();
        w.pointer_at(0);
        let tile_at = w.data.len() as u64;
        w.patch_pointer(tile_slot, tile_at);
        let encoded = match compression {
            0 => pixels.to_vec(),
            1 => rle_encode(pixels, 4),
            2 => {
                let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
                encoder.write_all(pixels).unwrap();
                encoder.finish().unwrap()
            }
            _ => pixels.to_vec(),
        };
        w.data.extend(encoded);
        w.data
    }

    fn rle_encode(pixels: &[u8], bpp: usize) -> Vec<u8> {
        let plane = pixels.len() / bpp;
        let mut out = Vec::new();
        for channel in 0..bpp {
            let mut data = Vec::with_capacity(plane);
            for i in 0..plane {
                data.push(pixels[i * bpp + channel]);
            }
            let mut i = 0;
            while i < data.len() {
                let remain = data.len() - i;
                if remain == 1 {
                    out.push(255);
                    out.push(data[i]);
                    break;
                }
                if data[i] == data[i + 1] {
                    let mut count = 1;
                    while i + count < data.len() && data[i + count] == data[i] && count < 127 {
                        count += 1;
                    }
                    out.push((count - 1) as u8);
                    out.push(data[i]);
                    i += count;
                } else {
                    let mut count = 1;
                    while i + count < data.len()
                        && (count == 1 || data[i + count] != data[i + count - 1])
                        && count < 127
                    {
                        if count >= 2 && data[i + count] == data[i + count - 1] {
                            count -= 1;
                            break;
                        }
                        count += 1;
                    }
                    out.push((256 - count) as u8);
                    out.extend(&data[i..i + count]);
                    i += count;
                }
            }
        }
        out
    }

    fn two_layer_group(version: u32, compression: u8) -> Vec<u8> {
        let mut w = Writer::new(version);
        w.u32(2);
        w.u32(1);
        w.u32(0);
        if version >= 4 {
            w.u32(1);
        }
        w.prop(PROP_COMPRESSION, &[compression]);
        w.end_props();
        let group_slot = w.pointer_slot();
        let red_slot = w.pointer_slot();
        let blue_slot = w.pointer_slot();
        w.pointer_at(0);
        w.pointer_at(0);
        let group_at = write_group(&mut w, "Top group", &[]);
        let red_at = write_rgba(
            &mut w,
            "Hidden red",
            1,
            1,
            &[250, 20, 10, 128],
            (-2, 3),
            compression,
            &[0],
            false,
            false,
        );
        let blue_at = write_rgba(
            &mut w,
            "Bottom blue",
            2,
            1,
            &[10, 20, 250, 255, 10, 20, 250, 255],
            (0, 0),
            compression,
            &[],
            false,
            true,
        );
        w.patch_pointer(group_slot, group_at);
        w.patch_pointer(red_slot, red_at);
        w.patch_pointer(blue_slot, blue_at);
        w.data
    }

    fn write_group(w: &mut Writer, name: &str, path: &[u32]) -> u64 {
        let at = w.data.len() as u64;
        w.u32(1);
        w.u32(1);
        w.u32(1);
        w.string(name);
        w.prop(PROP_GROUP_ITEM, &[]);
        if !path.is_empty() {
            let mut bytes = Vec::new();
            for index in path {
                bytes.extend(index.to_be_bytes());
            }
            w.prop(PROP_ITEM_PATH, &bytes);
        }
        w.end_props();
        w.pointer_at(0);
        w.pointer_at(0);
        at
    }

    fn write_rgba(
        w: &mut Writer,
        name: &str,
        width: u32,
        height: u32,
        pixels: &[u8],
        origin: (i32, i32),
        compression: u8,
        path: &[u32],
        _unused: bool,
        visible: bool,
    ) -> u64 {
        let at = w.data.len() as u64;
        w.u32(width);
        w.u32(height);
        w.u32(1);
        w.string(name);
        let mut offsets = Vec::new();
        offsets.extend(origin.0.to_be_bytes());
        offsets.extend(origin.1.to_be_bytes());
        w.prop(PROP_OFFSETS, &offsets);
        w.prop(PROP_VISIBLE, &(u32::from(visible).to_be_bytes()));
        if !path.is_empty() {
            let mut bytes = Vec::new();
            for index in path {
                bytes.extend(index.to_be_bytes());
            }
            w.prop(PROP_ITEM_PATH, &bytes);
        }
        w.end_props();
        let hierarchy_slot = w.pointer_slot();
        w.pointer_at(0);
        let hierarchy_at = w.data.len() as u64;
        w.patch_pointer(hierarchy_slot, hierarchy_at);
        w.u32(width);
        w.u32(height);
        w.u32(4);
        let level_slot = w.pointer_slot();
        w.pointer_at(0);
        let level_at = w.data.len() as u64;
        w.patch_pointer(level_slot, level_at);
        w.u32(width);
        w.u32(height);
        w.u32(4);
        let tile_slot = w.pointer_slot();
        w.pointer_at(0);
        let tile_at = w.data.len() as u64;
        w.patch_pointer(tile_slot, tile_at);
        let encoded = match compression {
            1 => rle_encode(pixels, 4),
            2 => {
                let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
                encoder.write_all(pixels).unwrap();
                encoder.finish().unwrap()
            }
            _ => pixels.to_vec(),
        };
        w.data.extend(encoded);
        at
    }

    #[test]
    fn reads_rle_group_stack_in_painter_order() {
        let bytes = two_layer_group(3, 1);
        let (doc, warnings) = read(&bytes, "Independent").unwrap();
        assert!(warnings.is_empty());
        assert_eq!((doc.width, doc.height), (2., 1.));
        assert_eq!(
            doc.layers
                .iter()
                .map(|l| l.name.as_str())
                .collect::<Vec<_>>(),
            ["Bottom blue", "Hidden red", "Top group"]
        );
        assert_eq!(doc.layers[1].parent, Some(doc.layers[2].id));
        assert!(!doc.layers[1].visible);
        assert_eq!(
            doc.layers[1].kind.raster_xform().unwrap().0,
            Pt::new(-2., 3.)
        );
        assert_eq!(
            doc.layers[1].kind.pixels().unwrap().data,
            [250, 20, 10, 128]
        );
        assert_eq!(
            doc.layers[0].kind.pixels().unwrap().data,
            [10, 20, 250, 255, 10, 20, 250, 255]
        );
    }

    #[test]
    fn reads_64bit_pointers_and_zlib_tiles() {
        let bytes = rgba_layer(11, "Zlib", 1, 1, &[9, 8, 7, 255], (4, 5), 2, &[], false);
        let (doc, _) = read(&bytes, "Wide").unwrap();
        assert_eq!(doc.layers[0].kind.pixels().unwrap().data, [9, 8, 7, 255]);
        assert_eq!(
            doc.layers[0].kind.raster_xform().unwrap().0,
            Pt::new(4., 5.)
        );
    }

    #[test]
    fn rejects_non_xcf_and_oversize_canvases() {
        assert!(read(b"not gimp", "x").is_err());
        let mut huge = Writer::new(0);
        huge.u32(33_000);
        huge.u32(1);
        huge.u32(0);
        huge.end_props();
        huge.pointer_at(0);
        huge.pointer_at(0);
        assert!(read(&huge.data, "Huge").unwrap_err().contains("32,768"));
    }

    #[test]
    fn rle_roundtrip_matches_planar_bytes() {
        let pixels = [1, 2, 3, 255, 1, 2, 3, 255, 9, 9, 9, 128];
        let encoded = rle_encode(&pixels, 4);
        let decoded = decode_rle(&encoded, pixels.len(), 4).unwrap();
        assert_eq!(decoded, pixels);
    }
}
