//! Asset placement has one destination: a frame and its owning vector layer.
//! Captured destinations survive asynchronous downloads without following a
//! later selection into a different frame.
use super::*;
use crate::document::next_id;
use crate::import::Imported;

impl Studio {
    /// Bounds carry local dimensions around a world-space center until commit.
    /// Transform every corner so the ghost matches inherited frame rotation.
    pub fn pending_preview_corners(&self, at: Pt) -> Option<[Pt; 4]> {
        let bounds = self.pending_preview_rect(at)?;
        let target = self.pending_place_frame.or_else(|| self.asset_frame_at(at));
        self.placement_preview_corners(bounds, target)
    }

    pub(super) fn pending_drag_rect(&self, start: Pt, cur: Pt) -> Option<Bounds> {
        let pending = self.pending_place.as_ref()?;
        let (sw, sh) = pending.native_size();
        let min = Pt::new(start.x.min(cur.x), start.y.min(cur.y));
        let max = Pt::new(start.x.max(cur.x), start.y.max(cur.y));
        let size = max - min;
        if size.x < 8.0 && size.y < 8.0 {
            return self.pending_preview_rect(start);
        }
        let scale = (size.x / sw.max(1.0)).min(size.y / sh.max(1.0)).max(0.01);
        let fitted = Pt::new(sw * scale, sh * scale);
        Some(Bounds::from_min_size(min + (size - fitted) * 0.5, fitted))
    }

    pub fn pending_drag_preview_corners(&self, start: Pt, cur: Pt) -> Option<[Pt; 4]> {
        if (cur.x - start.x).abs() < 8.0 && (cur.y - start.y).abs() < 8.0 {
            return self.pending_preview_corners(start);
        }
        let bounds = self.pending_drag_rect(start, cur)?;
        let target = self
            .pending_place_frame
            .or_else(|| self.asset_frame_at(bounds.center()));
        self.placement_preview_corners(bounds, target)
    }

    fn placement_preview_corners(
        &self,
        bounds: Bounds,
        target: Option<(usize, u64)>,
    ) -> Option<[Pt; 4]> {
        let local = if let Some(target) = target {
            self.asset_frame_bounds(target).ok()?;
            Bounds::from_min_size(
                self.asset_local_point(target, bounds.center()) - bounds.size() * 0.5,
                bounds.size(),
            )
        } else {
            bounds
        };
        let corners = [
            local.min,
            Pt::new(local.max.x, local.min.y),
            local.max,
            Pt::new(local.min.x, local.max.y),
        ];
        Some(
            corners
                .map(|point| target.map_or(point, |target| self.asset_world_point(target, point))),
        )
    }

    pub fn asset_frame_center(&self, target: (usize, u64)) -> Option<Pt> {
        let frame = self.doc.find_shape(target.0, target.1)?;
        let mut point = frame.geom.bbox().center();
        let mut parent = frame.layout.parent;
        for _ in 0..64 {
            let Some(id) = parent else { return Some(point) };
            let ancestor = self.doc.find_shape(target.0, id)?;
            point = point.rotate_about(ancestor.geom.bbox().center(), ancestor.rotation);
            parent = ancestor.layout.parent;
        }
        None
    }

    pub(super) fn asset_local_point(&self, target: (usize, u64), mut point: Pt) -> Pt {
        let mut chain = Vec::new();
        let mut parent = Some(target.1);
        for _ in 0..64 {
            let Some(shape) = parent.and_then(|id| self.doc.find_shape(target.0, id)) else {
                break;
            };
            chain.push(shape);
            parent = shape.layout.parent;
        }
        for shape in chain.into_iter().rev() {
            point = point.rotate_about(shape.geom.bbox().center(), -shape.rotation);
        }
        point
    }

    pub(super) fn asset_world_point(&self, target: (usize, u64), mut point: Pt) -> Pt {
        let mut parent = Some(target.1);
        for _ in 0..64 {
            let Some(shape) = parent.and_then(|id| self.doc.find_shape(target.0, id)) else {
                break;
            };
            point = point.rotate_about(shape.geom.bbox().center(), shape.rotation);
            parent = shape.layout.parent;
        }
        point
    }
    pub fn asset_frame_target(&self) -> Option<(usize, u64)> {
        self.selection.iter().find_map(|&(layer, id)| {
            let mut current = Some(id);
            let mut seen = HashSet::new();
            while let Some(id) = current {
                if !seen.insert(id) || seen.len() > 64 {
                    return None;
                }
                let shape = self.doc.find_shape(layer, id)?;
                if shape.layout.frame {
                    return Some((layer, id));
                }
                current = shape.layout.parent;
            }
            None
        })
    }

    pub fn asset_frame_at(&self, at: Pt) -> Option<(usize, u64)> {
        self.doc
            .layers
            .iter()
            .enumerate()
            .rev()
            .find_map(|(li, _)| {
                self.doc
                    .layer_editable(li)
                    .then(|| crate::layout::containing_frame(&self.doc, li, at))
                    .flatten()
                    .map(|id| (li, id))
            })
    }

    pub(super) fn asset_frame_bounds(&self, target: (usize, u64)) -> Result<Bounds, String> {
        let (layer, id) = target;
        if !self.doc.layer_editable(layer) {
            return Err("The destination frame's layer is hidden or locked".into());
        }
        let frame = self
            .doc
            .find_shape(layer, id)
            .filter(|s| s.layout.frame)
            .ok_or("The destination frame was removed. Choose a frame and place the asset again")?;
        let mut current = Some(id);
        let mut seen = HashSet::new();
        while let Some(id) = current {
            if !seen.insert(id) || seen.len() > 64 {
                return Err("The destination has an invalid frame hierarchy".into());
            }
            let shape = self
                .doc
                .find_shape(layer, id)
                .ok_or("The destination frame is missing")?;
            if !shape.visible || shape.locked {
                return Err("Unlock and show the destination frame before placing an asset".into());
            }
            current = shape.layout.parent;
        }
        let bounds = frame.geom.bbox();
        if ![bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y]
            .iter()
            .all(|v| v.is_finite())
            || bounds.width() < 1.0
            || bounds.height() < 1.0
        {
            return Err(
                "Give the destination frame a positive width and height before placing an asset"
                    .into(),
            );
        }
        Ok(bounds)
    }

    pub fn place_palette_shape(
        &mut self,
        mut geom: Geom,
        name: String,
        target: Option<(usize, u64)>,
    ) {
        let (layer, parent, center) = if let Some(target) = target {
            let bounds = match self.asset_frame_bounds(target) {
                Ok(b) => b,
                Err(e) => {
                    self.shape_status = e.clone();
                    self.status = e;
                    return;
                }
            };
            let source = geom.bbox();
            let edge = 48.0f32
                .min(bounds.width() * 0.5)
                .min(bounds.height() * 0.5)
                .max(1.0);
            let scale = (edge / source.width().max(1.0)).min(edge / source.height().max(1.0));
            geom.map_into(
                source,
                Bounds::from_min_size(
                    bounds.center() - source.size() * (scale * 0.5),
                    source.size() * scale,
                ),
            );
            (target.0, Some(target.1), bounds.center())
        } else {
            let Some(layer) = self.vector_target() else {
                self.shape_status = "Add a vector layer to place this shape".into();
                return;
            };
            (layer, None, self.doc.size() * 0.5)
        };
        geom.translate(center - geom.bbox().center());
        let mut style = self.style.clone();
        style.stroke = None;
        if style.fill.is_none() {
            style.fill = Fill::Solid(self.brush.color);
        }
        let mut shape = Shape::new(geom, style);
        shape.name = name.clone();
        shape.layout.parent = parent;
        let id = shape.id;
        self.commit(Cmd::AddShape { layer, shape });
        self.active_layer = Some(layer);
        self.selected_layer = None;
        self.selection = vec![(layer, id)];
        self.show_shape_browser = false;
        self.shape_status.clear();
        self.tool = Tool::Select;
        self.status = format!("Added {name}");
    }

    pub fn place_brand_imported_in_frame(
        &mut self,
        imported: Imported,
        at: Pt,
        target: Option<(usize, u64)>,
    ) -> Result<(), String> {
        match target {
            Some(target) => self.place_imported_in_frame(imported, at, target, None),
            None => self.place_imported_at(imported, at, None),
        }
    }

    pub(super) fn place_imported_in_frame(
        &mut self,
        imported: Imported,
        at: Pt,
        target: (usize, u64),
        destination: Option<Bounds>,
    ) -> Result<(), String> {
        if self.pending_nav.is_some() {
            return Err("Finish the open dialog before placing an asset".into());
        }
        let frame_bounds = self.asset_frame_bounds(target)?;
        let at = self.asset_local_point(target, at);
        let destination = destination.map(|bounds| {
            Bounds::from_min_size(
                self.asset_local_point(target, bounds.center()) - bounds.size() * 0.5,
                bounds.size(),
            )
        });
        let mut source = match imported {
            Imported::Document(doc) => doc,
            Imported::Svg { name, svg } => {
                let (mut doc, notes) = crate::formats::svg::read(&svg, &name)?;
                doc.import_notes.extend(notes);
                doc
            }
            Imported::Raster { name, image } => {
                let mut doc = Document::new(&name, 1.0, 1.0, 96.0);
                doc.width = image.w as f32;
                doc.height = image.h as f32;
                let fill = image_fill(image)?;
                let mut shape = crate::layout::make_frame(Pt::ZERO, doc.size());
                shape.name = name;
                shape.layout.image = Some(fill);
                shape.style = Style {
                    fill: Fill::None,
                    stroke: None,
                };
                let mut layer = Layer::vector("Image");
                layer.kind.shapes_mut().unwrap().push(shape);
                doc.layers = vec![layer];
                doc
            }
            Imported::Photo(_) => {
                return Err("Develop the photo before placing it in a frame".into());
            }
        };
        source.validate_hierarchy()?;
        if source
            .layers
            .iter()
            .filter_map(|l| l.kind.shapes())
            .map(|s| s.len())
            .sum::<usize>()
            > 100_000
        {
            return Err("The asset has too many objects to place".into());
        }
        let src = Bounds::from_min_size(Pt::ZERO, source.size());
        if ![src.width(), src.height(), at.x, at.y]
            .iter()
            .all(|v| v.is_finite())
            || src.width() <= 0.0
            || src.height() <= 0.0
        {
            return Err("The asset has invalid dimensions".into());
        }
        let dest = destination.unwrap_or_else(|| {
            let scale = (frame_bounds.width() * 0.8 / src.width())
                .min(frame_bounds.height() * 0.8 / src.height())
                .min(1.0);
            let size = src.size() * scale;
            let center = Pt::new(
                at.x.clamp(
                    frame_bounds.min.x + size.x * 0.5,
                    frame_bounds.max.x - size.x * 0.5,
                ),
                at.y.clamp(
                    frame_bounds.min.y + size.y * 0.5,
                    frame_bounds.max.y - size.y * 0.5,
                ),
            );
            Bounds::from_min_size(center - size * 0.5, size)
        });
        let scale = dest.width() / src.width();
        if !scale.is_finite() || scale <= 0.0 || dest.height() <= 0.0 {
            return Err("The placement bounds are invalid".into());
        }
        let (layer, parent) = target;
        // Frame nodes support group opacity and filters. Masks and non-normal
        // layer blending require pixels to retain the imported appearance.
        if source
            .layers
            .iter()
            .any(|l| l.mask.is_some() || l.blend != crate::color::Blend::Normal)
        {
            if source.width * source.height > 64_000_000.0 {
                return Err("The asset's masked canvas is too large".into());
            }
            let bytes = crate::compositor::export_png(&source, 1)?;
            let mut shape = crate::layout::make_frame(Pt::ZERO, source.size());
            shape.layout.image = Some(crate::layout_images::from_bytes(&bytes)?);
            shape.style = Style {
                fill: Fill::None,
                stroke: None,
            };
            let mut pixels = Layer::vector(&source.name);
            pixels.kind.shapes_mut().unwrap().push(shape);
            source.layers = vec![pixels];
            source.motion = Motion::default();
            source.import_notes.push("Imported layer masks/blending were preserved as an embedded image inside the frame.".into());
        }
        let mut root = asset_wrapper(&source.name, dest);
        root.layout.parent = Some(parent);
        let root_id = root.id;
        let mut shapes = vec![root];
        let mut ids = HashMap::new();
        for item in &source.layers {
            for shape in item.kind.shapes().unwrap_or_default() {
                if ids.insert(shape.id, next_id()).is_some() {
                    return Err("The asset contains duplicate object IDs".into());
                }
            }
        }
        let layer_ids: HashMap<_, _> = source.layers.iter().map(|l| (l.id, next_id())).collect();
        let mut tokens = self.doc.layout_tokens.clone();
        let mut token_ids = HashMap::new();
        for token in &source.layout_tokens {
            let mut token = token.clone();
            let old = token.id;
            token.id = next_id();
            token_ids.insert(old, token.id);
            let base = token.name.clone();
            let mut n = 2;
            while tokens
                .iter()
                .any(|t| t.name.eq_ignore_ascii_case(&token.name))
            {
                token.name = format!("{base} {n}");
                n += 1;
            }
            tokens.push(token);
        }
        for mut imported_layer in source.layers {
            let mut wrapper = asset_wrapper(&imported_layer.name, dest);
            wrapper.id = layer_ids[&imported_layer.id];
            wrapper.layout.parent = Some(
                imported_layer
                    .parent
                    .and_then(|p| layer_ids.get(&p).copied())
                    .unwrap_or(root_id),
            );
            wrapper.visible = imported_layer.visible;
            wrapper.locked = imported_layer.locked;
            wrapper.opacity = imported_layer.opacity;
            wrapper.filters = imported_layer.filters;
            super::brand_assets::scale_filters(&mut wrapper.filters, scale);
            let container = wrapper.id;
            shapes.push(wrapper);
            match &mut imported_layer.kind {
                LayerKind::Vector { shapes: items } => {
                    for mut shape in std::mem::take(items) {
                        let old = shape.id;
                        shape.id = ids[&old];
                        crate::layout_components::remap_duplicate(&mut shape, &ids);
                        shape.layout.parent = Some(
                            shape
                                .layout
                                .parent
                                .and_then(|p| ids.get(&p).copied())
                                .unwrap_or(container),
                        );
                        for property in crate::layout_tokens::TokenProperty::all() {
                            shape.layout.tokens.set(
                                property,
                                shape
                                    .layout
                                    .tokens
                                    .get(property)
                                    .and_then(|id| token_ids.get(&id).copied()),
                            );
                        }
                        if let Some(crate::layout_components::ComponentBinding::Instance {
                            nodes,
                            ..
                        }) = &mut shape.layout.component
                        {
                            for node in nodes {
                                for property in crate::layout_tokens::TokenProperty::all() {
                                    node.baseline.layout.tokens.set(
                                        property,
                                        node.baseline
                                            .layout
                                            .tokens
                                            .get(property)
                                            .and_then(|id| token_ids.get(&id).copied()),
                                    );
                                }
                            }
                        }
                        shape.geom.map_into(src, dest);
                        if let Geom::Rect { radius, .. } = &mut shape.geom {
                            *radius *= scale;
                        }
                        for r in &mut shape.corners {
                            *r *= scale;
                        }
                        if let Some(stroke) = &mut shape.style.stroke {
                            stroke.width *= scale;
                            if let Some(dash) = &mut stroke.dash {
                                dash.0 *= scale;
                                dash.1 *= scale;
                            }
                        }
                        super::brand_assets::scale_filters(&mut shape.filters, scale);
                        crate::text::fill_contours(&mut shape.geom);
                        shapes.push(shape);
                    }
                }
                LayerKind::Raster {
                    pixels,
                    origin,
                    size,
                    rotation,
                } => {
                    if pixels.is_invisible() {
                        continue;
                    }
                    let bounds = Bounds::from_min_size(
                        *origin,
                        if size.x > 0.0 && size.y > 0.0 {
                            *size
                        } else {
                            Pt::new(pixels.w as f32, pixels.h as f32)
                        },
                    );
                    let mut shape = asset_wrapper(&imported_layer.name, bounds);
                    shape.rotation = *rotation;
                    shape.layout.parent = Some(container);
                    shape.layout.image = Some(crate::layout_images::from_bytes(
                        &pixels
                            .to_pixmap()
                            .ok_or("Invalid image pixels")?
                            .encode_png()
                            .map_err(|e| e.to_string())?,
                    )?);
                    shape.geom.map_into(src, dest);
                    shapes.push(shape);
                }
            }
        }
        if shapes.len() <= 1
            || !shapes.iter().any(|s| {
                !s.style.fill.is_none() || s.style.stroke.is_some() || s.layout.image.is_some()
            })
        {
            return Err("The asset has no visible artwork".into());
        }
        let mut preview = self.doc.layout_snapshot();
        preview.layout_tokens = tokens.clone();
        preview.layers[layer]
            .kind
            .shapes_mut()
            .unwrap()
            .extend_from_slice(&shapes);
        preview.validate_hierarchy()?;
        let mut commands: Vec<_> = shapes
            .into_iter()
            .map(|shape| Cmd::AddShape { layer, shape })
            .collect();
        if tokens != self.doc.layout_tokens {
            commands.push(Cmd::SetLayoutTokens {
                before: self.doc.layout_tokens.clone(),
                after: tokens,
            });
        }
        let mut motion = self.doc.motion.clone();
        for mut track in source.motion.tracks {
            if let Some(id) = ids.get(&track.shape) {
                track.shape = *id;
                if matches!(track.prop, Prop::X | Prop::Y) {
                    for key in &mut track.keys {
                        key.value *= scale;
                    }
                }
                motion.tracks.push(track);
            }
        }
        if motion != self.doc.motion {
            motion.duration = motion.duration.max(source.motion.duration);
            commands.push(Cmd::SetMotion {
                before: self.doc.motion.clone(),
                after: motion,
            });
        }
        if !source.import_notes.is_empty() {
            let mut notes = self.doc.import_notes.clone();
            notes.extend(source.import_notes);
            commands.push(Cmd::SetImportNotes {
                before: self.doc.import_notes.clone(),
                after: notes,
            });
        }
        self.commit_type_edit();
        self.commit(Cmd::Batch(commands));
        self.active_layer = Some(layer);
        self.selection = vec![(layer, root_id)];
        self.selected_layer = None;
        self.op = None;
        self.pending_place = None;
        self.pending_place_frame = None;
        self.tool = Tool::Select;
        self.show_welcome = false;
        self.status = format!("{} placed in frame · one Undo", source.name);
        Ok(())
    }
}

fn asset_wrapper(name: &str, bounds: Bounds) -> Shape {
    let mut frame = crate::layout::make_frame(bounds.min, bounds.size());
    frame.name = name.into();
    frame.style = Style {
        fill: Fill::None,
        stroke: None,
    };
    frame.layout.clip = false;
    frame
}

fn image_fill(image: RgbaImage) -> Result<crate::layout_images::ImageFill, String> {
    let image = image::RgbaImage::from_raw(image.w, image.h, image.data)
        .ok_or("Invalid image dimensions")?;
    let mut encoded = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    crate::layout_images::from_bytes(encoded.get_ref())
}

#[cfg(test)]
#[path = "placement_tests.rs"]
mod tests;
