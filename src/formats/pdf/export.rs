use super::{blend_name, byte};
use crate::artwork_mask::{ArtworkMask, MaskMode, MaskSource};
use crate::color::{Blend, Rgba};
use crate::document::{Artboard, Cap, Document, Fill, Join, Layer, LayerKind, Pixels, Shape};
use crate::geom::{Bounds, Geom, Pt};
use crate::gradient::{Gradient, GradientKind, GradientStop};
use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document as Pdf, Object, ObjectId, Stream, dictionary};
use std::collections::{BTreeSet, HashMap};

mod fallback;
mod text;

/// Write editable PDF artwork.
/// Args: the source document. Returns PDF 1.7 bytes and any appearance fallback notes.
pub fn write(document: &Document) -> Result<(Vec<u8>, Vec<String>), String> {
    document.validate_hierarchy()?;
    let (prepared, warnings) = fallback::prepare(document)?;
    let document = prepared.as_ref();
    let mut writer = Writer {
        pdf: Pdf::with_version("1.7"),
        warnings,
        states: HashMap::new(),
        images: HashMap::new(),
        properties: Dictionary::new(),
        fonts: text::Fonts::default(),
    };
    let page_tree = writer.pdf.new_object_id();
    let mut ocgs = vec![];
    let mut off = vec![];
    let mut locked = vec![];
    let mut layer_ids = HashMap::new();
    let sources = document.mask_sources();
    for (index, layer) in document.layers.iter().enumerate() {
        if document.mask_source_member(index, None, &sources) {
            continue;
        }
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
        writer.properties.set(format!("L{}", layer.id), id);
    }
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
    let mut pages = vec![];
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
        let mut scene = Scene::default();
        op(&mut scene.ops, "q", &[]);
        op(
            &mut scene.ops,
            "cm",
            &[1., 0., 0., -1., -board.origin.x, height + board.origin.y],
        );
        writer.children(document, None, bounds, &mut scene, 0, false)?;
        op(&mut scene.ops, "Q", &[]);
        let encoded = scene.encode()?;
        let content = writer
            .pdf
            .add_object(Stream::new(Dictionary::new(), encoded));
        let page = writer.pdf.add_object(dictionary! {
            "Type" => "Page", "Parent" => page_tree,
            "MediaBox" => vec![0.into(),0.into(),Object::Real(width),Object::Real(height)],
            "Resources" => scene.resources, "Contents" => content,
            "Group" => dictionary! {"S"=>"Transparency", "CS"=>"DeviceRGB", "I"=>true},
        });
        pages.push(Object::Reference(page));
    }
    writer.pdf.objects.insert(
        page_tree,
        Object::Dictionary(
            dictionary! {"Type"=>"Pages", "Count"=>pages.len() as i64, "Kids"=>pages},
        ),
    );
    let catalog = writer.pdf.add_object(dictionary! {
        "Type"=>"Catalog", "Pages"=>page_tree,
        "OCProperties"=> dictionary! {"OCGs"=>ocgs, "D"=>dictionary! {"Name"=>lopdf::text_string("Omadesign layers"), "BaseState"=>"ON", "OFF"=>off, "Locked"=>locked, "Order"=>order(document, &layer_ids, None, 0)}},
    });
    let info = writer.pdf.add_object(dictionary! {"Title"=>lopdf::text_string(&document.name), "Producer"=>lopdf::text_string("Omadesign")});
    writer.pdf.trailer.set("Root", catalog);
    writer.pdf.trailer.set("Info", info);
    writer.fonts.finish(&mut writer.pdf)?;
    writer.pdf.compress();
    let mut bytes = Vec::new();
    writer
        .pdf
        .save_to(&mut bytes)
        .map_err(|e| format!("Could not encode PDF: {e}"))?;
    Ok((bytes, writer.warnings.into_iter().collect()))
}

#[derive(Default)]
struct Scene {
    ops: Vec<Operation>,
    resources: Dictionary,
}
impl Scene {
    /// Register a resource in this content stream.
    /// Args: category and object ID. Returns its local resource name.
    fn resource(&mut self, category: &str, id: ObjectId) -> String {
        let name = format!("R{}", id.0);
        if self.resources.get(category.as_bytes()).is_err() {
            self.resources.set(category, Dictionary::new());
        }
        if let Ok(Object::Dictionary(dict)) = self.resources.get_mut(category.as_bytes()) {
            dict.set(name.clone(), id);
        }
        name
    }
    /// Invoke a named resource.
    /// Args: operator, category and object ID. Appends the matching PDF operation.
    fn invoke(&mut self, operator: &str, category: &str, id: ObjectId) {
        let name = self.resource(category, id);
        self.ops.push(Operation::new(
            operator,
            vec![Object::Name(name.into_bytes())],
        ));
    }
    /// Encode the current content stream.
    /// Returns PDF drawing bytes or an encoding error.
    fn encode(&self) -> Result<Vec<u8>, String> {
        Content {
            operations: self.ops.clone(),
        }
        .encode()
        .map_err(|e| e.to_string())
    }
}

struct Writer {
    pdf: Pdf,
    warnings: BTreeSet<String>,
    states: HashMap<(u8, u8, String), ObjectId>,
    images: HashMap<u64, ObjectId>,
    properties: Dictionary,
    fonts: text::Fonts,
}
impl Writer {
    /// Encode a bounded transparency form.
    /// Args: content, world bounds and isolation. Returns a reusable vector Form XObject.
    fn form(&mut self, scene: Scene, bounds: Bounds, isolated: bool) -> Result<ObjectId, String> {
        let bytes = scene.encode()?;
        let mut group = dictionary! {"S"=>"Transparency","I"=>isolated,"K"=>false};
        if isolated {
            group.set("CS", "DeviceRGB");
        }
        Ok(self.pdf.add_object(Stream::new(dictionary! {
            "Type"=>"XObject", "Subtype"=>"Form", "FormType"=>1,
            "BBox"=>vec![Object::Real(bounds.min.x),Object::Real(bounds.min.y),Object::Real(bounds.max.x),Object::Real(bounds.max.y)],
            "Resources"=>scene.resources, "Group"=>group,
        }, bytes)))
    }
    /// Paint immediate layer children in native painter order.
    /// Args: document, parent, bounds, output, depth and mask mode. Returns an error for excessive nesting.
    fn children(
        &mut self,
        doc: &Document,
        parent: Option<u64>,
        bounds: Bounds,
        out: &mut Scene,
        depth: usize,
        mask: bool,
    ) -> Result<(), String> {
        if depth > 64 {
            return Err("PDF layer or mask nesting exceeds 64".into());
        }
        for (_, layer) in doc
            .layers
            .iter()
            .enumerate()
            .filter(|(_, l)| l.parent == parent)
        {
            if doc.mask_sources().contains(&MaskSource {
                layer: layer.id,
                shape: None,
            }) {
                continue;
            }
            self.layer(doc, layer, bounds, out, depth + 1, mask)?;
        }
        Ok(())
    }
    /// Paint one layer or mask source.
    /// Args: document, layer, bounds, output, depth and mask mode. Returns any export error.
    fn layer(
        &mut self,
        doc: &Document,
        layer: &Layer,
        bounds: Bounds,
        out: &mut Scene,
        depth: usize,
        mask: bool,
    ) -> Result<(), String> {
        if depth > 64 {
            return Err("PDF layer or mask nesting exceeds 64".into());
        }
        if mask && !layer.visible {
            return Ok(());
        }
        let mut scene = Scene::default();
        if !mask {
            scene.resources.set("Properties", self.properties.clone());
            mark(&mut scene.ops, layer.id);
        }
        if layer.is_group {
            self.children(doc, Some(layer.id), bounds, &mut scene, depth + 1, mask)?;
        } else {
            match &layer.kind {
                LayerKind::Vector { shapes } => {
                    for shape in shapes {
                        if !shape.visible
                            || shape.guide
                            || doc.mask_sources().contains(&MaskSource {
                                layer: layer.id,
                                shape: Some(shape.id),
                            })
                        {
                            continue;
                        }
                        if shape.opacity >= 1.
                            && shape.fill_opacity >= 1.
                            && shape.blend == Blend::Normal
                            && shape.mask.is_none()
                            && shape.artwork_masks.is_empty()
                        {
                            op(&mut scene.ops, "q", &[]);
                            self.shape(shape, bounds, &mut scene)?;
                            op(&mut scene.ops, "Q", &[]);
                            continue;
                        }
                        let mut object = Scene::default();
                        self.shape(shape, bounds, &mut object)?;
                        let id = self.form(object, bounds, true)?;
                        let id = self.masks(
                            doc,
                            id,
                            &shape.artwork_masks,
                            shape
                                .mask
                                .as_ref()
                                .map(|p| (p, crate::compositor::shape_mask_transform(shape))),
                            bounds,
                            depth + 1,
                        )?;
                        op(&mut scene.ops, "q", &[]);
                        let state = self.state(
                            byte(shape.opacity * shape.fill_opacity),
                            byte(shape.opacity * shape.fill_opacity),
                            shape.blend,
                        );
                        scene.invoke("gs", "ExtGState", state);
                        scene.invoke("Do", "XObject", id);
                        op(&mut scene.ops, "Q", &[]);
                    }
                }
                LayerKind::Raster { pixels, .. } => {
                    let image = self.image(layer)?;
                    let t = crate::compositor::layer_pixel_transform(layer);
                    image_matrix(&mut scene.ops, t, pixels.w, pixels.h);
                    scene.invoke("Do", "XObject", image);
                }
            }
        }
        if !mask {
            op(&mut scene.ops, "EMC", &[]);
        }
        if (!layer.is_group
            && layer
                .kind
                .shapes()
                .is_none_or(|shapes| shapes.iter().all(|s| s.blend == Blend::Normal))
            || layer.is_group && layer.pass_through)
            && layer.opacity >= 1.
            && layer.fill_opacity >= 1.
            && layer.mask.is_none()
            && layer.artwork_masks.is_empty()
            && layer.blend == Blend::Normal
        {
            op(&mut out.ops, "q", &[]);
            out.ops.extend(scene.ops);
            op(&mut out.ops, "Q", &[]);
            merge_resources(&mut out.resources, scene.resources);
            return Ok(());
        }
        let isolation = !layer.is_group || !layer.pass_through || !layer.artwork_masks.is_empty();
        let id = self.form(scene, bounds, isolation)?;
        let id = self.masks(
            doc,
            id,
            &layer.artwork_masks,
            layer
                .mask
                .as_ref()
                .map(|p| (p, crate::compositor::layer_pixel_transform(layer))),
            bounds,
            depth + 1,
        )?;
        op(&mut out.ops, "q", &[]);
        let state = self.state(
            byte(layer.opacity * layer.fill_opacity),
            byte(layer.opacity * layer.fill_opacity),
            layer.blend,
        );
        out.invoke("gs", "ExtGState", state);
        out.invoke("Do", "XObject", id);
        op(&mut out.ops, "Q", &[]);
        Ok(())
    }
    /// Paint fill and stroke independently before object opacity is applied.
    /// Args: object, page bounds and output. Returns an error for invalid gradient data.
    fn shape(&mut self, shape: &Shape, bounds: Bounds, out: &mut Scene) -> Result<(), String> {
        if !shape.rotation.is_finite()
            || !shape.opacity.is_finite()
            || !shape.fill_opacity.is_finite()
        {
            return Err("PDF object opacity and rotation must be finite".into());
        }
        if shape.style.stroke.as_ref().is_some_and(|s| {
            !s.width.is_finite()
                || !s.dash_offset.is_finite()
                || s.dash
                    .is_some_and(|(a, b)| !a.is_finite() || !b.is_finite())
        }) {
            return Err("PDF stroke geometry must be finite".into());
        }
        if let Geom::Text(run) = &shape.geom {
            text::validate(run)?;
            let mut scene = Scene::default();
            match text::write(&mut self.pdf, &mut self.fonts, shape, &mut scene) {
                Ok(()) => {
                    out.ops.extend(scene.ops);
                    merge_resources(&mut out.resources, scene.resources);
                    return Ok(());
                }
                Err(reason) => {
                    self.warnings
                        .insert(format!("PDF text uses vector outlines because {reason}."));
                }
            }
        }
        if !shape.style.fill.is_none() {
            op(&mut out.ops, "q", &[]);
            self.paint(shape, false, bounds, out)?;
            op(&mut out.ops, "Q", &[]);
        }
        if let Some(stroke) = &shape.style.stroke {
            if stroke.width > 0. {
                op(&mut out.ops, "q", &[]);
                self.paint(shape, true, bounds, out)?;
                op(&mut out.ops, "Q", &[]);
            }
        }
        Ok(())
    }
    /// Paint one fill or stroke as vector content.
    /// Args: object, stroke selection, bounds and output. Returns any paint encoding error.
    fn paint(
        &mut self,
        shape: &Shape,
        stroke: bool,
        bounds: Bounds,
        out: &mut Scene,
    ) -> Result<(), String> {
        let gradient = if stroke {
            shape.style.stroke.as_ref().and_then(|s| s.gradient.clone())
        } else {
            shape.style.fill.gradient()
        };
        let solid = if stroke {
            shape.style.stroke.as_ref().map(|s| s.color)
        } else {
            match shape.style.fill {
                Fill::Solid(c) => Some(c),
                _ => None,
            }
        };
        let alpha = solid.map_or(1., |c| c.a as f32 / 255.);
        let mut scene = Scene::default();
        let state = self.state(byte(alpha), byte(alpha), Blend::Normal);
        scene.invoke("gs", "ExtGState", state);
        if stroke {
            stroke_style(&mut scene.ops, shape);
        }
        if let Some(gradient) = gradient {
            let mut gradient = gradient;
            gradient.normalize();
            if stroke {
                let s = shape.style.stroke.as_ref().ok_or("PDF stroke missing")?;
                let mut style = tiny_skia::Stroke {
                    width: s.width,
                    line_cap: s.cap.to_skia(),
                    line_join: s.join.to_skia(),
                    ..Default::default()
                };
                style.dash = s
                    .dash
                    .and_then(|(a, b)| tiny_skia::StrokeDash::new(vec![a, b], s.dash_offset));
                if let Some(path) = shape.get_cached_path(96).and_then(|p| p.stroke(&style, 1.)) {
                    skia_path(&mut scene.ops, &path);
                }
            } else {
                path(&mut scene.ops, shape);
            }
            let winding = stroke
                || !matches!(
                    shape.geom,
                    Geom::Poly { winding: false, .. } | Geom::Paths { winding: false, .. }
                );
            op(&mut scene.ops, if winding { "W" } else { "W*" }, &[]);
            op(&mut scene.ops, "n", &[]);
            rotate_matrix(&mut scene.ops, shape.geom.bbox().center(), shape.rotation);
            let shading = self.shading(&gradient, shape.geom.bbox(), false)?;
            scene.invoke("sh", "Shading", shading);
            let id = self.form(scene, bounds, true)?;
            let id = if gradient.stops.iter().any(|s| s.color.a != 255) {
                let mut alpha = Scene::default();
                rotate_matrix(&mut alpha.ops, shape.geom.bbox().center(), shape.rotation);
                let shading = self.shading(&gradient, shape.geom.bbox(), true)?;
                alpha.invoke("sh", "Shading", shading);
                let mask = self.form(alpha, bounds, true)?;
                self.masked(id, mask, MaskMode::Luminance, bounds)?
            } else {
                id
            };
            out.invoke("Do", "XObject", id);
        } else if let Some(color) = solid {
            op(
                &mut scene.ops,
                if stroke { "RG" } else { "rg" },
                &[
                    color.r as f32 / 255.,
                    color.g as f32 / 255.,
                    color.b as f32 / 255.,
                ],
            );
            path(&mut scene.ops, shape);
            op(
                &mut scene.ops,
                if stroke {
                    "S"
                } else if matches!(
                    shape.geom,
                    Geom::Poly { winding: false, .. } | Geom::Paths { winding: false, .. }
                ) {
                    "f*"
                } else {
                    "f"
                },
                &[],
            );
            out.ops.extend(scene.ops);
            merge_resources(&mut out.resources, scene.resources);
        }
        Ok(())
    }
    /// Export linear or radial vector shading with independent color and alpha channels.
    /// Args: gradient, unrotated geometry bounds and alpha-channel selection. Returns a shading resource.
    fn shading(
        &mut self,
        gradient: &Gradient,
        bounds: Bounds,
        alpha: bool,
    ) -> Result<ObjectId, String> {
        let (a, b) = gradient.endpoints(bounds);
        let radial = gradient.kind == GradientKind::Radial;
        if !matches!(gradient.kind, GradientKind::Linear | GradientKind::Radial) {
            return Err("Unsupported PDF gradient reached vector writer".into());
        }
        let coords = if radial {
            vec![a.x, a.y, 0., a.x, a.y, (b - a).length().max(1e-6)]
        } else {
            vec![a.x, a.y, b.x, b.y]
        };
        if coords.iter().any(|v| !v.is_finite()) {
            return Err("PDF gradient coordinates must be finite".into());
        }
        let mut stops = gradient.stops.clone();
        if stops[0].offset > 0. {
            stops.insert(
                0,
                GradientStop {
                    offset: 0.,
                    color: stops[0].color,
                },
            );
        }
        let last = stops.last().cloned().ok_or("PDF gradient has no stops")?;
        if last.offset < 1. {
            stops.push(GradientStop {
                offset: 1.,
                color: last.color,
            });
        }
        let components = |c: Rgba| {
            if alpha {
                vec![Object::Real(c.a as f32 / 255.)]
            } else {
                vec![
                    Object::Real(c.r as f32 / 255.),
                    Object::Real(c.g as f32 / 255.),
                    Object::Real(c.b as f32 / 255.),
                ]
            }
        };
        let mut functions = vec![];
        let mut cuts = vec![];
        for pair in stops.windows(2) {
            if pair[1].offset <= pair[0].offset {
                continue;
            }
            if !functions.is_empty() {
                cuts.push(Object::Real(pair[0].offset));
            }
            functions.push(Object::Reference(self.pdf.add_object(dictionary!{"FunctionType"=>2,"Domain"=>vec![0.into(),1.into()],"C0"=>components(pair[0].color),"C1"=>components(pair[1].color),"N"=>1})));
        }
        let encode = functions
            .iter()
            .flat_map(|_| [0.into(), 1.into()])
            .collect::<Vec<Object>>();
        let function=self.pdf.add_object(dictionary!{"FunctionType"=>3,"Domain"=>vec![0.into(),1.into()],"Functions"=>functions,"Bounds"=>cuts,"Encode"=>encode});
        Ok(self.pdf.add_object(dictionary!{"ShadingType"=>if radial{3}else{2},"ColorSpace"=>if alpha{"DeviceGray"}else{"DeviceRGB"},"Coords"=>coords.into_iter().map(Object::Real).collect::<Vec<_>>(),"Function"=>function,"Extend"=>vec![Object::Boolean(true),Object::Boolean(true)]}))
    }
    /// Apply pixel and live vector masks without rasterizing the painted artwork.
    /// Args: source form, mask references, optional placed pixel mask, bounds and depth. Returns the nested masked form.
    fn masks(
        &mut self,
        doc: &Document,
        mut id: ObjectId,
        masks: &[ArtworkMask],
        pixels: Option<(&Pixels, tiny_skia::Transform)>,
        bounds: Bounds,
        depth: usize,
    ) -> Result<ObjectId, String> {
        if depth > 64 {
            return Err("PDF mask nesting exceeds 64".into());
        }
        if let Some((pixels, transform)) = pixels {
            validate_pixels(pixels)?;
            let gray = pixels
                .data
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| {
                    ((0.2126 * c[0] as f32 + 0.7152 * c[1] as f32 + 0.0722 * c[2] as f32)
                        * c[3] as f32
                        / 255.)
                        .round() as u8
                })
                .collect();
            let image=self.pdf.add_object(Stream::new(dictionary!{"Type"=>"XObject","Subtype"=>"Image","Width"=>pixels.w,"Height"=>pixels.h,"ColorSpace"=>"DeviceGray","BitsPerComponent"=>8},gray));
            let mut scene = Scene::default();
            image_matrix(&mut scene.ops, transform, pixels.w, pixels.h);
            scene.invoke("Do", "XObject", image);
            let mask = self.form(scene, bounds, true)?;
            id = self.masked(id, mask, MaskMode::Luminance, bounds)?;
        }
        for mask in masks {
            let mut scene = Scene::default();
            matrix(&mut scene.ops, mask.placement());
            if let Some(region) = mask.region {
                let t = mask.region_placement();
                let points = [
                    region.min,
                    Pt::new(region.max.x, region.min.y),
                    region.max,
                    Pt::new(region.min.x, region.max.y),
                ];
                for (i, p) in points.iter().enumerate() {
                    op(
                        &mut scene.ops,
                        if i == 0 { "m" } else { "l" },
                        &[
                            t.sx * p.x + t.kx * p.y + t.tx,
                            t.ky * p.x + t.sy * p.y + t.ty,
                        ],
                    );
                }
                op(&mut scene.ops, "h", &[]);
                op(&mut scene.ops, "W", &[]);
                op(&mut scene.ops, "n", &[]);
            }
            if let Some((index, shape)) = mask.source.resolve(doc) {
                let source = &doc.layers[index];
                let source_bounds = doc
                    .layers
                    .iter()
                    .filter_map(fallback::layer_bounds)
                    .fold(bounds, |a, b| a.union(b));
                if let Some(shape) = shape
                    .and_then(|id| source.find(id))
                    .filter(|s| s.visible && !s.guide)
                {
                    let mut content = Scene::default();
                    self.shape(shape, source_bounds, &mut content)?;
                    let form = self.form(content, source_bounds, true)?;
                    let form = self.masks(
                        doc,
                        form,
                        &shape.artwork_masks,
                        shape
                            .mask
                            .as_ref()
                            .map(|p| (p, crate::compositor::shape_mask_transform(shape))),
                        source_bounds,
                        depth + 1,
                    )?;
                    let state = self.state(
                        byte(shape.opacity * shape.fill_opacity),
                        byte(shape.opacity * shape.fill_opacity),
                        shape.blend,
                    );
                    scene.invoke("gs", "ExtGState", state);
                    scene.invoke("Do", "XObject", form);
                } else if mask.source.shape.is_none() {
                    self.layer(doc, source, source_bounds, &mut scene, depth + 1, true)?;
                }
            }
            let form = self.form(scene, bounds, true)?;
            id = self.masked(id, form, mask.mode, bounds)?;
        }
        Ok(id)
    }
    /// Wrap artwork in one soft mask while preserving isolation.
    /// Args: artwork form, mask form, mask mode and bounds. Returns the masked form.
    fn masked(
        &mut self,
        id: ObjectId,
        mask: ObjectId,
        mode: MaskMode,
        bounds: Bounds,
    ) -> Result<ObjectId, String> {
        let isolated = self
            .pdf
            .get_object(id)
            .ok()
            .and_then(|o| o.as_stream().ok())
            .and_then(|s| s.dict.get(b"Group").ok())
            .and_then(|o| o.as_dict().ok())
            .and_then(|d| d.get(b"I").ok())
            .and_then(|o| o.as_bool().ok())
            .unwrap_or(true);
        let state=self.pdf.add_object(dictionary!{"Type"=>"ExtGState","SMask"=>dictionary!{"S"=>if mode==MaskMode::Alpha{"Alpha"}else{"Luminosity"},"G"=>mask,"BC"=>vec![0.into(),0.into(),0.into()]}});
        let mut scene = Scene::default();
        scene.invoke("gs", "ExtGState", state);
        scene.invoke("Do", "XObject", id);
        let wrapper = self.form(scene, bounds, true)?;
        if !isolated {
            self.pdf
                .get_object_mut(wrapper)
                .map_err(|e| e.to_string())?
                .as_stream_mut()
                .map_err(|e| e.to_string())?
                .dict
                .remove(b"Group");
        }
        Ok(wrapper)
    }
    fn state(&mut self, fill: u8, stroke: u8, blend: Blend) -> ObjectId {
        let key = (fill, stroke, blend_name(blend).to_string());
        if let Some(id) = self.states.get(&key) {
            return *id;
        }
        let id=self.pdf.add_object(dictionary!{"Type"=>"ExtGState","ca"=>fill as f32/255.,"CA"=>stroke as f32/255.,"BM"=>Object::Name(blend_name(blend).as_bytes().to_vec())});
        self.states.insert(key, id);
        id
    }
    fn image(&mut self, layer: &Layer) -> Result<ObjectId, String> {
        if let Some(id) = self.images.get(&layer.id) {
            return Ok(*id);
        }
        let pixels = layer.kind.pixels().ok_or("Expected a raster layer")?;
        // w and h are file or document fields. w*h*4 can overflow usize, and
        // that panic used to fire inside the check that should reject them.
        let bytes = u64::from(pixels.w)
            .checked_mul(u64::from(pixels.h))
            .and_then(|n| n.checked_mul(4));
        if pixels.w == 0 || pixels.h == 0 || bytes != Some(pixels.data.len() as u64) {
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

/// Check native RGBA buffer dimensions.
/// Args: pixels. Returns an error for empty, overflowing or inconsistent buffers.
fn validate_pixels(pixels: &Pixels) -> Result<(), String> {
    if pixels.w == 0
        || pixels.h == 0
        || u64::from(pixels.w)
            .checked_mul(u64::from(pixels.h))
            .and_then(|n| n.checked_mul(4))
            != Some(pixels.data.len() as u64)
    {
        return Err("Invalid raster buffer during PDF export".into());
    }
    Ok(())
}
/// Append an affine transform.
/// Args: output operations and transform. Returns no value.
fn matrix(ops: &mut Vec<Operation>, t: tiny_skia::Transform) {
    op(ops, "cm", &[t.sx, t.ky, t.kx, t.sy, t.tx, t.ty]);
}
/// Place a PDF image in native pixel coordinates.
/// Args: output, transform and image dimensions. Returns no value.
fn image_matrix(ops: &mut Vec<Operation>, t: tiny_skia::Transform, w: u32, h: u32) {
    let (w, h) = (w as f32, h as f32);
    op(
        ops,
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
}
/// Rotate a paint coordinate system around its object.
/// Args: output, center and radians. Returns no value.
fn rotate_matrix(ops: &mut Vec<Operation>, center: Pt, rotation: f32) {
    if rotation.abs() > 1e-8 {
        matrix(
            ops,
            tiny_skia::Transform::from_rotate_at(rotation.to_degrees(), center.x, center.y),
        );
    }
}
/// Merge disjoint resource names.
/// Args: destination and source dictionaries. Returns no value.
fn merge_resources(out: &mut Dictionary, other: Dictionary) {
    for (category, values) in other {
        if let Ok(values) = values.as_dict() {
            if out.get(&category).is_err() {
                out.set(category.clone(), Dictionary::new());
            }
            if let Ok(Object::Dictionary(target)) = out.get_mut(&category) {
                for (name, id) in values {
                    target.set(name.clone(), id.clone());
                }
            }
        }
    }
}
/// Write native stroke parameters.
/// Args: output and object. Returns no value.
fn stroke_style(ops: &mut Vec<Operation>, shape: &Shape) {
    if let Some(s) = &shape.style.stroke {
        op(ops, "w", &[s.width]);
        op(
            ops,
            "J",
            &[match s.cap {
                Cap::Butt => 0.,
                Cap::Round => 1.,
                Cap::Square => 2.,
            }],
        );
        op(
            ops,
            "j",
            &[match s.join {
                Join::Miter => 0.,
                Join::Round => 1.,
                Join::Bevel => 2.,
            }],
        );
        ops.push(Operation::new(
            "d",
            vec![
                Object::Array(
                    s.dash
                        .map(|(a, b)| vec![Object::Real(a), Object::Real(b)])
                        .unwrap_or_default(),
                ),
                Object::Real(s.dash_offset),
            ],
        ));
    }
}
/// Write an outlined stroke as exact vector segments.
/// Args: output and Skia path. Returns no value.
fn skia_path(ops: &mut Vec<Operation>, path: &tiny_skia::Path) {
    let mut last = Pt::ZERO;
    for segment in path.segments() {
        use tiny_skia::PathSegment::*;
        match segment {
            MoveTo(p) => {
                op(ops, "m", &[p.x, p.y]);
                last = Pt::new(p.x, p.y);
            }
            LineTo(p) => {
                op(ops, "l", &[p.x, p.y]);
                last = Pt::new(p.x, p.y);
            }
            QuadTo(c, p) => {
                let c = Pt::new(c.x, c.y);
                let p = Pt::new(p.x, p.y);
                let a = last + (c - last) * (2. / 3.);
                let b = p + (c - p) * (2. / 3.);
                op(ops, "c", &[a.x, a.y, b.x, b.y, p.x, p.y]);
                last = p;
            }
            CubicTo(a, b, p) => {
                op(ops, "c", &[a.x, a.y, b.x, b.y, p.x, p.y]);
                last = Pt::new(p.x, p.y);
            }
            Close => op(ops, "h", &[]),
        }
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
    // Compounds keep a closed flag and cubics on every contour. `is_closed`
    // is true when any contour is closed, so it must not decide the others.
    if matches!(shape.geom, Geom::Path { .. } | Geom::Paths { .. }) {
        for (anchors, closed) in shape.geom.path_contours() {
            let Some(first) = anchors.first() else {
                continue;
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
            if closed {
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
