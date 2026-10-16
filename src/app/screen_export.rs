//! Asset collection captures concrete live artwork without copying or rewriting its paint.
use super::*;
use crate::screen_export::{Asset, Catalog, Object};

#[derive(Clone, Debug)]
pub(crate) struct AssetTarget {
    document: String,
    revision: u64,
    selection: Vec<(usize, u64)>,
    layer: Option<u64>,
    active: Option<u64>,
    objects: Vec<Object>,
}
fn ready(studio: &Studio) -> Result<(), String> {
    if !matches!(
        studio.persona,
        Persona::Design | Persona::Layout | Persona::Pixel
    ) {
        return Err("Collect assets from Design, Layout or Pixel artwork".into());
    }
    if studio.op.is_some()
        || studio.type_edit.is_some()
        || studio.file_dialog_pending()
        || studio.updates.freezing
        || studio.pending_nav.is_some()
        || studio.pixel_edit.is_some()
        || studio.pixel_sel.is_some()
    {
        return Err("Finish the current edit before changing export assets".into());
    }
    if studio.doc.collaboration.is_some() {
        return Err(
            "Asset export metadata is not supported in Live yet. Save a separate local copy first"
                .into(),
        );
    }
    Ok(())
}
fn name(doc: &Document, object: &Object) -> String {
    let layer = doc.layers.iter().find(|l| l.id == object.layer).unwrap();
    layer
        .kind
        .shapes()
        .and_then(|s| s.iter().find(|s| s.id == object.shape))
        .map(|s| s.name.as_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(&layer.name)
        .into()
}
fn unique_name(base: &str, assets: &[Asset]) -> String {
    let base: String = base
        .chars()
        .filter(|c| !c.is_control() && !"/\\:*?\"<>|".contains(*c))
        .take(40)
        .collect();
    let mut base = base.trim().trim_end_matches('.').to_owned();
    while base.len() > 60 {
        base.pop();
    }
    if base.is_empty() {
        base = "Asset".into();
    }
    let mut candidate = base.clone();
    let mut index = 2;
    while assets
        .iter()
        .any(|a| a.name.to_lowercase() == candidate.to_lowercase())
    {
        candidate = format!("{base} {index}");
        index += 1;
    }
    candidate
}
impl Studio {
    /// Capture the current concrete selection for an immediate asset collection command.
    /// Args: none. Returns stable artwork references bound to this document, revision and selection.
    pub(crate) fn capture_export_assets(&self) -> Result<AssetTarget, String> {
        ready(self)?;
        self.doc.validate_hierarchy()?;
        let mut objects = vec![];
        if !self.selection.is_empty() {
            for &(index, id) in &self.selection {
                let layer = self
                    .doc
                    .layers
                    .get(index)
                    .filter(|_| self.doc.layer_editable(index))
                    .ok_or("Selected artwork is missing, hidden or locked")?;
                if id == 0 {
                    if layer.kind.pixels().is_none() {
                        return Err("Selected raster artwork is missing".into());
                    }
                } else if self
                    .doc
                    .find_shape(index, id)
                    .is_none_or(|s| !s.visible || s.locked)
                {
                    return Err("Selected artwork is missing, hidden or locked".into());
                }
                let mut parent = self.doc.find_shape(index, id).and_then(|s| s.layout.parent);
                while let Some(id) = parent {
                    let shape = self
                        .doc
                        .find_shape(index, id)
                        .ok_or("Selected artwork has a missing frame")?;
                    if shape.locked || !shape.visible {
                        return Err("Selected artwork has a hidden or locked frame".into());
                    }
                    parent = shape.layout.parent;
                }
                let object = Object {
                    layer: layer.id,
                    shape: id,
                };
                if !objects.contains(&object) {
                    objects.push(object);
                }
            }
        } else if let Some(index) = match self.selected_layer {
            Some(id) => Some(
                self.doc
                    .layers
                    .iter()
                    .position(|l| l.id == id)
                    .ok_or("The selected layer no longer exists")?,
            ),
            None => self.active_layer,
        } {
            let root = self
                .doc
                .layers
                .get(index)
                .ok_or("Select artwork to collect")?;
            for (i, layer) in self
                .doc
                .layers
                .iter()
                .enumerate()
                .filter(|(i, _)| *i == index || self.doc.layer_ancestors(*i).contains(&index))
            {
                if layer.is_group {
                    continue;
                }
                if !self.doc.layer_editable(i) {
                    return Err("The selected layer tree includes hidden or locked artwork".into());
                }
                if layer.kind.pixels().is_some() {
                    objects.push(Object {
                        layer: layer.id,
                        shape: 0,
                    });
                } else if let Some(shapes) = layer.kind.shapes() {
                    objects.extend(
                        shapes
                            .iter()
                            .filter(|s| s.visible && s.layout.parent.is_none())
                            .map(|s| Object {
                                layer: layer.id,
                                shape: s.id,
                            }),
                    );
                }
            }
            if root.locked || !root.visible {
                return Err("Selected layer is hidden or locked".into());
            }
        }
        if objects.is_empty() {
            return Err("Select artwork to collect".into());
        }
        if objects.len() > 1024 {
            return Err("Collect at most 1024 objects at once".into());
        }
        Ok(AssetTarget {
            document: self.swap_id.clone(),
            revision: self.canvas_gen,
            selection: self.selection.clone(),
            layer: self.selected_layer,
            active: self
                .active_layer
                .and_then(|i| self.doc.layers.get(i))
                .map(|l| l.id),
            objects,
        })
    }

    /// Collect the captured artwork as one asset or one asset per selected object.
    /// Args: captured target and grouping choice. Returns new persistent asset IDs after one Undo command.
    pub(crate) fn collect_export_assets(
        &mut self,
        target: &AssetTarget,
        one: bool,
    ) -> Result<Vec<u64>, String> {
        ready(self)?;
        if target.document != self.swap_id
            || target.revision != self.canvas_gen
            || target.selection != self.selection
            || target.layer != self.selected_layer
            || target.active
                != self
                    .active_layer
                    .and_then(|i| self.doc.layers.get(i))
                    .map(|l| l.id)
        {
            return Err("The asset selection changed. Collect the current artwork again".into());
        }
        let before = self.doc.screen_export.clone();
        let mut after = before.clone();
        let mut ids = vec![];
        let groups = if one {
            vec![target.objects.clone()]
        } else {
            target.objects.iter().cloned().map(|o| vec![o]).collect()
        };
        for objects in groups {
            let id = crate::document::next_id();
            let asset = Asset {
                id,
                name: unique_name(&name(&self.doc, &objects[0]), &after.assets),
                objects,
            };
            asset.selection(&self.doc)?;
            after.assets.push(asset);
            ids.push(id);
        }
        self.set_screen_export(&before, after)?;
        Ok(ids)
    }

    /// Commit accepted export metadata through the ordinary document transaction.
    /// Args: expected catalog and replacement. Returns whether a validated command was committed.
    pub(crate) fn set_screen_export(
        &mut self,
        before: &Catalog,
        after: Catalog,
    ) -> Result<bool, String> {
        ready(self)?;
        if before != &self.doc.screen_export {
            return Err("Export settings changed. Edit the current rows again".into());
        }
        after.validate()?;
        if before == &after {
            return Ok(false);
        }
        self.commit(Cmd::SetScreenExport {
            before: before.clone(),
            after,
        });
        Ok(true)
    }
}

#[cfg(test)]
pub(crate) mod tests;
