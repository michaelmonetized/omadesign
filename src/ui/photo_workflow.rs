//! Photo transfer and preset dialogs; all filesystem work belongs to PhotoSession.
use crate::{
    app::Studio,
    photo::transfer::{AdjustmentSnapshot, Categories},
};
use eframe::egui::{self, Button, Context, Id, RichText, ScrollArea, Ui};

#[cfg(test)]
#[path = "photo_workflow_keyboard_tests.rs"]
mod keyboard_tests;

#[derive(Clone)]
enum Dialog {
    Apply(Box<AdjustmentSnapshot>),
    Presets,
}
#[derive(Clone, Default)]
struct State {
    dialog: Option<Dialog>,
    folder: bool,
    preset: Option<String>,
    query: String,
    name: String,
    categories: Categories,
    error: String,
    error_target: Option<Id>,
    owner: Option<Owner>,
}

#[derive(Clone)]
struct Owner {
    id: Id,
    document: String,
    active: Option<usize>,
    selection: Vec<usize>,
    images: Vec<(usize, std::sync::Arc<crate::photo::RgbaImage>)>,
}

impl Owner {
    /// Capture the actual photo instances without tying fields to changing edit revisions.
    /// Args: context and studio. Returns one transient owner for this open dialog.
    fn capture(ctx: &Context, studio: &Studio) -> Self {
        let selection = studio.photo.selection.iter().copied().collect::<Vec<_>>();
        let mut indices = selection.clone();
        indices.extend(studio.photo.selected);
        indices.sort_unstable();
        indices.dedup();
        Self {
            id: super::super::keyboard_controls::draft_id(ctx, "photo-workflow"),
            document: studio.swap_id.clone(),
            active: studio.photo.selected,
            selection,
            images: indices.into_iter().filter_map(|index| studio.photo.images.get(index)
                .map(|image| (index, image.full.clone()))).collect(),
        }
    }

    /// Reject a draft after its document, selection or open photo instance changes.
    /// Args: current studio. Returns whether the captured authoring target still exists.
    fn current(&self, studio: &Studio) -> bool {
        self.document == studio.swap_id && self.active == studio.photo.selected
            && self.selection == studio.photo.selection.iter().copied().collect::<Vec<_>>()
            && self.images.iter().all(|(index, full)| studio.photo.images.get(*index)
                .is_some_and(|image| std::sync::Arc::ptr_eq(full, &image.full)))
    }
}

/// Label one existing native draft and retain its identity through loading and filtering rows.
/// Args: UI, property, visible name and owned text. Returns the actual edit response.
fn text(ui: &mut Ui, owner: Id, property: &str, label: &str, value: &mut String) -> egui::Response {
    let name = ui.label(label);
    let response = ui.add(egui::TextEdit::singleline(value)
        .id(owner.with(("field", property))).desired_width(f32::INFINITY)).labelled_by(name.id);
    super::super::welcome::name_control(ui, &response, name.id, label);
    response
}

/// Preserve the identity and visible focus of one category checkbox.
/// Args: UI, existing category and visible label. Returns its actual native response.
fn category(ui: &mut Ui, owner: Id, value: &mut bool, label: &str) -> egui::Response {
    let id = owner.with(("adjustment-category", label));
    let response = super::super::fields::native_scope(ui, id, |ui| ui.checkbox(value, label)).inner;
    super::super::welcome::focus_control(ui, &response);
    response
}

/// Keep one native dialog action stable while status and validation rows change.
/// Args: UI, open draft owner, property, visible label and availability. Returns its actual response.
fn action(ui: &mut Ui, owner: Id, property: &str, label: &str, enabled: bool) -> egui::Response {
    let response = super::super::fields::native_scope(ui, owner.with(("action", property)),
        |ui| ui.add_enabled(enabled, Button::new(label))).inner;
    super::super::welcome::focus_control(ui, &response);
    response
}
fn state_id() -> Id {
    Id::new("photo-workflow")
}
fn read(ctx: &Context) -> State {
    ctx.data(|d| d.get_temp(state_id()).unwrap_or_default())
}
fn write(ctx: &Context, state: State) {
    ctx.data_mut(|d| d.insert_temp(state_id(), state));
}

pub(super) fn paste(ctx: &Context, studio: &mut Studio) {
    if studio.photo.is_batching() {
        return;
    }
    let Some(snapshot) = studio.photo.copied_adjustments.clone() else {
        studio.photo.status = "Copy adjustments from a photo first.".into();
        return;
    };
    let mut state = read(ctx);
    state.dialog = Some(Dialog::Apply(Box::new(snapshot)));
    state.folder = false;
    state.error.clear();
    state.error_target = None;
    state.owner = Some(Owner::capture(ctx, studio));
    write(ctx, state);
}
pub(super) fn presets(ctx: &Context, studio: &mut Studio) {
    studio.photo.ensure_presets_loaded();
    let mut state = read(ctx);
    state.dialog = Some(Dialog::Presets);
    state.error.clear();
    state.error_target = None;
    state.owner = Some(Owner::capture(ctx, studio));
    write(ctx, state);
}
pub(super) fn toolbar(ui: &mut Ui, studio: &mut Studio) {
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(!studio.photo.is_batching(), Button::new("Copy adjustments"))
            .on_hover_text("Copy the active photo’s adjustments · Ctrl+Shift+C")
            .clicked()
        {
            studio.photo.copy_adjustments();
        }
        if ui
            .add_enabled(
                !studio.photo.is_batching() && studio.photo.copied_adjustments.is_some(),
                Button::new("Paste…"),
            )
            .on_hover_text("Choose adjustments to paste to selected photos · Ctrl+Shift+V")
            .clicked()
        {
            paste(ui.ctx(), studio);
        }
    });
    ui.horizontal(|ui| {
        if ui
            .button("Presets…")
            .on_hover_text("Save a named look, reuse it, or import/export .omapreset files")
            .clicked()
        {
            presets(ui.ctx(), studio);
        }
        ui.label(RichText::new(format!("{} selected", studio.photo.selected_count())).small());
    });
}
fn categories(ui: &mut Ui, owner: Id, value: &mut Categories) {
    let mut responses = Vec::new();
    ui.columns(2, |c| {
        responses.push(category(&mut c[0], owner, &mut value.light, "Light / tone"));
        responses.push(category(&mut c[0], owner, &mut value.curve, "Tone curve"));
        responses.push(category(&mut c[0], owner, &mut value.color, "Color"));
        responses.push(category(&mut c[0], owner, &mut value.detail, "Effects"));
        responses.push(category(&mut c[1], owner, &mut value.hsl, "Color mixer"));
        responses.push(category(&mut c[1], owner, &mut value.split_tone, "Color grading"));
        responses.push(category(&mut c[1], owner, &mut value.crop, "Crop"));
        responses.push(category(&mut c[1], owner, &mut value.rotation, "Rotation"));
    });
    if !value.any() {
        super::super::welcome::control_error(ui, &responses.iter().collect::<Vec<_>>(), "Choose at least one adjustment category.");
    }
    ui.label(
        RichText::new("Crop and rotation are excluded unless you choose them.")
            .small()
            .weak(),
    );
}

pub(super) fn progress(ui: &mut Ui, studio: &mut Studio) {
    let busy = studio.photo.is_batching();
    let Some(progress) = studio.photo.batch_progress() else {
        return;
    };
    ui.add_space(8.);
    let fraction = progress.completed as f32 / progress.total.max(1) as f32;
    ui.add(egui::ProgressBar::new(fraction).text(format!(
        "{} / {} photos",
        progress.completed, progress.total
    )));
    ui.add(egui::Label::new(RichText::new(&progress.current).small()).truncate());
    if !progress.errors.is_empty() {
        ui.collapsing(format!("{} files need attention", progress.failed), |ui| {
            ScrollArea::vertical().max_height(90.).show(ui, |ui| {
                for error in &progress.errors {
                    ui.label(error);
                }
            });
        });
    }
    if busy {
        if ui.button("Cancel folder operation").clicked() {
            studio.photo.cancel_folder_batch();
        }
        ui.ctx().request_repaint();
    } else {
        ui.label(
            RichText::new(if progress.completed == progress.failed {
                "No files were changed."
            } else if progress.cancelled {
                "Stopped. Completed changes remain in history."
            } else {
                "Folder operation finished."
            })
            .small()
            .weak(),
        );
    }
}

pub(super) fn dialogs(ctx: &Context, studio: &mut Studio) {
    if studio.file_dialog_pending() {
        return;
    }
    let mut state = read(ctx);
    let Some(dialog) = state.dialog.clone() else {
        return;
    };
    let mut close = false;
    let mut next = None;
    let owner = state.owner.get_or_insert_with(|| Owner::capture(ctx, studio)).clone();
    let current = owner.current(studio);
    let mut actions = Vec::new();
    let response = egui::Modal::new(Id::new("photo-workflow-dialog")).show(ctx, |ui| {
        super::super::fields::native_scope(ui, owner.id, |ui| {
        ui.set_width((ctx.viewport_rect().width() - 64.).clamp(280., 440.));
        match dialog {
            Dialog::Apply(mut snapshot) => {
                ui.heading("Apply adjustments");
                ui.add(egui::Label::new(format!("From {}", snapshot.name)).truncate());
                ui.add_space(8.);
                categories(ui, owner.id, &mut snapshot.categories);
                ui.separator();
                let count = studio.photo.selected_count();
                let selected = super::super::fields::native_scope(ui, owner.id.with("selected-photos"), |ui|
                    ui.radio_value(&mut state.folder, false, format!("Selected photos ({count})"))).inner;
                super::super::welcome::focus_control(ui, &selected);
                ui.add_enabled_ui(!studio.photo.folder_files.is_empty(), |ui| {
                    let folder = super::super::fields::native_scope(ui, owner.id.with("whole-folder"), |ui|
                        ui.radio_value(&mut state.folder, true, format!("Whole folder ({} photos)", studio.photo.folder_files.len()))).inner;
                    super::super::welcome::focus_control(ui, &folder);
                });
                if state.folder {
                    ui.add(egui::Label::new(&studio.photo.folder).truncate());
                    ui.label("Writes .omaphoto settings beside the originals. RAW files stay unchanged; files are processed one at a time. Cancel stops the remaining files; Undo restores completed writes.");
                } else {
                    ui.label("Updates the selected photos in this session. Save selected settings when you’re ready; the originals stay unchanged.");
                }
                ui.add_space(8.);
                ui.horizontal(|ui| {
                    close = action(ui, owner.id, "cancel", "Cancel", true).clicked();
                    let enabled = current && snapshot.categories.any() && !studio.photo.is_saving() && if state.folder { !studio.photo.folder_files.is_empty() } else { count > 0 };
                    let label = if state.folder { format!("Write settings for {} photos", studio.photo.folder_files.len()) } else { format!("Apply to {count} photos") };
                    let apply = action(ui, owner.id, "apply", &label, enabled);
                    actions.push(apply.clone());
                    if apply.clicked() {
                        let result = if state.folder { studio.photo.start_folder_batch((*snapshot).clone()) } else { studio.photo.apply_adjustments(&snapshot).map(|_| ()) };
                        match result { Ok(()) => close = true, Err(error) => { state.error = error; state.error_target = Some(apply.id); } }
                    }
                });
                state.dialog = Some(Dialog::Apply(snapshot));
            }
            Dialog::Presets => {
                ui.heading("Photo presets");
                ui.label("Save, reuse and share development settings.");
                if studio.photo.is_loading_presets() { ui.spinner(); ctx.request_repaint(); }
                text(ui, owner.id, "preset-filter", "Filter presets", &mut state.query);
                ScrollArea::vertical().id_salt("photo-presets").max_height(130.).min_scrolled_height(40.).show(ui, |ui| {
                    for preset in &studio.photo.presets {
                        if !preset.name.to_lowercase().contains(&state.query.to_lowercase()) { continue; }
                        let id = owner.id.with(("saved-preset", &preset.name));
                        let selected = super::super::fields::native_scope(ui, id, |ui| ui.selectable_label(state.preset.as_deref() == Some(preset.name.as_str()), &preset.name)).inner;
                        super::super::welcome::focus_control(ui, &selected);
                        if selected.clicked() { state.preset = Some(preset.name.clone()); state.categories = preset.categories; }
                    }
                    if studio.photo.presets.is_empty() { ui.label(RichText::new("Your saved looks will appear here.").weak()); } else if !studio.photo.presets.iter().any(|preset| preset.name.to_lowercase().contains(&state.query.to_lowercase())) { ui.label(RichText::new("No matching presets.").weak()); }
                });
                let selected_index = state.preset.as_ref().and_then(|name| studio.photo.presets.iter().position(|p| &p.name == name));
                let selected = selected_index.and_then(|i| studio.photo.presets.get(i)).cloned();
                let busy = !current || studio.photo.is_loading_presets() || studio.photo.is_saving_presets() || studio.photo.is_batching();
                ui.horizontal_wrapped(|ui| {
                    if action(ui, owner.id, "use", "Use preset…", selected.is_some() && !busy).clicked() { next = selected.clone().map(|preset| Dialog::Apply(Box::new(preset))); state.folder = false; }
                    if action(ui, owner.id, "import", "Import…", !busy).on_hover_text("Import .omapreset files").clicked() {
                        studio.request_file_dialog(
                            || rfd::FileDialog::new().add_filter("Photo presets", &["omapreset"]).pick_file(),
                            |_, studio, path| {
                                if let Err(error) = studio.photo.import_presets(path) { studio.photo.status = error; }
                            },
                        );
                    }
                    if action(ui, owner.id, "export", "Export…", selected.is_some() && !busy).clicked() {
                        let selected_index = selected_index.unwrap();
                        studio.request_file_dialog(
                            || crate::project::dialog_export("Photo preset", "omapreset"),
                            move |_, studio, path| {
                                if let Err(error) = studio.photo.export_preset(selected_index, path) { studio.photo.status = error; }
                            },
                        );
                    }
                    let remove = action(ui, owner.id, "remove", "Remove", selected.is_some() && !busy);
                    actions.push(remove.clone());
                    if remove.clicked() {
                        if let Err(error) = studio.photo.delete_preset(selected_index.unwrap()) { state.error = error; state.error_target = Some(remove.id); } else { state.preset = None; }
                    }
                });
                ui.separator();
                ui.label(RichText::new("Save the active photo’s look").strong());
                let name = text(ui, owner.id, "preset-name", "Preset name", &mut state.name);
                let name_valid = !state.name.trim().is_empty() && state.name.trim().chars().count() <= 120 && !state.name.trim().chars().any(char::is_control);
                if !name_valid {
                    super::super::welcome::control_error(ui, &[&name], "Give this preset a name between 1 and 120 characters.");
                }
                categories(ui, owner.id, &mut state.categories);
                ui.horizontal(|ui| {
                    let save = action(ui, owner.id, "save", "Save preset", current && !busy && studio.photo.selected().is_some() && name_valid && state.categories.any());
                    actions.push(save.clone());
                    if save.clicked() {
                        match studio.photo.save_preset(&state.name, state.categories) { Ok(()) => state.error.clear(), Err(error) => { state.error = error; state.error_target = Some(save.id); } }
                    }
                    close = action(ui, owner.id, "done", "Done", true).clicked();
                });
                if !studio.photo.preset_error().is_empty() {
                    let retry = action(ui, owner.id, "retry", "Retry loading presets", true);
                    super::super::welcome::control_error(ui, &[&retry], studio.photo.preset_error());
                    if retry.clicked() { studio.photo.reload_presets(); }
                }
            }
        }
        if !current { super::super::welcome::control_error(ui, &actions.iter().collect::<Vec<_>>(), "The document or photo selection changed. Close and reopen this dialog."); }
        if !state.error.is_empty() {
            let fields = actions.iter().filter(|response| Some(response.id) == state.error_target).collect::<Vec<_>>();
            super::super::welcome::control_error(ui, &fields, &state.error);
        }
        });
    });
    if let Some(dialog) = next {
        state.dialog = Some(dialog);
        state.error.clear();
        state.error_target = None;
        state.owner = Some(Owner::capture(ctx, studio));
    }
    if close || response.should_close() {
        state.dialog = None;
        state.owner = None;
        super::super::panel_workspace::return_focus(ctx, studio);
    }
    write(ctx, state);
}
