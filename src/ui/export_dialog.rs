use super::jobs;
use crate::{
    app::Studio,
    export::{self, Format, Source},
    photo::PhotoImage,
    tools::Persona,
    upscale,
};
use eframe::egui::{self, Context, Id};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

const JOB: &str = "still-export";
fn id() -> Id {
    Id::new("still-export-dialog")
}
#[derive(Clone)]
struct Dialog {
    owner: String,
    persona: Persona,
    source: Arc<Mutex<Source>>,
    format: Format,
    scale: u32,
    ai: bool,
    upscale: upscale::Settings,
    copy: bool,
    busy: bool,
    error: Option<String>,
}
enum Output {
    Downloaded,
    Saved(PathBuf, Vec<String>, Option<PhotoImage>),
}
pub(crate) fn is_open(ctx: &Context) -> bool {
    ctx.data(|d| d.get_temp::<Dialog>(id()).is_some())
}
pub(crate) fn busy(ctx: &Context) -> bool {
    jobs::is_running::<Output>(ctx, JOB)
}

pub(crate) fn open(ctx: &Context, studio: &mut Studio, format: Format, copy: bool) {
    if is_open(ctx) {
        return;
    }
    studio.end_deform(false);
    studio.end_pixel_stroke(false);
    studio.commit_type_edit();
    let source = if studio.persona == Persona::Photo {
        let Some(photo) = studio.photo.selected().cloned() else {
            return;
        };
        Source::Photo(photo)
    } else {
        Source::Document(studio.doc.clone())
    };
    ctx.data_mut(|d| {
        d.insert_temp(
            id(),
            Dialog {
                owner: studio.swap_id.clone(),
                persona: studio.persona,
                source: Arc::new(Mutex::new(source)),
                format,
                scale: if copy || studio.persona == Persona::Photo {
                    1
                } else {
                    studio.export_scale.clamp(1, 8)
                },
                ai: copy,
                upscale: Default::default(),
                copy,
                busy: false,
                error: None,
            },
        )
    });
}
fn close(ctx: &Context) {
    jobs::cancel::<Output>(ctx, JOB);
    ctx.data_mut(|d| d.remove::<Dialog>(id()));
}
fn owner_matches(studio: &Studio, dialog: &Dialog) -> bool {
    studio.swap_id == dialog.owner
        && studio.persona == dialog.persona
        && match &*dialog.source.lock().unwrap() {
            Source::Photo(photo) => studio
                .photo
                .selected()
                .is_some_and(|p| Arc::ptr_eq(&p.full, &photo.full) && p.develop == photo.develop),
            Source::Document(_) => true,
        }
}
pub(super) fn model_controls(ui: &mut egui::Ui, settings: &mut upscale::Settings) {
    ui.horizontal(|ui| {
        ui.label("AI scale");
        ui.selectable_value(&mut settings.factor, 2., "×2");
        ui.selectable_value(&mut settings.factor, 4., "×4");
        ui.label("Custom");
        ui.add(
            egui::DragValue::new(&mut settings.factor)
                .range(1.01..=32.)
                .speed(0.1)
                .suffix("×"),
        );
    });
    model_choice(ui, &mut settings.model);
}
pub(super) fn model_choice(ui: &mut egui::Ui, model: &mut upscale::models::Model) {
    egui::ComboBox::from_id_salt("upscale-model")
        .selected_text(model.label())
        .show_ui(ui, |ui| {
            for value in upscale::models::Model::ALL {
                ui.selectable_value(model, value, value.label());
            }
        });
    ui.small("Real-ESRGAN · runs locally on your machine");
}
pub(super) fn show(ctx: &Context, studio: &mut Studio) {
    let Some(mut dialog) = ctx.data(|d| d.get_temp::<Dialog>(id())) else {
        return;
    };
    if !owner_matches(studio, &dialog) {
        close(ctx);
        studio.status = "Export cancelled because its source changed".into();
        return;
    }
    if let Some(result) = jobs::poll::<Output>(ctx, JOB) {
        dialog.busy = false;
        match result {
            Err(error) => dialog.error = Some(error),
            Ok(Output::Downloaded) => dialog.error = None,
            Ok(Output::Saved(path, notes, photo)) => {
                let status = format!(
                    "Exported {}{}",
                    path.display(),
                    if notes.is_empty() {
                        String::new()
                    } else {
                        format!(" · {}", notes.join("; "))
                    }
                );
                if let Some(photo) = photo {
                    studio.photo.import_photo(photo);
                }
                studio.status = status.clone();
                studio.photo.status = status;
                crate::telemetry::count("feature.export");
                close(ctx);
                return;
            }
        }
    }
    let mut cancel = false;
    let mut export = false;
    let mut settings = export::Settings::default();
    let response = egui::Modal::new(Id::new("still-export-modal")).show(ctx, |ui| {
        ui.set_width((ctx.content_rect().width() - 64.).clamp(300., 510.));
        ui.heading(if dialog.copy { "AI upscale photo" } else { "Export" });
        let (w,h) = dialog.source.lock().unwrap().dimensions();
        ui.label(format!("{} × {} px · {}", w, h, if matches!(*dialog.source.lock().unwrap(), Source::Photo(_)) { "developed photo" } else { "document" }));
        if let Source::Document(doc)=&*dialog.source.lock().unwrap() {
            let overflow=doc.layers.iter().filter_map(|l|l.kind.shapes()).flatten().filter(|s|matches!(&s.geom,crate::geom::Geom::Text(t) if t.layout.as_ref().is_some_and(|l|l.overflow))).count();
            if overflow>0 {ui.colored_label(egui::Color32::from_rgb(240,145,70),format!("{overflow} text frame(s) overflow. Hidden text remains editable in the project."));}
        }
        ui.separator();
        ui.add_enabled_ui(!dialog.busy, |ui| {
            egui::ComboBox::from_id_salt("still-export-format").selected_text(dialog.format.label()).show_ui(ui, |ui| {
                for format in Format::ALL {
                    if matches!(*dialog.source.lock().unwrap(), Source::Document(_)) || format.raster() {
                        ui.selectable_value(&mut dialog.format, format, format.label());
                    }
                }
            });
            if dialog.format.raster() {
                if !dialog.copy {
                    ui.horizontal(|ui| {
                        ui.label("Render scale");
                        ui.add(egui::DragValue::new(&mut dialog.scale).range(1..=8).suffix("×"));
                    });
                    ui.checkbox(&mut dialog.ai, "AI upscale");
                }
                if dialog.ai {
                    model_controls(ui, &mut dialog.upscale);
                    ui.small("AI output uses 8-bit RGB with a separately resized alpha channel.");
                    if !dialog.upscale.model.present() {
                        ui.label(format!("Optional model · {:.1} MB", dialog.upscale.model.size() as f64 / 1_000_000.));
                        if ui.button("Download model").clicked() {
                            let model = dialog.upscale.model;
                            dialog.busy = true;
                            dialog.error = None;
                            jobs::start_with_progress(ctx, JOB, 1, move |progress| { model.download(progress.as_ref())?; Ok(Output::Downloaded) });
                        }
                    }
                } else if matches!(&*dialog.source.lock().unwrap(), Source::Photo(p) if p.raw.is_some()) {
                    ui.small("RAW PNG and TIFF retain 16-bit precision.");
                }
            } else { ui.small("Vector and layered formats keep their native structure."); }
        });
        settings = export::Settings { format: dialog.format, scale: dialog.scale,
            ai: (dialog.format.raster() && dialog.ai).then_some(dialog.upscale) };
        let size = export::dimensions(&dialog.source.lock().unwrap(), settings).and_then(|size| {
            if dialog.copy && size.0 as u64 * size.1 as u64 > 64_000_000 {
                Err("Photo copies support up to 64 megapixels. Use Export for larger output.".into())
            } else { Ok(size) }
        });
        match &size {
            Ok((w,h)) => { ui.strong(format!("Output: {w} × {h} px")); }
            Err(error) => { ui.colored_label(egui::Color32::LIGHT_RED, error); }
        }
        if dialog.copy { ui.small("Save and open an upscaled copy. Your original photo and adjustments stay intact."); }
        if dialog.busy {
            let (done, total) = jobs::frame_progress::<Output>(ctx, JOB).unwrap_or((0,1));
            ui.add(egui::ProgressBar::new(done as f32 / total.max(1) as f32).animate(true)
                .text(format!("{} · {done} / {total}", jobs::stage::<Output>(ctx, JOB))));
        }
        if let Some(error) = &dialog.error {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
            if dialog.ai && dialog.upscale.model != upscale::models::Model::General && !dialog.busy
                && ui.button("Download verified model again").clicked() {
                let model = dialog.upscale.model;
                dialog.busy = true;
                dialog.error = None;
                jobs::start_with_progress(ctx, JOB, 1, move |progress| { model.download(progress.as_ref())?; Ok(Output::Downloaded) });
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            cancel = ui.button("Cancel").clicked();
            export = ui.add_enabled(!dialog.busy && size.is_ok() && settings.ai.is_none_or(|s| s.model.present()),
                egui::Button::new(if dialog.copy { "Save upscaled copy…" } else { "Export…" })).clicked();
        });
    });
    if cancel || response.should_close() {
        close(ctx);
        return;
    }
    if export {
        dialog.error = None;
        studio.export_scale = dialog.scale;
        studio.request_file_dialog(
            move || {
                crate::project::dialog_export(settings.format.label(), settings.format.extension())
            },
            move |ctx, studio, path| {
                if let Err(error) = save_to(ctx, studio, path) {
                    ctx.data_mut(|d| {
                        if let Some(mut dialog) = d.get_temp::<Dialog>(id()) {
                            dialog.error = Some(error);
                            d.insert_temp(id(), dialog);
                        }
                    });
                }
            },
        );
    }
    if dialog.busy {
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }
    ctx.data_mut(|d| d.insert_temp(id(), dialog));
}

/// Complete the native host's destination choice using the current preview.
pub(super) fn save_to(ctx: &Context, studio: &Studio, mut path: PathBuf) -> Result<(), String> {
    let mut dialog = ctx
        .data(|d| d.get_temp::<Dialog>(id()))
        .ok_or("Open Export first")?;
    if dialog.busy {
        return Err("Wait for the current export".into());
    }
    if !owner_matches(studio, &dialog) {
        return Err("The export source changed".into());
    }
    let settings = export::Settings {
        format: dialog.format,
        scale: dialog.scale,
        ai: (dialog.format.raster() && dialog.ai).then_some(dialog.upscale),
    };
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let compatible = extension == settings.format.extension()
        || settings.format == Format::Jpeg && extension == "jpeg"
        || settings.format == Format::Tiff && extension == "tiff";
    if extension.is_empty() {
        path.set_extension(settings.format.extension());
        if path.exists() {
            return Err("That file already exists. Select its full filename in the save dialog to replace it.".into());
        }
    } else if !compatible {
        return Err(format!(
            "Choose a .{} filename for this format",
            settings.format.extension()
        ));
    }
    let source = dialog.source.lock().unwrap().clone();
    let size = export::dimensions(&source, settings)?;
    if dialog.copy && size.0 as u64 * size.1 as u64 > 64_000_000 {
        return Err(
            "Photo copies support up to 64 megapixels. Use Export for larger output.".into(),
        );
    }
    let copy = dialog.copy;
    dialog.busy = true;
    dialog.error = None;
    ctx.data_mut(|d| d.insert_temp(id(), dialog));
    jobs::start_with_progress(ctx, JOB, 1, move |progress| {
        let notes = export::save(&source, settings, &path, progress.as_ref())?;
        let photo = if copy {
            Some(PhotoImage::load(&path)?)
        } else {
            None
        };
        Ok(Output::Saved(path, notes, photo))
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn photo_source_changes_cancel_owned_dialog() {
        let mut studio = Studio::new();
        studio.persona = Persona::Photo;
        studio.photo.import_image(
            "one".into(),
            crate::photo::RgbaImage {
                w: 2,
                h: 2,
                data: vec![255; 16],
            },
        );
        let ctx = Context::default();
        open(&ctx, &mut studio, Format::Png, true);
        let dialog = ctx.data(|d| d.get_temp::<Dialog>(id())).unwrap();
        assert!(owner_matches(&studio, &dialog));
        studio.photo.selected_mut().unwrap().develop.exposure = 1.;
        assert!(!owner_matches(&studio, &dialog));
        show(&ctx, &mut studio);
        assert!(!is_open(&ctx));
    }
}
