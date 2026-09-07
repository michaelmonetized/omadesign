//! Layer tree operations retain group descendants in painter order and history.

use super::*;

impl Studio {
    pub fn layer_ancestors_unlocked(&self, index: usize) -> bool {
        self.doc
            .layer_ancestors(index)
            .iter()
            .all(|&i| !self.doc.layers[i].locked)
    }

    pub fn layer_unlocked(&self, index: usize) -> bool {
        self.doc.layers.get(index).is_some_and(|l| !l.locked)
            && self.layer_ancestors_unlocked(index)
    }

    pub fn layer_tree_indices(&self, index: usize) -> Vec<usize> {
        if index >= self.doc.layers.len() {
            return vec![];
        }
        (0..self.doc.layers.len())
            .filter(|&i| i == index || self.doc.layer_ancestors(i).contains(&index))
            .collect()
    }

    pub fn layer_sibling(&self, index: usize, up: bool) -> Option<usize> {
        let parent = self.doc.layers.get(index)?.parent;
        let siblings = self
            .doc
            .layers
            .iter()
            .enumerate()
            .filter(|(_, l)| l.parent == parent);
        if up {
            siblings.filter(|(i, _)| *i > index).map(|(i, _)| i).min()
        } else {
            siblings.filter(|(i, _)| *i < index).map(|(i, _)| i).max()
        }
    }

    /// Map a hovered descendant to a sibling of the dragged layer. Reordering
    /// never reparents layers; a group's children travel with its marker.
    pub fn layer_drop_target(&self, source: usize, hovered: usize) -> Option<usize> {
        let parent = self.doc.layers.get(source)?.parent;
        let mut target = hovered;
        for _ in 0..64 {
            let layer = self.doc.layers.get(target)?;
            if layer.parent == parent {
                return (target != source).then_some(target);
            }
            target = self
                .doc
                .layers
                .iter()
                .position(|l| Some(l.id) == layer.parent)?;
            if target == source {
                return None;
            }
        }
        None
    }

    pub fn move_layer_tree(&mut self, index: usize, up: bool) {
        if let Some(sibling) = self.layer_sibling(index, up) {
            self.reorder_layer_tree(index, sibling, up);
        }
    }

    pub fn move_layer_tree_extreme(&mut self, index: usize, up: bool) {
        let Some(layer) = self.doc.layers.get(index) else {
            return;
        };
        let siblings = self
            .doc
            .layers
            .iter()
            .enumerate()
            .filter(|(_, l)| l.parent == layer.parent)
            .map(|(i, _)| i);
        let target = if up {
            siblings.last()
        } else {
            siblings.into_iter().next()
        };
        if let Some(target) = target {
            self.reorder_layer_tree(index, target, up);
        }
    }

    /// Place a complete subtree above/below another sibling in painter order.
    /// The complete permutation is a single history entry, including a jump
    /// across several groups. Indices are remapped by apply_with_layer_selection.
    pub fn reorder_layer_tree(&mut self, index: usize, target: usize, above: bool) -> bool {
        if index == target || !self.layer_unlocked(index) || self.op.is_some() {
            return false;
        }
        let Some(layer) = self.doc.layers.get(index) else {
            return false;
        };
        if self
            .doc
            .layers
            .get(target)
            .is_none_or(|l| l.parent != layer.parent)
            || self.doc.validate_hierarchy().is_err()
        {
            return false;
        }
        let source = self.layer_tree_indices(index);
        let destination = self.layer_tree_indices(target);
        let contiguous = |indices: &[usize], marker| {
            indices.last() == Some(&marker) && indices.windows(2).all(|w| w[1] == w[0] + 1)
        };
        if !contiguous(&source, index) || !contiguous(&destination, target) {
            self.status = "This layer group has an inconsistent order and cannot be moved.".into();
            return false;
        }
        let start = source[0];
        let end = index + 1;
        let insertion = if above { target + 1 } else { destination[0] };
        if insertion >= start && insertion <= end {
            return false;
        }
        let mut order: Vec<_> = self.doc.layers.iter().map(|l| l.id).collect();
        let mut desired = order.clone();
        let moved: Vec<_> = desired.drain(start..end).collect();
        let to = insertion - if insertion > end { end - start } else { 0 };
        desired.splice(to..to, moved);
        let mut commands = Vec::new();
        for to in 0..order.len() {
            if order[to] == desired[to] {
                continue;
            }
            let from = order.iter().position(|id| *id == desired[to]).unwrap();
            let id = order.remove(from);
            order.insert(to, id);
            commands.push(Cmd::ReorderLayer { from, to });
        }
        if commands.is_empty() {
            return false;
        }
        let name = self.doc.layers[index].name.clone();
        self.commit_type_edit();
        self.reset_snap_gesture();
        self.commit(Cmd::Batch(commands));
        self.status = format!("{name} moved {}", if above { "up" } else { "down" });
        true
    }

    /// Layer references in the UI use indices; history records index moves too.
    /// Preserve their stable identities around both forward and inverse batches.
    pub(super) fn apply_with_layer_selection(&mut self, cmd: &Cmd) {
        fn changes_layer_indices(cmd: &Cmd) -> bool {
            match cmd {
                Cmd::ReorderLayer { .. } | Cmd::AddLayer { .. } | Cmd::RemoveLayer { .. } => true,
                Cmd::Batch(commands) => commands.iter().any(changes_layer_indices),
                _ => false,
            }
        }
        if !changes_layer_indices(cmd) {
            apply_cmd(&mut self.doc, cmd);
            return;
        }
        let id_at = |index: usize| self.doc.layers.get(index).map(|l| l.id);
        let active = self.active_layer.and_then(id_at);
        let objects: Vec<_> = self
            .selection
            .iter()
            .filter_map(|&(i, object)| id_at(i).map(|layer| (layer, object)))
            .collect();
        let typing = self.type_edit.as_ref().and_then(|e| id_at(e.layer));
        let rename = self.layer_rename.as_ref().and_then(|(i, _)| id_at(*i));
        let shape_rename = self.shape_rename.as_ref().and_then(|(i, _, _)| id_at(*i));
        apply_cmd(&mut self.doc, cmd);
        let indices: HashMap<_, _> = self
            .doc
            .layers
            .iter()
            .enumerate()
            .map(|(i, l)| (l.id, i))
            .collect();
        self.active_layer = active.and_then(|id| indices.get(&id).copied());
        self.selection = objects
            .into_iter()
            .filter_map(|(layer, object)| indices.get(&layer).map(|&i| (i, object)))
            .collect();
        if let Some(edit) = self.type_edit.as_mut()
            && let Some(i) = typing.and_then(|id| indices.get(&id))
        {
            edit.layer = *i;
        }
        if let Some((i, _)) = self.layer_rename.as_mut()
            && let Some(index) = rename.and_then(|id| indices.get(&id))
        {
            *i = *index;
        }
        if let Some((i, _, _)) = self.shape_rename.as_mut()
            && let Some(index) = shape_rename.and_then(|id| indices.get(&id))
        {
            *i = *index;
        }
        self.selected_layer = self.selected_layer.filter(|id| indices.contains_key(id));
    }

    pub fn delete_layer_tree(&mut self, index: usize) {
        if !self.layer_unlocked(index) {
            return;
        }
        let indices = self.layer_tree_indices(index);
        if indices.is_empty() {
            return;
        }
        if indices.iter().any(|&i| self.doc.layers[i].locked) {
            self.status = "Unlock the layers in this group before deleting it.".into();
            return;
        }
        let mut commands = Vec::new();
        // A document always retains an editable layer, including when its only
        // root group is removed. The replacement is part of the same undo step.
        if indices.len() == self.doc.layers.len() {
            commands.push(Cmd::AddLayer {
                index: self.doc.layers.len(),
                layer: Layer::vector("Layer 1"),
            });
        }
        for &i in indices.iter().rev() {
            commands.push(Cmd::RemoveLayer {
                index: i,
                layer: self.doc.layers[i].clone(),
            });
        }
        self.clear_layer_interaction();
        self.commit(Cmd::Batch(commands));
        self.active_layer = self.doc.layers.len().checked_sub(1);
        self.layer_expanded
            .retain(|id| self.doc.layers.iter().any(|l| l.id == *id));
        self.status = if indices.len() > 1 {
            "Group and its layers deleted"
        } else {
            "Layer deleted"
        }
        .into();
    }

    pub fn activate_layer_tree(&mut self, index: usize) {
        let Some(layer) = self.doc.layers.get(index) else {
            return;
        };
        let group = layer.is_group;
        let id = layer.id;
        let name = layer.name.clone();
        self.clear_layer_interaction();
        self.active_layer = Some(index);
        self.selected_layer = Some(id);
        if group {
            self.selection = self
                .layer_tree_indices(index)
                .into_iter()
                .filter(|&i| self.doc.layer_editable(i))
                .flat_map(|i| {
                    let layer = &self.doc.layers[i];
                    if let Some(shapes) = layer.kind.shapes() {
                        shapes
                            .iter()
                            .filter(|s| {
                                s.visible
                                    && !s.locked
                                    && (!s.guide || self.doc.ruler.guides_visible)
                            })
                            .map(|s| (i, s.id))
                            .collect()
                    } else if layer.kind.is_placed_raster() {
                        vec![(i, RASTER_ID)]
                    } else {
                        vec![]
                    }
                })
                .collect();
            self.status = format!("{name}: {} editable objects", self.selection.len());
        } else {
            self.status = name;
        }
    }

    /// Use this insertion point for a new layer while a group is active.
    pub fn new_layer_parent(&self) -> (usize, Option<u64>) {
        if let Some(i) = self.active_layer
            && let Some(layer) = self.doc.layers.get(i)
            && layer.is_group
            && self.layer_unlocked(i)
        {
            return (i, Some(layer.id));
        }
        (self.doc.layers.len(), None)
    }

    pub fn add_layer_group(&mut self) {
        let (index, parent) = self.new_layer_parent();
        let mut layer = Layer::group("Group");
        layer.parent = parent;
        let id = layer.id;
        self.clear_layer_interaction();
        self.commit(Cmd::AddLayer { index, layer });
        self.active_layer = Some(index);
        self.selected_layer = Some(id);
        self.layer_expanded.insert(id);
        for ancestor in self.doc.layer_ancestors(index) {
            self.layer_expanded.insert(self.doc.layers[ancestor].id);
        }
    }

    fn clear_layer_interaction(&mut self) {
        self.deselect_all();
        self.paint_mask = false;
        self.type_edit = None;
        self.layer_rename = None;
        self.shape_rename = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nested_studio() -> Studio {
        let mut studio = Studio::new();
        let group = Layer::group("Group");
        let mut nested = Layer::group("Nested");
        nested.parent = Some(group.id);
        let mut child = Layer::vector("Child");
        child.parent = Some(nested.id);
        studio.doc.layers = vec![child, nested, group, Layer::vector("Sibling")];
        studio.history = History::default();
        studio.active_layer = Some(2);
        studio
    }

    #[test]
    fn moving_nested_group_preserves_parentage_and_undo_order() {
        let mut studio = nested_studio();
        studio.active_layer = Some(0);
        studio.finish_create(CreateKind::Rect, Pt::ZERO, Pt::new(20., 20.));
        let object = studio.primary().unwrap().1;
        studio.history = History::default();
        studio.activate_layer_tree(2);
        let group_id = studio.selected_layer;
        let before: Vec<_> = studio.doc.layers.iter().map(|l| (l.id, l.parent)).collect();
        studio.move_layer_tree(2, true);
        assert_eq!(studio.doc.layers[0].name, "Sibling");
        assert_eq!(studio.doc.layers[3].name, "Group");
        assert_eq!(studio.selection, vec![(1, object)]);
        assert_eq!(studio.active_layer, Some(3));
        assert_eq!(studio.selected_layer, group_id);
        assert!(studio.doc.validate_hierarchy().is_ok());
        studio.undo();
        assert_eq!(studio.selection, vec![(0, object)]);
        assert_eq!(studio.active_layer, Some(2));
        assert_eq!(studio.selected_layer, group_id);
        assert_eq!(
            before,
            studio
                .doc
                .layers
                .iter()
                .map(|l| (l.id, l.parent))
                .collect::<Vec<_>>()
        );
        // The first child has no siblings; moving it cannot escape its group.
        studio.move_layer_tree(0, true);
        assert_eq!(studio.doc.layers[0].name, "Child");
    }

    #[test]
    fn deleting_group_removes_descendants_in_one_undo_step() {
        let mut studio = nested_studio();
        studio.delete_layer_tree(2);
        assert_eq!(studio.doc.layers.len(), 1);
        assert_eq!(studio.doc.layers[0].name, "Sibling");
        studio.undo();
        assert_eq!(studio.doc.layers.len(), 4);
        assert!(studio.doc.validate_hierarchy().is_ok());
        studio.doc.layers.pop();
        studio.delete_layer_tree(2);
        assert_eq!(studio.doc.layers.len(), 1);
        assert!(!studio.doc.layers[0].is_group);
        studio.undo();
        assert_eq!(studio.doc.layers.len(), 3);
        assert!(studio.doc.validate_hierarchy().is_ok());
    }

    #[test]
    fn ancestor_lock_blocks_child_actions() {
        let mut studio = nested_studio();
        studio.doc.layers[2].locked = true;
        assert!(!studio.layer_unlocked(0));
        studio.delete_layer_tree(0);
        assert_eq!(studio.doc.layers.len(), 4);
        studio.activate_layer_tree(2);
        assert!(studio.selection.is_empty());
        studio.doc.layers[2].locked = false;
        studio.doc.layers[0].locked = true;
        studio.delete_layer_tree(2);
        assert_eq!(studio.doc.layers.len(), 4);
    }

    #[test]
    fn inserting_and_removing_layers_preserves_selected_object_identity() {
        let mut studio = nested_studio();
        studio.active_layer = Some(3);
        studio.finish_create(CreateKind::Rect, Pt::ZERO, Pt::new(20., 20.));
        let object = studio.primary().unwrap().1;
        let layer_id = studio.doc.layers[3].id;
        studio.selected_layer = Some(layer_id);
        studio.history = History::default();
        studio.commit(Cmd::Batch(vec![Cmd::AddLayer {
            index: 0,
            layer: Layer::vector("Inserted"),
        }]));
        assert_eq!(studio.selection, vec![(4, object)]);
        assert_eq!(studio.active_layer, Some(4));
        assert_eq!(studio.selected_layer, Some(layer_id));
        studio.undo();
        assert_eq!(studio.selection, vec![(3, object)]);
        assert_eq!(studio.active_layer, Some(3));
        studio.redo();
        assert_eq!(studio.selection, vec![(4, object)]);
        assert_eq!(studio.active_layer, Some(4));
    }
}
