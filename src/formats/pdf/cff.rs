//! Embedded Type 1C glyph outlines. PDF producers commonly subset CFF without
//! an OpenType wrapper, so these fonts cannot be loaded as ordinary live fonts.

use super::*;
use rustybuzz::ttf_parser::{OutlineBuilder, cff::Table};

impl Reader<'_> {
    pub(super) fn cff_text(
        &mut self,
        codes: &[u8],
        text: &str,
        font: &Dictionary,
        state: &State,
    ) -> Option<(Vec<Vec<Pt>>, f32)> {
        let descriptor = dictionary(self.pdf, font.get(b"FontDescriptor").ok()?)?;
        let stream = resolve(self.pdf, descriptor.get(b"FontFile3").ok()?)?
            .as_stream()
            .ok()?;
        if stream.dict.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"Type1C") {
            return None;
        }
        let key = stream.content.as_ptr() as usize;
        let bytes = if let Some(bytes) = self.cff_cache.get(&key) {
            bytes.clone()
        } else {
            let bytes = std::sync::Arc::new(decoded(stream, 32 * 1024 * 1024).ok()?);
            self.cff_cache.insert(key, bytes.clone());
            bytes
        };
        let table = Table::parse(&bytes)?;
        let matrix = table.matrix();
        let font_matrix = Matrix([
            matrix.sx, matrix.ky, matrix.kx, matrix.sy, matrix.tx, matrix.ty,
        ]);
        let mut differences = HashMap::new();
        if let Some(array) = font
            .get(b"Encoding")
            .ok()
            .and_then(|object| dictionary(self.pdf, object))
            .and_then(|dict| dict.get(b"Differences").ok())
            .and_then(|object| object.as_array().ok())
        {
            let mut code = 0u8;
            for item in array {
                match item {
                    Object::Integer(value) => code = *value as u8,
                    Object::Name(name) => {
                        if let Ok(name) = std::str::from_utf8(name)
                            && let Some(glyph) = table.glyph_index_by_name(name)
                        {
                            differences.insert(code, glyph);
                        }
                        code = code.wrapping_add(1);
                    }
                    _ => {}
                }
            }
        }
        // Type 1C fonts have one-byte character codes. A multi-code-point mapping
        // requires a more general text layout path and must not be guessed here.
        if text.chars().count() != codes.len() {
            return None;
        }
        let first = font
            .get(b"FirstChar")
            .ok()
            .and_then(number_value)
            .unwrap_or(0.) as usize;
        let widths = font
            .get(b"Widths")
            .ok()
            .and_then(|object| resolve(self.pdf, object))
            .and_then(|object| object.as_array().ok());
        let mut pen = 0.;
        let mut contours = vec![];
        for (code, ch) in codes.iter().copied().zip(text.chars()) {
            let id = differences
                .get(&code)
                .copied()
                .or_else(|| special_glyph_name(ch).and_then(|name| table.glyph_index_by_name(name)))
                .or_else(|| table.glyph_index(code))?;
            let transform = state
                .matrix
                .concat(state.text)
                .translate(pen * state.h_scale, state.rise)
                .concat(Matrix([
                    state.font_size * state.h_scale,
                    0.,
                    0.,
                    state.font_size,
                    0.,
                    0.,
                ]))
                .concat(font_matrix);
            let mut outline = Outline {
                contours: vec![],
                current: vec![],
                transform,
            };
            // Empty space glyphs have no bounding box. A failed outline is only
            // harmless for whitespace; otherwise fall back with a diagnostic.
            if table.outline(id, &mut outline).is_err() && !ch.is_whitespace() {
                return None;
            }
            outline.finish();
            contours.extend(outline.contours);
            let width = widths
                .and_then(|widths| {
                    (code as usize)
                        .checked_sub(first)
                        .and_then(|index| widths.get(index))
                })
                .and_then(number_value)
                .map(|width| width / 1000.)
                .unwrap_or_else(|| table.glyph_width(id).unwrap_or(500) as f32 * matrix.sx);
            pen += width * state.font_size
                + state.char_space
                + if code == b' ' { state.word_space } else { 0. };
        }
        Some((contours, pen))
    }
}

fn special_glyph_name(ch: char) -> Option<&'static str> {
    Some(match ch {
        '€' => "Euro",
        '£' => "sterling",
        '¥' => "yen",
        '¢' => "cent",
        '×' => "multiply",
        '÷' => "divide",
        '°' => "degree",
        '±' => "plusminus",
        'µ' => "mu",
        '©' => "copyright",
        '®' => "registered",
        '™' => "trademark",
        '‘' => "quoteleft",
        '’' => "quoteright",
        '“' => "quotedblleft",
        '”' => "quotedblright",
        '‚' => "quotesinglbase",
        '„' => "quotedblbase",
        '–' => "endash",
        '—' => "emdash",
        '…' => "ellipsis",
        '•' => "bullet",
        '†' => "dagger",
        '‡' => "daggerdbl",
        'á' => "aacute",
        'à' => "agrave",
        'â' => "acircumflex",
        'ä' => "adieresis",
        'ã' => "atilde",
        'å' => "aring",
        'é' => "eacute",
        'è' => "egrave",
        'ê' => "ecircumflex",
        'ë' => "edieresis",
        'í' => "iacute",
        'ì' => "igrave",
        'î' => "icircumflex",
        'ï' => "idieresis",
        'ó' => "oacute",
        'ò' => "ograve",
        'ô' => "ocircumflex",
        'ö' => "odieresis",
        'õ' => "otilde",
        'ø' => "oslash",
        'ú' => "uacute",
        'ù' => "ugrave",
        'û' => "ucircumflex",
        'ü' => "udieresis",
        'ñ' => "ntilde",
        'ç' => "ccedilla",
        'ß' => "germandbls",
        _ => return None,
    })
}

struct Outline {
    contours: Vec<Vec<Pt>>,
    current: Vec<Pt>,
    transform: Matrix,
}

impl Outline {
    fn finish(&mut self) {
        if self.current.len() > 1 {
            self.contours.push(std::mem::take(&mut self.current));
        } else {
            self.current.clear();
        }
    }
}

impl OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.finish();
        self.current.push(self.transform.map(Pt::new(x, y)));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.current.push(self.transform.map(Pt::new(x, y)));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        if let Some(&start) = self.current.last() {
            let control = self.transform.map(Pt::new(x1, y1));
            let end = self.transform.map(Pt::new(x, y));
            crate::geom::flatten_cubic(
                start,
                start + (control - start) * (2. / 3.),
                end + (control - end) * (2. / 3.),
                end,
                &mut self.current,
            );
        }
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        if let Some(&start) = self.current.last() {
            crate::geom::flatten_cubic(
                start,
                self.transform.map(Pt::new(x1, y1)),
                self.transform.map(Pt::new(x2, y2)),
                self.transform.map(Pt::new(x, y)),
                &mut self.current,
            );
        }
    }
    fn close(&mut self) {
        self.finish();
    }
}
