use super::*;
use crate::layout_components as components;

#[derive(Clone)]
pub struct ComponentPropertyTarget {
    document: String,
    generation: u64,
    selection: Vec<(usize, u64)>,
    before: Vec<(usize, Vec<Shape>)>,
    pub layer: usize,
    pub root: u64,
}
#[derive(Clone)]
pub enum ComponentPropertyEdit {
    Expose {
        object: u64,
        name: String,
        kind: components::PropertyKind,
    },
    Set {
        id: u64,
        value: components::PropertyValue,
    },
    Rename {
        id: u64,
        name: String,
    },
    Remove {
        id: u64,
    },
}

impl Studio {
    /// Capture the component and current document for a delayed inspector edit.
    /// Args: current selection. Returns an exact target, or None without a component.
    pub fn component_property_target(&self) -> Option<ComponentPropertyTarget> {
        let (layer, id) = self.primary()?;
        let root = components::component_ancestor(&self.doc, layer, id)?;
        Some(ComponentPropertyTarget {
            document: self.swap_id.clone(),
            generation: self.canvas_gen,
            selection: self.selection.clone(),
            before: self
                .doc
                .layers
                .iter()
                .enumerate()
                .filter_map(|(i, l)| Some((i, l.kind.shapes()?.to_vec())))
                .collect(),
            layer,
            root,
        })
    }
    /// Apply a typed property edit only to the captured editable component.
    /// Args: captured target and edit. Returns whether the atomic edit was accepted.
    pub fn apply_component_property(
        &mut self,
        target: ComponentPropertyTarget,
        edit: ComponentPropertyEdit,
    ) -> bool {
        if self.swap_id != target.document
            || self.canvas_gen != target.generation
            || self.selection != target.selection
            || self.file_dialog_pending()
            || self.pending_nav.is_some()
            || self.updates.freezing
            || self.op.is_some()
            || self.deformation.is_some()
            || self.free_transform.is_some()
            || target.before.iter().any(|(li, shapes)| {
                self.doc.layers.get(*li).and_then(|l| l.kind.shapes()) != Some(shapes.as_slice())
            })
        {
            self.status = "The component changed; select it again before editing".into();
            return false;
        }
        let object = match &edit {
            ComponentPropertyEdit::Expose { object, .. } => *object,
            ComponentPropertyEdit::Set { id, .. } => {
                match components::property_object(&self.doc, target.layer, target.root, *id) {
                    Ok(id) => id,
                    Err(e) => {
                        self.status = e;
                        return false;
                    }
                }
            }
            _ => target.root,
        };
        if !component_object_editable(&self.doc, target.layer, object)
            || !component_object_editable(&self.doc, target.layer, target.root)
        {
            self.status = "Choose an unlocked, visible component object".into();
            return false;
        }
        let (layer, root) = (target.layer, target.root);
        let mut after = self.doc.layout_snapshot();
        let result = match edit {
            ComponentPropertyEdit::Expose { object, name, kind } => {
                components::expose_property(&mut after, layer, root, object, &name, kind)
                    .map(|_| ())
            }
            ComponentPropertyEdit::Set { id, value } => {
                components::set_property(&mut after, layer, root, id, value)
            }
            ComponentPropertyEdit::Rename { id, name } => {
                components::edit_property_definition(&mut after, layer, root, id, Some(&name))
            }
            ComponentPropertyEdit::Remove { id } => {
                components::edit_property_definition(&mut after, layer, root, id, None)
            }
        };
        if let Err(error) = result {
            self.status = error;
            return false;
        }
        self.component_edit(
            |doc| {
                *doc = after;
                Ok(None)
            },
            "Component property updated",
        );
        true
    }

    /// Commit a complete component operation through the existing vector command batch.
    /// Args: isolated document edit and success text. Returns nothing; errors preserve artwork.
    pub(super) fn component_edit(
        &mut self,
        edit: impl FnOnce(&mut Document) -> Result<Option<(usize, u64)>, String>,
        success: &str,
    ) {
        let mut after = self.doc.layout_snapshot();
        match edit(&mut after) {
            Ok(selected) => {
                let commands: Vec<_> = self
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
                if !commands.is_empty() {
                    self.commit(Cmd::Batch(commands));
                }
                if let Some(selected) = selected {
                    self.selection = vec![selected];
                }
                self.status = success.into();
            }
            Err(error) => self.status = error,
        }
    }

    pub fn create_layout_component(&mut self) {
        let Some((layer, root)) = self.selected_frame() else {
            self.status = "Select a frame to create a component".into();
            return;
        };
        self.component_edit(
            |doc| {
                components::make_component(doc, layer, root)?;
                Ok(Some((layer, root)))
            },
            "Component created — instances inherit edits",
        );
    }

    pub fn insert_layout_instance(&mut self, main: u64) {
        let Some(layer) = self.vector_target() else {
            self.status = "Choose a vector layer".into();
            return;
        };
        let selected = self.selected_frame().filter(|(_, id)| *id != main);
        let parent = selected.filter(|(li, _)| *li == layer).map(|(_, id)| id);
        let origin = selected
            .and_then(|(li, id)| self.doc.find_shape(li, id))
            .map(|s| s.geom.bbox().min + Pt::splat(24.0))
            .or_else(|| {
                self.doc.layers.iter().find_map(|l| {
                    l.kind
                        .shapes()?
                        .iter()
                        .find(|s| s.id == main)
                        .map(|s| Pt::new(s.geom.bbox().max.x + 40.0, s.geom.bbox().min.y))
                })
            })
            .unwrap_or(Pt::new(80.0, 80.0));
        self.component_edit(
            |doc| {
                let id = components::insert_instance(doc, main, layer, origin, parent)?;
                Ok(Some((layer, id)))
            },
            "Instance inserted",
        );
    }

    pub fn add_layout_variant(&mut self, name: &str) {
        let Some((layer, root)) = self.selected_frame() else {
            self.status = "Select a main component".into();
            return;
        };
        let Some(frame) = self.doc.find_shape(layer, root) else {
            return;
        };
        let b = frame.geom.bbox();
        let origin = Pt::new(b.max.x + 40.0, b.min.y);
        self.component_edit(
            |doc| {
                let id = components::add_variant(doc, root, name, origin)?;
                Ok(Some((layer, id)))
            },
            "Variant created — edit its appearance, then switch instances to it",
        );
    }

    pub fn swap_layout_variant(&mut self, main: u64) {
        let Some((layer, id)) = self.primary() else {
            return;
        };
        let Some(root) = components::instance_ancestor(&self.doc, layer, id) else {
            self.status = "Select an instance".into();
            return;
        };
        self.component_edit(
            |doc| {
                components::swap_variant(doc, layer, root, main)?;
                Ok(Some((layer, root)))
            },
            "Variant switched; overrides retained",
        );
    }

    pub fn detach_layout_instance(&mut self) {
        let Some((layer, id)) = self.primary() else {
            return;
        };
        let Some(root) = components::instance_ancestor(&self.doc, layer, id) else {
            self.status = "Select an instance".into();
            return;
        };
        self.component_edit(
            |doc| {
                components::detach(doc, layer, root)?;
                Ok(Some((layer, root)))
            },
            "Instance detached; objects remain editable",
        );
    }

    pub fn reset_layout_instance(&mut self) {
        let Some((layer, id)) = self.primary() else {
            return;
        };
        let Some(root) = components::instance_ancestor(&self.doc, layer, id) else {
            self.status = "Select an instance".into();
            return;
        };
        self.component_edit(
            |doc| {
                components::sync_instance(doc, layer, root, true)?;
                Ok(Some((layer, root)))
            },
            "Instance overrides reset",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_main_updates_instances_and_undo_restores_both_atomically() {
        let mut studio = Studio::new();
        studio.doc = Document::new("Component undo", 600.0, 300.0, 96.0);
        studio.doc.layers = vec![Layer::vector("UI")];
        let main = crate::layout::make_frame(Pt::ZERO, Pt::new(120.0, 60.0));
        let main_id = main.id;
        studio.doc.layers[0].kind.shapes_mut().unwrap().push(main);
        components::make_component(&mut studio.doc, 0, main_id).unwrap();
        let instance =
            components::insert_instance(&mut studio.doc, main_id, 0, Pt::new(180.0, 0.0), None)
                .unwrap();
        let before = studio.doc.find_shape(0, main_id).unwrap().opacity;
        studio.commit(Cmd::SetOpacity {
            layer: 0,
            id: main_id,
            before,
            after: 0.42,
        });
        assert_eq!(studio.doc.find_shape(0, instance).unwrap().opacity, 0.42);
        studio.undo();
        assert_eq!(studio.doc.find_shape(0, main_id).unwrap().opacity, before);
        assert_eq!(studio.doc.find_shape(0, instance).unwrap().opacity, before);
        studio.redo();
        assert_eq!(studio.doc.find_shape(0, instance).unwrap().opacity, 0.42);
    }
}

pub(super) fn component_object_editable(doc: &Document, layer: usize, object: u64) -> bool {
    if !doc.layer_editable(layer) {
        return false;
    }
    let mut current = Some(object);
    let mut seen = std::collections::HashSet::new();
    while let Some(id) = current {
        if !seen.insert(id) {
            return false;
        }
        let Some(shape) = doc.find_shape(layer, id) else {
            return false;
        };
        if shape.locked || (!shape.visible && id != object) {
            return false;
        }
        current = shape.layout.parent;
    }
    true
}

#[cfg(test)]
mod property_command_tests {
    use super::*;
    use components::{PropertyKind, PropertyValue};
    fn studio() -> (Studio, u64, u64) {
        let mut studio = Studio::new();
        studio.doc = Document::new("Property commands", 600., 400., 96.);
        studio.doc.layers = vec![Layer::vector("UI")];
        studio.history = History::default();
        let main = crate::layout::make_frame(Pt::ZERO, Pt::new(120., 60.));
        let root = main.id;
        studio.doc.layers[0].kind.shapes_mut().unwrap().push(main);
        components::make_component(&mut studio.doc, 0, root).unwrap();
        let id = components::expose_property(
            &mut studio.doc,
            0,
            root,
            root,
            "Opacity",
            PropertyKind::Opacity,
        )
        .unwrap();
        studio.selection = vec![(0, root)];
        (studio, root, id)
    }
    #[test]
    fn named_property_edits_update_instances_and_undo_reopen_atomically() {
        let (mut studio, main, id) = studio();
        let instance =
            components::insert_instance(&mut studio.doc, main, 0, Pt::new(180., 0.), None).unwrap();
        let target = studio.component_property_target().unwrap();
        assert!(studio.apply_component_property(
            target,
            ComponentPropertyEdit::Set {
                id,
                value: PropertyValue::Opacity(0.25)
            }
        ));
        assert_eq!(studio.doc.find_shape(0, instance).unwrap().opacity, 0.25);
        assert_eq!(studio.history.len(), 1);
        studio.undo();
        assert_eq!(studio.doc.find_shape(0, main).unwrap().opacity, 1.);
        assert_eq!(studio.doc.find_shape(0, instance).unwrap().opacity, 1.);
        studio.redo();
        let doc = crate::project::decode(&crate::project::encode(&studio.doc).unwrap()).unwrap();
        assert_eq!(
            components::property_value(&doc, 0, instance, id).unwrap(),
            PropertyValue::Opacity(0.25)
        );
    }
    #[test]
    fn stale_locked_busy_and_wrong_property_types_do_not_change_artwork_or_history() {
        for case in [
            "document",
            "generation",
            "selection",
            "shape",
            "locked",
            "busy",
            "type",
        ] {
            let (mut studio, main, id) = studio();
            let target = studio.component_property_target().unwrap();
            let mut value = PropertyValue::Opacity(0.25);
            match case {
                "document" => studio.swap_id.push('x'),
                "generation" => studio.mark_interaction(),
                "selection" => studio.selection.clear(),
                "shape" => studio.doc.find_shape_mut(0, main).unwrap().opacity = 0.5,
                "locked" => studio.doc.find_shape_mut(0, main).unwrap().locked = true,
                "busy" => studio.updates.freezing = true,
                "type" => value = PropertyValue::Text("wrong".into()),
                _ => {}
            }
            let before = serde_json::to_string(&studio.doc).unwrap();
            assert!(
                !studio.apply_component_property(target, ComponentPropertyEdit::Set { id, value }),
                "{case}"
            );
            assert_eq!(before, serde_json::to_string(&studio.doc).unwrap());
            assert_eq!(studio.history.len(), 0);
        }
    }
}
