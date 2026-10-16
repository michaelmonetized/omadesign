//! Action playback stages ordinary editor commands in one reversible transaction.
use super::*;
use crate::actions::Step;

fn ready(studio: &Studio) -> Result<(), String> {
    if !matches!(studio.persona, Persona::Design | Persona::Layout) {
        return Err("These actions operate on vector artwork in Design or Layout".into());
    }
    if studio.pixel_sel.is_some() || studio.pixel_edit.is_some() {
        return Err(
            "Finish pixel editing and clear its selection before using this vector action".into(),
        );
    }
    if studio.op.is_some()
        || studio.type_edit.is_some()
        || studio.deformation.is_some()
        || studio.free_transform.is_some()
        || studio.pending_nav.is_some()
        || studio.file_dialog_pending()
        || studio.updates.freezing
    {
        return Err("Finish the current edit before using Actions".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Studio {
        let mut studio = crate::app::symbols::tests::fixture();
        studio.persona = Persona::Design;
        studio
    }
    fn pixels(studio: &Studio) -> Vec<u8> {
        crate::compositor::render_export(&studio.doc, 1)
            .unwrap()
            .data()
            .to_vec()
    }

    #[test]
    fn craft_actions_record_actual_commands_and_play_one_undo_with_native_reopen() {
        let mut source = fixture();
        source.start_action_recording(32).unwrap();
        source.duplicate_selection_by(Pt::new(80., 60.));
        source.flip_selection(true);
        let recording = source.actions.recording.take().unwrap();
        assert!(recording.paused.is_none());
        assert_eq!(
            recording.steps,
            vec![
                Step::Duplicate {
                    delta: Pt::new(80., 60.)
                },
                Step::Flip { horizontal: true }
            ]
        );
        let expected = pixels(&source);
        let mut destination = fixture();
        let before = crate::project::encode(&destination.doc).unwrap();
        let selection = destination.selection.clone();
        assert!(destination.play_action(&recording.steps).unwrap());
        assert_eq!(destination.history.len(), 1);
        assert_eq!(pixels(&destination), expected);
        let saved = crate::project::encode(&destination.doc).unwrap();
        assert_eq!(
            crate::project::encode(&crate::project::decode(&saved).unwrap()).unwrap(),
            saved
        );
        destination.undo();
        assert_eq!(crate::project::encode(&destination.doc).unwrap(), before);
        destination.selection = selection;
        destination.redo();
        assert_eq!(crate::project::encode(&destination.doc).unwrap(), saved);
    }

    #[test]
    fn craft_actions_explicit_unchanged_appearance_is_recorded_and_applies_to_another_selection() {
        let mut source = fixture();
        source.selection.truncate(1);
        let appearance = source.capture_graphic_style().unwrap();
        source.start_action_recording(32).unwrap();
        assert!(!source.apply_graphic_style(&appearance).unwrap());
        assert_eq!(source.history.len(), 0);
        let steps = source.actions.recording.take().unwrap().steps;
        assert_eq!(steps, vec![Step::Appearance(Box::new(appearance))]);
        let mut destination = fixture();
        let before = crate::project::encode(&destination.doc).unwrap();
        assert!(destination.play_action(&steps).unwrap());
        assert_eq!(destination.history.len(), 1);
        destination.undo();
        assert_eq!(crate::project::encode(&destination.doc).unwrap(), before);
    }

    #[test]
    fn craft_actions_failed_middle_step_restores_document_selection_dirty_and_original_redo() {
        let mut studio = fixture();
        studio.flip_selection(true);
        let flipped = crate::project::encode(&studio.doc).unwrap();
        studio.undo();
        studio.dirty = false;
        let before = crate::project::encode(&studio.doc).unwrap();
        let selection = studio.selection.clone();
        let error = studio
            .play_action(&[
                Step::Duplicate {
                    delta: Pt::new(20., 10.),
                },
                Step::Ungroup,
            ])
            .unwrap_err();
        assert!(error.contains("Step 2"));
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
        assert_eq!(studio.selection, selection);
        assert!(!studio.dirty);
        assert!(studio.history.can_redo());
        assert_eq!(studio.history.len(), 0);
        studio.redo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), flipped);
    }

    #[test]
    fn craft_actions_outline_more_than_history_limit_still_undoes_every_object() {
        let mut studio = fixture();
        studio.doc.layers = vec![Layer::vector("Many objects")];
        studio.selection.clear();
        for index in 0..250 {
            let shape = Shape::new(
                Geom::Rect {
                    origin: Pt::new((index % 25) as f32 * 8., (index / 25) as f32 * 8.),
                    size: Pt::splat(5.),
                    radius: 0.,
                },
                Style::default(),
            );
            studio.selection.push((0, shape.id));
            studio.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
        }
        let before = crate::project::encode(&studio.doc).unwrap();
        assert!(
            studio
                .play_action(&[Step::Outline { selected: None }])
                .unwrap()
        );
        assert_eq!(studio.history.len(), 1);
        assert!(
            studio.doc.layers[0]
                .kind
                .shapes()
                .unwrap()
                .iter()
                .all(|s| matches!(s.geom, Geom::Path { .. } | Geom::Paths { .. }))
        );
        studio.undo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    }

    #[test]
    fn craft_actions_unsupported_edit_or_uncaptured_selection_pauses_without_losing_steps() {
        let mut studio = fixture();
        studio.start_action_recording(32).unwrap();
        studio.flip_selection(true);
        let (layer, id) = studio.selection[0];
        let before = studio.doc.find_shape(layer, id).unwrap().opacity;
        studio.commit(Cmd::SetOpacity {
            layer,
            id,
            before,
            after: 0.5,
        });
        studio.duplicate_selection();
        let recording = studio.actions.recording.take().unwrap();
        assert_eq!(recording.steps.len(), 1);
        assert!(recording.paused.unwrap().contains("cannot be recorded"));
        studio.start_action_recording(32).unwrap();
        studio.flip_selection(true);
        studio.selection.reverse();
        studio.bring_to_front();
        let recording = studio.actions.recording.take().unwrap();
        assert_eq!(recording.steps.len(), 1);
        assert!(recording.paused.unwrap().contains("Selection changed"));
        studio.start_action_recording(32).unwrap();
        studio.flip_selection(true);
        studio.individual_object = studio.selection.first().copied();
        studio.bring_to_front();
        let recording = studio.actions.recording.take().unwrap();
        assert_eq!(recording.steps.len(), 1);
        assert!(recording.paused.unwrap().contains("Selection changed"));
    }

    #[test]
    fn craft_actions_large_appearance_pauses_at_real_library_budget_without_losing_prior_steps() {
        let mut studio = fixture();
        let mut appearance = studio.capture_graphic_style().unwrap();
        appearance.style.fill = Fill::Pattern(crate::svg_source::SvgPattern {
            definitions: format!("<pattern id=\"ink\" width=\"1\" height=\"1\"><!--{}--><rect width=\"1\" height=\"1\" fill=\"red\"/></pattern>", "x".repeat(2_100_000)).into(),
            id: "ink".into(), transform: [1., 0., 0., 1., 0., 0.], opacity: 1.,
        });
        let mut book = crate::actions::Library::default();
        let target = book.add("Recording", Vec::new()).unwrap();
        book.add(
            "Other saved action",
            vec![Step::Appearance(Box::new(appearance.clone()))],
        )
        .unwrap();
        studio.start_action_recording(32).unwrap();
        studio.actions.bind_library(book, target).unwrap();
        studio.flip_selection(true);
        assert!(studio.apply_graphic_style(&appearance).unwrap());
        let recording = studio.actions.recording.take().unwrap();
        assert_eq!(recording.steps, vec![Step::Flip { horizontal: true }]);
        assert!(recording.paused.unwrap().contains("4 MiB"));
        assert_eq!(studio.history.len(), 2);
    }

    #[test]
    fn craft_actions_locked_missing_and_nonfinite_inputs_leave_artwork_and_history_intact() {
        let mut studio = fixture();
        let (layer, id) = studio.selection[1];
        studio.doc.find_shape_mut(layer, id).unwrap().locked = true;
        let before = crate::project::encode(&studio.doc).unwrap();
        assert!(
            studio
                .play_action(&[Step::Duplicate { delta: Pt::ZERO }])
                .is_err()
        );
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
        assert_eq!(studio.history.len(), 0);
        studio.doc.find_shape_mut(layer, id).unwrap().locked = false;
        let before = crate::project::encode(&studio.doc).unwrap();
        assert!(
            studio
                .play_action(&[Step::Duplicate {
                    delta: Pt::new(f32::NAN, 0.)
                }])
                .is_err()
        );
        studio.selection.push((0, u64::MAX));
        assert!(studio.play_action(&[Step::Front]).is_err());
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
        assert_eq!(studio.history.len(), 0);
    }
}
fn targets(studio: &Studio) -> Result<Vec<(usize, u64)>, String> {
    ready(studio)?;
    if studio.selection.is_empty() {
        return Err("Select vector artwork before playing an action".into());
    }
    let mut targets = studio.selection.clone();
    for &(layer, id) in &studio.selection {
        targets.extend(
            crate::layout::descendants(&studio.doc, layer, id)
                .into_iter()
                .map(|id| (layer, id)),
        );
    }
    targets.sort_unstable();
    targets.dedup();
    if targets.iter().any(|&(layer, id)| {
        !studio.doc.layer_visible(layer)
            || !super::layout_components::component_object_editable(&studio.doc, layer, id)
            || studio
                .doc
                .find_shape(layer, id)
                .is_none_or(|s| !s.visible || s.guide)
    }) {
        return Err("The action needs existing, visible, unlocked vector artwork".into());
    }
    Ok(targets)
}

impl Studio {
    /// Identify the actual object and layer owners used by a recording.
    /// Args: none. Returns stable document IDs rather than mutable layer indices.
    pub(crate) fn action_selection(&self) -> crate::actions::Selection {
        crate::actions::Selection {
            objects: self
                .selection
                .iter()
                .map(|&(layer, id)| (self.doc.layers.get(layer).map_or(0, |l| l.id), id))
                .collect(),
            layer: self.selected_layer,
            active: self
                .active_layer
                .and_then(|index| self.doc.layers.get(index).map(|l| l.id)),
            individual: self
                .individual_object
                .and_then(|(layer, id)| self.doc.layers.get(layer).map(|l| (l.id, id))),
        }
    }
    /// Capture one explicit semantic operation only after its existing command body accepts an edit.
    /// Args: operation and unchanged editor command callback. Returns that callback's result.
    pub(super) fn with_action_step<R>(
        &mut self,
        step: Step,
        edit: impl FnOnce(&mut Self) -> R,
    ) -> R {
        if self
            .actions
            .recording
            .as_ref()
            .is_none_or(|r| r.paused.is_some())
        {
            return edit(self);
        }
        let root = self.actions.depth == 0;
        let mut accepted_noop = false;
        if root && self.actions.recording.is_some() {
            self.actions.check_document(&self.swap_id);
            let selection = self.action_selection();
            self.actions.check_selection(&selection);
            match self.check_action_step(&step) {
                Ok(value) => accepted_noop = value,
                Err(error) => self.actions.pause(&error),
            }
        }
        let before = self.actions.serial;
        self.actions.depth += 1;
        let result = edit(self);
        self.actions.depth -= 1;
        if root
            && (before != self.actions.serial || accepted_noop)
            && self.actions.recording.is_some()
        {
            let selection = self.action_selection();
            self.actions.append(&self.swap_id, step, selection);
        }
        result
    }

    /// Begin recording accepted vector commands in the current document.
    /// Args: remaining action capacity. Returns success without changing artwork or history.
    pub(crate) fn start_action_recording(&mut self, limit: usize) -> Result<(), String> {
        targets(self)?;
        let selection = self.action_selection();
        self.actions.start(self.swap_id.clone(), selection, limit)
    }

    /// Outline the current selection through the existing per-object conversion route.
    /// Args: none. Returns nothing; recording treats this as one semantic operation.
    pub(crate) fn outline_selection(&mut self) {
        self.with_action_step(Step::Outline { selected: None }, |studio| {
            for (layer, id) in studio.selection.clone() {
                studio.convert_object_to_path(layer, id);
            }
        });
    }

    fn check_action_step(&self, step: &Step) -> Result<bool, String> {
        let concrete = targets(self)?;
        step.validate()?;
        match step {
            Step::Duplicate { delta } => {
                for &(layer, id) in &concrete {
                    let b = self.doc.find_shape(layer, id).unwrap().geom.bbox();
                    if [
                        b.min.x + delta.x,
                        b.max.x + delta.x,
                        b.min.y + delta.y,
                        b.max.y + delta.y,
                    ]
                    .iter()
                    .any(|v| !v.is_finite())
                    {
                        return Err("The duplicate would leave finite document coordinates".into());
                    }
                }
            }
            Step::Flip { .. } => {
                if self
                    .selection
                    .iter()
                    .any(|&(layer, id)| self.flip_target(layer, id).is_none())
                {
                    return Err("Convert text to paths before recording or playing a flip".into());
                }
            }
            Step::Outline { selected } => {
                let chosen = if let Some(index) = selected {
                    vec![
                        *self
                            .selection
                            .get(*index)
                            .ok_or("The action's selected object is missing")?,
                    ]
                } else {
                    self.selection.clone()
                };
                if chosen.iter().any(|&(layer, id)| matches!(&self.doc.find_shape(layer, id).unwrap().geom, Geom::Text(run) if run.contours.is_empty())) {
                    return Err("The selected text has no outlines to convert".into());
                }
                return Ok(chosen.iter().all(|&(layer, id)| {
                    matches!(
                        self.doc.find_shape(layer, id).unwrap().geom,
                        Geom::Path { .. } | Geom::Paths { .. }
                    )
                }));
            }
            Step::Front | Step::Back | Step::Appearance(_) => return Ok(true),
            Step::Group | Step::Ungroup => {}
        }
        Ok(false)
    }

    fn action_step(&mut self, step: &Step) -> Result<(), String> {
        self.check_action_step(step)?;
        match step {
            Step::Duplicate { delta } => self.duplicate_selection_by(*delta),
            Step::Group => self.group_selected(),
            Step::Ungroup => self.ungroup_selected(),
            Step::Flip { horizontal } => self.flip_selection(*horizontal),
            Step::Outline {
                selected: Some(index),
            } => {
                let (layer, id) = self.selection[*index];
                self.convert_object_to_path(layer, id);
            }
            Step::Outline { selected: None } => self.outline_selection(),
            Step::Front => self.bring_to_front(),
            Step::Back => self.send_to_back(),
            Step::Appearance(value) => {
                self.apply_graphic_style(value)?;
            }
        }
        Ok(())
    }

    /// Run explicit commands as one Undo entry, or restore every applied command and the original redo stack.
    /// Args: bounded action steps. Returns whether artwork changed; any refused step rolls the whole action back.
    pub(crate) fn play_action(&mut self, steps: &[Step]) -> Result<bool, String> {
        ready(self)?;
        if self.actions.recording.is_some() || self.actions.playing {
            return Err("Save or discard the recording before playing an action".into());
        }
        if steps.is_empty() || steps.len() > 32 {
            return Err("Choose an action with one to 32 steps".into());
        }
        for step in steps {
            step.validate()?;
        }
        targets(self)?;
        let original_history = std::mem::replace(&mut self.history, History::staging());
        let selection = self.selection.clone();
        let selected_layer = self.selected_layer;
        let active_layer = self.active_layer;
        let individual_object = self.individual_object;
        let node_sel = self.node_sel.clone();
        let artboard_sel = self.artboard_sel.clone();
        let expanded = self.layer_expanded.clone();
        let dirty = self.dirty;
        let tool = self.tool;
        let last_tool = self.last_tool;
        let style = self.style.clone();
        let field = self.field_history_context.take();
        self.field_history_generation = None;
        self.actions.playing = true;
        self.actions.depth += 1;
        let result = (|| -> Result<(), String> {
            for (index, step) in steps.iter().enumerate() {
                let before = self.actions.serial;
                self.action_step(step)
                    .map_err(|e| format!("Step {} ({}): {e}", index + 1, step.name()))?;
                if before == self.actions.serial
                    && matches!(step, Step::Duplicate { .. } | Step::Group | Step::Ungroup)
                {
                    return Err(format!(
                        "Step {} ({}): {}",
                        index + 1,
                        step.name(),
                        self.status
                    ));
                }
            }
            Ok(())
        })();
        self.actions.depth -= 1;
        self.actions.playing = false;
        self.field_history_context = field;
        if let Err(error) = result {
            let commands = std::mem::take(&mut self.history).into_commands();
            for command in commands.into_iter().rev() {
                self.apply_with_layer_selection(&crate::document::invert_cmd(command));
            }
            self.history = original_history;
            self.selection = selection;
            self.selected_layer = selected_layer;
            self.active_layer = active_layer;
            self.individual_object = individual_object;
            self.node_sel = node_sel;
            self.artboard_sel = artboard_sel;
            self.layer_expanded = expanded;
            self.tool = tool;
            self.last_tool = last_tool;
            self.style = style;
            self.dirty = dirty;
            self.mark();
            self.status = error.clone();
            return Err(error);
        }
        let commands = std::mem::take(&mut self.history).into_commands();
        self.history = original_history;
        if commands.is_empty() {
            return Ok(false);
        }
        self.history.finish_field();
        self.history.push(Cmd::Batch(commands));
        self.status = "Action finished · one Undo restores the original artwork".into();
        Ok(true)
    }
}
