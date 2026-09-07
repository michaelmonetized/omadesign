//! Editable PDF interchange (ISO 32000 graphics, images and optional content).
//!
//! Illustrator's PDF-compatible representation uses the same reader. Illustrator
//! private editing data and PostScript programs are deliberately not interpreted.
//! Every unsupported visual operation produces an import diagnostic.

use crate::color::{Blend, Rgba};
use crate::document::{Artboard, Cap, Document, Fill, Join, Layer, Pixels, Shape, Stroke, Style};
use crate::geom::{Anchor, Geom, Pt, TypeRun};
use lopdf::content::Content;
use lopdf::{Dictionary, Document as Pdf, Object, ObjectId, Stream};
use std::collections::{BTreeSet, HashMap};

mod cff;
mod export;
mod masks;
mod shading;
pub use export::write;
#[cfg(test)]
mod tests;

const STREAM_LIMIT: usize = 128 * 1024 * 1024;
const PIXEL_LIMIT: usize = 64 * 1024 * 1024;
const OBJECT_LIMIT: usize = 500_000;

#[derive(Clone, Copy, Debug)]
struct Matrix([f32; 6]);

impl Matrix {
    const IDENTITY: Self = Self([1., 0., 0., 1., 0., 0.]);

    fn map(self, p: Pt) -> Pt {
        let [a, b, c, d, e, f] = self.0;
        Pt::new(a * p.x + c * p.y + e, b * p.x + d * p.y + f)
    }

    /// Matrix product: apply `rhs` first, then `self`.
    fn concat(self, rhs: Self) -> Self {
        let [a, b, c, d, e, f] = self.0;
        let [g, h, i, j, k, l] = rhs.0;
        Self([
            a * g + c * h,
            b * g + d * h,
            a * i + c * j,
            b * i + d * j,
            a * k + c * l + e,
            b * k + d * l + f,
        ])
    }

    fn scale(self) -> f32 {
        let [a, b, c, d, ..] = self.0;
        (a * d - b * c).abs().sqrt()
    }

    fn translate(self, x: f32, y: f32) -> Self {
        self.concat(Self([1., 0., 0., 1., x, y]))
    }
}

#[derive(Clone)]
struct State {
    matrix: Matrix,
    fill: Rgba,
    stroke: Stroke,
    fill_alpha: f32,
    stroke_alpha: f32,
    blend: Blend,
    fill_space: Vec<u8>,
    stroke_space: Vec<u8>,
    font: Vec<u8>,
    font_size: f32,
    text: Matrix,
    line: Matrix,
    char_space: f32,
    word_space: f32,
    h_scale: f32,
    leading: f32,
    rise: f32,
    text_mode: i64,
    clip: Option<std::sync::Arc<Geom>>,
    soft_mask: Option<std::rc::Rc<masks::MaskData>>,
}

impl State {
    fn new(matrix: Matrix) -> Self {
        Self {
            matrix,
            fill: Rgba::BLACK,
            stroke: Stroke {
                color: Rgba::BLACK,
                width: 1.,
                cap: Cap::Butt,
                join: Join::Miter,
                dash: None,
            },
            fill_alpha: 1.,
            stroke_alpha: 1.,
            blend: Blend::Normal,
            fill_space: b"DeviceGray".to_vec(),
            stroke_space: b"DeviceGray".to_vec(),
            font: vec![],
            font_size: 12.,
            text: Matrix::IDENTITY,
            line: Matrix::IDENTITY,
            char_space: 0.,
            word_space: 0.,
            h_scale: 1.,
            leading: 0.,
            rise: 0.,
            text_mode: 0,
            clip: None,
            soft_mask: None,
        }
    }
}

#[derive(Default)]
struct Subpath {
    anchors: Vec<Anchor>,
    closed: bool,
}

struct Reader<'a> {
    pdf: &'a Pdf,
    document: Document,
    warnings: BTreeSet<String>,
    off: BTreeSet<ObjectId>,
    base_off: bool,
    on: BTreeSet<ObjectId>,
    font_cache: HashMap<Vec<u8>, String>,
    cff_cache: HashMap<usize, std::sync::Arc<Vec<u8>>>,
    objects: usize,
    page_parent: u64,
    page_name: String,
    mask_depth: usize,
    last_mask_key: (usize, usize),
    last_mask_state: Option<State>,
    pixel_bytes: usize,
}

/// Import every PDF page as an artboard with editable objects and named layers.
/// Returns fidelity diagnostics separately so callers can show and persist them.
pub fn read(bytes: &[u8], name: &str) -> Result<(Document, Vec<String>), String> {
    if bytes.len() > 512 * 1024 * 1024 {
        return Err("PDF exceeds the 512 MiB import limit.".into());
    }
    if !bytes.windows(5).take(1024).any(|window| window == b"%PDF-") {
        return Err(if bytes.starts_with(b"%!") {
            "This is a legacy PostScript Illustrator/EPS file. Convert it to PDF with Ghostscript or export a PDF-compatible .ai file first.".into()
        } else {
            "No PDF representation was found. Illustrator files must be saved with Create PDF Compatible File enabled.".into()
        });
    }
    let pdf = Pdf::load_mem_with_options(
        bytes,
        lopdf::LoadOptions::with_max_decompressed_size(STREAM_LIMIT),
    )
    .map_err(|error| format!("Could not read PDF: {error}"))?;
    if pdf.is_encrypted() {
        return Err("This PDF is encrypted. Save an unlocked copy before importing it.".into());
    }
    let pages = pdf.get_pages();
    if pages.is_empty() || pages.len() > 10_000 {
        return Err("PDF must contain between 1 and 10,000 pages.".into());
    }
    // Start tiny: Document::new allocates a background the size of the canvas.
    let mut document = Document::new(name, 1., 1., 72.);
    document.layers.clear();
    document.artboards.clear();
    document.transparent = false;
    let mut reader = Reader {
        pdf: &pdf,
        document,
        warnings: BTreeSet::new(),
        off: BTreeSet::new(),
        base_off: false,
        on: BTreeSet::new(),
        font_cache: HashMap::new(),
        cff_cache: HashMap::new(),
        objects: 0,
        page_parent: 0,
        page_name: String::new(),
        mask_depth: 0,
        last_mask_key: (0, 0),
        last_mask_state: None,
        pixel_bytes: 0,
    };
    reader.optional_content_defaults();
    let mut x = 0.;
    let mut max_height: f32 = 0.;
    for (number, page_id) in pages {
        let media = inherited(&pdf, page_id, b"CropBox")
            .or_else(|| inherited(&pdf, page_id, b"MediaBox"))
            .and_then(|object| rectangle(&pdf, object))
            .ok_or_else(|| format!("Page {number} has no valid page box"))?;
        let user_unit = inherited(&pdf, page_id, b"UserUnit")
            .and_then(number_value)
            .unwrap_or(1.);
        if !(0.001..=75_000.).contains(&user_unit) {
            return Err(format!("Page {number} has an invalid UserUnit"));
        }
        let rotation = inherited(&pdf, page_id, b"Rotate")
            .and_then(number_value)
            .unwrap_or(0.) as i32;
        let rotation = rotation.rem_euclid(360);
        let width = (media[2] - media[0]) * user_unit;
        let height = (media[3] - media[1]) * user_unit;
        if !width.is_finite()
            || !height.is_finite()
            || width <= 0.
            || height <= 0.
            || width.max(height) > 1_000_000.
        {
            return Err(format!("Page {number} has invalid or excessive dimensions"));
        }
        let (page_w, page_h, orientation) = match rotation {
            0 => (width, height, Matrix([1., 0., 0., -1., 0., height])),
            90 => (height, width, Matrix([0., 1., 1., 0., 0., 0.])),
            180 => (width, height, Matrix([-1., 0., 0., 1., width, 0.])),
            270 => (height, width, Matrix([0., -1., -1., 0., height, width])),
            _ => {
                return Err(format!(
                    "Page {number} has unsupported rotation {rotation}; expected a multiple of 90 degrees"
                ));
            }
        };
        let matrix = Matrix::IDENTITY
            .translate(x, 0.)
            .concat(orientation)
            .concat(Matrix([
                user_unit,
                0.,
                0.,
                user_unit,
                -media[0] * user_unit,
                -media[1] * user_unit,
            ]));
        reader.page_name = format!("Page {number}");
        let mut board = Artboard::new(number as usize - 1, Pt::new(x, 0.), Pt::new(page_w, page_h));
        board.name = reader.page_name.clone();
        reader.document.artboards.push(board);
        let page_group = Layer::group(&reader.page_name);
        reader.page_parent = page_group.id;
        let resources = inherited(&pdf, page_id, b"Resources")
            .and_then(|object| dictionary(&pdf, object))
            .cloned()
            .unwrap_or_default();
        let contents = pdf
            .get_page_content_with_limit(page_id, STREAM_LIMIT)
            .map_err(|error| format!("Could not decode page {number}: {error}"))?;
        let mut state = State::new(matrix);
        state.clip = Some(std::sync::Arc::new(Geom::Rect {
            origin: Pt::new(x, 0.),
            size: Pt::new(page_w, page_h),
            radius: 0.,
        }));
        reader.operations(&contents, &resources, state, &[], 0)?;
        if let Ok(page) = pdf.get_dictionary(page_id)
            && page.get(b"Annots").is_ok()
        {
            reader.warn("PDF annotations and form fields are not imported as artwork.");
        }
        reader.document.layers.push(page_group);
        x += page_w + 48.;
        max_height = max_height.max(page_h);
    }
    reader.document.width = (x - 48.).max(1.);
    reader.document.height = max_height.max(1.);
    if name.to_ascii_lowercase().ends_with(".ai") {
        reader.warn("Illustrator's PDF artwork is imported; private Illustrator objects, effects, and editing metadata are not retained.");
        if reader.objects == 0 {
            reader.warn("This Illustrator file's PDF representation contains no artwork. Its artboards were imported, but private Illustrator artwork is unavailable.");
        }
    }
    Ok((reader.document, reader.warnings.into_iter().collect()))
}

fn resolve<'a>(pdf: &'a Pdf, object: &'a Object) -> Option<&'a Object> {
    pdf.dereference(object).ok().map(|(_, object)| object)
}

fn dictionary<'a>(pdf: &'a Pdf, object: &'a Object) -> Option<&'a Dictionary> {
    resolve(pdf, object)?.as_dict().ok()
}

fn inherited<'a>(pdf: &'a Pdf, mut id: ObjectId, key: &[u8]) -> Option<&'a Object> {
    for _ in 0..64 {
        let dict = pdf.get_dictionary(id).ok()?;
        if let Ok(value) = dict.get(key) {
            return resolve(pdf, value);
        }
        id = dict.get(b"Parent").ok()?.as_reference().ok()?;
    }
    None
}

fn number_value(object: &Object) -> Option<f32> {
    match object {
        Object::Integer(value) => Some(*value as f32),
        Object::Real(value) if value.is_finite() => Some(*value),
        _ => None,
    }
}

fn rectangle(pdf: &Pdf, object: &Object) -> Option<[f32; 4]> {
    let values = resolve(pdf, object)?.as_array().ok()?;
    if values.len() != 4 {
        return None;
    }
    Some([
        number_value(&values[0])?,
        number_value(&values[1])?,
        number_value(&values[2])?,
        number_value(&values[3])?,
    ])
}

fn matrix_value(pdf: &Pdf, object: &Object) -> Option<Matrix> {
    let values = resolve(pdf, object)?.as_array().ok()?;
    if values.len() != 6 {
        return None;
    }
    let mut matrix = [0.; 6];
    for (out, value) in matrix.iter_mut().zip(values) {
        *out = number_value(value)?;
    }
    Some(Matrix(matrix))
}

fn resource<'a>(
    pdf: &'a Pdf,
    resources: &'a Dictionary,
    kind: &[u8],
    name: &[u8],
) -> Option<&'a Object> {
    let dict = dictionary(pdf, resources.get(kind).ok()?)?;
    resolve(pdf, dict.get(name).ok()?)
}

fn blend_name(blend: Blend) -> &'static str {
    match blend {
        Blend::Normal => "Normal",
        Blend::Multiply => "Multiply",
        Blend::Screen => "Screen",
        Blend::Overlay => "Overlay",
        Blend::Darken => "Darken",
        Blend::Lighten => "Lighten",
        Blend::ColorDodge => "ColorDodge",
        Blend::ColorBurn => "ColorBurn",
        Blend::HardLight => "HardLight",
        Blend::SoftLight => "SoftLight",
        Blend::Difference => "Difference",
        Blend::Exclusion => "Exclusion",
        Blend::Hue => "Hue",
        Blend::Saturation => "Saturation",
        Blend::Color => "Color",
        Blend::Luminosity => "Luminosity",
    }
}

impl Reader<'_> {
    fn reserve_pixels(&mut self, w: u32, h: u32) -> Result<(), String> {
        let bytes = (w as usize)
            .checked_mul(h as usize)
            .and_then(|count| count.checked_mul(4))
            .ok_or("PDF pixel dimensions overflow")?;
        self.pixel_bytes = self
            .pixel_bytes
            .checked_add(bytes)
            .filter(|total| *total <= 512 * 1024 * 1024)
            .ok_or("PDF decoded images and masks exceed the 512 MiB import budget")?;
        Ok(())
    }
    fn warn(&mut self, warning: &str) {
        if self.warnings.len() >= 64 {
            self.warnings
                .insert("Additional PDF compatibility warnings were omitted.".into());
            return;
        }
        self.warnings.insert(warning.into());
    }

    fn optional_content_defaults(&mut self) {
        let Some(config) = self
            .pdf
            .catalog()
            .ok()
            .and_then(|catalog| catalog.get(b"OCProperties").ok())
            .and_then(|object| dictionary(self.pdf, object))
            .and_then(|properties| properties.get(b"D").ok())
            .and_then(|object| dictionary(self.pdf, object))
        else {
            return;
        };
        self.base_off = config
            .get(b"BaseState")
            .and_then(Object::as_name)
            .is_ok_and(|value| value == b"OFF");
        for (key, set) in [
            (b"OFF".as_slice(), &mut self.off),
            (b"ON".as_slice(), &mut self.on),
        ] {
            if let Ok(objects) = config.get(key).and_then(Object::as_array) {
                set.extend(
                    objects
                        .iter()
                        .filter_map(|object| object.as_reference().ok()),
                );
            }
        }
    }

    fn ocg(&mut self, object: &Object) -> Option<(String, bool)> {
        let (id, object) = self.pdf.dereference(object).ok()?;
        let dict = object.as_dict().ok()?;
        if dict
            .get(b"Type")
            .and_then(Object::as_name)
            .is_ok_and(|name| name == b"OCMD")
        {
            self.warn("PDF optional-content membership expressions are not evaluated; their artwork remains editable.");
            return Some(("Optional content".into(), true));
        }
        let name: String = dict
            .get(b"Name")
            .ok()
            .and_then(|object| lopdf::decode_text_string(object).ok())
            .unwrap_or_else(|| "PDF layer".into())
            .chars()
            .take(256)
            .collect();
        let visible = id
            .is_none_or(|id| !self.off.contains(&id) && (!self.base_off || self.on.contains(&id)));
        Some((name, visible))
    }

    fn add_layer(&mut self, mut layer: Layer, _marked: &[(String, bool)]) -> Result<(), String> {
        self.objects += 1;
        if self.objects > OBJECT_LIMIT {
            return Err("PDF contains too many editable objects (limit 500,000).".into());
        }
        layer.parent = Some(self.page_parent);
        self.document.layers.push(layer);
        Ok(())
    }

    fn add_shapes(
        &mut self,
        shapes: Vec<Shape>,
        state: &State,
        marked: &[(String, bool)],
    ) -> Result<(), String> {
        if shapes.is_empty() {
            return Ok(());
        }
        let mut effective_state = state.clone();
        let artwork_bounds = shapes
            .iter()
            .map(|shape| {
                shape.world_bbox().inflate(
                    shape
                        .style
                        .stroke
                        .as_ref()
                        .map_or(0., |stroke| stroke.width * 0.5),
                )
            })
            .reduce(|a, b| a.union(b))
            .unwrap();
        if let Some(clip) = &state.clip
            && masks::rectangle_contains(clip, artwork_bounds)
        {
            effective_state.clip = state.soft_mask.as_ref().map(|_| {
                std::sync::Arc::new(Geom::Rect {
                    origin: artwork_bounds.min,
                    size: Pt::new(artwork_bounds.width(), artwork_bounds.height()),
                    radius: 0.,
                })
            });
        }
        let state = &effective_state;
        let mask_key = (
            state
                .clip
                .as_ref()
                .map_or(0, |value| std::sync::Arc::as_ptr(value) as usize),
            state
                .soft_mask
                .as_ref()
                .map_or(0, |value| std::rc::Rc::as_ptr(value) as usize),
        );
        self.objects += shapes.len();
        if self.objects > OBJECT_LIMIT {
            return Err("PDF contains too many editable objects (limit 500,000).".into());
        }
        // Adjacent vector paints under the same optional-content parent form a
        // single native layer. Images and group boundaries still split the run,
        // preserving the PDF painter order without thousands of one-shape layers.
        if let Some(previous) = self.document.layers.last_mut()
            && !previous.is_group
            && previous.parent == Some(self.page_parent)
            && previous.blend == state.blend
            && self.last_mask_key == mask_key
            && let Some(previous_shapes) = previous.kind.shapes_mut()
        {
            previous_shapes.extend(shapes);
            return Ok(());
        }
        let mut layer = Layer::vector("Artwork");
        layer.blend = state.blend;
        if let Some(mask) = self.paint_mask(state)? {
            layer.mask_origin = mask.origin;
            layer.mask_size = mask.size;
            layer.mask = Some(mask.pixels);
        }
        self.last_mask_key = mask_key;
        self.last_mask_state = Some(state.clone());
        layer.kind.shapes_mut().unwrap().extend(shapes);
        self.add_layer(layer, marked)
    }

    fn paint(
        &mut self,
        paths: &[Subpath],
        state: &State,
        fill: bool,
        stroke: bool,
        winding: bool,
        marked: &[(String, bool)],
    ) -> Result<(), String> {
        let paths = paths
            .iter()
            .filter(|path| path.anchors.len() > 1)
            .collect::<Vec<_>>();
        let mut fill_color = state.fill;
        fill_color.a = (state.fill_alpha.clamp(0., 1.) * 255.).round() as u8;
        let mut line = state.stroke.clone();
        line.width *= state.matrix.scale();
        line.color.a = (state.stroke_alpha.clamp(0., 1.) * 255.).round() as u8;
        if let Some((dash, gap)) = &mut line.dash {
            let scale = state.matrix.scale();
            *dash *= scale;
            *gap *= scale;
        }
        let make_style = |has_fill: bool, has_stroke: bool| Style {
            fill: if has_fill {
                Fill::Solid(fill_color)
            } else {
                Fill::None
            },
            stroke: has_stroke.then(|| line.clone()),
        };
        let mut shapes = vec![];
        if paths.len() == 1 && paths[0].anchors.len() > 1 {
            let path = &paths[0];
            if fill && stroke && !path.closed {
                shapes.push(Shape::new(
                    Geom::Path {
                        anchors: path.anchors.clone(),
                        closed: true,
                    },
                    make_style(true, false),
                ));
                shapes.push(Shape::new(
                    Geom::Path {
                        anchors: path.anchors.clone(),
                        closed: false,
                    },
                    make_style(false, true),
                ));
            } else {
                shapes.push(Shape::new(
                    Geom::Path {
                        anchors: path.anchors.clone(),
                        closed: fill || path.closed,
                    },
                    make_style(fill, stroke),
                ));
            }
        } else {
            if fill {
                if paths.iter().any(|path| {
                    path.anchors
                        .iter()
                        .any(|anchor| anchor.h_in != Pt::ZERO || anchor.h_out != Pt::ZERO)
                }) {
                    self.warn("Compound PDF curves are retained as editable polygon contours; single-contour curves keep their Bézier handles.");
                }
                let contours = paths
                    .iter()
                    .filter(|path| path.anchors.len() > 1)
                    .flat_map(|path| {
                        Geom::Path {
                            anchors: path.anchors.clone(),
                            closed: true,
                        }
                        .contours(96)
                    })
                    .collect::<Vec<_>>();
                if !contours.is_empty() {
                    shapes.push(Shape::new(
                        Geom::Poly { contours, winding },
                        make_style(true, false),
                    ));
                }
            }
            if stroke {
                shapes.extend(
                    paths
                        .iter()
                        .filter(|path| path.anchors.len() > 1)
                        .map(|path| {
                            Shape::new(
                                Geom::Path {
                                    anchors: path.anchors.clone(),
                                    closed: path.closed,
                                },
                                make_style(false, true),
                            )
                        }),
                );
            }
        }
        self.add_shapes(shapes, state, marked)
    }

    fn operations(
        &mut self,
        bytes: &[u8],
        resources: &Dictionary,
        mut state: State,
        inherited_marks: &[(String, bool)],
        depth: usize,
    ) -> Result<(), String> {
        if depth > 24 {
            return Err("PDF Form XObjects exceed the nesting limit of 24.".into());
        }
        let content = Content::decode(bytes)
            .map_err(|error| format!("Could not decode PDF drawing instructions: {error}"))?;
        if content.operations.len() > 2_000_000 {
            return Err("PDF drawing instruction limit exceeded.".into());
        }
        let mut saved = Vec::new();
        let mut paths: Vec<Subpath> = Vec::new();
        let mut pending_clip: Option<Geom> = None;
        let mut marked = inherited_marks.to_vec();
        let mut mark_lengths = Vec::new();
        let mut mark_groups: Vec<Option<Layer>> = Vec::new();
        for operation in content.operations {
            let args = &operation.operands;
            let n = |index: usize| args.get(index).and_then(number_value).unwrap_or(0.);
            let point = |index: usize| state.matrix.map(Pt::new(n(index), n(index + 1)));
            match operation.operator.as_str() {
                "q" => { if saved.len() >= 128 { return Err("PDF graphics-state nesting exceeds 128.".into()); } saved.push(state.clone()); }
                "Q" => { if let Some(previous) = saved.pop() { state = previous; } else { self.warn("PDF contains unmatched graphics-state restore operations."); } }
                "cm" if args.len() == 6 => state.matrix = state.matrix.concat(Matrix([n(0),n(1),n(2),n(3),n(4),n(5)])),
                "m" => paths.push(Subpath { anchors: vec![Anchor::corner(point(0))], closed: false }),
                "l" => { if let Some(path) = paths.last_mut() { path.anchors.push(Anchor::corner(point(0))); } }
                "c" | "v" | "y" => {
                    if let Some(path) = paths.last_mut() && let Some(last) = path.anchors.last_mut() {
                        let (a, b, end) = match operation.operator.as_str() {
                            "v" => (last.pt, point(0), point(2)), "y" => (point(0), point(2), point(2)),
                            _ => (point(0), point(2), point(4)),
                        };
                        last.h_out = a-last.pt;
                        let mut anchor = Anchor::corner(end); anchor.h_in = b-end; path.anchors.push(anchor);
                    }
                }
                "h" => { if let Some(path) = paths.last_mut() { path.closed = true; } }
                "re" => {
                    paths.push(Subpath { anchors: [Pt::new(n(0),n(1)), Pt::new(n(0)+n(2),n(1)), Pt::new(n(0)+n(2),n(1)+n(3)), Pt::new(n(0),n(1)+n(3))]
                        .map(|p| Anchor::corner(state.matrix.map(p))).to_vec(), closed: true });
                }
                "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" => {
                    let op = operation.operator.as_str();
                    if matches!(op, "s" | "b" | "b*") && let Some(path) = paths.last_mut() { path.closed = true; }
                    self.paint(&paths, &state, !matches!(op, "S" | "s"), matches!(op, "S" | "s" | "B" | "B*" | "b" | "b*"), !op.ends_with('*'), &marked)?;
                    paths.clear();
                    if let Some(clip)=pending_clip.take() {masks::intersect_clip(&mut state,clip);}
                }
                "n" => { paths.clear(); if let Some(clip)=pending_clip.take() {masks::intersect_clip(&mut state,clip);} }
                "W" | "W*" => { pending_clip=Some(Geom::Poly {contours:paths.iter().flat_map(|path|Geom::Path {anchors:path.anchors.clone(),closed:true}.contours(96)).collect(),winding:operation.operator=="W"}); }
                "w" => state.stroke.width = n(0).abs(),
                "J" => state.stroke.cap = match n(0) as i32 { 1 => Cap::Round, 2 => Cap::Square, _ => Cap::Butt },
                "j" => state.stroke.join = match n(0) as i32 { 1 => Join::Round, 2 => Join::Bevel, _ => Join::Miter },
                "M" => { if (n(0)-4.).abs() > 0.01 { self.warn("Custom PDF stroke miter limits use Omadesign's default limit."); } }
                "d" => {
                    let dash = args.first().and_then(|object| object.as_array().ok()).cloned().unwrap_or_default();
                    state.stroke.dash = match dash.as_slice() {
                        [] => None,
                        [one] => Some((number_value(one).unwrap_or(1.), number_value(one).unwrap_or(1.))),
                        [one, two] => Some((number_value(one).unwrap_or(1.), number_value(two).unwrap_or(1.))),
                        _ => { self.warn("Complex PDF dash patterns are approximated by their first dash and gap."); Some((number_value(&dash[0]).unwrap_or(1.), number_value(&dash[1]).unwrap_or(1.))) },
                    };
                    if n(1).abs() > 0.001 { self.warn("PDF dash phase is not retained."); }
                }
                "g" => state.fill = gray(n(0)), "G" => state.stroke.color = gray(n(0)),
                "rg" => state.fill = rgb(n(0), n(1), n(2)), "RG" => state.stroke.color = rgb(n(0), n(1), n(2)),
                "k" => { state.fill = cmyk(n(0),n(1),n(2),n(3)); self.warn("PDF CMYK colors are converted to RGB without an output ICC profile."); }
                "K" => { state.stroke.color = cmyk(n(0),n(1),n(2),n(3)); self.warn("PDF CMYK colors are converted to RGB without an output ICC profile."); }
                "cs" => state.fill_space = args.first().and_then(|object| object.as_name().ok()).unwrap_or(b"DeviceGray").to_vec(),
                "CS" => state.stroke_space = args.first().and_then(|object| object.as_name().ok()).unwrap_or(b"DeviceGray").to_vec(),
                "sc" | "scn" | "SC" | "SCN" => {
                    let fill = operation.operator.starts_with('s');
                    let space = if fill { &state.fill_space } else { &state.stroke_space };
                    let color = match space.as_slice() { b"DeviceRGB" => rgb(n(0),n(1),n(2)), b"DeviceGray" => gray(n(0)), b"DeviceCMYK" => { self.warn("PDF CMYK colors are converted to RGB without an output ICC profile."); cmyk(n(0),n(1),n(2),n(3)) }, _ => { self.warn("PDF pattern, spot, and ICC color spaces are approximated; embedded color profiles are not applied."); if args.len() >= 3 { rgb(n(0),n(1),n(2)) } else { gray(n(0)) } } };
                    if fill { state.fill = color; } else { state.stroke.color = color; }
                }
                "gs" => { if let Some(name) = args.first().and_then(|object| object.as_name().ok()) { self.graphics_state(resources, name, &mut state); } }
                "Do" => {
                    if let Some(name) = args.first().and_then(|object| object.as_name().ok())
                        && let Some(object) = resource(self.pdf, resources, b"XObject", name)
                        && let Ok(stream) = object.as_stream() {
                        let subtype = stream.dict.get(b"Subtype").and_then(Object::as_name).unwrap_or(b"");
                        let mut own_marks = marked.clone();
                        let own_group = if let Ok(oc) = stream.dict.get(b"OC") && let Some(mark) = self.ocg(oc) {
                            let mut group = Layer::group(&mark.0);
                            group.visible = mark.1;
                            group.parent = Some(self.page_parent);
                            self.page_parent = group.id;
                            own_marks.push(mark);
                            Some(group)
                        } else { None };
                        match subtype {
                            b"Form" => {
                                let mut form_state = state.clone();
                                if let Ok(matrix) = stream.dict.get(b"Matrix") && let Some(matrix) = matrix_value(self.pdf, matrix) { form_state.matrix = form_state.matrix.concat(matrix); }
                                if let Ok(bbox)=stream.dict.get(b"BBox") && let Some(bbox)=rectangle(self.pdf,bbox) {let clip=masks::transformed_rectangle(bbox,form_state.matrix);masks::intersect_clip(&mut form_state,clip);}
                                let form_group=if let Some(group_dict)=stream.dict.get(b"Group").ok().and_then(|object|dictionary(self.pdf,object)) {
                                    let mut group=Layer::group("Transparency group");group.parent=Some(self.page_parent);group.pass_through=false;group.opacity=state.fill_alpha;group.blend=state.blend;
                                    if let Some(mask)=self.paint_mask(&form_state)? {group.mask_origin=mask.origin;group.mask_size=mask.size;group.mask=Some(mask.pixels);}
                                    if group_dict.get(b"K").and_then(Object::as_bool).unwrap_or(false) {self.warn("PDF transparency-group knockout is not retained.");}
                                    if !group_dict.get(b"I").and_then(Object::as_bool).unwrap_or(false) {self.warn("Non-isolated PDF transparency groups use isolated compositing.");}
                                    self.page_parent=group.id;form_state.clip=None;form_state.soft_mask=None;form_state.fill_alpha=1.;form_state.stroke_alpha=1.;form_state.blend=Blend::Normal;
                                    Some(group)
                                }else{None};
                                let form_resources = stream.dict.get(b"Resources").ok().and_then(|object| dictionary(self.pdf, object)).unwrap_or(resources);
                                let content = decoded(stream, STREAM_LIMIT)?;
                                self.operations(&content, form_resources, form_state, &own_marks, depth+1)?;
                                if let Some(group)=form_group {self.page_parent=group.parent.unwrap();self.document.layers.push(group);}
                            }
                            b"Image" => match self.image(stream, 0) {
                                Ok(pixels) => { let mut layer = placed_image(pixels, state.matrix, &mut self.warnings)?; self.mask_image(&mut layer,&state)?; layer.opacity = state.fill_alpha; layer.blend = state.blend; self.add_layer(layer, &own_marks)?; }
                                Err(error) => self.warn(&format!("An embedded PDF image could not be imported: {error}")),
                            },
                            _ => self.warn("An unsupported PDF XObject was skipped."),
                        }
                        if let Some(group) = own_group {
                            self.page_parent = group.parent.unwrap();
                            self.document.layers.push(group);
                        }
                    } else { self.warn("A PDF XObject resource is missing."); }
                }
                "BMC" => { mark_lengths.push(marked.len()); mark_groups.push(None); }
                "BDC" => {
                    mark_lengths.push(marked.len());
                    let mut group = None;
                    if args.first().and_then(|object| object.as_name().ok()) == Some(b"OC") && let Some(properties) = args.get(1) {
                        let object = if let Ok(name) = properties.as_name() {
                            resources.get(b"Properties").ok().and_then(|object| dictionary(self.pdf, object)).and_then(|dict| dict.get(name).ok())
                        } else { Some(properties) };
                        if let Some(object) = object && let Some(mark) = self.ocg(object) {
                            let mut layer = Layer::group(&mark.0);
                            layer.visible = mark.1;
                            layer.parent = Some(self.page_parent);
                            self.page_parent = layer.id;
                            group = Some(layer);
                            marked.push(mark);
                        }
                    }
                    if mark_groups.len() >= 64 { return Err("PDF marked-content nesting exceeds 64.".into()); }
                    mark_groups.push(group);
                }
                "EMC" => {
                    if let Some(length) = mark_lengths.pop() { marked.truncate(length); }
                    if let Some(Some(group)) = mark_groups.pop() {
                        self.page_parent = group.parent.unwrap();
                        self.document.layers.push(group);
                    }
                }
                "BT" => { state.text = Matrix::IDENTITY; state.line = Matrix::IDENTITY; }
                "ET" => {},
                "Tf" => { state.font = args.first().and_then(|object| object.as_name().ok()).unwrap_or(b"").to_vec(); state.font_size = n(1); }
                "Tm" => { state.text = Matrix([n(0),n(1),n(2),n(3),n(4),n(5)]); state.line = state.text; }
                "Td" | "TD" => { if operation.operator == "TD" { state.leading = -n(1); } state.line = state.line.translate(n(0),n(1)); state.text = state.line; }
                "T*" => { state.line = state.line.translate(0.,-state.leading); state.text = state.line; }
                "Tc" => state.char_space = n(0), "Tw" => state.word_space = n(0), "Tz" => state.h_scale = n(0)/100.,
                "TL" => state.leading = n(0), "Ts" => state.rise = n(0), "Tr" => state.text_mode = n(0) as i64,
                "Tj" => { if let Some(Object::String(bytes, _)) = args.first() { self.text(bytes, resources, &mut state, &marked)?; } }
                "TJ" => { if let Some(Object::Array(items)) = args.first() { for item in items { match item { Object::String(bytes, _) => self.text(bytes, resources, &mut state, &marked)?, _ => { if let Some(adjust) = number_value(item) { state.text = state.text.translate(-adjust*state.font_size*state.h_scale/1000.,0.); } } } } } }
                "'" | "\"" => {
                    if operation.operator == "\"" { state.word_space = n(0); state.char_space = n(1); }
                    state.line = state.line.translate(0.,-state.leading); state.text = state.line;
                    if let Some(Object::String(bytes, _)) = args.last() { self.text(bytes, resources, &mut state, &marked)?; }
                }
                "sh" => { if let Some(name)=args.first().and_then(|object|object.as_name().ok()) && let Err(error)=self.shading(resources,name,&state,&marked) {self.warn(&format!("PDF shading could not be imported: {error}"));} }
                "BI" | "ID" | "EI" => self.warn("PDF inline image records are not imported; use Image XObjects for interchange."),
                "ri" => self.warn("PDF rendering intents are not applied."),
                "i" | "BX" | "EX" | "MP" | "DP" => {},
                other => self.warn(&format!("Unsupported PDF drawing operator {other:?}; its effect is not retained.")),
            }
        }
        for group in mark_groups.into_iter().rev().flatten() {
            self.warn("An unclosed PDF optional-content group was repaired during import.");
            self.page_parent = group.parent.unwrap();
            self.document.layers.push(group);
        }
        Ok(())
    }

    fn graphics_state(&mut self, resources: &Dictionary, name: &[u8], state: &mut State) {
        let Some(dict) = resource(self.pdf, resources, b"ExtGState", name)
            .and_then(|object| object.as_dict().ok())
        else {
            return;
        };
        if let Ok(value) = dict.get(b"ca")
            && let Some(alpha) = number_value(value)
        {
            state.fill_alpha = alpha.clamp(0., 1.);
        }
        if let Ok(value) = dict.get(b"CA")
            && let Some(alpha) = number_value(value)
        {
            state.stroke_alpha = alpha.clamp(0., 1.);
        }
        if let Ok(value) = dict.get(b"LW")
            && let Some(width) = number_value(value)
        {
            state.stroke.width = width.abs();
        }
        if let Ok(value) = dict.get(b"BM") {
            let name = value
                .as_name()
                .ok()
                .or_else(|| value.as_array().ok()?.first()?.as_name().ok());
            if let Some(blend) = name.and_then(|name| {
                Blend::ALL
                    .into_iter()
                    .find(|blend| blend_name(*blend).as_bytes() == name)
            }) {
                state.blend = blend;
            } else {
                self.warn("An unsupported PDF blend mode uses Normal blending.");
            }
        }
        if let Ok(value) = dict.get(b"SMask") {
            if value.as_name().ok() == Some(b"None") {
                state.soft_mask = None;
            } else {
                match self.soft_mask(value, state) {
                    Ok(mask) => state.soft_mask = Some(mask),
                    Err(error) => {
                        state.soft_mask = None;
                        self.warn(&format!("PDF soft mask could not be imported: {error}"));
                    }
                }
            }
        }
        if [b"OP".as_slice(), b"op"]
            .iter()
            .any(|key| dict.get(key).and_then(Object::as_bool).unwrap_or(false))
            || [b"TR".as_slice(), b"TR2"].iter().any(|key| {
                dict.get(key).is_ok_and(|value| {
                    !matches!(value.as_name().ok(), Some(b"Identity" | b"Default"))
                })
            })
            || dict.get(b"HT").is_ok()
        {
            self.warn("PDF overprint, transfer functions, and halftones are not retained.");
        }
    }
}

fn byte(value: f32) -> u8 {
    (value.clamp(0., 1.) * 255.).round() as u8
}
fn rgb(r: f32, g: f32, b: f32) -> Rgba {
    Rgba::rgb(byte(r), byte(g), byte(b))
}
fn gray(g: f32) -> Rgba {
    rgb(g, g, g)
}
fn cmyk(c: f32, m: f32, y: f32, k: f32) -> Rgba {
    rgb(
        (1. - c) * (1. - k),
        (1. - m) * (1. - k),
        (1. - y) * (1. - k),
    )
}

fn decoded(stream: &Stream, limit: usize) -> Result<Vec<u8>, String> {
    if stream.dict.get(b"Filter").is_err() {
        if stream.content.len() > limit {
            return Err("PDF stream exceeds the decoded-size limit.".into());
        }
        Ok(stream.content.clone())
    } else {
        stream
            .decompressed_content_with_limit(limit)
            .map_err(|error| format!("PDF stream decoding failed: {error}"))
    }
}

impl Reader<'_> {
    fn text(
        &mut self,
        bytes: &[u8],
        resources: &Dictionary,
        state: &mut State,
        marked: &[(String, bool)],
    ) -> Result<(), String> {
        let font = resource(self.pdf, resources, b"Font", &state.font)
            .and_then(|object| object.as_dict().ok());
        let content = font
            .and_then(|font| {
                font.get_font_encoding_with_limit(self.pdf, STREAM_LIMIT)
                    .ok()
            })
            .and_then(|encoding| Pdf::decode_text(&encoding, bytes).ok());
        let content = content.unwrap_or_else(|| {
            self.warn("Some PDF text could not be decoded with its font encoding; replacement characters may appear.");
            String::from_utf8_lossy(bytes).into_owned()
        });
        if content.contains('\u{fffd}') {
            self.warn(
                "Some PDF font character mappings are missing; replacement characters may appear.",
            );
        }
        if let Some(font) = font
            && let Some((contours, advance)) = self.cff_text(bytes, &content, font, state)
        {
            state.text = state.text.translate(advance * state.h_scale, 0.);
            self.warn("Embedded CFF PDF text is preserved as editable outlines; live text editing is not retained for these fonts.");
            if state.text_mode >= 4 {
                self.warn("PDF text clipping is not retained.");
            }
            if matches!(state.text_mode, 3 | 7) || content.is_empty() {
                return Ok(());
            }
            let mut shape = Shape::new(
                Geom::Poly {
                    contours,
                    winding: true,
                },
                text_style(state),
            );
            shape.name = content.chars().take(80).collect();
            return self.add_shapes(vec![shape], state, marked);
        }
        let font_path = self.text_font(font);
        let font_size = state.font_size.abs().max(0.1);
        let transform = state
            .matrix
            .concat(state.text)
            .translate(0., state.rise)
            .concat(Matrix([state.h_scale, 0., 0., -1., 0., 0.]));
        let origin = transform.map(Pt::ZERO);
        let [a, b, c, d, ..] = transform.0;
        let simple =
            b.abs() < 0.0001 && c.abs() < 0.0001 && a > 0. && d > 0. && (a - d).abs() < 0.0001;
        let mut run = TypeRun {
            origin: if simple { origin } else { Pt::ZERO },
            content: content.clone(),
            font: font_path,
            px: font_size * if simple { a } else { 1. },
            tracking: state.char_space * if simple { a } else { 1. },
            kern: false,
            liga: false,
            ..TypeRun::default()
        };
        run.contours = crate::text::shape(&run);
        let advance = if let Some(font) = font {
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
            if let Some(widths) = widths {
                bytes
                    .iter()
                    .map(|code| {
                        (*code as usize)
                            .checked_sub(first)
                            .and_then(|index| widths.get(index))
                            .and_then(number_value)
                            .unwrap_or(500.)
                            * font_size
                            / 1000.
                    })
                    .sum::<f32>()
            } else {
                self.warn("PDF text without simple font widths is reflowed using the imported font; glyph positioning can differ.");
                text_advance(&run) / if simple { a.max(0.0001) } else { 1. }
            }
        } else {
            font_size * 0.5 * content.chars().count() as f32
        };
        let advance = advance
            + state.char_space * content.chars().count() as f32
            + state.word_space * bytes.iter().filter(|byte| **byte == b' ').count() as f32;
        state.text = state.text.translate(advance * state.h_scale, 0.);
        if state.word_space.abs() > 0.001 {
            self.warn("PDF word spacing is approximated by the editable text engine.");
        }
        if state.text_mode >= 4 {
            self.warn("PDF text clipping is not retained.");
        }
        if matches!(state.text_mode, 3 | 7) || content.is_empty() {
            return Ok(());
        }
        let geom = if simple {
            Geom::Text(run)
        } else {
            self.warn("Rotated, reflected, or sheared PDF text is retained as editable outlines.");
            for contour in &mut run.contours {
                for point in contour {
                    *point = transform.map(*point);
                }
            }
            Geom::Poly {
                contours: run.contours,
                winding: true,
            }
        };
        let mut shape = Shape::new(geom, text_style(state));
        shape.name = content.chars().take(80).collect();
        self.add_shapes(vec![shape], state, marked)
    }

    fn text_font(&mut self, font: Option<&Dictionary>) -> String {
        let Some(font) = font else {
            self.warn("A PDF text font is missing; the default font is substituted.");
            return String::new();
        };
        let base = font
            .get(b"BaseFont")
            .and_then(Object::as_name)
            .unwrap_or(b"Unknown");
        // Include descriptor reference in the cache key: different embedded fonts
        // may intentionally use the same PostScript name on different pages.
        let mut key = base.to_vec();
        if let Ok(descriptor) = font.get(b"FontDescriptor") {
            key.extend_from_slice(format!("{descriptor:?}").as_bytes());
        }
        if let Some(path) = self.font_cache.get(&key) {
            return path.clone();
        }
        let descriptor_font = font
            .get(b"DescendantFonts")
            .ok()
            .and_then(|object| resolve(self.pdf, object))
            .and_then(|object| object.as_array().ok())
            .and_then(|array| array.first())
            .and_then(|object| dictionary(self.pdf, object))
            .unwrap_or(font);
        let descriptor = descriptor_font
            .get(b"FontDescriptor")
            .ok()
            .and_then(|object| dictionary(self.pdf, object));
        let embedded = descriptor
            .and_then(|dict| {
                dict.get(b"FontFile2")
                    .or_else(|_| dict.get(b"FontFile3"))
                    .ok()
            })
            .and_then(|object| resolve(self.pdf, object))
            .and_then(|object| object.as_stream().ok());
        if let Some(stream) = embedded
            && let Ok(bytes) = decoded(stream, 32 * 1024 * 1024)
        {
            let id = format!("omatype:{:032x}", crate::typography::fingerprint(&bytes));
            let label = String::from_utf8_lossy(base);
            if crate::text::register_memory_font(&id, &label, std::sync::Arc::new(bytes)).is_ok() {
                self.font_cache.insert(key, id.clone());
                self.warn("Embedded PDF fonts are retained when readable; subset fonts may lack characters needed for later text edits.");
                return id;
            }
        }
        let name = String::from_utf8_lossy(base);
        let name = name.split_once('+').map(|(_, name)| name).unwrap_or(&name);
        let normalized = |name: &str| {
            name.chars()
                .filter(|ch| ch.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect::<String>()
        };
        let wanted = normalized(name);
        let path = crate::text::fonts()
            .iter()
            .find(|face| normalized(&face.name) == wanted)
            .map(|face| face.path.to_string_lossy().into_owned())
            .unwrap_or_default();
        if path.is_empty() {
            self.warn(&format!(
                "PDF font {name:?} is unavailable or unsupported; the default font is substituted."
            ));
        }
        self.font_cache.insert(key, path.clone());
        path
    }

    fn image(&mut self, stream: &Stream, depth: usize) -> Result<Pixels, String> {
        if depth > 4 {
            return Err("Image soft-mask nesting exceeds 4".into());
        }
        let w = stream
            .dict
            .get(b"Width")
            .ok()
            .and_then(number_value)
            .unwrap_or(0.) as u32;
        let h = stream
            .dict
            .get(b"Height")
            .ok()
            .and_then(number_value)
            .unwrap_or(0.) as u32;
        let count = (w as usize)
            .checked_mul(h as usize)
            .filter(|count| *count > 0 && *count <= PIXEL_LIMIT)
            .ok_or("Image dimensions exceed the 64-megapixel limit")?;
        self.reserve_pixels(w, h)?;
        let filters = stream.filters().unwrap_or_default();
        let mut pixels = if filters.iter().any(|filter| filter == b"DCTDecode") {
            if filters.len() != 1 {
                return Err("JPEG with additional PDF filters is unsupported".into());
            }
            let probe = image::ImageReader::new(std::io::Cursor::new(&stream.content))
                .with_guessed_format()
                .map_err(|error| error.to_string())?;
            let actual = probe.into_dimensions().map_err(|error| error.to_string())?;
            if actual != (w, h) {
                return Err("JPEG dimensions do not match the PDF image dictionary".into());
            }
            let image = image::load_from_memory(&stream.content)
                .map_err(|error| error.to_string())?
                .to_rgba8();
            Pixels::from_rgba(w, h, image.into_raw()).ok_or("Invalid JPEG pixels")?
        } else {
            let depth = stream
                .dict
                .get(b"BitsPerComponent")
                .ok()
                .and_then(number_value)
                .unwrap_or(1.) as usize;
            if !matches!(depth, 1 | 8) {
                return Err(format!("{depth}-bit PDF image samples are unsupported"));
            }
            if stream
                .dict
                .get(b"ImageMask")
                .and_then(Object::as_bool)
                .unwrap_or(false)
            {
                return Err(
                    "Stencil image masks require the current paint and are unsupported".into(),
                );
            }
            let space = stream
                .dict
                .get(b"ColorSpace")
                .ok()
                .and_then(|object| resolve(self.pdf, object))
                .ok_or("Image color space is missing")?;
            let mut palette: Option<Vec<u8>> = None;
            let channels = match space {
                Object::Name(name) => match name.as_slice() {
                    b"DeviceGray" | b"G" => 1,
                    b"DeviceRGB" | b"RGB" => 3,
                    b"DeviceCMYK" | b"CMYK" => 4,
                    _ => return Err("Unsupported image color space".into()),
                },
                Object::Array(array)
                    if array.first().and_then(|object| object.as_name().ok())
                        == Some(b"ICCBased") =>
                {
                    self.warn("Embedded PDF image ICC profiles are not applied.");
                    array
                        .get(1)
                        .and_then(|object| resolve(self.pdf, object))
                        .and_then(|object| object.as_stream().ok())
                        .and_then(|stream| stream.dict.get(b"N").ok())
                        .and_then(number_value)
                        .unwrap_or(3.) as usize
                }
                Object::Array(array)
                    if array.first().and_then(|object| object.as_name().ok())
                        == Some(b"Indexed") =>
                {
                    if array.get(1).and_then(|object| object.as_name().ok()) != Some(b"DeviceRGB") {
                        return Err("Indexed PDF images require an RGB palette".into());
                    }
                    let lookup = array
                        .get(3)
                        .and_then(|object| resolve(self.pdf, object))
                        .ok_or("Indexed image palette missing")?;
                    palette = Some(match lookup {
                        Object::String(bytes, _) => bytes.clone(),
                        Object::Stream(stream) => decoded(stream, 256 * 4)?,
                        _ => return Err("Invalid indexed image palette".into()),
                    });
                    1
                }
                _ => return Err("Unsupported image color space".into()),
            };
            if !matches!(channels, 1 | 3 | 4) {
                return Err("Unsupported image channel count".into());
            }
            let row = (w as usize * channels * depth).div_ceil(8);
            let expected = row
                .checked_mul(h as usize)
                .ok_or("Image dimensions overflow")?;
            let raw = decoded(stream, expected.saturating_add(1024))?;
            if raw.len() < expected {
                return Err("Image sample data is truncated".into());
            }
            let decode = stream
                .dict
                .get(b"Decode")
                .ok()
                .and_then(|object| object.as_array().ok());
            let mut rgba = Vec::with_capacity(count * 4);
            for y in 0..h as usize {
                for x in 0..w as usize {
                    let sample = |channel: usize| -> f32 {
                        let index = x * channels + channel;
                        let value = if depth == 8 {
                            raw[y * row + index] as f32 / 255.
                        } else {
                            ((raw[y * row + index / 8] >> (7 - index % 8)) & 1) as f32
                        };
                        if let Some(decode) = decode
                            && let (Some(min), Some(max)) = (
                                decode.get(channel * 2).and_then(number_value),
                                decode.get(channel * 2 + 1).and_then(number_value),
                            )
                        {
                            min + value * (max - min)
                        } else {
                            value
                        }
                    };
                    let color = if let Some(palette) = &palette {
                        let index = if depth == 8 {
                            raw[y * row + x] as usize
                        } else {
                            ((raw[y * row + x / 8] >> (7 - x % 8)) & 1) as usize
                        };
                        let p = palette
                            .get(index * 3..index * 3 + 3)
                            .ok_or("Image palette index out of bounds")?;
                        Rgba::rgb(p[0], p[1], p[2])
                    } else {
                        match channels {
                            1 => gray(sample(0)),
                            3 => rgb(sample(0), sample(1), sample(2)),
                            _ => cmyk(sample(0), sample(1), sample(2), sample(3)),
                        }
                    };
                    rgba.extend_from_slice(&color.to_array());
                }
            }
            if channels == 4 {
                self.warn(
                    "PDF CMYK image samples are converted to RGB without an output ICC profile.",
                );
            }
            Pixels::from_rgba(w, h, rgba).ok_or("Invalid decoded image")?
        };
        if let Ok(mask) = stream.dict.get(b"SMask") {
            let mask = resolve(self.pdf, mask)
                .and_then(|object| object.as_stream().ok())
                .ok_or("Image soft mask is not a stream")?;
            let mask = self.image(mask, depth + 1)?;
            for y in 0..h as usize {
                for x in 0..w as usize {
                    let mx = x * mask.w as usize / w as usize;
                    let my = y * mask.h as usize / h as usize;
                    pixels.data[(y * w as usize + x) * 4 + 3] =
                        mask.data[(my * mask.w as usize + mx) * 4];
                }
            }
            pixels.touch();
        }
        if stream.dict.get(b"Mask").is_ok() {
            self.warn("PDF image color-key and explicit image masks are not applied.");
        }
        Ok(pixels)
    }
}

fn text_advance(run: &TypeRun) -> f32 {
    run.contours
        .iter()
        .flatten()
        .map(|point| point.x - run.origin.x)
        .fold(0., f32::max)
        .max(run.content.chars().count() as f32 * run.px * 0.25)
}

fn text_style(state: &State) -> Style {
    let mut color = state.fill;
    color.a = byte(state.fill_alpha);
    let mut stroke = state.stroke.clone();
    stroke.color.a = byte(state.stroke_alpha);
    stroke.width *= state.matrix.scale();
    Style {
        fill: if matches!(state.text_mode, 1 | 5) {
            Fill::None
        } else {
            Fill::Solid(color)
        },
        stroke: matches!(state.text_mode, 1 | 2 | 5 | 6).then_some(stroke),
    }
}

fn placed_image(
    mut pixels: Pixels,
    matrix: Matrix,
    warnings: &mut BTreeSet<String>,
) -> Result<Layer, String> {
    let top_left = matrix.map(Pt::new(0., 1.));
    let dx = matrix.map(Pt::new(1., 1.)) - top_left;
    let dy = matrix.map(Pt::ZERO) - top_left;
    let size = Pt::new(dx.length(), dy.length());
    if !size.x.is_finite() || !size.y.is_finite() || size.x <= 0.0001 || size.y <= 0.0001 {
        return Err("PDF image transform is degenerate".into());
    }
    let rotation = dx.y.atan2(dx.x);
    if dx.dot(dy).abs() < size.x * size.y * 0.0001 {
        if dx.cross(dy) < 0. {
            let row = pixels.w as usize * 4;
            for y in 0..pixels.h as usize / 2 {
                let opposite = pixels.h as usize - y - 1;
                for i in 0..row {
                    pixels.data.swap(y * row + i, opposite * row + i);
                }
            }
            pixels.touch();
        }
        let center = top_left + (dx + dy) * 0.5;
        let origin = center - size * 0.5;
        let mut layer = Layer::placed_raster("Image", pixels, origin, size);
        layer.kind.set_raster_xform(origin, size, rotation);
        return Ok(layer);
    }
    warnings.insert("Sheared PDF images are resampled into editable raster layers.".into());
    let corners = [top_left, top_left + dx, top_left + dy, top_left + dx + dy];
    let min = corners.iter().copied().fold(corners[0], Pt::min);
    let max = corners.iter().copied().fold(corners[0], Pt::max);
    let bounds = max - min;
    let w = bounds.x.ceil().max(1.) as u32;
    let h = bounds.y.ceil().max(1.) as u32;
    if w as u64 * h as u64 > PIXEL_LIMIT as u64 {
        return Err("Sheared PDF image exceeds the raster limit".into());
    }
    let mut output =
        tiny_skia::Pixmap::new(w, h).ok_or("Could not allocate transformed PDF image")?;
    let input = pixels.to_pixmap().ok_or("Invalid PDF image")?;
    let transform = tiny_skia::Transform::from_row(
        dx.x / pixels.w as f32,
        dx.y / pixels.w as f32,
        dy.x / pixels.h as f32,
        dy.y / pixels.h as f32,
        top_left.x - min.x,
        top_left.y - min.y,
    );
    output.draw_pixmap(
        0,
        0,
        input.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        transform,
        None,
    );
    Ok(Layer::placed_raster(
        "Image",
        Pixels::from_pixmap(&output),
        min,
        Pt::new(w as f32, h as f32),
    ))
}
