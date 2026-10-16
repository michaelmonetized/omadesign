//! Brush selection, portable imports, and user presets.
use crate::brushes::ImportJob;
use crate::{
    app::Studio,
    brushes::{Pack, Preset},
    tools::Tool,
};
use eframe::egui::{self, TextureOptions};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

impl Studio {
    /// Import a brush library without blocking painting or touching its source.
    /// Takes a source path, installs a validated pack asynchronously, and reports progress in the status bar.
    pub fn import_brush_pack(&mut self, path: &Path) {
        if self.brush_import_job.is_some() {
            self.status = "A brush import is already running".into();
            return;
        }
        let path = path.to_owned();
        let job: ImportJob = Arc::new(Mutex::new(None));
        self.brush_import_job = Some(job.clone());
        self.status = format!("Importing brushes from {}…", path.display());
        let directory = self.brushes.directory.clone();
        std::thread::spawn(move || {
            let result = crate::brushes::import(&path).and_then(|pack| {
                let hash = crate::brushes::Library::persist(&directory, &pack)?;
                Ok((pack, hash))
            });
            if let Ok(mut state) = job.lock() {
                *state = Some(result);
            }
        });
    }

    pub fn select_brush(&mut self, pack: usize, brush: usize) {
        let Some(preset) = self
            .brushes
            .packs
            .get(pack)
            .and_then(|p| p.brushes.get(brush))
            .cloned()
        else {
            return;
        };
        self.end_pixel_stroke(false);
        preset.apply(&mut self.brush);
        self.brushes.selected = Some((pack, brush));
        self.status = preset.name;
    }
}

pub(super) fn poll(ctx: &egui::Context, studio: &mut Studio) {
    let result = studio
        .brush_import_job
        .as_ref()
        .and_then(|job| job.lock().ok()?.take());
    if let Some(result) = result {
        studio.brush_import_job = None;
        match result.map(|(pack, hash)| studio.brushes.accept(pack, hash)) {
            Ok(index) => {
                studio.select_brush(index, 0);
                let pack = &studio.brushes.packs[index];
                studio.status = format!(
                    "Imported {} · {} brushes{}",
                    pack.name,
                    pack.brushes.len(),
                    if pack.notes.is_empty() {
                        String::new()
                    } else {
                        format!(" · {}", pack.notes.join(" "))
                    }
                );
            }
            Err(error) => studio.status = format!("Brush import failed: {error}"),
        }
    }
    if studio.brush_import_job.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(60));
    }
}

pub(super) fn inspector(ui: &mut egui::Ui, studio: &mut Studio) {
    let name = studio
        .brushes
        .selected
        .and_then(|(p, b)| studio.brushes.packs.get(p)?.brushes.get(b))
        .map_or("Round brush", |b| b.name.as_str())
        .to_owned();
    let mut select = None;
    let mut round=false;
    let owner=egui::Id::new(("brush-preset-picker",&studio.swap_id,&studio.brushes.directory));
    let label=ui.label("Brush preset");
    let response=super::fields::native_scope(ui,owner,|ui|egui::ComboBox::from_id_salt("brush-pack-picker")
        .selected_text(name)
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            let round_response = super::fields::native_scope(ui,owner.with("round"),|ui|ui.selectable_label(studio.brushes.selected.is_none(), "Round brush")).inner;
            if super::agent::named(ui,round_response,"Round brush").clicked()
            {
                studio.end_pixel_stroke(false);
                studio.brushes.selected = None;
                round=true;
                ui.close();
            }
            for (p, pack) in studio.brushes.packs.iter().enumerate() {
                ui.label(egui::RichText::new(&pack.name).strong());
                for (b, brush) in pack.brushes.iter().enumerate() {
                    let response=super::fields::native_scope(ui,owner.with(("preset",p,b,Arc::as_ptr(&brush.tip))),|ui|ui.selectable_label(studio.brushes.selected == Some((p, b)), &brush.name)).inner;
                    let response=super::agent::named(ui,response,&format!("Select {} brush {}",pack.name,brush.name));
                    if response
                        .clicked()
                    {
                        select = Some((p, b));
                        ui.close();
                    }
                }
            }
        }).response).inner;
    super::welcome::name_control(ui,&response,label.id,"Brush preset");
    if let Some((p, b)) = select {
        studio.select_brush(p, b);
    }
    if select.is_some()||round { response.request_focus(); }
    ui.horizontal(|ui| {
        if ui.button("Import…").clicked() {
            studio.request_file_dialog(
                || {
                    rfd::FileDialog::new()
                        .add_filter(
                            "Brush libraries and tips",
                            &[
                                "omabrush", "abr", "ai", "svg", "eps", "png", "jpg", "jpeg",
                                "webp", "tif", "tiff",
                            ],
                        )
                        .pick_file()
                },
                |_, s, path| s.import_brush_pack(&path),
            );
        }
        if ui.button("Brush library…").clicked() {
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::new("brush-library-open"), true));
        }
    });
}

pub(super) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| {
        d.get_temp::<bool>(egui::Id::new("brush-library-open"))
            .unwrap_or(false)
    })
}

pub(super) fn window(ctx: &egui::Context, studio: &mut Studio) {
    let id = egui::Id::new("brush-library-open");
    let mut open = ctx.data(|d| d.get_temp::<bool>(id).unwrap_or(false));
    if !open {
        return;
    }
    let mut select = None;
    egui::Window::new("Brush library")
        .open(&mut open)
        .default_size([620.0, 520.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Import brushes or artwork…").clicked() {
                    studio.request_file_dialog(
                        || {
                            rfd::FileDialog::new()
                                .add_filter(
                                    "Brush libraries and tips",
                                    &[
                                        "omabrush", "abr", "ai", "svg", "eps", "png", "jpg",
                                        "jpeg", "webp", "tif", "tiff",
                                    ],
                                )
                                .pick_file()
                        },
                        |_, s, path| s.import_brush_pack(&path),
                    );
                }
                ui.hyperlink_to("Download packs ↗", "https://omadesign.app/brushes");
            });
            ui.small(
                "Photoshop sampled ABR · Illustrator artwork · PNG/SVG tips · Omadesign packs",
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        studio.brushes.selected.is_some(),
                        egui::Button::new("Save current settings as a brush"),
                    )
                    .clicked()
                {
                    if let Some((p, b)) = studio.brushes.selected {
                        let source = studio.brushes.packs[p].brushes[b].clone();
                        let source_pack = &studio.brushes.packs[p];
                        let mut pack = Pack::new("My brushes");
                        pack.author = source_pack.author.clone();
                        pack.license = source_pack.license.clone();
                        pack.notes = source_pack.notes.clone();
                        pack.brushes.push(Preset {
                            name: format!("{} · custom", source.name),
                            size: studio.brush.size,
                            spacing: studio.brush.spacing,
                            hardness: studio.brush.hardness,
                            flow: studio.brush.flow,
                            opacity: studio.brush.opacity,
                            tip: source.tip,
                            angle: studio.brush.angle,
                            scatter: studio.brush.scatter,
                            angle_jitter: studio.brush.angle_jitter,
                            size_jitter: studio.brush.size_jitter,
                            opacity_jitter: studio.brush.opacity_jitter,
                            follow_direction: studio.brush.follow_direction,
                        });
                        match studio.brushes.install(pack) {
                            Ok(p) => studio.select_brush(p, 0),
                            Err(e) => studio.status = e,
                        }
                    }
                }
                if ui
                    .add_enabled(
                        studio.brushes.selected.is_some(),
                        egui::Button::new("Export selected pack…"),
                    )
                    .clicked()
                {
                    if let Some((p, _)) = studio.brushes.selected {
                        let pack = studio.brushes.packs[p].clone();
                        studio.request_file_dialog(
                            || {
                                rfd::FileDialog::new()
                                    .add_filter("Omadesign brushes", &["omabrush"])
                                    .set_file_name("brushes.omabrush")
                                    .save_file()
                            },
                            move |_, s, path| {
                                s.status = match pack
                                    .bytes()
                                    .and_then(|bytes| crate::formats::write_atomic(&path, &bytes))
                                {
                                    Ok(()) => format!("Exported {}", path.display()),
                                    Err(e) => format!("Brush export failed: {e}"),
                                };
                            },
                        );
                    }
                }
            });
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (p, pack) in studio.brushes.packs.iter().enumerate() {
                    ui.heading(&pack.name);
                    ui.small(format!("{} · {}", pack.author, pack.license));
                    for note in &pack.notes {
                        ui.small(note);
                    }
                    for (b, preset) in pack.brushes.iter().enumerate() {
                        ui.allocate_ui(egui::vec2(ui.available_width(), 48.0), |ui| {
                            if !ui.is_rect_visible(ui.max_rect()) {
                                return;
                            }
                            ui.horizontal(|ui| {
                                let key = egui::Id::new(("brush-preview", p, b));
                                let cached = ctx.data(|d| d.get_temp::<egui::TextureHandle>(key));
                                let texture = cached.unwrap_or_else(|| {
                                    let mut pm = tiny_skia::Pixmap::new(160, 48).unwrap();
                                    let mut brush = crate::paint::Brush::default();
                                    preset.apply(&mut brush);
                                    brush.size = 30.0;
                                    brush.color = crate::color::Rgba::BLACK;
                                    crate::brushes::stroke(
                                        &mut pm,
                                        crate::geom::Pt::new(20.0, 24.0),
                                        crate::geom::Pt::new(140.0, 24.0),
                                        &brush,
                                        &preset.tip,
                                        false,
                                    );
                                    let texture = ctx.load_texture(
                                        format!("brush-{p}-{b}"),
                                        egui::ColorImage::from_rgba_premultiplied(
                                            [160, 48],
                                            pm.data(),
                                        ),
                                        TextureOptions::LINEAR,
                                    );
                                    ctx.data_mut(|d| d.insert_temp(key, texture.clone()));
                                    texture
                                });
                                ui.add(egui::Image::new(&texture).bg_fill(egui::Color32::WHITE));
                                let owner=egui::Id::new(("brush-library-preset",&studio.swap_id,&studio.brushes.directory,p,b,Arc::as_ptr(&preset.tip)));
                                let response=super::fields::native_scope(ui,owner,|ui|ui.selectable_label(
                                        studio.brushes.selected == Some((p, b)),
                                        &preset.name,
                                    )).inner;
                                let response=super::agent::named(ui,response,&format!("Select {} brush {}",pack.name,preset.name));
                                if response
                                    .clicked()
                                {
                                    select = Some((p, b));
                                }
                            });
                        });
                    }
                    ui.separator();
                }
                for error in &studio.brushes.errors {
                    let response=ui.colored_label(egui::Color32::LIGHT_RED, error);
                    ctx.accesskit_node_builder(response.id,|node|{node.set_live(egui::accesskit::Live::Polite);node.set_live_atomic();});
                }
            });
        });
    ctx.data_mut(|d| d.insert_temp(id, open));
    if let Some((p, b)) = select {
        studio.select_brush(p, b);
        studio.switch_persona(crate::tools::Persona::Pixel);
        studio.tool = Tool::Brush;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keyboard_pixel_fields_brush_popup_selects_existing_preset_and_keeps_native_owner() {
        use crate::ui::welcome::keyboard_tests as keys;
        use egui::{Event, Key, Modifiers};
        fn frame(
            ctx: &egui::Context,
            studio: &mut Studio,
            preceding: bool,
            events: Vec<Event>,
        ) -> egui::FullOutput {
            keys::render(ctx, events, |ui| {
                ui.set_max_width(340.);
                if preceding {
                    let _ = ui.button("Conditional brush status");
                }
                inspector(ui, studio);
            })
        }
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::ui::theme::apply(&ctx);
        let mut studio = Studio::new();
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".artifacts")
            .join(format!(
                "keyboard-brush-read-only-{}",
                crate::document::next_id()
            ));
        studio.brushes = crate::brushes::Library::load(directory);
        studio.brushes.packs.truncate(1);
        studio.brushes.packs[0].brushes.truncate(2);
        let pack = &studio.brushes.packs[0];
        let preset = pack.brushes[0].clone();
        let purpose = format!("Select {} brush {}", pack.name, preset.name);
        let before = crate::project::encode(&studio.doc).unwrap();
        let history = studio.history.len();
        let output = keys::tab_to(&ctx, "Brush preset", |events| {
            frame(&ctx, &mut studio, false, events)
        });
        let target = keys::named(&output, "Brush preset").0;
        frame(
            &ctx,
            &mut studio,
            false,
            keys::key(Key::Enter, Modifiers::NONE),
        );
        keys::tab_to(&ctx, &purpose, |events| {
            frame(&ctx, &mut studio, false, events)
        });
        frame(
            &ctx,
            &mut studio,
            false,
            keys::key(Key::Enter, Modifiers::NONE),
        );
        let output = frame(&ctx, &mut studio, true, vec![]);
        assert_eq!(studio.brushes.selected, Some((0, 0)));
        assert_eq!(studio.brush.size, preset.size);
        assert_eq!(studio.brush.spacing, preset.spacing);
        assert_eq!(studio.brush.opacity, preset.opacity);
        assert_eq!(keys::named(&output, "Brush preset").0, target);
        keys::focused(&ctx, &output, "Brush preset");
        studio.brush.size += 1.;
        let output = frame(&ctx, &mut studio, true, vec![]);
        assert_eq!(keys::named(&output, "Brush preset").0, target);
        frame(
            &ctx,
            &mut studio,
            true,
            keys::key(Key::Enter, Modifiers::NONE),
        );
        keys::tab_to(&ctx, "Round brush", |events| {
            frame(&ctx, &mut studio, true, events)
        });
        frame(
            &ctx,
            &mut studio,
            true,
            keys::key(Key::Enter, Modifiers::NONE),
        );
        assert!(studio.brushes.selected.is_none());
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
        assert_eq!(studio.history.len(), history);
        assert!(studio.selection.is_empty());
    }
    #[test]
    fn library_previews_render_and_reuse_textures_across_frames() {
        let ctx = egui::Context::default();
        let mut studio = Studio::new();
        crate::ui::show_brush_library(&ctx);
        let input = |pass: usize| egui::RawInput {
            time: Some(pass as f64 * 0.1),
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 900.0),
            )),
            ..Default::default()
        };
        let mut first = None;
        for pass in 0..6 {
            let mut output = ctx.run_ui(input(pass), |_| window(&ctx, &mut studio));
            output.textures_delta.clear();
            let texture = ctx.data(|d| {
                d.get_temp::<egui::TextureHandle>(egui::Id::new(("brush-preview", 0usize, 0usize)))
            });
            let Some(texture) = texture else {
                continue;
            };
            if let Some(id) = first {
                assert_eq!(id, texture.id());
            }
            first = Some(texture.id());
        }
        assert!(first.is_some(), "visible brush previews were drawn");
    }
}

/// Draw real brush presets in a dock, using the existing brush renderer and selection route.
/// Args: pane UI and editor. Returns nothing; imports retain the same native picker and worker.
pub(super) fn pane(ui:&mut egui::Ui,studio:&mut Studio){
    let mut selected=None;
    for (pack_index,pack) in studio.brushes.packs.iter().enumerate(){
        for (brush_index,preset) in pack.brushes.iter().enumerate(){
            let key=egui::Id::new(("dock-brush",pack_index,brush_index,Arc::as_ptr(&preset.tip),super::theme::fg().to_array()));
            let texture=ui.data(|d|d.get_temp::<egui::TextureHandle>(key)).unwrap_or_else(||{
                let mut pixels=tiny_skia::Pixmap::new(120,32).unwrap();
                let mut brush=crate::paint::Brush::default();preset.apply(&mut brush);brush.size=20.0;
                brush.color=crate::color::Rgba::from_array(super::theme::fg().to_array());
                crate::brushes::stroke(&mut pixels,crate::geom::Pt::new(12.0,16.0),crate::geom::Pt::new(108.0,16.0),&brush,&preset.tip,false);
                let image=egui::ColorImage::from_rgba_premultiplied([120,32],pixels.data());
                let texture=ui.ctx().load_texture(format!("brush-{key:?}"),image,TextureOptions::LINEAR);
                ui.data_mut(|d|d.insert_temp(key,texture.clone()));texture
            });
            ui.horizontal(|ui|{
                let response=ui.add(egui::Button::image(egui::Image::new(&texture).fit_to_exact_size(egui::vec2(96.0,26.0)))
                    .selected(studio.brushes.selected==Some((pack_index,brush_index)))).on_hover_text(&preset.name);
                if response.clicked(){selected=Some((pack_index,brush_index));}
                ui.add(egui::Label::new(&preset.name).truncate());
            });
        }
    }
    if let Some((pack,brush))=selected {studio.select_brush(pack,brush);studio.set_tool(Tool::Brush);}
    if super::icons::tiny_icon(ui,super::icons::ph::PLUS,"Import brushes",false){
        studio.request_file_dialog(||rfd::FileDialog::new().add_filter("Brushes",&["omabrush","abr","svg","png"]).pick_file(),|_,studio,path|studio.import_brush_pack(&path));
    }
}
