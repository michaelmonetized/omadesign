use super::{MAX_DECODED_BYTES, MAX_LAYERS, Rect, dimensions, warn};
use std::io::Read;

pub(super) struct Reader<'a> {
    pub bytes: &'a [u8],
    pub pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }
    pub fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or("Photoshop section length overflow")?;
        let bytes = self
            .bytes
            .get(self.pos..end)
            .ok_or("Truncated Photoshop file")?;
        self.pos = end;
        Ok(bytes)
    }
    pub fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    pub fn i16(&mut self) -> Result<i16, String> {
        Ok(self.u16()? as i16)
    }
    pub fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn i32(&mut self) -> Result<i32, String> {
        Ok(self.u32()? as i32)
    }
    pub fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
    pub fn key(&mut self) -> Result<[u8; 4], String> {
        Ok(self.take(4)?.try_into().unwrap())
    }
    pub fn length(&mut self, large: bool) -> Result<usize, String> {
        let len = if large {
            self.u64()?
        } else {
            self.u32()? as u64
        };
        usize::try_from(len).map_err(|_| "Photoshop section too large".into())
    }
    pub fn section(&mut self, large: bool) -> Result<Self, String> {
        let len = self.length(large)?;
        Ok(Self::new(self.take(len)?))
    }
    pub fn rect(&mut self) -> Result<Rect, String> {
        Ok(Rect {
            top: self.i32()?,
            left: self.i32()?,
            bottom: self.i32()?,
            right: self.i32()?,
        })
    }
    pub fn pascal(&mut self, alignment: usize) -> Result<String, String> {
        let n = self.u8()? as usize;
        let bytes = self.take(n)?;
        self.take((alignment - (n + 1) % alignment) % alignment)?;
        // Modern files carry a `luni` UTF-16 name; ASCII legacy names remain exact.
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }
}

#[derive(Clone)]
pub(super) struct Mask {
    pub rect: Rect,
    pub background: u8,
    pub flags: u8,
    pub density: u8,
}

pub(super) struct Record {
    pub rect: Rect,
    pub channels: Vec<(i16, usize)>,
    pub pixels: Vec<(i16, Vec<u8>)>,
    pub name: String,
    pub blend: [u8; 4],
    pub opacity: u8,
    pub flags: u8,
    pub clipping: bool,
    pub section: u32,
    pub mask: Option<Mask>,
    pub real_mask: Option<Mask>,
    pub filters: crate::filter::FilterStack,
}

pub(super) struct Parsed {
    pub width: u32,
    pub height: u32,
    pub mode: u16,
    pub dpi: f32,
    pub layers: Vec<Record>,
    pub merged: Option<Vec<Vec<u8>>>,
    pub warnings: Vec<String>,
}

pub(super) fn parse(bytes: &[u8]) -> Result<Parsed, String> {
    let mut r = Reader::new(bytes);
    if r.take(4)? != b"8BPS" {
        return Err("Not a Photoshop PSD/PSB file".into());
    }
    let version = r.u16()?;
    if version != 1 && version != 2 {
        return Err(format!("Unsupported Photoshop version {version}"));
    }
    let large = version == 2;
    if r.take(6)?.iter().any(|b| *b != 0) {
        return Err("Invalid Photoshop header".into());
    }
    let channels = r.u16()? as usize;
    let height = r.u32()?;
    let width = r.u32()?;
    let depth = r.u16()?;
    let mode = r.u16()?;
    let edge_limit = if large { 300_000 } else { 30_000 };
    if !(1..=56).contains(&channels)
        || width == 0
        || height == 0
        || width > edge_limit
        || height > edge_limit
    {
        return Err("Invalid Photoshop canvas dimensions or channel count".into());
    }
    dimensions(width, height)?;
    if ![8, 16, 32].contains(&depth) {
        return Err(format!(
            "Photoshop {depth}-bit depth is not supported; use 8, 16, or 32-bit RGB."
        ));
    }
    if ![1, 3, 4].contains(&mode) {
        return Err(format!(
            "Photoshop color mode {mode} is not supported; convert to RGB or grayscale before importing."
        ));
    }
    if channels < color_channels(mode) {
        return Err("Photoshop file has too few color channels".into());
    }
    let mut warnings = Vec::new();
    if depth != 8 {
        warn(
            &mut warnings,
            format!(
                "{depth}-bit Photoshop channels were converted to the document's 8-bit precision."
            ),
        );
    }
    if mode == 4 {
        warn(
            &mut warnings,
            "CMYK pixels were converted to RGB without an ICC color transform; print colors can differ.",
        );
    }
    let _ = r.section(false)?;
    let mut resources = r.section(false)?;
    let mut dpi = 72.0;
    while resources.remaining() >= 12 {
        if resources.take(4)? != b"8BIM" {
            return Err("Invalid Photoshop image resource signature".into());
        }
        let id = resources.u16()?;
        let _ = resources.pascal(2)?;
        let mut data = resources.section(false)?;
        if id == 1005 && data.remaining() >= 16 {
            dpi = data.u32()? as f32 / 65536.0;
            if !(1.0..=100_000.0).contains(&dpi) {
                dpi = 72.0;
            }
        }
        if id == 1039 {
            warn(
                &mut warnings,
                "The embedded color profile is not applied; pixels are displayed in the document's RGB space.",
            );
        }
        if data.bytes.len() % 2 != 0 {
            resources.take(1)?;
        }
    }
    let mut layer_mask = r.section(large)?;
    let mut layers = Vec::new();
    let mut merged_alpha = false;
    let mut allocation = 0usize;
    if layer_mask.remaining() > 0 {
        let mut info = layer_mask.section(large)?;
        if info.remaining() > 0 {
            (layers, merged_alpha) =
                parse_layers(&mut info, large, depth, &mut allocation, &mut warnings)?;
        }
        if layer_mask.remaining() >= 4 {
            let _ = layer_mask.section(false)?;
        }
        while layer_mask.remaining() >= 12 {
            let (key, data) = tagged(&mut layer_mask, large, 4)?;
            if matches!(&key, b"Layr" | b"Lr16" | b"Lr32") && layers.is_empty() {
                let mut info = Reader::new(data);
                (layers, merged_alpha) =
                    parse_layers(&mut info, large, depth, &mut allocation, &mut warnings)?;
            }
        }
    }
    let merged = if layers.is_empty() {
        let pixels = dimensions(width, height)?;
        budget(
            &mut allocation,
            pixels
                .checked_mul(channels)
                .ok_or("Photoshop channel count overflow")?,
        )?;
        let samples = decode_channel(
            r.take(r.remaining())?,
            width,
            height
                .checked_mul(channels as u32)
                .ok_or("Photoshop merged channel overflow")?,
            depth,
            large,
        )?;
        let mut merged = samples
            .chunks_exact(pixels)
            .map(|p| p.to_vec())
            .collect::<Vec<_>>();
        if !merged_alpha && merged.len() > color_channels(mode) {
            warn(
                &mut warnings,
                "Extra merged alpha or spot channels were not treated as document transparency.",
            );
            merged.truncate(color_channels(mode));
        }
        Some(merged)
    } else {
        None
    };
    Ok(Parsed {
        width,
        height,
        mode,
        dpi,
        layers,
        merged,
        warnings,
    })
}

pub(super) fn color_channels(mode: u16) -> usize {
    if mode == 1 {
        1
    } else if mode == 4 {
        4
    } else {
        3
    }
}

fn budget(total: &mut usize, additional: usize) -> Result<(), String> {
    *total = total
        .checked_add(additional)
        .ok_or("Photoshop decoded size overflow")?;
    if *total > MAX_DECODED_BYTES {
        return Err("Photoshop layers exceed the 512 MiB decoded import limit".into());
    }
    Ok(())
}

fn tagged<'a>(
    r: &mut Reader<'a>,
    large: bool,
    alignment: usize,
) -> Result<([u8; 4], &'a [u8]), String> {
    let sig = r.key()?;
    if &sig != b"8BIM" && &sig != b"8B64" {
        return Err("Invalid Photoshop additional data signature".into());
    }
    let key = r.key()?;
    let wide = large
        && matches!(
            &key,
            b"LMsk"
                | b"Lr16"
                | b"Lr32"
                | b"Layr"
                | b"Mt16"
                | b"Mt32"
                | b"Mtrn"
                | b"Alph"
                | b"FMsk"
                | b"lnk2"
                | b"FEid"
                | b"FXid"
                | b"PxSD"
        );
    let len = r.length(wide)?;
    let data = r.take(len)?;
    let padding = (alignment - len % alignment) % alignment;
    if padding <= r.remaining() {
        r.take(padding)?;
    }
    Ok((key, data))
}

fn parse_layers(
    r: &mut Reader<'_>,
    large: bool,
    depth: u16,
    allocation: &mut usize,
    warnings: &mut Vec<String>,
) -> Result<(Vec<Record>, bool), String> {
    let count = r.i16()?;
    let n = count.unsigned_abs() as usize;
    if n > MAX_LAYERS {
        return Err("Photoshop file exceeds the 8192 layer import limit".into());
    }
    let mut records = Vec::with_capacity(n);
    for _ in 0..n {
        let rect = r.rect()?;
        rect.size()?;
        let channels = r.u16()? as usize;
        if channels > 56 {
            return Err("Too many Photoshop layer channels".into());
        }
        let mut channel_info = Vec::with_capacity(channels);
        for _ in 0..channels {
            channel_info.push((r.i16()?, r.length(large)?));
        }
        if r.take(4)? != b"8BIM" {
            return Err("Invalid Photoshop layer blend signature".into());
        }
        let blend = r.key()?;
        let opacity = r.u8()?;
        let clipping = r.u8()? != 0;
        let mut flags = r.u8()?;
        r.take(1)?;
        let mut extra = r.section(false)?;
        let mut mask_data = extra.section(false)?;
        let (mask, real_mask) = if mask_data.remaining() >= 18 {
            let rect = mask_data.rect()?;
            rect.size()?;
            let background = mask_data.u8()?;
            let flags = mask_data.u8()?;
            let mut density = 255;
            // Actual Photoshop files store the real-mask header before mask
            // parameters, contrary to the published table. Presence follows
            // channel -3, not block length (feather parameters can be long).
            let mut real = if channel_info.iter().any(|(id, _)| *id == -3) {
                let real_flags = mask_data.u8()?;
                let real_background = mask_data.u8()?;
                let real_rect = mask_data.rect()?;
                real_rect.size()?;
                Some(Mask {
                    rect: real_rect,
                    background: real_background,
                    flags: real_flags,
                    density,
                })
            } else {
                None
            };
            if flags & 16 != 0 {
                let params = mask_data.u8()?;
                if params & 1 != 0 {
                    density = mask_data.u8()?;
                }
                if params & 2 != 0 {
                    mask_data.take(8)?;
                    warn(warnings, "Photoshop mask feathering is not applied.");
                }
                if params & 4 != 0 {
                    mask_data.take(1)?;
                }
                if params & 8 != 0 {
                    mask_data.take(8)?;
                }
            }
            if let Some(real_mask) = &mut real {
                real_mask.density = density;
            }
            (
                Some(Mask {
                    rect,
                    background,
                    flags,
                    density,
                }),
                real,
            )
        } else {
            (None, None)
        };
        let ranges = extra.section(false)?;
        if ranges
            .bytes
            .as_chunks::<8>()
            .0
            .iter()
            .any(|v| *v != [0, 0, 255, 255, 0, 0, 255, 255])
        {
            warn(
                warnings,
                "Custom Photoshop Blend If ranges are not applied.",
            );
        }
        let mut name = extra.pascal(4)?;
        let mut section = 0;
        let mut record_blend = blend;
        let mut filters = crate::filter::FilterStack::default();
        while extra.remaining() >= 12 {
            let (key, data) = tagged(&mut extra, large, 1)?;
            let mut value = Reader::new(data);
            match &key {
                b"luni" => {
                    let n = value.u32()? as usize;
                    if n > 32_768 || n > value.remaining() / 2 {
                        return Err("Invalid Photoshop Unicode layer name".into());
                    }
                    let units = (0..n).map(|_| value.u16()).collect::<Result<Vec<_>, _>>()?;
                    name = String::from_utf16_lossy(&units)
                        .trim_end_matches('\0')
                        .to_string();
                }
                b"lsct" | b"lsdk" => {
                    section = value.u32()?;
                    if value.remaining() >= 8 && value.take(4)? == b"8BIM" {
                        record_blend = value.key()?;
                    }
                }
                b"lspf" if value.remaining() >= 4 => {
                    if value.u32()? & 7 != 0 {
                        flags |= 1;
                    }
                }
                b"TySh" | b"tySh" => warn(
                    warnings,
                    "Photoshop text layers are imported as editable pixel layers; text formatting is not converted.",
                ),
                b"SoLd" | b"SoLE" | b"PlLd" | b"plLd" => warn(
                    warnings,
                    "Smart objects are imported from their saved layer pixels; embedded documents and smart filters are not editable.",
                ),
                b"vmsk" | b"vsms" => warn(
                    warnings,
                    "Photoshop vector masks are not converted to editable paths; saved pixel masks are used when present.",
                ),
                b"lrFX" => {
                    filters = super::effects::decode(data, warnings)?;
                }
                b"lfx2" | b"lmfx" => warn(
                    warnings,
                    "Photoshop effects beyond supported Normal color overlays are not recreated and can change the appearance.",
                ),
                b"SoCo" | b"GdFl" | b"PtFl" | b"brit" | b"levl" | b"curv" | b"expA" | b"vibA"
                | b"hue " | b"hue2" | b"blnc" | b"blwh" | b"phfl" | b"mixr" | b"clrL" | b"nvrt"
                | b"post" | b"thrs" | b"grdm" | b"selc" => warn(
                    warnings,
                    "Photoshop adjustment and fill parameters are not recreated; saved pixels are retained when available.",
                ),
                b"iOpa" if !data.is_empty() && data[0] != 255 => warn(
                    warnings,
                    "Photoshop fill opacity is not separately applied.",
                ),
                _ => {}
            }
        }
        budget(allocation, name.len())?;
        records.push(Record {
            rect,
            channels: channel_info,
            pixels: Vec::new(),
            name,
            blend: record_blend,
            opacity,
            flags,
            clipping,
            section,
            mask,
            real_mask,
            filters,
        });
    }
    for record in &mut records {
        for &(id, len) in &record.channels {
            let bytes = r.take(len)?;
            if len == 0 {
                continue;
            }
            let rect = match id {
                -2 => record.mask.as_ref().map(|m| m.rect).unwrap_or(record.rect),
                -3 => record
                    .real_mask
                    .as_ref()
                    .or(record.mask.as_ref())
                    .map(|m| m.rect)
                    .unwrap_or(record.rect),
                _ => record.rect,
            };
            let (w, h) = rect.size()?;
            if w == 0 || h == 0 {
                continue;
            }
            // Only RGB/CMYK/gray, alpha and raster mask channels can affect our document.
            if !(-3..=3).contains(&id) {
                warn(
                    warnings,
                    "Photoshop spot or auxiliary channels are not imported.",
                );
                continue;
            }
            let pixels = dimensions(w, h)?;
            budget(allocation, pixels)?;
            record
                .pixels
                .push((id, decode_channel(bytes, w, h, depth, large)?));
        }
        let (w, h) = record.rect.size()?;
        budget(
            allocation,
            dimensions(w, h)?
                .checked_mul(8)
                .ok_or("Photoshop layer size overflow")?,
        )?;
    }
    Ok((records, count < 0))
}

pub(super) fn decode_channel(
    bytes: &[u8],
    w: u32,
    h: u32,
    depth: u16,
    large: bool,
) -> Result<Vec<u8>, String> {
    let bps = depth as usize / 8;
    let row_bytes = (w as usize)
        .checked_mul(bps)
        .ok_or("Photoshop row overflow")?;
    let expected = row_bytes
        .checked_mul(h as usize)
        .ok_or("Photoshop channel size overflow")?;
    if expected > MAX_DECODED_BYTES {
        return Err("Photoshop channel exceeds the decoded size limit".into());
    }
    let mut r = Reader::new(bytes);
    let compression = r.u16()?;
    let mut data = match compression {
        0 => r.take(expected)?.to_vec(),
        1 => {
            let entry_bytes = if large { 4 } else { 2 };
            if h as usize > r.remaining() / entry_bytes {
                return Err("Truncated Photoshop RLE row lengths".into());
            }
            let mut sizes = Vec::with_capacity(h as usize);
            for _ in 0..h {
                sizes.push(if large {
                    r.u32()? as usize
                } else {
                    r.u16()? as usize
                });
            }
            let mut data = Vec::with_capacity(expected);
            for size in sizes {
                unpack_bits(r.take(size)?, row_bytes, &mut data)?;
            }
            data
        }
        2 | 3 => {
            let mut data = Vec::new();
            let decoder = flate2::read::ZlibDecoder::new(r.take(r.remaining())?);
            decoder
                .take(expected as u64 + 1)
                .read_to_end(&mut data)
                .map_err(|e| format!("Invalid Photoshop ZIP channel: {e}"))?;
            if data.len() != expected {
                return Err("Photoshop ZIP channel has an invalid decoded size".into());
            }
            data
        }
        _ => return Err(format!("Unsupported Photoshop compression {compression}")),
    };
    if compression == 3 {
        for row in data.chunks_exact_mut(row_bytes) {
            if depth == 16 {
                let mut last = 0u16;
                for sample in row.as_chunks_mut::<2>().0.iter_mut() {
                    last = last.wrapping_add(u16::from_be_bytes([sample[0], sample[1]]));
                    sample.copy_from_slice(&last.to_be_bytes());
                }
            } else {
                for i in 1..row.len() {
                    row[i] = row[i].wrapping_add(row[i - 1]);
                }
                if depth == 32 {
                    let planar = row.to_vec();
                    let width = w as usize;
                    for x in 0..width {
                        for b in 0..4 {
                            row[x * 4 + b] = planar[b * width + x];
                        }
                    }
                }
            }
        }
    }
    Ok(match depth {
        8 => data,
        16 => data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| ((u16::from_be_bytes([b[0], b[1]]) as u32 + 128) / 257) as u8)
            .collect(),
        32 => data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| (f32::from_be_bytes(*b).clamp(0.0, 1.0) * 255.0).round() as u8)
            .collect(),
        _ => return Err("Unsupported Photoshop channel depth".into()),
    })
}

fn unpack_bits(encoded: &[u8], expected: usize, output: &mut Vec<u8>) -> Result<(), String> {
    let start = output.len();
    let mut r = Reader::new(encoded);
    while r.remaining() > 0 {
        let control = r.u8()? as i8;
        let n = if control >= 0 {
            control as usize + 1
        } else if control != -128 {
            (1i16 - control as i16) as usize
        } else {
            continue;
        };
        if output.len() - start + n > expected {
            return Err("Photoshop RLE row exceeds its declared width".into());
        }
        if control >= 0 {
            output.extend_from_slice(r.take(n)?);
        } else {
            let value = r.u8()?;
            output.resize(output.len() + n, value);
        }
    }
    if output.len() - start != expected {
        return Err("Photoshop RLE row is shorter than its declared width".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn packbits_runs_literals_and_noop_obey_exact_row_boundaries() {
        let mut decoded = Vec::new();
        unpack_bits(&[128, 254, 7, 1, 3, 4], 5, &mut decoded).unwrap();
        assert_eq!(decoded, [7, 7, 7, 3, 4]);
        assert!(unpack_bits(&[129, 7], 3, &mut Vec::new()).is_err());
        assert!(unpack_bits(&[3, 7], 4, &mut Vec::new()).is_err());
        assert!(decode_channel(&[0, 1, 0, 1], 1, 40_000_000, 8, false).is_err());
    }

    #[test]
    fn zip_predictors_restore_eight_sixteen_and_float_samples() {
        for (depth, predicted, expected) in [
            (
                8,
                vec![10, 10, 5, 90, 246, 255],
                vec![10, 20, 25, 90, 80, 79],
            ),
            (
                16,
                vec![0, 0, 128, 0, 127, 255, 255, 255, 128, 1, 128, 0],
                vec![0, 128, 255, 255, 128, 0],
            ),
            // Float bytes are shuffled before byte-wise delta prediction.
            (
                32,
                vec![0, 63, 0, 193, 0, 128, 128, 0, 0, 0, 0, 0],
                vec![0, 128, 255],
            ),
        ] {
            let mut data = vec![0, 3];
            let mut zip = flate2::write::ZlibEncoder::new(&mut data, flate2::Compression::fast());
            zip.write_all(&predicted).unwrap();
            zip.finish().unwrap();
            let h = if depth == 32 { 1 } else { 2 };
            assert_eq!(
                decode_channel(&data, 3, h, depth, false).unwrap(),
                expected,
                "depth={depth}"
            );
        }
    }

    #[test]
    fn zip_bombs_stop_at_declared_channel_size() {
        let mut bytes = vec![0, 2];
        let mut zip = flate2::write::ZlibEncoder::new(&mut bytes, flate2::Compression::best());
        zip.write_all(&[0; 65_536]).unwrap();
        zip.finish().unwrap();
        assert!(decode_channel(&bytes, 1, 1, 8, false).is_err());
    }
}
