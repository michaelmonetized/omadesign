//! Bounded Illustrator operators mapped to native document objects.
use super::lex::{Lexer, Tok};
use super::obj::{self, Obj, V};
use crate::artwork_mask::{ArtworkMask, MaskSource};
use crate::color::{Blend, Rgba};
use crate::document::{Artboard, Cap, Document, Fill, Join, Layer, Shape, Stroke, Style};
use crate::geom::{Anchor, Geom, PathContour, Pt, TypeRun};
use crate::gradient::{Gradient, GradientKind, GradientStop};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::rc::Rc;

const BLENDS: [Blend; 16] = [
    Blend::Normal,
    Blend::Multiply,
    Blend::Screen,
    Blend::Overlay,
    Blend::SoftLight,
    Blend::HardLight,
    Blend::ColorDodge,
    Blend::ColorBurn,
    Blend::Darken,
    Blend::Lighten,
    Blend::Difference,
    Blend::Exclusion,
    Blend::Hue,
    Blend::Saturation,
    Blend::Color,
    Blend::Luminosity,
];

#[derive(Clone)]
struct State {
    fill: Fill,
    stroke: Stroke,
    opacity: f32,
    blend: Blend,
    visible: bool,
    locked: bool,
    winding: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            fill: Fill::Solid(Rgba::BLACK),
            stroke: Stroke {
                color: Rgba::BLACK,
                width: 1.,
                cap: Cap::Butt,
                join: Join::Miter,
                ..Stroke::default()
            },
            opacity: 1.,
            blend: Blend::Normal,
            visible: true,
            locked: false,
            winding: true,
        }
    }
}

struct Frame {
    layer: Layer,
    kind: String,
    state: State,
    contours: Vec<PathContour>,
    clip: Option<MaskSource>,
    paint: (bool, bool),
}

struct DictFrame {
    obj: Obj,
    base: usize,
    state: State,
}
struct GradientDef {
    name: String,
    kind: GradientKind,
    stops: Vec<GradientStop>,
}
struct GradientPlace {
    name: String,
    from: Pt,
    to: Pt,
    stroke: bool,
}
struct Place {
    matrix: [f32; 6],
    bounds: [f32; 4],
    path: String,
    preview: Option<crate::document::Pixels>,
}

struct Reader<'a> {
    lex: Lexer<'a>,
    values: Vec<V>,
    dictionaries: Vec<DictFrame>,
    frames: Vec<Frame>,
    state: State,
    layers: Vec<Layer>,
    paths: Vec<PathContour>,
    clip_next: bool,
    notes: BTreeSet<String>,
    boards: Vec<(String, [f32; 4])>,
    crop: Option<[f32; 4]>,
    bbox: Option<[f32; 4]>,
    template: Option<[f32; 4]>,
    art_size: Option<[f32; 2]>,
    gradients: HashMap<String, (GradientKind, Vec<GradientStop>)>,
    gradient_def: Option<GradientDef>,
    gradient_place: Option<GradientPlace>,
    gradient_stroke: bool,
    text: Option<TypeRun>,
    text_style: Option<Style>,
    place: Option<Place>,
    raster: Option<(String, Vec<V>)>,
    source: Option<&'a Path>,
    texts: Option<super::text::Texts>,
    text_slots: Vec<(u64, usize, usize, bool)>,
    last: Option<u64>,
    objects: usize,
    sources: HashMap<String, crate::source_attachment::SourceAttachment>,
    source_bytes: usize,
    raster_placements: HashMap<u64, [f32; 6]>,
}

/// Reconstruct native artwork.
/// `data` holds decoded AI operators; `source` locates optional local placements.
/// Returns the document and exact fidelity notes. It never executes PostScript.
pub(super) fn read(
    data: &[u8],
    name: &str,
    source: Option<&Path>,
) -> Result<(Document, Vec<String>), String> {
    let mut reader = Reader {
        lex: Lexer::new(data),
        values: vec![],
        dictionaries: vec![],
        frames: vec![],
        state: State::default(),
        layers: vec![],
        paths: vec![],
        clip_next: false,
        notes: BTreeSet::new(),
        boards: vec![],
        crop: None,
        bbox: None,
        template: None,
        art_size: None,
        gradients: HashMap::new(),
        gradient_def: None,
        gradient_place: None,
        gradient_stroke: false,
        text: None,
        text_style: None,
        place: None,
        raster: None,
        source,
        texts: super::text::Texts::read(data),
        text_slots: vec![],
        last: None,
        objects: 0,
        sources: HashMap::new(),
        source_bytes: 0,
        raster_placements: HashMap::new(),
    };
    while let Some(token) = reader.lex.next_token() {
        if reader.values.len() > 1_000_000 || reader.layers.len() > 100_000 {
            return Err("Illustrator object or operand limit exceeded".into());
        }
        match token.tok {
            Tok::Num(n) => reader.values.push(V::Num(n)),
            Tok::Str(s) | Tok::Hex(s) => reader.values.push(V::Str(s)),
            Tok::Name(n) => reader.values.push(V::Name(obj::text(n))),
            Tok::Comment(c) => {
                if reader.comment(&String::from_utf8_lossy(c))? {
                    break;
                }
            }
            Tok::Word(w) => reader.op(std::str::from_utf8(w).unwrap_or("?"), token.hidden)?,
            Tok::Preview(p) => {
                if let Some(place) = reader.place.as_mut() {
                    place.preview = preview(p).map_err(super::picture_error)?;
                }
            }
            Tok::Data(data) => {
                if let Some((_, operands)) = reader.raster.as_mut() {
                    if operands.is_empty() {
                        *operands = std::mem::take(&mut reader.values);
                    }
                }
                reader.image(data).map_err(super::picture_error)?;
            }
        }
    }
    if !reader.frames.is_empty() || !reader.dictionaries.is_empty() {
        return Err("Illustrator data ends inside a layer, group or dictionary".into());
    }
    reader.finish(name)
}

fn numbers(values: &[V]) -> Vec<f32> {
    values.iter().filter_map(V::num).map(|n| n as f32).collect()
}
fn first_text(values: &[V]) -> Option<String> {
    values.iter().find_map(V::text)
}
fn four(text: &str) -> Option<[f32; 4]> {
    let numbers: Vec<f32> = text
        .split_whitespace()
        .filter_map(|n| n.parse().ok())
        .collect();
    numbers
        .get(..4)?
        .try_into()
        .ok()
        .filter(|n: &[f32; 4]| n.iter().all(|n| n.is_finite()))
}
fn point(x: f32, y: f32) -> Anchor {
    Anchor {
        pt: Pt::new(x, y),
        h_in: Pt::ZERO,
        h_out: Pt::ZERO,
        radius: 0.,
    }
}

/// Decode Illustrator object names.
/// `id` is an AI XML identifier. Returns human text with escaped characters and
/// spaces restored, without the long numeric suffix Illustrator adds for uniqueness.
fn object_name(id: &str) -> String {
    let id = id
        .strip_suffix('_')
        .and_then(|body| {
            let at = body.rfind('_')?;
            let digits = &body[at + 1..];
            (digits.len() >= 20 && digits.bytes().all(|b| b.is_ascii_digit()))
                .then_some(&body[..at])
        })
        .unwrap_or(id);
    let mut rest = id;
    let mut result = String::with_capacity(id.len());
    while let Some(ch) = rest.chars().next() {
        if let Some(body) = rest.strip_prefix("_x") {
            if let Some(end) = body.find('_').filter(|n| *n > 0 && *n <= 6) {
                if let Some(ch) = u32::from_str_radix(&body[..end], 16)
                    .ok()
                    .and_then(char::from_u32)
                {
                    result.push(ch);
                    rest = &body[end + 1..];
                    continue;
                }
            }
        }
        result.push(if ch == '_' { ' ' } else { ch });
        rest = &rest[ch.len_utf8()..];
    }
    result
}
fn cmyk(c: f32, m: f32, y: f32, k: f32) -> Rgba {
    rgb(
        (1. - c) * (1. - k),
        (1. - m) * (1. - k),
        (1. - y) * (1. - k),
    )
}
fn rgb(r: f32, g: f32, b: f32) -> Rgba {
    Rgba::rgb(
        (r.clamp(0., 1.) * 255.).round() as u8,
        (g.clamp(0., 1.) * 255.).round() as u8,
        (b.clamp(0., 1.) * 255.).round() as u8,
    )
}

impl Reader<'_> {
    fn note(&mut self, note: impl Into<String>) {
        if self.notes.len() < 64 {
            self.notes.insert(note.into());
        }
    }

    fn comment(&mut self, line: &str) -> Result<bool, String> {
        let marker = line.split([':', ' ', '\t']).next().unwrap_or("");
        let marker = marker
            .strip_prefix("AI")
            .map(|m| {
                m.trim_start_matches(|c: char| c.is_ascii_digit())
                    .trim_start_matches('_')
            })
            .unwrap_or(marker);
        if matches!(marker, "%BeginProlog" | "%BeginResource") {
            self.skip(if marker == "%BeginProlog" {
                "%EndProlog"
            } else {
                "%EndResource"
            })?;
        } else if marker == "BeginLayer" {
            self.open("Layer")?;
        } else if matches!(marker, "%Trailer" | "%PageTrailer") {
            return Ok(true);
        } else if let Some(value) = line
            .strip_prefix("%HiResBoundingBox:")
            .or_else(|| line.strip_prefix("%BoundingBox:"))
        {
            self.bbox = four(value).or(self.bbox);
        } else if let Some(value) = line.strip_prefix("AI3_Cropmarks:") {
            self.crop = four(value);
        } else if let Some(value) = line.strip_prefix("AI3_TemplateBox:") {
            self.template = four(value);
        } else if let Some(value) = line.strip_prefix("AI5_ArtSize:") {
            let n: Vec<f32> = value
                .split_whitespace()
                .filter_map(|v| v.parse().ok())
                .collect();
            self.art_size = n.get(..2).and_then(|v| v.try_into().ok());
        } else if marker == "BeginRaster" {
            self.raster = Some(("DeviceRGB".into(), vec![]));
        } else if marker == "EndRaster" {
            self.raster = None;
        } else if marker == "BeginPlace" {
            self.place = Some(Place {
                matrix: [1., 0., 0., 1., 0., 0.],
                bounds: [0.; 4],
                path: String::new(),
                preview: None,
            });
        } else if marker == "EndPlace" {
            if self.place.take().is_some() {
                self.note("A placed file could not be reconstructed.");
            }
        } else if marker.starts_with("Begin")
            && matches!(
                marker,
                "BeginTextDocument"
                    | "BeginEncoding"
                    | "BeginPattern"
                    | "BeginBrushPattern"
                    | "BeginSVGFilter"
                    | "BeginSymbol"
                    | "BeginPluginObject"
                    | "BeginArtStyles"
                    | "BeginArtStyleList"
                    | "BeginPalette"
                    | "BeginSymbolList"
            )
        {
            if matches!(
                marker,
                "BeginPattern"
                    | "BeginBrushPattern"
                    | "BeginSVGFilter"
                    | "BeginSymbol"
                    | "BeginPluginObject"
                    | "BeginArtStyles"
            ) {
                self.note(format!("Illustrator {} definitions are retained only in the original source; their live behavior is unsupported.", marker.trim_start_matches("Begin")));
            }
            self.skip(&marker.replacen("Begin", "End", 1))?;
        }
        Ok(false)
    }

    fn skip(&mut self, end: &str) -> Result<(), String> {
        while let Some(token) = self.lex.next_token() {
            if let Tok::Comment(c) = token.tok {
                let line = String::from_utf8_lossy(c);
                if line
                    .split([':', ' ', '\t'])
                    .next()
                    .unwrap_or("")
                    .ends_with(end)
                {
                    return Ok(());
                }
            }
        }
        Err(format!("Illustrator section {end} is not closed"))
    }

    fn open(&mut self, kind: &str) -> Result<(), String> {
        if self.frames.len() + self.dictionaries.len() >= 256 {
            return Err("Illustrator nesting exceeds 256 levels".into());
        }
        let mut layer = Layer::group(if kind == "Layer" { "Layer" } else { "Group" });
        layer.parent = self.frames.last().map(|f| f.layer.id);
        layer.visible = self.state.visible;
        layer.locked = self.state.locked;
        self.last = Some(layer.id);
        self.frames.push(Frame {
            layer,
            kind: kind.into(),
            state: self.state.clone(),
            contours: vec![],
            clip: None,
            paint: (true, false),
        });
        self.paths.clear();
        Ok(())
    }

    fn close(&mut self, kind: &str) -> Result<(), String> {
        let frame = self
            .frames
            .pop()
            .ok_or("Unbalanced Illustrator container")?;
        if frame.kind != kind {
            return Err(format!("Unbalanced Illustrator {kind}"));
        }
        let mut layer = frame.layer;
        if kind == "Compound" && !frame.contours.is_empty() {
            let geom = Geom::Paths {
                paths: frame.contours,
                winding: self.state.winding,
            };
            let shape = self.shape(geom, frame.paint.0, frame.paint.1);
            let mut compound = Layer::vector("Compound path");
            compound.kind.shapes_mut().unwrap().push(shape);
            if self
                .layers
                .iter()
                .any(|child| child.parent == Some(layer.id))
            {
                compound.parent = Some(layer.id);
                self.layers.push(compound);
            } else {
                compound.parent = layer.parent;
                layer = compound;
            }
        }
        if let Some(clip) = frame.clip {
            layer.artwork_masks.push(ArtworkMask::new(clip));
        }
        self.last = Some(layer.id);
        self.layers.push(layer);
        self.state = frame.state;
        Ok(())
    }

    fn shape(&self, geom: Geom, fill: bool, stroke: bool) -> Shape {
        let mut shape = Shape::new(
            geom,
            Style {
                fill: if fill {
                    self.state.fill.clone()
                } else {
                    Fill::None
                },
                stroke: stroke.then(|| self.state.stroke.clone()),
            },
        );
        shape.visible = self.state.visible;
        shape.locked = self.state.locked;
        shape.opacity = self.state.opacity;
        shape.blend = self.state.blend;
        shape
    }

    fn add(&mut self, mut layer: Layer) -> Result<(), String> {
        self.objects += 1;
        if self.objects > 100_000 {
            return Err("Illustrator exceeds 100,000 objects".into());
        }
        layer.parent = self.frames.last().map(|f| f.layer.id);
        self.last = Some(layer.id);
        self.layers.push(layer);
        Ok(())
    }

    fn paint(&mut self, op: &str, guide: bool) -> Result<(), String> {
        let mut paths = std::mem::take(&mut self.paths);
        if paths.is_empty() {
            return Ok(());
        }
        if matches!(op, "n" | "f" | "s" | "b") {
            paths.last_mut().unwrap().closed = true;
        }
        if self.frames.last().is_some_and(|f| f.kind == "Compound") {
            let frame = self.frames.last_mut().unwrap();
            frame.contours.extend(paths);
            frame.paint = (
                matches!(op, "F" | "f" | "B" | "b"),
                matches!(op, "S" | "s" | "B" | "b"),
            );
            return Ok(());
        }
        let geom = Geom::Paths {
            paths,
            winding: self.state.winding,
        };
        let mut shape = self.shape(
            geom,
            matches!(op, "F" | "f" | "B" | "b"),
            matches!(op, "S" | "s" | "B" | "b"),
        );
        shape.guide = guide || matches!(op, "N" | "n") && !self.clip_next;
        if let Some(place) = &self.gradient_place {
            if let Some((kind, stops)) = self.gradients.get(&place.name) {
                let bounds = shape.geom.bbox();
                let size = bounds.size();
                let norm = |pt: Pt| {
                    [
                        (pt.x - bounds.min.x) / size.x.max(0.0001),
                        (pt.y - bounds.min.y) / size.y.max(0.0001),
                    ]
                };
                let mut gradient = Gradient::new(*kind, Rgba::BLACK, Rgba::WHITE);
                gradient.from = norm(place.from);
                gradient.to = norm(place.to);
                gradient.stops = stops.clone();
                if place.stroke {
                    if let Some(stroke) = shape.style.stroke.as_mut() {
                        stroke.gradient = Some(gradient);
                    }
                } else {
                    shape.style.fill = Fill::Gradient(gradient);
                }
            } else {
                self.note(format!(
                    "Gradient {} could not be reconstructed.",
                    place.name
                ));
            }
        }
        if self.clip_next {
            shape.style.fill = Fill::Solid(Rgba::WHITE);
            shape.style.stroke = None;
        }
        let mut layer = Layer::vector(if self.clip_next {
            "Clipping path"
        } else {
            "Path"
        });
        let source = MaskSource {
            layer: layer.id,
            shape: Some(shape.id),
        };
        layer.kind.shapes_mut().unwrap().push(shape);
        if self.clip_next {
            layer.visible = false;
            if let Some(frame) = self.frames.last_mut() {
                frame.clip = Some(source);
            }
        }
        self.clip_next = false;
        self.add(layer)
    }

    fn op(&mut self, op: &str, hidden: bool) -> Result<(), String> {
        match op {
            "[" => {
                self.values.push(V::Mark);
                return Ok(());
            }
            "]" => {
                let at = self
                    .values
                    .iter()
                    .rposition(|v| matches!(v, V::Mark))
                    .ok_or("Unbalanced Illustrator array")?;
                let arr = self.values.split_off(at + 1);
                self.values.pop();
                self.values.push(V::Arr(arr));
                return Ok(());
            }
            ":" => {
                if self.dictionaries.len() >= 256 {
                    return Err("Illustrator dictionary nesting exceeds 256".into());
                }
                let ty = self.values.pop().and_then(|v| v.text()).unwrap_or_default();
                self.dictionaries.push(DictFrame {
                    obj: Obj {
                        ty,
                        ..Obj::default()
                    },
                    base: self.values.len(),
                    state: self.state.clone(),
                });
                return Ok(());
            }
            "," => {
                if let Some(dict) = self.dictionaries.last_mut() {
                    obj::add_entry(&mut dict.obj, &mut self.values, dict.base);
                    if dict.obj.ty == "Binary"
                        && dict
                            .obj
                            .entries
                            .last()
                            .is_some_and(|(k, _)| k.as_deref() == Some("ASCII85Decode"))
                    {
                        self.lex.skip_past(b"~>");
                    }
                }
                return Ok(());
            }
            ";" => {
                if let Some(mut dict) = self.dictionaries.pop() {
                    obj::close(&mut dict.obj, &mut self.values, dict.base);
                    self.state = dict.state;
                    if self.dictionaries.is_empty() {
                        self.dictionary(&dict.obj, hidden)?;
                    } else {
                        self.values.push(V::Obj(Rc::new(dict.obj)));
                    }
                }
                return Ok(());
            }
            _ => {}
        }
        let base = self
            .dictionaries
            .last()
            .map_or(0, |d| d.base)
            .min(self.values.len());
        let values = self.values.split_off(base);
        let n = numbers(&values);
        if !self.dictionaries.is_empty() {
            return Ok(());
        }
        if let Some(def) = self.gradient_def.as_mut() {
            if op == "Bs" && n.len() >= 4 && def.stops.len() < 256 {
                let len = n.len();
                let modern = len >= 6 && n[len - 3] == 6.;
                let style_at = if modern { len - 5 } else { len - 3 };
                let style = n[style_at] as i32;
                let components = &n[..style_at];
                let color = match style {
                    0 => components.last().map(|v| rgb(*v, *v, *v)),
                    1 if components.len() >= 4 => Some(cmyk(
                        components[0],
                        components[1],
                        components[2],
                        components[3],
                    )),
                    2 if components.len() >= 3 => Some(rgb(
                        components[components.len() - 3],
                        components[components.len() - 2],
                        components[components.len() - 1],
                    )),
                    _ => None,
                };
                if let Some(mut color) = color {
                    if modern {
                        color.a = (n[len - 4].clamp(0., 1.) * 255.).round() as u8;
                    }
                    def.stops.push(GradientStop {
                        offset: n[len - 1] / 100.,
                        color,
                    });
                }
            } else if op == "BD" {
                let mut def = self.gradient_def.take().unwrap();
                def.stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
                self.gradients.insert(def.name, (def.kind, def.stops));
            }
            return Ok(());
        }
        match op {
            "u" => self.open("Group")?,
            "U" => self.close("Group")?,
            "*u" => self.open("Compound")?,
            "*U" => self.close("Compound")?,
            "q" => self.open("Clip")?,
            "Q" => self.close("Clip")?,
            "LB" => self.close("Layer")?,
            "Lb" => {
                if let Some(frame) = self.frames.last_mut() {
                    frame.layer.visible = n.first() != Some(&0.);
                    frame.layer.locked = n.get(2) == Some(&0.);
                }
            }
            "Ln" => {
                if let Some(frame) = self.frames.last_mut() {
                    frame.layer.name = first_text(&values).unwrap_or_default();
                }
            }
            "m" | "l" | "L" | "c" | "C" | "v" | "V" | "y" | "Y" => self.path(op, &n)?,
            "h" => {
                if let Some(path) = self.paths.last_mut() {
                    path.closed = true;
                }
            }
            "W" => self.clip_next = true,
            "N" | "n" | "F" | "f" | "S" | "s" | "B" | "b" => self.paint(op, false)?,
            "*" => self.paint(&first_text(&values).unwrap_or_else(|| "N".into()), true)?,
            "g" | "G" | "k" | "K" | "x" | "X" | "Xa" | "XA" | "Xx" | "XX" | "Xk" | "XK" => {
                self.color(op, &values)
            }
            "w" => {
                if let Some(width) = n.last() {
                    self.state.stroke.width = width.max(0.);
                }
            }
            "J" => {
                self.state.stroke.cap = match n.last().copied().unwrap_or(0.) as i32 {
                    1 => Cap::Round,
                    2 => Cap::Square,
                    _ => Cap::Butt,
                }
            }
            "j" => {
                self.state.stroke.join = match n.last().copied().unwrap_or(0.) as i32 {
                    1 => Join::Round,
                    2 => Join::Bevel,
                    _ => Join::Miter,
                }
            }
            "d" => {
                let pattern = values
                    .iter()
                    .find_map(|v| {
                        if let V::Arr(a) = v {
                            Some(numbers(a))
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();
                self.state.stroke.dash = match pattern.as_slice() {
                    [] => None,
                    [a] => Some((*a, *a)),
                    [a, b] => Some((*a, *b)),
                    _ => {
                        self.note(
                            "Stroke dash sequences with more than two values are approximated.",
                        );
                        Some((pattern[0], pattern[1]))
                    }
                };
                self.state.stroke.dash_offset = n.last().copied().unwrap_or(0.);
            }
            "XR" => self.state.winding = n.last() != Some(&1.),
            "Xw" => self.state.visible = n.last() != Some(&1.),
            "A" => self.state.locked = n.last() == Some(&1.),
            "Xy" => {
                if n.len() >= 2 {
                    self.state.blend = BLENDS.get(n[0] as usize).copied().unwrap_or(Blend::Normal);
                    self.state.opacity = n[1].clamp(0., 1.);
                    if let Some(last) = self
                        .last
                        .and_then(|id| self.layers.iter_mut().find(|l| l.id == id && l.is_group))
                    {
                        last.opacity = self.state.opacity;
                        last.blend = self.state.blend;
                    }
                    if n.get(3).is_some_and(|v| *v != 0.) {
                        self.note("Illustrator knockout groups are unsupported.");
                    }
                }
            }
            "Bd" => {
                self.gradient_def = Some(GradientDef {
                    name: first_text(&values).unwrap_or_default(),
                    kind: if n.first() == Some(&1.) {
                        GradientKind::Radial
                    } else {
                        GradientKind::Linear
                    },
                    stops: vec![],
                })
            }
            "Bb" => self.gradient_stroke = n.last() == Some(&1.),
            "Bg" => {
                if n.len() >= 5 {
                    let angle = n[3].to_radians();
                    let from = Pt::new(n[1], n[2]);
                    self.gradient_place = Some(GradientPlace {
                        name: first_text(&values).unwrap_or_default(),
                        from,
                        to: from + Pt::new(angle.cos() * n[4], angle.sin() * n[4]),
                        stroke: self.gradient_stroke,
                    });
                }
            }
            "Xm" => {
                if n.len() >= 6 {
                    if let Some(place) = self.gradient_place.as_mut() {
                        place.from = Pt::new(n[4], n[5]);
                        place.to = place.from + Pt::new(n[0], n[1]);
                    }
                }
            }
            "BB" => self.gradient_place = None,
            "To" => {
                self.text = Some(TypeRun::default());
                self.text_style = Some(Style {
                    fill: self.state.fill.clone(),
                    stroke: None,
                });
            }
            "Tp" => {
                if let Some(text) = self.text.as_mut() {
                    if n.len() >= 6 {
                        text.origin = Pt::new(n[n.len() - 2], n[n.len() - 1]);
                    }
                }
            }
            "Tf" => {
                if let Some(text) = self.text.as_mut() {
                    text.font = first_text(&values).unwrap_or_default();
                    text.px = n.last().copied().unwrap_or(12.).max(0.1);
                }
            }
            "Tl" => {
                if let Some(text) = self.text.as_mut() {
                    text.leading = n.first().copied().unwrap_or(0.);
                }
            }
            "Tt" => {
                if let Some(text) = self.text.as_mut() {
                    text.tracking = n.last().copied().unwrap_or(0.) * text.px / 1000.;
                }
            }
            "Tx" | "Tj" | "TX" => {
                if let Some(text) = self.text.as_mut() {
                    text.content
                        .push_str(&first_text(&values).unwrap_or_default());
                }
            }
            "TO" => self.end_text()?,
            "XN" => {
                if let Some((space, _)) = self.raster.as_mut() {
                    *space = first_text(&values).unwrap_or_else(|| "DeviceRGB".into());
                }
            }
            "XI" => {
                if let Some((_, operands)) = self.raster.as_mut() {
                    *operands = values;
                }
            }
            "`" => {
                if let Some(place) = self.place.as_mut() {
                    if let Some(matrix) = values.iter().find_map(|v| {
                        if let V::Arr(a) = v {
                            numbers(a).try_into().ok()
                        } else {
                            None
                        }
                    }) {
                        place.matrix = matrix;
                    }
                    if let Some(bounds) = n.get(..4).and_then(|n| n.try_into().ok()) {
                        place.bounds = bounds;
                    }
                    place.path = values.iter().rev().find_map(V::text).unwrap_or_default();
                }
            }
            "~" => self.end_place().map_err(super::picture_error)?,
            "p" | "P" => {
                self.note(
                    "Pattern paint is unsupported; its original definition remains in the source.",
                );
                if op == "p" {
                    self.state.fill = Fill::None;
                } else {
                    self.state.stroke.width = 0.;
                }
            }
            "XW" => {
                if n.last() == Some(&1.) || n.last() == Some(&9.) {
                    self.note("Live brushes, effects and multiple appearances retain their drawn artwork; the live appearance is unsupported.");
                }
            }
            "X!" => self
                .note("Live gradient meshes are unsupported; compare against the saved preview."),
            "O" | "R" => {
                if n.last() == Some(&1.) {
                    self.note("Overprint is not reproduced by the RGB editor.");
                }
            }
            "M" | "i" | "H" | "D" | "Xd" | "Xr" | "XG" | "Xh" | "XH" | "XF" | "X=" | "X+"
            | "Bc" | "XP" | "Np" | "TE" | "TZ" | "Xs" | "Xt" | "XD" | "Xi" | "LB2" | "Lc"
            | "Xv" | "XV" | "Xq" | "XQ" | "Xg" | "Xn" | "Bn" | "gsave" | "grestore"
            | "showpage" | "annotatepage" | "Bm" | "Bh" | "Bs" | "BD" | "Tk" | "Tz" | "TA"
            | "Tr" | "Ts" | "TW" | "TC" | "TQ" => {}
            _ if !self.frames.is_empty() => self.note(format!(
                "Unsupported Illustrator operator `{}`; its live behavior was not reconstructed.",
                op.chars().take(32).collect::<String>()
            )),
            _ => {}
        }
        Ok(())
    }

    fn path(&mut self, op: &str, n: &[f32]) -> Result<(), String> {
        if n.iter().any(|v| !v.is_finite() || v.abs() > 1e7) {
            return Err("Invalid Illustrator path coordinates".into());
        }
        if op == "m" {
            if let Some(p) = n.get(n.len().saturating_sub(2)..).filter(|n| n.len() == 2) {
                self.paths.push(PathContour {
                    anchors: vec![point(p[0], p[1])],
                    closed: false,
                });
            }
            return Ok(());
        }
        let Some(path) = self.paths.last_mut() else {
            return Ok(());
        };
        if path.anchors.len() >= 1_000_000 {
            return Err("Illustrator path exceeds one million points".into());
        }
        let Some(last) = path.anchors.last_mut() else {
            return Ok(());
        };
        let tail = |count| n.get(n.len().checked_sub(count)?..);
        match op {
            "l" | "L" => {
                if let Some(p) = tail(2) {
                    path.anchors.push(point(p[0], p[1]));
                }
            }
            "c" | "C" => {
                if let Some(p) = tail(6) {
                    last.h_out = Pt::new(p[0], p[1]) - last.pt;
                    let mut next = point(p[4], p[5]);
                    next.h_in = Pt::new(p[2], p[3]) - next.pt;
                    path.anchors.push(next);
                }
            }
            "v" | "V" => {
                if let Some(p) = tail(4) {
                    let mut next = point(p[2], p[3]);
                    next.h_in = Pt::new(p[0], p[1]) - next.pt;
                    path.anchors.push(next);
                }
            }
            "y" | "Y" => {
                if let Some(p) = tail(4) {
                    last.h_out = Pt::new(p[0], p[1]) - last.pt;
                    path.anchors.push(point(p[2], p[3]));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn color(&mut self, op: &str, values: &[V]) {
        let n = numbers(values);
        let lower = op.to_ascii_lowercase();
        let color = match lower.as_str() {
            "g" => n.last().map(|v| rgb(*v, *v, *v)),
            "k" if n.len() >= 4 => {
                self.note("CMYK paint was converted to RGB; process-ink values remain in the original source.");
                Some(cmyk(n[0], n[1], n[2], n[3]))
            }
            "xa" if n.len() >= 3 => Some(rgb(n[n.len() - 3], n[n.len() - 2], n[n.len() - 1])),
            "x" | "xx" | "xk" => {
                self.note("Spot/global paint uses its process-color equivalent; named-ink behavior is unsupported.");
                let before: Vec<f32> = numbers(
                    &values[..values
                        .iter()
                        .position(|v| matches!(v, V::Str(_)))
                        .unwrap_or(values.len())],
                );
                let after: Vec<f32> = values
                    .iter()
                    .position(|v| matches!(v, V::Str(_)))
                    .map(|at| numbers(&values[at + 1..]))
                    .unwrap_or_default();
                let base = if lower != "x" && after.get(1) == Some(&1.) && before.len() >= 3 {
                    Some(rgb(
                        before[before.len() - 3],
                        before[before.len() - 2],
                        before[before.len() - 1],
                    ))
                } else if before.len() >= 4 {
                    Some(cmyk(before[0], before[1], before[2], before[3]))
                } else {
                    None
                };
                base.map(|c| {
                    let t = after.first().copied().unwrap_or(0.).clamp(0., 1.);
                    rgb(
                        c.r as f32 / 255. * (1. - t) + t,
                        c.g as f32 / 255. * (1. - t) + t,
                        c.b as f32 / 255. * (1. - t) + t,
                    )
                })
            }
            _ => None,
        };
        if let Some(color) = color {
            if op.chars().last().is_some_and(char::is_lowercase) {
                self.state.fill = Fill::Solid(color);
            } else {
                self.state.stroke.color = color;
                self.state.stroke.gradient = None;
            }
        }
    }

    fn dictionary(&mut self, obj: &Obj, hidden: bool) -> Result<(), String> {
        if obj.ty == "Document" {
            fn find<'a>(obj: &'a Obj, key: &str, depth: usize) -> Option<&'a Obj> {
                if depth > 64 {
                    return None;
                }
                obj.obj(key).or_else(|| {
                    obj.entries
                        .iter()
                        .flat_map(|(_, v)| v.iter())
                        .filter_map(V::obj)
                        .find_map(|o| find(o, key, depth + 1))
                })
            }
            if let Some(array) = find(obj, "ArtboardArray", 0) {
                for (i, board) in array.items().take(1000).enumerate() {
                    let (a, b) = (board.nums("PositionPoint1"), board.nums("PositionPoint2"));
                    if a.len() >= 2 && b.len() >= 2 {
                        self.boards.push((
                            board
                                .text("Name")
                                .unwrap_or_else(|| format!("Artboard {}", i + 1)),
                            [
                                a[0].min(b[0]) as f32,
                                a[1].min(b[1]) as f32,
                                a[0].max(b[0]) as f32,
                                a[1].max(b[1]) as f32,
                            ],
                        ));
                    }
                }
            }
        } else if obj.ty == "ArtDictionary" {
            fn name(obj: &Obj, depth: usize) -> Option<String> {
                if depth > 64 {
                    return None;
                }
                obj.text("AI10_ArtUID")
                    .or_else(|| obj.obj("AI10_ArtUID").and_then(Obj::content_text))
                    .or_else(|| obj.text("AIArtName"))
                    .or_else(|| obj.text("Name"))
                    .or_else(|| {
                        obj.entries
                            .iter()
                            .flat_map(|(_, v)| v.iter())
                            .filter_map(V::obj)
                            .find_map(|o| name(o, depth + 1))
                    })
            }
            if let Some(name) = name(obj, 0) {
                let last = self.last;
                if let Some(layer) =
                    self.layers
                        .iter_mut()
                        .find(|l| Some(l.id) == last)
                        .or_else(|| {
                            self.frames
                                .iter_mut()
                                .find(|f| Some(f.layer.id) == last)
                                .map(|f| &mut f.layer)
                        })
                {
                    layer.name = object_name(&name);
                }
            }
        } else if obj.ty == "AI11Text" {
            let story = obj.nums("StoryIndex").first().copied().unwrap_or(0.) as usize;
            let frame = obj.nums("FrameIndex").first().copied().unwrap_or(0.) as usize;
            if obj.nums("FreeUndo").first() == Some(&1.) {
                return Ok(());
            }
            let template = self
                .template
                .map(|t| ((t[0] + t[2]) / 2., (t[1] + t[3]) / 2.))
                .unwrap_or((0., 0.));
            let shape = self
                .texts
                .as_ref()
                .and_then(|texts| texts.shape(story, frame, template));
            if let Some((mut shape, notes)) = shape {
                for note in notes {
                    self.note(note);
                }
                shape.visible = self.state.visible;
                shape.locked = self.state.locked;
                shape.opacity = self.state.opacity;
                shape.blend = self.state.blend;
                let mut layer = Layer::vector("Text");
                layer.kind.shapes_mut().unwrap().push(shape);
                self.text_slots.push((layer.id, story, frame, hidden));
                self.add(layer)?;
            } else {
                self.note(format!("Illustrator story {story}, frame {frame} could not be reconstructed as editable type."));
            }
        } else if matches!(obj.ty.as_str(), "SymbolInstance" | "ForeignObject" | "Mesh") {
            self.note(format!(
                "Illustrator {} is unsupported; inspect the PDF preview and original source.",
                obj.ty
            ));
        }
        Ok(())
    }

    fn end_text(&mut self) -> Result<(), String> {
        if let Some(mut text) = self.text.take() {
            text.content = text.content.replace('\r', "\n");
            let style = self.text_style.take().unwrap_or(Style {
                fill: self.state.fill.clone(),
                stroke: None,
            });
            let mut shape = Shape::new(Geom::Text(text), style);
            shape.visible = self.state.visible;
            shape.locked = self.state.locked;
            shape.opacity = self.state.opacity;
            shape.blend = self.state.blend;
            let mut layer = Layer::vector("Text");
            layer.kind.shapes_mut().unwrap().push(shape);
            self.add(layer)?;
        }
        Ok(())
    }

    fn end_place(&mut self) -> Result<(), String> {
        let Some(mut place) = self.place.take() else {
            return Ok(());
        };
        let mut preview_only = true;
        let mut source_id = None;
        if let Some(base) = self
            .source
            .and_then(Path::parent)
            .and_then(|p| p.canonicalize().ok())
        {
            let candidate = Path::new(&place.path);
            let candidate = if candidate.is_absolute() {
                candidate.to_path_buf()
            } else {
                base.join(candidate)
            };
            if let Ok(canonical) = candidate.canonicalize() {
                if canonical.starts_with(&base) {
                    if let Ok(metadata) = std::fs::metadata(&canonical) {
                        if metadata.len()
                            <= (256usize << 20).saturating_sub(self.source_bytes) as u64
                        {
                            if let Ok(bytes) = std::fs::read(&canonical) {
                                let source = crate::source_attachment::SourceAttachment::new(
                                    canonical
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("linked-image"),
                                    "linked-image",
                                    bytes,
                                )?;
                                if image::guess_format(source.bytes()).is_ok() {
                                    let pixels = crate::formats::decode_image(source.bytes())
                                        .map_err(|e| format!("Could not decode linked Illustrator picture {}: {e}", place.path))?;
                                    place.preview = Some(pixels);
                                    preview_only = false;
                                    source_id = Some(source.id.clone());
                                    if !self.sources.contains_key(&source.id) {
                                        self.source_bytes += source.bytes().len();
                                        self.sources.insert(source.id.clone(), source);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some(pixels) = place.preview {
            let [a, b, c, d, tx, ty] = place.matrix;
            let w = (place.bounds[2] - place.bounds[0]).abs();
            let h = (place.bounds[3] - place.bounds[1]).abs();
            let (ox, oy) = self
                .template
                .map(|t| (8191.5 - (t[0] + t[2]) / 2., 8191.5 + (t[1] + t[3]) / 2.))
                .unwrap_or((place.bounds[0], place.bounds[1]));
            let origin = Pt::new(a * ox + c * oy + tx, b * ox + d * oy + ty);
            let (sx, sy) = (w / pixels.w.max(1) as f32, h / pixels.h.max(1) as f32);
            let mut layer = Layer::placed_raster(
                place.path.clone(),
                pixels,
                origin,
                Pt::new(w * a.hypot(b), h * c.hypot(d)),
            );
            layer.source_attachment = source_id;
            self.raster_placements.insert(
                layer.id,
                [a * sx, b * sx, c * sy, d * sy, origin.x, origin.y],
            );
            if preview_only {
                self.note(format!("Linked picture {} uses its saved preview; relink the original file for full resolution.",place.path));
            }
            self.add(layer)?;
        } else {
            self.note(format!(
                "Placed file {} has no readable picture or saved preview.",
                place.path
            ));
        }
        Ok(())
    }

    fn image(&mut self, data: &[u8]) -> Result<(), String> {
        let Some((space, values)) = self.raster.as_ref() else {
            self.note("An embedded image outside a raster record was skipped.");
            return Ok(());
        };
        let n = numbers(values);
        let matrix = values
            .iter()
            .find_map(|v| {
                if let V::Arr(a) = v {
                    numbers(a).try_into().ok()
                } else {
                    None
                }
            })
            .unwrap_or([1., 0., 0., -1., 0., 0.]);
        if n.len() < 8 {
            self.note("Embedded Illustrator raster operands were not recognized.");
            return Ok(());
        }
        let (w, h) = (n[4] as u32, n[5] as u32);
        let bits = n[6] as u32;
        let binary = n.get(10).copied().unwrap_or(1.) != 0.;
        let channels = if space.contains("CMYK") {
            4
        } else if space.contains("Gray") {
            1
        } else {
            3
        };
        if w == 0 || h == 0 || u64::from(w) * u64::from(h) > 32 << 20 || bits != 8 {
            self.note("Embedded Illustrator raster depth or size is unsupported.");
            return Ok(());
        }
        let decoded_hex;
        let _decoded_memory;
        let decoded = if !binary {
            let scratch = data
                .len()
                .checked_mul(3)
                .and_then(|n| n.checked_add(16))
                .ok_or("Illustrator hex image size overflows")?;
            _decoded_memory = crate::resources::global()
                .reserve_now(scratch as u64, &crate::deep_color::storage::check)?;
            decoded_hex = super::hex_decode(data).unwrap_or_default();
            decoded_hex.as_slice()
        } else {
            data
        };
        let count = w as usize * h as usize;
        let need = count * channels;
        if decoded.len() < need {
            self.note("An embedded Illustrator raster is truncated.");
            return Ok(());
        }
        let mut rgba = crate::resources::global()
            .allocate_now::<u8, _>(count * 4, &crate::deep_color::storage::check)?;
        for i in 0..count {
            let p = &decoded[i * channels..(i + 1) * channels];
            let color = match channels {
                4 => cmyk(
                    p[0] as f32 / 255.,
                    p[1] as f32 / 255.,
                    p[2] as f32 / 255.,
                    p[3] as f32 / 255.,
                ),
                1 => Rgba::rgb(p[0], p[0], p[0]),
                _ => Rgba::rgb(p[0], p[1], p[2]),
            };
            rgba.as_mut_slice()[i * 4..i * 4 + 4].copy_from_slice(&[
                color.r,
                color.g,
                color.b,
                if n.get(8).is_some_and(|n| *n > 0.) {
                    decoded.get(need + i).copied().unwrap_or(255)
                } else {
                    255
                },
            ]);
        }
        let pixels =
            crate::document::Pixels::from_byte_allocation(w, h, std::sync::Arc::new(rgba))?;
        let [a, b, c, d, tx, ty] = matrix;
        let layer = Layer::placed_raster(
            "Embedded picture",
            pixels,
            Pt::new(tx, ty),
            Pt::new(w as f32 * a.hypot(b), h as f32 * c.hypot(d)),
        );
        self.raster_placements
            .insert(layer.id, [a, -b, c, -d, tx, ty]);
        self.add(layer)
    }

    fn finish(mut self, name: &str) -> Result<(Document, Vec<String>), String> {
        let plain = self
            .text_slots
            .iter()
            .filter(|(_, _, _, hidden)| !hidden)
            .map(|(_, story, frame, _)| (*story, *frame))
            .collect::<std::collections::HashSet<_>>();
        let copies = self
            .text_slots
            .iter()
            .filter(|(_, story, frame, hidden)| *hidden && plain.contains(&(*story, *frame)))
            .map(|(id, _, _, _)| *id)
            .collect::<std::collections::HashSet<_>>();
        self.layers.retain(|l| !copies.contains(&l.id));
        if self.layers.is_empty() {
            return Err("Illustrator editing data contains no supported artwork".into());
        }
        let sized = self.art_size.zip(self.template).map(|(s, t)| {
            let cx = (t[0] + t[2]) / 2.;
            let cy = (t[1] + t[3]) / 2.;
            [
                cx - s[0] / 2.,
                cy - s[1] / 2.,
                cx + s[0] / 2.,
                cy + s[1] / 2.,
            ]
        });
        let first = self
            .boards
            .first()
            .map(|(_, b)| *b)
            .or(self.crop)
            .or(sized)
            .or(self.bbox)
            .unwrap_or([0., 0., 612., 792.]);
        if self.boards.is_empty() {
            self.boards.push(("Artboard 1".into(), first));
        }
        for (_, b) in &self.boards {
            if !b.iter().all(|v| v.is_finite() && v.abs() < 1e7) || b[2] <= b[0] || b[3] <= b[1] {
                return Err("Invalid Illustrator artboard bounds".into());
            }
        }
        let transform = |p: Pt| Pt::new(p.x - first[0], first[3] - p.y);
        for layer in &mut self.layers {
            if let Some(shapes) = layer.kind.shapes_mut() {
                for shape in shapes {
                    match &mut shape.geom {
                        Geom::Paths { paths, .. } => {
                            for path in paths {
                                for anchor in &mut path.anchors {
                                    anchor.pt = transform(anchor.pt);
                                    anchor.h_in.y = -anchor.h_in.y;
                                    anchor.h_out.y = -anchor.h_out.y;
                                }
                            }
                        }
                        Geom::Text(text) => {
                            text.origin = transform(text.origin);
                            shape.rotation = -shape.rotation;
                        }
                        _ => {}
                    }
                    if let Fill::Gradient(g) = &mut shape.style.fill {
                        g.from[1] = 1. - g.from[1];
                        g.to[1] = 1. - g.to[1];
                    }
                    if let Some(g) = shape
                        .style
                        .stroke
                        .as_mut()
                        .and_then(|s| s.gradient.as_mut())
                    {
                        g.from[1] = 1. - g.from[1];
                        g.to[1] = 1. - g.to[1];
                    }
                }
            }
            if let Some(matrix) = self.raster_placements.get(&layer.id) {
                if place_pixels(layer, *matrix, first).map_err(super::picture_error)? {
                    self.notes.insert("Sheared Illustrator pictures are resampled into placed raster layers; retained linked source bytes remain unchanged.".into());
                }
            }
        }
        let mut doc = Document::new(name, 1., 1., 72.);
        doc.layers = self.layers;
        for source in self.sources.into_values() {
            doc.retain_source(source);
        }
        doc.transparent = true;
        doc.width = first[2] - first[0];
        doc.height = first[3] - first[1];
        doc.artboards = self
            .boards
            .into_iter()
            .enumerate()
            .map(|(i, (name, b))| {
                let mut board = Artboard::new(
                    i,
                    transform(Pt::new(b[0], b[3])),
                    Pt::new(b[2] - b[0], b[3] - b[1]),
                );
                board.name = name;
                board
            })
            .collect();
        if let Some(texts) = &self.texts {
            let mut body = String::new();
            let mut targets = HashMap::new();
            let template = self
                .template
                .map(|t| ((t[0] + t[2]) / 2., (t[1] + t[3]) / 2.))
                .unwrap_or((0., 0.));
            for (slot, (layer, story, frame, _)) in self.text_slots.iter().enumerate() {
                if !doc.layers.iter().any(|l| l.id == *layer) {
                    continue;
                }
                let id = format!("ai-type-{slot}");
                if let Some(svg) = texts.svg_story(*story, *frame, template, first, &id) {
                    body.push_str(&svg);
                    targets.insert(id, *layer);
                }
            }
            if !body.is_empty() {
                let svg = format!(
                    "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\">{body}</svg>",
                    doc.width, doc.height
                );
                match crate::formats::svg::read(&svg, "Illustrator type") {
                    Ok((typed, notes)) => {
                        for shape in typed
                            .layers
                            .iter()
                            .filter_map(|l| l.kind.shapes())
                            .flatten()
                        {
                            if !matches!(&shape.geom, Geom::Text(_)) {
                                continue;
                            }
                            if let Some(layer) = targets
                                .get(&shape.name)
                                .and_then(|id| doc.layers.iter_mut().find(|l| l.id == *id))
                                && let Some(original) =
                                    layer.kind.shapes_mut().and_then(|s| s.first_mut())
                            {
                                let (Geom::Text(source), Geom::Text(imported)) =
                                    (&original.geom, &shape.geom)
                                else {
                                    continue;
                                };
                                if source.content != imported.content {
                                    self.notes.insert("Illustrator source text keeps its original characters and paragraph breaks; the shared SVG type importer changed its content, so native fallback layout is retained.".into());
                                    continue;
                                }
                                original.geom = shape.geom.clone();
                                original.style = shape.style.clone();
                                original.rotation = shape.rotation;
                            }
                        }
                        self.notes
                            .extend(notes.into_iter().map(|n| format!("Illustrator type: {n}")));
                    }
                    Err(error) => {
                        self.notes.insert(format!("Illustrator styled type could not use the shared SVG importer: {error}. Editable source characters remain available."));
                    }
                }
            }
        }
        for layer in &mut doc.layers {
            if let Some(shapes) = layer.kind.shapes_mut() {
                for shape in shapes {
                    crate::text::fill_contours(&mut shape.geom);
                    if self.text_slots.iter().any(|(id, _, _, _)| *id == layer.id)
                        && shape.rotation.abs() > 1e-6
                    {
                        let anchor = if let Geom::Text(text) = &shape.geom {
                            text.origin
                        } else {
                            continue;
                        };
                        let centre = shape.geom.bbox().center();
                        shape
                            .geom
                            .translate(centre.rotate_about(anchor, shape.rotation) - centre);
                    }
                }
            }
        }
        doc.validate_hierarchy()
            .map_err(|error| format!("Invalid Illustrator artwork hierarchy: {error}"))?;
        Ok((doc, self.notes.into_iter().collect()))
    }
}

fn preview(data: &[u8]) -> Result<Option<crate::document::Pixels>, String> {
    let layout = (|| {
        let mut rest = data.trim_ascii_start();
        let mut head = [0usize; 5];
        for item in &mut head {
            let at = rest.iter().position(|b| *b == b' ')?;
            *item = std::str::from_utf8(&rest[..at]).ok()?.parse().ok()?;
            rest = &rest[at + 1..];
        }
        let [w, h, row, bits, _] = head;
        if w == 0
            || h == 0
            || w.checked_mul(h)? > 32 << 20
            || bits != 32
            || row < w.checked_mul(4)?
            || rest.len() < row.checked_mul(h)?
        {
            return None;
        }
        Some((w, h, row, rest))
    })();
    let Some((w, h, row, rest)) = layout else {
        return Ok(None);
    };
    let mut pixels = crate::document::Pixels::new(w as u32, h as u32)?;
    let rgba = pixels.bytes_mut()?;
    for y in 0..h {
        for x in 0..w {
            let p = &rest[y * row + x * 4..y * row + x * 4 + 4];
            let a = p[0] as u32;
            let unpremul = |v| {
                if a == 0 {
                    0
                } else {
                    (v as u32 * 255 / a).min(255) as u8
                }
            };
            let i = (y * w + x) * 4;
            rgba[i..i + 4].copy_from_slice(&[unpremul(p[1]), unpremul(p[2]), unpremul(p[3]), p[0]]);
        }
    }
    Ok(Some(pixels))
}

/// Place picture pixels using their exact Illustrator affine.
/// `matrix` maps pixel coordinates to art space and `board` gives its first
/// artboard. Returns whether shear required resampling; rotation and reflection
/// keep the original sample grid with a native placement transform.
fn place_pixels(layer: &mut Layer, matrix: [f32; 6], board: [f32; 4]) -> Result<bool, String> {
    use crate::document::{LayerKind, Pixels};
    let LayerKind::Raster {
        pixels,
        origin,
        size,
        rotation,
        shear,
    } = &mut layer.kind
    else {
        return Ok(false);
    };
    if !matrix.iter().all(|v| v.is_finite() && v.abs() < 1e8) {
        return Err("Invalid Illustrator picture transform".into());
    }
    let [a, b, c, d, tx, ty] = matrix;
    let top_left = Pt::new(tx - board[0], board[3] - ty);
    let dx = Pt::new(a * pixels.w as f32, -b * pixels.w as f32);
    let dy = Pt::new(c * pixels.h as f32, -d * pixels.h as f32);
    let dimensions = Pt::new(dx.length(), dy.length());
    if dimensions.x <= 0.0001 || dimensions.y <= 0.0001 || dx.cross(dy).abs() < 1e-10 {
        return Err("Degenerate Illustrator picture transform".into());
    }
    if dx.dot(dy).abs() < dimensions.x * dimensions.y * 0.0001 {
        if dx.cross(dy) < 0. {
            super::raster::reflect(pixels)?;
        }
        let centre = top_left + (dx + dy) * 0.5;
        *origin = centre - dimensions * 0.5;
        *size = dimensions;
        *rotation = dx.y.atan2(dx.x);
        *shear = 0.;
        return Ok(false);
    }
    let corners = [top_left, top_left + dx, top_left + dy, top_left + dx + dy];
    let min = corners.iter().copied().reduce(Pt::min).unwrap();
    let max = corners.iter().copied().reduce(Pt::max).unwrap();
    let width = (max.x - min.x).ceil().max(1.) as u32;
    let height = (max.y - min.y).ceil().max(1.) as u32;
    if width as u64 * height as u64 > 32 << 20 {
        return Err("Transformed Illustrator picture exceeds 32 megapixels".into());
    }
    let transform =
        tiny_skia::Transform::from_row(a, -b, c, -d, top_left.x - min.x, top_left.y - min.y);
    if pixels.native.is_some() {
        *pixels = super::raster::resample_native(
            pixels,
            width,
            height,
            transform
                .invert()
                .ok_or("Invalid Illustrator picture transform")?,
        )?;
    } else {
        let mut output = crate::deep_color::surface::Surface::new(width, height)?;
        let input = pixels.try_to_pixmap()?;
        output.draw_pixmap(
            0,
            0,
            input.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            transform,
            None,
        );
        *pixels = Pixels::from_pixmap(&output)?;
    }
    *origin = min;
    *size = Pt::new(width as f32, height as f32);
    *rotation = 0.;
    *shear = 0.;
    Ok(true)
}
