use crate::app::Studio;
use eframe::egui::{self, Ui};
pub fn show(ui: &mut Ui, studio: &mut Studio, font: &str) {
    let features = crate::text::font_features(font);
    let has = |tag: &[u8; 4]| features.iter().any(|(t, _)| t.to_bytes() == *tag);
    egui::CollapsingHeader::new("OpenType features").show(ui, |ui| {
        if features.is_empty() {
            ui.small("This font has no OpenType features.");
            return;
        }
        for (tag, label) in [
            (*b"liga", "Ligatures"),
            (*b"dlig", "Discretionary ligatures"),
            (*b"hlig", "Historical ligatures"),
            (*b"swsh", "Swash"),
            (*b"cswh", "Contextual swash"),
            (*b"salt", "Stylistic alternates"),
            (*b"calt", "Contextual alternates"),
            (*b"smcp", "Small caps"),
            (*b"c2sc", "Capitals to small caps"),
            (*b"cpsp", "Capital spacing"),
            (*b"case", "Case-sensitive forms"),
            (*b"zero", "Slashed zero"),
            (*b"frac", "Fractions"),
        ] {
            if has(&tag) {
                toggle(ui, studio, tag, label);
            }
        }
        if has(b"onum") || has(b"lnum") || has(b"tnum") || has(b"pnum") {
            let current =
                [*b"onum", *b"lnum", *b"pnum", *b"tnum"].map(|tag| studio.selection_feature(tag));
            let label = if current.iter().any(Option::is_none) {
                "Mixed"
            } else {
                match current.map(|v| v.unwrap_or(0) > 0) {
                    [true, _, _, true] => "Tabular oldstyle",
                    [true, _, _, _] => "Proportional oldstyle",
                    [_, true, _, true] => "Tabular lining",
                    [_, true, _, _] => "Proportional lining",
                    _ => "Default",
                }
            };
            egui::ComboBox::from_label("Figures")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    for (name, values, tags) in [
                        ("Default", [0, 0, 0, 0], None),
                        (
                            "Proportional oldstyle",
                            [1, 0, 1, 0],
                            Some([*b"onum", *b"pnum"]),
                        ),
                        ("Tabular oldstyle", [1, 0, 0, 1], Some([*b"onum", *b"tnum"])),
                        (
                            "Proportional lining",
                            [0, 1, 1, 0],
                            Some([*b"lnum", *b"pnum"]),
                        ),
                        ("Tabular lining", [0, 1, 0, 1], Some([*b"lnum", *b"tnum"])),
                    ] {
                        if tags.is_none_or(|tags| tags.iter().all(has))
                            && ui.selectable_label(label == name, name).clicked()
                        {
                            for (tag, value) in [*b"onum", *b"lnum", *b"pnum", *b"tnum"]
                                .into_iter()
                                .zip(values)
                            {
                                studio.patch_feature(tag, value);
                            }
                        }
                    }
                });
        }
        if has(b"sups") || has(b"subs") || has(b"ordn") {
            let label = if studio.selection_feature(*b"sups") == Some(1) {
                "Superscript"
            } else if studio.selection_feature(*b"subs") == Some(1) {
                "Subscript"
            } else if studio.selection_feature(*b"ordn") == Some(1) {
                "Ordinal"
            } else {
                "Normal"
            };
            egui::ComboBox::from_label("Position")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    for (name, tag) in [
                        ("Normal", None),
                        ("Superscript", Some(*b"sups")),
                        ("Subscript", Some(*b"subs")),
                        ("Ordinal", Some(*b"ordn")),
                    ] {
                        if tag.is_none_or(|t| has(&t))
                            && ui.selectable_label(name == label, name).clicked()
                        {
                            for t in [*b"sups", *b"subs", *b"ordn"] {
                                studio.patch_feature(t, u32::from(Some(t) == tag));
                            }
                        }
                    }
                });
        }
        let sets: Vec<_> = features
            .iter()
            .filter(|(tag, _)| tag.to_bytes().starts_with(b"ss"))
            .collect();
        if !sets.is_empty() {
            ui.separator();
            ui.strong("Stylistic sets");
            for (tag, name) in sets {
                let tag = tag.to_bytes();
                let code = String::from_utf8_lossy(&tag);
                let label = name
                    .as_ref()
                    .map_or_else(|| code.to_string(), |name| format!("{code} — {name}"));
                toggle(ui, studio, tag, &label);
            }
        }
    });
}
fn toggle(ui: &mut Ui, studio: &mut Studio, tag: [u8; 4], label: &str) {
    let value = studio.selection_feature(tag);
    let mut on = value.unwrap_or(0) > 0;
    if ui
        .add(egui::Checkbox::new(&mut on, label).indeterminate(value.is_none()))
        .changed()
    {
        studio.patch_feature(tag, u32::from(on));
        if studio.type_edit.is_some(){ui.memory_mut(|m|m.request_focus(egui::Id::new("studio-canvas")));}
        if tag == *b"liga" {
            studio.patch_feature(*b"clig", u32::from(on));
        }
    }
}

pub fn alternates(ui: &mut Ui, studio: &mut Studio, anchor: egui::Pos2) {
    let Some(edit) = studio.type_edit.clone() else {
        return;
    };
    let Some(run) = studio.selected_type() else {
        return;
    };
    let choices = crate::text::glyph_alternates(
        &run,
        edit.caret.min(edit.anchor),
        edit.caret.max(edit.anchor),
    );
    if choices.is_empty() {
        return;
    }
    egui::Area::new(egui::Id::new("glyph_alternates"))
        .order(egui::Order::Foreground)
        .fixed_pos(anchor)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.small("Glyph alternates");
                ui.horizontal(|ui| {
                    for choice in choices.iter().take(12) {
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(38., 48.), egui::Sense::click());
                        let key = egui::Id::new((
                            "glyph_thumbnail",
                            &run.font,
                            &choice.preview.content,
                            choice.tag,
                            choice.value,
                        ));
                        let texture = ui
                            .ctx()
                            .data_mut(|d| d.get_temp::<egui::TextureHandle>(key))
                            .unwrap_or_else(|| {
                                let contours = crate::text::shape(&choice.preview);
                                let mut min = crate::geom::Pt::new(f32::INFINITY, f32::INFINITY);
                                let mut max =
                                    crate::geom::Pt::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
                                for p in contours.iter().flatten() {
                                    min.x = min.x.min(p.x);
                                    min.y = min.y.min(p.y);
                                    max.x = max.x.max(p.x);
                                    max.y = max.y.max(p.y);
                                }
                                let scale = (30. / (max.x - min.x).max(1.))
                                    .min(40. / (max.y - min.y).max(1.));
                                let mut builder = tiny_skia::PathBuilder::new();
                                for contour in contours {
                                    if let Some(p) = contour.first() {
                                        builder.move_to(
                                            4. + (p.x - min.x) * scale,
                                            4. + (p.y - min.y) * scale,
                                        );
                                        for p in &contour[1..] {
                                            builder.line_to(
                                                4. + (p.x - min.x) * scale,
                                                4. + (p.y - min.y) * scale,
                                            );
                                        }
                                        builder.close();
                                    }
                                }
                                let mut pixmap = tiny_skia::Pixmap::new(38, 48).unwrap();
                                let mut paint = tiny_skia::Paint::default();
                                paint.set_color_rgba8(220, 225, 235, 255);
                                if let Some(path) = builder.finish() {
                                    pixmap.fill_path(
                                        &path,
                                        &paint,
                                        tiny_skia::FillRule::Winding,
                                        tiny_skia::Transform::identity(),
                                        None,
                                    );
                                }
                                let image = egui::ColorImage::from_rgba_premultiplied(
                                    [38, 48],
                                    pixmap.data(),
                                );
                                let handle = ui.ctx().load_texture(
                                    format!("{key:?}"),
                                    image,
                                    Default::default(),
                                );
                                ui.ctx().data_mut(|d| d.insert_temp(key, handle.clone()));
                                handle
                            });
                        ui.painter().image(
                            texture.id(),
                            rect,
                            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                            egui::Color32::WHITE,
                        );
                        let response = response.on_hover_text(format!(
                            "{} · {}",
                            String::from_utf8_lossy(&choice.tag),
                            choice.value
                        ));
                        if response.clicked() {
                            studio.patch_feature(choice.tag, choice.value);
                        ui.memory_mut(|m|m.request_focus(egui::Id::new("studio-canvas")));
                        }
                    }
                });
            });
        });
}
