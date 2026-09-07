//! Photo transfer and preset dialogs; all filesystem work belongs to PhotoSession.
use crate::{
    app::Studio,
    photo::transfer::{AdjustmentSnapshot, Categories},
};
use eframe::egui::{self, Button, Context, Id, RichText, ScrollArea, Ui};

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
    write(ctx, state);
}
pub(super) fn presets(ctx: &Context, studio: &mut Studio) {
    studio.photo.ensure_presets_loaded();
    let mut state = read(ctx);
    state.dialog = Some(Dialog::Presets);
    state.error.clear();
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
fn categories(ui: &mut Ui, value: &mut Categories) {
    ui.columns(2, |c| {
        c[0].checkbox(&mut value.light, "Light / tone");
        c[0].checkbox(&mut value.curve, "Tone curve");
        c[0].checkbox(&mut value.color, "Color");
        c[0].checkbox(&mut value.detail, "Effects");
        c[1].checkbox(&mut value.hsl, "Color mixer");
        c[1].checkbox(&mut value.split_tone, "Color grading");
        c[1].checkbox(&mut value.crop, "Crop");
        c[1].checkbox(&mut value.rotation, "Rotation");
    });
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
    let mut state = read(ctx);
    let Some(dialog) = state.dialog.clone() else {
        return;
    };
    let mut close = false;
    let mut next = None;
    let response = egui::Modal::new(Id::new("photo-workflow-dialog")).show(ctx, |ui| {
        ui.set_width((ctx.viewport_rect().width() - 64.).clamp(280., 440.));
        match dialog {
            Dialog::Apply(mut snapshot) => {
                ui.heading("Apply adjustments");
                ui.add(egui::Label::new(format!("From {}", snapshot.name)).truncate());
                ui.add_space(8.);
                categories(ui, &mut snapshot.categories);
                ui.separator();
                let count = studio.photo.selected_count();
                ui.radio_value(&mut state.folder, false, format!("Selected photos ({count})"));
                ui.add_enabled_ui(!studio.photo.folder_files.is_empty(), |ui| {
                    ui.radio_value(&mut state.folder, true, format!("Whole folder ({} photos)", studio.photo.folder_files.len()));
                });
                if state.folder {
                    ui.add(egui::Label::new(&studio.photo.folder).truncate());
                    ui.label("Writes .omaphoto settings beside the originals. RAW files stay unchanged; files are processed one at a time. Cancel stops the remaining files; Undo restores completed writes.");
                } else {
                    ui.label("Updates the selected photos in this session. Save selected settings when you’re ready; the originals stay unchanged.");
                }
                ui.add_space(8.);
                ui.horizontal(|ui| {
                    close = ui.button("Cancel").clicked();
                    let enabled = snapshot.categories.any() && !studio.photo.is_saving() && if state.folder { !studio.photo.folder_files.is_empty() } else { count > 0 };
                    let label = if state.folder { format!("Write settings for {} photos", studio.photo.folder_files.len()) } else { format!("Apply to {count} photos") };
                    if ui.add_enabled(enabled, Button::new(label)).clicked() {
                        let result = if state.folder { studio.photo.start_folder_batch((*snapshot).clone()) } else { studio.photo.apply_adjustments(&snapshot).map(|_| ()) };
                        match result { Ok(()) => close = true, Err(error) => state.error = error }
                    }
                });
                state.dialog = Some(Dialog::Apply(snapshot));
            }
            Dialog::Presets => {
                ui.heading("Photo presets");
                ui.label("Save, reuse and share development settings.");
                if studio.photo.is_loading_presets() { ui.spinner(); ctx.request_repaint(); }
                ui.add(egui::TextEdit::singleline(&mut state.query).hint_text("Filter presets…").desired_width(f32::INFINITY));
                ScrollArea::vertical().id_salt("photo-presets").max_height(130.).min_scrolled_height(40.).show(ui, |ui| {
                    for preset in &studio.photo.presets {
                        if !preset.name.to_lowercase().contains(&state.query.to_lowercase()) { continue; }
                        if ui.selectable_label(state.preset.as_deref() == Some(preset.name.as_str()), &preset.name).clicked() { state.preset = Some(preset.name.clone()); state.categories = preset.categories; }
                    }
                    if studio.photo.presets.is_empty() { ui.label(RichText::new("Your saved looks will appear here.").weak()); } else if !studio.photo.presets.iter().any(|preset| preset.name.to_lowercase().contains(&state.query.to_lowercase())) { ui.label(RichText::new("No matching presets.").weak()); }
                });
                let selected_index = state.preset.as_ref().and_then(|name| studio.photo.presets.iter().position(|p| &p.name == name));
                let selected = selected_index.and_then(|i| studio.photo.presets.get(i)).cloned();
                let busy = studio.photo.is_loading_presets() || studio.photo.is_saving_presets() || studio.photo.is_batching();
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(selected.is_some() && !busy, Button::new("Use preset…")).clicked() { next = selected.clone().map(|preset| Dialog::Apply(Box::new(preset))); state.folder = false; }
                    if ui.add_enabled(!busy, Button::new("Import…")).on_hover_text("Import .omapreset files").clicked()
                        && let Some(path) = rfd::FileDialog::new().add_filter("Photo presets", &["omapreset"]).pick_file()
                        && let Err(error) = studio.photo.import_presets(path)
                    { state.error = error; }
                    if ui.add_enabled(selected.is_some() && !busy, Button::new("Export…")).clicked()
                        && let Some(path) = crate::project::dialog_export("Photo preset", "omapreset")
                        && let Err(error) = studio.photo.export_preset(selected_index.unwrap(), path)
                    { state.error = error; }
                    if ui.add_enabled(selected.is_some() && !busy, Button::new("Remove")).clicked() {
                        if let Err(error) = studio.photo.delete_preset(selected_index.unwrap()) { state.error = error; } else { state.preset = None; }
                    }
                });
                ui.separator();
                ui.label(RichText::new("Save the active photo’s look").strong());
                ui.add(egui::TextEdit::singleline(&mut state.name).hint_text("Preset name").desired_width(f32::INFINITY));
                categories(ui, &mut state.categories);
                ui.horizontal(|ui| {
                    if ui.add_enabled(!busy && studio.photo.selected().is_some() && !state.name.trim().is_empty() && state.categories.any(), Button::new("Save preset")).clicked() {
                        match studio.photo.save_preset(&state.name, state.categories) { Ok(()) => state.error.clear(), Err(error) => state.error = error }
                    }
                    close = ui.button("Done").clicked();
                });
                if !studio.photo.preset_error().is_empty() {
                    ui.label(studio.photo.preset_error());
                    if ui.button("Retry loading presets").clicked() { studio.photo.reload_presets(); }
                }
            }
        }
        if !state.error.is_empty() { ui.label(RichText::new(&state.error).color(crate::ui::theme::accent())); }
    });
    if let Some(dialog) = next {
        state.dialog = Some(dialog);
        state.error.clear();
    }
    if close || response.should_close() {
        state.dialog = None;
    }
    write(ctx, state);
}
