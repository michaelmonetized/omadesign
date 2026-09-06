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
    fn selectable_objects(&self) -> Vec<(usize, u64)> {
        self.doc
            .layers
            .iter()
            .enumerate()
            .filter(|(_, l)| l.visible && !l.locked)
            .flat_map(|(li, l)| {
                if let Some(shapes) = l.kind.shapes() {
                    shapes
                        .iter()
                        .filter(|s| {
                            s.visible && !s.locked && (!s.guide || self.doc.ruler.guides_visible)
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
            let color = shape.style.stroke.as_ref().unwrap().color;
            let outlined_style = Style {
                fill: Fill::Solid(color),
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
