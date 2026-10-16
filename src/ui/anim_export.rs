use crate::anim_export::{
    AnimExport, AnimKind, AnimSettings, export_animation, ffmpeg_installed, frame_times, summary,
};
use crate::app::Studio;
use eframe::egui::{self, Id, Response, Ui};
use std::{cell::RefCell, path::PathBuf};

const JOB: &str = "anim-export";

#[derive(Clone)]
struct Session {
    owner: String,
    generation: u64,
    focus: bool,
    previous_focus: Option<Id>,
}

impl Session {
    /// Check the document that opened this export.
    /// Args: editor. Returns false after a document switch or artwork revision.
    fn current(&self, studio: &Studio) -> bool {
        self.owner == studio.swap_id && self.generation == studio.canvas_gen
    }
}

/// Identify the transient animation dialog owner.
/// Args: none. Returns a context key independent of artwork persistence.
fn session_id() -> Id {
    Id::new((JOB, "keyboard-owner"))
}

pub(crate) fn open(ctx: &egui::Context, studio: &mut Studio, kind: AnimKind) {
    if studio.file_dialog_pending()
        || studio.updates.freezing
        || super::jobs::is_running::<PathBuf>(ctx, JOB)
    {
        return;
    }
    let settings = AnimSettings::from_document(&studio.doc, studio.export_scale as f32, kind);
    studio.status = summary(kind, &settings);
    studio.anim_export = Some(AnimExport { kind, settings });
    let owner = Id::new((JOB, &studio.swap_id));
    let previous_focus = ctx.memory(|memory| memory.focused());
    ctx.data_mut(|data| {
        for property in ["scale", "fps", "start", "end"] {
            data.remove::<String>(owner.with(property).with("number-validation"));
        }
        data.insert_temp(
            session_id(),
            Session {
                owner: studio.swap_id.clone(),
                generation: studio.canvas_gen,
                focus: true,
                previous_focus,
            },
        );
    });
}

pub(crate) fn poll(ctx: &egui::Context, studio: &mut Studio) {
    if let Some(result) = super::jobs::poll::<PathBuf>(ctx, JOB) {
        studio.status = match result {
            Ok(path) => {
                crate::telemetry::count("feature.export");
                format!("exported {}", path.display())
            }
            Err(error) if error == "Export cancelled" => "Export cancelled".into(),
            Err(error) => {
                crate::telemetry::count("error.export");
                format!("export failed: {error}")
            }
        };
        return;
    }
    if super::jobs::is_running::<PathBuf>(ctx, JOB) {
        ctx.request_repaint_after(std::time::Duration::from_millis(80));
        let (done, total) = super::jobs::frame_progress::<PathBuf>(ctx, JOB).unwrap_or((0, 0));
        studio.status = progress_line(done, total);
    }
}

/// Edit one existing native numeric draft.
/// Args: UI, property owner, visible name, value, range, speed and unit.
/// Returns its actual response and whether its parsed draft is valid.
fn number(
    ui: &mut Ui,
    id: Id,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    speed: f64,
    unit: &str,
) -> (Response, bool) {
    let before = *value;
    let attempt = RefCell::new(None);
    let (response, label_id) = ui
        .scope_builder(egui::UiBuilder::new().id(id), |ui| {
            let label_id = ui.label(label).id;
            let response = ui.add(
                egui::DragValue::new(value)
                    .range(range.clone())
                    .speed(speed)
                    .max_decimals(2)
                    .suffix(unit)
                    .custom_parser(|text| super::welcome::parse_number(&attempt, text)),
            );
            (response, label_id)
        })
        .inner;
    super::welcome::name_control(ui, &response, label_id, label);
    let valid = super::welcome::native_number_valid(
        ui,
        &response,
        id,
        attempt.into_inner(),
        f64::from(*value),
        f64::from(*range.start())..=f64::from(*range.end()),
    );
    let cancelled = (response.has_focus() || response.lost_focus())
        && ui.input(|input| input.key_pressed(egui::Key::Escape));
    if !valid || cancelled {
        *value = before;
    }
    (response, valid)
}

/// Revalidate the captured export after the chooser returns.
/// Args: editor, opening owner and accepted settings. Returns whether that same export can start.
fn current(studio: &Studio, session: &Session, panel: AnimExport) -> bool {
    session.current(studio) && studio.anim_export == Some(panel) && !studio.updates.freezing
}

pub(crate) fn show(ui: &mut Ui, studio: &mut Studio) {
    let Some(mut panel) = studio.anim_export else {
        return;
    };
    let ctx = ui.ctx().clone();
    let Some(mut session) = ctx.data(|data| data.get_temp::<Session>(session_id())) else {
        studio.anim_export = None;
        return;
    };
    let exporting = super::jobs::is_running::<PathBuf>(&ctx, JOB);
    let duration = studio.doc.motion.duration.max(0.05);
    let doc_w = studio.doc.width;
    let doc_h = studio.doc.height;
    let mut export_clicked = false;
    let mut cancel_clicked = false;
    let allowed = session.current(studio)
        && !exporting
        && !studio.file_dialog_pending()
        && !studio.updates.freezing;
    let owner = Id::new((JOB, &session.owner));
    let mut valid = true;
    let modal = egui::Modal::new(owner).show(&ctx, |ui| {
        ui.set_min_width(320.0);
        ui.heading("Export animation");
        ui.push_id(owner, |ui| {
            ui.add_enabled_ui(allowed, |ui| {
                ui.horizontal(|ui| {
                    for kind in AnimKind::ALL {
                        let response = ui.selectable_label(panel.kind == kind, kind.label());
                        super::welcome::focus_control(ui, &response);
                        if response.clicked() && panel.kind != kind {
                            panel.kind = kind;
                            panel.settings.transparent = kind.allows_alpha();
                        }
                    }
                });
                ui.add_space(8.0);
                let mut fields = Vec::new();
                let mut invalid = Vec::new();
                let (response, accepted) = number(
                    ui,
                    owner.with("scale"),
                    "Animation scale (×)",
                    &mut panel.settings.scale,
                    0.25..=4.,
                    0.05,
                    "×",
                );
                if session.focus && allowed {
                    response.request_focus();
                }
                if !accepted && allowed {
                    invalid.push(response.clone());
                }
                fields.push(response);
                let w = (doc_w * panel.settings.scale).round().max(1.0) as u32;
                let h = (doc_h * panel.settings.scale).round().max(1.0) as u32;
                ui.label(format!("{w} × {h}"));
                let (response, accepted) = number(
                    ui,
                    owner.with("fps"),
                    "Animation frame rate (fps)",
                    &mut panel.settings.fps,
                    1.0..=60.,
                    1.,
                    " fps",
                );
                if !accepted && allowed {
                    invalid.push(response.clone());
                }
                fields.push(response);
                for (property, label, value) in [
                    ("start", "Animation start (s)", &mut panel.settings.start),
                    ("end", "Animation end (s)", &mut panel.settings.end),
                ] {
                    let (response, accepted) = number(
                        ui,
                        owner.with(property),
                        label,
                        value,
                        0.0..=duration,
                        0.05,
                        " s",
                    );
                    if !accepted && allowed {
                        invalid.push(response.clone());
                    }
                    fields.push(response);
                }
                let looped = ui.checkbox(&mut panel.settings.looped, "Loop");
                super::welcome::focus_control(ui, &looped);
                if panel.kind.allows_alpha() {
                    let alpha =
                        ui.checkbox(&mut panel.settings.transparent, "Transparent background");
                    super::welcome::focus_control(ui, &alpha);
                } else {
                    panel.settings.transparent = false;
                    ui.label("MP4 is flat on white.");
                }
                valid = invalid.is_empty()
                    && panel.settings.start <= panel.settings.end
                    && studio.doc.motion.duration.is_finite()
                    && duration.is_finite()
                    && doc_w.is_finite()
                    && doc_h.is_finite()
                    && doc_w > 0.
                    && doc_h > 0.;
                if !invalid.is_empty() {
                    super::welcome::control_error(
                        ui,
                        &invalid.iter().collect::<Vec<_>>(),
                        "Enter a finite number within the animation field's range.",
                    );
                } else if panel.settings.start > panel.settings.end {
                    super::welcome::control_error(
                        ui,
                        &fields[2..].iter().collect::<Vec<_>>(),
                        "Animation end must be at or after its start.",
                    );
                }
            });
            if !session.current(studio) {
                super::welcome::control_error(
                    ui,
                    &[],
                    "The document changed. Cancel and open Export animation again.",
                );
            } else if studio.file_dialog_pending() || studio.updates.freezing {
                ui.label("Finish the current operation before changing this export.");
            }
            ui.add_space(6.0);
            ui.label(summary(panel.kind, &panel.settings));
            if matches!(panel.kind, AnimKind::Mp4 | AnimKind::Webm) && !ffmpeg_installed() {
                ui.label("ffmpeg is not installed");
            }
            if exporting {
                let (done, total) =
                    super::jobs::frame_progress::<PathBuf>(&ctx, JOB).unwrap_or((0, 0));
                ui.label(progress_line(done, total));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let response = ui.add_enabled(allowed && valid, egui::Button::new("Export"));
                super::welcome::focus_control(ui, &response);
                if allowed && valid && response.clicked() {
                    export_clicked = true;
                }
                let response = ui.button("Cancel");
                super::welcome::focus_control(ui, &response);
                if response.clicked() {
                    cancel_clicked = true;
                }
            });
        });
    });
    session.focus = false;
    ctx.data_mut(|data| data.insert_temp(session_id(), session.clone()));
    if cancel_clicked || modal.should_close() {
        if exporting {
            super::jobs::cancel::<PathBuf>(&ctx, JOB);
            studio.status = "Export cancelled".into();
        }
        studio.anim_export = None;
        ctx.data_mut(|data| data.remove::<Session>(session_id()));
        if let Some(id) = session.previous_focus {
            ctx.memory_mut(|memory| memory.request_focus(id));
        }
        return;
    }
    let changed = studio.anim_export != Some(panel);
    studio.anim_export = Some(panel);
    if changed && !exporting {
        studio.status = summary(panel.kind, &panel.settings);
    }
    if !export_clicked || !allowed || !valid {
        return;
    }
    let kind = panel.kind;
    let settings = panel.settings;
    studio.request_file_dialog(
        move || crate::project::dialog_export(kind.label(), kind.extension()),
        move |ctx, studio, path| {
            if !current(studio, &session, panel) {
                studio.status =
                    "The document or export settings changed. Open Export animation again.".into();
                return;
            }
            if super::jobs::is_running::<PathBuf>(ctx, JOB) {
                studio.status = "An animation export is already running.".into();
                return;
            }
            let doc = studio.doc.clone();
            let total = frame_times(&settings).len() as u32;
            studio.status = "Exporting animation…".into();
            super::jobs::start_with_progress(ctx, JOB, total, move |progress| {
                let bytes = export_animation(&doc, kind, &settings, |done, _| {
                    progress.report(done);
                    !progress.cancelled()
                })?;
                crate::formats::write_atomic(&path, &bytes)?;
                Ok(path)
            });
        },
    );
}

fn progress_line(done: u32, total: u32) -> String {
    if total == 0 {
        "Exporting animation…".into()
    } else if done >= total {
        "Encoding animation…".into()
    } else {
        format!("Exporting frame {} of {total}", done.saturating_add(1))
    }
}

#[cfg(test)]
mod keyboard_tests {
    use super::*;
    use crate::ui::welcome::keyboard_tests as keys;
    use egui::{Event, Key, Modifiers};

    fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
        keys::render(ctx, events, |ui| {
            super::super::chrome::top_bar(ui, studio);
            show(ui, studio);
        })
    }

    fn editor() -> (egui::Context, Studio) {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::ui::theme::apply(&ctx);
        let mut studio = Studio::new();
        studio.doc = crate::document::Document::new("Keyboard animation settings", 32., 24., 72.);
        studio.show_welcome = false;
        (ctx, studio)
    }

    fn launch(ctx: &egui::Context, studio: &mut Studio) {
        keys::tab_to(ctx, "File", |events| frame(ctx, studio, events));
        frame(ctx, studio, keys::key(Key::Enter, Modifiers::NONE));
        frame(ctx, studio, vec![]);
        keys::tab_to(ctx, "Export APNG…", |events| frame(ctx, studio, events));
        frame(ctx, studio, keys::key(Key::Enter, Modifiers::NONE));
        frame(ctx, studio, vec![]);
        assert_eq!(studio.anim_export.unwrap().kind, AnimKind::Apng);
    }

    #[test]
    fn keyboard_animation_menu_fields_invalid_and_busy_edits_keep_native_owners() {
        let (ctx, mut studio) = editor();
        assert!(studio.selection.is_empty());
        let history = studio.history.len();
        launch(&ctx, &mut studio);
        let mut fps_id = None;
        for (name, text) in [
            ("Animation scale (×)", "1.5"),
            ("Animation frame rate (fps)", "24"),
            ("Animation start (s)", "0.25"),
            ("Animation end (s)", "1.5"),
        ] {
            let output = keys::tab_to(&ctx, name, |events| frame(&ctx, &mut studio, events));
            keys::focused(&ctx, &output, name);
            assert!(!keys::named(&output, name).1.labelled_by().is_empty());
            if name == "Animation frame rate (fps)" {
                fps_id = Some(keys::named(&output, name).0);
            }
            keys::replace(text, |events| frame(&ctx, &mut studio, events));
            frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        }
        let settings = studio.anim_export.unwrap().settings;
        assert_eq!(
            (settings.scale, settings.fps, settings.start, settings.end),
            (1.5, 24., 0.25, 1.5)
        );
        let output = keys::tab_to(&ctx, "Animation frame rate (fps)", |events| {
            frame(&ctx, &mut studio, events)
        });
        assert_eq!(
            keys::named(&output, "Animation frame rate (fps)").0,
            fps_id.unwrap()
        );
        let output = keys::replace("not a rate", |events| frame(&ctx, &mut studio, events));
        keys::associated_error(&output, "Animation frame rate (fps)");
        assert!(keys::named(&output, "Export").1.is_disabled());
        assert_eq!(studio.anim_export.unwrap().settings.fps, 24.);
        keys::replace("30", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        studio.updates.freezing = true;
        let before = studio.anim_export.unwrap();
        let output = frame(&ctx, &mut studio, vec![]);
        assert!(
            keys::named(&output, "Animation frame rate (fps)")
                .1
                .is_disabled()
        );
        assert!(keys::named(&output, "Export").1.is_disabled());
        frame(&ctx, &mut studio, vec![Event::Text("60".into())]);
        assert_eq!(studio.anim_export.unwrap(), before);
        studio.updates.freezing = false;
        keys::tab_to(&ctx, "Animation end (s)", |events| {
            frame(&ctx, &mut studio, events)
        });
        let output = keys::tab(true, |events| frame(&ctx, &mut studio, events));
        keys::focused(&ctx, &output, "Animation start (s)");
        keys::tab_to(&ctx, "Cancel", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        assert!(studio.anim_export.is_none());
        assert!(!studio.file_dialog_pending());
        assert!(!super::super::jobs::is_running::<PathBuf>(&ctx, JOB));
        assert_eq!(studio.history.len(), history);
        assert!(!studio.dirty);
    }

    #[test]
    fn keyboard_animation_stale_revision_and_document_reject_the_captured_export() {
        let (ctx, mut studio) = editor();
        launch(&ctx, &mut studio);
        let session = ctx
            .data(|data| data.get_temp::<Session>(session_id()))
            .unwrap();
        let panel = studio.anim_export.unwrap();
        let output = keys::tab_to(&ctx, "Animation scale (×)", |events| {
            frame(&ctx, &mut studio, events)
        });
        let scale = keys::named(&output, "Animation scale (×)").0;
        assert!(current(&studio, &session, panel));
        studio.canvas_gen += 1;
        let output = frame(&ctx, &mut studio, vec![]);
        assert_eq!(keys::named(&output, "Animation scale (×)").0, scale);
        assert!(keys::named(&output, "Animation scale (×)").1.is_disabled());
        assert!(keys::named(&output, "Export").1.is_disabled());
        assert!(!current(&studio, &session, panel));
        keys::tab_to(&ctx, "Cancel", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        launch(&ctx, &mut studio);
        let output = frame(&ctx, &mut studio, vec![]);
        assert_eq!(keys::named(&output, "Animation scale (×)").0, scale);
        studio.swap_id.push_str("-other-document");
        let output = frame(&ctx, &mut studio, vec![]);
        assert!(keys::named(&output, "Export").1.is_disabled());
        assert!(!current(&studio, &session, panel));
        keys::tab_to(&ctx, "Cancel", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        launch(&ctx, &mut studio);
        let output = frame(&ctx, &mut studio, vec![]);
        assert_ne!(keys::named(&output, "Animation scale (×)").0, scale);
        assert!(!studio.file_dialog_pending());
    }
}
