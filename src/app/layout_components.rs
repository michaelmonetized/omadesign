use super::*;
use crate::layout_components as components;

impl Studio {
    fn component_edit(
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
