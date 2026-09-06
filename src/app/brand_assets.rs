//! Brand files are placed into the current document, including native projects.
use super::*;
use crate::document::{Pixels, next_id};
use crate::import::Imported;
use std::path::Path;

const MAX_PIXELS: usize = 32_000_000;

impl Studio {
    /// Synchronous entry point for callers outside the frame loop. The sidebar
    /// decodes in its worker and calls `place_brand_imported` when it is ready.
    pub fn place_brand_asset(&mut self, path: &Path, at: Pt) -> Result<(), String> {
        self.place_brand_imported(crate::brand::load_asset(path)?, at)
    }

    pub fn place_brand_imported(&mut self, imported: Imported, at: Pt) -> Result<(), String> {
        self.place_imported_at(imported, at, None)
    }

    pub(super) fn place_layered_document(
        &mut self,
        doc: Document,
        destination: Bounds,
    ) -> Result<(), String> {
        let source = Bounds::from_min_size(Pt::ZERO, doc.size());
        self.place_imported_at(
            Imported::Document(doc),
            destination.center(),
            Some((source, destination)),
        )?;
        Ok(())
    }

    fn place_imported_at(
        &mut self,
        imported: Imported,
        at: Pt,
        placement: Option<(Bounds, Bounds)>,
    ) -> Result<(), String> {
        if self.pending_nav.is_some() {
            return Err("Finish the open dialog before placing an asset".into());
        }
        if !at.x.is_finite() || !at.y.is_finite() {
            return Err("The placement point is invalid".into());
        }
        let (name, mut layers, motion, notes) = match imported {
            Imported::Document(doc) => {
                doc.validate_hierarchy()?;
                (doc.name, doc.layers, doc.motion, doc.import_notes)
            }
            Imported::Raster { name, image } => {
                checked_pixels(image.w, image.h, image.data.len())?;
                let pixels = Pixels::from_rgba(image.w, image.h, image.data)
                    .ok_or("The asset contains invalid image pixels")?;
                let layer = Layer::placed_raster(
                    name.clone(),
                    pixels,
                    Pt::ZERO,
                    Pt::new(image.w as f32, image.h as f32),
                );
                (name, vec![layer], Motion::default(), vec![])
            }
            Imported::Svg { name, svg } => {
                if svg.len() > 16 * 1024 * 1024 {
                    return Err("Choose an SVG smaller than 16 MB".into());
                }
                let layer = svg_layer(&name, &svg)?;
                (name, vec![layer], Motion::default(), vec![])
            }
        };
        if layers.len() > 1024 {
            return Err("The brand asset contains too many layers".into());
        }
        let mut total_pixels = 0usize;
        let mut total_shapes = 0usize;
        let mut largest_id = 0;
        let mut vector_masks = 0usize;
        for layer in &layers {
            largest_id = largest_id.max(layer.id);
            for pixels in [layer.kind.pixels(), layer.mask.as_ref()]
                .into_iter()
                .flatten()
            {
                total_pixels += checked_pixels(pixels.w, pixels.h, pixels.data.len())?;
            }
            total_shapes += layer.kind.shapes().map_or(0, <[Shape]>::len);
            if let Some(shapes) = layer.kind.shapes() {
                vector_masks += usize::from(
                    layer.mask.is_some() && (layer.mask_size.x <= 0.0 || layer.mask_size.y <= 0.0),
                );
                for shape in shapes {
                    largest_id = largest_id.max(shape.id);
                    if !shape.rotation.is_finite() || !finite_bounds(shape.geom.bbox()) {
                        return Err("The asset has invalid geometry".into());
                    }
                }
            }
        }
        if total_pixels > MAX_PIXELS || total_shapes > 100_000 {
            return Err("The brand asset is too large to place safely".into());
        }
        if vector_masks as f64
            * f64::from(self.doc.width.ceil())
            * f64::from(self.doc.height.ceil())
            > MAX_PIXELS as f64
        {
            return Err("The destination is too large for the asset's vector masks".into());
        }
        if largest_id >= u64::MAX - 200_000 {
            return Err("The asset contains invalid object IDs".into());
        }
        crate::document::bump_id(largest_id);
        // Empty paint buffers and empty vector layers carry no artwork.
        layers.retain(|layer| {
            layer.is_group
                || match &layer.kind {
                    LayerKind::Vector { shapes } => !shapes.is_empty(),
                    LayerKind::Raster { pixels, .. } => !pixels.is_invisible(),
                }
        });
        let mut bounds: Option<Bounds> = None;
        for layer in &layers {
            if !layer.visible {
                continue;
            }
            let items = match &layer.kind {
                LayerKind::Vector { shapes } => shapes
                    .iter()
                    .filter(|shape| shape.visible && !shape.guide)
                    .map(Shape::world_bbox)
                    .collect::<Vec<_>>(),
                LayerKind::Raster { .. } => layer.kind.raster_bounds().into_iter().collect(),
            };
            for item in items {
                if !finite_bounds(item) {
                    return Err("The asset has invalid geometry".into());
                }
                bounds = Some(bounds.map_or(item, |bounds| bounds.union(item)));
            }
        }
        let source = placement
            .map(|(source, _)| source)
            .or(bounds)
            .ok_or("The brand asset has no visible artwork")?;
        let source = Bounds::from_min_size(
            source.center() - Pt::new(source.width().max(1.0), source.height().max(1.0)) * 0.5,
            Pt::new(source.width().max(1.0), source.height().max(1.0)),
        );
        let scale = placement
            .map(|(_, destination)| destination.width() / source.width())
            .unwrap_or_else(|| {
                (self.doc.width * 0.92 / source.width())
                    .min(self.doc.height * 0.92 / source.height())
                    .min(1.0)
            });
        if !scale.is_finite() || scale <= 0.0 {
            return Err("The destination document has invalid dimensions".into());
        }
        // Do not clamp to the paper: dropping on another artboard or the pasteboard
        // must use the point the user chose.
        let destination =
            Bounds::from_min_size(at - source.size() * (scale * 0.5), source.size() * scale);
        let offset = destination.min - source.min * scale;
        let mut ids = HashMap::new();
        let mut selected = Vec::new();
        let mut commands = Vec::new();
        let layer_ids: HashMap<_, _> = layers.iter().map(|layer| (layer.id, next_id())).collect();
        for (position, mut layer) in layers.into_iter().enumerate() {
            let index = self.doc.layers.len() + position;
            layer.id = layer_ids[&layer.id];
            layer.parent = layer.parent.and_then(|id| layer_ids.get(&id).copied());
            match &mut layer.kind {
                LayerKind::Vector { shapes } => {
                    if layer.mask_size.x > 0.0 && layer.mask_size.y > 0.0 {
                        layer.mask_origin = layer.mask_origin * scale + offset;
                        layer.mask_size = layer.mask_size * scale;
                    } else if let Some(mask) = layer.mask.take() {
                        layer.mask = Some(map_mask(
                            mask,
                            scale,
                            offset,
                            self.doc.width,
                            self.doc.height,
                        )?);
                    }
                    for shape in shapes {
                        let old_id = shape.id;
                        shape.id = next_id();
                        if ids.insert(old_id, shape.id).is_some() {
                            return Err("The asset contains duplicate object IDs".into());
                        }
                        shape.geom.map_into(source, destination);
                        match &mut shape.geom {
                            Geom::Rect { radius, .. } => *radius *= scale,
                            Geom::Path { anchors, .. } => {
                                for anchor in anchors {
                                    anchor.radius *= scale;
                                }
                            }
                            _ => {}
                        }
                        for radius in &mut shape.corners {
                            *radius *= scale;
                        }
                        if let Some(stroke) = &mut shape.style.stroke {
                            stroke.width *= scale;
                            if let Some(dash) = &mut stroke.dash {
                                dash.0 *= scale;
                                dash.1 *= scale;
                            }
                        }
                        scale_filters(&mut shape.filters, scale);
                        if layer.visible
                            && !layer.locked
                            && shape.visible
                            && !shape.locked
                            && !shape.guide
                        {
                            selected.push((index, shape.id));
                        }
                    }
                }
                LayerKind::Raster {
                    pixels,
                    origin,
                    size,
                    ..
                } => {
                    let native = if size.x.abs() > 0.5 && size.y.abs() > 0.5 {
                        *size
                    } else {
                        Pt::new(pixels.w as f32, pixels.h as f32)
                    };
                    *origin = *origin * scale + offset;
                    *size = native * scale;
                    if layer.visible && !layer.locked {
                        selected.push((index, RASTER_ID));
                    }
                }
            }
            scale_filters(&mut layer.filters, scale);
            commands.push(Cmd::AddLayer { index, layer });
        }
        let mut after = self.doc.motion.clone();
        for mut track in motion.tracks {
            let Some(id) = ids.get(&track.shape) else {
                continue;
            };
            track.shape = *id;
            if matches!(track.prop, Prop::X | Prop::Y) {
                for key in &mut track.keys {
                    key.value *= scale;
                }
            }
            after.tracks.push(track);
        }
        if after.tracks.len() != self.doc.motion.tracks.len() {
            after.duration = after.duration.max(motion.duration);
            commands.push(Cmd::SetMotion {
                before: self.doc.motion.clone(),
                after,
            });
        }
        if !notes.is_empty() {
            let before = self.doc.import_notes.clone();
            let mut after = before.clone();
            for note in notes {
                if !after.contains(&note) {
                    after.push(note);
                }
            }
            if before != after {
                commands.push(Cmd::SetImportNotes { before, after });
            }
        }
        // Complete the existing edit only after every import validation succeeded.
        self.commit_type_edit();
        self.commit(Cmd::Batch(commands));
        selected.retain(|(index, _)| self.doc.layer_editable(*index));
        self.selection = selected;
        self.active_layer = self
            .selection
            .last()
            .map(|(layer, _)| *layer)
            .or_else(|| self.doc.layers.len().checked_sub(1));
        self.pending_place = None;
        self.op = None;
        self.key_drag = None;
        self.paint_mask = false;
        self.tool = Tool::Select;
        if matches!(self.persona, Persona::Photo | Persona::Pixel) {
            self.persona = Persona::Design;
        }
        self.show_welcome = false;
        self.status = format!("{name} placed · editable · one Undo");
        Ok(())
    }
}

fn checked_pixels(width: u32, height: u32, bytes: usize) -> Result<usize, String> {
    let pixels = (width as usize)
        .checked_mul(height as usize)
        .filter(|&pixels| pixels > 0 && pixels <= MAX_PIXELS)
        .ok_or("The asset image is too large")?;
    if pixels.checked_mul(4) != Some(bytes) {
        return Err("The asset contains invalid image pixels".into());
    }
    Ok(pixels)
}

fn finite_bounds(bounds: Bounds) -> bool {
    [bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y]
        .into_iter()
        .all(f32::is_finite)
}

fn svg_layer(name: &str, svg: &str) -> Result<Layer, String> {
    use crate::shape_browser::SvgPaint;
    let mut layer = Layer::vector(name);
    let shapes = layer.kind.shapes_mut().unwrap();
    for element in crate::shape_browser::svg_to_elements(svg)? {
        // SVG defaults to black; a brand asset must not inherit the editor's fill.
        let fill = match element.fill {
            SvgPaint::None => Fill::None,
            SvgPaint::Solid(color) => Fill::Solid(color),
            SvgPaint::Unspecified => Fill::Solid(Rgba::BLACK),
        };
        let stroke = match element.stroke {
            SvgPaint::Solid(color) => Some(Stroke {
                color,
                width: element.stroke_width.max(0.25),
                cap: match element.stroke_cap.as_deref() {
                    Some("round") => Cap::Round,
                    Some("square") => Cap::Square,
                    _ => Cap::Butt,
                },
                join: match element.stroke_join.as_deref() {
                    Some("round") => Join::Round,
                    Some("bevel") => Join::Bevel,
                    _ => Join::Miter,
                },
                dash: None,
            }),
            _ => None,
        };
        shapes.push(Shape::new(element.geom, Style { fill, stroke }));
    }
    Ok(layer)
}

fn map_mask(
    mask: Pixels,
    scale: f32,
    offset: Pt,
    width: f32,
    height: f32,
) -> Result<Pixels, String> {
    let width = width.ceil().max(1.0) as u32;
    let height = height.ceil().max(1.0) as u32;
    if u64::from(width) * u64::from(height) > MAX_PIXELS as u64 {
        return Err("The destination is too large for a placed vector mask".into());
    }
    let mut mapped = Pixmap::new(width, height).ok_or("Could not allocate the placed mask")?;
    mask.with_pm(|source| {
        mapped.draw_pixmap(
            0,
            0,
            source.as_ref(),
            &tiny_skia::PixmapPaint {
                quality: tiny_skia::FilterQuality::Bilinear,
                ..Default::default()
            },
            tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, offset.x, offset.y),
            None,
        )
    })
    .ok_or("The source mask is invalid")?;
    let bytes = mapped
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let pixel = pixel.demultiply();
            [pixel.red(), pixel.green(), pixel.blue(), pixel.alpha()]
        })
        .collect();
    Pixels::from_rgba(width, height, bytes).ok_or("The placed mask is invalid".into())
}

fn scale_filters(stack: &mut crate::filter::FilterStack, scale: f32) {
    use crate::filter::Fx;
    for effect in &mut stack.items {
        match effect {
            Fx::Blur { std } => *std *= scale,
            Fx::Shadow { dx, dy, blur, .. } | Fx::InnerShadow { dx, dy, blur, .. } => {
                *dx *= scale;
                *dy *= scale;
                *blur *= scale;
            }
            Fx::Offset { dx, dy } => {
                *dx *= scale;
                *dy *= scale;
            }
            Fx::Morphology { radius, .. } => *radius *= scale,
            Fx::Displacement { scale: amount, .. } => *amount *= scale,
            Fx::Turbulence { base, .. } => *base /= scale,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layered_place_scales_canvas_geometry_remaps_groups_and_undoes_every_change() {
        let mut source = document("Layered artwork", 64.0, 32.0);
        source.import_notes = vec!["Imported color conversion".into()];
        let group = Layer::group("Artwork group");
        let source_group_id = group.id;
        let mut vector = Layer::vector("Masked vector");
        vector.parent = Some(group.id);
        vector.kind.shapes_mut().unwrap().push(rect(
            Pt::new(10.0, 4.0),
            Pt::new(20.0, 10.0),
            Rgba::BLACK,
        ));
        let source_shape_id = vector.kind.shapes().unwrap()[0].id;
        vector.mask = Pixels::from_rgba(2, 1, vec![255; 8]);
        vector.mask_origin = Pt::new(10.0, 4.0);
        vector.mask_size = Pt::new(20.0, 10.0);
        let data = [230, 60, 30, 255].repeat(4);
        let mut raster = Layer::placed_raster(
            "Native pixels",
            Pixels::from_rgba(2, 2, data.clone()).unwrap(),
            Pt::new(6.0, 18.0),
            Pt::ZERO,
        );
        raster.parent = Some(group.id);
        source.layers = vec![vector, raster, group];
        let mut studio = studio();
        let before = serde_json::to_value(&studio.doc).unwrap();
        let old_layers = studio.doc.layers.len();
        let path = studio.path.clone();
        studio.pending_place = Some(PendingPlace::Document(source));
        studio.commit_place_rect(Pt::new(80.0, 60.0), Pt::new(208.0, 124.0));
        assert_eq!(studio.path, path);
        assert_eq!(studio.doc.layers.len(), old_layers + 3);
        assert_eq!(studio.history.len(), 1);
        let vector = &studio.doc.layers[old_layers];
        let raster = &studio.doc.layers[old_layers + 1];
        let group = &studio.doc.layers[old_layers + 2];
        assert_ne!(group.id, source_group_id);
        assert_eq!(vector.parent, Some(group.id));
        assert_eq!(raster.parent, Some(group.id));
        assert_ne!(vector.kind.shapes().unwrap()[0].id, source_shape_id);
        assert_eq!(
            vector.kind.shapes().unwrap()[0].geom.bbox().min,
            Pt::new(100.0, 68.0)
        );
        assert_eq!(vector.mask_origin, Pt::new(100.0, 68.0));
        assert_eq!(vector.mask_size, Pt::new(40.0, 20.0));
        let (origin, size, _) = raster.kind.raster_xform().unwrap();
        assert_eq!(origin, Pt::new(92.0, 96.0));
        assert_eq!(size, Pt::new(4.0, 4.0));
        assert_eq!(raster.kind.pixels().unwrap().data, data);
        studio.doc.validate_hierarchy().unwrap();
        let placed = serde_json::to_value(&studio.doc).unwrap();
        studio.undo();
        assert_eq!(serde_json::to_value(&studio.doc).unwrap(), before);
        studio.redo();
        assert_eq!(serde_json::to_value(&studio.doc).unwrap(), placed);
    }

    fn document(name: &str, width: f32, height: f32) -> Document {
        let mut doc = Document::new(name, 1.0, 1.0, 72.0);
        doc.layers = vec![Layer::vector("Artwork")];
        doc.width = width;
        doc.height = height;
        doc.artboards = vec![Artboard::new(0, Pt::ZERO, Pt::new(width, height))];
        doc.transparent = true;
        doc
    }

    fn rect(origin: Pt, size: Pt, fill: Rgba) -> Shape {
        Shape::new(
            Geom::Rect {
                origin,
                size,
                radius: 0.0,
            },
            Style {
                fill: Fill::Solid(fill),
                stroke: None,
            },
        )
    }

    fn studio() -> Studio {
        let mut studio = Studio::new();
        studio.doc = document("Current project", 320.0, 240.0);
        studio.doc.layers[0].kind.shapes_mut().unwrap().push(rect(
            Pt::new(12.0, 20.0),
            Pt::new(30.0, 20.0),
            Rgba::BLACK,
        ));
        studio.path = Some(PathBuf::from("/tmp/current-brand-destination.oma"));
        studio.active_layer = Some(0);
        studio.need_fit = false;
        studio.show_welcome = false;
        studio
    }

    struct FileGuard(PathBuf);
    impl Drop for FileGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn native_brand_placement_preserves_live_artwork_motion_and_tabs_with_one_undo() {
        let mut source = document("Brand mark", 128.0, 96.0);
        let mut mark = rect(Pt::new(12.0, 18.0), Pt::new(36.0, 25.0), Rgba::WHITE);
        mark.rotation = 0.35;
        mark.style.fill = Fill::Linear {
            from: [-0.2, 0.0],
            to: [1.2, 1.0],
            c0: Rgba::BLACK,
            c1: Rgba::from_hex(0xD97C5B),
        };
        let original_mark = mark.clone();
        let title = Shape::new(
            Geom::Text(TypeRun {
                content: "Brand".into(),
                origin: Pt::new(15.0, 70.0),
                px: 18.0,
                ..Default::default()
            }),
            Style::default(),
        );
        source.layers[0]
            .kind
            .shapes_mut()
            .unwrap()
            .extend([mark, title]);
        source
            .motion
            .set_key(original_mark.id, Prop::X, 1.0, 12.0, Ease::EaseOut);
        let path = FileGuard(
            std::env::temp_dir().join(format!("omadesign-brand-place-{}.oma", next_id())),
        );
        crate::project::save_to(&source, &path.0).unwrap();
        let mut studio = studio();
        studio.ensure_tabs();
        let existing_id = studio.doc.layers[0].kind.shapes().unwrap()[0].id;
        studio
            .doc
            .motion
            .set_key(existing_id, Prop::Y, 1.0, 9.0, Ease::Linear);
        let before = serde_json::to_value(&studio.doc).unwrap();
        let path_before = studio.path.clone();
        let tabs_before = studio.tab_count();
        let view_before = studio.view;
        let at = Pt::new(-60.0, 100.0);
        studio.place_brand_asset(&path.0, at).unwrap();
        assert_eq!(studio.path, path_before);
        assert_eq!(studio.tab_count(), tabs_before);
        assert_eq!(studio.doc.name, "Current project");
        assert_eq!(studio.view.offset, view_before.offset);
        assert_eq!(studio.view.scale, view_before.scale);
        assert_eq!(studio.history.len(), 1);
        let shapes = studio.doc.layers[1].kind.shapes().unwrap();
        let union = shapes
            .iter()
            .map(Shape::world_bbox)
            .reduce(|a, b| a.union(b))
            .unwrap();
        assert!(
            (union.center() - at).length() < 0.01,
            "drop point was clamped or shifted"
        );
        let placed_mark = &shapes[0];
        assert_ne!(placed_mark.id, original_mark.id);
        assert_eq!(placed_mark.rotation, original_mark.rotation);
        assert_eq!(placed_mark.style, original_mark.style);
        assert!(matches!(&shapes[1].geom, Geom::Text(run) if run.content == "Brand"));
        let new_id = placed_mark.id;
        assert_eq!(studio.doc.motion.value(new_id, Prop::X, 1.0), Some(12.0));
        assert_eq!(
            studio.doc.motion.value(existing_id, Prop::Y, 1.0),
            Some(9.0)
        );
        let placed = serde_json::to_value(&studio.doc).unwrap();
        studio.undo();
        assert_eq!(serde_json::to_value(&studio.doc).unwrap(), before);
        studio.redo();
        assert_eq!(serde_json::to_value(&studio.doc).unwrap(), placed);
        studio.place_brand_asset(&path.0, at).unwrap();
        assert_ne!(studio.doc.layers[2].kind.shapes().unwrap()[0].id, new_id);
        studio.undo();
        assert_eq!(serde_json::to_value(&studio.doc).unwrap(), placed);
    }

    #[test]
    fn svg_and_raster_placement_are_atomic_and_independent_of_current_fill() {
        let mut studio = studio();
        studio.style.fill = Fill::Solid(Rgba::from_hex(0x00FF00));
        let before = serde_json::to_value(&studio.doc).unwrap();
        let svg = "<svg viewBox=\"0 0 40 20\"><path d=\"M0 0 H20 V20 H0 Z\"/><path d=\"M25 0 H40 V20 H25 Z\" fill=\"none\" stroke=\"#D97C5B\" stroke-width=\"2\"/></svg>";
        studio
            .place_brand_imported(
                Imported::Svg {
                    name: "Two paths".into(),
                    svg: svg.into(),
                },
                Pt::new(100.0, 90.0),
            )
            .unwrap();
        let shapes = studio.doc.layers[1].kind.shapes().unwrap();
        assert_eq!(shapes.len(), 2);
        assert_eq!(shapes[0].style.fill, Fill::Solid(Rgba::BLACK));
        assert_eq!(shapes[1].style.fill, Fill::None);
        assert_eq!(
            shapes[1].style.stroke.as_ref().unwrap().color,
            Rgba::from_hex(0xD97C5B)
        );
        assert_eq!(studio.history.len(), 1);
        studio.undo();
        assert_eq!(serde_json::to_value(&studio.doc).unwrap(), before);
        let data = vec![210, 50, 90, 110, 40, 100, 200, 255];
        studio
            .place_brand_imported(
                Imported::Raster {
                    name: "Transparent mark".into(),
                    image: RgbaImage {
                        w: 2,
                        h: 1,
                        data: data.clone(),
                    },
                },
                Pt::new(150.0, 120.0),
            )
            .unwrap();
        let layer = studio.doc.layers.last().unwrap();
        assert_eq!(layer.kind.pixels().unwrap().data, data);
        assert_eq!(
            layer.kind.raster_bounds().unwrap().center(),
            Pt::new(150.0, 120.0)
        );
        assert_eq!(studio.selection, vec![(1, RASTER_ID)]);
        studio.undo();
        assert_eq!(serde_json::to_value(&studio.doc).unwrap(), before);
    }

    #[test]
    fn native_masks_follow_vector_and_rotated_image_placement_in_export() {
        let mut source = document("Masked mark", 32.0, 16.0);
        source.layers[0].kind.shapes_mut().unwrap().push(rect(
            Pt::ZERO,
            Pt::new(16.0, 16.0),
            Rgba::WHITE,
        ));
        let mut mask = Pixels::new(32, 16);
        for y in 0..16 {
            for x in 0..8 {
                let i = (y * 32 + x) * 4;
                mask.data[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        source.layers[0].mask = Some(mask);
        let pixels = Pixels::from_rgba(8, 8, [255, 120, 20, 255].repeat(64)).unwrap();
        let mut image =
            Layer::placed_raster("Image", pixels, Pt::new(24.0, 4.0), Pt::new(8.0, 8.0));
        image
            .kind
            .set_raster_xform(Pt::new(24.0, 4.0), Pt::new(8.0, 8.0), 0.35);
        image.mask = Some(Pixels::from_rgba(8, 8, [255, 255, 255, 128].repeat(64)).unwrap());
        source.layers.push(image);
        let source_bounds = source.layers[0].kind.shapes().unwrap()[0]
            .world_bbox()
            .union(source.layers[1].kind.raster_bounds().unwrap());
        let expected = crate::compositor::export_png(&source, 1).unwrap();
        let expected = image::load_from_memory(&expected).unwrap().to_rgba8();
        let mut studio = studio();
        studio.doc = document("Destination", 80.0, 48.0);
        let offset = Pt::new(20.0, 16.0);
        studio
            .place_brand_imported(Imported::Document(source), source_bounds.center() + offset)
            .unwrap();
        let output = crate::compositor::export_png(&studio.doc, 1).unwrap();
        let output = image::load_from_memory(&output).unwrap().to_rgba8();
        for (x, y, pixel) in expected.enumerate_pixels() {
            let actual = output.get_pixel(x + 20, y + 16);
            assert!(
                pixel
                    .0
                    .iter()
                    .zip(actual.0)
                    .all(|(a, b)| a.abs_diff(b) <= 1),
                "mask moved incorrectly at {x},{y}"
            );
        }
        assert!(output.get_pixel(22, 20)[3] > 250);
        assert_eq!(output.get_pixel(32, 20)[3], 0);
        assert_eq!(studio.doc.layers[2].mask.as_ref().unwrap().w, 8);
        studio.undo();
        assert_eq!(studio.doc.layers.len(), 1);
    }

    #[test]
    fn rejected_asset_does_not_change_document_or_pending_edit() {
        let mut studio = studio();
        studio.pending_place = Some(PendingPlace::Svg {
            name: "Keep this placement".into(),
            svg: "<svg/>".into(),
        });
        let before = serde_json::to_value(&studio.doc).unwrap();
        let mut source = document("Empty", 10_000.0, 10_000.0);
        assert!(
            studio
                .place_brand_imported(Imported::Document(source.clone()), Pt::ZERO)
                .is_err()
        );
        source.layers[0].kind.shapes_mut().unwrap().push(rect(
            Pt::ZERO,
            Pt::new(5.0, 5.0),
            Rgba::WHITE,
        ));
        assert!(
            studio
                .place_brand_imported(Imported::Document(source), Pt::new(f32::NAN, 0.0))
                .is_err()
        );
        assert!(
            studio
                .place_brand_imported(
                    Imported::Raster {
                        name: "Invalid".into(),
                        image: RgbaImage {
                            w: 2,
                            h: 2,
                            data: vec![0; 3]
                        },
                    },
                    Pt::ZERO
                )
                .is_err()
        );
        assert_eq!(serde_json::to_value(&studio.doc).unwrap(), before);
        assert_eq!(studio.history.len(), 0);
        assert_eq!(
            studio.pending_place.as_ref().unwrap().name(),
            "Keep this placement"
        );
    }
}
