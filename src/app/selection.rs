use super::*;

#[derive(Clone, Copy)]
pub enum Same {
    Fill,
    Stroke,
    Effects,
}
#[derive(Clone, Copy)]
pub enum With {
    Fill,
    Stroke,
    Effects,
    NoFill,
    NoStroke,
    NoEffects,
}

impl Studio {
    pub fn delete_object_item(&mut self, layer: usize, id: u64) {
        if !self.layer_unlocked(layer) || self.doc.find_shape(layer, id).is_none_or(|s| s.locked) {
            return;
        }
        self.enter_group_item((layer, id));
        self.delete_objects();
    }

    /// Explicit outlining may discard live text; entering the Node tool must not.
    pub fn convert_object_to_path(&mut self, layer: usize, id: u64) {
        if !self.doc.layer_editable(layer)
            || !self.doc.find_shape(layer, id).is_some_and(|shape| {
                shape.visible && !shape.locked && (!shape.guide || self.doc.ruler.guides_visible)
            })
        {
            self.status = "Select an unlocked vector object to convert".into();
            return;
        }
        self.commit_type_edit();
        self.end_deform(false);
        let Some(shape) = self.doc.find_shape(layer, id) else {
            return;
        };
        if let Geom::Text(run) = &shape.geom {
            if run.contours.is_empty() {
                self.status = "This text has no outlines to convert".into();
                return;
            }
            // Keep every glyph and counter, plus the existing bounds/rotation pivot.
            self.commit(Cmd::SetGeom {
                layer,
                id,
                before: shape.geom.clone(),
                after: Geom::Poly {
                    contours: run.contours.clone(),
                    winding: false,
                },
                rot_before: shape.rotation,
                rot_after: shape.rotation,
            });
            self.status = "Text converted to paths · Undo restores editable text".into();
        } else {
            self.ensure_path(layer, id);
        }
        self.node_sel.clear();
        self.reset_snap_gesture();
    }

    fn selectable_objects(&self) -> Vec<(usize, u64)> {
        self.doc
            .layers
            .iter()
            .enumerate()
            .filter(|(li, _)| self.doc.layer_editable(*li))
            .flat_map(|(li, l)| {
                if let Some(shapes) = l.kind.shapes() {
                    shapes
                        .iter()
                        .filter(|s| {
                            s.visible
                                && !s.locked
                                && (!s.guide
                                    || (self.doc.ruler.guides_visible
                                        && !self.doc.ruler.guides_locked))
                        })
                        .map(|s| (li, s.id))
                        .collect()
                } else if l.kind.is_placed_raster() {
                    vec![(li, RASTER_ID)]
                } else {
                    vec![]
                }
            })
            .collect()
    }

    fn selected_objects(&mut self, objects: Vec<(usize, u64)>) {
        self.selected_layer = None;
        self.individual_object = None;
        self.end_deform(true);
        self.reset_snap_gesture();
        self.selection = objects;
        self.node_sel.clear();
        self.artboard_sel.clear();
        self.status = format!("{} objects selected", self.selection.len());
    }

    pub fn select_all(&mut self) {
        self.selected_objects(self.selectable_objects());
    }
    pub fn deselect_all(&mut self) {
        self.selected_objects(vec![]);
        if self.pixel_sel.is_some() {
            self.set_pixel_sel(None);
        }
    }
    pub fn invert_selection(&mut self) {
        let selected: HashSet<_> = self.selection.iter().copied().collect();
        self.selected_objects(
            self.selectable_objects()
                .into_iter()
                .filter(|s| !selected.contains(s))
                .collect(),
        );
    }

    pub fn select_same(&mut self, property: Same) {
        let Some(primary) = self.primary() else {
            self.status = "Select a reference object first".into();
            return;
        };
        let candidates = self.selectable_objects();
        if !candidates.contains(&primary) {
            return;
        }
        let reference = self.doc.find_shape(primary.0, primary.1);
        let reference_fx = reference
            .map(|s| &s.filters)
            .unwrap_or(&self.doc.layers[primary.0].filters);
        let mut matching: Vec<_> = candidates
            .into_iter()
            .filter(|(li, id)| {
                let shape = self.doc.find_shape(*li, *id);
                match property {
                    Same::Fill => reference
                        .zip(shape)
                        .is_some_and(|(a, b)| a.style.fill == b.style.fill),
                    Same::Stroke => reference
                        .zip(shape)
                        .is_some_and(|(a, b)| a.style.stroke == b.style.stroke),
                    Same::Effects => {
                        reference_fx
                            == shape
                                .map(|s| &s.filters)
                                .unwrap_or(&self.doc.layers[*li].filters)
                    }
                }
            })
            .collect();
        if let Some(index) = matching.iter().position(|item| *item == primary) {
            matching.swap(0, index);
        }
        self.selected_objects(matching);
    }

    pub fn select_with(&mut self, property: With) {
        let matching = self
            .selectable_objects()
            .into_iter()
            .filter(|(li, id)| {
                let shape = self.doc.find_shape(*li, *id);
                match property {
                    With::Fill => shape.is_some_and(|s| !s.style.fill.is_none()),
                    With::NoFill => shape.is_some_and(|s| s.style.fill.is_none()),
                    With::Stroke => shape.is_some_and(|s| {
                        s.style
                            .stroke
                            .as_ref()
                            .is_some_and(|stroke| stroke.width > 0.0)
                    }),
                    With::NoStroke => shape.is_some_and(|s| {
                        s.style
                            .stroke
                            .as_ref()
                            .is_none_or(|stroke| stroke.width <= 0.0)
                    }),
                    With::Effects => shape
                        .map(|s| &s.filters)
                        .unwrap_or(&self.doc.layers[*li].filters)
                        .active(),
                    With::NoEffects => !shape
                        .map(|s| &s.filters)
                        .unwrap_or(&self.doc.layers[*li].filters)
                        .active(),
                }
            })
            .collect();
        self.selected_objects(matching);
    }

    pub fn expand_strokes(&mut self) {
        let selected: HashSet<_> = self.selection.iter().copied().collect();
        let mut shapes: Vec<_> = self
            .selectable_objects()
            .into_iter()
            .filter(|p| selected.contains(p))
            .filter_map(|(li, id)| {
                let layer = self.doc.layers[li].kind.shapes()?;
                let index = layer.iter().position(|s| s.id == id)?;
                Some((li, index, layer[index].clone()))
            })
            .collect();
        // Insert above each original without shifting the remaining source indices.
        shapes.sort_by_key(|(li, index, _)| std::cmp::Reverse((*li, *index)));
        let mut lengths: Vec<_> = self
            .doc
            .layers
            .iter()
            .map(|l| l.kind.shapes().map_or(0, |shapes| shapes.len()))
            .collect();
        let mut commands = vec![];
        let mut selection = vec![];
        for (layer, index, shape) in shapes {
            let Some(geom) = crate::outline::expand(&shape) else {
                continue;
            };
            let stroke = shape.style.stroke.as_ref().unwrap();
            let mut painted = shape.clone();
            painted.style.fill = stroke
                .gradient
                .clone()
                .map(Fill::Gradient)
                .unwrap_or(Fill::Solid(stroke.color));
            let outlined_style = Style {
                fill: Self::compound_fill(&painted, &geom, 0.0),
                stroke: None,
            };
            if shape.style.fill.is_none() || !shape.geom.is_closed() {
                commands.push(Cmd::SetGeom {
                    layer,
                    id: shape.id,
                    before: shape.geom.clone(),
                    after: geom,
                    rot_before: shape.rotation,
                    rot_after: 0.0,
                });
                commands.push(Cmd::SetStyle {
                    layer,
                    id: shape.id,
                    before: shape.style.clone(),
                    after: outlined_style,
                });
                selection.push((layer, shape.id));
            } else {
                let mut fill_style = shape.style.clone();
                fill_style.stroke = None;
                commands.push(Cmd::SetStyle {
                    layer,
                    id: shape.id,
                    before: shape.style.clone(),
                    after: fill_style,
                });
                let mut outline = shape.clone();
                outline.id = crate::document::next_id();
                outline.name = format!("{} · outline", shape.name);
                outline.geom = geom;
                outline.style = outlined_style;
                outline.rotation = 0.0;
                outline.corners = [0.0; 4];
                selection.extend([(layer, shape.id), (layer, outline.id)]);
                commands.push(Cmd::AddShape {
                    layer,
                    shape: outline,
                });
                commands.push(Cmd::ReorderShape {
                    layer,
                    from: lengths[layer],
                    to: index + 1,
                });
                lengths[layer] += 1;
            }
        }
        if commands.is_empty() {
            self.status = "Select objects with a visible stroke".into();
            return;
        }
        self.commit(Cmd::Batch(commands));
        self.selected_objects(selection);
        self.status = "Expanded to filled outlines".into();
    }

    pub fn selection_has_path(&self) -> bool {
        self.selection.iter().any(|(layer, id)| {
            self.doc.find_shape(*layer, *id).is_some_and(|shape| {
                matches!(
                    shape.geom,
                    Geom::Path { .. } | Geom::Paths { .. } | Geom::Poly { .. }
                )
            })
        })
    }

    pub fn simplify_selection(&mut self) {
        let targets: Vec<_> = self
            .selection
            .iter()
            .copied()
            .filter(|(layer, id)| {
                self.doc.find_shape(*layer, *id).is_some_and(|shape| {
                    matches!(
                        shape.geom,
                        Geom::Path { .. } | Geom::Paths { .. } | Geom::Poly { .. }
                    )
                })
            })
            .collect();
        if targets.is_empty() {
            self.status = "Select a path to simplify".into();
            return;
        }
        let mut commands = Vec::new();
        for (layer, id) in targets {
            let Some(shape) = self.doc.find_shape(layer, id) else {
                continue;
            };
            let mut after = shape.geom.clone();
            crate::geom::simplify_geom(&mut after, 1.25);
            if after != shape.geom {
                commands.push(Cmd::SetGeom {
                    layer,
                    id,
                    before: shape.geom.clone(),
                    after,
                    rot_before: shape.rotation,
                    rot_after: shape.rotation,
                });
            }
        }
        if commands.is_empty() {
            self.status = "Already simple".into();
            return;
        }
        self.commit(Cmd::Batch(commands));
        self.status = "Simplified".into();
    }

    pub fn pathfinder(&mut self, operation: BoolOp) {
        self.pathfinder_operation(Some(operation));
    }
    pub fn divide_selection(&mut self) {
        self.pathfinder_operation(None);
    }

    fn pathfinder_operation(&mut self, operation: Option<BoolOp>) {
        let eligible: HashSet<_> = self.selectable_objects().into_iter().collect();
        let selection: Vec<_> = self
            .selection
            .iter()
            .copied()
            .filter(|s| eligible.contains(s) && s.1 != RASTER_ID)
            .collect();
        if selection.len() < 2 {
            self.status = "Select at least two vector objects".into();
            return;
        }
        let layer = selection[0].0;
        if selection.iter().any(|(li, _)| *li != layer) {
            self.status = "Pathfinder needs objects on the same layer".into();
            return;
        }
        let source = self.doc.layers[layer].kind.shapes().unwrap();
        let shapes: Vec<_> = selection
            .iter()
            .filter_map(|(_, id)| source.iter().find(|s| s.id == *id).cloned())
            .collect();
        if shapes.iter().any(|shape| shape.guide) && shapes.iter().any(|shape| !shape.guide) {
            self.status = "Pathfinder needs only artwork or only guides".into();
            return;
        }
        let geoms: Vec<_> = shapes
            .iter()
            .map(|s| Geom::Poly {
                contours: s.world_contours(96),
                winding: matches!(s.geom, Geom::Poly { winding: true, .. }),
            })
            .collect();
        let results = match operation {
            Some(op) => boolean::apply_many(op, &geoms).into_iter().collect(),
            None => boolean::divide(&geoms),
        };
        let name = operation.map_or("Divide", BoolOp::name);
        let ids: HashSet<_> = shapes.iter().map(|s| s.id).collect();
        let insert = source.iter().position(|s| ids.contains(&s.id)).unwrap();
        let mut order: Vec<_> = source.iter().map(|s| s.id).collect();
        // Put removed originals at the end first. Reversing this batch restores
        // their exact original stacking positions, even across unselected objects.
        let mut commands = vec![Cmd::SetMotion {
            before: self.doc.motion.clone(),
            after: self.doc.motion.clone(),
        }];
        for shape in &shapes {
            let from = order.iter().position(|id| *id == shape.id).unwrap();
            let to = order.len() - 1;
            commands.push(Cmd::ReorderShape { layer, from, to });
            order.remove(from);
            order.push(shape.id);
        }
        commands.push(Cmd::RemoveShapes {
            layer,
            shapes: shapes.clone(),
        });
        let remaining = order.len() - shapes.len();
        let mut selected = vec![];
        for (index, geom) in results.into_iter().enumerate() {
            let mut shape = shapes[0].clone();
            shape.id = crate::document::next_id();
            shape.name = if operation.is_none() {
                format!("Divide · {}", index + 1)
            } else {
                name.into()
            };
            shape.geom = geom;
            shape.rotation = 0.0;
            shape.corners = [0.0; 4];
            selected.push((layer, shape.id));
            commands.push(Cmd::AddShape { layer, shape });
            commands.push(Cmd::ReorderShape {
                layer,
                from: remaining + index,
                to: insert + index,
            });
        }
        self.commit(Cmd::Batch(commands));
        self.selected_objects(selected);
        self.status = format!(
            "{name} · {} {}",
            self.selection.len(),
            if self.selection.len() == 1 {
                "piece"
            } else {
                "pieces"
            }
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn shape(x: f32) -> Shape {
        Shape::new(
            Geom::Rect {
                origin: Pt::new(x, 10.0),
                size: Pt::splat(20.0),
                radius: 0.0,
            },
            Style::default(),
        )
    }
    #[test]
    fn matching_uses_full_styles_and_skips_hidden_locked_children_and_parents() {
        let mut s = Studio::new();
        s.doc.layers = vec![
            Layer::vector("Objects"),
            Layer::vector("Locked"),
            Layer::vector("Hidden"),
        ];
        let mut a = shape(0.0);
        let mut b = shape(30.0);
        let mut c = shape(60.0);
        a.style.fill = Fill::Linear {
            from: [0.0, 0.0],
            to: [1.0, 0.0],
            c0: Rgba::BLACK,
            c1: Rgba::WHITE,
        };
        b.style = a.style.clone();
        c.style = a.style.clone();
        if let Fill::Linear { to, .. } = &mut c.style.fill {
            *to = [0.0, 1.0];
        }
        c.style.stroke.as_mut().unwrap().width = 8.0;
        c.filters.items.push(crate::filter::Fx::Blur { std: 4.0 });
        let primary = a.id;
        let matching = b.id;
        let different = c.id;
        let mut hidden = a.clone();
        hidden.id = crate::document::next_id();
        hidden.visible = false;
        let mut locked = a.clone();
        locked.id = crate::document::next_id();
        locked.locked = true;
        *s.doc.layers[0].kind.shapes_mut().unwrap() = vec![a.clone(), b, c, hidden, locked];
        s.doc.layers[1].kind.shapes_mut().unwrap().push(a.clone());
        s.doc.layers[1].locked = true;
        s.doc.layers[2].kind.shapes_mut().unwrap().push(a);
        s.doc.layers[2].visible = false;
        s.selection = vec![(0, primary)];
        s.select_same(Same::Fill);
        assert_eq!(s.selection, vec![(0, primary), (0, matching)]);
        s.select_same(Same::Stroke);
        assert_eq!(s.selection, vec![(0, primary), (0, matching)]);
        s.select_with(With::Effects);
        assert_eq!(s.selection, vec![(0, different)]);
        s.select_same(Same::Effects);
        assert_eq!(s.selection, vec![(0, different)]);
        s.invert_selection();
        assert_eq!(s.selection, vec![(0, primary), (0, matching)]);
        s.select_all();
        assert_eq!(s.selection.len(), 3);
        s.deselect_all();
        assert!(s.selection.is_empty());
    }
    #[test]
    fn expand_keeps_fill_stacking_and_restores_editable_strokes_in_one_undo() {
        let mut s = Studio::new();
        s.doc.layers = vec![Layer::vector("Shapes")];
        let a = shape(0.0);
        let middle = shape(30.0);
        let mut b = shape(60.0);
        b.style.fill = Fill::None;
        b.rotation = 0.4;
        let original = vec![a.clone(), middle.clone(), b.clone()];
        *s.doc.layers[0].kind.shapes_mut().unwrap() = original.clone();
        s.selection = vec![(0, a.id), (0, b.id)];
        s.expand_strokes();
        let current = s.doc.layers[0].kind.shapes().unwrap();
        assert_eq!(current.len(), 4);
        assert_eq!(current[0].id, a.id);
        assert_eq!(current[2].id, middle.id);
        assert_eq!(current[3].id, b.id);
        assert_eq!(current[0].style.fill, a.style.fill);
        assert!(current[0].style.stroke.is_none());
        assert!(matches!(current[1].geom, Geom::Poly { winding: true, .. }));
        assert!(current[3].style.stroke.is_none());
        assert_eq!(s.history.len(), 1);
        s.undo();
        assert_eq!(*s.doc.layers[0].kind.shapes().unwrap(), original);
        s.redo();
        assert_eq!(s.doc.layers[0].kind.shapes().unwrap().len(), 4);
    }
    #[test]
    fn pathfinder_uses_world_geometry_and_restores_original_z_order_in_one_undo() {
        let mut s = Studio::new();
        s.doc.layers = vec![Layer::vector("Shapes")];
        let mut a = shape(0.0);
        a.rotation = std::f32::consts::FRAC_PI_4;
        let between = shape(100.0);
        let b = shape(10.0);
        let end = shape(130.0);
        let original = vec![a.clone(), between, b.clone(), end];
        *s.doc.layers[0].kind.shapes_mut().unwrap() = original.clone();
        s.selection = vec![(0, b.id), (0, a.id)];
        s.divide_selection();
        assert_eq!(s.history.len(), 1);
        assert!(s.selection.len() >= 3);
        let pieces: Vec<_> = s
            .selection
            .iter()
            .map(|(li, id)| s.doc.find_shape(*li, *id).unwrap().geom.clone())
            .collect();
        let expected = boolean::apply(
            BoolOp::Union,
            &Geom::Poly {
                contours: a.world_contours(96),
                winding: false,
            },
            &b.geom,
        )
        .unwrap();
        assert!(
            (pieces.iter().map(boolean::area).sum::<f32>() - boolean::area(&expected)).abs() < 0.05
        );
        s.undo();
        assert_eq!(*s.doc.layers[0].kind.shapes().unwrap(), original);
        s.redo();
        assert!(s.doc.layers[0].kind.shapes().unwrap().len() > original.len());
    }
}

#[cfg(test)]
mod text_outline_tests {
    use super::*;

    #[test]
    fn explicit_text_outlining_preserves_all_glyphs_holes_and_restores_live_type() {
        for (content, px, tracking) in [("BO8", 58.0, 0.0), ("OO", 80.0, -30.0)] {
            let mut geometry = Geom::Text(TypeRun {
                origin: Pt::new(45.0, 120.0),
                content: content.into(),
                px,
                tracking,
                ..TypeRun::default()
            });
            crate::text::fill_contours(&mut geometry);
            let Geom::Text(run) = &geometry else {
                unreachable!()
            };
            assert!(
                run.contours.len() >= if content == "BO8" { 6 } else { 4 },
                "fixture must have multiple glyphs and counters"
            );
            let mut original = Shape::new(
                geometry,
                Style {
                    fill: Fill::Linear {
                        from: [-0.1, 0.0],
                        to: [1.0, 0.8],
                        c0: Rgba::rgb(220, 40, 80),
                        c1: Rgba::rgb(30, 110, 230),
                    },
                    stroke: None,
                },
            );
            original.rotation = 31.0_f32.to_radians();
            let id = original.id;
            let mut studio = Studio::new();
            studio.doc = Document::new("Outlined type", 240.0, 200.0, 96.0);
            studio.doc.layers[1]
                .kind
                .shapes_mut()
                .unwrap()
                .push(original.clone());
            studio.selection = vec![(1, id)];
            studio.begin_type_edit((1, id), Pt::new(50.0, 110.0));
            let before = compositor::export_png(&studio.doc, 1).unwrap();
            studio.convert_object_to_path(1, id);
            assert!(studio.type_edit.is_none());
            assert_eq!(studio.history.len(), 1);
            assert!(studio.can_flip_selection());
            let outlined = studio.doc.find_shape(1, id).unwrap().clone();
            let Geom::Poly { contours, winding } = &outlined.geom else {
                panic!("all-contour outline")
            };
            let Geom::Text(run) = &original.geom else {
                unreachable!()
            };
            assert!(
                !*winding,
                "preserve live text's even-odd overlapping-glyph appearance"
            );
            assert_eq!(contours, &run.contours);
            assert_eq!(outlined.style, original.style);
            assert_eq!(outlined.rotation, original.rotation);
            assert_eq!(outlined.geom.bbox(), original.geom.bbox());
            assert_eq!(compositor::export_png(&studio.doc, 1).unwrap(), before);
            let reopened =
                crate::project::decode(&crate::project::encode(&studio.doc).unwrap()).unwrap();
            assert_eq!(reopened.find_shape(1, id), Some(&outlined));
            assert_eq!(compositor::export_png(&reopened, 1).unwrap(), before);
            studio.undo();
            assert_eq!(studio.doc.find_shape(1, id), Some(&original));
            studio.redo();
            assert_eq!(studio.doc.find_shape(1, id), Some(&outlined));
        }
    }
}

#[cfg(test)]
mod stack_tests {
    use super::*;
    use crate::geom::Pt;

    fn shape(x: f32) -> Shape {
        Shape::new(
            Geom::Rect {
                origin: Pt::new(x, 10.0),
                size: Pt::new(20.0, 20.0),
                radius: 0.0,
            },
            Style::default(),
        )
    }

    fn ids(studio: &Studio, layer: usize) -> Vec<u64> {
        studio.doc.layers[layer]
            .kind
            .shapes()
            .unwrap()
            .iter()
            .map(|shape| shape.id)
            .collect()
    }

    #[test]
    fn bring_forward_and_send_backward_cross_layers() {
        let mut studio = Studio::new();
        studio.doc.layers.push(Layer::vector("Layer 2"));
        let lower = shape(10.0);
        let upper = shape(40.0);
        let lower_id = lower.id;
        let upper_id = upper.id;
        studio.doc.layers[1].kind.shapes_mut().unwrap().push(lower);
        studio.doc.layers[2].kind.shapes_mut().unwrap().push(upper);
        studio.selection = vec![(1, lower_id)];
        studio.bring_forward();
        assert_eq!(ids(&studio, 1), Vec::<u64>::new());
        assert_eq!(ids(&studio, 2), vec![lower_id, upper_id]);
        assert_eq!(studio.selection, vec![(2, lower_id)]);
        studio.send_backward();
        assert_eq!(ids(&studio, 1), vec![lower_id]);
        assert_eq!(ids(&studio, 2), vec![upper_id]);
        studio.selection = vec![(1, lower_id)];
        studio.bring_to_front();
        assert_eq!(ids(&studio, 2), vec![upper_id, lower_id]);
        studio.undo();
        assert_eq!(ids(&studio, 1), vec![lower_id]);
        assert_eq!(ids(&studio, 2), vec![upper_id]);
    }

    #[test]
    fn a_locked_layer_is_skipped() {
        let mut studio = Studio::new();
        let mut locked = Layer::vector("Locked");
        locked.locked = true;
        studio.doc.layers.push(locked);
        studio.doc.layers.push(Layer::vector("Above"));
        let lower = shape(10.0);
        let above = shape(80.0);
        let lower_id = lower.id;
        let above_id = above.id;
        studio.doc.layers[1].kind.shapes_mut().unwrap().push(lower);
        studio.doc.layers[3].kind.shapes_mut().unwrap().push(above);
        studio.selection = vec![(1, lower_id)];
        studio.bring_forward();
        assert!(ids(&studio, 2).is_empty());
        assert_eq!(ids(&studio, 3), vec![lower_id, above_id]);
    }

    #[test]
    fn free_transform_scales_rotates_and_shears_in_one_undo() {
        let mut studio = Studio::new();
        let rect = shape(20.0);
        let id = rect.id;
        studio.doc.layers[1].kind.shapes_mut().unwrap().push(rect);
        let pixels = crate::document::Pixels::from_rgba(2, 2, [200, 20, 20, 255].repeat(4)).unwrap();
        let image = studio.doc.layers.len();
        studio.doc.layers.push(Layer::placed_raster(
            "photo",
            pixels,
            Pt::new(80.0, 30.0),
            Pt::new(40.0, 40.0),
        ));
        studio.selection = vec![(1, id), (image, RASTER_ID)];
        let history = studio.history.len();
        studio.free_transform();
        let before = studio.doc.find_shape(1, id).unwrap().geom.clone();
        let Geom::Rect { origin, size, .. } = before else {
            panic!("rect");
        };
        studio
            .doc
            .find_shape_mut(1, id)
            .unwrap()
            .geom
            .map_into(
                crate::geom::Bounds::from_min_size(origin, size),
                crate::geom::Bounds::from_min_size(origin, size * 2.0),
            );
        studio.doc.find_shape_mut(1, id).unwrap().rotation = 0.4;
        studio.doc.layers[image]
            .kind
            .set_raster_xform(Pt::new(90.0, 40.0), Pt::new(80.0, 50.0), 0.3);
        studio.shear_selection(true, 16.0);
        studio.finish_free_transform(false);
        assert_eq!(studio.history.len(), history + 1);
        assert_ne!(studio.doc.find_shape(1, id).unwrap().geom, before);
        assert!((studio.doc.layers[image].kind.raster_shear() - 16.0).abs() < 0.01);
        let (origin, size, rotation) = studio.doc.layers[image].kind.raster_xform().unwrap();
        assert!((origin.x - 90.0).abs() < 0.1 && (size.x - 80.0).abs() < 0.1);
        assert!((rotation - 0.3).abs() < 0.01);
        studio.undo();
        assert_eq!(studio.doc.find_shape(1, id).unwrap().geom, before);
        assert!(studio.doc.layers[image].kind.raster_shear().abs() < 0.01);
        let (origin, size, rotation) = studio.doc.layers[image].kind.raster_xform().unwrap();
        assert!((origin.x - 80.0).abs() < 0.1 && (size.x - 40.0).abs() < 0.1);
        assert!(rotation.abs() < 0.01);
        studio.redo();
        assert!((studio.doc.layers[image].kind.raster_shear() - 16.0).abs() < 0.01);
        studio.free_transform();
        studio.shear_selection(true, 8.0);
        let dirty = studio.history.len();
        studio.finish_free_transform(true);
        assert_eq!(studio.history.len(), dirty);
        assert!((studio.doc.layers[image].kind.raster_shear() - 16.0).abs() < 0.01);
    }
}
