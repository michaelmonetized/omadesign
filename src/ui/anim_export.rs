use crate::anim_export::{
    AnimExport, AnimKind, AnimSettings, export_animation, ffmpeg_installed, frame_times, summary,
};
use crate::app::Studio;
use eframe::egui::{self, Ui};
use std::path::PathBuf;

const JOB: &str = "anim-export";

pub(crate) fn open(studio: &mut Studio, kind: AnimKind) {
    let settings = AnimSettings::from_document(&studio.doc, studio.export_scale as f32, kind);
    studio.status = summary(kind, &settings);
    studio.anim_export = Some(AnimExport { kind, settings });
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

pub(crate) fn show(ui: &mut Ui, studio: &mut Studio) {
    let Some(mut panel) = studio.anim_export else {
        return;
    };
    let ctx = ui.ctx().clone();
    let exporting = super::jobs::is_running::<PathBuf>(&ctx, JOB);
    let duration = studio.doc.motion.duration.max(0.05);
    let doc_w = studio.doc.width;
    let doc_h = studio.doc.height;
    let mut open = true;
    let mut export_clicked = false;
    let mut cancel_clicked = false;
    egui::Window::new("Export animation")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(&ctx, |ui| {
            ui.set_min_width(320.0);
            ui.horizontal(|ui| {
                for kind in AnimKind::ALL {
                    if ui
                        .selectable_label(panel.kind == kind, kind.label())
                        .clicked()
                        && panel.kind != kind
                    {
                        panel.kind = kind;
                        panel.settings.transparent = kind.allows_alpha();
                    }
                }
            });
            ui.add_space(8.0);
            ui.add_enabled_ui(!exporting, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Scale");
                    ui.add(
                        egui::DragValue::new(&mut panel.settings.scale)
                            .range(0.25..=4.0)
                            .speed(0.05)
                            .max_decimals(2)
                            .suffix("×"),
                    );
                    let w = (doc_w * panel.settings.scale).round().max(1.0) as u32;
                    let h = (doc_h * panel.settings.scale).round().max(1.0) as u32;
                    ui.label(format!("{w} × {h}"));
                });
                ui.horizontal(|ui| {
                    ui.label("Frame rate");
                    ui.add(
                        egui::DragValue::new(&mut panel.settings.fps)
                            .range(1.0..=60.0)
                            .speed(1.0)
                            .max_decimals(2),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Start");
                    ui.add(
                        egui::DragValue::new(&mut panel.settings.start)
                            .range(0.0..=duration)
                            .speed(0.05)
                            .max_decimals(2)
                            .suffix("s"),
                    );
                    ui.label("End");
                    ui.add(
                        egui::DragValue::new(&mut panel.settings.end)
                            .range(0.0..=duration)
                            .speed(0.05)
                            .max_decimals(2)
                            .suffix("s"),
                    );
                });
                ui.checkbox(&mut panel.settings.looped, "Loop");
                if panel.kind.allows_alpha() {
                    ui.checkbox(&mut panel.settings.transparent, "Transparent background");
                } else {
                    panel.settings.transparent = false;
                    ui.label("MP4 is flat on white.");
                }
            });
            if panel.settings.end < panel.settings.start {
                panel.settings.end = panel.settings.start;
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
                if ui
                    .add_enabled(!exporting, egui::Button::new("Export"))
                    .clicked()
                {
                    export_clicked = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel_clicked = true;
                }
            });
        });
    if cancel_clicked {
        if exporting {
            super::jobs::cancel::<PathBuf>(&ctx, JOB);
            studio.status = "Export cancelled".into();
        }
        studio.anim_export = None;
        return;
    }
    if !open {
        studio.anim_export = None;
        return;
    }
    let changed = studio.anim_export != Some(panel);
    studio.anim_export = Some(panel);
    if changed && !exporting {
        studio.status = summary(panel.kind, &panel.settings);
    }
    if !export_clicked {
        return;
    }
    let kind = panel.kind;
    let settings = panel.settings;
    studio.request_file_dialog(
        move || crate::project::dialog_export(kind.label(), kind.extension()),
        move |ctx, studio, path| {
            if studio.anim_export.is_none() {
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
