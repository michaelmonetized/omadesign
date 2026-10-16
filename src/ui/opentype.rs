use crate::app::Studio;
use eframe::egui::{self, Ui};

#[cfg(test)]
#[path = "opentype_keyboard_tests.rs"]
mod keyboard_tests;

/// Scope feature controls to the artwork owner while keeping edited values out of their IDs.
/// Args: studio. Returns the document and selection feature namespace.
fn feature_owner(studio: &Studio) -> egui::Id {
    egui::Id::new(("opentype",&studio.swap_id,&studio.selection,studio.active_layer.and_then(|layer|studio.doc.layers.get(layer).map(|layer|layer.id))))
}

/// Reveal native feature focus and describe an existing mixed selection.
/// Args: UI, actual response and mixed flag. Returns no value.
fn feature_response(ui: &Ui, response: &egui::Response, mixed: bool) {
    if mixed{ui.ctx().accesskit_node_builder(response.id,|node|{node.set_state_description("Mixed");node.set_value("Mixed");});}
    super::welcome::focus_control(ui,response);
}

pub fn show(ui: &mut Ui, studio: &mut Studio, font: &str) {
    let owner=feature_owner(studio);
    let features = crate::text::font_features(font);
    let has = |tag: &[u8; 4]| features.iter().any(|(t, _)| t.to_bytes() == *tag);
    let section=egui::CollapsingHeader::new("OpenType features").id_salt(owner.with("section")).show(ui, |ui| {
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
            (*b"c2sc", "All small caps"),
            (*b"cpsp", "Capital spacing"),
            (*b"case", "Case-sensitive forms"),
            (*b"zero", "Slashed zero"),
            (*b"frac", "Fractions"),
        ] {
            if has(&tag) && (tag != *b"c2sc" || has(b"smcp")) {
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
            let response=super::fields::native_scope(ui,owner.with("figures"),|ui|egui::ComboBox::from_label("Figures")
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
                            && super::fields::native_scope(ui,owner.with(("figures-choice",values)),|ui|{
                                let response=ui.selectable_label(label==name,name);feature_response(ui,&response,false);response
                            }).inner.clicked()
                        {
                            let features: Vec<_> = [*b"onum", *b"lnum", *b"pnum", *b"tnum"]
                                .into_iter().zip(values).collect();
                            studio.patch_features(&features);
                            restore_edit_focus(ui, studio);
                        }
                    }
                }).response).inner;
            feature_response(ui,&response,current.iter().any(Option::is_none));
            ui.ctx().accesskit_node_builder(response.id, |node| node.set_label("Figures"));
        }
        if has(b"sups") || has(b"subs") || has(b"ordn") {
            let current = [*b"sups", *b"subs", *b"ordn"].map(|tag| studio.selection_feature(tag));
            let label = if current.iter().any(Option::is_none) {
                "Mixed"
            } else if current[0] == Some(1) {
                "Superscript"
            } else if current[1] == Some(1) {
                "Subscript"
            } else if current[2] == Some(1) {
                "Ordinal"
            } else {
                "Normal"
            };
            let response=super::fields::native_scope(ui,owner.with("position"),|ui|egui::ComboBox::from_label("Position")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    for (name, tag) in [
                        ("Normal", None),
                        ("Superscript", Some(*b"sups")),
                        ("Subscript", Some(*b"subs")),
                        ("Ordinal", Some(*b"ordn")),
                    ] {
                        if tag.is_none_or(|t| has(&t))
                            && super::fields::native_scope(ui,owner.with(("position-choice",tag)),|ui|{
                                let response=ui.selectable_label(name==label,name);feature_response(ui,&response,false);response
                            }).inner.clicked()
                        {
                            studio.patch_features(&[*b"sups", *b"subs", *b"ordn"].map(|t| (t, u32::from(Some(t) == tag))));
                            restore_edit_focus(ui, studio);
                        }
                    }
                }).response).inner;
            feature_response(ui,&response,current.iter().any(Option::is_none));
            ui.ctx().accesskit_node_builder(response.id, |node| node.set_label("Position"));
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
    super::welcome::focus_control(ui,&section.header_response);
}
fn restore_edit_focus(ui: &mut Ui, studio: &Studio) {
    if studio.type_edit.is_some() {
        ui.memory_mut(|m| m.request_focus(egui::Id::new("studio-canvas")));
    }
}
fn toggle(ui: &mut Ui, studio: &mut Studio, tag: [u8; 4], label: &str) {
    let value = if tag == *b"c2sc" {
        studio.selection_feature(tag).zip(studio.selection_feature(*b"smcp")).map(|(a,b)|u32::from(a>0&&b>0))
    } else { studio.selection_feature(tag) };
    let mut on = value.unwrap_or(0) > 0;
    let response=super::fields::native_scope(ui,feature_owner(studio).with(("feature",tag)),|ui|
        ui.add(egui::Checkbox::new(&mut on,label).indeterminate(value.is_none()))).inner;
    feature_response(ui,&response,value.is_none());
    if response.changed()
    {
        if tag == *b"liga" {
            studio.patch_features(&[(tag, u32::from(on)), (*b"clig", u32::from(on))]);
        } else if tag == *b"c2sc" {
            studio.patch_features(&[(tag, u32::from(on)), (*b"smcp", u32::from(on))]);
        } else {
            studio.patch_feature(tag, u32::from(on));
        }
        restore_edit_focus(ui, studio);
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
                            serde_json::to_string(&choice.preview).unwrap_or_default(),
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
