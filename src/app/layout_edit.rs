use super::*;
use crate::layout::{self, AutoStack, FrameLayout};

/// A Layout image owns its cutout alpha. Keep straight RGB intact while baking
/// the same luminance/alpha coverage used by the layer compositor. Raster masks
/// share the image's pixel transform; differently sized masks retain their
/// native extent, with zero coverage outside, rather than being stretched.
fn raster_pixels_for_frame(layer: &Layer) -> Result<Vec<u8>, String> {
    let pixels = layer
        .kind
        .pixels()
        .ok_or("The selected image no longer exists")?;
    let mut data = pixels.data.clone();
    let Some(mask) = &layer.mask else {
        return Ok(data);
    };
    let mask_pm = mask
        .to_pixmap()
        .ok_or("The image's layer mask is invalid")?;
    let mut placed = tiny_skia::Pixmap::new(pixels.w, pixels.h)
        .ok_or("Cannot allocate the image's layer mask")?;
    placed.draw_pixmap(
        0,
        0,
        mask_pm.as_ref(),
        &tiny_skia::PixmapPaint {
            quality: tiny_skia::FilterQuality::Bilinear,
            ..Default::default()
        },
        tiny_skia::Transform::identity(),
        None,
    );
    let coverage = tiny_skia::Mask::from_pixmap(placed.as_ref(), tiny_skia::MaskType::Luminance);
    for (pixel, mask) in data.chunks_exact_mut(4).zip(coverage.data()) {
        pixel[3] = ((u16::from(pixel[3]) * u16::from(*mask) + 127) / 255) as u8;
    }
    Ok(data)
}

impl Studio {
    pub fn set_layout_tokens(&mut self, after: Vec<crate::layout_tokens::DesignToken>) {
        if let Err(error) = crate::layout_tokens::validate_token_list(&after) {
            self.status = error;
            return;
        }
        self.commit(Cmd::SetLayoutTokens {
            before: self.doc.layout_tokens.clone(),
            after,
        });
    }

    pub fn bind_layout_token(
        &mut self,
        property: crate::layout_tokens::TokenProperty,
        token: Option<u64>,
    ) {
        let mut after = self.doc.layout_snapshot();
        for (li, id) in self.selection.clone() {
            if let Err(e) = crate::layout_tokens::bind(&mut after, li, &[id], property, token) {
                self.status = e;
                return;
            }
        }
        let commands = self
            .doc
            .layers
            .iter()
            .zip(&after.layers)
            .enumerate()
            .filter_map(|(layer, (a, b))| {
                let (a, b) = (a.kind.shapes()?, b.kind.shapes()?);
                (a != b).then(|| Cmd::SetVectorShapes {
                    layer,
                    before: a.to_vec(),
                    after: b.to_vec(),
                })
            })
            .collect();
        self.commit(Cmd::Batch(commands));
    }

    pub fn remove_layout_token(&mut self, id: u64) {
        let mut after = self.doc.layout_snapshot();
        if let Err(e) = crate::layout_tokens::remove_token(&mut after, id) {
            self.status = e;
            return;
        }
        let mut commands = vec![Cmd::SetLayoutTokens {
            before: self.doc.layout_tokens.clone(),
            after: after.layout_tokens.clone(),
        }];
        commands.extend(
            self.doc
                .layers
                .iter()
                .zip(&after.layers)
                .enumerate()
                .filter_map(|(layer, (a, b))| {
                    let (a, b) = (a.kind.shapes()?, b.kind.shapes()?);
                    (a != b).then(|| Cmd::SetVectorShapes {
                        layer,
                        before: a.to_vec(),
                        after: b.to_vec(),
                    })
                }),
        );
        self.commit(Cmd::Batch(commands));
    }
    /// Snapshot vectors only; raster documents can be hundreds of megabytes.
    pub(crate) fn reconcile_layout(&mut self) -> Vec<Cmd> {
        if !self.doc.layers.iter().any(|l| {
            l.kind
                .shapes()
                .is_some_and(|s| s.iter().any(|s| s.layout.frame))
        }) {
            return Vec::new();
        }
        let before: Vec<_> = self
            .doc
            .layers
            .iter()
            .enumerate()
            .filter_map(|(li, l)| l.kind.shapes().map(|s| (li, s.to_vec())))
            .collect();
        crate::layout_tokens::apply_tokens(&mut self.doc);
        let _ = crate::layout_components::synchronize_all(&mut self.doc);
        for li in 0..self.doc.layers.len() {
            layout::reflow_roots(&mut self.doc, li);
        }
        before
            .into_iter()
            .filter_map(|(layer, before)| {
                let after = self.doc.layers[layer].kind.shapes()?;
                (before != *after).then(|| Cmd::SetVectorShapes {
                    layer,
                    before,
                    after: after.to_vec(),
                })
            })
            .collect()
    }

    pub fn edit_layout(&mut self, edit: impl Fn(&mut FrameLayout)) {
        let mut commands = Vec::new();
        for (layer, id) in self.selection.clone() {
            if let Some(shape) = self.doc.find_shape(layer, id) {
                let before = shape.layout.clone();
                let mut after = before.clone();
                edit(&mut after);
                if before != after {
                    commands.push(Cmd::SetLayout {
                        layer,
                        id,
                        before,
                        after,
                    });
                }
            }
        }
        if !commands.is_empty() {
            self.commit(Cmd::Batch(commands));
        }
    }

    pub fn auto_layout_selection(&mut self) {
        if self.selected_frame().is_none() {
            self.wrap_selection_frame_with_stack(Some(AutoStack::default()));
        } else {
            self.edit_layout(|l| {
                l.frame = true;
                l.stack = Some(AutoStack::default());
            });
        }
        self.status = "Auto layout · resize the frame to see it respond".into();
    }

    pub fn insert_layout_frame(&mut self, name: &str, width: f32, height: f32) {
        let Some(layer) = self.vector_target() else {
            return;
        };
        let right = self.doc.layers[layer]
            .kind
            .shapes()
            .map(|shapes| {
                shapes
                    .iter()
                    .filter(|s| s.layout.parent.is_none())
                    .map(|s| s.world_bbox().max.x)
                    .fold(0.0f32, f32::max)
            })
            .unwrap_or(0.0);
        let mut shape = layout::make_frame(
            Pt::new(if right > 0.0 { right + 64.0 } else { 48.0 }, 48.0),
            Pt::new(width, height),
        );
        shape.name = name.into();
        let id = shape.id;
        self.commit(Cmd::AddShape { layer, shape });
        self.selection = vec![(layer, id)];
        self.tool = Tool::Select;
        self.need_fit = true;
        self.status = format!("{name} · {width:.0} × {height:.0}");
    }

    pub fn resize_layout_frame(&mut self, width: f32, height: f32) {
        let Some((layer, id)) = self.selected_frame() else {
            return;
        };
        let Some(frame) = self.doc.find_shape(layer, id) else {
            return;
        };
        self.set_layout_frame_bounds(
            layer,
            id,
            Bounds::from_min_size(frame.geom.bbox().min, Pt::new(width, height)),
        );
    }

    pub fn set_layout_frame_bounds(&mut self, layer: usize, id: u64, bounds: Bounds) {
        let Some(before) = self.doc.layers[layer].kind.shapes().map(|s| s.to_vec()) else {
            return;
        };
        let orig: Vec<_> = before
            .iter()
            .map(|s| (layer, s.id, s.geom.clone()))
            .collect();
        if let Some(frame) = self.doc.find_shape_mut(layer, id) {
            layout::set_bounds(&mut frame.geom, bounds);
        }
        layout::apply_resize(&mut self.doc, &orig, &[(layer, id)]);
        let after = self.doc.layers[layer].kind.shapes().unwrap().to_vec();
        self.doc.layers[layer]
            .kind
            .shapes_mut()
            .unwrap()
            .clone_from(&before);
        self.commit(Cmd::SetVectorShapes {
            layer,
            before,
            after,
        });
    }

    pub fn reparent_layout_selection(&mut self, parent: Option<u64>) {
        self.reparent_layout_selection_after(parent, Vec::new());
    }

    /// Include an already-applied drag in the same undo transaction. Reflow
    /// must happen after nesting, otherwise the old stack snaps the drag back.
    pub(crate) fn reparent_layout_selection_after(
        &mut self,
        parent: Option<u64>,
        mut preceding: Vec<Cmd>,
    ) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        let bakes_mask = parent.is_some()
            && self.selection.iter().any(|&(li, id)| {
                id == RASTER_ID && self.doc.layers.get(li).is_some_and(|l| l.mask.is_some())
            });

        // The hierarchy stores document coordinates, with frame rotations
        // inherited during rendering. A change of parent therefore needs a
        // coordinate-space conversion as well as a new parent id.
        fn ancestors(
            doc: &Document,
            li: usize,
            mut id: Option<u64>,
        ) -> Result<Vec<&Shape>, String> {
            let mut result: Vec<&Shape> = Vec::new();
            while let Some(current) = id {
                if result.len() >= 64 || result.iter().any(|s| s.id == current) {
                    return Err("Cannot nest objects in a cyclic frame hierarchy".into());
                }
                let shape = doc
                    .find_shape(li, current)
                    .ok_or("The frame no longer exists")?;
                if !shape.visible || shape.locked {
                    return Err("Show and unlock the object and its containing frames first".into());
                }
                result.push(shape);
                id = shape.layout.parent;
            }
            Ok(result)
        }

        let prepare = || -> Result<(Vec<Cmd>, Vec<(usize, u64)>), String> {
            let target = parent
                .map(|id| {
                    self.doc
                        .layers
                        .iter()
                        .enumerate()
                        .find_map(|(li, layer)| {
                            layer.find(id).filter(|s| s.layout.frame).map(|_| li)
                        })
                        .ok_or("Choose an existing frame to receive these objects")
                })
                .transpose()?;
            if let Some(li) = target {
                if !self.doc.layer_editable(li) {
                    return Err("Show and unlock the destination frame's layer first".into());
                }
                ancestors(&self.doc, li, parent)?;
            }

            let selected: HashSet<_> = self.selection.iter().copied().collect();
            let mut roots = Vec::new();
            for &(li, id) in &self.selection {
                if !self.doc.layer_editable(li) {
                    return Err("Show and unlock the source object's layer first".into());
                }
                if roots.contains(&(li, id)) {
                    continue;
                }
                if id == RASTER_ID {
                    if self.doc.layers[li].kind.pixels().is_none() {
                        return Err("The selected image no longer exists".into());
                    }
                    roots.push((li, id));
                    continue;
                }
                let chain = ancestors(&self.doc, li, Some(id))?;
                // Selecting a frame and one of its children must retain the
                // child's relationship, rather than flattening both roots.
                if chain.iter().skip(1).any(|s| selected.contains(&(li, s.id))) {
                    continue;
                }
                roots.push((li, id));
            }
            roots.sort_by_key(|&(li, id)| {
                (
                    li,
                    self.doc.layers[li]
                        .kind
                        .shapes()
                        .and_then(|shapes| shapes.iter().position(|s| s.id == id))
                        .unwrap_or(0),
                )
            });

            let mut after = self.doc.layout_snapshot();
            let mut removed_rasters = Vec::new();
            let mut selection = Vec::new();
            for (li, id) in roots {
                let destination = target.unwrap_or(li);
                let mut subtree = if id == RASTER_ID {
                    if parent.is_none() {
                        selection.push((li, id));
                        continue;
                    }
                    let layer = &self.doc.layers[li];
                    if !layer.kind.is_placed_raster() {
                        return Err(
                            "Place this paint layer as an image before moving it into a frame"
                                .into(),
                        );
                    }
                    let pixels = layer
                        .kind
                        .pixels()
                        .ok_or("The selected image no longer exists")?;
                    let image = image::RgbaImage::from_raw(
                        pixels.w,
                        pixels.h,
                        raster_pixels_for_frame(layer)?,
                    )
                    .ok_or("The selected image has invalid pixel data")?;
                    let mut bytes = std::io::Cursor::new(Vec::new());
                    image::DynamicImage::ImageRgba8(image)
                        .write_to(&mut bytes, image::ImageFormat::Png)
                        .map_err(|e| format!("Cannot prepare the image for the frame: {e}"))?;
                    let mut fill = crate::layout_images::from_bytes(bytes.get_ref())?;
                    fill.fit = crate::layout_images::ImageFit::Stretch;
                    let (origin, size, rotation) = layer.kind.raster_xform().unwrap();
                    let mut shape = Shape::new(
                        Geom::Rect {
                            origin,
                            size,
                            radius: 0.0,
                        },
                        Style {
                            fill: Fill::None,
                            stroke: None,
                        },
                    );
                    shape.name = layer.name.clone();
                    shape.rotation = rotation;
                    shape.opacity = layer.opacity;
                    shape.blend = layer.blend;
                    shape.filters = layer.filters.clone();
                    shape.layout.image = Some(fill);
                    removed_rasters.push(li);
                    vec![shape]
                } else {
                    let root = self
                        .doc
                        .find_shape(li, id)
                        .ok_or("The selected object no longer exists")?;
                    let ids: HashSet<_> = std::iter::once(id)
                        .chain(layout::descendants(&self.doc, li, id))
                        .collect();
                    if target == Some(li) && parent.is_some_and(|p| ids.contains(&p)) {
                        return Err(
                            "A frame cannot be moved into itself or one of its children".into()
                        );
                    }
                    if (target == Some(li) && root.layout.parent == parent)
                        || (parent.is_none() && root.layout.parent.is_none())
                    {
                        selection.push((li, id));
                        continue;
                    }
                    let shapes = self.doc.layers[li]
                        .kind
                        .shapes()
                        .ok_or("Choose a vector object or placed image")?;
                    let moving: Vec<_> = shapes
                        .iter()
                        .filter(|s| ids.contains(&s.id))
                        .cloned()
                        .collect();
                    after.layers[li]
                        .kind
                        .shapes_mut()
                        .unwrap()
                        .retain(|s| !ids.contains(&s.id));
                    moving
                };

                let root_id = if id == RASTER_ID { subtree[0].id } else { id };
                let root_index = subtree.iter().position(|s| s.id == root_id).unwrap();
                let root = &subtree[root_index];
                let centre = root.geom.bbox().center();
                let mut world = centre;
                let mut rotation = 0.0;
                if id != RASTER_ID {
                    for frame in ancestors(&self.doc, li, root.layout.parent)? {
                        world = world.rotate_about(frame.geom.bbox().center(), frame.rotation);
                        rotation += frame.rotation;
                    }
                }
                let target_chain = ancestors(&self.doc, destination, parent)?;
                for frame in target_chain.into_iter().rev() {
                    world = world.rotate_about(frame.geom.bbox().center(), -frame.rotation);
                    rotation -= frame.rotation;
                }
                let delta = world - centre;
                for shape in &mut subtree {
                    shape.geom.translate(delta);
                }
                subtree[root_index].rotation += rotation;
                subtree[root_index].layout.parent = parent;
                after.layers[destination]
                    .kind
                    .shapes_mut()
                    .ok_or("Choose a frame on a vector layer")?
                    .extend(subtree);
                selection.push((destination, root_id));
            }

            layout::validate_hierarchy(&after)?;
            let mut commands: Vec<_> = self
                .doc
                .layers
                .iter()
                .zip(&after.layers)
                .enumerate()
                .filter_map(|(layer, (before, after))| {
                    let (before, after) = (before.kind.shapes()?, after.kind.shapes()?);
                    (before != after).then(|| Cmd::SetVectorShapes {
                        layer,
                        before: before.to_vec(),
                        after: after.to_vec(),
                    })
                })
                .collect();
            removed_rasters.sort_unstable();
            removed_rasters.dedup();
            for &index in removed_rasters.iter().rev() {
                commands.push(Cmd::RemoveLayer {
                    index,
                    layer: self.doc.layers[index].clone(),
                });
            }
            for (li, _) in &mut selection {
                *li -= removed_rasters
                    .iter()
                    .filter(|removed| **removed < *li)
                    .count();
            }
            Ok((commands, selection))
        };

        match prepare() {
            Ok((commands, selection)) => {
                preceding.extend(commands);
                if !preceding.is_empty() {
                    self.commit(Cmd::Batch(preceding));
                }
                self.active_layer = selection.first().map(|&(li, _)| li);
                self.selected_layer = None;
                self.selection = selection;
                self.status = if bakes_mask {
                    "Moved into frame · mask baked into image alpha; Undo restores the editable mask"
                } else if parent.is_some() {
                    "Moved into frame"
                } else {
                    "Moved out of frame"
                }
                .into();
                true
            }
            Err(error) => {
                self.status = error;
                false
            }
        }
    }

    pub fn reorder_layout_selection(&mut self, forward: bool) {
        let Some((layer, id)) = self.primary() else {
            return;
        };
        let Some(before) = self.doc.layers[layer].kind.shapes().map(|s| s.to_vec()) else {
            return;
        };
        let Some(shape) = before.iter().find(|s| s.id == id) else {
            return;
        };
        let siblings: Vec<_> = before
            .iter()
            .filter(|s| s.layout.parent == shape.layout.parent)
            .map(|s| s.id)
            .collect();
        let Some(i) = siblings.iter().position(|s| *s == id) else {
            return;
        };
        let target = if forward {
            siblings.get(i + 1)
        } else {
            i.checked_sub(1).and_then(|i| siblings.get(i))
        };
        let Some(target) = target else { return };
        let mut after = before.clone();
        let a = after.iter().position(|s| s.id == id).unwrap();
        let b = after.iter().position(|s| s.id == *target).unwrap();
        after.swap(a, b);
        self.commit(Cmd::SetVectorShapes {
            layer,
            before,
            after,
        });
    }
}

#[cfg(test)]
#[path = "layout_reparent_tests.rs"]
mod reparent_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Studio, u64, u64) {
        let mut s = Studio::new();
        s.doc = Document::new("Layout editing", 1.0, 1.0, 96.0);
        s.doc.layers = vec![Layer::vector("Design")];
        s.active_layer = Some(0);
        s.show_welcome = false;
        s.persona = Persona::Layout;
        let frame = layout::make_frame(Pt::new(20.0, 20.0), Pt::new(200.0, 180.0));
        let root = frame.id;
        let mut child = Shape::new(
            Geom::Rect {
                origin: Pt::new(40.0, 50.0),
                size: Pt::new(80.0, 30.0),
                radius: 0.0,
            },
            Style::default(),
        );
        child.layout.parent = Some(root);
        let id = child.id;
        s.doc.layers[0]
            .kind
            .shapes_mut()
            .unwrap()
            .extend([frame, child]);
        s.selection = vec![(0, root)];
        (s, root, id)
    }

    #[test]
    fn complete_frame_duplicate_delete_and_paste_are_atomic() {
        let (mut s, root, child) = setup();
        let original = s.doc.layers[0].kind.shapes().unwrap().to_vec();
        s.duplicate_selection();
        let copy = s.selection[0].1;
        assert_ne!(copy, root);
        assert_eq!(layout::children(&s.doc, 0, copy).len(), 1);
        s.undo();
        assert_eq!(s.doc.layers[0].kind.shapes().unwrap(), original);
        s.selection = vec![(0, root)];
        s.copy_selection(&egui::Context::default());
        s.paste_clipboard(None);
        let pasted = s.selection[0].1;
        assert_ne!(pasted, root);
        assert_eq!(layout::children(&s.doc, 0, pasted).len(), 1);
        s.undo();
        assert_eq!(s.doc.layers[0].kind.shapes().unwrap(), original);
        s.selection = vec![(0, root)];
        s.delete_selection();
        assert!(s.doc.find_shape(0, child).is_none());
        s.undo();
        assert_eq!(s.doc.layers[0].kind.shapes().unwrap(), original);
        assert!(s.doc.validate_hierarchy().is_ok());
    }

    #[test]
    fn keyboard_frame_movement_and_inspector_resize_keep_children() {
        let (mut s, root, child) = setup();
        let before = s.doc.find_shape(0, child).unwrap().geom.bbox();
        s.nudge(16.0, 12.0);
        assert_eq!(
            s.doc.find_shape(0, child).unwrap().geom.bbox().min,
            before.min + Pt::new(16.0, 12.0)
        );
        s.undo();
        assert_eq!(s.doc.find_shape(0, child).unwrap().geom.bbox(), before);
        s.doc.find_shape_mut(0, child).unwrap().layout.constraint_x = layout::Constraint::Stretch;
        s.set_layout_frame_bounds(
            0,
            root,
            Bounds::from_min_size(Pt::new(20.0, 20.0), Pt::new(300.0, 180.0)),
        );
        assert_eq!(
            s.doc.find_shape(0, child).unwrap().geom.bbox().width(),
            180.0
        );
        s.undo();
        assert_eq!(s.doc.find_shape(0, child).unwrap().geom.bbox(), before);
    }

    #[test]
    fn clipping_and_tree_paint_order_drive_pointer_selection() {
        let (mut s, root, child) = setup();
        // A frame appended after its children is still painted behind them.
        s.doc.layers[0].kind.shapes_mut().unwrap().swap(0, 1);
        assert_eq!(s.doc.hit_test(Pt::new(60.0, 60.0), 0.0), Some((0, child)));
        s.doc
            .find_shape_mut(0, child)
            .unwrap()
            .geom
            .translate(Pt::new(200.0, 0.0));
        assert_eq!(s.doc.hit_test(Pt::new(260.0, 60.0), 0.0), None);
        s.doc.find_shape_mut(0, root).unwrap().layout.clip = false;
        assert_eq!(s.doc.hit_test(Pt::new(260.0, 60.0), 0.0), Some((0, child)));
    }

    #[test]
    fn pasting_into_another_document_preserves_color_without_dangling_variables() {
        use crate::layout_tokens::{DesignToken, TokenProperty};
        let (mut source, _, child) = setup();
        source.selection = vec![(0, child)];
        let color = Rgba::rgb(214, 64, 24);
        let token = DesignToken::color("Accent", color);
        let token_id = token.id;
        source.set_layout_tokens(vec![token]);
        source.bind_layout_token(TokenProperty::Fill, Some(token_id));
        let payload = format!(
            "{}{}",
            Studio::CLIP_PREFIX,
            serde_json::to_string(&vec![source.doc.find_shape(0, child).unwrap()]).unwrap()
        );
        let (mut destination, _, _) = setup();
        destination.paste_clipboard(Some(&payload));
        let pasted = destination
            .doc
            .find_shape(0, destination.selection[0].1)
            .unwrap();
        assert_eq!(pasted.style.fill, Fill::Solid(color));
        assert_eq!(pasted.layout.tokens.fill, None);
        assert_eq!(pasted.layout.parent, None);
        let saved = crate::project::encode(&destination.doc).unwrap();
        assert!(
            crate::project::decode(&saved)
                .unwrap()
                .validate_hierarchy()
                .is_ok()
        );
    }

    #[test]
    fn drawing_uses_pointer_frame_and_paths_remain_nested() {
        let (mut studio, root, _) = setup();
        studio.finish_create(
            CreateKind::Ellipse,
            Pt::new(60.0, 90.0),
            Pt::new(90.0, 120.0),
        );
        let ellipse = studio.selection[0].1;
        assert_eq!(
            studio.doc.find_shape(0, ellipse).unwrap().layout.parent,
            Some(root)
        );
        studio.selection = vec![(0, root)];
        studio.finish_create(
            CreateKind::Frame,
            Pt::new(400.0, 40.0),
            Pt::new(600.0, 200.0),
        );
        let outside = studio.selection[0].1;
        assert_eq!(
            studio.doc.find_shape(0, outside).unwrap().layout.parent,
            None
        );
        studio.finish_pen(
            vec![
                Anchor::corner(Pt::new(50.0, 100.0)),
                Anchor::corner(Pt::new(100.0, 140.0)),
            ],
            false,
            None,
        );
        let path = studio.selection[0].1;
        assert_eq!(
            studio.doc.find_shape(0, path).unwrap().layout.parent,
            Some(root)
        );
        studio.undo();
        assert!(studio.doc.find_shape(0, path).is_none());
        assert!(studio.doc.validate_hierarchy().is_ok());
    }

    #[test]
    fn variable_edit_and_delete_undo_restore_binding_and_appearance() {
        use crate::layout_tokens::{DesignToken, TokenProperty, TokenValue};
        let (mut s, _, child) = setup();
        s.selection = vec![(0, child)];
        let token = DesignToken::color("Brand", Rgba::rgb(200, 20, 10));
        let id = token.id;
        s.set_layout_tokens(vec![token]);
        s.bind_layout_token(TokenProperty::Fill, Some(id));
        let mut updated = s.doc.layout_tokens.clone();
        updated[0].value = TokenValue::Color(Rgba::rgb(20, 180, 100));
        s.set_layout_tokens(updated);
        assert_eq!(
            s.doc.find_shape(0, child).unwrap().style.fill,
            Fill::Solid(Rgba::rgb(20, 180, 100))
        );
        s.undo();
        assert_eq!(
            s.doc.find_shape(0, child).unwrap().style.fill,
            Fill::Solid(Rgba::rgb(200, 20, 10))
        );
        s.remove_layout_token(id);
        assert!(
            s.doc
                .find_shape(0, child)
                .unwrap()
                .layout
                .tokens
                .fill
                .is_none()
        );
        s.undo();
        assert_eq!(
            s.doc.find_shape(0, child).unwrap().layout.tokens.fill,
            Some(id)
        );
        let encoded = crate::project::encode(&s.doc).unwrap();
        assert!(
            crate::project::decode(&encoded)
                .unwrap()
                .validate_hierarchy()
                .is_ok()
        );
    }
}
