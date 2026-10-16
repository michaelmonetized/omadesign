//! Actions records actual editor operations and keeps interrupted saves recoverable.
use super::{
    fields,
    icons::{self, ph},
    welcome,
};
use crate::{
    actions::{Action, Library, Step},
    app::Studio,
};
use eframe::egui::{self, Id, Ui};
use std::path::PathBuf;

#[derive(Clone, Default)]
struct State {
    path: PathBuf,
    book: Option<Library>,
    selected: Option<u64>,
    name: String,
    target: Option<Action>,
    error: String,
    canvas_focus: Option<CanvasFocus>,
}
#[derive(Clone)]
struct CanvasFocus {
    document: String,
    revision: u64,
    from: Option<Id>,
    frame: u64,
}

fn return_to_canvas(ui: &Ui, studio: &Studio, state: &mut State) {
    let Some(request) = state.canvas_focus.take() else {
        return;
    };
    if request.document != studio.swap_id
        || request.revision != studio.canvas_gen
        || studio.file_dialog_pending()
        || studio.updates.freezing
        || studio.op.is_some()
        || studio.type_edit.is_some()
    {
        return;
    }
    let canvas = Id::new("studio-canvas");
    let focused = ui.ctx().memory(|m| m.focused());
    if focused == Some(canvas) || (focused.is_some() && focused != request.from) {
        return;
    }
    if ui.ctx().cumulative_frame_nr() <= request.frame
        || ui.input(|i| {
            i.pointer.any_down() || i.key_down(egui::Key::Enter) || i.key_down(egui::Key::Space)
        })
    {
        state.canvas_focus = Some(request);
        ui.ctx().request_repaint();
        return;
    }
    if let Some(response) = ui.ctx().read_response(canvas)
        && response.enabled()
        && ui.ctx().memory(|m| m.allows_interaction(response.layer_id))
    {
        response.request_focus();
    }
}
fn load(path: PathBuf) -> State {
    match Library::load(&path) {
        Ok(book) => State {
            path,
            book: Some(book),
            ..Default::default()
        },
        Err(error) => State {
            path,
            error,
            ..Default::default()
        },
    }
}
fn icon(ui: &mut Ui, id: Id, glyph: &str, name: &str, enabled: bool) -> bool {
    let response = fields::native_scope(ui, id, |ui| {
        ui.add_enabled(
            enabled,
            egui::Button::new(egui::RichText::new(glyph).font(icons::font(16.))).frame(false),
        )
    })
    .inner;
    welcome::name_control(ui, &response, id.with("label"), name);
    response.on_hover_text(name).clicked()
}
fn save(state: &mut State, mut after: Library) -> bool {
    match after.save(&state.path) {
        Ok(()) => {
            state.book = Some(after);
            state.error.clear();
            true
        }
        Err(error) => {
            state.error = error;
            false
        }
    }
}
fn finish(studio: &mut Studio, state: &mut State, as_new: bool) {
    studio
        .actions
        .pause("Recording stopped. Save or discard the pending steps");
    let result = (|| -> Result<(Library, u64), String> {
        let recording = studio
            .actions
            .recording
            .as_ref()
            .ok_or("There is no pending recording")?;
        let mut book = Library::load(&state.path)?;
        let id = if as_new {
            let mut steps = state
                .target
                .as_ref()
                .map_or_else(Vec::new, |a| a.steps.clone());
            steps.extend(recording.steps.clone());
            let base = state.target.as_ref().map_or("Action", |a| a.name.as_str());
            book.add(
                &format!("{} recording", base.chars().take(54).collect::<String>()),
                steps,
            )?
        } else {
            let target = state
                .target
                .as_ref()
                .ok_or("Save the recovered recording as a new action")?;
            let action = book
                .entries
                .iter_mut()
                .find(|a| a.id == target.id)
                .filter(|a| **a == *target)
                .ok_or("The recorded action changed on disk. Save the recording as a new action")?;
            action.steps.extend(recording.steps.clone());
            action.id
        };
        book.save(&state.path)?;
        Ok((book, id))
    })();
    match result {
        Ok((book, id)) => {
            state.name = book
                .entries
                .iter()
                .find(|a| a.id == id)
                .unwrap()
                .name
                .clone();
            state.book = Some(book);
            state.selected = Some(id);
            state.target = None;
            state.error.clear();
            studio.actions.recording = None;
        }
        Err(error) => state.error = error,
    }
}

/// Draw native recording and playback controls for a portable action library.
/// Args: pane UI and current editor. Returns nothing; playback owns its atomic document transaction.
pub(super) fn show(ui: &mut Ui, studio: &mut Studio) {
    let path = ui
        .data(|d| d.get_temp::<PathBuf>(Id::new("action-library-file")))
        .unwrap_or_else(|| crate::palette::path().with_file_name("actions.json"));
    let owner = Id::new(("action-library", &path));
    let mut state = ui
        .data_mut(|d| d.remove_temp::<State>(owner))
        .unwrap_or_else(|| load(path.clone()));
    return_to_canvas(ui, studio, &mut state);
    studio.actions.check_document(&studio.swap_id);
    let selection = studio.action_selection();
    studio.actions.check_selection(&selection);
    let recording = studio.actions.recording.is_some();
    let paused = studio
        .actions
        .recording
        .as_ref()
        .is_some_and(|r| r.paused.is_some());
    let entries = state
        .book
        .as_ref()
        .map_or_else(Vec::new, |b| b.entries.clone());
    let mut play = None;
    for action in &entries {
        let response = fields::native_scope(ui, owner.with(action.id), |ui| {
            ui.add_enabled(
                !recording || paused,
                egui::Button::new(&action.name)
                    .selected(state.selected == Some(action.id))
                    .min_size(egui::vec2(ui.available_width(), 24.)),
            )
        })
        .inner;
        welcome::name_control(
            ui,
            &response,
            owner.with((action.id, "label")),
            &format!("Action {}", action.name),
        );
        if response.clicked() {
            state.selected = Some(action.id);
            state.name = action.name.clone();
        }
        if response.double_clicked() {
            play = Some(action.steps.clone());
        }
    }
    let selected = entries.iter().find(|a| Some(a.id) == state.selected);
    ui.horizontal_wrapped(|ui| {
        if icon(
            ui,
            owner.with("new"),
            ph::PLUS,
            "New action",
            !recording && state.book.is_some() && entries.len() < 256,
        ) && let Some(mut after) = state.book.clone()
        {
            match after.add(&format!("Action {}", after.next + 1), Vec::new()) {
                Ok(id) => {
                    let name = after.entries.last().unwrap().name.clone();
                    if save(&mut state, after) {
                        state.selected = Some(id);
                        state.name = name;
                    }
                }
                Err(error) => state.error = error,
            }
        }
        if icon(
            ui,
            owner.with("play"),
            ph::PLAY,
            "Play selected action",
            !recording && selected.is_some_and(|a| !a.steps.is_empty()),
        ) {
            play = selected.map(|a| a.steps.clone());
        }
        if icon(
            ui,
            owner.with("record"),
            ph::CIRCLE,
            "Record vector commands",
            !recording && selected.is_some_and(|a| a.steps.len() < 32),
        ) && let Some(action) = selected
        {
            match studio.start_action_recording(32 - action.steps.len()) {
                Ok(()) => {
                    match studio
                        .actions
                        .bind_library(state.book.clone().unwrap(), action.id)
                    {
                        Ok(()) => {
                            state.target = Some(action.clone());
                            state.error.clear();
                            state.canvas_focus = Some(CanvasFocus {
                                document: studio.swap_id.clone(),
                                revision: studio.canvas_gen,
                                from: ui.ctx().memory(|m| m.focused()),
                                frame: ui.ctx().cumulative_frame_nr(),
                            });
                            ui.ctx().request_repaint();
                        }
                        Err(error) => {
                            studio.actions.recording = None;
                            state.error = error;
                        }
                    }
                }
                Err(error) => state.error = error,
            }
        }
        if icon(
            ui,
            owner.with("stop"),
            ph::FLOPPY_DISK,
            "Stop and save recording",
            recording,
        ) {
            finish(studio, &mut state, false);
        }
        if icon(
            ui,
            owner.with("recover"),
            ph::COPY,
            "Save recording as a new action",
            recording,
        ) {
            finish(studio, &mut state, true);
        }
        if icon(
            ui,
            owner.with("discard"),
            ph::X,
            "Discard pending recording",
            recording,
        ) {
            studio.actions.recording = None;
            state.target = None;
            state.error.clear();
        }
        if icon(
            ui,
            owner.with("delete"),
            ph::TRASH,
            "Delete action",
            (!recording || paused) && selected.is_some(),
        ) && let Some(mut after) = state.book.clone()
        {
            after.entries.retain(|a| Some(a.id) != state.selected);
            if save(&mut state, after) {
                state.selected = None;
                state.name.clear();
            }
        }
        if icon(
            ui,
            owner.with("reload"),
            ph::ARROW_CLOCKWISE,
            "Reload actions",
            true,
        ) {
            let target = state.target.take();
            state = load(path.clone());
            state.target = target;
        }
    });
    if let Some(action) = selected {
        let mut rename = false;
        ui.add_enabled_ui(!recording, |ui| {
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut state.name)
                        .id(owner.with((action.id, "name")))
                        .desired_width((ui.available_width() - 28.).max(24.)),
                );
                welcome::name_control(
                    ui,
                    &response,
                    owner.with((action.id, "name-label")),
                    "Action name",
                );
                rename = (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                    || icon(ui, owner.with("rename"), ph::PENCIL, "Rename action", true);
            })
        });
        if rename
            && let Some(mut after) = state.book.clone()
            && let Some(entry) = after.entries.iter_mut().find(|a| a.id == action.id)
        {
            entry.name = state.name.trim().into();
            save(&mut state, after);
        }
        let mut remove = None;
        for (index, step) in action.steps.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{}  {}", index + 1, step.name()));
                if icon(
                    ui,
                    owner.with((action.id, index, "remove")),
                    ph::X,
                    "Remove action step",
                    !recording,
                ) {
                    remove = Some(index);
                }
            });
        }
        if let Some(index) = remove
            && let Some(mut after) = state.book.clone()
            && let Some(entry) = after.entries.iter_mut().find(|a| a.id == action.id)
        {
            entry.steps.remove(index);
            save(&mut state, after);
        }
        ui.add_enabled_ui(!recording && action.steps.len() < 32, |ui| {
            let response = fields::native_scope(ui, owner.with("add-step"), |ui| {
                ui.menu_button(egui::RichText::new(ph::PLUS).font(icons::font(16.)), |ui| {
                    for step in Step::defaults() {
                        if ui.button(step.name()).clicked()
                            && let Some(mut after) = state.book.clone()
                            && let Some(entry) =
                                after.entries.iter_mut().find(|a| a.id == action.id)
                        {
                            entry.steps.push(step);
                            save(&mut state, after);
                            ui.close();
                        }
                    }
                })
            })
            .inner
            .response;
            welcome::name_control(
                ui,
                &response,
                owner.with("add-step-label"),
                "Add an explicit action step",
            );
            response.on_hover_text("Add an explicit action step");
        });
    }
    if let Some(recording) = &studio.actions.recording {
        if let Some(reason) = &recording.paused {
            ui.colored_label(ui.visuals().warn_fg_color, reason);
        } else {
            ui.colored_label(
                ui.visuals().selection.stroke.color,
                format!("Recording · {} steps", recording.steps.len()),
            );
        }
        for step in &recording.steps {
            ui.label(step.name());
        }
    }
    if let Some(steps) = play {
        if let Err(error) = studio.play_action(&steps) {
            state.error = error;
        } else {
            state.error.clear();
        }
    }
    if !state.error.is_empty() {
        ui.colored_label(ui.visuals().error_fg_color, &state.error);
    }
    ui.data_mut(|d| d.insert_temp(owner, state));
}

#[cfg(test)]
mod tests {
    use super::super::welcome::keyboard_tests as keys;
    use super::*;
    use egui::{Event, Key, Modifiers};
    fn context(path: &std::path::Path) -> egui::Context {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        super::super::theme::apply(&ctx);
        ctx.data_mut(|d| d.insert_temp(Id::new("action-library-file"), path.to_owned()));
        ctx
    }
    #[test]
    fn craft_actions_record_returns_real_canvas_focus_and_records_native_group_shortcuts() {
        let folder = std::env::temp_dir().join(format!(
            "native-action-canvas-{}",
            crate::document::next_id()
        ));
        let path = folder.join("actions.json");
        let ctx = context(&path);
        let mut studio = crate::app::symbols::tests::fixture();
        studio.persona = crate::tools::Persona::Design;
        let mut frame = |events| {
            keys::render(&ctx, events, |ui| {
                studio.handle_shortcuts(ui.ctx());
                ui.columns(2, |columns| {
                    show(&mut columns[0], &mut studio);
                    super::super::canvas::show(&mut columns[1], &mut studio);
                });
            })
        };
        frame(vec![]);
        frame(vec![]);
        keys::tab_to(&ctx, "New action", &mut frame);
        frame(keys::key(Key::Space, Modifiers::NONE));
        frame(vec![]);
        keys::tab_to(&ctx, "Record vector commands", &mut frame);
        frame(keys::key(Key::Space, Modifiers::NONE));
        frame(vec![]);
        frame(vec![]);
        let canvas = ctx.read_response(Id::new("studio-canvas")).unwrap();
        assert!(canvas.enabled());
        assert_eq!(ctx.memory(|m| m.focused()), Some(canvas.id));
        for (key, modifiers) in [
            (Key::G, Modifiers::CTRL | Modifiers::COMMAND),
            (Key::D, Modifiers::CTRL | Modifiers::COMMAND),
            (
                Key::G,
                Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
            ),
        ] {
            frame(keys::key(key, modifiers));
            frame(vec![]);
        }
        keys::tab_to(&ctx, "Stop and save recording", &mut frame);
        frame(keys::key(Key::Space, Modifiers::NONE));
        frame(vec![]);
        drop(frame);
        assert!(studio.actions.recording.is_none());
        assert_eq!(
            Library::load(&path).unwrap().entries[0].steps,
            vec![
                Step::Group,
                Step::Duplicate {
                    delta: crate::geom::Pt::ZERO
                },
                Step::Ungroup
            ]
        );
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn craft_actions_native_record_save_cold_load_and_atomic_playback() {
        let folder =
            std::env::temp_dir().join(format!("native-actions-{}", crate::document::next_id()));
        let path = folder.join("actions.json");
        let ctx = context(&path);
        let mut studio = crate::app::symbols::tests::fixture();
        studio.persona = crate::tools::Persona::Design;
        let mut frame = |events| {
            keys::render(&ctx, events, |ui| {
                ui.set_max_width(256.);
                show(ui, &mut studio);
            })
        };
        frame(vec![]);
        frame(vec![]);
        keys::tab_to(&ctx, "New action", &mut frame);
        frame(keys::key(Key::Space, Modifiers::NONE));
        frame(vec![]);
        keys::tab_to(&ctx, "Action name", &mut frame);
        frame(keys::key(Key::A, Modifiers::CTRL | Modifiers::COMMAND));
        frame(vec![Event::Paste("Recorded offset Ω".into())]);
        frame(keys::key(Key::Enter, Modifiers::NONE));
        frame(vec![]);
        keys::tab_to(&ctx, "Record vector commands", &mut frame);
        frame(keys::key(Key::Space, Modifiers::NONE));
        frame(vec![]);
        drop(frame);
        assert!(studio.actions.recording.is_some());
        studio.duplicate_selection_by(crate::geom::Pt::new(30., 40.));
        studio.flip_selection(true);
        let mut frame = |events| {
            keys::render(&ctx, events, |ui| {
                ui.set_max_width(256.);
                show(ui, &mut studio);
            })
        };
        frame(vec![]);
        keys::tab_to(&ctx, "Stop and save recording", &mut frame);
        frame(keys::key(Key::Space, Modifiers::NONE));
        frame(vec![]);
        drop(frame);
        assert!(studio.actions.recording.is_none());
        let saved = Library::load(&path).unwrap();
        assert_eq!(saved.entries[0].name, "Recorded offset Ω");
        assert_eq!(saved.entries[0].steps.len(), 2);
        let cold = context(&path);
        let mut destination = crate::app::symbols::tests::fixture();
        destination.persona = crate::tools::Persona::Design;
        let before = crate::project::encode(&destination.doc).unwrap();
        let mut frame = |events| {
            keys::render(&cold, events, |ui| {
                ui.set_max_width(256.);
                show(ui, &mut destination);
            })
        };
        frame(vec![]);
        frame(vec![]);
        keys::tab_to(&cold, "Action Recorded offset Ω", &mut frame);
        frame(keys::key(Key::Space, Modifiers::NONE));
        frame(vec![]);
        keys::tab_to(&cold, "Play selected action", &mut frame);
        frame(keys::key(Key::Space, Modifiers::NONE));
        frame(vec![]);
        drop(frame);
        assert_eq!(destination.history.len(), 1);
        let applied = crate::project::encode(&destination.doc).unwrap();
        assert_eq!(
            crate::project::encode(&crate::project::decode(&applied).unwrap()).unwrap(),
            applied
        );
        destination.undo();
        assert_eq!(crate::project::encode(&destination.doc).unwrap(), before);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn craft_actions_recording_save_conflict_retains_steps_and_save_as_new_preserves_both_actions()
    {
        let folder = std::env::temp_dir().join(format!(
            "action-recording-conflict-{}",
            crate::document::next_id()
        ));
        let path = folder.join("actions.json");
        let mut book = Library::default();
        let id = book.add("Original", vec![Step::Group]).unwrap();
        book.save(&path).unwrap();
        let mut state = load(path.clone());
        state.selected = Some(id);
        state.target = Some(book.entries[0].clone());
        let mut studio = crate::app::symbols::tests::fixture();
        studio.persona = crate::tools::Persona::Design;
        studio.start_action_recording(31).unwrap();
        studio.flip_selection(true);
        let mut external = Library::load(&path).unwrap();
        external.entries[0].name = "External rename".into();
        external.save(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        finish(&mut studio, &mut state, false);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert!(!state.error.is_empty());
        assert_eq!(studio.actions.recording.as_ref().unwrap().steps.len(), 1);
        finish(&mut studio, &mut state, true);
        assert!(studio.actions.recording.is_none());
        let saved = Library::load(&path).unwrap();
        assert_eq!(saved.entries.len(), 2);
        assert_eq!(saved.entries[0].name, "External rename");
        assert_eq!(
            saved.entries[1].steps,
            vec![Step::Group, Step::Flip { horizontal: true }]
        );
        std::fs::remove_dir_all(folder).unwrap();
    }
}
