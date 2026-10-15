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
    cell::RefCell,
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
    generation: u64,
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
    validation_error: Option<String>,
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
        generation: studio.canvas_gen,
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
        validation_error: None,
    };
    dialog.update_source();
    dialog.reset_number_drafts(ctx);
    dialog.start_preview(ctx);
    ctx.data_mut(|d| d.insert_temp(id(), dialog));
}
impl Dialog {
    /// Identify this document's export settings.
    /// Args: current dialog. Returns an identity independent of preview state and canvas generation.
    fn settings_id(&self) -> Id {
        Id::new((
            "still-export-settings",
            &self.owner,
            &self.file_key,
            self.copy,
        ))
    }

    /// Reset rejected number drafts after opening or choosing a preset.
    /// Args: dialog and UI context. Returns no value; accepted export options are preserved.
    fn reset_number_drafts(&self, ctx: &Context) {
        for property in [
            self.settings_id().with("render-scale"),
            self.settings_id().with("upscale").with("factor"),
        ] {
            ctx.data_mut(|data| data.remove::<String>(property.with("number-validation")));
        }
    }

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
        && studio.canvas_gen == dialog.generation
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
    let owner = ui.make_persistent_id("upscale-settings");
    let (response, valid) = model_fields(ui, settings, owner);
    if !valid && response.enabled() {
        super::welcome::control_error(
            ui,
            &[&response],
            "Enter a finite AI scale from 1.01× to 32×.",
        );
    }
}

/// Draw the existing upscale controls under their settings owner.
/// Args: UI, settings and owner identity. Returns the native factor response and accepted draft state.
fn model_fields(
    ui: &mut egui::Ui,
    settings: &mut upscale::Settings,
    owner: Id,
) -> (egui::Response, bool) {
    ui.scope_builder(egui::UiBuilder::new().id(owner), |ui| {
        let property = owner.with("factor");
        let result = ui
            .horizontal(|ui| {
                ui.label("AI scale");
                let mut reset = false;
                for factor in [2., 4.] {
                    let response = ui
                        .scope_builder(
                            egui::UiBuilder::new().id(owner.with(("factor-preset", factor as u32))),
                            |ui| {
                                ui.selectable_value(
                                    &mut settings.factor,
                                    factor,
                                    format!("×{factor}"),
                                )
                            },
                        )
                        .inner;
                    ui.ctx().accesskit_node_builder(response.id, |node| {
                        node.set_label(format!("AI scale ×{factor}"))
                    });
                    super::welcome::focus_control(ui, &response);
                    reset |= response.enabled() && response.clicked();
                }
                let before = settings.factor;
                let attempt = RefCell::new(None);
                let step = vertical_step(ui);
                let (response, label) = ui
                    .scope_builder(egui::UiBuilder::new().id(property), |ui| {
                        let label = ui.label("Custom").id;
                        (
                            ui.add(
                                egui::DragValue::new(&mut settings.factor)
                                    .range(1.01..=32.)
                                    .speed(0.1)
                                    .suffix("×")
                                    .custom_parser(|text| {
                                        super::welcome::parse_number(&attempt, text)
                                    }),
                            ),
                            label,
                        )
                    })
                    .inner;
                super::welcome::name_control(ui, &response, label, "AI upscale factor (×)");
                number_focus(ui, &response, step);
                let valid = super::welcome::native_number_valid(
                    ui,
                    &response,
                    property,
                    attempt.into_inner(),
                    settings.factor,
                    1.01..=32.,
                );
                if !valid {
                    settings.factor = before;
                }
                if reset {
                    ui.data_mut(|data| data.remove::<String>(property.with("number-validation")));
                }
                (response, valid || reset)
            })
            .inner;
        model_choice(ui, &mut settings.model);
        result
    })
    .inner
}
pub(super) fn model_choice(ui: &mut egui::Ui, model: &mut upscale::models::Model) {
    let before = *model;
    let property = ui.make_persistent_id("upscale-model");
    ui.scope_builder(egui::UiBuilder::new().id(property), |ui| {
        ui.horizontal(|ui| {
            let label = ui.label("Upscale model").id;
            let mut chosen = false;
            let response = egui::ComboBox::from_id_salt("choice")
                .selected_text(model.label())
                .show_ui(ui, |ui| {
                    for value in upscale::models::Model::ALL {
                        let response = ui
                            .scope_builder(
                                egui::UiBuilder::new().id(property.with(value.filename())),
                                |ui| ui.selectable_value(model, value, value.label()),
                            )
                            .inner;
                        if response.clicked() {
                            chosen = true;
                            ui.close();
                        }
                    }
                })
                .response;
            if chosen {
                response.request_focus();
            }
            super::welcome::name_control(ui, &response, label, "Upscale model");
        });
    });
    if !ui.is_enabled() {
        *model = before;
    }
    ui.small("Real-ESRGAN · runs locally on your machine");
}

/// Observe an unmodified native spinner step before the widget consumes it.
/// Args: UI. Returns whether this frame contains an up or down key press.
fn vertical_step(ui: &egui::Ui) -> bool {
    ui.input(|input| {
        input.events.iter().any(|event| {
            matches!(event, egui::Event::Key {
        key: egui::Key::ArrowUp | egui::Key::ArrowDown, pressed: true, modifiers, ..
    } if !modifiers.any())
        })
    })
}

/// Keep spinner steps in the actual focused native field.
/// Args: UI, native response and observed step. Returns no value; Tab and Escape remain available.
fn number_focus(ui: &egui::Ui, response: &egui::Response, step: bool) {
    if response.enabled() && response.has_focus() {
        ui.memory_mut(|memory| {
            memory.set_focus_lock_filter(
                response.id,
                egui::EventFilter {
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    tab: false,
                    escape: false,
                },
            );
            if step {
                memory.move_focus(egui::FocusDirection::None);
            }
        });
    }
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
    let owner = dialog.settings_id();
    let available =
        !dialog.busy && !busy(ctx) && !studio.file_dialog_pending() && !studio.updates.freezing;
    let response = egui::Modal::new(Id::new("still-export-modal"))
        .area(egui::Modal::default_area(Id::new("still-export-modal")).default_size(egui::vec2(510., (ctx.content_rect().height()-96.).min(840.))))
        .show(ctx, |ui| {
        ui.set_width((ctx.content_rect().width() - 64.).clamp(300., 510.));
        ui.heading(if dialog.copy { "AI upscale photo" } else { "Export" });
        egui::ScrollArea::vertical().max_height((ctx.content_rect().height()-160.).clamp(120.,600.)).min_scrolled_height((ctx.content_rect().height()-160.).clamp(120.,600.)).show(ui, |ui| {
        ui.scope_builder(egui::UiBuilder::new().id(owner), |ui| {
        let mut fields = Vec::new();
        let mut invalid_fields = Vec::new();
        let mut validation_error = None;
        ui.add_enabled_ui(available, |ui| {
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
                    let property = owner.with("scope");
                    let response = ui.scope_builder(egui::UiBuilder::new().id(property), |ui| ui.horizontal(|ui| {
                        let label_id = ui.label("Export scope").id;
                        let mut chosen = false;
                        let response = egui::ComboBox::from_id_salt("choice").selected_text(label).show_ui(ui, |ui| {
                            let mut choices = vec![(property.with("document"), Scope::Document, "Entire document".to_owned())];
                            if dialog.selection.is_some() { choices.push((property.with("selection"), Scope::Selection, "Selection".to_owned())); }
                            choices.extend(boards.into_iter().map(|(id, name)| (property.with(("artboard", id)), Scope::Artboard(id), name)));
                            for (id, scope, name) in choices {
                                let response = ui.scope_builder(egui::UiBuilder::new().id(id), |ui| ui.selectable_label(dialog.scope == scope, name)).inner;
                                if available && response.clicked() { dialog.scope = scope; chosen = true; ui.close(); }
                            }
                        }).response;
                        if chosen { response.request_focus(); }
                        super::welcome::name_control(ui, &response, label_id, "Export scope");
                        response
                    }).inner).inner;
                    fields.push(response);
                }
                let property = owner.with("preset");
                let mut chosen = false;
                ui.scope_builder(egui::UiBuilder::new().id(property), |ui| ui.horizontal(|ui| {
                    let label = ui.label("Export preset").id;
                    let response = egui::ComboBox::from_id_salt("choice").selected_text("Choose preset…").show_ui(ui, |ui| {
                        for preset in preferences::presets(ctx) {
                            if matches!(&*dialog.original.lock().unwrap(), Source::Photo(_)) && !preset.options.format.raster() { continue; }
                            let response = ui.scope_builder(egui::UiBuilder::new().id(property.with(&preset.name)), |ui| ui.selectable_label(false, &preset.name)).inner;
                            if available && response.clicked() {
                                dialog.format = preset.options.format; dialog.scale = preset.options.scale;
                                dialog.ai = preset.options.ai; dialog.upscale = preset.options.upscale;
                                chosen = true; ui.close();
                            }
                        }
                    }).response;
                    if chosen { response.request_focus(); }
                    super::welcome::name_control(ui, &response, label, "Export preset");
                }));
                if chosen { dialog.reset_number_drafts(ctx); }
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
        ui.add_enabled_ui(available, |ui| {
            let property = owner.with("format");
            let response = ui.scope_builder(egui::UiBuilder::new().id(property), |ui| ui.horizontal(|ui| {
                let label = ui.label("Export format").id;
                let mut chosen = false;
                let response = egui::ComboBox::from_id_salt("choice").selected_text(dialog.format.label()).show_ui(ui, |ui| {
                    for format in Format::ALL {
                        if matches!(*dialog.source.lock().unwrap(), Source::Document(_)) || format.raster() {
                            let response = ui.scope_builder(egui::UiBuilder::new().id(property.with(format.extension())), |ui| ui.selectable_label(dialog.format == format, format.label())).inner;
                            if available && response.clicked() { dialog.format = format; chosen = true; ui.close(); }
                        }
                    }
                }).response;
                if chosen { response.request_focus(); }
                super::welcome::name_control(ui, &response, label, "Export format");
                response
            }).inner).inner;
            fields.push(response);
            if dialog.format.raster() {
                if !dialog.copy {
                    let before = dialog.scale;
                    let property = owner.with("render-scale");
                    let attempt = RefCell::new(None);
                    let step = vertical_step(ui);
                    let response = ui.scope_builder(egui::UiBuilder::new().id(property), |ui| ui.horizontal(|ui| {
                        let label = ui.label("Render scale").id;
                        let response = ui.add(egui::DragValue::new(&mut dialog.scale).range(1..=8).suffix("×")
                            .custom_parser(|text| super::welcome::parse_number(&attempt, text)));
                        super::welcome::name_control(ui, &response, label, "Export render scale (×)");
                        number_focus(ui, &response, step);
                        response
                    }).inner).inner;
                    if !super::welcome::native_number_valid(ui, &response, property, attempt.into_inner(), f64::from(dialog.scale), 1.0..=8.) {
                        dialog.scale = before;
                        if response.enabled() { invalid_fields.push(response.clone()); validation_error = Some("Enter a finite render scale from 1× to 8×.".to_owned()); }
                    }
                    fields.push(response);
                    let response = ui.scope_builder(egui::UiBuilder::new().id(owner.with("ai-enabled")), |ui| ui.checkbox(&mut dialog.ai, "AI upscale")).inner;
                    super::welcome::focus_control(ui, &response);
                }
                if dialog.ai {
                    let (response, valid) = model_fields(ui, &mut dialog.upscale, owner.with("upscale"));
                    if !valid && response.enabled() {
                        invalid_fields.push(response.clone());
                        validation_error = Some(if validation_error.is_some() {
                            "Enter finite scales: render from 1× to 8× and AI from 1.01× to 32×."
                        } else { "Enter a finite AI scale from 1.01× to 32×." }.to_owned());
                    }
                    fields.push(response);
                    ui.small("AI output uses 8-bit RGB with a separately resized alpha channel.");
                    if !dialog.upscale.model.present() {
                        ui.label(format!("Optional model · {:.1} MB", dialog.upscale.model.size() as f64 / 1_000_000.));
                        let response = ui.scope_builder(egui::UiBuilder::new().id(owner.with("download-model")), |ui| ui.add_enabled(available && validation_error.is_none(), egui::Button::new("Download model"))).inner;
                        super::welcome::focus_control(ui, &response);
                        if available && validation_error.is_none() && response.clicked() {
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
            Err(error) => {
                if validation_error.is_none() { invalid_fields.extend(fields.iter().cloned()); validation_error = Some(error.clone()); }
            }
        }
        dialog.validation_error = validation_error.clone();
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
            } else if dialog.preview_error.is_none() { ui.horizontal(|ui| { ui.spinner(); ui.label("Rendering preview…"); }); }
        }
        if dialog.ai && dialog.format.raster() { ui.small("Composition preview · AI detail is generated during export."); }
        let mut preset_error = None;
        if !dialog.copy {
            ui.horizontal(|ui| {
                let label = ui.label("Preset name").id;
                let before = dialog.preset_name.clone();
                let response = ui.add_enabled(available && !dialog.busy, egui::TextEdit::singleline(&mut dialog.preset_name)
                    .id(owner.with("preset-name")).hint_text("Preset name").desired_width(180.));
                if !response.enabled() { dialog.preset_name = before; }
                super::welcome::name_control(ui, &response, label, "Export preset name");
                let name_valid = dialog.preset_name.trim().chars().count() <= 80;
                if !name_valid {
                    preset_error = Some("Use a preset name between 1 and 80 characters".to_owned());
                    if validation_error.is_none() { invalid_fields.push(response); }
                }
                let can_save = available && !dialog.busy && validation_error.is_none() && name_valid && !dialog.preset_name.trim().is_empty();
                let response = ui.scope_builder(egui::UiBuilder::new().id(owner.with("save-preset")), |ui| ui.add_enabled(can_save, egui::Button::new("Save preset"))).inner;
                super::welcome::focus_control(ui, &response);
                if can_save && response.clicked() {
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
        if dialog.error.is_some() && dialog.ai && dialog.upscale.model != upscale::models::Model::General {
            let allowed = available && !dialog.busy && validation_error.is_none();
            let response = ui.scope_builder(egui::UiBuilder::new().id(owner.with("download-model-again")), |ui| ui.add_enabled(allowed, egui::Button::new("Download verified model again"))).inner;
            super::welcome::focus_control(ui, &response);
            if allowed && response.clicked() {
                let model = dialog.upscale.model;
                dialog.busy = true;
                dialog.error = None;
                jobs::start_with_progress(ctx, JOB, 1, move |progress| { model.download(progress.as_ref())?; Ok(Output::Downloaded) });
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            let response = ui.scope_builder(egui::UiBuilder::new().id(owner.with("cancel")), |ui| ui.button("Cancel")).inner;
            super::welcome::focus_control(ui, &response);
            cancel = response.clicked();
            let allowed = available && !dialog.busy && validation_error.is_none() && size.is_ok() && settings.ai.is_none_or(|s| s.model.present());
            let response = ui.scope_builder(egui::UiBuilder::new().id(owner.with("export")), |ui| ui.add_enabled(allowed,
                egui::Button::new(if dialog.copy { "Save upscaled copy…" } else { "Export…" }))).inner;
            super::welcome::focus_control(ui, &response);
            export = allowed && response.clicked();
        });
        if let Some(error) = validation_error.or(preset_error).or_else(|| dialog.error.clone()).or_else(|| dialog.preview_error.as_ref().map(|error| format!("Preview: {error}"))) {
            let affected: Vec<_> = invalid_fields.iter().collect();
            super::welcome::control_error(ui, &affected, &error);
        }
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
    if dialog.busy || busy(ctx) {
        return Err("Wait for the current export".into());
    }
    if studio.updates.freezing || studio.file_dialog_pending() {
        return Err("Finish the current file operation before exporting".into());
    }
    if !owner_matches(studio, &dialog) {
        return Err("The export source changed".into());
    }
    if let Some(error) = &dialog.validation_error {
        return Err(error.clone());
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
    use crate::ui::welcome::keyboard_tests as keys;
    use egui::{Event, Key, Modifiers, accesskit};

    fn keyboard_studio(ctx: &Context, copy: bool) -> Studio {
        ctx.enable_accesskit();
        ctx.data_mut(|data| {
            data.insert_temp(
                Id::new("export-options-memory"),
                preferences::Memory::default(),
            )
        });
        let mut studio = Studio::new();
        studio.show_welcome = false;
        studio.doc = crate::document::Document::new("Keyboard export", 32., 24., 96.);
        if copy {
            studio.persona = Persona::Photo;
            studio.photo.import_image(
                "Keyboard photo".into(),
                crate::photo::RgbaImage {
                    w: 4,
                    h: 3,
                    data: vec![255; 48],
                },
            );
        }
        assert!(studio.selection.is_empty());
        open(ctx, &mut studio, Format::Png, copy);
        jobs::cancel::<crate::photo::RgbaImage>(ctx, PREVIEW_JOB);
        studio
    }

    fn frame(ctx: &Context, studio: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
        let output = keys::render(ctx, events, |ui| show(ui.ctx(), studio));
        assert!(
            !studio.file_dialog_pending(),
            "keyboard field tests must never open an OS chooser"
        );
        assert!(
            !busy(ctx),
            "keyboard field tests must never start exports or model downloads"
        );
        output
    }

    fn dialog(ctx: &Context) -> Dialog {
        ctx.data(|data| data.get_temp::<Dialog>(id())).unwrap()
    }

    fn choose(ctx: &Context, studio: &mut Studio, control: &str, choice: &str) -> egui::FullOutput {
        keys::tab_to(ctx, control, |events| frame(ctx, studio, events));
        frame(ctx, studio, keys::key(Key::Space, Modifiers::NONE));
        keys::tab_to(ctx, choice, |events| frame(ctx, studio, events));
        frame(ctx, studio, keys::key(Key::Enter, Modifiers::NONE));
        jobs::cancel::<crate::photo::RgbaImage>(ctx, PREVIEW_JOB);
        let output = frame(ctx, studio, vec![]);
        keys::focused(ctx, &output, control);
        output
    }

    #[test]
    fn export_keyboard_shortcut_reaches_fields_and_cancel_without_pointer_setup() {
        let ctx = Context::default();
        ctx.enable_accesskit();
        crate::ui::theme::apply(&ctx);
        let mut studio = Studio::new();
        studio.doc = crate::document::Document::new("Keyboard export route", 16., 12., 72.);
        studio.show_welcome = false;
        assert!(studio.selection.is_empty());
        let render = |events, studio: &mut Studio| {
            keys::render(&ctx, events, |ui| {
                studio.handle_shortcuts(ui.ctx());
                crate::ui::chrome::top_bar(ui, studio);
                crate::ui::canvas::show(ui, studio);
                show(ui.ctx(), studio);
            })
        };
        render(vec![], &mut studio);
        render(keys::key(Key::F8, Modifiers::NONE), &mut studio);
        render(
            keys::key(Key::E, Modifiers::CTRL | Modifiers::COMMAND),
            &mut studio,
        );
        assert!(is_open(&ctx));
        jobs::cancel::<crate::photo::RgbaImage>(&ctx, PREVIEW_JOB);
        let output = keys::tab_to(&ctx, "Export render scale (×)", |events| {
            render(events, &mut studio)
        });
        keys::focused(&ctx, &output, "Export render scale (×)");
        keys::replace("2", |events| render(events, &mut studio));
        assert_eq!(dialog(&ctx).scale, 2);
        keys::tab_to(&ctx, "Cancel", |events| render(events, &mut studio));
        render(keys::key(Key::Enter, Modifiers::NONE), &mut studio);
        assert!(!is_open(&ctx));
        assert!(!studio.file_dialog_pending());
        assert!(!busy(&ctx));
    }

    #[test]
    fn export_keyboard_names_edits_and_conditional_controls_keep_native_identity() {
        let ctx = Context::default();
        let mut studio = keyboard_studio(&ctx, false);
        let initial = frame(&ctx, &mut studio, vec![]);
        let render = keys::named(&initial, "Export render scale (×)").0;
        let preset_name = keys::named(&initial, "Export preset name").0;
        for name in [
            "Export scope",
            "Export preset",
            "Export format",
            "Export render scale (×)",
            "Export preset name",
        ] {
            assert!(
                !keys::named(&initial, name).1.labelled_by().is_empty(),
                "{name}"
            );
        }
        let board = studio.doc.artboards[0].clone();
        choose(&ctx, &mut studio, "Export scope", &board.name);
        assert_eq!(dialog(&ctx).scope, Scope::Artboard(board.id));
        keys::tab_to(&ctx, "Export render scale (×)", |events| {
            frame(&ctx, &mut studio, events)
        });
        let output = keys::replace("3", |events| frame(&ctx, &mut studio, events));
        keys::focused(&ctx, &output, "Export render scale (×)");
        assert_eq!(dialog(&ctx).scale, 3);
        let output = keys::tab(false, |events| frame(&ctx, &mut studio, events));
        keys::focused(&ctx, &output, "AI upscale");
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        assert!(dialog(&ctx).ai);
        let output = keys::tab_to(&ctx, "AI upscale factor (×)", |events| {
            frame(&ctx, &mut studio, events)
        });
        keys::focused(&ctx, &output, "AI upscale factor (×)");
        let factor = keys::named(&output, "AI upscale factor (×)").0;
        keys::replace("2.5", |events| frame(&ctx, &mut studio, events));
        assert_eq!(dialog(&ctx).upscale.factor, 2.5);
        choose(
            &ctx,
            &mut studio,
            "Upscale model",
            upscale::models::Model::Native2.label(),
        );
        assert_eq!(dialog(&ctx).upscale.model, upscale::models::Model::Native2);
        choose(
            &ctx,
            &mut studio,
            "Upscale model",
            upscale::models::Model::General.label(),
        );
        let output = choose(&ctx, &mut studio, "Export format", Format::Svg.label());
        assert_eq!(dialog(&ctx).format, Format::Svg);
        assert_eq!(keys::named(&output, "Export preset name").0, preset_name);
        let output = choose(&ctx, &mut studio, "Export format", Format::Png.label());
        assert_eq!(keys::named(&output, "Export render scale (×)").0, render);
        assert_eq!(keys::named(&output, "AI upscale factor (×)").0, factor);
        keys::tab_to(&ctx, "Export render scale (×)", |events| {
            frame(&ctx, &mut studio, events)
        });
        let output = keys::tab(true, |events| frame(&ctx, &mut studio, events));
        keys::focused(&ctx, &output, "Export format");
        close(&ctx);
    }

    #[test]
    fn export_keyboard_invalid_drafts_and_preset_name_block_native_actions() {
        let ctx = Context::default();
        let mut studio = keyboard_studio(&ctx, false);
        keys::tab_to(&ctx, "Export render scale (×)", |events| {
            frame(&ctx, &mut studio, events)
        });
        let before = dialog(&ctx).scale;
        let output = keys::replace("NaN", |events| frame(&ctx, &mut studio, events));
        keys::associated_error(&output, "Export render scale (×)");
        assert_eq!(dialog(&ctx).scale, before);
        let export = keys::named(&output, "Export…");
        assert!(export.1.is_disabled());
        frame(
            &ctx,
            &mut studio,
            keys::action(export.0, accesskit::Action::Click, None),
        );
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("artifacts/keyboard-export-rejected.png");
        assert!(save_to(&ctx, &studio, path).is_err());
        assert!(!busy(&ctx));
        let output = keys::tab(false, |events| frame(&ctx, &mut studio, events));
        keys::associated_error(&output, "Export render scale (×)");
        keys::tab_to(&ctx, "Export render scale (×)", |events| {
            frame(&ctx, &mut studio, events)
        });
        for (text, expected) in [(" 3 ", 3), ("99", 8), ("−2", 1)] {
            let output = keys::replace(text, |events| frame(&ctx, &mut studio, events));
            assert_eq!(dialog(&ctx).scale, expected);
            assert_eq!(
                keys::named(&output, "Export render scale (×)").1.invalid(),
                None
            );
        }
        keys::tab_to(&ctx, "AI upscale", |events| {
            frame(&ctx, &mut studio, events)
        });
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        keys::tab_to(&ctx, "AI upscale factor (×)", |events| {
            frame(&ctx, &mut studio, events)
        });
        let before = dialog(&ctx).upscale.factor;
        let output = keys::replace("1e309", |events| frame(&ctx, &mut studio, events));
        keys::associated_error(&output, "AI upscale factor (×)");
        assert_eq!(dialog(&ctx).upscale.factor, before);
        let output = keys::tab(true, |events| frame(&ctx, &mut studio, events));
        keys::focused(&ctx, &output, "AI scale ×4");
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        assert_eq!(dialog(&ctx).upscale.factor, 4.);
        assert!(dialog(&ctx).validation_error.is_none());
        keys::tab_to(&ctx, "Export preset name", |events| {
            frame(&ctx, &mut studio, events)
        });
        let output = keys::replace(&"x".repeat(81), |events| frame(&ctx, &mut studio, events));
        keys::associated_error(&output, "Export preset name");
        let save = keys::named(&output, "Save preset");
        assert!(save.1.is_disabled());
        frame(
            &ctx,
            &mut studio,
            keys::action(save.0, accesskit::Action::Click, None),
        );
        keys::replace("Keyboard draft", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        let output = frame(&ctx, &mut studio, vec![]);
        assert_eq!(keys::named(&output, "Export preset name").1.invalid(), None);
        assert!(!keys::named(&output, "Save preset").1.is_disabled());
        choose(&ctx, &mut studio, "Export preset", "PNG · 2×");
        assert_eq!(dialog(&ctx).scale, 2);
        assert!(!dialog(&ctx).ai);
        assert!(dialog(&ctx).validation_error.is_none());
        close(&ctx);
    }

    #[test]
    fn export_keyboard_busy_and_stale_guards_preserve_settings_and_owner_identity() {
        let ctx = Context::default();
        let mut studio = keyboard_studio(&ctx, false);
        let initial = frame(&ctx, &mut studio, vec![]);
        let render = keys::named(&initial, "Export render scale (×)").0;
        keys::tab_to(&ctx, "AI upscale", |events| {
            frame(&ctx, &mut studio, events)
        });
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        let output = frame(&ctx, &mut studio, vec![]);
        let factor = keys::named(&output, "AI upscale factor (×)").0;
        let before = dialog(&ctx).options();
        ctx.data_mut(|data| {
            let mut dialog = data.get_temp::<Dialog>(id()).unwrap();
            dialog.busy = true;
            data.insert_temp(id(), dialog);
        });
        let output = frame(&ctx, &mut studio, vec![]);
        assert!(
            keys::named(&output, "Export render scale (×)")
                .1
                .is_disabled()
        );
        frame(
            &ctx,
            &mut studio,
            keys::action(
                render,
                accesskit::Action::SetValue,
                Some(accesskit::ActionData::NumericValue(6.)),
            ),
        );
        frame(
            &ctx,
            &mut studio,
            keys::action(
                factor,
                accesskit::Action::SetValue,
                Some(accesskit::ActionData::NumericValue(7.5)),
            ),
        );
        assert_eq!(dialog(&ctx).options(), before);
        let export = keys::named(&output, "Export…").0;
        frame(
            &ctx,
            &mut studio,
            keys::action(export, accesskit::Action::Click, None),
        );
        assert_eq!(dialog(&ctx).options(), before);
        ctx.data_mut(|data| {
            let mut dialog = data.get_temp::<Dialog>(id()).unwrap();
            dialog.busy = false;
            data.insert_temp(id(), dialog);
        });
        studio.updates.freezing = true;
        let output = frame(&ctx, &mut studio, vec![]);
        assert!(keys::named(&output, "Export…").1.is_disabled());
        frame(
            &ctx,
            &mut studio,
            keys::action(
                render,
                accesskit::Action::SetValue,
                Some(accesskit::ActionData::NumericValue(4.)),
            ),
        );
        assert_eq!(dialog(&ctx).options(), before);
        studio.updates.freezing = false;
        studio.canvas_gen += 1;
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("artifacts/keyboard-export-stale.png");
        assert!(save_to(&ctx, &studio, path).is_err());
        frame(&ctx, &mut studio, vec![]);
        assert!(!is_open(&ctx));
        open(&ctx, &mut studio, Format::Png, false);
        jobs::cancel::<crate::photo::RgbaImage>(&ctx, PREVIEW_JOB);
        let output = frame(&ctx, &mut studio, vec![]);
        assert_eq!(
            keys::named(&output, "Export render scale (×)").0,
            render,
            "canvas generation is a stale guard, not widget identity"
        );
        studio.swap_id.push_str("-new-document");
        frame(&ctx, &mut studio, vec![]);
        assert!(!is_open(&ctx));
        open(&ctx, &mut studio, Format::Png, false);
        jobs::cancel::<crate::photo::RgbaImage>(&ctx, PREVIEW_JOB);
        let output = frame(&ctx, &mut studio, vec![]);
        assert_ne!(keys::named(&output, "Export render scale (×)").0, render);
        close(&ctx);
    }

    #[test]
    fn export_keyboard_photo_copy_native_arrows_cancel_and_escape_preserve_source() {
        let ctx = Context::default();
        let mut studio = keyboard_studio(&ctx, true);
        let photo = studio.photo.selected().unwrap().clone();
        let output = keys::tab_to(&ctx, "AI upscale factor (×)", |events| {
            frame(&ctx, &mut studio, events)
        });
        keys::focused(&ctx, &output, "AI upscale factor (×)");
        keys::replace("3.25", |events| frame(&ctx, &mut studio, events));
        assert_eq!(dialog(&ctx).upscale.factor, 3.25);
        let output = frame(&ctx, &mut studio, keys::key(Key::ArrowUp, Modifiers::NONE));
        keys::focused(&ctx, &output, "AI upscale factor (×)");
        assert_eq!(
            dialog(&ctx).upscale.factor,
            3.4,
            "native 0.1 step rounds to its displayed precision"
        );
        keys::tab_to(&ctx, "Cancel", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        assert!(!is_open(&ctx));
        assert!(Arc::ptr_eq(
            &studio.photo.selected().unwrap().full,
            &photo.full
        ));
        assert_eq!(studio.photo.selected().unwrap().develop, photo.develop);
        open(&ctx, &mut studio, Format::Png, true);
        jobs::cancel::<crate::photo::RgbaImage>(&ctx, PREVIEW_JOB);
        keys::tab_to(&ctx, "AI upscale factor (×)", |events| {
            frame(&ctx, &mut studio, events)
        });
        let output = keys::replace("NaN", |events| frame(&ctx, &mut studio, events));
        keys::associated_error(&output, "AI upscale factor (×)");
        frame(&ctx, &mut studio, keys::key(Key::Escape, Modifiers::NONE));
        assert!(!is_open(&ctx));
        assert!(Arc::ptr_eq(
            &studio.photo.selected().unwrap().full,
            &photo.full
        ));
        assert_eq!(studio.photo.selected().unwrap().develop, photo.develop);
    }

    #[test]
    fn export_modal_keeps_preset_and_export_controls_visible_after_preview_arrives() {
        let mut studio = Studio::new();
        let ctx = Context::default();
        open(&ctx, &mut studio, Format::Png, false);
        jobs::cancel::<crate::photo::RgbaImage>(&ctx, PREVIEW_JOB);
        for frame in 0..5 {
            if frame == 2 {
                let texture = ctx.load_texture(
                    "qa-preview",
                    egui::ColorImage::filled([320, 210], egui::Color32::WHITE),
                    Default::default(),
                );
                ctx.data_mut(|d| {
                    let mut dialog = d.get_temp::<Dialog>(id()).unwrap();
                    dialog.preview = Some(texture);
                    d.insert_temp(id(), dialog);
                });
            }
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1600., 1000.),
                    )),
                    time: Some(frame as f64),
                    ..Default::default()
                },
                |ui| show(ui.ctx(), &mut studio),
            );
            if frame == 4 {
                let labels: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|s| {
                        if let egui::Shape::Text(t) = &s.shape {
                            let rect = t.galley.rect.translate(t.pos.to_vec2());
                            s.clip_rect
                                .contains(rect.center())
                                .then_some(t.galley.job.text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();
                for label in ["Preset name", "Save preset", "Cancel", "Export…"] {
                    assert!(labels.contains(&label), "{label}: {labels:?}");
                }
            }
            output.textures_delta.clear();
        }
        close(&ctx);
    }
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
