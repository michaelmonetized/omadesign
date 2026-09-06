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

    /// Layer markers follow their descendants. Reorder complete adjacent sibling
    /// subtrees, never an isolated group marker or a child across its parent.
    pub fn move_layer_tree(&mut self, index: usize, up: bool) {
        if !self.layer_unlocked(index) {
            return;
        }
        let Some(sibling) = self.layer_sibling(index, up) else {
            return;
        };
        let a = self.layer_tree_indices(index.min(sibling));
        let b = self.layer_tree_indices(index.max(sibling));
        let contiguous = |indices: &[usize]| indices.windows(2).all(|w| w[1] == w[0] + 1);
        if !contiguous(&a) || !contiguous(&b) || a.last().map(|v| v + 1) != b.first().copied() {
            self.status = "This layer group has an inconsistent order and cannot be moved.".into();
            return;
        }
        let active_id = self.doc.layers[index].id;
        let mut order: Vec<u64> = self.doc.layers.iter().map(|l| l.id).collect();
        let mut desired = order.clone();
        let start = a[0];
        let end = b[b.len() - 1] + 1;
        desired[start..end].rotate_left(a.len());
        let mut commands = Vec::new();
        for to in start..end {
            if order[to] == desired[to] {
                continue;
            }
            let from = order.iter().position(|id| *id == desired[to]).unwrap();
            let id = order.remove(from);
            order.insert(to, id);
            commands.push(Cmd::ReorderLayer { from, to });
        }
        self.clear_layer_interaction();
        self.commit(Cmd::Batch(commands));
        self.active_layer = self.doc.layers.iter().position(|l| l.id == active_id);
        self.status = "Layer group order updated".into();
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
        let name = layer.name.clone();
        self.clear_layer_interaction();
        self.active_layer = Some(index);
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
        let before: Vec<_> = studio.doc.layers.iter().map(|l| (l.id, l.parent)).collect();
        studio.move_layer_tree(2, true);
        assert_eq!(studio.doc.layers[0].name, "Sibling");
        assert_eq!(studio.doc.layers[3].name, "Group");
        assert!(studio.doc.validate_hierarchy().is_ok());
        studio.undo();
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
}
