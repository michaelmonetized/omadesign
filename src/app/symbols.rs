//! Vector symbols share the existing component definitions, instances and history.
use super::*;
use crate::layout_components::{self as components, ComponentBinding};

fn available(s: &Studio) -> bool {
    s.op.is_none()
        && s.type_edit.is_none()
        && s.deformation.is_none()
        && s.free_transform.is_none()
        && s.pending_nav.is_none()
        && !s.file_dialog_pending()
        && !s.updates.freezing
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn fixture() -> Studio {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.doc = Document::new("Symbols", 240., 180., 72.);
        s.doc.transparent = true;
        let mut layer = Layer::vector("Artwork");
        let a = Shape::new(
            Geom::Rect {
                origin: Pt::new(10., 10.),
                size: Pt::new(40., 30.),
                radius: 4.,
            },
            Style {
                fill: Fill::Solid(Rgba::new(230, 50, 20, 173)),
                stroke: None,
            },
        );
        let b = Shape::new(
            Geom::Ellipse {
                center: Pt::new(60., 30.),
                radii: Pt::new(12., 18.),
            },
            Style {
                fill: Fill::Solid(Rgba::rgb(20, 50, 230)),
                stroke: None,
            },
        );
        s.selection = vec![(0, a.id), (0, b.id)];
        *layer.kind.shapes_mut().unwrap() = vec![a, b];
        s.doc.layers = vec![layer];
        s.active_layer = Some(0);
        s
    }
    fn image(s: &Studio) -> Vec<u8> {
        crate::compositor::render_export(&s.doc, 1)
            .unwrap()
            .data()
            .to_vec()
    }
    #[test]
    fn craft_symbol_creation_preserves_pixels_and_instance_edits_undo_and_native_reopen() {
        let mut s = fixture();
        let original = crate::project::encode(&s.doc).unwrap();
        let pixels = image(&s);
        let child = s.selection[0].1;
        assert!(s.create_vector_symbol("Badge"));
        assert_eq!(s.history.len(), 1);
        assert_eq!(image(&s), pixels);
        let main = s.primary().unwrap().1;
        assert_eq!(components::definitions(&s.doc).len(), 1);
        let created = crate::project::encode(&s.doc).unwrap();
        s.undo();
        assert_eq!(crate::project::encode(&s.doc).unwrap(), original);
        s.redo();
        assert_eq!(crate::project::encode(&s.doc).unwrap(), created);
        assert!(s.place_vector_symbol(main, Pt::new(120., 70.)));
        let instance = s.primary().unwrap().1;
        let linked = match &s.doc.find_shape(0, instance).unwrap().layout.component {
            Some(ComponentBinding::Instance { nodes, .. }) => {
                nodes.iter().find(|n| n.source == child).unwrap().instance
            }
            _ => panic!(),
        };
        let before = s.doc.find_shape(0, child).unwrap().style.clone();
        let mut after = before.clone();
        after.fill = Fill::Solid(Rgba::rgb(10, 240, 80));
        s.commit(Cmd::SetStyle {
            layer: 0,
            id: child,
            before,
            after: after.clone(),
        });
        assert_eq!(s.doc.find_shape(0, linked).unwrap().style, after);
        let saved = crate::project::encode(&s.doc).unwrap();
        let reopened = crate::project::decode(&saved).unwrap();
        assert_eq!(reopened.find_shape(0, linked).unwrap().style, after);
        assert_eq!(crate::project::encode(&reopened).unwrap(), saved);
        assert!(s.break_vector_symbol_link());
        assert!(
            s.doc
                .find_shape(0, instance)
                .unwrap()
                .layout
                .component
                .is_none()
        );
    }
    #[test]
    fn craft_symbol_removal_retains_artwork_and_one_undo_restores_all_links() {
        let mut s = fixture();
        assert!(s.create_vector_symbol("Badge"));
        let main = s.primary().unwrap().1;
        assert!(s.place_vector_symbol(main, Pt::new(120., 70.)));
        let before = crate::project::encode(&s.doc).unwrap();
        let pixels = image(&s);
        let history = s.history.len();
        assert!(s.remove_vector_symbol(main));
        assert_eq!(s.history.len(), history + 1);
        assert!(components::definitions(&s.doc).is_empty());
        assert_eq!(image(&s), pixels);
        let saved = crate::project::encode(&s.doc).unwrap();
        assert_eq!(
            crate::project::encode(&crate::project::decode(&saved).unwrap()).unwrap(),
            saved
        );
        s.undo();
        assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
        s.redo();
        assert_eq!(crate::project::encode(&s.doc).unwrap(), saved);
    }
    #[test]
    fn craft_symbol_invalid_locked_and_stale_targets_preserve_the_document() {
        let mut s = fixture();
        let before = crate::project::encode(&s.doc).unwrap();
        s.doc.layers[0].locked = true;
        assert!(!s.create_vector_symbol("No"));
        s.doc.layers[0].locked = false;
        s.selection.push((0, u64::MAX));
        assert!(!s.create_vector_symbol("Missing"));
        s.selection.pop();
        assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
        assert_eq!(s.history.len(), 0);
        assert!(s.create_vector_symbol("Badge"));
        let main = s.primary().unwrap().1;
        let original = crate::project::encode(&s.doc).unwrap();
        assert!(!s.rename_vector_symbol(main, "Other name", "Bad rename"));
        assert!(!s.place_vector_symbol(main, Pt::new(f32::NAN, 0.)));
        assert_eq!(crate::project::encode(&s.doc).unwrap(), original);
        assert!(s.rename_vector_symbol(main, "Badge", "Badge renamed"));
        assert_eq!(components::definitions(&s.doc)[0].2, "Badge renamed");
    }
}
fn editable(s: &Studio, layer: usize, id: u64) -> bool {
    s.doc.layer_visible(layer)
        && super::layout_components::component_object_editable(&s.doc, layer, id)
        && s.doc
            .find_shape(layer, id)
            .is_some_and(|shape| shape.visible && !shape.guide)
}
fn named(doc: &Document, wanted: &str) -> Result<String, String> {
    let text = wanted.trim();
    if text.chars().count() > 64 || text.chars().any(char::is_control) {
        return Err("Symbol names need at most 64 printable characters".into());
    }
    let base = if text.is_empty() { "Symbol" } else { text };
    let names: HashSet<_> = components::definitions(doc)
        .into_iter()
        .map(|(_, _, name, _)| name.to_lowercase())
        .collect();
    if !names.contains(&base.to_lowercase()) {
        return Ok(base.into());
    }
    for index in 2..=10000 {
        let suffix = format!(" ({index})");
        let candidate = format!(
            "{}{}",
            base.chars().take(64 - suffix.len()).collect::<String>(),
            suffix
        );
        if !names.contains(&candidate.to_lowercase()) {
            return Ok(candidate);
        }
    }
    Err("Choose a different symbol name".into())
}

impl Studio {
    /// Rename one concrete editable definition without overwriting a concurrent name edit.
    /// Args: definition, captured name and replacement. Returns whether a real metadata command was committed.
    pub(crate) fn rename_vector_symbol(&mut self, main: u64, expected: &str, name: &str) -> bool {
        if !available(self) {
            return false;
        }
        let Some((layer, _, _, _)) = components::definitions(&self.doc)
            .into_iter()
            .find(|(_, id, _, _)| *id == main)
        else {
            return false;
        };
        let shape = self.doc.find_shape(layer, main).unwrap();
        let name = name.trim();
        if !editable(self, layer, main)
            || shape.name != expected
            || name.is_empty()
            || name.chars().count() > 64
            || name.chars().any(char::is_control)
        {
            self.status = "Select the current editable symbol and use a printable name".into();
            return false;
        }
        if name == shape.name {
            return true;
        }
        self.commit(Cmd::SetShapeMeta {
            layer,
            id: main,
            name: name.into(),
            visible: shape.visible,
            locked: shape.locked,
            before: (shape.name.clone(), shape.visible, shape.locked),
        });
        true
    }

    /// Break only the selected instance's link while preserving its editable artwork.
    /// Args: none. Returns whether the existing component command accepted the selection.
    pub(crate) fn break_vector_symbol_link(&mut self) -> bool {
        if !available(self) {
            return false;
        }
        let Some((layer, id)) = self.primary() else {
            return false;
        };
        let Some(root) = components::instance_ancestor(&self.doc, layer, id) else {
            return false;
        };
        if !editable(self, layer, root) {
            self.status = "Unlock the instance before breaking its link".into();
            return false;
        }
        self.detach_layout_instance();
        true
    }

    /// Create a linked symbol definition from adjacent artwork on one vector layer.
    /// Args: desired name. Returns success after one ordinary component transaction.
    pub(crate) fn create_vector_symbol(&mut self, name: &str) -> bool {
        let prepare = || -> Result<(usize, Vec<u64>, crate::geom::Bounds, String), String> {
            if !available(self) {
                return Err("Finish the current edit before creating a symbol".into());
            }
            crate::layout::validate_hierarchy(&self.doc)?;
            let layer = self
                .selection
                .first()
                .map(|p| p.0)
                .ok_or("Select vector artwork to create a symbol")?;
            if self
                .selection
                .iter()
                .any(|p| p.0 != layer || p.1 == RASTER_ID)
            {
                return Err("Create a symbol from artwork on one vector layer".into());
            }
            if self
                .selection
                .iter()
                .any(|&(li, id)| self.doc.find_shape(li, id).is_none())
            {
                return Err("The selected artwork changed".into());
            }
            let shapes = self
                .doc
                .layers
                .get(layer)
                .and_then(|l| l.kind.shapes())
                .ok_or("Choose a vector layer")?;
            let selected: HashSet<_> = self.selection.iter().map(|p| p.1).collect();
            let roots: Vec<_> = shapes
                .iter()
                .filter(|s| selected.contains(&s.id))
                .filter(|s| {
                    let mut parent = s.layout.parent;
                    let mut seen = HashSet::new();
                    while let Some(id) = parent {
                        if !seen.insert(id) {
                            return false;
                        }
                        if selected.contains(&id) {
                            return false;
                        }
                        parent = self.doc.find_shape(layer, id).and_then(|s| s.layout.parent);
                    }
                    true
                })
                .map(|s| s.id)
                .collect();
            if roots.is_empty() {
                return Err("Select existing vector artwork".into());
            }
            if roots.iter().any(|&id| {
                matches!(
                    self.doc.find_shape(layer, id).unwrap().layout.component,
                    Some(ComponentBinding::Main { .. })
                )
            }) {
                return Err("This artwork is already a symbol definition".into());
            }
            if roots.iter().any(|&id| {
                self.doc
                    .find_shape(layer, id)
                    .unwrap()
                    .layout
                    .parent
                    .is_some()
            }) {
                return Err(
                    "Move the symbol's root artwork out of its containing frame first".into(),
                );
            }
            let mut members = HashSet::new();
            for &root in &roots {
                members.insert(root);
                members.extend(crate::layout::descendants(&self.doc, layer, root));
            }
            if members.iter().any(|&id| !editable(self, layer, id)) {
                return Err("Show and unlock the symbol artwork and its ancestors".into());
            }
            let indices: Vec<_> = shapes
                .iter()
                .enumerate()
                .filter(|(_, s)| members.contains(&s.id))
                .map(|(i, _)| i)
                .collect();
            if indices.last().unwrap() - indices[0] + 1 != indices.len() {
                return Err("Group adjacent artwork before creating a symbol".into());
            }
            let bounds = roots
                .iter()
                .filter_map(|&id| self.doc.find_shape(layer, id).map(Shape::world_bbox))
                .reduce(|a, b| a.union(b))
                .unwrap();
            let name = named(&self.doc, name)?;
            Ok((layer, roots, bounds, name))
        };
        let (layer, roots, bounds, name) = match prepare() {
            Ok(p) => p,
            Err(error) => {
                self.status = error;
                return false;
            }
        };
        let mut created = false;
        self.component_edit(
            |doc| {
                let root = if roots.len() == 1
                    && doc
                        .find_shape(layer, roots[0])
                        .is_some_and(|s| s.layout.frame && s.layout.component.is_none())
                {
                    roots[0]
                } else {
                    let mut frame = Shape::new(
                        Geom::Rect {
                            origin: bounds.min,
                            size: Pt::new(bounds.width().max(1.), bounds.height().max(1.)),
                            radius: 0.,
                        },
                        Style {
                            fill: Fill::None,
                            stroke: None,
                        },
                    );
                    frame.layout.frame = true;
                    let id = frame.id;
                    let shapes = doc.layers[layer].kind.shapes_mut().unwrap();
                    let index = shapes.iter().position(|s| roots.contains(&s.id)).unwrap();
                    for shape in shapes.iter_mut().filter(|s| roots.contains(&s.id)) {
                        shape.layout.parent = Some(id);
                    }
                    shapes.insert(index, frame);
                    id
                };
                components::make_component(doc, layer, root)?;
                doc.find_shape_mut(layer, root).unwrap().name = name;
                created = true;
                Ok(Some((layer, root)))
            },
            "Symbol created",
        );
        if created {
            self.selected_layer = None;
        }
        created
    }

    /// Place an ordinary linked instance at a finite document position.
    /// Args: definition ID and top-left document point. Returns whether a new instance was committed.
    pub(crate) fn place_vector_symbol(&mut self, main: u64, origin: Pt) -> bool {
        if !available(self) || !origin.x.is_finite() || !origin.y.is_finite() {
            self.status = "Finish the current edit before placing a symbol".into();
            return false;
        }
        let layer = self.active_layer.filter(|&i| {
            self.doc.layer_editable(i)
                && self.doc.layer_visible(i)
                && self.doc.layers[i].kind.shapes().is_some()
        });
        let Some(layer) = layer else {
            self.status = "Choose an unlocked visible vector layer".into();
            return false;
        };
        let mut placed = false;
        self.component_edit(
            |doc| {
                let root = components::insert_instance(doc, main, layer, origin, None)?;
                placed = true;
                Ok(Some((layer, root)))
            },
            "Symbol instance placed",
        );
        if placed {
            self.selected_layer = None;
        }
        placed
    }

    /// Remove a definition while retaining the visible artwork of every instance.
    /// Args: definition ID. Returns whether the links were removed in one Undo step.
    pub(crate) fn remove_vector_symbol(&mut self, main: u64) -> bool {
        if !available(self) {
            return false;
        }
        let Some((layer, _, _, _)) = components::definitions(&self.doc)
            .into_iter()
            .find(|(_, id, _, _)| *id == main)
        else {
            return false;
        };
        if !editable(self,layer,main) || self.doc.layers.iter().enumerate().any(|(li,l)|l.kind.shapes().unwrap_or_default().iter().any(|s|matches!(s.layout.component,Some(ComponentBinding::Instance {main:id,..}) if id==main)&&!editable(self,li,s.id))) {
            self.status="Unlock the symbol and its instances before removing its definition".into();return false;
        }
        let mut removed = false;
        self.component_edit(|doc| {
            let replacement=components::definitions(doc).iter().filter(|(_,id,_,_)|*id!=main).find_map(|(li,id,_,_)|matches!(doc.find_shape(*li,*id)?.layout.component,Some(ComponentBinding::Main {family,..}) if family==main).then_some(*id));
            for layer in &mut doc.layers {if let Some(shapes)=layer.kind.shapes_mut() {for shape in shapes {
                if shape.id==main || matches!(shape.layout.component,Some(ComponentBinding::Instance {main:id,..}) if id==main) {shape.layout.component=None;}
                else {match &mut shape.layout.component {
                    Some(ComponentBinding::Main {family,..}) if *family==main=>{if let Some(id)=replacement {*family=id;}},
                    Some(ComponentBinding::Instance {nodes,..})=>for node in nodes {if node.nested_main==Some(main) {node.nested_main=None;}},
                    _=>{}
                }}
            }}}
            components::validate_properties(doc)?;removed=true;Ok(None)
        },"Symbol removed; instance artwork retained");
        removed
    }
}
