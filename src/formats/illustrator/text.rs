//! Illustrator ATE dictionaries. Numeric keys follow the pinned MIT VectorCraft reader.
use crate::color::Rgba;
use crate::document::{Fill, Shape, Stroke, Style};
use crate::geom::{CharSpan, Geom, Pt, TextAlign, TypeRun};
use std::collections::BTreeMap;
/// Longest story read (bytes).
const MAX_STORY: usize = 1 << 20;
/// Biggest text document read (bytes, decoded).
const MAX_BYTES: usize = 64 << 20;
/// Deepest nesting read.
const MAX_DEPTH: usize = 64;
/// Most values read.
const MAX_VALUES: usize = 4_000_000;
/// The centre of the app's canvas, in the units of its text document.
const CANVAS_CENTRE: f64 = 8191.5;

/// A value of the text document.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Val {
    Num(f64),
    Str(String),
    Name(String),
    Bool(bool),
    List(Vec<Val>),
    Dict(BTreeMap<String, Val>),
}

impl Val {
    fn get(&self, key: &str) -> Option<&Val> {
        match self {
            Val::Dict(d) => d.get(key),
            _ => None,
        }
    }

    fn num(&self) -> Option<f64> {
        match self {
            Val::Num(n) if n.is_finite() => Some(*n),
            _ => None,
        }
    }

    fn str(&self) -> Option<&str> {
        match self {
            Val::Str(s) => Some(s),
            _ => None,
        }
    }

    fn list(&self) -> &[Val] {
        match self {
            Val::List(l) => l,
            _ => &[],
        }
    }

    fn nums(&self) -> Option<Vec<f64>> {
        self.list().iter().map(Val::num).collect()
    }

    /// The `name` entry of a dictionary that is a typed node (`/99 /name`).
    fn is_node(&self, name: &str) -> bool {
        matches!(self.get("99"), Some(Val::Name(n)) if n == name)
    }
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
    budget: usize,
}

impl Reader<'_> {
    fn skip_space(&mut self) {
        while self.b.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
    }

    fn value(&mut self, depth: usize) -> Option<Val> {
        self.skip_space();
        self.budget = self.budget.checked_sub(1)?;
        let c = *self.b.get(self.at)?;
        match c {
            b'<' if self.b.get(self.at + 1) == Some(&b'<') => {
                if depth > MAX_DEPTH {
                    return None;
                }
                self.at += 2;
                let mut d = BTreeMap::new();
                loop {
                    self.skip_space();
                    if self.b.get(self.at..self.at + 2) == Some(b">>") {
                        self.at += 2;
                        return Some(Val::Dict(d));
                    }
                    let key = self.key()?;
                    let v = self.value(depth + 1)?;
                    d.insert(key, v);
                }
            }
            b'[' => {
                if depth > MAX_DEPTH {
                    return None;
                }
                self.at += 1;
                let mut l = vec![];
                loop {
                    self.skip_space();
                    if self.b.get(self.at) == Some(&b']') {
                        self.at += 1;
                        return Some(Val::List(l));
                    }
                    l.push(self.value(depth + 1)?);
                }
            }
            b'(' => self.string(),
            b'/' => Some(Val::Name(self.key()?)),
            _ => {
                let start = self.at;
                while self.b.get(self.at).is_some_and(|c| {
                    !c.is_ascii_whitespace()
                        && !matches!(c, b'<' | b'>' | b'[' | b']' | b'(' | b')' | b'/')
                }) {
                    self.at += 1;
                }
                let word = std::str::from_utf8(self.b.get(start..self.at)?).ok()?;
                match word {
                    "true" => Some(Val::Bool(true)),
                    "false" => Some(Val::Bool(false)),
                    "" => None,
                    w => Some(Val::Num(w.parse().ok()?)),
                }
            }
        }
    }

    /// A `/name`, without its slash.
    fn key(&mut self) -> Option<String> {
        self.skip_space();
        if self.b.get(self.at) != Some(&b'/') {
            return None;
        }
        self.at += 1;
        let start = self.at;
        while self.b.get(self.at).is_some_and(|c| {
            !c.is_ascii_whitespace() && !matches!(c, b'<' | b'>' | b'[' | b']' | b'(' | b')' | b'/')
        }) {
            self.at += 1;
        }
        Some(String::from_utf8_lossy(self.b.get(start..self.at)?).into_owned())
    }

    /// A string: up to the first `)` that isn't escaped; UTF-16 when it starts with a byte order mark.
    fn string(&mut self) -> Option<Val> {
        self.at += 1;
        let mut bytes = vec![];
        loop {
            let c = *self.b.get(self.at)?;
            self.at += 1;
            match c {
                b')' => break,
                b'\\' => {
                    let e = *self.b.get(self.at)?;
                    self.at += 1;
                    bytes.push(match e {
                        b'n' => b'\n',
                        b'r' => b'\r',
                        b't' => b'\t',
                        b'b' => 8,
                        b'f' => 12,
                        b'0'..=b'7' => {
                            let mut v = u32::from(e - b'0');
                            for _ in 0..2 {
                                match self.b.get(self.at) {
                                    Some(d @ b'0'..=b'7') => {
                                        v = v * 8 + u32::from(d - b'0');
                                        self.at += 1;
                                    }
                                    _ => break,
                                }
                            }
                            (v & 0xff) as u8
                        }
                        other => other,
                    });
                }
                c => bytes.push(c),
            }
            if bytes.len() > MAX_BYTES {
                return None;
            }
        }
        Some(Val::Str(match bytes.strip_prefix(&[0xfe, 0xff]) {
            Some(rest) => {
                let units: Vec<u16> = rest
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|p| u16::from_be_bytes(*p))
                    .collect();
                String::from_utf16_lossy(&units)
            }
            None => bytes.iter().map(|b| char::from(*b)).collect(),
        }))
    }
}

/// The values of a text document: its top-level `/key value` pairs.
fn read(bytes: &[u8]) -> Option<Val> {
    let mut r = Reader {
        b: bytes,
        at: 0,
        budget: MAX_VALUES,
    };
    let mut d = BTreeMap::new();
    loop {
        r.skip_space();
        if r.at >= bytes.len() {
            return Some(Val::Dict(d));
        }
        let key = r.key()?;
        let v = r.value(0)?;
        d.insert(key, v);
    }
}

/// Decoded ATE stories and their style/font tables.
pub(super) struct Texts {
    doc: Val,
}

impl Texts {
    /// Forward native story styles to the shared SVG type importer.
    /// Args: story/frame indices, canvas centre, artboard origin and unique element id.
    /// Returns standard SVG text with its source affine map and styled character runs.
    pub(super) fn svg_story(
        &self,
        index: usize,
        frame: usize,
        template: (f32, f32),
        first: [f32; 4],
        id: &str,
    ) -> Option<String> {
        use std::fmt::Write as _;
        let main = self.doc.get("1")?;
        let story = main.get("1")?.list().get(index)?;
        let chars = story.get("0")?;
        let content = chars
            .get("0")?
            .str()?
            .trim_end_matches('\r')
            .replace(['\r', '\x03'], "\n");
        if content.len() > MAX_STORY {
            return None;
        }
        let view = story.get("1")?;
        let frame_index = view.get("0")?.list().get(frame)?.get("0")?.num()? as usize;
        let data = self
            .doc
            .get("0")?
            .get("8")?
            .get("0")?
            .list()
            .get(frame_index)?
            .get("0")?;
        let carries = data.get("2")?;
        let kind = carries.get("0").and_then(Val::num).unwrap_or(0.) as i32;
        let defaults = main.get("2")?;
        let runs = chars
            .get("6")
            .and_then(|v| v.get("0"))
            .map(Val::list)
            .unwrap_or_default();
        let props = runs
            .first()
            .and_then(|v| v.get("0"))
            .and_then(|v| v.get("0"))
            .and_then(|v| v.get("6"));
        let pick = |key: &str| props.and_then(|v| v.get(key)).or_else(|| defaults.get(key));
        let px = pick("1")
            .and_then(Val::num)
            .filter(|n| *n > 0. && *n < 1e5)
            .unwrap_or(12.);
        let escape = |s: &str| {
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")
                .replace('\'', "&apos;")
        };
        let mut definitions = String::new();
        let mut transform = String::new();
        let mut flow = String::new();
        let to_doc = |x: f64, y: f64| {
            (
                (x - CANVAS_CENTRE) as f32 + template.0 - first[0],
                first[3] - ((CANVAS_CENTRE - y) as f32 + template.1),
            )
        };
        if kind == 0 {
            if frame > 0 || content.contains('\n') {
                return None;
            }
            let matrix = carries.get("2")?.nums()?;
            let [a, b, c, d, e, f] = matrix.as_slice() else {
                return None;
            };
            if !matrix.iter().all(|n| n.is_finite()) {
                return None;
            }
            let anchor = find_node(view.get("2"), "F")?.get("0")?.get("0")?.nums()?;
            let x = anchor.first().copied().unwrap_or(0.);
            let y = anchor.get(1).copied().unwrap_or(0.);
            let (x, y) = to_doc(a * x + c * y + e, b * x + d * y + f);
            transform = format!(" transform=\"matrix({a} {b} {c} {d} {x} {y})\"");
        } else if kind == 1 {
            let points = data.get("1")?.get("0")?.nums()?;
            if points.len() < 8 || points.len() % 8 != 0 {
                return None;
            }
            let points = points
                .chunks_exact(2)
                .map(|p| to_doc(p[0], p[1]))
                .collect::<Vec<_>>();
            let x = points.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
            let y = points.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
            let w = points.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max) - x;
            let h = points.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max) - y;
            write!(
                definitions,
                "<rect id=\"{id}-frame\" x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"/>"
            )
            .ok()?;
            let leading = if matches!(pick("4"), Some(Val::Bool(false))) {
                pick("5").and_then(Val::num).unwrap_or(px * 1.2)
            } else {
                px * 1.2
            };
            flow = format!("shape-inside:url(#{id}-frame);line-height:{leading}px;");
        } else {
            return None;
        }
        let paragraph = chars
            .get("5")
            .and_then(|v| v.get("0"))
            .map(Val::list)
            .unwrap_or_default()
            .first()
            .and_then(|v| v.get("0"))
            .and_then(|v| v.get("0"))
            .and_then(|v| v.get("5"));
        let alignment = paragraph
            .and_then(|v| v.get("0"))
            .or_else(|| main.get("3").and_then(|v| v.get("0")))
            .and_then(Val::num)
            .unwrap_or(0.) as i32;
        let anchor = match alignment {
            1 => "middle",
            2 => "end",
            _ => "start",
        };
        let mut out = format!(
            "<defs>{definitions}</defs><text id=\"{id}\" font-size=\"{px}\" text-anchor=\"{anchor}\"{transform} style=\"{flow}\" xml:space=\"preserve\">"
        );
        let fonts = self.doc.get("0")?.get("1")?.get("0")?.list();
        let characters = content.chars().collect::<Vec<_>>();
        let mut from = 0usize;
        let mut spans = runs
            .iter()
            .map(|r| {
                (
                    r.get("1").and_then(Val::num).unwrap_or(0.).max(0.) as usize,
                    r.get("0").and_then(|v| v.get("0")).and_then(|v| v.get("6")),
                )
            })
            .collect::<Vec<_>>();
        if spans.is_empty() {
            spans.push((characters.len(), None));
        }
        for (count, props) in spans {
            let end = from.saturating_add(count).min(characters.len());
            let text = characters[from..end].iter().collect::<String>();
            from = end;
            if text.is_empty() {
                continue;
            }
            let pick = |key: &str| props.and_then(|v| v.get(key)).or_else(|| defaults.get(key));
            let size = pick("1")
                .and_then(Val::num)
                .filter(|n| *n > 0. && *n < 1e5)
                .unwrap_or(px);
            let font = pick("0")
                .and_then(Val::num)
                .and_then(|n| fonts.get(n as usize))
                .and_then(|v| v.get("0"))
                .and_then(|v| v.get("0"))
                .and_then(|v| v.get("0"))
                .and_then(Val::str)
                .unwrap_or("sans-serif");
            let fill = if matches!(pick("56"), Some(Val::Bool(false))) {
                "none".to_string()
            } else {
                pick("53").and_then(paint).unwrap_or(Rgba::BLACK).css()
            };
            let tracking = pick("8")
                .and_then(Val::num)
                .filter(|n| n.abs() < 1e5)
                .unwrap_or(0.)
                * size
                / 1000.;
            let shift = pick("9")
                .and_then(Val::num)
                .filter(|n| n.abs() < 1e5)
                .unwrap_or(0.);
            let stroke = if matches!(pick("57"), Some(Val::Bool(true))) {
                let color = pick("54").and_then(paint).unwrap_or(Rgba::BLACK).css();
                let width = pick("63")
                    .and_then(Val::num)
                    .filter(|n| *n >= 0. && *n < 1e4)
                    .unwrap_or(1.);
                format!(" stroke=\"{color}\" stroke-width=\"{width}\"")
            } else {
                String::new()
            };
            write!(out,"<tspan font-family=\"{}\" font-size=\"{size}\" fill=\"{fill}\"{stroke} letter-spacing=\"{tracking}\" baseline-shift=\"{shift}\">{}</tspan>",escape(font),escape(&text)).ok()?;
        }
        if from < characters.len() {
            out.push_str(&escape(&characters[from..].iter().collect::<String>()));
        }
        out.push_str("</text>");
        Some(out)
    }

    /// Decode the Illustrator text document.
    /// `data` contains AI editing operators. Returns bounded ATE dictionaries,
    /// or `None` when no text engine data can be read.
    pub(super) fn read(data: &[u8]) -> Option<Self> {
        let start = super::find(data, b"%AI11_BeginTextDocument")?;
        let tail = &data[start..];
        let at = super::find(tail, b"/ASCII85Decode")? + b"/ASCII85Decode".len();
        let tail = &tail[at..];
        let end = super::find(tail, b"~>")? + 2;
        if end > MAX_BYTES / 4 {
            return None;
        }
        let packed: Vec<u8> = tail[..end]
            .split(|b| matches!(b, b'\r' | b'\n'))
            .enumerate()
            .flat_map(|(i, line)| {
                if i > 0 {
                    line.strip_prefix(b"%").unwrap_or(line)
                } else {
                    line
                }
                .iter()
                .copied()
            })
            .collect();
        let packed = packed
            .trim_ascii_start()
            .strip_prefix(b",")
            .unwrap_or(packed.trim_ascii_start());
        let bytes = super::ascii85(packed).ok()?;
        if bytes.len() > MAX_BYTES {
            return None;
        }
        Some(Self { doc: read(&bytes)? })
    }

    /// Convert a story frame to native editable type.
    /// `template` is the AI canvas centre in art coordinates. Returns a shape
    /// in art coordinates plus any representational limits that must be saved.
    pub(super) fn shape(
        &self,
        index: usize,
        frame: usize,
        template: (f32, f32),
    ) -> Option<(Shape, Vec<String>)> {
        let main = self.doc.get("1")?;
        let story = main.get("1")?.list().get(index)?;
        let chars = story.get("0")?;
        let content = chars
            .get("0")?
            .str()?
            .trim_end_matches('\r')
            .replace(['\r', '\x03'], "\n");
        if content.len() > MAX_STORY {
            return None;
        }
        let view = story.get("1")?;
        let frames = view.get("0")?.list();
        let frame_index = frames.get(frame)?.get("0")?.num()? as usize;
        let frame_data = self
            .doc
            .get("0")?
            .get("8")?
            .get("0")?
            .list()
            .get(frame_index)?
            .get("0")?;
        let carries = frame_data.get("2")?;
        let kind = carries.get("0").and_then(Val::num).unwrap_or(0.) as i32;
        let defaults = main.get("2")?;
        let runs = chars
            .get("6")
            .and_then(|v| v.get("0"))
            .map(Val::list)
            .unwrap_or_default();
        let props = runs
            .first()
            .and_then(|v| v.get("0"))
            .and_then(|v| v.get("0"))
            .and_then(|v| v.get("6"));
        let pick = |key: &str| props.and_then(|v| v.get(key)).or_else(|| defaults.get(key));
        let num = |key: &str| pick(key).and_then(Val::num);
        let font_index = num("0").unwrap_or(0.) as usize;
        let font = self
            .doc
            .get("0")?
            .get("1")?
            .get("0")?
            .list()
            .get(font_index)
            .and_then(|v| v.get("0"))
            .and_then(|v| v.get("0"))
            .and_then(|v| v.get("0"))
            .and_then(Val::str)
            .unwrap_or("");
        let font_key = |s: &str| {
            s.chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect::<String>()
        };
        let key = font_key(font);
        let resolved = crate::text::all_fonts_cached()
            .iter()
            .find(|f| font_key(&f.name) == key)
            .map(|f| f.path.to_string_lossy().into_owned());
        let mut notes = Vec::new();
        if resolved.is_none() && !font.is_empty() {
            notes.push(format!(
                "Illustrator font {font} is unavailable; editable text uses a substitute."
            ));
        }
        let mut run = TypeRun {
            content,
            px: num("1").filter(|n| *n > 0. && *n < 1e5).unwrap_or(12.) as f32,
            font: resolved.unwrap_or_else(|| font.to_string()),
            leading: if matches!(pick("4"), Some(Val::Bool(false))) {
                num("5").unwrap_or(0.) as f32
            } else {
                0.
            },
            ..TypeRun::default()
        };
        run.tracking = num("8").unwrap_or(0.) as f32 * run.px / 1000.;
        let para = chars
            .get("5")
            .and_then(|v| v.get("0"))
            .map(Val::list)
            .unwrap_or_default()
            .first()
            .and_then(|v| v.get("0"))
            .and_then(|v| v.get("0"))
            .and_then(|v| v.get("5"));
        let align = para
            .and_then(|v| v.get("0"))
            .or_else(|| main.get("3").and_then(|v| v.get("0")))
            .and_then(Val::num)
            .unwrap_or(0.) as i32;
        run.align = match align {
            1 => TextAlign::Center,
            2 => TextAlign::End,
            _ => TextAlign::Start,
        };
        let anchor = find_node(view.get("2"), "F")
            .and_then(|v| v.get("0"))
            .and_then(|v| v.get("0"))
            .and_then(Val::nums)
            .unwrap_or_else(|| vec![0., 0.]);
        let to_art = |x: f64, y: f64| {
            Pt::new(
                (x - CANVAS_CENTRE) as f32 + template.0,
                (CANVAS_CENTRE - y) as f32 + template.1,
            )
        };
        let mut rotation = 0.;
        let mut point_scale = 1.;
        if kind == 0 {
            if frame > 0 {
                return None;
            }
            let matrix = carries.get("2")?.nums()?;
            let [a, b, c, d, e, f] = matrix.as_slice() else {
                return None;
            };
            if !matrix.iter().all(|n| n.is_finite()) || a * d - b * c <= 1e-8 {
                return None;
            }
            run.origin = to_art(
                a * anchor.first().copied().unwrap_or(0.)
                    + c * anchor.get(1).copied().unwrap_or(0.)
                    + e,
                b * anchor.first().copied().unwrap_or(0.)
                    + d * anchor.get(1).copied().unwrap_or(0.)
                    + f,
            );
            let scale = (a * d - b * c).sqrt() as f32;
            point_scale = scale;
            run.px *= scale;
            run.leading *= scale;
            run.tracking *= scale;
            rotation = -b.atan2(*a) as f32;
            let hscale = a.hypot(*b) as f32 / scale;
            let vscale = c.hypot(*d) as f32 / scale;
            if (hscale - 1.).abs() > 1e-4 || (vscale - 1.).abs() > 1e-4 {
                run.spans.push(CharSpan {
                    start: 0,
                    end: run.content.len(),
                    hscale: Some(hscale * 100.),
                    vscale: Some(vscale * 100.),
                    ..CharSpan::default()
                });
            }
            if (a * c + b * d).abs() > 1e-5 {
                notes.push("Sheared Illustrator type retains editable characters and rotation; compare its shape to the preview.".into());
            }
        } else if kind == 1 {
            let segments = frame_data.get("1")?.get("0")?.nums()?;
            if segments.len() < 8 || segments.len() > 8 * 65536 || segments.len() % 8 != 0 {
                return None;
            }
            let points: Vec<Pt> = segments
                .chunks_exact(2)
                .map(|p| to_art(p[0], p[1]))
                .collect();
            let min = points.iter().copied().reduce(Pt::min)?;
            let max = points.iter().copied().reduce(Pt::max)?;
            let size = max - min;
            run.origin = Pt::new(min.x, max.y - run.px * 0.85);
            run.wrap_width = Some(size.x);
            run.frame = Some(crate::text_geometry::TextFrame {
                size,
                ..Default::default()
            });
            notes.push("Illustrator area type keeps editable text and frame bounds; irregular frame contours and threading require review.".into());
        } else {
            return None;
        }
        if runs.len() > 1 {
            let first = props;
            if runs
                .iter()
                .skip(1)
                .any(|v| v.get("0").and_then(|v| v.get("0")).and_then(|v| v.get("6")) != first)
            {
                notes.push("Illustrator mixed font/paint runs require the shared rich type importer; compare the result with the retained original.".into());
            }
        }
        let fill = if matches!(pick("56"), Some(Val::Bool(false))) {
            Fill::None
        } else {
            Fill::Solid(pick("53").and_then(paint).unwrap_or(Rgba::BLACK))
        };
        let stroke = if matches!(pick("57"), Some(Val::Bool(true))) {
            Some(Stroke {
                color: pick("54").and_then(paint).unwrap_or(Rgba::BLACK),
                width: num("63").unwrap_or(1.) as f32 * point_scale,
                ..Stroke::default()
            })
        } else {
            None
        };
        let mut shape = Shape::new(Geom::Text(run), Style { fill, stroke });
        shape.rotation = rotation;
        Some((shape, notes))
    }
}

fn find_node<'a>(value: Option<&'a Val>, name: &str) -> Option<&'a Val> {
    let value = value?;
    if value.is_node(name) {
        return Some(value);
    }
    match value {
        Val::Dict(values) => values.values().find_map(|v| find_node(Some(v), name)),
        Val::List(values) => values.iter().find_map(|v| find_node(Some(v), name)),
        _ => None,
    }
}

fn paint(value: &Val) -> Option<Rgba> {
    let value = value.get("0")?;
    let n = value.get("1")?.nums()?;
    let color = |r: f64, g: f64, b: f64| {
        Rgba::rgb(
            (r.clamp(0., 1.) * 255.).round() as u8,
            (g.clamp(0., 1.) * 255.).round() as u8,
            (b.clamp(0., 1.) * 255.).round() as u8,
        )
    };
    let mut result = match value.get("0")?.num()? as i32 {
        0 if n.len() >= 2 => color(n[1], n[1], n[1]),
        1 if n.len() >= 4 => color(n[1], n[2], n[3]),
        2 if n.len() >= 5 => color(
            (1. - n[1]) * (1. - n[4]),
            (1. - n[2]) * (1. - n[4]),
            (1. - n[3]) * (1. - n[4]),
        ),
        _ => return None,
    };
    result.a = (n[0].clamp(0., 1.) * 255.).round() as u8;
    Some(result)
}
