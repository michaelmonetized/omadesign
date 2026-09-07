use crate::app::Studio;
#[path = "photo_workflow.rs"]
mod workflow;
use crate::photo::{self, DevelopParams, HSL_NAMES, Histogram};
use crate::ui::theme::{accent, accent_soft, bg_canvas, bg_extreme, bg_panel, fg_weak};
use eframe::egui::{
    Align, Button, Color32, ColorImage, DragValue, Frame, Layout, Margin, PointerButton, Pos2,
    Rect, RichText, ScrollArea, Sense, Slider, Stroke, TextureOptions, Ui, pos2, vec2,
};

pub(crate) fn poll_jobs(ctx: &eframe::egui::Context, studio: &mut Studio) {
    if let Some(result) = super::jobs::poll::<std::path::PathBuf>(ctx, "photo-export") {
        studio.photo.status = match result {
            Ok(path) => format!(
                "Exported {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ),
            Err(error) => {
                studio.photo.report_write_error(error.clone());
                format!("Export failed: {error}")
            }
        };
    }
}

pub(crate) fn is_exporting(ctx: &eframe::egui::Context) -> bool {
    super::jobs::is_running::<std::path::PathBuf>(ctx, "photo-export")
}

pub(crate) fn copy_adjustments(studio: &mut Studio) {
    studio.photo.copy_adjustments();
}

pub(crate) fn paste_adjustments(ctx: &eframe::egui::Context, studio: &mut Studio) {
    workflow::paste(ctx, studio);
}

pub(crate) fn preset_library(ctx: &eframe::egui::Context, studio: &mut Studio) {
    workflow::presets(ctx, studio);
}

pub fn show(ui: &mut Ui, studio: &mut Studio) {
    poll_jobs(ui.ctx(), studio);
    let before = studio.photo.selected().map(|image| {
        (
            studio.photo.selected,
            image.develop.clone(),
            studio.photo.edit_revision,
        )
    });
    if studio.photo.dirty || studio.photo.built_version != studio.photo.sel_version {
        studio.photo.rebuild();
    }
    upload_textures(ui, studio);

    eframe::egui::Panel::left("filmstrip")
        .resizable(true)
        .default_size(200.0)
        .size_range(176.0..=300.0)
        .frame(Frame::new().fill(bg_panel()).inner_margin(Margin::same(10)))
        .show(ui, |ui| {
            filmstrip(ui, studio);
        });
    eframe::egui::Panel::right("develop")
        .resizable(true)
        .default_size(288.0)
        .size_range(256.0..=420.0)
        .frame(Frame::new().fill(bg_panel()).inner_margin(Margin::same(14)))
        .show(ui, |ui| {
            super::library::tabs(ui, studio);
            if studio.libraries.sidebar == crate::app::libraries::Sidebar::Inspector {
                ui.add_enabled_ui(!studio.photo.is_batching(), |ui| develop_panel(ui, studio));
            } else {
                super::library::show(ui, studio);
            }
        });
    if let Some((index, params, revision)) = before
        && index == studio.photo.selected
        && revision == studio.photo.edit_revision
    {
        studio
            .photo
            .record_edit(params, ui.input(|i| i.pointer.primary_down()));
    }
    ui.add_enabled_ui(!studio.photo.is_batching(), |ui| viewer(ui, studio));
    if !ui.input(|i| i.pointer.primary_down()) {
        studio.photo.finish_edit();
    }
    workflow::dialogs(ui.ctx(), studio);
}

fn upload_textures(ui: &mut Ui, studio: &mut Studio) {
    // Thumbnails: only upload new ones when images are added
    if studio.photo.thumbs.len() != studio.photo.images.len() {
        studio.photo.thumbs.clear();
        for (i, img) in studio.photo.images.iter().enumerate() {
            let tex = ui.ctx().load_texture(
                format!("thumb-{i}-{}", img.name),
                ColorImage::from_rgba_unmultiplied(
                    [img.thumb.w as usize, img.thumb.h as usize],
                    &img.thumb.data,
                ),
                TextureOptions::LINEAR,
            );
            studio.photo.thumbs.push(tex);
        }
    }
    // Adjusted image: only re-upload when sel_version changes
    if studio.photo.built_version != studio.photo.sel_version {
        if let Some(adj) = &studio.photo.adjusted {
            let tex = if let Some(mut existing) = studio.photo.tex.take()
                && existing.size() == [adj.w as usize, adj.h as usize]
            {
                existing.set(
                    ColorImage::from_rgba_unmultiplied([adj.w as usize, adj.h as usize], &adj.data),
                    TextureOptions::LINEAR,
                );
                existing
            } else {
                ui.ctx().load_texture(
                    format!("dev-{}", studio.photo.sel_version),
                    ColorImage::from_rgba_unmultiplied([adj.w as usize, adj.h as usize], &adj.data),
                    TextureOptions::LINEAR,
                )
            };
            studio.photo.tex = Some(tex);
        }
        studio.photo.built_version = studio.photo.sel_version;
    }
    if studio.photo.orig_built != studio.photo.sel_version
        && let Some(img) = studio.photo.selected()
    {
        let (pw, ph) = (img.preview.w as usize, img.preview.h as usize);
        let image = ColorImage::from_rgba_unmultiplied([pw, ph], &img.preview.data);
        let tex = if let Some(mut existing) = studio.photo.orig_tex.take()
            && existing.size() == [pw, ph]
        {
            existing.set(image, TextureOptions::LINEAR);
            existing
        } else {
            ui.ctx().load_texture(
                format!("orig-{}", studio.photo.sel_version),
                image,
                TextureOptions::LINEAR,
            )
        };
        studio.photo.orig_tex = Some(tex);
        studio.photo.orig_built = studio.photo.sel_version;
    }
}

fn filmstrip(ui: &mut Ui, studio: &mut Studio) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Library").strong());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.menu_button("···", |ui| {
                if ui.button("Open photo or settings…").clicked() {
                    ui.close();
                    if let Some(path) = crate::project::dialog_photo() {
                        studio.photo.import_file(&path);
                    }
                }
                if ui.button("Browse folder…").clicked() {
                    ui.close();
                    if let Some(path) = crate::project::dialog_folder() {
                        studio.photo.set_folder(&path.to_string_lossy());
                    }
                }
                if ui.button("Load samples").clicked() {
                    ui.close();
                    studio.photo.import_samples();
                }
                if ui
                    .add_enabled(
                        !studio.photo.is_saving() && studio.photo.has_unsaved_settings(),
                        Button::new("Save all changed settings"),
                    )
                    .clicked()
                {
                    studio.photo.save_all_settings();
                    ui.close();
                }
                if ui.button("Photo presets…").clicked() {
                    workflow::presets(ui.ctx(), studio);
                    ui.close();
                }
            });
        });
    });
    ui.add(
        eframe::egui::Label::new(RichText::new(&studio.photo.status).small().color(fg_weak()))
            .wrap(),
    );
    workflow::progress(ui, studio);
    if studio.photo.queued_imports() > 0 {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(format!("{} opening", studio.photo.queued_imports()));
            if ui.small_button("Cancel").clicked() {
                studio.photo.cancel_imports();
            }
        });
    }
    if !studio.photo.folder_files.is_empty() {
        ui.add_space(8.0);
        ui.add(
            eframe::egui::Label::new(RichText::new(&studio.photo.folder).small().color(fg_weak()))
                .truncate(),
        );
        ScrollArea::vertical()
            .id_salt("photo-folder")
            .max_height(140.0)
            .show(ui, |ui| {
                for index in 0..studio.photo.folder_files.len() {
                    let (name, _) = &studio.photo.folder_files[index];
                    if ui
                        .add(
                            eframe::egui::Label::new(name)
                                .truncate()
                                .sense(Sense::click()),
                        )
                        .clicked()
                    {
                        let path = studio.photo.folder_files[index].1.clone();
                        studio.photo.import_file(std::path::Path::new(&path));
                    }
                }
            });
    }
    ui.add_space(12.0);
    ui.add_enabled_ui(!studio.photo.is_batching(), |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "{} / {} selected",
                    studio.photo.selected_count(),
                    studio.photo.images.len()
                ))
                .small(),
            );
            if ui
                .small_button("All")
                .on_hover_text("Select all loaded photos · Ctrl+A")
                .clicked()
            {
                studio.photo.select_all_images();
            }
            if ui
                .small_button("None")
                .on_hover_text("Clear selection; keep the active preview")
                .clicked()
            {
                studio.photo.deselect_all_images();
            }
        });
    });
    ui.add_space(4.0);
    ScrollArea::vertical()
        .id_salt("photo-library")
        .show(ui, |ui| {
            for i in 0..studio.photo.images.len() {
                let selected = studio.photo.selection.contains(&i);
                let active = studio.photo.selected == Some(i);
                let width = ui.available_width();
                let row = Frame::new()
                    .fill(if selected {
                        accent_soft()
                    } else {
                        Color32::TRANSPARENT
                    })
                    .stroke(Stroke::new(
                        1.0,
                        if active {
                            accent()
                        } else {
                            Color32::TRANSPARENT
                        },
                    ))
                    .corner_radius(7.0)
                    .inner_margin(Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_width((width - 12.0).max(40.0));
                        ui.horizontal(|ui| {
                            if let Some(tex) = studio.photo.thumbs.get(i) {
                                let source = tex.size_vec2();
                                let scale = (52.0 / source.x).min(40.0 / source.y);
                                ui.allocate_ui(vec2(52.0, 40.0), |ui| {
                                    ui.centered_and_justified(|ui| {
                                        ui.image((tex.id(), source * scale))
                                    });
                                });
                            }
                            let img = &studio.photo.images[i];
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 3.0;
                                ui.add(
                                    eframe::egui::Label::new(RichText::new(&img.name).size(11.0))
                                        .truncate(),
                                );
                                ui.label(
                                    RichText::new(format!(
                                        "{} × {}{}",
                                        img.dimensions().0,
                                        img.dimensions().1,
                                        if active { " · Active" } else { "" }
                                    ))
                                    .size(10.0)
                                    .color(fg_weak()),
                                );
                            });
                        });
                    })
                    .response
                    .interact(Sense::click())
                    .on_hover_text(format!(
                        "{}\nCtrl-click to toggle · Shift-click for a range",
                        studio.photo.images[i].name
                    ));
                let press_id = ui.id().with(("photo-selection-press", i));
                if let Some(modifiers) = ui.input(|input| {
                    input.events.iter().find_map(|event| match event {
                        eframe::egui::Event::PointerButton {
                            pos,
                            button: PointerButton::Primary,
                            pressed: true,
                            modifiers,
                        } if row.rect.contains(*pos) => Some(*modifiers),
                        _ => None,
                    })
                }) {
                    ui.data_mut(|data| data.insert_temp(press_id, modifiers));
                }
                if row.clicked() {
                    let modifiers = ui
                        .data_mut(|data| data.remove_temp::<eframe::egui::Modifiers>(press_id))
                        .unwrap_or_else(|| ui.input(|input| input.modifiers));
                    studio.photo.select_with(
                        i,
                        modifiers.command || modifiers.ctrl,
                        modifiers.shift,
                    );
                }
            }
        });
}

fn develop_panel(ui: &mut Ui, studio: &mut Studio) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Develop").size(16.0).strong());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add_enabled(
                    studio.photo.selected().is_some(),
                    Button::new("Reset").frame(false),
                )
                .on_hover_text("Reset all adjustments for this photo")
                .clicked()
                && let Some(img) = studio.photo.selected_mut()
            {
                img.develop = DevelopParams::default();
                studio.photo.dirty = true;
                studio.photo.sel_version += 1;
            }
        });
    });
    let Some(img) = studio.photo.selected() else {
        ui.add_space(24.0);
        ui.label(RichText::new("A little light. A little color.").strong());
        ui.label(RichText::new("Open a photo to make it yours.").color(fg_weak()));
        return;
    };
    ui.add(eframe::egui::Label::new(RichText::new(&img.name).small().color(fg_weak())).truncate());
    if let Some(raw) = &img.raw {
        ui.label(
            RichText::new(format!(
                "RAW · {} {}",
                raw.metadata.make, raw.metadata.model
            ))
            .small(),
        );
        let m = &raw.metadata;
        let mut exposure = Vec::new();
        if m.iso > 0. {
            exposure.push(format!("ISO {:.0}", m.iso));
        }
        if m.aperture > 0. {
            exposure.push(format!("f/{:.1}", m.aperture));
        }
        if m.shutter > 0. {
            exposure.push(if m.shutter < 1. {
                format!("1/{:.0} s", 1. / m.shutter)
            } else {
                format!("{:.1} s", m.shutter)
            });
        }
        ui.label(RichText::new(exposure.join(" · ")).small().color(fg_weak()))
            .on_hover_text(format!("{} · {:.0} mm", m.lens, m.focal_length));
    }
    if !img.notes.is_empty() {
        ui.collapsing("Photo notes", |ui| {
            for note in &img.notes {
                ui.label(note);
            }
        });
    }
    ui.add_space(8.0);
    workflow::toolbar(ui, studio);
    ui.add_space(10.0);
    draw_hist(ui, &studio.photo.hists);
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui
            .button("Auto light")
            .on_hover_text("Balance exposure and contrast automatically")
            .clicked()
            && let Some(img) = studio.photo.selected_mut()
        {
            photo::auto_tone(&mut img.develop, &img.preview);
            studio.photo.dirty = true;
            studio.photo.sel_version += 1;
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.toggle_value(&mut studio.photo.show_original, "Before");
        });
    });
    ui.add_space(12.0);
    let tab_id = ui.id().with("develop-tab");
    let mut tab = ui.data_mut(|data| data.get_temp::<usize>(tab_id).unwrap_or(0));
    ui.columns(3, |columns| {
        for (i, label) in ["Light", "Color", "Detail"].iter().enumerate() {
            if columns[i]
                .add_sized(
                    [columns[i].available_width(), 28.0],
                    Button::selectable(tab == i, *label),
                )
                .clicked()
            {
                tab = i;
            }
        }
    });
    ui.data_mut(|data| data.insert_temp(tab_id, tab));
    ui.add_space(12.0);

    let footer_height = 130.0;
    let mut p = studio.photo.selected().unwrap().develop.clone();
    let mut changed = false;
    ScrollArea::vertical()
        .id_salt(("develop-controls", tab))
        .max_height((ui.available_height() - footer_height).max(80.0))
        .auto_shrink([false, false])
        .show(ui, |ui| match tab {
            0 => {
                changed |= slider(ui, "Exposure", &mut p.exposure, -4.0, 4.0);
                changed |= slider(ui, "Contrast", &mut p.contrast, 0.2, 2.4);
                changed |= slider(ui, "Highlights", &mut p.highlights, -1.0, 1.0);
                changed |= slider(ui, "Shadows", &mut p.shadows, -1.0, 1.0);
                changed |= slider(ui, "Whites", &mut p.whites, -1.0, 1.0);
                changed |= slider(ui, "Blacks", &mut p.blacks, -1.0, 1.0);
                ui.add_space(6.0);
                ui.collapsing("Tone curve", |ui| {
                    for (i, label) in ["Blacks", "Shadows", "Midtones", "Highlights", "Whites"]
                        .iter()
                        .enumerate()
                    {
                        changed |= slider(ui, label, &mut p.curve[i], 0.0, 1.0);
                    }
                });
            }
            1 => {
                changed |= slider(ui, "Temperature", &mut p.temperature, -100.0, 100.0);
                changed |= slider(ui, "Tint", &mut p.tint, -100.0, 100.0);
                changed |= slider(ui, "Vibrance", &mut p.vibrance, 0.0, 2.0);
                changed |= slider(ui, "Saturation", &mut p.saturation, 0.0, 2.0);
                changed |= slider(ui, "Hue", &mut p.hue, -180.0, 180.0);
                ui.add_space(6.0);
                ui.collapsing("Color mixer", |ui| {
                    let id = ui.id().with("hsl-channel");
                    let mut channel = ui.data_mut(|data| data.get_temp::<usize>(id).unwrap_or(0));
                    eframe::egui::ComboBox::from_id_salt("hsl-channel")
                        .selected_text(HSL_NAMES[channel])
                        .width(ui.available_width() - 12.0)
                        .show_ui(ui, |ui| {
                            for (i, name) in HSL_NAMES.iter().enumerate() {
                                ui.selectable_value(&mut channel, i, *name);
                            }
                        });
                    ui.data_mut(|data| data.insert_temp(id, channel));
                    ui.add_space(8.0);
                    changed |= slider(ui, "Hue", &mut p.hsl[channel].hue, -40.0, 40.0);
                    changed |= slider(ui, "Saturation", &mut p.hsl[channel].sat, -1.0, 1.0);
                    changed |= slider(ui, "Luminance", &mut p.hsl[channel].luma, -1.0, 1.0);
                });
                ui.collapsing("Color grading", |ui| {
                    ui.label(RichText::new("Shadows").small().color(fg_weak()));
                    for (i, label) in ["Red", "Green", "Blue"].iter().enumerate() {
                        changed |= slider(ui, label, &mut p.split_shadow[i], -0.4, 0.4);
                    }
                    ui.label(RichText::new("Highlights").small().color(fg_weak()));
                    for (i, label) in ["Red", "Green", "Blue"].iter().enumerate() {
                        changed |= slider(ui, label, &mut p.split_highlight[i], -0.4, 0.4);
                    }
                    changed |= slider(ui, "Balance", &mut p.split_balance, -1.0, 1.0);
                });
            }
            _ => {
                changed |= slider(ui, "Clarity", &mut p.clarity, -1.0, 1.0);
                changed |= slider(ui, "Dehaze", &mut p.dehaze, -1.0, 1.0);
                changed |= slider(ui, "Grain", &mut p.grain, 0.0, 1.0);
                changed |= slider(ui, "Vignette", &mut p.vignette, -1.0, 1.0);
                ui.add_space(10.0);
                ui.label(RichText::new("Orientation").small().color(fg_weak()));
                ui.horizontal(|ui| {
                    for a in [0u32, 90, 180, 270] {
                        changed |= ui
                            .selectable_value(&mut p.rotate, a, format!("{a}°"))
                            .changed();
                    }
                });
                if ui
                    .add_enabled(p.crop.is_some(), Button::new("Clear crop"))
                    .clicked()
                {
                    p.crop = None;
                    changed = true;
                }
            }
        });
    if changed && let Some(img) = studio.photo.selected_mut() {
        img.develop = p;
        studio.photo.dirty = true;
        studio.photo.sel_version += 1;
    }
    ui.add_space(10.0);
    ui.separator();
    ui.add_enabled_ui(
        !studio.photo.is_saving() && studio.photo.selected_count() > 0 && studio.photo.selection.iter().all(|i| studio.photo.images.get(*i).is_some_and(|p| p.source.is_some())),
        |ui| {
            if ui
                .add_sized(
                    [ui.available_width(), 28.0],
                    Button::new(if studio.photo.is_saving() {
                        "Saving…".to_owned()
                    } else if studio.photo.selected_count() > 1 {
                        format!("Save {} selected settings", studio.photo.selected_count())
                    } else if studio.photo.settings_dirty() {
                        "Save settings •".to_owned()
                    } else {
                        "Save settings".to_owned()
                    }),
                )
                .on_hover_text(
                    "Save selected photos' adjustments beside their originals (Ctrl+S). Open a photo or its .omaphoto file to resume editing.",
                )
                .on_disabled_hover_text(if studio.photo.is_saving() {
                    "The current save is still finishing."
                } else {
                    "Open a photo from disk before saving settings. Samples and pasted images have no original file."
                })
                .clicked()
            {
                studio.photo.save_selected_settings();
            }
        },
    );
    let exporting = is_exporting(ui.ctx());
    ui.add_enabled_ui(!exporting, |ui| {
        ui.horizontal(|ui| {
            let id = ui.id().with("photo-export-format");
            let mut extension =
                ui.data_mut(|data| data.get_temp::<String>(id).unwrap_or_else(|| "jpg".into()));
            eframe::egui::ComboBox::from_id_salt("photo-export-format")
                .selected_text(match extension.as_str() {
                    "png" => "PNG",
                    "tif" => "TIFF",
                    _ => "JPEG",
                })
                .width(78.)
                .show_ui(ui, |ui| {
                    for (ext, label) in [("jpg", "JPEG"), ("png", "PNG"), ("tif", "TIFF")] {
                        ui.selectable_value(&mut extension, ext.into(), label);
                    }
                });
            ui.data_mut(|data| data.insert_temp(id, extension.clone()));
            if ui
                .button(if exporting {
                    "Exporting…"
                } else {
                    "Export…"
                })
                .on_hover_text("Full resolution. RAW PNG and TIFF retain 16-bit precision.")
                .clicked()
            {
                export_developed(ui.ctx(), studio, &extension);
            }
        });
    });
    if ui
        .add_sized(
            [ui.available_width(), 28.0],
            Button::new("Place in Design").frame(false),
        )
        .clicked()
    {
        studio.send_photo_to_design();
    }
}

fn slider(ui: &mut Ui, label: &str, v: &mut f32, lo: f32, hi: f32) -> bool {
    let mut changed = false;
    ui.push_id(label, |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.spacing_mut().interact_size.y = 18.0;
        ui.horizontal(|ui| {
            ui.label(label);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                changed |= ui
                    .add(
                        DragValue::new(v)
                            .range(lo..=hi)
                            .speed((hi - lo) / 200.0)
                            .max_decimals(2),
                    )
                    .changed();
            });
        });
        ui.spacing_mut().slider_width = ui.available_width();
        changed |= ui.add(Slider::new(v, lo..=hi).show_value(false)).changed();
        ui.add_space(8.0);
    });
    changed
}

fn draw_hist(ui: &mut Ui, hists: &[Histogram; 4]) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::hover());
    ui.painter().rect_filled(rect, 3.0, bg_extreme());
    let cols = [
        Color32::from_rgba_unmultiplied(220, 70, 70, 140),
        Color32::from_rgba_unmultiplied(70, 200, 90, 140),
        Color32::from_rgba_unmultiplied(70, 120, 230, 140),
        Color32::from_rgba_unmultiplied(230, 230, 230, 180),
    ];
    for (hi, col) in hists.iter().zip(cols) {
        let max = hi.max.max(1) as f32;
        let mut pts = Vec::with_capacity(256);
        for (x, bin) in hi.bins.iter().enumerate() {
            let nx = rect.min.x + rect.width() * x as f32 / 255.0;
            let ny = rect.max.y - rect.height() * (*bin as f32 / max).sqrt();
            pts.push(pos2(nx, ny));
        }
        ui.painter()
            .add(eframe::egui::Shape::line(pts, Stroke::new(1.0, col)));
    }
}

fn viewer(ui: &mut Ui, studio: &mut Studio) {
    if !ui.is_enabled() {
        studio.photo.crop_drag = None;
    }
    let (rect, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
    let painter = ui.painter().with_clip_rect(rect);
    let resp = ui.interact(
        rect,
        eframe::egui::Id::new("studio-photo-canvas"),
        Sense::click_and_drag(),
    );
    painter.rect_filled(rect, 0.0, bg_canvas());
    let tex = if studio.photo.show_original {
        studio.photo.orig_tex.as_ref()
    } else {
        studio.photo.tex.as_ref().or(studio.photo.orig_tex.as_ref())
    };
    let Some(tex) = tex else {
        painter.text(
            rect.center(),
            eframe::egui::Align2::CENTER_CENTER,
            "Drop photos here, or load samples",
            eframe::egui::FontId::proportional(16.0),
            fg_weak(),
        );
        handle_drops(ui, studio);
        return;
    };
    let size = tex.size_vec2();
    let full_size = studio.photo.selected().map_or(size, |image| {
        let (w, h) = image.dimensions();
        let (w, h) = if studio.photo.show_original {
            (w, h)
        } else {
            image.develop.output_dim(w, h)
        };
        vec2(w as f32, h as f32)
    });
    studio.photo.fit_scale = (rect.width() / full_size.x).min(rect.height() / full_size.y) * 0.96;
    let scale = studio.photo.fit_scale * studio.photo.view_scale;
    let vis = full_size * scale;
    let mut dest = Rect::from_center_size(rect.center() + studio.photo.view_offset, vis);
    let ppp = ui.ctx().pixels_per_point();
    if (scale * ppp - 1.0).abs() < 0.001 {
        // At actual size, align source samples to physical pixels. Rounded
        // preview dimensions must not distort or blur the full-resolution tile.
        let min = pos2(
            (dest.min.x * ppp).round() / ppp,
            (dest.min.y * ppp).round() / ppp,
        );
        dest = Rect::from_min_size(min, full_size / ppp);
    }
    painter.image(
        tex.id(),
        dest,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );

    super::photo_detail::draw(ui, studio, dest, rect, size);

    if !ui.is_enabled() {
        return;
    }

    let panning = ui.ctx().input(|i| i.key_down(eframe::egui::Key::Space))
        || studio.tool == crate::tools::Tool::Hand;
    if studio.tool == crate::tools::Tool::Crop && !panning && !studio.photo.show_original {
        if resp.ctx.input(|i| i.pointer.primary_pressed())
            && let Some(a) = resp.interact_pointer_pos()
        {
            let start = to_img(a, dest, size);
            studio.photo.crop_drag = Some((start, start));
        }
        if resp.dragged_by(PointerButton::Primary)
            && let Some(b) = resp.interact_pointer_pos().or(resp.hover_pos())
        {
            let cur = to_img(b, dest, size);
            // Enter/Escape may end a crop while the button is still held.
            // Only a new pointer press may start the next crop.
            if let Some((_, c)) = &mut studio.photo.crop_drag {
                *c = cur;
            }
        }
        if let Some((start, cur)) = studio.photo.crop_drag {
            let ra = dest.min
                + eframe::egui::vec2(
                    start.x / size.x * dest.width(),
                    start.y / size.y * dest.height(),
                );
            let rb = dest.min
                + eframe::egui::vec2(
                    cur.x / size.x * dest.width(),
                    cur.y / size.y * dest.height(),
                );
            painter.rect_stroke(
                Rect::from_two_pos(ra, rb),
                0.0,
                Stroke::new(1.0, accent()),
                eframe::egui::StrokeKind::Middle,
            );
        }
        if studio.photo.crop_drag.is_some()
            && !resp
                .ctx
                .input(|i| i.pointer.button_down(PointerButton::Primary))
            && let Some((a, b)) = studio.photo.crop_drag.take()
        {
            studio.commit_photo_crop(a, b);
        }
    }

    if resp.dragged_by(PointerButton::Middle)
        || (panning && resp.dragged_by(PointerButton::Primary))
    {
        studio.photo.view_offset += resp.drag_delta();
    }
    let z = ui.ctx().input(|i| i.zoom_delta());
    let scroll = ui.ctx().input(|i| i.smooth_scroll_delta);
    if resp.hovered() || resp.dragged() {
        if (z - 1.0).abs() > 1e-4 {
            studio.photo.view_scale = (studio.photo.view_scale * z).clamp(0.1, 8.0);
            ui.ctx().request_repaint();
        } else if scroll != eframe::egui::Vec2::ZERO {
            if studio.tool == crate::tools::Tool::Zoom {
                studio.photo.view_scale =
                    (studio.photo.view_scale * (scroll.y / 200.0).exp()).clamp(0.1, 8.0);
            } else {
                studio.photo.view_offset += scroll;
            }
        }
    }
    handle_drops(ui, studio);
}

fn to_img(p: Pos2, dest: Rect, size: eframe::egui::Vec2) -> crate::geom::Pt {
    crate::geom::Pt::new(
        ((p.x - dest.min.x) / dest.width() * size.x).clamp(0.0, size.x),
        ((p.y - dest.min.y) / dest.height() * size.y).clamp(0.0, size.y),
    )
}

fn handle_drops(ui: &mut Ui, studio: &mut Studio) {
    if !ui.is_enabled() {
        return;
    }
    let files: Vec<_> = ui.ctx().input(|i| i.raw.dropped_files.clone());
    for f in files {
        studio.ingest_dropped(f.path(), None);
    }
}

pub(crate) fn export_developed(ctx: &eframe::egui::Context, studio: &mut Studio, extension: &str) {
    if is_exporting(ctx) {
        return;
    }
    let Some(img) = studio.photo.selected().cloned() else {
        return;
    };
    let Some(path) = crate::project::dialog_export(&extension.to_ascii_uppercase(), extension)
    else {
        return;
    };
    studio.photo.status = "Exporting full-resolution photo…".into();
    studio.photo.clear_save_error();
    super::jobs::start(ctx, "photo-export", move || {
        img.export_to(&path)?;
        Ok(path)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Context, Event, Key, Modifiers, MouseWheelUnit, RawInput, TouchPhase};

    fn fixture() -> (Context, Studio) {
        let ctx = Context::default();
        crate::ui::theme::apply(&ctx);
        let mut studio = Studio::new();
        studio.photo.tex = Some(ctx.load_texture(
            "photo-gesture-fixture",
            eframe::egui::ColorImage::filled([80, 60], Color32::GRAY),
            eframe::egui::TextureOptions::LINEAR,
        ));
        (ctx, studio)
    }

    fn frame(ctx: &Context, studio: &mut Studio, events: Vec<Event>) {
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))),
                events,
                ..Default::default()
            },
            |ui| {
                studio.handle_shortcuts(ui.ctx());
                viewer(ui, studio);
            },
        );
        output.textures_delta.clear();
    }

    #[test]
    fn library_selection_and_paste_dialog_apply_once_and_keep_framing() {
        use std::collections::HashMap;
        fn show_frame(
            ctx: &Context,
            studio: &mut Studio,
            events: Vec<Event>,
        ) -> HashMap<String, Rect> {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200., 900.))),
                    events,
                    ..Default::default()
                },
                |ui| show(ui, studio),
            );
            fn collect(shape: &eframe::egui::Shape, result: &mut HashMap<String, Rect>) {
                match shape {
                    eframe::egui::Shape::Text(t) => {
                        result
                            .entry(t.galley.job.text.clone())
                            .or_insert(t.galley.rect.translate(t.pos.to_vec2()));
                    }
                    eframe::egui::Shape::Vec(shapes) => {
                        for s in shapes {
                            collect(s, result);
                        }
                    }
                    _ => {}
                }
            }
            let mut result = HashMap::new();
            for shape in output.shapes {
                collect(&shape.shape, &mut result);
            }
            output.textures_delta.clear();
            result
        }
        fn click(
            ctx: &Context,
            studio: &mut Studio,
            at: Pos2,
            modifiers: Modifiers,
        ) -> HashMap<String, Rect> {
            show_frame(
                ctx,
                studio,
                vec![
                    Event::PointerMoved(at),
                    Event::PointerButton {
                        pos: at,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers,
                    },
                ],
            );
            show_frame(
                ctx,
                studio,
                vec![
                    Event::PointerButton {
                        pos: at,
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers,
                    },
                    Event::ModifiersChanged(Modifiers::NONE),
                ],
            );
            show_frame(ctx, studio, vec![])
        }
        let ctx = Context::default();
        crate::ui::theme::apply(&ctx);
        let mut studio = Studio::new();
        studio.persona = crate::tools::Persona::Photo;
        for name in ["Source", "Second", "Third"] {
            studio.photo.import_image(
                name.into(),
                photo::RgbaImage::new(8, 8, [80, 100, 120, 255].repeat(64)).unwrap(),
            );
        }
        studio.photo.images[0].develop.exposure = 1.25;
        studio.photo.images[1].develop.crop = Some([0.1, 0.2, 0.8, 0.9]);
        studio.photo.images[1].develop.rotate = 90;
        studio.photo.select_image(0);
        let labels = show_frame(&ctx, &mut studio, vec![]);
        let labels = click(
            &ctx,
            &mut studio,
            labels["Copy adjustments"].center(),
            Modifiers::NONE,
        );
        assert!(studio.photo.copied_adjustments.is_some());
        let labels = click(
            &ctx,
            &mut studio,
            labels["Second"].center(),
            Modifiers::CTRL,
        );
        assert_eq!(studio.photo.selected_count(), 2);
        let labels = click(
            &ctx,
            &mut studio,
            labels["Second"].center(),
            Modifiers::CTRL,
        );
        assert_eq!(
            studio.photo.selection.iter().copied().collect::<Vec<_>>(),
            vec![0]
        );
        let labels = click(
            &ctx,
            &mut studio,
            labels["Source"].center(),
            Modifiers::NONE,
        );
        let labels = click(
            &ctx,
            &mut studio,
            labels["Third"].center(),
            Modifiers::SHIFT,
        );
        assert_eq!(studio.photo.selected_count(), 3);
        click(
            &ctx,
            &mut studio,
            labels["Paste…"].center(),
            Modifiers::NONE,
        );
        let labels = show_frame(&ctx, &mut studio, vec![]);
        assert!(labels.contains_key("Crop"));
        click(
            &ctx,
            &mut studio,
            labels["Apply to 3 photos"].center(),
            Modifiers::NONE,
        );
        assert!(
            studio
                .photo
                .images
                .iter()
                .all(|image| image.develop.exposure == 1.25)
        );
        assert_eq!(
            studio.photo.images[1].develop.crop,
            Some([0.1, 0.2, 0.8, 0.9])
        );
        assert_eq!(studio.photo.images[1].develop.rotate, 90);
        show_frame(&ctx, &mut studio, vec![]);
        studio.photo.undo();
        assert_eq!(studio.photo.images[1].develop.exposure, 0.0);
        assert_eq!(studio.photo.images[2].develop.exposure, 0.0);
        assert!(
            !studio.photo.can_undo(),
            "One application must create exactly one history entry"
        );
        studio.photo.redo();
        assert_eq!(studio.photo.images[2].develop.exposure, 1.25);
    }

    #[test]
    fn disabled_photo_view_cancels_pending_crop_without_committing_it() {
        let (ctx, mut studio) = fixture();
        studio.persona = crate::tools::Persona::Photo;
        studio.tool = crate::tools::Tool::Crop;
        studio.photo.import_image(
            "Original".into(),
            photo::RgbaImage::new(80, 60, [100, 100, 100, 255].repeat(80 * 60)).unwrap(),
        );
        studio.photo.crop_drag = Some((
            crate::geom::Pt::new(10., 10.),
            crate::geom::Pt::new(50., 40.),
        ));
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            ui.add_enabled_ui(false, |ui| viewer(ui, &mut studio));
        });
        output.textures_delta.clear();
        assert!(studio.photo.crop_drag.is_none());
        assert!(studio.photo.selected().unwrap().develop.crop.is_none());
        assert!(!studio.photo.can_undo());
        frame(&ctx, &mut studio, vec![]);
        assert!(studio.photo.selected().unwrap().develop.crop.is_none());
    }

    #[test]
    fn zoomed_photo_is_clipped_to_the_canvas() {
        let (ctx, mut studio) = fixture();
        studio.photo.view_scale = 4.0;
        let texture = studio.photo.tex.as_ref().unwrap().id();
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))),
                ..Default::default()
            },
            |ui| {
                eframe::egui::Panel::left("photo-test-sidebar")
                    .exact_size(100.0)
                    .show(ui, |ui| {
                        ui.label("Photo tools");
                    });
                viewer(ui, &mut studio);
            },
        );
        let image = output.shapes.iter().find(|shape| matches!(&shape.shape, eframe::egui::Shape::Mesh(mesh) if mesh.texture_id == texture)).expect("photo was painted");
        assert!(
            image.clip_rect.min.x >= 100.0,
            "zoomed pixels must not cover the sidebar"
        );
        output.textures_delta.clear();
    }

    #[test]
    fn photo_crop_enter_and_escape_finish_the_gesture_until_the_next_press() {
        for key in [Key::Enter, Key::Escape] {
            let (ctx, mut studio) = fixture();
            studio.persona = crate::tools::Persona::Photo;
            studio.tool = crate::tools::Tool::Crop;
            studio.photo.images.push(photo::PhotoImage::from_full(
                "Crop fixture".into(),
                photo::RgbaImage {
                    w: 80,
                    h: 60,
                    data: [128, 128, 128, 255].repeat(80 * 60),
                },
            ));
            studio.photo.selected = Some(0);
            let original = Some([0.1, 0.1, 0.9, 0.9]);
            studio.photo.selected_mut().unwrap().develop.crop = original;
            studio.photo.tex = Some(ctx.load_texture(
                "cropped-photo-gesture-fixture",
                eframe::egui::ColorImage::filled([64, 48], Color32::GRAY),
                TextureOptions::LINEAR,
            ));
            let press = |pos, pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            let start = pos2(100.0, 75.0);
            frame(&ctx, &mut studio, vec![Event::PointerMoved(start)]);
            frame(&ctx, &mut studio, vec![press(start, true)]);
            frame(
                &ctx,
                &mut studio,
                vec![Event::PointerMoved(pos2(220.0, 185.0))],
            );
            let (a, b) = studio.photo.crop_drag.expect("drag creates a crop");
            let version = studio.photo.sel_version;
            frame(
                &ctx,
                &mut studio,
                vec![Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
            );
            assert!(studio.photo.crop_drag.is_none());
            let committed = studio.photo.selected().unwrap().develop.crop;
            if key == Key::Enter {
                assert_eq!(
                    committed,
                    Some([
                        0.1 + a.x.min(b.x) / 64.0 * (0.9 - 0.1),
                        0.1 + a.y.min(b.y) / 48.0 * (0.9 - 0.1),
                        0.1 + a.x.max(b.x) / 64.0 * (0.9 - 0.1),
                        0.1 + a.y.max(b.y) / 48.0 * (0.9 - 0.1)
                    ])
                );
                assert_eq!(studio.photo.sel_version, version + 1);
            } else {
                assert_eq!(committed, original);
                assert_eq!(studio.photo.sel_version, version);
            }
            let version = studio.photo.sel_version;
            frame(
                &ctx,
                &mut studio,
                vec![Event::PointerMoved(pos2(260.0, 200.0))],
            );
            frame(&ctx, &mut studio, vec![press(pos2(260.0, 200.0), false)]);
            assert!(
                studio.photo.crop_drag.is_none(),
                "continued dragging must not recreate the crop"
            );
            assert_eq!(studio.photo.selected().unwrap().develop.crop, committed);
            assert_eq!(
                studio.photo.sel_version, version,
                "release must not commit twice"
            );
            frame(&ctx, &mut studio, vec![press(start, true)]);
            frame(
                &ctx,
                &mut studio,
                vec![Event::PointerMoved(pos2(240.0, 210.0))],
            );
            assert!(
                studio.photo.crop_drag.is_some(),
                "a new press starts a fresh crop"
            );
            frame(&ctx, &mut studio, vec![press(pos2(240.0, 210.0), false)]);
            assert!(studio.photo.crop_drag.is_none());
            assert_eq!(
                studio.photo.sel_version,
                version + 1,
                "release still commits a fresh crop"
            );
        }
    }

    #[test]
    fn modifier_scroll_zooms_only_inside_the_photo() {
        for modifiers in [Modifiers::CTRL | Modifiers::COMMAND, Modifiers::ALT] {
            for hovered in [false, true] {
                let (ctx, mut studio) = fixture();
                studio.tool = crate::tools::Tool::Hand;
                let pointer = if hovered {
                    pos2(200.0, 150.0)
                } else {
                    pos2(500.0, 150.0)
                };
                frame(&ctx, &mut studio, vec![Event::PointerMoved(pointer)]);
                frame(
                    &ctx,
                    &mut studio,
                    vec![
                        Event::ModifiersChanged(modifiers),
                        Event::MouseWheel {
                            unit: MouseWheelUnit::Point,
                            delta: vec2(0.0, 40.0),
                            phase: TouchPhase::Move,
                            modifiers,
                        },
                        Event::ModifiersChanged(Modifiers::NONE),
                    ],
                );
                frame(&ctx, &mut studio, vec![]);
                if hovered {
                    assert!(studio.photo.view_scale > 1.0, "{modifiers:?}");
                } else {
                    assert_eq!(studio.photo.view_scale, 1.0);
                }
                assert_eq!(studio.photo.view_offset, eframe::egui::Vec2::ZERO);
            }
        }
    }

    #[test]
    fn space_drag_moves_the_photo_without_starting_a_crop() {
        let (ctx, mut studio) = fixture();
        studio.tool = crate::tools::Tool::Crop;
        let start = pos2(200.0, 150.0);
        frame(&ctx, &mut studio, vec![Event::PointerMoved(start)]);
        frame(
            &ctx,
            &mut studio,
            vec![
                Event::Key {
                    key: Key::Space,
                    physical_key: Some(Key::Space),
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
                Event::PointerButton {
                    pos: start,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        frame(
            &ctx,
            &mut studio,
            vec![Event::PointerMoved(start + vec2(36.0, 24.0))],
        );
        assert_eq!(studio.photo.view_offset, vec2(36.0, 24.0));
        assert!(studio.photo.crop_drag.is_none());
    }

    #[test]
    fn actual_size_uses_original_photo_pixels_with_a_downscaled_preview() {
        let (ctx, mut studio) = fixture();
        studio.persona = crate::tools::Persona::Photo;
        let full = photo::RgbaImage {
            w: 800,
            h: 600,
            data: vec![128; 800 * 600 * 4],
        };
        let mut image = photo::PhotoImage::from_full("Full-size fixture".into(), full);
        image.preview = image.full.downscaled(80).into();
        studio.photo.images.push(image);
        studio.photo.selected = Some(0);
        frame(&ctx, &mut studio, vec![]);
        let texture = studio.photo.tex.as_ref().unwrap().id();
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))),
                events: vec![Event::Key {
                    key: Key::Num1,
                    physical_key: Some(Key::Num1),
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::CTRL | Modifiers::COMMAND,
                }],
                ..Default::default()
            },
            |ui| {
                studio.handle_shortcuts(ui.ctx());
                viewer(ui, &mut studio);
            },
        );
        output.textures_delta.clear();
        let bounds = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                eframe::egui::Shape::Mesh(mesh) if mesh.texture_id == texture => {
                    Some(mesh.calc_bounds())
                }
                _ => None,
            })
            .expect("photo texture drawn");
        assert!((bounds.width() - 800.0).abs() < 0.01);
        assert!((bounds.height() - 600.0).abs() < 0.01);
    }
}
