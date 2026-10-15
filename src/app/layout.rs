use super::*;
use crate::layout::{self, AutoStack, Constraint, FrameLayout};

#[derive(Clone)]
pub(crate) struct LayoutInspectorTarget {
    document: String,
    generation: u64,
    selection: Vec<(usize, u64)>,
    layouts: Vec<FrameLayout>,
}

#[cfg(test)]
mod grid_command_tests {
    use super::*;
    use crate::layout::{GridPlacement, GridTrack, GridTrackSize, Sizing, StackFlow};

    fn studio() -> (Studio, u64, u64) {
        let mut studio = Studio::new();
        studio.doc = Document::new("Grid commands", 800.0, 600.0, 96.0);
        studio.doc.layers = vec![Layer::vector("Layout")];
        studio.history = History::default();
        studio.active_layer = Some(0);
        let mut frame = layout::make_frame(Pt::ZERO, Pt::new(600.0, 200.0));
        frame.layout.stack = Some(AutoStack {
            flow: StackFlow::Grid,
            gap: 10.0,
            padding: [0.0; 4],
            ..Default::default()
        });
        let root = frame.id;
        let mut child = layout::make_frame(Pt::new(30.0, 40.0), Pt::new(20.0, 30.0));
        child.layout = FrameLayout {
            parent: Some(root),
            width: Sizing::Fill,
            ..Default::default()
        };
        let id = child.id;
        studio.doc.layers[0]
            .kind
            .shapes_mut()
            .unwrap()
            .extend([frame, child]);
        studio.selection = vec![(0, root)];
        (studio, root, id)
    }

    #[test]
    fn typed_grid_track_and_placement_edits_reflow_in_one_undo_and_reopen() {
        let (mut studio, root, child) = studio();
        let before = studio.doc.layers[0].kind.shapes().unwrap().to_vec();
        let target = studio.layout_inspector_target().unwrap();
        let mut properties = studio.doc.find_shape(0, root).unwrap().layout.clone();
        properties.stack.as_mut().unwrap().column_tracks = vec![
            GridTrack {
                size: GridTrackSize::Fixed(80.0),
                ..Default::default()
            },
            GridTrack::default(),
        ];
        assert!(studio.apply_layout_inspector(&target, &properties));
        assert_eq!(studio.history.len(), 1);
        assert!((studio.doc.find_shape(0, child).unwrap().geom.bbox().width() - 80.0).abs() < 0.01);
        studio.undo();
        assert_eq!(studio.doc.layers[0].kind.shapes().unwrap(), before);
        studio.redo();
        studio.selection = vec![(0, child)];
        let target = studio.layout_inspector_target().unwrap();
        let mut properties = studio.doc.find_shape(0, child).unwrap().layout.clone();
        properties.grid = GridPlacement {
            column: Some(1),
            row: Some(0),
            ..Default::default()
        };
        assert!(studio.apply_layout_inspector(&target, &properties));
        assert!(
            (studio.doc.find_shape(0, child).unwrap().geom.bbox().width() - 510.0).abs() < 0.01
        );
        assert!((studio.doc.find_shape(0, child).unwrap().geom.bbox().min.x - 90.0).abs() < 0.01);
        let reopened =
            crate::project::decode(&crate::project::encode(&studio.doc).unwrap()).unwrap();
        assert_eq!(
            reopened.find_shape(0, child).unwrap().layout.grid.column,
            Some(1)
        );
        assert_eq!(reopened.find_shape(0, child).unwrap().id, child);
        studio.undo();
        assert!(
            studio
                .doc
                .find_shape(0, child)
                .unwrap()
                .layout
                .grid
                .is_auto()
        );
    }

    #[test]
    fn typed_grid_edits_reject_stale_locked_busy_and_malformed_targets_atomically() {
        for rejection in [
            "document",
            "generation",
            "selection",
            "layout",
            "layer",
            "shape",
            "parent",
            "busy",
            "malformed",
        ] {
            let (mut studio, root, child) = studio();
            studio.selection = vec![(0, child)];
            let target = studio.layout_inspector_target().unwrap();
            let mut properties = studio.doc.find_shape(0, child).unwrap().layout.clone();
            properties.grid.column = Some(1);
            match rejection {
                "document" => studio.swap_id.push_str("-replaced"),
                "generation" => studio.mark_interaction(),
                "selection" => studio.selection = vec![(0, root)],
                "layout" => studio.doc.find_shape_mut(0, child).unwrap().layout.absolute = true,
                "layer" => studio.doc.layers[0].locked = true,
                "shape" => studio.doc.find_shape_mut(0, child).unwrap().locked = true,
                "parent" => studio.doc.find_shape_mut(0, root).unwrap().locked = true,
                "busy" => studio.updates.freezing = true,
                "malformed" => properties.grid.column_span = 0,
                _ => unreachable!(),
            }
            let before = serde_json::to_string(&studio.doc).unwrap();
            assert!(
                !studio.apply_layout_inspector(&target, &properties),
                "{rejection}"
            );
            assert_eq!(
                serde_json::to_string(&studio.doc).unwrap(),
                before,
                "{rejection}"
            );
            assert_eq!(studio.history.len(), 0, "{rejection}");
        }
    }
}

impl Studio {
    /// Capture the layout inspector's destination.
    /// Args: current document and selection. Returns a typed target, or None for missing shapes.
    pub(crate) fn layout_inspector_target(&self) -> Option<LayoutInspectorTarget> {
        if self.selection.is_empty() {
            return None;
        }
        Some(LayoutInspectorTarget {
            document: self.swap_id.clone(),
            generation: self.canvas_gen,
            selection: self.selection.clone(),
            layouts: self
                .selection
                .iter()
                .map(|&(layer, id)| {
                    self.doc
                        .find_shape(layer, id)
                        .map(|shape| shape.layout.clone())
                })
                .collect::<Option<Vec<_>>>()?,
        })
    }

    fn validate_layout_inspector_target(
        &self,
        target: &LayoutInspectorTarget,
    ) -> Result<(), String> {
        if target.document != self.swap_id
            || target.generation != self.canvas_gen
            || target.selection != self.selection
        {
            return Err("The layout selection changed; select the object again".into());
        }
        if self.op.is_some()
            || self.deformation.is_some()
            || self.free_transform.is_some()
            || self.pending_nav.is_some()
            || self.file_dialog_pending()
            || self.updates.freezing
        {
            return Err("Finish the current operation before changing layout".into());
        }
        for (&(layer, id), before) in target.selection.iter().zip(&target.layouts) {
            if !self.doc.layer_editable(layer) {
                return Err("Show and unlock the object's layer first".into());
            }
            let shape = self
                .doc
                .find_shape(layer, id)
                .ok_or("The layout object no longer exists")?;
            if &shape.layout != before {
                return Err("The layout changed; select the object again".into());
            }
            if !shape.visible || shape.locked {
                return Err("Show and unlock the layout object first".into());
            }
            let mut root = id;
            let mut seen = std::collections::HashSet::new();
            while let Some(parent) = self
                .doc
                .find_shape(layer, root)
                .and_then(|s| s.layout.parent)
            {
                if !seen.insert(root) {
                    return Err("The layout hierarchy contains a cycle".into());
                }
                root = parent;
                let ancestor = self
                    .doc
                    .find_shape(layer, root)
                    .ok_or("The layout parent no longer exists")?;
                if ancestor.locked || !ancestor.visible {
                    return Err("Show and unlock the parent frame first".into());
                }
            }
            if std::iter::once(root)
                .chain(layout::descendants(&self.doc, layer, root))
                .any(|id| self.doc.find_shape(layer, id).is_some_and(|s| s.locked))
            {
                return Err("Unlock the objects in this frame before changing its layout".into());
            }
        }
        Ok(())
    }

    /// Apply typed layout properties as one undoable edit.
    /// Args: captured target and edited primary properties. Returns false for stale, locked or busy targets.
    pub(crate) fn apply_layout_inspector(
        &mut self,
        target: &LayoutInspectorTarget,
        properties: &FrameLayout,
    ) -> bool {
        let prepare = || -> Result<Vec<Cmd>, String> {
            self.validate_layout_inspector_target(target)?;
            let primary = target
                .layouts
                .first()
                .ok_or("Select a layout object first")?;
            let mut preview = self.doc.layout_snapshot();
            let mut commands = Vec::new();
            for (&(layer, id), before) in target.selection.iter().zip(&target.layouts) {
                let mut after = before.clone();
                macro_rules! copy_changed { ($($field:ident),*) => { $(
                    if primary.$field != properties.$field { after.$field = properties.$field.clone(); }
                )* }; }
                copy_changed!(
                    width,
                    height,
                    min_width,
                    max_width,
                    min_height,
                    max_height,
                    clip,
                    absolute,
                    constraint_x,
                    constraint_y,
                    stack,
                    grid,
                    breakpoints,
                    text_size,
                    aspect_ratio,
                    image
                );
                if before == &after {
                    continue;
                }
                if after.stack.is_some() && !after.frame {
                    return Err("Auto layout requires a frame".into());
                }
                if !after.grid.is_auto() {
                    let parent = after
                        .parent
                        .and_then(|parent| preview.find_shape(layer, parent));
                    if !parent
                        .and_then(|s| s.layout.stack.as_ref())
                        .is_some_and(|s| s.flow == layout::StackFlow::Grid)
                    {
                        return Err("Grid placement requires a parent with Grid layout".into());
                    }
                }
                preview
                    .find_shape_mut(layer, id)
                    .ok_or("The layout object no longer exists")?
                    .layout = after.clone();
                commands.push(Cmd::SetLayout {
                    layer,
                    id,
                    before: before.clone(),
                    after,
                });
            }
            layout::validate_hierarchy(&preview)?;
            Ok(commands)
        };
        match prepare() {
            Ok(commands) => {
                if !commands.is_empty() {
                    self.commit(Cmd::Batch(commands));
                }
                true
            }
            Err(error) => {
                self.status = error;
                false
            }
        }
    }

    pub fn selected_frame(&self) -> Option<(usize, u64)> {
        self.selection
            .iter()
            .copied()
            .find(|&(li, id)| self.doc.find_shape(li, id).is_some_and(|s| s.layout.frame))
    }

    pub fn wrap_selection_frame(&mut self) {
        self.wrap_selection_frame_with_stack(None);
    }

    pub(crate) fn wrap_selection_frame_with_stack(&mut self, stack: Option<AutoStack>) {
        let mut ids = self.selection.clone();
        if ids.is_empty() {
            self.status = "Select objects to wrap in a frame".into();
            return;
        }
        let layer = ids[0].0;
        // Selecting both a frame and a descendant must retain their relationship.
        let selected: std::collections::HashSet<_> = ids.iter().copied().collect();
        ids.retain(|(li, id)| {
            !selected.iter().any(|(pli, parent)| {
                pli == li
                    && parent != id
                    && layout::descendants(&self.doc, *pli, *parent).contains(id)
            })
        });
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
        frame.layout.stack = stack;
        let parents: Vec<_> = ids
            .iter()
            .filter_map(|(li, id)| self.doc.find_shape(*li, *id).map(|s| s.layout.parent))
            .collect();
        if parents.iter().all(|p| Some(p) == parents.first()) {
            frame.layout.parent = parents.first().copied().flatten();
        }
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
        let mut preview = self.doc.layout_snapshot();
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
        let mut preview = self.doc.layout_snapshot();
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
        self.reparent_layout_selection(Some(frame_id));
    }

    pub fn export_selected_frame_png(&mut self) {
        let Some((li, id)) = self.selected_frame() else {
            self.status = "Select a frame to export".into();
            return;
        };
        self.request_file_dialog(
            || crate::project::dialog_export("PNG", "png"),
            move |_, studio, path| match compositor::export_frame_png(
                &studio.doc,
                li,
                id,
                studio.export_scale,
            ) {
                Ok(bytes) => {
                    if let Err(e) = std::fs::write(&path, bytes) {
                        studio.status = format!("write failed: {e}");
                    } else {
                        studio.status = format!("exported {}", path.display());
                    }
                }
                Err(e) => studio.status = format!("export failed: {e}"),
            },
        );
    }

    pub fn export_selected_frame_svg(&mut self) {
        let Some((li, id)) = self.selected_frame() else {
            self.status = "Select a frame to export".into();
            return;
        };
        self.request_file_dialog(
            || crate::project::dialog_export("SVG", "svg"),
            move |_, studio, path| match crate::svg::export_frame(&studio.doc, li, id) {
                Ok(s) => {
                    studio.status = match std::fs::write(&path, s) {
                        Ok(()) => format!("exported {}", path.display()),
                        Err(e) => format!("export failed: {e}"),
                    };
                }
                Err(e) => studio.status = format!("export failed: {e}"),
            },
        );
    }

    pub fn export_selected_frame_html(&mut self) {
        let Some((li, id)) = self.selected_frame() else {
            self.status = "Select a frame to export".into();
            return;
        };
        self.request_file_dialog(
            || crate::project::dialog_export("HTML", "html"),
            move |_, studio, path| match crate::layout_export::export_html(&studio.doc, li, id) {
                Ok(s) => {
                    studio.status = match std::fs::write(&path, s) {
                        Ok(()) => format!("exported {}", path.display()),
                        Err(e) => format!("export failed: {e}"),
                    };
                }
                Err(e) => studio.status = format!("export failed: {e}"),
            },
        );
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
