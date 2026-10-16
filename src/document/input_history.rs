use super::{Cmd, Document, History, MAX_HISTORY, coalesce};

impl History {
    /// End field grouping after an edit from another owner.
    /// Args: none. Returns nothing; the next edit starts a separate history step.
    pub(crate) fn finish_field(&mut self) {
        self.field = None;
        self.sealed = true;
    }

    /// Keep one starting and final command per property in a field gesture.
    /// Args: applied command, document/selection/field/gesture identity and current document.
    /// Returns nothing; unrelated commands always keep their own history step.
    pub(crate) fn push_field(&mut self, cmd: Cmd, field: u64, doc: &Document) {
        let mut next = Vec::new();
        if !flatten(cmd.clone(), &mut next) {
            self.sealed = true;
            self.push(cmd);
            self.sealed = true;
            return;
        }
        let mut previous = Vec::new();
        if self.field == Some(field)
            && let Some(last) = self.undo.last()
            && flatten(last.clone(), &mut previous)
        {
            for command in next {
                if !previous.iter_mut().any(|old| merge(old, &command)) {
                    previous.push(command);
                }
            }
            *self.undo.last_mut().unwrap() = pack(previous, doc);
        } else {
            self.undo.push(pack(next, doc));
            if !self.staging && self.undo.len() > MAX_HISTORY {
                self.undo.remove(0);
            }
        }
        self.field = Some(field);
        self.sealed = true;
        self.redo.clear();
    }
}

/// Finish a field entry with consistent final values across overlapping commands.
/// Args: bounded property commands and applied document. Returns one history command.
fn pack(mut commands: Vec<Cmd>, doc: &Document) -> Cmd {
    for command in &mut commands {
        refresh_after(command, doc);
    }
    if commands.len() == 1 {
        commands.remove(0)
    } else {
        Cmd::Batch(commands)
    }
}

/// Keep retained snapshots from overwriting newer property values during Redo.
/// Args: one property command and applied document. Returns nothing; starting values stay intact.
fn refresh_after(command: &mut Cmd, doc: &Document) {
    match command {
        Cmd::SetGeom {
            layer,
            id,
            after,
            rot_after,
            ..
        } => {
            if let Some(shape) = doc.find_shape(*layer, *id) {
                *after = shape.geom.clone();
                *rot_after = shape.rotation;
            }
        }
        Cmd::SetGeoms { items } => {
            for (layer, id, _, after, _, rotation) in items {
                if let Some(shape) = doc.find_shape(*layer, *id) {
                    *after = shape.geom.clone();
                    *rotation = shape.rotation;
                }
            }
        }
        Cmd::SetVectorShapes { layer, after, .. } => {
            if let Some(shapes) = doc.layers.get(*layer).and_then(|layer| layer.kind.shapes()) {
                *after = shapes.to_vec();
            }
        }
        Cmd::SetStyle {
            layer, id, after, ..
        } => {
            if let Some(shape) = doc.find_shape(*layer, *id) {
                *after = shape.style.clone();
            }
        }
        Cmd::SetOpacity {
            layer, id, after, ..
        } => {
            if let Some(shape) = doc.find_shape(*layer, *id) {
                *after = shape.opacity;
            }
        }
        Cmd::SetFillOpacity {
            layer, id, after, ..
        } => {
            let value = match id {
                Some(id) => doc.find_shape(*layer, *id).map(|shape| shape.fill_opacity),
                None => doc.layers.get(*layer).map(|layer| layer.fill_opacity),
            };
            if let Some(value) = value {
                *after = value;
            }
        }
        Cmd::SetLayerMeta {
            index,
            name,
            visible,
            locked,
            opacity,
            blend,
            ..
        } => {
            if let Some(layer) = doc.layers.get(*index) {
                *name = layer.name.clone();
                *visible = layer.visible;
                *locked = layer.locked;
                *opacity = layer.opacity;
                *blend = layer.blend;
            }
        }
        Cmd::SetMotion { after, .. } => *after = doc.motion.clone(),
        Cmd::SetFilters { index, after, .. } => {
            if let Some(layer) = doc.layers.get(*index) {
                *after = layer.filters.clone();
            }
        }
        Cmd::SetShapeFilters {
            layer, id, after, ..
        } => {
            if let Some(shape) = doc.find_shape(*layer, *id) {
                *after = shape.filters.clone();
            }
        }
        Cmd::SetRasterXform { layer, after, .. } => {
            if let Some(value) = doc
                .layers
                .get(*layer)
                .and_then(|layer| layer.kind.raster_xform())
            {
                *after = value;
            }
        }
        Cmd::SetRasterShear { layer, after, .. } => {
            if let Some(layer) = doc.layers.get(*layer) {
                *after = layer.kind.raster_shear();
            }
        }
        Cmd::SetArtboards { after, .. } => *after = doc.artboards.clone(),
        Cmd::SetCorners {
            layer,
            id,
            after,
            radius_after,
            ..
        } => {
            if let Some(shape) = doc.find_shape(*layer, *id) {
                *after = shape.corners;
                if let crate::geom::Geom::Rect { radius, .. } = &shape.geom {
                    *radius_after = *radius;
                }
            }
        }
        Cmd::SetLayout {
            layer, id, after, ..
        } => {
            if let Some(shape) = doc.find_shape(*layer, *id) {
                *after = shape.layout.clone();
            }
        }
        _ => {}
    }
}

fn flatten(cmd: Cmd, commands: &mut Vec<Cmd>) -> bool {
    match cmd {
        Cmd::Batch(batch) => batch.into_iter().all(|command| flatten(command, commands)),
        command @ (Cmd::SetGeom { .. }
        | Cmd::SetGeoms { .. }
        | Cmd::SetVectorShapes { .. }
        | Cmd::SetStyle { .. }
        | Cmd::SetOpacity { .. }
        | Cmd::SetFillOpacity { .. }
        | Cmd::SetLayerMeta { .. }
        | Cmd::SetMotion { .. }
        | Cmd::SetFilters { .. }
        | Cmd::SetShapeFilters { .. }
        | Cmd::SetRasterXform { .. }
        | Cmd::SetRasterShear { .. }
        | Cmd::SetArtboards { .. }
        | Cmd::SetCorners { .. }
        | Cmd::SetLayout { .. }) => {
            if !commands.iter_mut().any(|old| merge(old, &command)) {
                commands.push(command);
            }
            true
        }
        _ => false,
    }
}

fn merge(previous: &mut Cmd, next: &Cmd) -> bool {
    match (&mut *previous, next) {
        (
            Cmd::SetGeom {
                layer,
                id,
                after,
                rot_after,
                ..
            },
            Cmd::SetGeom {
                layer: l,
                id: i,
                after: a,
                rot_after: r,
                ..
            },
        ) if layer == l && id == i => {
            *after = a.clone();
            *rot_after = *r;
            true
        }
        (Cmd::SetGeoms { items }, Cmd::SetGeoms { items: next }) => {
            for item in next {
                if let Some(old) = items
                    .iter_mut()
                    .find(|old| old.0 == item.0 && old.1 == item.1)
                {
                    old.3 = item.3.clone();
                    old.5 = item.5;
                } else {
                    items.push(item.clone());
                }
            }
            true
        }
        (
            Cmd::SetVectorShapes { layer, after, .. },
            Cmd::SetVectorShapes {
                layer: l, after: a, ..
            },
        ) if layer == l => {
            *after = a.clone();
            true
        }
        (
            Cmd::SetFillOpacity {
                layer, id, after, ..
            },
            Cmd::SetFillOpacity {
                layer: l,
                id: i,
                after: a,
                ..
            },
        ) if layer == l && id == i => {
            *after = *a;
            true
        }
        (
            Cmd::SetRasterShear { layer, after, .. },
            Cmd::SetRasterShear {
                layer: l, after: a, ..
            },
        ) if layer == l => {
            *after = *a;
            true
        }
        _ => coalesce(previous, next),
    }
}
