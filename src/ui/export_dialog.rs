use super::jobs;
mod preferences;
use crate::{
    app::Studio,
    export::{self, Format, Source},
    photo::PhotoImage,
    tools::Persona,
    upscale,
};
use eframe::egui::{self, Context, Id};
use export::target::Scope;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

const JOB: &str = "still-export";
const PREVIEW_JOB: &str = "still-export-preview";
fn id() -> Id {
    Id::new("still-export-dialog")
}
#[derive(Clone)]
struct Dialog {
    owner: String,
    persona: Persona,
    source: Arc<Mutex<Source>>,
    original: Arc<Mutex<Source>>,
    selection: Option<Arc<Mutex<Source>>>,
    scope: Scope,
    file_key: Option<String>,
    preview: Option<egui::TextureHandle>,
    preview_error: Option<String>,
    preset_name: String,
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

pub(crate) fn ready(ctx: &Context) -> bool {
    !jobs::is_running::<crate::photo::RgbaImage>(ctx, PREVIEW_JOB) && !busy(ctx)
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
    let file_key = match &source {
        Source::Document(_) => studio.path.as_ref(),
        Source::Photo(photo) => photo.source.as_ref(),
    }
    .map(|path| {
        path.canonicalize()
            .unwrap_or_else(|_| path.clone())
            .to_string_lossy()
            .into_owned()
    });
    let mut options = preferences::recall(ctx, file_key.as_deref(), &studio.swap_id);
    let selection = if studio.persona != Persona::Photo {
        let selected = if studio.persona == Persona::Pixel {
            studio
                .pixel_sel
                .as_ref()
                .zip(studio.pixel_sel_space)
                .map(|(coverage, space)| {
                    export::target::pixels(&studio.doc, coverage, space.w, space.h, space.transform)
                })
        } else {
            None
        };
        selected
            .unwrap_or_else(|| export::target::selection(&studio.doc, &studio.selection))
            .ok()
            .map(|doc| Arc::new(Mutex::new(Source::Document(doc))))
    } else {
        None
    };
    match &options.scope {
        Scope::Selection if selection.is_none() => options.scope = Scope::Document,
        Scope::Artboard(id) if !matches!(&source, Source::Document(doc) if doc.artboards.iter().any(|a| a.id == *id)) => {
            options.scope = Scope::Document
        }
        _ => {}
    }
    if matches!(&source, Source::Photo(_)) {
        if !options.format.raster() {
            options.format = format;
        }
    }
    if copy {
        options.format = format;
        options.ai = true;
        options.scale = 1;
    }
    let original = Arc::new(Mutex::new(source));
    let mut dialog = Dialog {
        owner: studio.swap_id.clone(),
        persona: studio.persona,
        source: Arc::clone(&original),
        original,
        selection,
        scope: options.scope.clone(),
        file_key,
        preview: None,
        preview_error: None,
        preset_name: String::new(),
        format: options.format,
        scale: options.scale,
        ai: options.ai,
        upscale: options.upscale,
        copy,
        busy: false,
        error: None,
    };
    dialog.update_source();
    dialog.start_preview(ctx);
    ctx.data_mut(|d| d.insert_temp(id(), dialog));
}
impl Dialog {
    fn options(&self) -> preferences::Options {
        preferences::Options {
            format: self.format,
            scale: self.scale,
            ai: self.ai,
            upscale: self.upscale,
            scope: self.scope.clone(),
        }
    }
    fn remember(&self, ctx: &Context) -> Result<(), String> {
        // Upscaled-copy settings should not replace ordinary export preferences.
        if self.copy {
            return Ok(());
        }
        preferences::remember(ctx, self.file_key.as_deref(), &self.owner, self.options())
    }
    fn update_source(&mut self) {
        self.source = match self.scope {
            Scope::Document => Arc::clone(&self.original),
            Scope::Selection => self
                .selection
                .as_ref()
                .cloned()
                .unwrap_or_else(|| Arc::clone(&self.original)),
            Scope::Artboard(id) => {
                if let Source::Document(doc) = &*self.original.lock().unwrap() {
                    match export::target::artboard(doc, id) {
                        Ok(doc) => Arc::new(Mutex::new(Source::Document(doc))),
                        Err(error) => {
                            self.error = Some(error);
                            Arc::clone(&self.original)
                        }
                    }
                } else {
                    Arc::clone(&self.original)
                }
            }
        };
    }
    fn start_preview(&mut self, ctx: &Context) {
        self.preview = None;
        self.preview_error = None;
        jobs::cancel::<crate::photo::RgbaImage>(ctx, PREVIEW_JOB);
        let source = self.source.lock().unwrap().clone();
        let format = self.format;
        jobs::start(ctx, PREVIEW_JOB, move || preview_image(&source, format));
    }
}
fn preview_image(source: &Source, format: Format) -> Result<crate::photo::RgbaImage, String> {
    let mut image = match source {
        Source::Photo(photo) => photo.render_thumbnail(420),
        Source::Document(doc) => {
            let pm = crate::compositor::render_export_preview(doc, 420)?;
            let pixels = crate::document::Pixels::from_pixmap(&pm);
            crate::photo::RgbaImage {
                w: pixels.w,
                h: pixels.h,
                data: pixels.data,
            }
        }
    };
    if format == Format::Jpeg {
        for p in image.data.chunks_exact_mut(4) {
            let alpha = p[3] as u32;
            for channel in &mut p[..3] {
                *channel = ((*channel as u32 * alpha + 127) / 255 + 255 - alpha) as u8;
            }
            p[3] = 255;
        }
    }
    Ok(image)
}
fn close(ctx: &Context) {
    jobs::cancel::<Output>(ctx, JOB);
    jobs::cancel::<crate::photo::RgbaImage>(ctx, PREVIEW_JOB);
    ctx.data_mut(|d| d.remove::<Dialog>(id()));
}

fn owner_matches(studio: &Studio, dialog: &Dialog) -> bool {
    studio.swap_id == dialog.owner
        && studio.persona == dialog.persona
        && match &*dialog.original.lock().unwrap() {
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
                if let Err(error) = dialog.remember(ctx) {
                    studio
                        .status
                        .push_str(&format!(" · Could not remember options: {error}"));
                }
                close(ctx);
                return;
            }
        }
    }
    if let Some(result) = jobs::poll::<crate::photo::RgbaImage>(ctx, PREVIEW_JOB) {
        match result {
            Ok(image) => {
                dialog.preview = Some(ctx.load_texture(
                    "export-preview",
                    egui::ColorImage::from_rgba_unmultiplied(
                        [image.w as usize, image.h as usize],
                        &image.data,
                    ),
                    egui::TextureOptions::LINEAR,
                ))
            }
            Err(error) => dialog.preview_error = Some(error),
        }
    }
    let previous_scope = dialog.scope.clone();
    let previous_format = dialog.format;
    let mut cancel = false;
    let mut export = false;
    let mut settings = export::Settings::default();
    let response = egui::Modal::new(Id::new("still-export-modal")).show(ctx, |ui| {
        ui.set_width((ctx.content_rect().width() - 64.).clamp(300., 510.));
        ui.heading(if dialog.copy { "AI upscale photo" } else { "Export" });
        egui::ScrollArea::vertical().max_height((ctx.content_rect().height()-180.).max(180.)).show(ui, |ui| {
        ui.add_enabled_ui(!dialog.busy, |ui| {
            if !dialog.copy {
                let boards = match &*dialog.original.lock().unwrap() {
                    Source::Document(doc) => doc.artboards.iter().map(|a| (a.id, a.name.clone())).collect::<Vec<_>>(),
                    Source::Photo(_) => vec![],
                };
                if matches!(&*dialog.original.lock().unwrap(), Source::Document(_)) {
                    let label = match dialog.scope {
                        Scope::Document => "Entire document".to_string(),
                        Scope::Selection => "Selection".to_string(),
                        Scope::Artboard(id) => boards.iter().find(|a| a.0 == id).map(|a| a.1.clone()).unwrap_or_default(),
                    };
                    egui::ComboBox::from_id_salt("export-scope").selected_text(label).show_ui(ui, |ui| {
                        ui.selectable_value(&mut dialog.scope, Scope::Document, "Entire document");
                        if dialog.selection.is_some() { ui.selectable_value(&mut dialog.scope, Scope::Selection, "Selection"); }
                        for (id, name) in boards { ui.selectable_value(&mut dialog.scope, Scope::Artboard(id), name); }
                    });
                }
                egui::ComboBox::from_id_salt("export-preset").selected_text("Choose preset…").show_ui(ui, |ui| {
                    for preset in preferences::presets(ctx) {
                        if matches!(&*dialog.original.lock().unwrap(), Source::Photo(_)) && !preset.options.format.raster() { continue; }
                        if ui.selectable_label(false, &preset.name).clicked() {
                            dialog.format = preset.options.format; dialog.scale = preset.options.scale;
                            dialog.ai = preset.options.ai; dialog.upscale = preset.options.upscale;
                            ui.close();
                        }
                    }
                });
            }
        });
        if previous_scope != dialog.scope { dialog.update_source(); }
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
        if previous_scope == dialog.scope && previous_format == dialog.format {
            if let Some(texture) = &dialog.preview {
                let available = egui::vec2(ui.available_width(), 210.);
                let size = texture.size_vec2();
                let size = size * (available.x / size.x).min(available.y / size.y).min(1.);
                let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                let cell = 12.;
                for y in 0..(size.y / cell).ceil() as usize {
                    for x in 0..(size.x / cell).ceil() as usize {
                        let r = egui::Rect::from_min_size(rect.min + egui::vec2(x as f32 * cell, y as f32 * cell), egui::vec2(cell,cell)).intersect(rect);
                        ui.painter().rect_filled(r, 0., egui::Color32::from_gray(if (x+y)%2 == 0 { 175 } else { 215 }));
                    }
                }
                ui.painter().image(texture.id(), rect, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.,1.)), egui::Color32::WHITE);
            } else if let Some(error) = &dialog.preview_error { ui.colored_label(egui::Color32::LIGHT_RED, format!("Preview: {error}")); }
            else { ui.horizontal(|ui| { ui.spinner(); ui.label("Rendering preview…"); }); }
        }
        if dialog.ai && dialog.format.raster() { ui.small("Composition preview · AI detail is generated during export."); }
        if !dialog.copy {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut dialog.preset_name).hint_text("Preset name").desired_width(180.));
                if ui.add_enabled(!dialog.busy && !dialog.preset_name.trim().is_empty(), egui::Button::new("Save preset")).clicked() {
                    match preferences::save_preset(ctx, &dialog.preset_name, dialog.options()) {
                        Ok(()) => { dialog.preset_name.clear(); dialog.error = None; }
                        Err(error) => dialog.error = Some(error),
                    }
                }
            });
            ui.small("Options are remembered for this file and session.");
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
    });
    if cancel || response.should_close() {
        if let Err(error) = dialog.remember(ctx) {
            studio.status = format!("Could not remember export options: {error}");
        }
        close(ctx);
        return;
    }
    if previous_scope != dialog.scope || previous_format != dialog.format {
        dialog.start_preview(ctx);
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
    fn export_preview_matches_native_still_and_jpeg_background() {
        let mut doc = crate::document::Document::new("Preview", 23., 17., 96.);
        doc.transparent = true;
        let source = Source::Document(doc);
        let image = preview_image(&source, Format::Png).unwrap();
        let (bytes, _) =
            export::encode(&source, export::Settings::default(), &crate::ml::NoProgress).unwrap();
        let output = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(image.data, output.into_raw());
        let jpeg = preview_image(&source, Format::Jpeg).unwrap();
        assert!(jpeg.data.iter().all(|v| *v == 255));
    }
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
