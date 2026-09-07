use super::{blend_name, byte};
use crate::color::Blend;
use crate::document::{Artboard, Cap, Document, Fill, Join, Layer, LayerKind, Shape};
use crate::geom::{Bounds, Geom, Pt};
use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document as Pdf, Object, ObjectId, Stream, dictionary};
use std::collections::{BTreeSet, HashMap};

mod fallback;

/// Write a PDF 1.7 document with one page per artboard and named optional-content
/// layers. Diagnostics describe document features that PDF export approximates.
pub fn write(document: &Document) -> Result<(Vec<u8>, Vec<String>), String> {
    let (prepared, warnings) = fallback::prepare(document)?;
    let document = prepared.as_ref();
    let mut writer = Writer {
        pdf: Pdf::with_version("1.7"),
        warnings,
        states: HashMap::new(),
        images: HashMap::new(),
    };
    let page_tree = writer.pdf.new_object_id();
    let mut ocgs = vec![];
    let mut off = vec![];
    let mut locked = vec![];
    let mut properties = Dictionary::new();
    let mut layer_ids = HashMap::new();
    for layer in &document.layers {
        let id = writer
            .pdf
            .add_object(dictionary! { "Type" => "OCG", "Name" => lopdf::text_string(&layer.name) });
        ocgs.push(Object::Reference(id));
        if !layer.visible {
            off.push(Object::Reference(id));
        }
        if layer.locked {
            locked.push(Object::Reference(id));
        }
        layer_ids.insert(layer.id, id);
        properties.set(format!("L{}", layer.id), id);
    }
    let mut pages = vec![];
    let fallback = vec![Artboard::new(
        0,
        Pt::ZERO,
        Pt::new(document.width, document.height),
    )];
    let boards = if document.artboards.is_empty() {
        &fallback
    } else {
        &document.artboards
    };
    for board in boards {
        let width = board.size.x;
        let height = board.size.y;
        if !width.is_finite() || !height.is_finite() || width <= 0. || height <= 0. {
            return Err("PDF export requires positive, finite artboard dimensions.".into());
        }
        if board.rotation.abs() > 0.0001 {
            writer
                .warnings
                .insert("Rotated artboards export their unrotated page rectangle.".into());
        }
        let bounds = Bounds::from_min_size(board.origin, board.size);
        let mut operations = vec![];
        op(&mut operations, "q", &[]);
        op(
            &mut operations,
            "cm",
            &[1., 0., 0., -1., -board.origin.x, height + board.origin.y],
        );
        // PDF's page box clips the output; do not add a redundant clipping path.
        let mut xobjects = Dictionary::new();
        let mut states = Dictionary::new();
        for layer in &document.layers {
            if layer.is_group {
                continue;
            }
            let ancestors = ancestors(document, layer);
            let mut opacity = layer.opacity;
            for parent in &ancestors {
                opacity *= parent.opacity;
            }
            for parent in ancestors.iter().rev() {
                mark(&mut operations, parent.id);
            }
            mark(&mut operations, layer.id);
            op(&mut operations, "q", &[]);
            match &layer.kind {
                LayerKind::Vector { shapes } => {
                    for shape in shapes {
                        if !shape.visible
                            || shape.guide
                            || !bounds.intersects(
                                shape.world_bbox().inflate(
                                    shape
                                        .style
                                        .stroke
                                        .as_ref()
                                        .map(|stroke| stroke.width)
                                        .unwrap_or(0.),
                                ),
                            )
                        {
                            continue;
                        }
                        if matches!(shape.geom, Geom::Text(_)) {
                            writer.warnings.insert("PDF text exports as vector outlines, preserving appearance but not live text editing.".into());
                        }
                        let fill = match &shape.style.fill {
                            Fill::None => None,
                            Fill::Solid(color) => Some(*color),
                            Fill::Linear { .. } | Fill::Radial { .. } => return Err(
                                "PDF gradient was not prepared for appearance-preserving export"
                                    .into(),
                            ),
                        };
                        if fill.is_none() && shape.style.stroke.is_none() {
                            continue;
                        }
                        let alpha = opacity * shape.opacity;
                        let ca =
                            byte(alpha * fill.map(|color| color.a as f32 / 255.).unwrap_or(1.));
                        let stroke_alpha = byte(
                            alpha
                                * shape
                                    .style
                                    .stroke
                                    .as_ref()
                                    .map(|stroke| stroke.color.a as f32 / 255.)
                                    .unwrap_or(1.),
                        );
                        let state = writer.state(ca, stroke_alpha, layer.blend);
                        let state_name = format!("G{}", state.0);
                        states.set(state_name.clone(), state);
                        operations.push(Operation::new(
                            "gs",
                            vec![Object::Name(state_name.into_bytes())],
                        ));
                        if let Some(fill) = fill {
                            op(
                                &mut operations,
                                "rg",
                                &[
                                    fill.r as f32 / 255.,
                                    fill.g as f32 / 255.,
                                    fill.b as f32 / 255.,
                                ],
                            );
                        }
                        if let Some(stroke) = &shape.style.stroke {
                            let color = stroke.color;
                            op(
                                &mut operations,
                                "RG",
                                &[
                                    color.r as f32 / 255.,
                                    color.g as f32 / 255.,
                                    color.b as f32 / 255.,
                                ],
                            );
                            op(&mut operations, "w", &[stroke.width.max(0.)]);
                            op(
                                &mut operations,
                                "J",
                                &[match stroke.cap {
                                    Cap::Butt => 0.,
                                    Cap::Round => 1.,
                                    Cap::Square => 2.,
                                }],
                            );
                            op(
                                &mut operations,
                                "j",
                                &[match stroke.join {
                                    Join::Miter => 0.,
                                    Join::Round => 1.,
                                    Join::Bevel => 2.,
                                }],
                            );
                            let dashes = stroke
                                .dash
                                .map(|(dash, gap)| vec![Object::Real(dash), Object::Real(gap)])
                                .unwrap_or_default();
                            operations
                                .push(Operation::new("d", vec![Object::Array(dashes), 0.into()]));
                        }
                        path(&mut operations, shape);
                        let winding = !matches!(shape.geom, Geom::Poly { winding: false, .. });
                        let paint = match (fill.is_some(), shape.style.stroke.is_some(), winding) {
                            (true, true, true) => "B",
                            (true, true, false) => "B*",
                            (true, false, true) => "f",
                            (true, false, false) => "f*",
                            _ => "S",
                        };
                        op(&mut operations, paint, &[]);
                    }
                }
                LayerKind::Raster { pixels, .. } => {
                    if pixels.is_invisible()
                        || !bounds.intersects(layer.kind.raster_bounds().unwrap_or(bounds))
                    {
                        // Keep the OCG even when no artwork intersects this page.
                    } else {
                        let image = writer.image(layer)?;
                        let name = format!("I{}", image.0);
                        xobjects.set(name.clone(), image);
                        let state = writer.state(byte(opacity), byte(opacity), layer.blend);
                        let state_name = format!("G{}", state.0);
                        states.set(state_name.clone(), state);
                        operations.push(Operation::new(
                            "gs",
                            vec![Object::Name(state_name.into_bytes())],
                        ));
                        let t = crate::compositor::layer_pixel_transform(layer);
                        let w = pixels.w as f32;
                        let h = pixels.h as f32;
                        op(
                            &mut operations,
                            "cm",
                            &[
                                t.sx * w,
                                t.ky * w,
                                -t.kx * h,
                                -t.sy * h,
                                t.tx + t.kx * h,
                                t.ty + t.sy * h,
                            ],
                        );
                        operations
                            .push(Operation::new("Do", vec![Object::Name(name.into_bytes())]));
                    }
                }
            }
            op(&mut operations, "Q", &[]);
            op(&mut operations, "EMC", &[]);
            for _ in ancestors {
                op(&mut operations, "EMC", &[]);
            }
        }
        op(&mut operations, "Q", &[]);
        let encoded = Content { operations }
            .encode()
            .map_err(|error| error.to_string())?;
        let content = writer
            .pdf
            .add_object(Stream::new(Dictionary::new(), encoded));
        let resources = dictionary! { "Properties" => properties.clone(), "XObject" => xobjects, "ExtGState" => states };
        let page = writer.pdf.add_object(dictionary! {
            "Type" => "Page", "Parent" => page_tree,
            "MediaBox" => vec![Object::Integer(0),Object::Integer(0),Object::Real(width),Object::Real(height)],
            "Resources" => resources, "Contents" => content,
        });
        pages.push(Object::Reference(page));
    }
    writer.pdf.objects.insert(
        page_tree,
        Object::Dictionary(
            dictionary! { "Type"=>"Pages", "Count"=>pages.len() as i64, "Kids"=>pages },
        ),
    );
    let order = order(document, &layer_ids, None, 0);
    let catalog = writer.pdf.add_object(dictionary! {
        "Type"=>"Catalog", "Pages"=>page_tree,
        "OCProperties"=> dictionary! { "OCGs"=>ocgs, "D"=>dictionary! { "Name"=>lopdf::text_string("Omadesign layers"), "BaseState"=>"ON", "OFF"=>off, "Locked"=>locked, "Order"=>order } },
    });
    let info = writer.pdf.add_object(dictionary! { "Title"=>lopdf::text_string(&document.name), "Producer"=>lopdf::text_string("Omadesign") });
    writer.pdf.trailer.set("Root", catalog);
    writer.pdf.trailer.set("Info", info);
    writer.pdf.compress();
    let mut bytes = Vec::new();
    writer
        .pdf
        .save_to(&mut bytes)
        .map_err(|error| format!("Could not encode PDF: {error}"))?;
    Ok((bytes, writer.warnings.into_iter().collect()))
}

struct Writer {
    pdf: Pdf,
    warnings: BTreeSet<String>,
    states: HashMap<(u8, u8, String), ObjectId>,
    images: HashMap<u64, ObjectId>,
}

impl Writer {
    fn state(&mut self, fill: u8, stroke: u8, blend: Blend) -> ObjectId {
        let key = (fill, stroke, blend_name(blend).to_string());
        if let Some(id) = self.states.get(&key) {
            return *id;
        }
        let id = self.pdf.add_object(dictionary! {"Type"=>"ExtGState","ca"=>fill as f32/255.,"CA"=>stroke as f32/255.,"BM"=>Object::Name(blend_name(blend).as_bytes().to_vec())});
        self.states.insert(key, id);
        id
    }

    fn image(&mut self, layer: &Layer) -> Result<ObjectId, String> {
        if let Some(id) = self.images.get(&layer.id) {
            return Ok(*id);
        }
        let pixels = layer.kind.pixels().ok_or("Expected a raster layer")?;
        if pixels.w == 0
            || pixels.h == 0
            || pixels.data.len() != pixels.w as usize * pixels.h as usize * 4
        {
            return Err("Invalid raster buffer during PDF export".into());
        }
        let mut rgb = Vec::with_capacity(pixels.data.len() / 4 * 3);
        let mut alpha = Vec::with_capacity(pixels.data.len() / 4);
        for pixel in pixels.data.as_chunks::<4>().0 {
            rgb.extend_from_slice(&pixel[..3]);
            alpha.push(pixel[3]);
        }
        let mut dict = dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>pixels.w,"Height"=>pixels.h,"ColorSpace"=>"DeviceRGB","BitsPerComponent"=>8};
        if alpha.iter().any(|alpha| *alpha != 255) {
            let mask = self.pdf.add_object(Stream::new(dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>pixels.w,"Height"=>pixels.h,"ColorSpace"=>"DeviceGray","BitsPerComponent"=>8},alpha));
            dict.set("SMask", mask);
        }
        let id = self.pdf.add_object(Stream::new(dict, rgb));
        self.images.insert(layer.id, id);
        Ok(id)
    }
}

fn op(operations: &mut Vec<Operation>, operator: &str, operands: &[f32]) {
    operations.push(Operation::new(
        operator,
        operands.iter().map(|value| Object::Real(*value)).collect(),
    ));
}

fn mark(operations: &mut Vec<Operation>, id: u64) {
    operations.push(Operation::new(
        "BDC",
        vec![
            Object::Name(b"OC".to_vec()),
            Object::Name(format!("L{id}").into_bytes()),
        ],
    ));
}

fn ancestors<'a>(document: &'a Document, layer: &Layer) -> Vec<&'a Layer> {
    let mut out = Vec::new();
    let mut parent = layer.parent;
    for _ in 0..64 {
        let Some(found) = parent.and_then(|id| {
            document
                .layers
                .iter()
                .find(|layer| layer.id == id && layer.is_group)
        }) else {
            break;
        };
        if out.iter().any(|layer: &&Layer| layer.id == found.id) {
            break;
        }
        out.push(found);
        parent = found.parent;
    }
    out
}

fn order(
    document: &Document,
    ids: &HashMap<u64, ObjectId>,
    parent: Option<u64>,
    depth: usize,
) -> Vec<Object> {
    if depth > 64 {
        return vec![];
    }
    let mut out = Vec::new();
    for layer in document
        .layers
        .iter()
        .rev()
        .filter(|layer| layer.parent == parent)
    {
        if let Some(id) = ids.get(&layer.id) {
            out.push(Object::Reference(*id));
            if layer.is_group {
                out.push(Object::Array(order(
                    document,
                    ids,
                    Some(layer.id),
                    depth + 1,
                )));
            }
        }
    }
    out
}

fn path(operations: &mut Vec<Operation>, shape: &Shape) {
    let center = shape.geom.bbox().center();
    let rotate = |point: Pt| point.rotate_about(center, shape.rotation);
    if let Geom::Path { anchors, closed } = &shape.geom {
        let Some(first) = anchors.first() else {
            return;
        };
        let first_pt = rotate(first.pt);
        op(operations, "m", &[first_pt.x, first_pt.y]);
        for pair in anchors.windows(2) {
            let a = &pair[0];
            let b = &pair[1];
            let to = rotate(b.pt);
            if a.h_out == Pt::ZERO && b.h_in == Pt::ZERO {
                op(operations, "l", &[to.x, to.y]);
            } else {
                let c1 = rotate(a.pt + a.h_out);
                let c2 = rotate(b.pt + b.h_in);
                op(operations, "c", &[c1.x, c1.y, c2.x, c2.y, to.x, to.y]);
            }
        }
        if *closed {
            if let Some(last) = anchors.last()
                && (last.h_out != Pt::ZERO || first.h_in != Pt::ZERO)
            {
                let c1 = rotate(last.pt + last.h_out);
                let c2 = rotate(first.pt + first.h_in);
                op(
                    operations,
                    "c",
                    &[c1.x, c1.y, c2.x, c2.y, first_pt.x, first_pt.y],
                );
            }
            op(operations, "h", &[]);
        }
    } else {
        for contour in shape.world_contours(96) {
            let Some(first) = contour.first() else {
                continue;
            };
            op(operations, "m", &[first.x, first.y]);
            for point in contour.iter().skip(1) {
                op(operations, "l", &[point.x, point.y]);
            }
            if shape.geom.is_closed() {
                op(operations, "h", &[]);
            }
        }
    }
}
