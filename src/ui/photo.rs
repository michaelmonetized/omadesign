use crate::app::Studio;
use crate::photo::{self, DevelopParams, Histogram, HSL_NAMES};
use crate::ui::theme::{accent, accent_dim, bg_canvas, bg_extreme, fg_weak};
use eframe::egui::{
    Color32, ColorImage, PointerButton, Pos2, Rect, RichText, ScrollArea, Sense, Slider, Stroke,
    TextureOptions, Ui, pos2, vec2,
};

pub fn show(ui: &mut Ui, studio: &mut Studio) {
    if studio.photo.dirty || studio.photo.built_version != studio.photo.sel_version {
        studio.photo.rebuild();
    }
    upload_textures(ui, studio);

    eframe::egui::Panel::left("filmstrip")
        .resizable(true)
        .default_size(200.0)
        .show(ui, |ui| {
            filmstrip(ui, studio);
        });
    eframe::egui::Panel::right("develop")
        .resizable(true)
        .default_size(300.0)
        .show(ui, |ui| {
            develop_panel(ui, studio);
        });
    viewer(ui, studio);
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
    // Original preview: only re-upload when sel_version changes
    let orig_data = studio.photo.selected().map(|img| (img.preview.w, img.preview.h, img.preview.data.clone()));
    if let Some((pw, ph, data)) = orig_data {
        let tex = if let Some(mut existing) = studio.photo.orig_tex.take()
            && existing.size() == [pw as usize, ph as usize]
        {
            existing.set(
                ColorImage::from_rgba_unmultiplied([pw as usize, ph as usize], &data),
                TextureOptions::LINEAR,
            );
            existing
        } else {
            ui.ctx().load_texture(
                format!("orig-{}", studio.photo.sel_version),
                ColorImage::from_rgba_unmultiplied([pw as usize, ph as usize], &data),
                TextureOptions::LINEAR,
            )
        };
        studio.photo.orig_tex = Some(tex);
    }
}

fn filmstrip(ui: &mut Ui, studio: &mut Studio) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Library").strong());
        if ui.small_button("Folder").clicked() {
            if let Some(p) = crate::project::dialog_folder() {
                studio.photo.set_folder(&p.to_string_lossy());
            }
        }
        if ui.small_button("Open").clicked() {
            if let Some(p) = crate::project::dialog_open() {
                studio.photo.import_file(&p);
            }
        }
        if ui.small_button("Samples").clicked() {
            studio.photo.import_samples();
        }
    });
    ui.label(RichText::new(&studio.photo.status).small().color(fg_weak()));
    if !studio.photo.folder_files.is_empty() {
        ui.label(RichText::new(&studio.photo.folder).small().color(fg_weak()));
        ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
            let files = studio.photo.folder_files.clone();
            for (name, path) in files {
                if ui.small_button(&name).clicked() {
                    studio.photo.import_file(std::path::Path::new(&path));
                }
            }
        });
    }
    ui.separator();
    ScrollArea::vertical().show(ui, |ui| {
        let n = studio.photo.images.len();
        for i in 0..n {
            let on = studio.photo.selected == Some(i);
            ui.horizontal(|ui| {
                if let Some(tex) = studio.photo.thumbs.get(i) {
                    let size = vec2(72.0, 48.0);
                    ui.image((tex.id(), size));
                }
                let name = studio.photo.images[i].name.clone();
                let fill = if on { accent_dim() } else { Color32::TRANSPARENT };
                if ui
                    .add(eframe::egui::Button::new(RichText::new(name).size(12.0)).fill(fill))
                    .clicked()
                {
                    studio.photo.selected = Some(i);
                    studio.photo.sel_version += 1;
                    studio.photo.dirty = true;
                }
            });
        }
    });
}

fn develop_panel(ui: &mut Ui, studio: &mut Studio) {
    ScrollArea::vertical().show(ui, |ui| {
        ui.label(RichText::new("Develop").strong().color(accent()));
        if studio.photo.selected().is_none() {
            ui.label(RichText::new("Open a photo or load samples.").color(fg_weak()));
            return;
        }
        ui.horizontal(|ui| {
            if ui.button("Auto").clicked() {
                if let Some(img) = studio.photo.selected_mut() {
                    let src = img.preview.clone();
                    photo::auto_tone(&mut img.develop, &src);
                    studio.photo.dirty = true;
                    studio.photo.sel_version += 1;
                }
            }
            if ui.button("Reset").clicked() {
                if let Some(img) = studio.photo.selected_mut() {
                    img.develop = DevelopParams::default();
                    studio.photo.dirty = true;
                    studio.photo.sel_version += 1;
                }
            }
            ui.checkbox(&mut studio.photo.show_original, "Before");
        });
        draw_hist(ui, &studio.photo.hists);

        let mut p = studio.photo.selected().map(|i| i.develop.clone()).unwrap_or_default();
        let mut changed = false;
        changed |= slider(ui, "Exposure", &mut p.exposure, -4.0, 4.0);
        changed |= slider(ui, "Contrast", &mut p.contrast, 0.2, 2.4);
        changed |= slider(ui, "Highlights", &mut p.highlights, -1.0, 1.0);
        changed |= slider(ui, "Shadows", &mut p.shadows, -1.0, 1.0);
        changed |= slider(ui, "Whites", &mut p.whites, -1.0, 1.0);
        changed |= slider(ui, "Blacks", &mut p.blacks, -1.0, 1.0);
        ui.add_space(6.0);
        ui.label(RichText::new("White balance").small().color(fg_weak()));
        changed |= slider(ui, "Temp", &mut p.temperature, -100.0, 100.0);
        changed |= slider(ui, "Tint", &mut p.tint, -100.0, 100.0);
        ui.add_space(6.0);
        ui.label(RichText::new("Presence").small().color(fg_weak()));
        changed |= slider(ui, "Clarity", &mut p.clarity, -1.0, 1.0);
        changed |= slider(ui, "Dehaze", &mut p.dehaze, -1.0, 1.0);
        changed |= slider(ui, "Vibrance", &mut p.vibrance, 0.0, 2.0);
        changed |= slider(ui, "Saturation", &mut p.saturation, 0.0, 2.0);
        changed |= slider(ui, "Hue", &mut p.hue, -180.0, 180.0);
        ui.add_space(6.0);
        ui.label(RichText::new("Tone curve").small().color(fg_weak()));
        for (i, label) in ["Blacks", "Shadows", "Mid", "Highlights", "Whites"]
            .iter()
            .enumerate()
        {
            changed |= slider(ui, label, &mut p.curve[i], 0.0, 1.0);
        }
        ui.add_space(6.0);
        ui.label(RichText::new("Split tone").small().color(fg_weak()));
        changed |= slider(ui, "Shadow red", &mut p.split_shadow[0], -0.4, 0.4);
        changed |= slider(ui, "Shadow green", &mut p.split_shadow[1], -0.4, 0.4);
        changed |= slider(ui, "Shadow blue", &mut p.split_shadow[2], -0.4, 0.4);
        changed |= slider(ui, "Highlight red", &mut p.split_highlight[0], -0.4, 0.4);
        changed |= slider(ui, "Highlight green", &mut p.split_highlight[1], -0.4, 0.4);
        changed |= slider(ui, "Highlight blue", &mut p.split_highlight[2], -0.4, 0.4);
        changed |= slider(ui, "Balance", &mut p.split_balance, -1.0, 1.0);
        ui.add_space(6.0);
        ui.label(RichText::new("HSL").small().color(fg_weak()));
        for (i, name) in HSL_NAMES.iter().enumerate() {
            ui.label(RichText::new(*name).small());
            changed |= slider(ui, "  hue", &mut p.hsl[i].hue, -40.0, 40.0);
            changed |= slider(ui, "  sat", &mut p.hsl[i].sat, -1.0, 1.0);
            changed |= slider(ui, "  luma", &mut p.hsl[i].luma, -1.0, 1.0);
        }
        ui.add_space(6.0);
        changed |= slider(ui, "Grain", &mut p.grain, 0.0, 1.0);
        changed |= slider(ui, "Vignette", &mut p.vignette, -1.0, 1.0);
        ui.horizontal(|ui| {
            ui.label("Rotate");
            for a in [0u32, 90, 180, 270] {
                if ui.selectable_label(p.rotate == a, format!("{a}°")).clicked() {
                    p.rotate = a;
                    changed = true;
                }
            }
        });
        if ui.button("Clear crop").clicked() {
            p.crop = None;
            changed = true;
        }
        if changed {
            if let Some(img) = studio.photo.selected_mut() {
                img.develop = p;
                studio.photo.dirty = true;
                studio.photo.sel_version += 1;
            }
        }
        ui.add_space(8.0);
        if ui.button("Export developed JPEG").clicked() {
            export_developed(studio);
        }
        if ui.button("Place in Design").clicked() {
            studio.send_photo_to_design();
        }
    });
}

fn slider(ui: &mut Ui, label: &str, v: &mut f32, lo: f32, hi: f32) -> bool {
    ui.add(Slider::new(v, lo..=hi).text(label).show_value(true))
        .changed()
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
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    ui.painter().rect_filled(rect, 0.0, bg_canvas());
    let tex = if studio.photo.show_original {
        studio.photo.orig_tex.as_ref()
    } else {
        studio.photo.tex.as_ref().or(studio.photo.orig_tex.as_ref())
    };
    let Some(tex) = tex else {
        ui.painter().text(
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
    let scale = (rect.width() / size.x)
        .min(rect.height() / size.y)
        * studio.photo.view_scale
        * 0.96;
    let vis = size * scale;
    let dest = Rect::from_center_size(rect.center(), vis);
    ui.painter().image(
        tex.id(),
        dest,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );

    if studio.tool == crate::tools::Tool::Crop && resp.dragged_by(PointerButton::Primary) {
        if let (Some(a), Some(b)) = (resp.interact_pointer_pos(), resp.hover_pos()) {
            let start = to_img(a, dest, size);
            let cur = to_img(b, dest, size);
            studio.photo.crop_drag = Some((start, cur));
            let ra = dest.min + eframe::egui::vec2(start.x / size.x * dest.width(), start.y / size.y * dest.height());
            let rb = dest.min + eframe::egui::vec2(cur.x / size.x * dest.width(), cur.y / size.y * dest.height());
            ui.painter().rect_stroke(
                Rect::from_two_pos(ra, rb),
                0.0,
                Stroke::new(1.0, accent()),
                eframe::egui::StrokeKind::Middle,
            );
        }
    }
    if studio.tool == crate::tools::Tool::Crop && resp.drag_stopped() {
        if let Some((a, b)) = studio.photo.crop_drag.take() {
            studio.commit_photo_crop(a, b);
        }
    }

    if resp.dragged_by(PointerButton::Middle)
        || (ui.ctx().input(|i| i.key_down(eframe::egui::Key::Space))
            && resp.dragged_by(PointerButton::Primary))
    {
        // view_scale only for zoom; pan via scroll
    }
    let z = ui.ctx().input(|i| i.zoom_delta());
    if z != 1.0 {
        studio.photo.view_scale = (studio.photo.view_scale * z).clamp(0.1, 8.0);
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
    let files: Vec<_> = ui
        .ctx()
        .input(|i| i.raw.dropped_files.clone());
    for f in files {
        studio.photo.import_file(f.path());
    }
}

fn export_developed(studio: &mut Studio) {
    let Some(img) = studio.photo.selected() else {
        return;
    };
    let out = photo::develop(&img.full, &img.develop);
    if let Some(path) = crate::project::dialog_export("JPEG", "jpg") {
        if let Some(png) = out.encode_png() {
            if let Ok(dynimg) = image::load_from_memory(&png) {
                let rgb = dynimg.to_rgb8();
                let mut buf = Vec::new();
                let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 92);
                let _ = enc.encode(
                    rgb.as_raw(),
                    rgb.width(),
                    rgb.height(),
                    image::ExtendedColorType::Rgb8,
                );
                let _ = std::fs::write(path, buf);
                studio.photo.status = "exported".into();
            }
        }
    }
}
