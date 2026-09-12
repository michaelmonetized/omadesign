use super::*;
use crate::layout::{self, AutoStack, Constraint};

impl Studio {
    pub fn selected_frame(&self) -> Option<(usize, u64)> {
        self.selection
            .iter()
            .copied()
            .find(|&(li, id)| self.doc.find_shape(li, id).is_some_and(|s| s.layout.frame))
    }

    pub fn wrap_selection_frame(&mut self) {
        let ids = self.selection.clone();
        if ids.is_empty() {
            self.status = "Select objects to wrap in a frame".into();
            return;
        }
        let layer = ids[0].0;
        let mut bounds: Option<crate::geom::Bounds> = None;
        for (li, id) in &ids {
            if *li != layer {
                self.status = "Wrap objects on one layer".into();
                return;
            }
            if let Some(shape) = self.doc.find_shape(*li, *id) {
                let b = shape.world_bbox();
                bounds = Some(match bounds {
                    None => b,
                    Some(acc) => acc.union(b),
                });
            }
        }
        let Some(bounds) = bounds else {
            return;
        };
        let padded = bounds.inflate(16.0);
        let mut frame = layout::make_frame(padded.min, padded.size());
        frame.layout.stack = None;
        let frame_id = frame.id;
        let mut commands = vec![Cmd::AddShape {
            layer,
            shape: frame,
        }];
        for (li, id) in &ids {
            if let Some(shape) = self.doc.find_shape(*li, *id) {
                let mut after = shape.layout.clone();
                after.parent = Some(frame_id);
                commands.push(Cmd::SetLayout {
                    layer: *li,
                    id: *id,
                    before: shape.layout.clone(),
                    after,
                });
            }
        }
        self.commit(Cmd::Batch(commands));
        self.selection = vec![(layer, frame_id)];
        self.status = "Wrapped in a frame".into();
    }

    pub fn add_placeholder(&mut self) {
        let Some(li) = self.vector_target() else {
            self.status = "Choose a vector layer".into();
            return;
        };
        let parent = self.selected_frame().map(|(_, id)| id);
        let origin = if let Some((_, id)) = self.selected_frame() {
            let b = self.doc.find_shape(li, id).map(|s| s.world_bbox());
            b.map(|b| b.min + Pt::splat(24.0))
                .unwrap_or(Pt::new(80.0, 80.0))
        } else {
            Pt::new(80.0, 80.0)
        };
        let shape = layout::make_placeholder(origin, Pt::new(240.0, 160.0), parent);
        let id = shape.id;
        self.commit(Cmd::Batch(self.add_and_reflow(li, shape, parent)));
        self.selection = vec![(li, id)];
        self.status = "Image placeholder".into();
    }

    pub(super) fn add_and_reflow(
        &self,
        layer: usize,
        mut shape: crate::document::Shape,
        parent: Option<u64>,
    ) -> Vec<Cmd> {
        let id = shape.id;
        let Some(frame_id) = parent else {
            return vec![Cmd::AddShape { layer, shape }];
        };
        let mut preview = self.doc.clone();
        crate::document::apply(
            &mut preview,
            &Cmd::AddShape {
                layer,
                shape: shape.clone(),
            },
        );
        layout::reflow(&mut preview, layer, frame_id);
        if let Some(placed) = preview.find_shape(layer, id) {
            shape.geom = placed.geom.clone();
            shape.rotation = placed.rotation;
        }
        let ids = layout::descendants(&preview, layer, frame_id);
        let items: Vec<_> = ids
            .into_iter()
            .filter(|&child| child != id)
            .filter_map(|child| {
                let before = self.doc.find_shape(layer, child)?;
                let after = preview.find_shape(layer, child)?;
                if before.geom == after.geom && before.rotation == after.rotation {
                    return None;
                }
                Some((
                    layer,
                    child,
                    before.geom.clone(),
                    after.geom.clone(),
                    before.rotation,
                    after.rotation,
                ))
            })
            .collect();
        let mut commands = vec![Cmd::AddShape { layer, shape }];
        if !items.is_empty() {
            commands.push(Cmd::SetGeoms { items });
        }
        commands
    }

    pub fn set_frame_stack(&mut self, stack: Option<AutoStack>) {
        let Some((li, id)) = self.selected_frame() else {
            self.status = "Select a frame".into();
            return;
        };
        let Some(shape) = self.doc.find_shape(li, id) else {
            return;
        };
        let mut after = shape.layout.clone();
        after.frame = true;
        after.stack = stack;
        let mut commands = vec![Cmd::SetLayout {
            layer: li,
            id,
            before: shape.layout.clone(),
            after: after.clone(),
        }];
        let mut preview = self.doc.clone();
        if let Some(shape) = preview.find_shape_mut(li, id) {
            shape.layout = after;
        }
        layout::reflow(&mut preview, li, id);
        let ids = layout::descendants(&preview, li, id);
        let items: Vec<_> = ids
            .into_iter()
            .filter_map(|child| {
                let before = self.doc.find_shape(li, child)?;
                let after = preview.find_shape(li, child)?;
                if before.geom == after.geom && before.rotation == after.rotation {
                    return None;
                }
                Some((
                    li,
                    child,
                    before.geom.clone(),
                    after.geom.clone(),
                    before.rotation,
                    after.rotation,
                ))
            })
            .collect();
        if !items.is_empty() {
            commands.push(Cmd::SetGeoms { items });
        }
        self.commit(Cmd::Batch(commands));
    }

    pub fn set_constraints(&mut self, x: Constraint, y: Constraint) {
        let Some((li, id)) = self.primary() else {
            return;
        };
        let Some(shape) = self.doc.find_shape(li, id) else {
            return;
        };
        let mut after = shape.layout.clone();
        after.constraint_x = x;
        after.constraint_y = y;
        self.commit(Cmd::SetLayout {
            layer: li,
            id,
            before: shape.layout.clone(),
            after,
        });
    }

    pub fn parent_selection_to_frame(&mut self, frame_id: u64) {
        let mut commands = Vec::new();
        let mut layer = None;
        for (li, id) in self.selection.clone() {
            if id == frame_id {
                continue;
            }
            if let Some(shape) = self.doc.find_shape(li, id) {
                let mut after = shape.layout.clone();
                after.parent = Some(frame_id);
                commands.push(Cmd::SetLayout {
                    layer: li,
                    id,
                    before: shape.layout.clone(),
                    after,
                });
                layer = Some(li);
            }
        }
        if commands.is_empty() {
            return;
        }
        self.commit(Cmd::Batch(commands));
        if let Some(li) = layer {
            layout::reflow(&mut self.doc, li, frame_id);
        }
        self.status = "Nested in the frame".into();
    }

    pub fn export_selected_frame_png(&mut self) {
        let Some((li, id)) = self.selected_frame() else {
            self.status = "Select a frame to export".into();
            return;
        };
        let Some(shape) = self.doc.find_shape(li, id).cloned() else {
            return;
        };
        if let Some(path) = crate::project::dialog_export("PNG", "png") {
            match compositor::export_png_bounds(&self.doc, self.export_scale, shape.world_bbox()) {
                Ok(bytes) => {
                    if let Err(e) = std::fs::write(&path, bytes) {
                        self.status = format!("write failed: {e}");
                    } else {
                        self.status = format!("exported {}", path.display());
                    }
                }
                Err(e) => self.status = format!("export failed: {e}"),
            }
        }
    }

    pub fn export_selected_frame_svg(&mut self) {
        let Some((li, id)) = self.selected_frame() else {
            self.status = "Select a frame to export".into();
            return;
        };
        if let Some(path) = crate::project::dialog_export("SVG", "svg") {
            match crate::svg::export_frame(&self.doc, li, id) {
                Ok(s) => {
                    let _ = std::fs::write(&path, s);
                    self.status = format!("exported {}", path.display());
                }
                Err(e) => self.status = format!("export failed: {e}"),
            }
        }
    }

    pub fn export_selected_frame_html(&mut self) {
        let Some((li, id)) = self.selected_frame() else {
            self.status = "Select a frame to export".into();
            return;
        };
        if let Some(path) = crate::project::dialog_export("HTML", "html") {
            match layout::export_html(&self.doc, li, id) {
                Ok(s) => {
                    let _ = std::fs::write(&path, s);
                    self.status = format!("exported {}", path.display());
                }
                Err(e) => self.status = format!("export failed: {e}"),
            }
        }
    }

    pub fn use_layout_template(&mut self, id: &str, width: f32, height: f32, dpi: f32) {
        match crate::layout_templates::build(id, width, height, dpi) {
            Ok(document) => {
                self.ensure_tabs();
                self.open_document(document, None);
                self.persona = Persona::Layout;
                self.set_tool(Tool::Select);
                self.dirty = true;
                self.show_templates = false;
                self.show_welcome = false;
                self.need_fit = true;
                self.status = "Layout template ready · frames stay nested and editable".into();
                self.mark();
            }
            Err(error) => self.status = error,
        }
    }
}
