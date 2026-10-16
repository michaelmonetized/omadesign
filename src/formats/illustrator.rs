//! Editable Illustrator artwork from its private data or legacy PostScript operators.
//!
//! The tokenizer, postfix dictionaries and ATE key mappings are adapted from
//! VectorCraft b12a52f7249e0aa73f030587e05ed9ce0c73ab67. Its MIT notice is
//! retained in illustrator/LICENSE-MIT. No PostScript program is executed.

mod lex;
mod obj;
mod raster;
mod reader;
mod text;

use crate::document::Document;
use std::io::Read;
use std::path::Path;

const MAX_DATA: usize = 256 << 20;
const PICTURE_ERROR: &str = "Illustrator picture import failed: ";

/// Identify a picture failure that must not select a byte preview fallback.
/// Args: native import error. Returns whether the caller must preserve that failure.
pub(crate) fn picture_import_failed(error: &str) -> bool {
    error.starts_with(PICTURE_ERROR)
}

fn picture_error(error: String) -> String {
    format!("{PICTURE_ERROR}{error}")
}

/// Read editable Illustrator art.
/// `bytes` is an AI/EPS file and `name` names the new document. Returns editable
/// artwork and persistent fidelity notes, or an error suitable for a preview fallback.
pub fn read(bytes: &[u8], name: &str) -> Result<(Document, Vec<String>), String> {
    read_with_path(bytes, name, None)
}

/// Read editable Illustrator art with local placements.
/// `path` optionally locates sibling linked pictures. Returns the document and
/// notes; linked files are opened only beneath the source file's directory.
pub fn read_with_path(
    bytes: &[u8],
    name: &str,
    path: Option<&Path>,
) -> Result<(Document, Vec<String>), String> {
    if bytes.len() > MAX_DATA * 2 {
        return Err("Illustrator file exceeds the 512 MiB import limit".into());
    }
    let data = editing_data(bytes)?;
    let (mut doc, mut notes) = reader::read(&data, name, path)?;
    let id = doc.retain_source(crate::source_attachment::SourceAttachment::new(
        name,
        "illustrator",
        bytes.to_vec(),
    )?);
    for layer in &mut doc.layers {
        if layer.source_attachment.is_none() {
            layer.source_attachment = Some(id.clone());
        }
    }
    doc.validate_sources()?;
    notes.insert(0, "Illustrator editing data imported as native editable artwork; the PDF preview was not used as its document structure.".into());
    doc.import_notes = notes.clone();
    Ok((doc, notes))
}

/// Extract editing operators.
/// `bytes` is a complete AI/EPS file. Returns bounded decompressed private data,
/// or legacy operators when a PostScript Illustrator header identifies them.
pub fn editing_data(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.windows(5).take(1024).any(|w| w == b"%PDF-") {
        return decode_private(&pdf_private(bytes)?);
    }
    if let Some(at) = find(bytes, b"%AI9_PrivateDataBegin") {
        let rest = &bytes[at + b"%AI9_PrivateDataBegin".len()..];
        let end = find(rest, b"%AI9_PrivateDataEnd")
            .ok_or("Illustrator private data has no closing marker")?;
        let section = &rest[..end];
        let mut header = Vec::new();
        let mut lines = section.split(|b| matches!(b, b'\r' | b'\n'));
        let mut codec = None;
        for line in lines.by_ref() {
            if line.starts_with(b"%AI24_DataStream") {
                codec = Some(true);
                break;
            }
            if line.starts_with(b"%AI9_DataStream") {
                codec = Some(false);
                break;
            }
            header.extend_from_slice(line);
            header.push(b'\n');
        }
        if let Some(zstd) = codec {
            let packed: Vec<u8> = lines
                .flat_map(|line| line.strip_prefix(b"%").unwrap_or(line).iter().copied())
                .collect();
            header.extend(decompress(
                &ascii85(&packed)?,
                zstd,
                MAX_DATA.saturating_sub(header.len()),
            )?);
            return Ok(header);
        }
        return bounded(section.to_vec());
    }
    if bytes.starts_with(b"%!")
        && (find(bytes, b"Illustrator").is_some() || find(bytes, b"%AI5_BeginLayer").is_some())
    {
        return bounded(bytes.to_vec());
    }
    Err("No Illustrator editing data found; use the PDF preview or a legacy AI/EPS file".into())
}

fn bounded(data: Vec<u8>) -> Result<Vec<u8>, String> {
    if data.len() > MAX_DATA {
        Err("Illustrator editing data exceeds 256 MiB".into())
    } else {
        Ok(data)
    }
}

fn decode_private(raw: &[u8]) -> Result<Vec<u8>, String> {
    if raw.len() > MAX_DATA {
        return Err("Illustrator private data exceeds 256 MiB".into());
    }
    for (marker, zstd) in [
        (b"%AI24_ZStandard_Data".as_slice(), true),
        (b"%AI12_CompressedData".as_slice(), false),
    ] {
        if let Some(at) = find(raw, marker) {
            let mut data = raw[..at].to_vec();
            data.extend(decompress(&raw[at + marker.len()..], zstd, MAX_DATA - at)?);
            return Ok(data);
        }
    }
    Ok(raw.to_vec())
}

fn decompress(data: &[u8], zstd: bool, limit: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    if zstd {
        let mut decoder = zstd::stream::read::Decoder::new(data)
            .map_err(|e| format!("Damaged Illustrator Zstandard data: {e}"))?;
        decoder.window_log_max(26).map_err(|e| e.to_string())?;
        decoder
            .take(limit as u64 + 1)
            .read_to_end(&mut out)
            .map_err(|e| format!("Damaged Illustrator Zstandard data: {e}"))?;
    } else {
        flate2::read::ZlibDecoder::new(data)
            .take(limit as u64 + 1)
            .read_to_end(&mut out)
            .map_err(|e| format!("Damaged Illustrator zlib data: {e}"))?;
    }
    if out.len() > limit {
        return Err("Illustrator decompression exceeds the 256 MiB import limit".into());
    }
    Ok(out)
}

fn pdf_private(bytes: &[u8]) -> Result<Vec<u8>, String> {
    use lopdf::{Dictionary, Object};
    let pdf = lopdf::Document::load_mem_with_options(
        bytes,
        lopdf::LoadOptions::with_max_decompressed_size(MAX_DATA),
    )
    .map_err(|e| format!("Could not read Illustrator container: {e}"))?;
    if pdf.is_encrypted() {
        return Err("Illustrator container is encrypted; save an unlocked copy".into());
    }
    fn resolve<'a>(pdf: &'a lopdf::Document, object: &'a Object) -> Option<&'a Object> {
        pdf.dereference(object).ok().map(|(_, object)| object)
    }
    fn get<'a>(
        pdf: &'a lopdf::Document,
        dict: &'a Dictionary,
        key: &[u8],
    ) -> Option<&'a Dictionary> {
        dict.get(key)
            .ok()
            .and_then(|o| resolve(pdf, o))
            .and_then(|o| o.as_dict().ok())
    }
    let page_id = pdf
        .get_pages()
        .values()
        .next()
        .copied()
        .ok_or("Illustrator container has no page")?;
    let page = pdf.get_dictionary(page_id).map_err(|e| e.to_string())?;
    let private = get(&pdf, page, b"PieceInfo")
        .and_then(|d| get(&pdf, d, b"Illustrator"))
        .and_then(|d| get(&pdf, d, b"Private"))
        .ok_or("No Illustrator editing data in the PDF container")?;
    let prefix = if private.has(b"AIPrivateData1") {
        "AIPrivateData"
    } else {
        "AIPDFPrivateData"
    };
    let mut indices: Vec<usize> = private
        .iter()
        .filter_map(|(key, _)| {
            std::str::from_utf8(key)
                .ok()?
                .strip_prefix(prefix)?
                .parse()
                .ok()
        })
        .collect();
    indices.sort_unstable();
    if indices.is_empty()
        || indices.len() > 100_000
        || indices.iter().enumerate().any(|(i, n)| *n != i + 1)
    {
        return Err("Illustrator private streams are missing or out of sequence".into());
    }
    let mut raw = Vec::new();
    for i in indices {
        let object = resolve(
            &pdf,
            private
                .get(format!("{prefix}{i}").as_bytes())
                .map_err(|e| e.to_string())?,
        )
        .ok_or("Broken Illustrator private stream reference")?;
        let stream = object.as_stream().map_err(|e| e.to_string())?;
        if stream
            .dict
            .get(b"DecodeParms")
            .ok()
            .is_some_and(|p| !matches!(p, Object::Null))
        {
            return Err("Illustrator private stream uses unsupported decode parameters".into());
        }
        let filters = if stream.dict.has(b"Filter") {
            stream.filters().map_err(|e| e.to_string())?
        } else {
            vec![]
        };
        let data = match filters.as_slice() {
            [] => stream.content.clone(),
            [filter] if matches!(*filter, b"FlateDecode" | b"Fl") => {
                decompress(&stream.content, false, MAX_DATA - raw.len())?
            }
            [filter] if matches!(*filter, b"ASCII85Decode" | b"A85") => ascii85(&stream.content)?,
            _ => return Err("Unsupported Illustrator private stream compression".into()),
        };
        if data.len() > MAX_DATA - raw.len() {
            return Err("Illustrator private streams exceed 256 MiB".into());
        }
        raw.extend(data);
    }
    Ok(raw)
}

fn find(data: &[u8], needle: &[u8]) -> Option<usize> {
    data.windows(needle.len()).position(|w| w == needle)
}

fn hex_decode(data: &[u8]) -> Option<Vec<u8>> {
    let mut digits: Vec<u8> = data
        .iter()
        .copied()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    if digits.len() % 2 != 0 {
        digits.push(b'0');
    }
    digits
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).ok()?, 16).ok())
        .collect()
}

fn ascii85(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut data = data.trim_ascii();
    if let Some(rest) = data.strip_prefix(b"<~") {
        data = rest;
    }
    let mut out = Vec::new();
    let mut group = Vec::with_capacity(5);
    for &b in data {
        if b.is_ascii_whitespace() {
            continue;
        }
        if b == b'~' {
            break;
        }
        if b == b'z' && group.is_empty() {
            out.extend_from_slice(&[0; 4]);
        } else if (b'!'..=b'u').contains(&b) {
            group.push(b - b'!');
            if group.len() == 5 {
                let value = group.iter().fold(0u64, |n, b| n * 85 + u64::from(*b));
                let value = u32::try_from(value).map_err(|_| "Damaged Illustrator ASCII85 data")?;
                out.extend_from_slice(&value.to_be_bytes());
                group.clear();
            }
        } else {
            return Err("Damaged Illustrator ASCII85 data".into());
        }
        if out.len() > MAX_DATA {
            return Err("Illustrator ASCII85 data exceeds 256 MiB".into());
        }
    }
    if group.len() == 1 {
        return Err("Incomplete Illustrator ASCII85 group".into());
    }
    if !group.is_empty() {
        let n = group.len();
        group.resize(5, 84);
        let value = group.iter().fold(0u64, |n, b| n * 85 + u64::from(*b));
        let value = u32::try_from(value).map_err(|_| "Damaged Illustrator ASCII85 data")?;
        out.extend_from_slice(&value.to_be_bytes()[..n - 1]);
    }
    bounded(out)
}

#[cfg(test)]
mod tests;
