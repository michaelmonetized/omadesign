//! Editable layer groups and object/frame sidebar moves.
use super::*;

/// Visual sidebar placement; document vectors use bottom-to-top painter order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreePlacement {
    Above,
    Below,
    Inside,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn studio() -> Studio {
        let mut studio = Studio::new();
        studio.doc = Document::new("Tree", 400., 400., 96.);
        studio.doc.layers = vec![Layer::vector("Artwork"), Layer::vector("Frames")];
        studio.active_layer = Some(0);
        studio.selection.clear();
        studio.selected_layer = None;
        studio.history = History::default();
        studio
    }
    fn rect(x: f32) -> Shape {
        Shape::new(
            Geom::Rect {
                origin: Pt::new(x, 20.),
                size: Pt::new(30., 40.),
                radius: 0.,
            },
            Style::default(),
        )
    }
    fn state(s: &Studio) -> String {
        serde_json::to_string(&s.doc.layers).unwrap()
    }
    fn points(doc: &Document, li: usize, id: u64) -> Vec<Pt> {
        let shape = doc.find_shape(li, id).unwrap();
        let mut points: Vec<_> = shape.world_contours(12).into_iter().flatten().collect();
        for frame in shape_ancestors(doc, li, id) {
            for p in &mut points {
                *p = p.rotate_about(frame.geom.bbox().center(), frame.rotation);
            }
        }
        points
    }
    fn same_points(a: &[Pt], b: &[Pt]) {
        assert_eq!(a.len(), b.len());
        for (a, b) in a.iter().zip(b) {
            assert!(
                (a.x - b.x).abs() < 0.002 && (a.y - b.y).abs() < 0.002,
                "{a:?} != {b:?}"
            );
        }
    }

    #[test]
    fn real_layer_group_preserves_objects_roundtrip_and_single_undo() {
        let mut s = studio();
        let objects = vec![rect(0.), rect(50.), rect(100.)];
        let ids: Vec<_> = objects.iter().map(|s| s.id).collect();
        *s.doc.layers[0].kind.shapes_mut().unwrap() = objects.clone();
        s.selection = vec![(0, ids[0]), (0, ids[2])];
        let before = state(&s);
        s.group_selected();
        assert_eq!(s.history.len(), 1);
        let group_id = s.selected_layer.unwrap();
        let group = s.doc.layers.iter().find(|l| l.id == group_id).unwrap();
        assert!(group.is_group && group.pass_through);
        let child = s
            .doc
            .layers
            .iter()
            .find(|l| l.parent == Some(group_id))
            .unwrap();
        assert_eq!(
            child.kind.shapes().unwrap(),
            &[objects[0].clone(), objects[2].clone()]
        );
        assert!(s.doc.validate_hierarchy().is_ok());
        let saved: Document =
            serde_json::from_str(&serde_json::to_string(&s.doc).unwrap()).unwrap();
        assert!(saved.validate_hierarchy().is_ok());
        let grouped = state(&s);
        s.undo();
        assert_eq!(state(&s), before);
        assert!(
            s.selection
                .iter()
                .all(|&(li, id)| li == 0 && ids.contains(&id))
        );
        s.redo();
        assert_eq!(state(&s), grouped);
        s.selected_layer = Some(group_id);
        s.ungroup_selected();
        assert!(!s.doc.layers.iter().any(|l| l.id == group_id));
        assert!(s.doc.layers.iter().all(|l| l.parent.is_none()));
        s.undo();
        assert_eq!(state(&s), grouped);
    }

    #[test]
    fn layer_drop_nests_unnests_and_rejects_cycles_and_locked_descendants() {
        let mut s = studio();
        let group = Layer::group("Group");
        let group_id = group.id;
        s.doc.layers.push(group);
        let before = state(&s);
        assert!(s.drop_layer(0, 2, TreePlacement::Inside));
        let child = s
            .doc
            .layers
            .iter()
            .position(|l| l.parent == Some(group_id))
            .unwrap();
        let group = s.doc.layers.iter().position(|l| l.id == group_id).unwrap();
        assert!(!s.drop_layer(group, child, TreePlacement::Inside));
        assert!(s.doc.validate_hierarchy().is_ok());
        s.undo();
        assert_eq!(state(&s), before);
        s.redo();
        let child = s
            .doc
            .layers
            .iter()
            .position(|l| l.parent == Some(group_id))
            .unwrap();
        let group = s.doc.layers.iter().position(|l| l.id == group_id).unwrap();
        assert!(s.drop_layer(child, group, TreePlacement::Above));
        assert!(s.doc.layers.iter().all(|l| l.parent.is_none()));
        s.undo();
        let child = s
            .doc
            .layers
            .iter()
            .position(|l| l.parent == Some(group_id))
            .unwrap();
        s.doc.layers[child].locked = true;
        let group = s.doc.layers.iter().position(|l| l.id == group_id).unwrap();
        let sibling = s
            .doc
            .layers
            .iter()
            .position(|l| l.name == "Frames")
            .unwrap();
        assert!(!s.drop_layer(group, sibling, TreePlacement::Above));
    }

    #[test]
    fn rotated_frame_subtree_cross_layer_drop_and_detach_preserve_world_coordinates() {
        let mut s = studio();
        let mut outer = crate::layout::make_frame(Pt::ZERO, Pt::splat(250.));
        outer.rotation = 0.4;
        let mut nested = crate::layout::make_frame(Pt::splat(40.), Pt::splat(100.));
        nested.layout.parent = Some(outer.id);
        nested.rotation = 0.2;
        let mut child = rect(65.);
        child.layout.parent = Some(nested.id);
        let mut destination = crate::layout::make_frame(Pt::new(250., 80.), Pt::splat(120.));
        destination.rotation = -0.5;
        let (root, leaf, target) = (nested.id, child.id, destination.id);
        *s.doc.layers[0].kind.shapes_mut().unwrap() = vec![outer, nested, child];
        s.doc.layers[1].kind.shapes_mut().unwrap().push(destination);
        let before = state(&s);
        let root_points = points(&s.doc, 0, root);
        let child_points = points(&s.doc, 0, leaf);
        assert!(s.drop_objects(
            &[(0, root), (0, leaf)],
            1,
            Some(target),
            TreePlacement::Inside
        ));
        assert_eq!(s.history.len(), 1);
        assert_eq!(s.doc.find_shape(1, leaf).unwrap().layout.parent, Some(root));
        same_points(&root_points, &points(&s.doc, 1, root));
        same_points(&child_points, &points(&s.doc, 1, leaf));
        s.undo();
        assert_eq!(state(&s), before);
        s.redo();
        assert!(s.drop_objects(&[(1, root)], 0, None, TreePlacement::Inside));
        same_points(&root_points, &points(&s.doc, 0, root));
        same_points(&child_points, &points(&s.doc, 0, leaf));
        assert_eq!(s.doc.find_shape(0, root).unwrap().layout.parent, None);
        assert!(s.doc.validate_hierarchy().is_ok());
    }

    #[test]
    fn multi_object_drop_keeps_order_and_selection_through_undo() {
        let mut s = studio();
        let objects = vec![rect(0.), rect(50.), rect(100.), rect(150.)];
        let ids: Vec<_> = objects.iter().map(|s| s.id).collect();
        *s.doc.layers[0].kind.shapes_mut().unwrap() = objects;
        s.selection = vec![(0, ids[2]), (0, ids[0])];
        let before = state(&s);
        assert!(s.drop_objects(&s.selection.clone(), 0, Some(ids[3]), TreePlacement::Above));
        assert_eq!(
            s.doc.layers[0]
                .kind
                .shapes()
                .unwrap()
                .iter()
                .map(|s| s.id)
                .collect::<Vec<_>>(),
            vec![ids[1], ids[3], ids[0], ids[2]]
        );
        s.undo();
        assert_eq!(state(&s), before);
        assert!(s.drop_objects(&s.selection.clone(), 1, None, TreePlacement::Inside));
        assert!(s.selection.iter().all(|s| s.0 == 1));
        s.undo();
        assert_eq!(state(&s), before);
        assert!(s.selection.iter().all(|s| s.0 == 0));
        s.redo();
        assert!(s.selection.iter().all(|s| s.0 == 1));
    }
}

fn reorder_commands(order: &mut Vec<u64>, desired: &[u64], commands: &mut Vec<Cmd>) {
    for (to, id) in desired.iter().enumerate() {
        if order[to] != *id {
            let from = order.iter().position(|v| v == id).unwrap();
            order.remove(from);
            order.insert(to, *id);
            commands.push(Cmd::ReorderLayer { from, to });
        }
    }
}

fn shape_ancestors(doc: &Document, layer: usize, id: u64) -> Vec<&Shape> {
    let mut result = Vec::new();
    let mut parent = doc.find_shape(layer, id).and_then(|s| s.layout.parent);
    for _ in 0..64 {
        let Some(shape) = parent.and_then(|id| doc.find_shape(layer, id)) else {
            break;
        };
        if result.iter().any(|s: &&Shape| s.id == shape.id) {
            break;
        }
        result.push(shape);
        parent = shape.layout.parent;
    }
    result
}

impl Studio {
    /// Move whole frame subtrees, retaining world coordinates and stable object ids.
    pub fn drop_objects(
        &mut self,
        source: &[(usize, u64)],
        target_layer: usize,
        target: Option<u64>,
        place: TreePlacement,
    ) -> bool {
        if source.is_empty()
            || self.op.is_some()
            || !self.layer_unlocked(target_layer)
            || !self.doc.layer_visible(target_layer)
        {
            return false;
        }
        let target_shape = target.and_then(|id| self.doc.find_shape(target_layer, id));
        if target.is_some() && target_shape.is_none() {
            return false;
        }
        let parent = if place == TreePlacement::Inside {
            if let Some(shape) = target_shape {
                if !shape.layout.frame || shape.locked || !shape.visible {
                    return false;
                }
                Some(shape.id)
            } else {
                None
            }
        } else {
            target_shape.and_then(|s| s.layout.parent)
        };
        if let Some(shape) = target_shape {
            if shape.locked
                || shape_ancestors(&self.doc, target_layer, shape.id)
                    .iter()
                    .any(|s| s.locked)
            {
                return false;
            }
        }
        let mut after = self.doc.layout_snapshot();
        let mut commands = Vec::new();
        let destination = if self.doc.layers[target_layer].is_group {
            let mut child = Layer::vector("Objects");
            child.parent = Some(self.doc.layers[target_layer].id);
            // Insert at the group marker after vector updates, so their indices stay stable.
            after.layers.push(child);
            after.layers.len() - 1
        } else {
            if self.doc.layers[target_layer].kind.shapes().is_none() {
                return false;
            }
            target_layer
        };
        let selected: HashSet<_> = source.iter().copied().collect();
        let mut roots: Vec<_> = source
            .iter()
            .copied()
            .filter(|&(li, id)| {
                !shape_ancestors(&self.doc, li, id)
                    .iter()
                    .any(|s| selected.contains(&(li, s.id)))
            })
            .collect();
        roots.sort_by_key(|&(li, id)| {
            (
                li,
                self.doc
                    .layers
                    .get(li)
                    .and_then(|l| l.kind.shapes())
                    .and_then(|s| s.iter().position(|s| s.id == id))
                    .unwrap_or(0),
            )
        });
        roots.dedup();
        let mut moving = Vec::new();
        let mut root_ids = Vec::new();
        for (li, id) in roots {
            if !self.layer_unlocked(li) {
                return false;
            }
            let Some(shape) = self.doc.find_shape(li, id) else {
                return false;
            };
            let ids: HashSet<_> = std::iter::once(id)
                .chain(crate::layout::descendants(&self.doc, li, id))
                .collect();
            if target.is_some_and(|id| li == target_layer && ids.contains(&id)) {
                return false;
            }
            if shape_ancestors(&self.doc, li, id)
                .iter()
                .any(|s| s.locked || !s.visible)
            {
                return false;
            }
            let mut subtree: Vec<_> = self.doc.layers[li]
                .kind
                .shapes()
                .unwrap()
                .iter()
                .filter(|s| ids.contains(&s.id))
                .cloned()
                .collect();
            if subtree.iter().any(|s| s.locked || !s.visible) {
                return false;
            }
            let centre = shape.geom.bbox().center();
            let mut world = centre;
            let mut rotation = 0.0;
            for frame in shape_ancestors(&self.doc, li, id) {
                world = world.rotate_about(frame.geom.bbox().center(), frame.rotation);
                rotation += frame.rotation;
            }
            let mut target_chain = parent
                .map(|id| {
                    let mut v = vec![self.doc.find_shape(target_layer, id).unwrap()];
                    v.extend(shape_ancestors(&self.doc, target_layer, id));
                    v
                })
                .unwrap_or_default();
            target_chain.reverse();
            for frame in target_chain {
                if frame.locked {
                    return false;
                }
                world = world.rotate_about(frame.geom.bbox().center(), -frame.rotation);
                rotation -= frame.rotation;
            }
            for item in &mut subtree {
                item.geom.translate(world - centre);
            }
            let root = subtree.iter_mut().find(|s| s.id == id).unwrap();
            root.rotation += rotation;
            root.layout.parent = parent;
            after.layers[li]
                .kind
                .shapes_mut()
                .unwrap()
                .retain(|s| !ids.contains(&s.id));
            root_ids.push(id);
            moving.extend(subtree);
        }
        let shapes = after.layers[destination].kind.shapes_mut().unwrap();
        let insertion = if let Some(target) = target.filter(|_| place != TreePlacement::Inside) {
            let Some(index) = shapes.iter().position(|s| s.id == target) else {
                return false;
            };
            index + usize::from(place == TreePlacement::Above)
        } else {
            shapes.len()
        };
        shapes.splice(insertion..insertion, moving);
        if crate::layout::validate_hierarchy(&after).is_err() {
            return false;
        }
        for (li, layer) in self.doc.layers.iter().enumerate() {
            if let (Some(before), Some(next)) =
                (layer.kind.shapes(), after.layers[li].kind.shapes())
            {
                if before != next {
                    commands.push(Cmd::SetVectorShapes {
                        layer: li,
                        before: before.to_vec(),
                        after: next.to_vec(),
                    });
                }
            }
        }
        let destination = if destination == self.doc.layers.len() {
            commands.push(Cmd::AddLayer {
                index: target_layer,
                layer: after.layers.pop().unwrap(),
            });
            target_layer
        } else {
            destination
        };
        if commands.is_empty() {
            return false;
        }
        self.commit_type_edit();
        self.commit(Cmd::Batch(commands));
        self.selected_layer = None;
        self.selection = root_ids.into_iter().map(|id| (destination, id)).collect();
        self.active_layer = Some(destination);
        self.layer_expanded.insert(self.doc.layers[destination].id);
        for ancestor in self.doc.layer_ancestors(destination) {
            self.layer_expanded.insert(self.doc.layers[ancestor].id);
        }
        self.status = "Objects moved".into();
        true
    }

    pub fn can_drop_layer(&self, source: usize, target: usize, place: TreePlacement) -> bool {
        source != target
            && self.op.is_none()
            && self.layer_unlocked(source)
            && self.layer_unlocked(target)
            && !self.layer_tree_indices(source).contains(&target)
            && self
                .layer_tree_indices(source)
                .iter()
                .all(|&i| !self.doc.layers[i].locked)
            && (place != TreePlacement::Inside || self.doc.layers[target].is_group)
            && self.doc.validate_hierarchy().is_ok()
    }

    pub fn drop_layer(&mut self, source: usize, target: usize, place: TreePlacement) -> bool {
        if !self.can_drop_layer(source, target, place) {
            return false;
        }
        let source_tree = self.layer_tree_indices(source);
        let target_tree = self.layer_tree_indices(target);
        let parent = if place == TreePlacement::Inside {
            Some(self.doc.layers[target].id)
        } else {
            self.doc.layers[target].parent
        };
        let mut order: Vec<_> = self.doc.layers.iter().map(|l| l.id).collect();
        let moved: Vec<_> = source_tree.iter().map(|&i| order[i]).collect();
        let anchor = match place {
            TreePlacement::Above | TreePlacement::Inside => self.doc.layers[target].id,
            TreePlacement::Below => self.doc.layers[target_tree[0]].id,
        };
        let mut desired = order.clone();
        desired.retain(|id| !moved.contains(id));
        let insertion = desired.iter().position(|id| *id == anchor).unwrap()
            + usize::from(place == TreePlacement::Above);
        desired.splice(insertion..insertion, moved);
        let mut commands = Vec::new();
        if self.doc.layers[source].parent != parent {
            commands.push(Cmd::SetLayerParent {
                index: source,
                before: self.doc.layers[source].parent,
                after: parent,
            });
        }
        reorder_commands(&mut order, &desired, &mut commands);
        if commands.is_empty() {
            return false;
        }
        self.commit_type_edit();
        self.commit(Cmd::Batch(commands));
        if let Some(parent) = parent {
            self.layer_expanded.insert(parent);
        }
        self.status = "Layer moved".into();
        true
    }

    pub fn group_selected(&mut self) {
        if self.op.is_some() {
            return;
        }
        self.commit_type_edit();
        let mut whole = Vec::new();
        let mut pieces: Vec<(usize, Vec<Shape>, Vec<Shape>)> = Vec::new();
        if let Some(index) = self
            .selected_layer
            .and_then(|id| self.doc.layers.iter().position(|l| l.id == id))
        {
            whole.push(index);
        } else {
            let mut layers: Vec<_> = self.selection.iter().map(|s| s.0).collect();
            layers.sort_unstable();
            layers.dedup();
            for li in layers {
                if self.selection.contains(&(li, RASTER_ID)) {
                    whole.push(li);
                    continue;
                }
                let Some(shapes) = self.doc.layers[li].kind.shapes() else {
                    continue;
                };
                let mut ids: HashSet<_> = self
                    .selection
                    .iter()
                    .filter(|s| s.0 == li)
                    .map(|s| s.1)
                    .collect();
                for id in ids.clone() {
                    ids.extend(crate::layout::descendants(&self.doc, li, id));
                }
                let (mut moving, remaining): (Vec<_>, Vec<_>) =
                    shapes.iter().cloned().partition(|s| ids.contains(&s.id));
                if moving.is_empty() {
                    continue;
                }
                if moving.iter().any(|s| s.locked || !s.visible) {
                    self.status =
                        "Unlock selected objects and their children before grouping".into();
                    return;
                }
                if remaining.is_empty() {
                    whole.push(li);
                } else {
                    // Extracted roots no longer inherit their enclosing frame transforms.
                    let roots: Vec<_> = moving
                        .iter()
                        .filter(|s| s.layout.parent.is_some_and(|id| !ids.contains(&id)))
                        .map(|s| s.id)
                        .collect();
                    for root_id in roots {
                        let shape = self.doc.find_shape(li, root_id).unwrap();
                        let centre = shape.geom.bbox().center();
                        let mut point = centre;
                        let mut rotation = 0.0;
                        for frame in shape_ancestors(&self.doc, li, root_id) {
                            if frame.locked {
                                self.status = "Unlock the containing frame before grouping".into();
                                return;
                            }
                            point = point.rotate_about(frame.geom.bbox().center(), frame.rotation);
                            rotation += frame.rotation;
                        }
                        let descendants = crate::layout::descendants(&self.doc, li, root_id);
                        for item in &mut moving {
                            if item.id == root_id || descendants.contains(&item.id) {
                                item.geom.translate(point - centre);
                            }
                            if item.id == root_id {
                                item.rotation += rotation;
                                item.layout.parent = None;
                            }
                        }
                    }
                    pieces.push((li, moving, remaining));
                }
            }
        }
        let mut sources = whole.clone();
        sources.extend(pieces.iter().map(|p| p.0));
        if sources.is_empty() {
            self.status = "Select objects or a layer to group".into();
            return;
        }
        if sources.iter().any(|&i| !self.layer_unlocked(i))
            || whole.iter().any(|&i| {
                self.layer_tree_indices(i)
                    .iter()
                    .any(|&j| self.doc.layers[j].locked)
            })
        {
            self.status = "Unlock the layers before grouping".into();
            return;
        }
        // Find the nearest common layer group and insert after the affected branches.
        let mut candidates: Vec<_> = self
            .doc
            .layer_ancestors(sources[0])
            .into_iter()
            .map(|i| Some(self.doc.layers[i].id))
            .collect();
        candidates.push(None);
        let parent = candidates
            .into_iter()
            .find(|candidate| {
                candidate.is_none_or(|id| {
                    sources.iter().all(|&i| {
                        self.doc
                            .layer_ancestors(i)
                            .iter()
                            .any(|&a| self.doc.layers[a].id == id)
                    })
                })
            })
            .flatten();
        let insertion_top = sources
            .iter()
            .map(|&i| {
                let mut branch = i;
                for ancestor in self.doc.layer_ancestors(i) {
                    if Some(self.doc.layers[ancestor].id) == parent {
                        break;
                    }
                    branch = ancestor;
                }
                branch
            })
            .max()
            .unwrap();
        let mut group = Layer::group("Group");
        group.parent = parent;
        let group_id = group.id;
        let mut commands = vec![Cmd::AddLayer {
            index: self.doc.layers.len(),
            layer: group,
        }];
        let mut order: Vec<_> = self.doc.layers.iter().map(|l| l.id).collect();
        order.push(group_id);
        let mut children: Vec<(usize, Vec<u64>)> = Vec::new();
        for &li in &whole {
            commands.push(Cmd::SetLayerParent {
                index: li,
                before: parent,
                after: Some(group_id),
            });
            children.push((
                li,
                self.layer_tree_indices(li)
                    .iter()
                    .map(|&i| self.doc.layers[i].id)
                    .collect(),
            ));
        }
        for (li, moving, remaining) in pieces {
            let mut child = self.doc.layers[li].clone();
            child.id = crate::document::next_id();
            child.name = if moving.len() == 1 {
                moving[0].name.clone()
            } else {
                "Objects".into()
            };
            child.parent = Some(group_id);
            *child.kind.shapes_mut().unwrap() = moving;
            commands.push(Cmd::SetVectorShapes {
                layer: li,
                before: self.doc.layers[li].kind.shapes().unwrap().to_vec(),
                after: remaining,
            });
            children.push((li, vec![child.id]));
            commands.push(Cmd::AddLayer {
                index: order.len(),
                layer: child.clone(),
            });
            order.push(child.id);
        }
        children.sort_by_key(|p| p.0);
        let moved: Vec<_> = children.iter().flat_map(|p| p.1.iter().copied()).collect();
        let top = insertion_top;
        let mut desired: Vec<_> = self.doc.layers[..=top]
            .iter()
            .map(|l| l.id)
            .filter(|id| !moved.contains(id))
            .collect();
        desired.extend(moved.iter().copied());
        desired.push(group_id);
        desired.extend(
            self.doc.layers[top + 1..]
                .iter()
                .map(|l| l.id)
                .filter(|id| !moved.contains(id)),
        );
        reorder_commands(&mut order, &desired, &mut commands);
        self.commit(Cmd::Batch(commands));
        let index = self
            .doc
            .layers
            .iter()
            .position(|l| l.id == group_id)
            .unwrap();
        self.layer_expanded.insert(group_id);
        self.activate_layer_tree(index);
        self.status = "Grouped · Ctrl+Shift+G to ungroup".into();
    }

    pub fn ungroup_selected(&mut self) {
        let index = self
            .selected_layer
            .and_then(|id| {
                self.doc
                    .layers
                    .iter()
                    .position(|l| l.id == id && l.is_group)
            })
            .or_else(|| {
                self.primary()
                    .and_then(|(li, _)| self.doc.layer_ancestors(li).first().copied())
            })
            .or_else(|| {
                self.active_layer
                    .filter(|&i| self.doc.layers.get(i).is_some_and(|l| l.is_group))
            });
        let Some(index) = index else {
            self.status = "Select a layer group to ungroup".into();
            return;
        };
        if !self.layer_unlocked(index)
            || self
                .layer_tree_indices(index)
                .iter()
                .any(|&i| self.doc.layers[i].locked)
        {
            self.status = "Unlock the group and its layers before ungrouping".into();
            return;
        }
        let group = self.doc.layers[index].clone();
        let mut commands = Vec::new();
        for (i, layer) in self.doc.layers.iter().enumerate() {
            if layer.parent == Some(group.id) {
                commands.push(Cmd::SetLayerParent {
                    index: i,
                    before: layer.parent,
                    after: group.parent,
                });
            }
        }
        if self.doc.layers.len() == 1 {
            commands.push(Cmd::AddLayer {
                index: 1,
                layer: Layer::vector("Layer 1"),
            });
        }
        commands.push(Cmd::RemoveLayer {
            index,
            layer: group,
        });
        self.commit(Cmd::Batch(commands));
        self.selected_layer = None;
        self.active_layer = self
            .selection
            .first()
            .map(|s| s.0)
            .or_else(|| self.doc.layers.len().checked_sub(1));
        self.status = "Group released".into();
    }
}
