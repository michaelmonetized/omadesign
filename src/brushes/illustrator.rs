//! Bounded Illustrator brush artwork decoding; never executes PostScript.
use super::{Pack, Preset, Tip};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
};
use tiny_skia::{FillRule, Paint, Path, PathBuilder, Pixmap, Transform};

pub(super) fn decode(bytes: &[u8], name: &str) -> Result<Pack, String> {
    let source = private_data(bytes)?;
    let text = String::from_utf8_lossy(&source).replace('\r', "\n");
    let mut pack = Pack::new(name);
    pack.notes.push("Illustrator art and pattern artwork is converted to sampled monochrome tips. Vector stretching, pattern corners, scattering, bristles, and pressure dynamics are not retained.".into());
    let mut names = BTreeMap::new();
    for block in text.split("%AI8_BeginPluginObject").skip(1) {
        let block = block.split("%AI8_EndPluginObject").next().unwrap_or(block);
        let strings = literals(block);
        if strings.len() >= 3 && strings[0].contains("Brush Tool") {
            let data = strings[2..].join("");
            let fields: Vec<_> = data.split('/').collect();
            if let Some(pattern) = fields.get(1) {
                names.insert(pattern.trim().to_owned(), strings[1].clone());
            }
        }
    }
    for block in text.split("%AI8_BeginBrushPattern").skip(1) {
        let block = block
            .split_once("%AI8_EndBrushPattern")
            .ok_or("Truncated Illustrator brush pattern")?
            .0;
        let pattern_name = literals(block)
            .first()
            .cloned()
            .unwrap_or_else(|| format!("Tip {}", pack.brushes.len() + 1));
        if !names.is_empty() && !names.contains_key(&pattern_name) {
            continue;
        }
        let brush_name = names.get(&pattern_name).cloned().unwrap_or(pattern_name);
        match render_pattern(block) {
            Ok((tip, unsupported)) => {
                if !unsupported.is_empty() {
                    pack.notes.push(format!(
                        "{brush_name}: skipped unsupported Illustrator operations: {}",
                        unsupported.into_iter().collect::<Vec<_>>().join(", ")
                    ));
                }
                pack.brushes.push(Preset::new(brush_name, tip, 0.15));
            }
            Err(e) => pack.notes.push(format!("Skipped {brush_name}: {e}")),
        }
        if pack.brushes.len() > super::MAX_BRUSHES
            || pack
                .brushes
                .iter()
                .map(|p| p.tip.alpha.len())
                .sum::<usize>()
                > super::MAX_FILE
        {
            return Err("Illustrator brush library exceeds decoded limits".into());
        }
    }
    if pack.brushes.is_empty() {
        return Err("This Illustrator file contains no supported brush patterns. In Illustrator, apply a brush to a path, expand appearance, export SVG, and import that SVG as a tip.".into());
    }
    Ok(pack)
}

fn private_data(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.starts_with(b"%!") {
        return Ok(bytes.to_vec());
    }
    let pdf = lopdf::Document::load_mem_with_options(
        bytes,
        lopdf::LoadOptions::with_max_decompressed_size(super::MAX_FILE),
    )
    .map_err(|e| format!("Could not read Illustrator library: {e}"))?;
    let mut pieces = BTreeMap::new();
    for object in pdf.objects.values() {
        if let Ok(dict) = object.as_dict() {
            for (key, value) in dict.iter() {
                if let Some(index) = key
                    .strip_prefix(b"AIPrivateData")
                    .and_then(|v| std::str::from_utf8(v).ok())
                    .and_then(|s| s.parse::<u32>().ok())
                {
                    let id = value.as_reference().map_err(|e| e.to_string())?;
                    let stream = pdf
                        .get_object(id)
                        .and_then(|o| o.as_stream())
                        .map_err(|e| e.to_string())?;
                    let data = if stream.dict.get(b"Filter").is_err() {
                        stream.content.clone()
                    } else {
                        stream
                            .decompressed_content_with_limit(super::MAX_FILE)
                            .map_err(|e| e.to_string())?
                    };
                    pieces.insert(index, data);
                }
            }
        }
    }
    if pieces.values().map(Vec::len).sum::<usize>() > super::MAX_FILE {
        return Err("Illustrator private data exceeds 128 MiB".into());
    }
    let source: Vec<_> = pieces.into_values().flatten().collect();
    if source
        .windows(b"%AI24_ZStandard_Data".len())
        .any(|w| w == b"%AI24_ZStandard_Data")
    {
        let offset = source
            .windows(4)
            .position(|b| b == [0x28, 0xb5, 0x2f, 0xfd])
            .ok_or("Missing Illustrator Zstandard frame")?;
        let mut data = Vec::new();
        zstd::stream::read::Decoder::new(&source[offset..])
            .map_err(|e| e.to_string())?
            .take(super::MAX_FILE as u64 + 1)
            .read_to_end(&mut data)
            .map_err(|e| format!("Invalid Illustrator Zstandard data: {e}"))?;
        if data.len() > super::MAX_FILE {
            return Err("Illustrator private data expands beyond 128 MiB".into());
        }
        Ok(data)
    } else if let Some(at) = source
        .windows(b"%AI12_CompressedData".len())
        .position(|w| w == b"%AI12_CompressedData")
    {
        let mut offset = at + b"%AI12_CompressedData".len();
        while source
            .get(offset)
            .is_some_and(|b| matches!(b, b'\n' | b'\r'))
        {
            offset += 1;
        }
        let mut data = Vec::new();
        flate2::read::ZlibDecoder::new(&source[offset..])
            .take(super::MAX_FILE as u64 + 1)
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        if data.len() > super::MAX_FILE {
            return Err("Illustrator data expands beyond 128 MiB".into());
        }
        Ok(data)
    } else {
        Ok(source)
    }
}

fn literals(text: &str) -> Vec<String> {
    let mut strings = vec![];
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '(' {
            continue;
        }
        let mut s = String::new();
        let mut depth = 1;
        while let Some(ch) = chars.next() {
            match ch {
                '\\' => {
                    if let Some(next) = chars.next() {
                        s.push(next);
                    }
                }
                '(' => {
                    depth += 1;
                    s.push(ch);
                }
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    s.push(ch);
                }
                _ => s.push(ch),
            }
        }
        strings.push(s);
    }
    strings
}

fn render_pattern(block: &str) -> Result<(Tip, BTreeSet<String>), String> {
    let mut paths: Vec<(Path, u8, Option<f32>)> = vec![];
    let mut pb = PathBuilder::new();
    let mut nums = Vec::<f32>::new();
    let (mut fill, mut stroke, mut width) = (0u8, 0u8, 1.0f32);
    let mut unsupported = BTreeSet::new();
    let mut point = (0.0, 0.0);
    let mut segments = 0usize;
    for line in block.lines() {
        if line.starts_with('%') || line.starts_with('(') {
            continue;
        }
        for token in line.split_whitespace() {
            if let Ok(value) = token.parse::<f32>() {
                if !value.is_finite() || value.abs() > 1e7 {
                    return Err("Invalid Illustrator coordinate".into());
                }
                nums.push(value);
                if nums.len() > 4096 {
                    return Err("Excessive Illustrator operands".into());
                }
                continue;
            }
            let n = nums.len();
            let v = |i: usize| nums.get(n.saturating_sub(i)).copied().unwrap_or(0.0);
            match token {
                "m" if n >= 2 => {
                    pb.move_to(v(2), v(1));
                    point = (v(2), v(1));
                }
                "l" | "L" if n >= 2 => {
                    pb.line_to(v(2), v(1));
                    point = (v(2), v(1));
                }
                "c" | "C" if n >= 6 => {
                    pb.cubic_to(v(6), v(5), v(4), v(3), v(2), v(1));
                    point = (v(2), v(1));
                }
                "v" | "V" if n >= 4 => {
                    pb.cubic_to(point.0, point.1, v(4), v(3), v(2), v(1));
                    point = (v(2), v(1));
                }
                "y" | "Y" if n >= 4 => {
                    pb.cubic_to(v(4), v(3), v(2), v(1), v(2), v(1));
                    point = (v(2), v(1));
                }
                "h" => pb.close(),
                "k" | "K" if n >= 4 => {
                    let luminance = ((1.0 - v(1))
                        * (1.0 - (v(4) * 0.2126 + v(3) * 0.7152 + v(2) * 0.0722)))
                        .clamp(0.0, 1.0);
                    let c = (luminance * 255.0).round() as u8;
                    if token == "k" {
                        fill = c;
                    } else {
                        stroke = c;
                    }
                }
                "g" | "G" if n >= 1 => {
                    let c = (v(1).clamp(0.0, 1.0) * 255.0).round() as u8;
                    if token == "g" {
                        fill = c;
                    } else {
                        stroke = c;
                    }
                }
                "Xa" | "XA" if n >= 3 => {
                    let c = ((v(3) * 0.2126 + v(2) * 0.7152 + v(1) * 0.0722).clamp(0.0, 1.0)
                        * 255.0)
                        .round() as u8;
                    if token == "Xa" {
                        fill = c;
                    } else {
                        stroke = c;
                    }
                }
                "w" if n >= 1 => width = v(1).clamp(0.0, 2048.0),
                "f" | "F" | "s" | "S" | "b" | "B" | "n" | "N" => {
                    if matches!(token, "f" | "s" | "b") {
                        pb.close();
                    }
                    if let Some(path) = std::mem::replace(&mut pb, PathBuilder::new()).finish() {
                        if matches!(token, "f" | "F" | "b" | "B") {
                            paths.push((path.clone(), fill, None));
                        }
                        if matches!(token, "s" | "S" | "b" | "B") {
                            paths.push((path, stroke, Some(width)));
                        }
                    }
                }
                "u" | "U" | "A" | "Xw" | "Ae" | "As" | "Xy" | "J" | "j" | "M" | "d" | "XR"
                | "O" | "D" | "i" | "XW" | "*u" | "*U" | "[]0" | "()" | "Ap" => {}
                other => {
                    if unsupported.len() < 32 {
                        unsupported.insert(other.to_owned());
                    }
                }
            }
            nums.clear();
            segments += 1;
            if segments > 500_000 {
                return Err("Illustrator tip exceeds path complexity limit".into());
            }
        }
    }
    if paths.is_empty() {
        return Err("No supported painted paths".into());
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (path, _, stroke) in &paths {
        let b = path.bounds();
        let pad = stroke.unwrap_or(0.0) / 2.0;
        x0 = x0.min(b.left() - pad);
        y0 = y0.min(b.top() - pad);
        x1 = x1.max(b.right() + pad);
        y1 = y1.max(b.bottom() + pad);
    }
    let s = 510.0 / (x1 - x0).max(y1 - y0).max(1.0);
    let mut pm = Pixmap::new(
        ((x1 - x0) * s).ceil() as u32 + 2,
        ((y1 - y0) * s).ceil() as u32 + 2,
    )
    .ok_or("Invalid Illustrator brush bounds")?;
    pm.fill(tiny_skia::Color::WHITE);
    let transform = Transform::from_row(s, 0.0, 0.0, -s, 1.0 - x0 * s, 1.0 + y1 * s);
    for (path, c, stroke) in paths {
        let mut paint = Paint::default();
        paint.set_color_rgba8(c, c, c, 255);
        if let Some(width) = stroke {
            pm.stroke_path(
                &path,
                &paint,
                &tiny_skia::Stroke {
                    width,
                    ..Default::default()
                },
                transform,
                None,
            );
        } else {
            pm.fill_path(&path, &paint, FillRule::Winding, transform, None);
        }
    }
    let pixels = crate::document::Pixels::from_pixmap(&pm)?;
    Ok((
        super::tip_from_rgba(pixels.w, pixels.h, &pixels.data)?,
        unsupported,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_vector_paths_import_without_executing_postscript() {
        let bytes=b"%!PS-Adobe-3.0\n%AI8_BeginBrushPattern\n(Test)\n0 0 0 1 k\n0 0 m\n20 0 L\n20 10 L\n0 10 L\nf\n%AI8_EndBrushPattern\n(delete everything) system\n";
        let pack = decode(bytes, "Legacy").unwrap();
        assert_eq!(pack.brushes.len(), 1);
        assert_eq!(pack.brushes[0].name, "Test");
        assert!(pack.brushes[0].tip.width > pack.brushes[0].tip.height);
        assert!(decode(b"%!PS-Adobe\n%AI8_BeginBrushPattern\nbroken", "Broken").is_err());
    }
}
