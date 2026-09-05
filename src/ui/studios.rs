use crate::app::Studio;
use crate::color::{Blend, Rgba};
use crate::document::{Cap, Fill, Join, Stroke as DocStroke};
use crate::geom::Geom;
use crate::tools::{Persona, Tool};
use crate::ui::icons::{self, ph};
use crate::ui::theme::{accent, accent_dim, bg_widget, fg_weak};
use eframe::egui::{
    vec2, Color32, ComboBox, Layout, Panel, RichText, ScrollArea, Slider, Stroke, Ui,
};

pub fn right_panel(ui: &mut Ui, studio: &mut Studio) {
    Panel::right("studios")
        .resizable(true)
        .default_size(272.0)
        .size_range(240.0..=420.0)
        .show(ui, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                let design = studio.persona == Persona::Design;
                let motion = studio.persona == Persona::Motion;
                let pixel = studio.persona == Persona::Pixel;
                let paint = pixel
                    || matches!(
                        studio.tool,
                        Tool::Brush
                            | Tool::Eraser
                            | Tool::Fill
                            | Tool::Clone
                            | Tool::Smudge
                            | Tool::Wand
                            | Tool::Marquee
                            | Tool::EllipseMarquee
                            | Tool::Lasso
                    );
                let typing = studio.type_edit.is_some()
                    || studio.tool == Tool::Text
                    || studio.selected_type().is_some();

                color_studio(ui, studio);
                if studio.tool == Tool::Trace {
                    ui.add_space(8.0);
                    trace_studio(ui, studio);
                }
                if design {
                    ui.add_space(8.0);
                    stroke_studio(ui, studio);
                    ui.add_space(8.0);
                    transform_studio(ui, studio);
                    ui.add_space(8.0);
                    fx_studio(ui, studio);
                }
                if motion {
                    ui.add_space(8.0);
                    motion_studio(ui, studio);
                }
                if design && typing {
                    ui.add_space(8.0);
                    character_studio(ui, studio);
                }
                if paint {
                    ui.add_space(8.0);
                    brush_studio(ui, studio);
                }
                ui.add_space(8.0);
                layers_studio(ui, studio);
            });
        });
}

fn heading(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).strong().size(12.0).color(accent()));
    ui.add_space(4.0);
}

fn color_studio(ui: &mut Ui, studio: &mut Studio) {
    heading(ui, "Colour");
    ui.horizontal(|ui| {
        if fill_well(ui, studio) {
            studio.fill_active = true;
        }
        if stroke_well(ui, studio) {
            studio.fill_active = false;
        }
        ui.vertical(|ui| {
            ui.label(
                RichText::new(if studio.fill_active {
                    "Editing fill"
                } else {
                    "Editing stroke"
                })
                .small()
                .color(fg_weak()),
            );
            if ui.small_button("Swap  X").clicked() {
                studio.swap_fill_stroke();
            }
            if ui.small_button("None").clicked() {
                if studio.fill_active {
                    studio.set_fill(Fill::None);
                } else {
                    studio.style.stroke = None;
                    apply_stroke(studio, None);
                }
            }
        });
    });

    let mut c = if studio.fill_active {
        match studio.style.fill {
            Fill::Solid(c) => c.to_egui(),
            Fill::Linear { c0, .. } | Fill::Radial { c0, .. } => c0.to_egui(),
            Fill::None => studio.brush.color.to_egui(),
        }
    } else {
        studio
            .style
            .stroke
            .as_ref()
            .map(|s| s.color.to_egui())
            .unwrap_or(Color32::BLACK)
    };
    if eframe::egui::widgets::color_picker::color_picker_color32(
        ui,
        &mut c,
        eframe::egui::widgets::color_picker::Alpha::OnlyBlend,
    ) {
        let col = Rgba::from_egui(c);
        if studio.fill_active {
            studio.set_fill(Fill::Solid(col));
            studio.brush.color = col;
        } else {
            studio.set_stroke_color(col);
        }
    }

    ui.horizontal(|ui| {
        ui.label("#");
        if ui
            .add(eframe::egui::TextEdit::singleline(&mut studio.hex_buf).desired_width(80.0))
            .lost_focus()
            && let Some(col) = Rgba::parse_hex(&studio.hex_buf)
        {
            if studio.fill_active {
                studio.set_fill(Fill::Solid(col));
                studio.brush.color = col;
            } else {
                studio.set_stroke_color(col);
            }
        }
    });
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for c in studio.swatches.clone() {
            let (rect, resp) = ui.allocate_exact_size(vec2(16.0, 16.0), eframe::egui::Sense::click());
            ui.painter().rect_filled(rect, 2.0, c.to_egui());
            if resp.clicked() {
                if studio.fill_active {
                    studio.set_fill(Fill::Solid(c));
                    studio.brush.color = c;
                } else {
                    studio.set_stroke_color(c);
                }
            }
            if resp.secondary_clicked() {
                studio.set_stroke_color(c);
            }
        }
    });
    if !studio.recent.is_empty() {
        ui.label(RichText::new("Recent").small().color(fg_weak()));
        ui.horizontal_wrapped(|ui| {
            for c in studio.recent.clone() {
                let (rect, resp) =
                    ui.allocate_exact_size(vec2(16.0, 16.0), eframe::egui::Sense::click());
                ui.painter().rect_filled(rect, 2.0, c.to_egui());
                if resp.clicked() {
                    studio.set_fill(Fill::Solid(c));
                }
            }
        });
    }
    ui.horizontal(|ui| {
        let (a, b) = studio.gradient;
        swatch_btn(ui, a, "Gradient A");
        swatch_btn(ui, b, "Gradient B");
        if ui.small_button("Linear").clicked() {
            studio.set_fill(Fill::Linear {
                from: [0.0, 0.0],
                to: [1.0, 0.0],
                c0: studio.gradient.0,
                c1: studio.gradient.1,
            });
        }
        if ui.small_button("Radial").clicked() {
            studio.set_fill(Fill::Radial {
                c0: studio.gradient.0,
                c1: studio.gradient.1,
            });
        }
    });
    eframe::egui::CollapsingHeader::new("Palettes")
        .default_open(false)
        .show(ui, |ui| palette_ui(ui, studio));
}

fn palette_ui(ui: &mut Ui, studio: &mut Studio) {
    if studio.palettes.is_empty() {
        studio.palettes = crate::palette::load();
    }
    if studio.palette_idx >= studio.palettes.len() {
        studio.palette_idx = 0;
    }
    ui.horizontal(|ui| {
        if ui.small_button("New").clicked() {
            let name = if studio.palette_name_buf.trim().is_empty() {
                format!("Palette {}", studio.palettes.len() + 1)
            } else {
                studio.palette_name_buf.trim().to_string()
            };
            if crate::palette::validate_name(&name).is_ok() {
                studio.palettes.push(crate::palette::Palette::new(name.clone(), vec![]));
                studio.palette_idx = studio.palettes.len() - 1;
                studio.palette_name_buf = name;
                let _ = crate::palette::save(&studio.palettes);
            }
        }
        if ui.small_button("Delete").clicked() && studio.palettes.len() > 1 {
            studio.palettes.remove(studio.palette_idx);
            studio.palette_idx = studio.palette_idx.min(studio.palettes.len() - 1);
            studio.palette_name_buf = studio.palettes[studio.palette_idx].name.clone();
            let _ = crate::palette::save(&studio.palettes);
        }
    });
    let names: Vec<String> = studio.palettes.iter().map(|p| p.name.clone()).collect();
    let cur_name = names.get(studio.palette_idx).cloned().unwrap_or_default();
    ComboBox::from_id_salt("palette-select")
        .selected_text(cur_name)
        .width(180.0)
        .show_ui(ui, |ui| {
            for (i, n) in names.iter().enumerate() {
                if ui
                    .selectable_value(&mut studio.palette_idx, i, n)
                    .clicked()
                    && let Some(p) = studio.palettes.get(i)
                {
                    studio.palette_name_buf = p.name.clone();
                }
            }
        });
    ui.horizontal(|ui| {
        ui.add(
            eframe::egui::TextEdit::singleline(&mut studio.palette_name_buf)
                .desired_width(120.0)
                .hint_text("name"),
        );
        if ui.small_button("Rename").clicked() {
            let new_name = studio.palette_name_buf.trim().to_string();
            if crate::palette::validate_name(&new_name).is_ok() {
                if let Some(p) = studio.palettes.get_mut(studio.palette_idx) {
                    p.name = new_name;
                    let _ = crate::palette::save(&studio.palettes);
                }
            }
        }
    });
    if let Some(pal) = studio.palettes.get(studio.palette_idx).cloned() {
        ui.horizontal_wrapped(|ui| {
            for (idx, c) in pal.colors.iter().cloned().enumerate() {
                let (rect, resp) =
                    ui.allocate_exact_size(vec2(16.0, 16.0), eframe::egui::Sense::click());
                ui.painter().rect_filled(rect, 2.0, c.to_egui());
                if resp.clicked() {
                    studio.set_fill(Fill::Solid(c));
                    studio.brush.color = c;
                }
                if resp.secondary_clicked() {
                    if let Some(p) = studio.palettes.get_mut(studio.palette_idx) {
                        if idx < p.colors.len() {
                            p.colors.remove(idx);
                        }
                    }
                    let _ = crate::palette::save(&studio.palettes);
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.small_button("+ Fill").clicked() {
                let col = match studio.style.fill {
                    Fill::Solid(c) => c,
                    Fill::Linear { c0, .. } | Fill::Radial { c0, .. } => c0,
                    Fill::None => studio.brush.color,
                };
                if let Some(p) = studio.palettes.get_mut(studio.palette_idx)
                    && !p.colors.contains(&col)
                {
                    p.colors.push(col);
                    let _ = crate::palette::save(&studio.palettes);
                }
            }
            if ui.small_button("Clear").clicked() {
                if let Some(p) = studio.palettes.get_mut(studio.palette_idx) {
                    p.colors.clear();
                }
                let _ = crate::palette::save(&studio.palettes);
            }
        });
        if pal.colors.is_empty() {
            ui.label(
                RichText::new("Empty. + Fill adds the current colour. Right-click a swatch to remove.")
                    .small()
                    .color(fg_weak()),
            );
        }
    }
}

fn swatch_btn(ui: &mut Ui, c: Rgba, tip: &str) {
    let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), eframe::egui::Sense::click());
    ui.painter().rect_filled(rect, 3.0, c.to_egui());
    let _ = resp.on_hover_text(tip);
}

fn fill_well(ui: &mut Ui, studio: &mut Studio) -> bool {
    let c = match studio.style.fill {
        Fill::Solid(c) => c.to_egui(),
        Fill::Linear { c0, .. } | Fill::Radial { c0, .. } => c0.to_egui(),
        Fill::None => Color32::TRANSPARENT,
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(36.0, 36.0), eframe::egui::Sense::click());
    ui.painter().rect_filled(rect, 4.0, bg_widget());
    ui.painter().rect_filled(rect.shrink(4.0), 3.0, c);
    let stroke_c = if studio.fill_active {
        accent()
    } else {
        accent_dim()
    };
    ui.painter().rect_stroke(
        rect,
        4.0,
        Stroke::new(if studio.fill_active { 2.0 } else { 1.0 }, stroke_c),
        eframe::egui::StrokeKind::Middle,
    );
    resp.on_hover_text("Fill").clicked()
}

fn stroke_well(ui: &mut Ui, studio: &mut Studio) -> bool {
    let c = studio
        .style
        .stroke
        .as_ref()
        .map(|s| s.color.to_egui())
        .unwrap_or(Color32::TRANSPARENT);
    let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), eframe::egui::Sense::click());
    ui.painter().rect_stroke(
        rect.shrink(3.0),
        3.0,
        Stroke::new(3.0, c),
        eframe::egui::StrokeKind::Middle,
    );
    if !studio.fill_active {
        ui.painter().rect_stroke(
            rect,
            4.0,
            Stroke::new(2.0, accent()),
            eframe::egui::StrokeKind::Middle,
        );
    }
    resp.on_hover_text("Stroke").clicked()
}

fn stroke_studio(ui: &mut Ui, studio: &mut Studio) {
    heading(ui, "Stroke");
    let mut width = studio.style.stroke.as_ref().map(|s| s.width).unwrap_or(0.0);
    if ui
        .add(Slider::new(&mut width, 0.0..=64.0).text("Width"))
        .changed()
    {
        let mut st = studio.style.stroke.clone().unwrap_or_default();
        st.width = width;
        studio.style.stroke = if width <= 0.01 { None } else { Some(st.clone()) };
        apply_stroke(studio, studio.style.stroke.clone());
    }
    ui.horizontal(|ui| {
        for (cap, label) in [
            (Cap::Butt, "Butt"),
            (Cap::Round, "Round"),
            (Cap::Square, "Square"),
        ] {
            if ui
                .selectable_label(
                    studio.style.stroke.as_ref().map(|s| s.cap) == Some(cap),
                    label,
                )
                .clicked()
            {
                if let Some(st) = &mut studio.style.stroke {
                    st.cap = cap;
                }
                apply_stroke(studio, studio.style.stroke.clone());
            }
        }
    });
    ui.horizontal(|ui| {
        for (join, label) in [
            (Join::Miter, "Miter"),
            (Join::Round, "Round"),
            (Join::Bevel, "Bevel"),
        ] {
            if ui
                .selectable_label(
                    studio.style.stroke.as_ref().map(|s| s.join) == Some(join),
                    label,
                )
                .clicked()
            {
                if let Some(st) = &mut studio.style.stroke {
                    st.join = join;
                }
                apply_stroke(studio, studio.style.stroke.clone());
            }
        }
    });
    let dashed = studio.style.stroke.as_ref().and_then(|s| s.dash).is_some();
    let mut d = dashed;
    if ui.checkbox(&mut d, "Dashed").changed() {
        if let Some(st) = &mut studio.style.stroke {
            st.dash = if d { Some((6.0, 4.0)) } else { None };
        }
        apply_stroke(studio, studio.style.stroke.clone());
    }
}

fn apply_stroke(studio: &mut Studio, stroke: Option<DocStroke>) {
    for (li, id) in studio.selection.clone() {
        if let Some(s) = studio.doc.find_shape(li, id) {
            let mut after = s.style.clone();
            after.stroke = stroke.clone();
            studio.commit(crate::document::Cmd::SetStyle {
                layer: li,
                id,
                before: s.style.clone(),
                after,
            });
        }
    }
}

fn character_studio(ui: &mut Ui, studio: &mut Studio) {
    heading(ui, "Character");
    let live = studio.selected_type();
    let font = live
        .as_ref()
        .map(|t| t.font.clone())
        .unwrap_or_else(|| studio.text_font.clone());
    let label = crate::text::label_for(&font);
    ui.label(RichText::new(&label).small().strong());
    ui.add(
        eframe::egui::TextEdit::singleline(&mut studio.font_query)
            .hint_text("Search fonts")
            .desired_width(220.0),
    );
    let q = studio.font_query.to_ascii_lowercase();
    let recents = studio.font_recents.clone();
    let used = studio.used_fonts();
    let all = crate::text::all_fonts();
    let mut chosen = font.clone();
    ScrollArea::vertical()
        .id_salt("font-list")
        .max_height(180.0)
        .show(ui, |ui| {
            if !recents.is_empty() {
                ui.label(RichText::new("Recents").small().color(fg_weak()));
                for p in recents.iter().take(5) {
                    let name = crate::text::label_for(p);
                    if ui.selectable_label(*p == font, &name).clicked() {
                        chosen = p.clone();
                    }
                }
            }
            if !used.is_empty() {
                ui.label(RichText::new("Used in document").small().color(fg_weak()));
                for p in &used {
                    let name = crate::text::label_for(p);
                    if ui.selectable_label(*p == font, &name).clicked() {
                        chosen = p.clone();
                    }
                }
            }
            ui.label(RichText::new("All").small().color(fg_weak()));
            for f in &all {
                if !q.is_empty() && !f.name.to_ascii_lowercase().contains(&q) {
                    continue;
                }
                let path = f.path.to_string_lossy().to_string();
                let resp = ui.selectable_label(path == font, &f.name);
                if path == font && studio.font_scroll_once {
                    resp.scroll_to_me(Some(eframe::egui::Align::Center));
                    studio.font_scroll_once = false;
                }
                if resp.clicked() {
                    chosen = path;
                }
            }
        });
    if chosen != font {
        let path = chosen.clone();
        studio.remember_font(&path);
        studio.patch_type(|t| t.font = path.clone());
        studio.text_font = path;
        studio.font_scroll_once = true;
    }
    eframe::egui::CollapsingHeader::new("Google Fonts")
        .default_open(false)
        .show(ui, |ui| google_fonts_ui(ui, studio));

    let mut px = live.as_ref().map(|t| t.px).unwrap_or(studio.text_px);
    if ui
        .add(Slider::new(&mut px, 8.0..=400.0).text("Size"))
        .changed()
    {
        studio.patch_type(|t| t.px = px);
    }
    let mut track = live
        .as_ref()
        .map(|t| t.tracking)
        .unwrap_or(studio.text_tracking);
    if ui
        .add(Slider::new(&mut track, -40.0..=80.0).text("Tracking"))
        .changed()
    {
        studio.patch_type(|t| t.tracking = track);
    }
    let mut lead = live
        .as_ref()
        .map(|t| t.leading)
        .unwrap_or(studio.text_leading);
    if ui
        .add(Slider::new(&mut lead, 0.0..=400.0).text("Leading (0 auto)"))
        .changed()
    {
        studio.patch_type(|t| t.leading = lead);
    }
    ui.add_space(4.0);
    ui.label(RichText::new("OpenType").small().color(fg_weak()));
    let mut kern = live.as_ref().map(|t| t.kern).unwrap_or(studio.text_kern);
    let mut liga = live.as_ref().map(|t| t.liga).unwrap_or(studio.text_liga);
    let mut tnum = live.as_ref().map(|t| t.tnum).unwrap_or(studio.text_tnum);
    let mut smcp = live.as_ref().map(|t| t.smcp).unwrap_or(studio.text_smcp);
    ui.horizontal(|ui| {
        if ui.checkbox(&mut kern, "Kerning").changed() {
            studio.patch_type(|t| t.kern = kern);
        }
        if ui.checkbox(&mut liga, "Ligatures").changed() {
            studio.patch_type(|t| t.liga = liga);
        }
    });
    ui.horizontal(|ui| {
        if ui.checkbox(&mut tnum, "Tabular figs").changed() {
            studio.patch_type(|t| t.tnum = tnum);
        }
        if ui.checkbox(&mut smcp, "Small caps").changed() {
            studio.patch_type(|t| t.smcp = smcp);
        }
    });
    if studio.type_edit.is_some() {
        ui.label(
            RichText::new("Typing on the canvas. Esc finishes.")
                .small()
                .color(accent()),
        );
    } else if live.is_none() {
        ui.label(
            RichText::new("Applies to the next type you place.")
                .small()
                .color(fg_weak()),
        );
    }
}

fn google_fonts_ui(ui: &mut Ui, studio: &mut Studio) {
    if !studio.google_catalog_loaded {
        studio.google_catalog = crate::google_fonts::catalog();
        studio.google_catalog_loaded = true;
        studio.google_status = if studio.google_catalog.is_empty() {
            "offline – bundled list".into()
        } else {
            format!("{} families", studio.google_catalog.len())
        };
    }
    ui.horizontal(|ui| {
        ui.add(
            eframe::egui::TextEdit::singleline(&mut studio.google_query)
                .hint_text("Search Inter, mono…")
                .desired_width(140.0),
        );
        if ui.small_button("Refresh").clicked() {
            studio.google_catalog = crate::google_fonts::catalog();
            studio.google_status = format!("{} families", studio.google_catalog.len());
        }
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("Variant").small().color(fg_weak()));
        ComboBox::from_id_salt("google-variant")
            .selected_text(studio.google_variant.clone())
            .width(100.0)
            .show_ui(ui, |ui| {
                for v in ["regular", "italic", "700", "700italic"] {
                    ui.selectable_value(&mut studio.google_variant, v.to_string(), v);
                }
            });
    });
    let filtered: Vec<crate::google_fonts::GoogleFont> =
        crate::google_fonts::search(&studio.google_catalog, &studio.google_query)
            .into_iter()
            .cloned()
            .collect();
    ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
        for f in filtered {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&f.family).small());
                ui.label(RichText::new(&f.category).small().color(fg_weak()));
                let installed =
                    crate::google_fonts::is_installed(&f.family, &studio.google_variant);
                if installed {
                    if ui.small_button("Use").clicked() {
                        let found = crate::text::all_fonts()
                            .iter()
                            .find(|ff| {
                                ff.name
                                    .to_ascii_lowercase()
                                    .contains(&f.family.to_ascii_lowercase())
                            })
                            .map(|ff| ff.path.to_string_lossy().to_string());
                        let chosen = found.unwrap_or_else(|| {
                            crate::google_fonts::installed_path(&f.family, &studio.google_variant)
                                .to_string_lossy()
                                .to_string()
                        });
                        studio.patch_type(|t| t.font = chosen);
                        studio.google_status = format!("using {}", f.family);
                    }
                } else if ui.small_button("Download").clicked() {
                    studio.google_status =
                        format!("Downloading {} {}…", f.family, studio.google_variant);
                    let fam = f.family.clone();
                    let var = studio.google_variant.clone();
                    let cat = studio.google_catalog.clone();
                    match crate::google_fonts::download(&fam, &var, &cat) {
                        Ok(p) => {
                            let chosen = p.to_string_lossy().to_string();
                            studio.patch_type(|t| t.font = chosen);
                            studio.google_status = format!("Installed {}", fam);
                        }
                        Err(e) => studio.google_status = e,
                    }
                }
            });
        }
    });
    if !studio.google_status.is_empty() {
        ui.label(RichText::new(&studio.google_status).small().color(accent()));
    }
}

fn motion_studio(ui: &mut Ui, studio: &mut Studio) {
    heading(ui, "Motion");
    ui.label(
        RichText::new("Rest pose is Design. Keys are offsets.")
            .small()
            .color(fg_weak()),
    );
    ui.add_space(4.0);
    if studio.selection.is_empty() {
        ui.label(RichText::new("Select a shape, then Key.").small().color(fg_weak()));
        return;
    }
    let Some((_, id)) = studio.primary() else {
        return;
    };
    let pose = studio.live_pose(id);
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("X  {:+.1}", pose.dx)).small().monospace());
        if ui.small_button("Key").clicked() {
            studio.key_prop(id, crate::motion::Prop::X, pose.dx);
        }
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("Y  {:+.1}", pose.dy)).small().monospace());
        if ui.small_button("Key").clicked() {
            studio.key_prop(id, crate::motion::Prop::Y, pose.dy);
        }
    });
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("R  {:+.1}°", pose.rotation.to_degrees()))
                .small()
                .monospace(),
        );
        if ui.small_button("Key").clicked() {
            studio.key_prop(id, crate::motion::Prop::Rotation, pose.rotation);
        }
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("S  {:.2}", pose.scale)).small().monospace());
        if ui.small_button("Key").clicked() {
            studio.key_prop(id, crate::motion::Prop::Scale, pose.scale);
        }
    });
    let mut op = pose.opacity.unwrap_or_else(|| {
        studio
            .doc
            .find_shape(studio.primary().map(|p| p.0).unwrap_or(0), id)
            .map(|s| s.opacity)
            .unwrap_or(1.0)
    });
    ui.horizontal(|ui| {
        if ui.add(Slider::new(&mut op, 0.0..=1.0).text("Opacity")).changed() {
            studio.key_prop(id, crate::motion::Prop::Opacity, op);
        }
    });
}

fn artboard_transform(ui: &mut Ui, studio: &mut Studio) {
    let Some(id) = studio.artboard_sel.first().copied() else {
        ui.label(RichText::new("Draw or click an artboard").small().color(fg_weak()));
        if ui.button("Wrap selection in artboard").clicked() {
            studio.wrap_selection_artboard();
        }
        return;
    };
    let Some(idx) = studio.doc.artboards.iter().position(|a| a.id == id) else {
        return;
    };
    let mut a = studio.doc.artboards[idx].clone();
    if studio.artboard_rename.as_ref().map(|(i, _)| *i) == Some(id) {
        if let Some((_, buf)) = studio.artboard_rename.as_mut() {
            let r = ui.add(eframe::egui::TextEdit::singleline(buf).desired_width(160.0));
            if r.lost_focus() {
                let name = buf.trim().to_string();
                studio.artboard_rename = None;
                if !name.is_empty() {
                    a.name = studio.doc.unique_artboard_name(&name);
                    let mut after = studio.doc.artboards.clone();
                    if let Some(slot) = after.iter_mut().find(|x| x.id == id) {
                        *slot = a.clone();
                    }
                    studio.commit_artboards(after);
                }
            }
        }
    } else if ui.button(&a.name).on_hover_text("Rename").clicked() {
        studio.artboard_rename = Some((id, a.name.clone()));
    }
    let mut x = a.origin.x;
    let mut y = a.origin.y;
    let mut w = a.size.x;
    let mut h = a.size.y;
    let mut deg = a.rotation.to_degrees();
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= ui.add(eframe::egui::DragValue::new(&mut x).speed(1.0).prefix("X  ")).changed();
        changed |= ui.add(eframe::egui::DragValue::new(&mut y).speed(1.0).prefix("Y  ")).changed();
    });
    ui.horizontal(|ui| {
        changed |= ui
            .add(eframe::egui::DragValue::new(&mut w).speed(1.0).range(8.0..=20000.0).prefix("W  "))
            .changed();
        changed |= ui
            .add(eframe::egui::DragValue::new(&mut h).speed(1.0).range(8.0..=20000.0).prefix("H  "))
            .changed();
    });
    changed |= ui.add(Slider::new(&mut deg, -180.0..=180.0).text("Rotate°")).changed();
    if changed {
        let orig = a.clone();
        a.origin = crate::geom::Pt::new(x, y);
        a.size = crate::geom::Pt::new(w, h);
        a.rotation = crate::document::Artboard::snap_rotation(deg.to_radians());
        let snaps = studio.snapshot_artboard_contents(&orig);
        let mut after = studio.doc.artboards.clone();
        if let Some(slot) = after.iter_mut().find(|x| x.id == id) {
            *slot = a.clone();
        }
        studio.commit_artboards(after);
        studio.apply_artboard_contents(&orig, &a, &snaps);
        studio.commit_mapped_contents(snaps);
    }
    ui.horizontal(|ui| {
        if ui.button("Clone").clicked() {
            studio.clone_artboard(id);
        }
        if ui.button("Delete").clicked() {
            studio.delete_artboards();
        }
    });
}

fn raster_transform(ui: &mut Ui, studio: &mut Studio, li: usize) {
    let Some(layer) = studio.doc.layers.get(li) else {
        return;
    };
    let Some((origin, size, rot)) = layer.kind.raster_xform() else {
        return;
    };
    ui.label(RichText::new(&layer.name).small().color(fg_weak()));
    let mut x = origin.x;
    let mut y = origin.y;
    let mut w = size.x;
    let mut h = size.y;
    let mut deg = rot.to_degrees();
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= ui.add(eframe::egui::DragValue::new(&mut x).speed(1.0).prefix("X  ")).changed();
        changed |= ui.add(eframe::egui::DragValue::new(&mut y).speed(1.0).prefix("Y  ")).changed();
    });
    ui.horizontal(|ui| {
        changed |= ui
            .add(eframe::egui::DragValue::new(&mut w).speed(1.0).range(1.0..=20000.0).prefix("W  "))
            .changed();
        changed |= ui
            .add(eframe::egui::DragValue::new(&mut h).speed(1.0).range(1.0..=20000.0).prefix("H  "))
            .changed();
    });
    changed |= ui.add(Slider::new(&mut deg, -180.0..=180.0).text("Rotate°")).changed();
    if changed {
        studio.commit(crate::document::Cmd::SetRasterXform {
            layer: li,
            before: (origin, size, rot),
            after: (
                crate::geom::Pt::new(x, y),
                crate::geom::Pt::new(w, h),
                deg.to_radians(),
            ),
        });
    }
}

fn transform_studio(ui: &mut Ui, studio: &mut Studio) {
    heading(ui, "Transform");
    if studio.tool == crate::tools::Tool::Artboard
        || (studio.selection.is_empty() && !studio.artboard_sel.is_empty())
    {
        artboard_transform(ui, studio);
        return;
    }
    if studio.selection.is_empty() {
        ui.label(RichText::new("Nothing selected").small().color(fg_weak()));
        ui.add(Slider::new(&mut studio.polygon_sides, 3..=12).text("Sides"));
        ui.add(Slider::new(&mut studio.star_points, 3..=12).text("Star points"));
        ui.add(Slider::new(&mut studio.star_inner, 0.15..=0.8).text("Star inner"));
        ui.add(Slider::new(&mut studio.rect_radius, 0.0..=80.0).text("Corner radius"));
        return;
    }
    let Some((li, id)) = studio.primary() else {
        return;
    };
    if id == crate::document::RASTER_ID {
        raster_transform(ui, studio, li);
        return;
    }
    let Some(shape) = studio.doc.find_shape(li, id).cloned() else {
        return;
    };
    let b = shape.world_bbox();
    ui.label(
        RichText::new(format!(
            "{}  {:.0} × {:.0}",
            shape.name,
            b.width(),
            b.height()
        ))
        .small()
        .color(fg_weak()),
    );
    {
        let mut x = b.min.x;
        let mut y = b.min.y;
        let mut w = b.width().max(1.0);
        let mut h = b.height().max(1.0);
        let mut changed = false;
        ui.horizontal(|ui| {
            changed |= ui
                .add(eframe::egui::DragValue::new(&mut x).speed(1.0).prefix("X  "))
                .changed();
            changed |= ui
                .add(eframe::egui::DragValue::new(&mut y).speed(1.0).prefix("Y  "))
                .changed();
        });
        ui.horizontal(|ui| {
            changed |= ui
                .add(
                    eframe::egui::DragValue::new(&mut w)
                        .speed(1.0)
                        .range(1.0..=10000.0)
                        .prefix("W  "),
                )
                .changed();
            changed |= ui
                .add(
                    eframe::egui::DragValue::new(&mut h)
                        .speed(1.0)
                        .range(1.0..=10000.0)
                        .prefix("H  "),
                )
                .changed();
        });
        if changed {
            let dst = crate::geom::Bounds {
                min: crate::geom::Pt::new(x, y),
                max: crate::geom::Pt::new(x + w, y + h),
            };
            let mut g = shape.geom.clone();
            g.map_into(b, dst);
            studio.commit(crate::document::Cmd::SetGeom {
                layer: li,
                id,
                before: shape.geom.clone(),
                after: g,
                rot_before: shape.rotation,
                rot_after: shape.rotation,
            });
        }
        let mut deg = shape.rotation.to_degrees();
        if ui
            .add(Slider::new(&mut deg, -180.0..=180.0).text("Rotate°"))
            .changed()
        {
            studio.commit(crate::document::Cmd::SetGeom {
                layer: li,
                id,
                before: shape.geom.clone(),
                after: shape.geom.clone(),
                rot_before: shape.rotation,
                rot_after: deg.to_radians(),
            });
        }
        ui.horizontal(|ui| {
            if ui.small_button("Flip H").clicked() {
                studio.flip_selection(true);
            }
            if ui.small_button("Flip V").clicked() {
                studio.flip_selection(false);
            }
        });
    }
    let mut op = shape.opacity;
    if ui
        .add(Slider::new(&mut op, 0.0..=1.0).text("Opacity"))
        .changed()
    {
        studio.commit(crate::document::Cmd::SetOpacity {
            layer: li,
            id,
            before: shape.opacity,
            after: op,
        });
    }
    match &shape.geom {
        Geom::Polygon { sides, .. } => {
            let mut n = *sides;
            if ui.add(Slider::new(&mut n, 3..=16).text("Sides")).changed() {
                let mut g = shape.geom.clone();
                if let Geom::Polygon { sides, .. } = &mut g {
                    *sides = n;
                }
                studio.commit(crate::document::Cmd::SetGeom {
                    layer: li,
                    id,
                    before: shape.geom.clone(),
                    after: g,
                    rot_before: shape.rotation,
                    rot_after: shape.rotation,
                });
            }
        }
        Geom::Star { points, inner, .. } => {
            let mut n = *points;
            let mut inn = *inner;
            if ui.add(Slider::new(&mut n, 3..=16).text("Points")).changed()
                || ui
                    .add(Slider::new(&mut inn, 0.15..=0.85).text("Inner"))
                    .changed()
            {
                let mut g = shape.geom.clone();
                if let Geom::Star { points, inner, .. } = &mut g {
                    *points = n;
                    *inner = inn;
                }
                studio.commit(crate::document::Cmd::SetGeom {
                    layer: li,
                    id,
                    before: shape.geom.clone(),
                    after: g,
                    rot_before: shape.rotation,
                    rot_after: shape.rotation,
                });
            }
        }
        Geom::Rect { radius, .. } => {
            let mut r = *radius;
            if ui
                .add(Slider::new(&mut r, 0.0..=200.0).text("Radius"))
                .changed()
            {
                let mut g = shape.geom.clone();
                if let Geom::Rect { radius, .. } = &mut g {
                    *radius = r;
                }
                studio.commit(crate::document::Cmd::SetGeom {
                    layer: li,
                    id,
                    before: shape.geom.clone(),
                    after: g,
                    rot_before: shape.rotation,
                    rot_after: shape.rotation,
                });
            }
        }
        _ => {}
    }
    ui.horizontal(|ui| {
        if icons::tiny_icon(ui, ph::ALIGN_LEFT, "Align left", false) {
            studio.align_sel(crate::align::Align::Left);
        }
        if icons::tiny_icon(ui, ph::ALIGN_CENTER_H, "Align centre", false) {
            studio.align_sel(crate::align::Align::CenterX);
        }
        if icons::tiny_icon(ui, ph::ALIGN_RIGHT, "Align right", false) {
            studio.align_sel(crate::align::Align::Right);
        }
        if icons::tiny_icon(ui, ph::ALIGN_TOP, "Align top", false) {
            studio.align_sel(crate::align::Align::Top);
        }
        if icons::tiny_icon(ui, ph::ALIGN_CENTER_V, "Align middle", false) {
            studio.align_sel(crate::align::Align::CenterY);
        }
        if icons::tiny_icon(ui, ph::ALIGN_BOTTOM, "Align bottom", false) {
            studio.align_sel(crate::align::Align::Bottom);
        }
    });
    ui.add_space(4.0);
    let n = studio.selection.len();
    let is_compound =
        matches!(&shape.geom, crate::geom::Geom::Poly { contours, .. } if contours.len() > 1);
    ui.horizontal_wrapped(|ui| {
        let can = n >= 2;
        if ui
            .add_enabled(can, eframe::egui::Button::new("Union"))
            .clicked()
        {
            studio.apply_boolean_multi(crate::boolean::BoolOp::Union);
        }
        if ui
            .add_enabled(can, eframe::egui::Button::new("Subtract"))
            .clicked()
        {
            studio.apply_boolean_multi(crate::boolean::BoolOp::Subtract);
        }
        if ui
            .add_enabled(can, eframe::egui::Button::new("Intersect"))
            .clicked()
        {
            studio.apply_boolean_multi(crate::boolean::BoolOp::Intersect);
        }
        if ui
            .add_enabled(can, eframe::egui::Button::new("Xor"))
            .clicked()
        {
            studio.apply_boolean_multi(crate::boolean::BoolOp::Xor);
        }
    });
    ui.horizontal(|ui| {
        if ui
            .add_enabled(n >= 2, eframe::egui::Button::new("Combine"))
            .on_hover_text("Ctrl+G")
            .clicked()
        {
            studio.combine_selected();
        }
        if ui
            .add_enabled(is_compound, eframe::egui::Button::new("Release"))
            .on_hover_text("Ctrl+Shift+G")
            .clicked()
        {
            studio.release_compound();
        }
    });
}

fn fx_stack_editor(ui: &mut Ui, stack: &mut crate::filter::FilterStack, salt: &str) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut stack.enabled, "Enabled");
        ComboBox::from_id_salt(format!("fx-add-{salt}"))
            .selected_text("Add")
            .width(72.0)
            .show_ui(ui, |ui| {
                for (name, make) in crate::filter::Fx::catalog() {
                    if ui.selectable_label(false, *name).clicked() {
                        stack.items.push(make());
                    }
                }
            });
    });
    let mut remove = None;
    let mut bump = None;
    for (i, fx) in stack.items.iter_mut().enumerate() {
        ui.add_space(4.0);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(fx.name()).small().strong().color(accent()));
                ui.with_layout(Layout::right_to_left(eframe::egui::Align::Center), |ui| {
                    if ui.small_button("−").clicked() {
                        remove = Some(i);
                    }
                    if ui.small_button("↓").clicked() {
                        bump = Some((i, 1));
                    }
                    if ui.small_button("↑").clicked() {
                        bump = Some((i, -1));
                    }
                });
            });
            match fx {
                crate::filter::Fx::Blur { std } => {
                    ui.add(Slider::new(std, 0.0..=80.0).text("stdDeviation"));
                }
                crate::filter::Fx::Shadow {
                    dx,
                    dy,
                    blur,
                    color,
                }
                | crate::filter::Fx::InnerShadow {
                    dx,
                    dy,
                    blur,
                    color,
                } => {
                    ui.add(Slider::new(dx, -80.0..=80.0).text("dx"));
                    ui.add(Slider::new(dy, -80.0..=80.0).text("dy"));
                    ui.add(Slider::new(blur, 0.0..=80.0).text("stdDeviation"));
                    ui.horizontal(|ui| {
                        let mut rgb = [color.r, color.g, color.b];
                        if ui.color_edit_button_srgb(&mut rgb).changed() {
                            color.r = rgb[0];
                            color.g = rgb[1];
                            color.b = rgb[2];
                        }
                    });
                }
                crate::filter::Fx::Offset { dx, dy } => {
                    ui.add(Slider::new(dx, -200.0..=200.0).text("dx"));
                    ui.add(Slider::new(dy, -200.0..=200.0).text("dy"));
                }
                crate::filter::Fx::Morphology { erode, radius } => {
                    ui.checkbox(erode, "Erode");
                    ui.add(Slider::new(radius, 0.0..=40.0).text("radius"));
                }
                crate::filter::Fx::Saturate { amount } => {
                    ui.add(Slider::new(amount, 0.0..=3.0).text("values"));
                }
                crate::filter::Fx::HueRotate { degrees } => {
                    ui.add(Slider::new(degrees, -180.0..=180.0).text("degrees"));
                }
                crate::filter::Fx::Brightness { amount } => {
                    ui.add(Slider::new(amount, 0.0..=3.0).text("slope"));
                }
                crate::filter::Fx::Contrast { amount } => {
                    ui.add(Slider::new(amount, 0.0..=3.0).text("slope"));
                }
                crate::filter::Fx::Invert { amount } => {
                    ui.add(Slider::new(amount, 0.0..=1.0).text("amount"));
                }
                crate::filter::Fx::ColorMatrix { values } => {
                    ui.add(Slider::new(&mut values[0], -2.0..=2.0).text("m00"));
                }
                crate::filter::Fx::Turbulence {
                    fractal,
                    base,
                    octaves,
                    seed,
                } => {
                    ui.checkbox(fractal, "fractalNoise");
                    ui.add(Slider::new(base, 0.001..=0.5).text("baseFrequency"));
                    ui.add(Slider::new(octaves, 1..=8).text("numOctaves"));
                    ui.add(Slider::new(seed, 0..=9999).text("seed"));
                }
                crate::filter::Fx::Displacement { scale, x_ch, y_ch } => {
                    ui.add(Slider::new(scale, 0.0..=120.0).text("scale"));
                    ui.add(Slider::new(x_ch, 0..=3).text("xChannel"));
                    ui.add(Slider::new(y_ch, 0..=3).text("yChannel"));
                }
            }
        });
    }
    if let Some(i) = remove {
        stack.items.remove(i);
    }
    if let Some((i, dir)) = bump {
        let j = i as i32 + dir;
        if j >= 0 && (j as usize) < stack.items.len() {
            stack.items.swap(i, j as usize);
        }
    }
}

fn fx_studio(ui: &mut Ui, studio: &mut Studio) {
    heading(ui, "FX");
    let shape_target = studio.primary().and_then(|(li, id)| {
        if id == crate::document::RASTER_ID {
            None
        } else {
            studio.doc.find_shape(li, id).map(|_| (li, id))
        }
    });
    if let Some((li, id)) = shape_target {
        ui.label(
            RichText::new("Object")
                .small()
                .color(fg_weak()),
        );
        let mut stack = studio
            .doc
            .find_shape(li, id)
            .map(|s| s.filters.clone())
            .unwrap_or_default();
        fx_stack_editor(ui, &mut stack, &format!("obj-{id}"));
        studio.commit_shape_filters(li, id, stack);
        ui.add_space(6.0);
        ui.separator();
        ui.label(RichText::new("Layer").small().color(fg_weak()));
    }
    let Some(li) = studio.active_layer else {
        ui.label(RichText::new("Select a layer").small().color(fg_weak()));
        return;
    };
    if li >= studio.doc.layers.len() {
        return;
    }
    let mut stack = studio.doc.layers[li].filters.clone();
    fx_stack_editor(ui, &mut stack, "layer");
    if stack != studio.doc.layers[li].filters {
        studio.commit_filters(li, stack.clone());
    }
}

fn trace_studio(ui: &mut Ui, studio: &mut Studio) {
    heading(ui, "Trace");
    ui.label(
        RichText::new("Raster to vector on the active pixel layer.")
            .small()
            .color(fg_weak()),
    );
    ui.add(Slider::new(&mut studio.trace_opts.colors, 1..=12).text("Colours"));
    if studio.trace_opts.colors <= 1 {
        ui.add(Slider::new(&mut studio.trace_opts.threshold, 0.05..=0.95).text("Threshold"));
    }
    ui.add(Slider::new(&mut studio.trace_opts.smoothness, 0.2..=8.0).text("Smoothness"));
    ui.add(Slider::new(&mut studio.trace_opts.min_area, 1.0..=64.0).text("Min area"));
    ui.checkbox(&mut studio.trace_opts.ignore_white, "Ignore white");
    ui.add_space(4.0);
    if ui.button("Trace to vector").clicked() {
        studio.trace_active_raster();
    }
}

fn brush_studio(ui: &mut Ui, studio: &mut Studio) {
    heading(ui, "Brush");
    ui.add(Slider::new(&mut studio.brush.size, 1.0..=256.0).text("Size"));
    ui.add(Slider::new(&mut studio.brush.hardness, 0.0..=1.0).text("Hardness"));
    ui.add(Slider::new(&mut studio.brush.opacity, 0.05..=1.0).text("Opacity"));
    ui.add(Slider::new(&mut studio.brush.flow, 0.05..=1.0).text("Flow"));
    ui.add(Slider::new(&mut studio.fill_tolerance, 0.0..=180.0).text("Fill / wand tolerance"));
    if studio.clone_source.is_some() {
        ui.label(RichText::new("Clone source set").small().color(accent()));
    } else {
        ui.label(
            RichText::new("Alt-click sets clone source")
                .small()
                .color(fg_weak()),
        );
    }
}

fn layers_studio(ui: &mut Ui, studio: &mut Studio) {
    ui.horizontal(|ui| {
        heading(ui, "Layers");
        ui.with_layout(Layout::right_to_left(eframe::egui::Align::Center), |ui| {
            if icons::tiny_icon(ui, ph::MINUS, "Delete layer", false) {
                studio.delete_layer();
            }
            if icons::tiny_icon(ui, ph::STACK, "New pixel layer", false) {
                studio.add_layer(true);
            }
            if icons::tiny_icon(ui, ph::PLUS, "New vector layer", false) {
                studio.add_layer(false);
            }
        });
    });

    let mut activate = None;
    let mut vis = None;
    let mut lock = None;
    let mut up = None;
    let mut down = None;
    let mut start_rename = None;
    let mut toggle_expand = None;
    let mut pick_shape: Option<(usize, u64)> = None;
    let mut vis_shape: Option<(usize, u64)> = None;
    let mut lock_shape: Option<(usize, u64)> = None;
    let mut start_shape_rename: Option<(usize, u64)> = None;
    let mut shape_up: Option<(usize, usize)> = None;
    let mut shape_down: Option<(usize, usize)> = None;
    let n = studio.doc.layers.len();
    for i in (0..n).rev() {
        ui.push_id(studio.doc.layers[i].id, |ui| {
            ui.horizontal(|ui| {
                let layer = &studio.doc.layers[i];
                let on = studio.active_layer == Some(i);
                let expanded = studio.layer_expanded.contains(&layer.id);
                let has_kids = layer.kind.shapes().map(|s| !s.is_empty()).unwrap_or(false)
                    || layer.kind.is_placed_raster();
                if has_kids {
                    if icons::tiny_icon(
                        ui,
                        if expanded {
                            ph::CARET_DOWN
                        } else {
                            ph::CARET_RIGHT
                        },
                        "Expand objects",
                        expanded,
                    ) {
                        toggle_expand = Some(layer.id);
                    }
                } else {
                    ui.add_space(22.0);
                }
                if icons::tiny_icon(
                    ui,
                    if layer.visible {
                        ph::EYE
                    } else {
                        ph::EYE_SLASH
                    },
                    "Visibility",
                    !layer.visible,
                ) {
                    vis = Some(i);
                }
                if icons::tiny_icon(
                    ui,
                    if layer.locked {
                        ph::LOCK
                    } else {
                        ph::LOCK_OPEN
                    },
                    "Lock",
                    layer.locked,
                ) {
                    lock = Some(i);
                }
                let tag = layer.kind.tag();
                ui.label(RichText::new(tag).small().color(fg_weak()).monospace());
                if studio.layer_rename.as_ref().map(|(idx, _)| *idx) == Some(i) {
                    if let Some((_, buf)) = studio.layer_rename.as_mut() {
                        let r = ui.add(
                            eframe::egui::TextEdit::singleline(buf)
                                .desired_width(100.0)
                                .font(eframe::egui::TextStyle::Small),
                        );
                        if r.lost_focus() {
                            start_rename = Some(usize::MAX); // sentinel: commit
                        }
                    }
                } else {
                    let fill = if on {
                        accent_dim()
                    } else {
                        Color32::TRANSPARENT
                    };
                    let resp = ui.add(
                        eframe::egui::Button::new(RichText::new(&layer.name).size(12.0)).fill(fill),
                    );
                    if resp.clicked() {
                        activate = Some(i);
                    }
                    if resp.double_clicked() {
                        start_rename = Some(i);
                    }
                }
                if icons::tiny_icon(ui, ph::CARET_UP, "Move up", false) {
                    up = Some(i);
                }
                if icons::tiny_icon(ui, ph::CARET_DOWN, "Move down", false) {
                    down = Some(i);
                }
            });
            if studio.layer_expanded.contains(&studio.doc.layers[i].id) {
                if studio.doc.layers[i].kind.is_placed_raster() {
                    ui.horizontal(|ui| {
                        ui.add_space(18.0);
                        let on = studio.selection.contains(&(i, crate::document::RASTER_ID));
                        let fill = if on {
                            accent_dim()
                        } else {
                            Color32::TRANSPARENT
                        };
                        if ui
                            .add(
                                eframe::egui::Button::new(
                                    RichText::new(&studio.doc.layers[i].name).size(11.0),
                                )
                                .fill(fill),
                            )
                            .clicked()
                        {
                            pick_shape = Some((i, crate::document::RASTER_ID));
                        }
                    });
                }
                if let Some(shapes) = studio.doc.layers[i].kind.shapes() {
                    for (si, sh) in shapes.iter().enumerate().rev() {
                        ui.horizontal(|ui| {
                            ui.add_space(12.0);
                            if icons::tiny_icon(
                                ui,
                                if sh.visible { ph::EYE } else { ph::EYE_SLASH },
                                "Object visibility",
                                !sh.visible,
                            ) {
                                vis_shape = Some((i, sh.id));
                            }
                            if icons::tiny_icon(
                                ui,
                                if sh.locked { ph::LOCK } else { ph::LOCK_OPEN },
                                "Object lock",
                                sh.locked,
                            ) {
                                lock_shape = Some((i, sh.id));
                            }
                            let on = studio.selection.contains(&(i, sh.id));
                            let fill = if on {
                                accent_dim()
                            } else {
                                Color32::TRANSPARENT
                            };
                            if studio.shape_rename.as_ref().map(|(li, id, _)| (*li, *id))
                                == Some((i, sh.id))
                            {
                                if let Some((_, _, buf)) = studio.shape_rename.as_mut() {
                                    let r = ui.add(
                                        eframe::egui::TextEdit::singleline(buf)
                                            .desired_width(90.0)
                                            .font(eframe::egui::TextStyle::Small),
                                    );
                                    if r.lost_focus() {
                                        start_shape_rename = Some((usize::MAX, 0));
                                    }
                                }
                            } else {
                                let resp = ui.add(
                                    eframe::egui::Button::new(RichText::new(&sh.name).size(11.0))
                                        .fill(fill),
                                );
                                if resp.clicked() {
                                    pick_shape = Some((i, sh.id));
                                }
                                if resp.double_clicked() {
                                    start_shape_rename = Some((i, sh.id));
                                }
                            }
                            if icons::tiny_icon(ui, ph::CARET_UP, "Object up", false) {
                                shape_up = Some((i, si));
                            }
                            if icons::tiny_icon(ui, ph::CARET_DOWN, "Object down", false) {
                                shape_down = Some((i, si));
                            }
                        });
                    }
                }
            }
        });
        if studio.active_layer == Some(i) {
            let mut op = studio.doc.layers[i].opacity;
            if ui
                .add(Slider::new(&mut op, 0.0..=1.0).show_value(false))
                .changed()
            {
                let l = &studio.doc.layers[i];
                studio.commit(crate::document::Cmd::SetLayerMeta {
                    index: i,
                    name: l.name.clone(),
                    visible: l.visible,
                    locked: l.locked,
                    opacity: op,
                    blend: l.blend,
                    before: (l.name.clone(), l.visible, l.locked, l.opacity, l.blend),
                });
            }
            let mut blend = studio.doc.layers[i].blend;
            ComboBox::from_id_salt(format!("blend-{i}"))
                .selected_text(blend.name())
                .width(140.0)
                .show_ui(ui, |ui| {
                    for b in Blend::ALL {
                        ui.selectable_value(&mut blend, b, b.name());
                    }
                });
            if blend != studio.doc.layers[i].blend {
                let l = &studio.doc.layers[i];
                studio.commit(crate::document::Cmd::SetLayerMeta {
                    index: i,
                    name: l.name.clone(),
                    visible: l.visible,
                    locked: l.locked,
                    opacity: l.opacity,
                    blend,
                    before: (l.name.clone(), l.visible, l.locked, l.opacity, l.blend),
                });
            }
        }
    }
    if let Some(i) = start_rename {
        if i == usize::MAX {
            if let Some((idx, name)) = studio.layer_rename.take() {
                if let Some(l) = studio.doc.layers.get(idx) {
                    let trimmed = name.trim().to_string();
                    if !trimmed.is_empty() && trimmed != l.name {
                        studio.commit(crate::document::Cmd::SetLayerMeta {
                            index: idx,
                            name: trimmed,
                            visible: l.visible,
                            locked: l.locked,
                            opacity: l.opacity,
                            blend: l.blend,
                            before: (
                                l.name.clone(),
                                l.visible,
                                l.locked,
                                l.opacity,
                                l.blend,
                            ),
                        });
                    }
                }
            }
        } else {
            studio.layer_rename = Some((i, studio.doc.layers[i].name.clone()));
        }
    }
    if let Some(i) = activate {
        studio.active_layer = Some(i);
    }
    if let Some(i) = vis {
        let l = &studio.doc.layers[i];
        studio.commit(crate::document::Cmd::SetLayerMeta {
            index: i,
            name: l.name.clone(),
            visible: !l.visible,
            locked: l.locked,
            opacity: l.opacity,
            blend: l.blend,
            before: (l.name.clone(), l.visible, l.locked, l.opacity, l.blend),
        });
    }
    if let Some(i) = lock {
        let l = &studio.doc.layers[i];
        studio.commit(crate::document::Cmd::SetLayerMeta {
            index: i,
            name: l.name.clone(),
            visible: l.visible,
            locked: !l.locked,
            opacity: l.opacity,
            blend: l.blend,
            before: (l.name.clone(), l.visible, l.locked, l.opacity, l.blend),
        });
    }
    if let Some(i) = up
        && i + 1 < n
    {
        studio.commit(crate::document::Cmd::ReorderLayer { from: i, to: i + 1 });
        studio.active_layer = Some(i + 1);
    }
    if let Some(i) = down
        && i > 0
    {
        studio.commit(crate::document::Cmd::ReorderLayer { from: i, to: i - 1 });
        studio.active_layer = Some(i - 1);
    }
    if let Some(id) = toggle_expand {
        if !studio.layer_expanded.remove(&id) {
            studio.layer_expanded.insert(id);
        }
    }
    if let Some((li, id)) = pick_shape {
        studio.selection = vec![(li, id)];
        studio.active_layer = Some(li);
        studio.artboard_sel.clear();
    }
    if let Some((li, id)) = vis_shape {
        if let Some(s) = studio.doc.find_shape(li, id) {
            studio.commit(crate::document::Cmd::SetShapeMeta {
                layer: li,
                id,
                name: s.name.clone(),
                visible: !s.visible,
                locked: s.locked,
                before: (s.name.clone(), s.visible, s.locked),
            });
        }
    }
    if let Some((li, id)) = lock_shape {
        if let Some(s) = studio.doc.find_shape(li, id) {
            studio.commit(crate::document::Cmd::SetShapeMeta {
                layer: li,
                id,
                name: s.name.clone(),
                visible: s.visible,
                locked: !s.locked,
                before: (s.name.clone(), s.visible, s.locked),
            });
        }
    }
    if let Some((li, id)) = start_shape_rename {
        if li == usize::MAX {
            if let Some((l, sid, name)) = studio.shape_rename.take() {
                if let Some(s) = studio.doc.find_shape(l, sid) {
                    let trimmed = name.trim().to_string();
                    if !trimmed.is_empty() && trimmed != s.name {
                        studio.commit(crate::document::Cmd::SetShapeMeta {
                            layer: l,
                            id: sid,
                            name: trimmed,
                            visible: s.visible,
                            locked: s.locked,
                            before: (s.name.clone(), s.visible, s.locked),
                        });
                    }
                }
            }
        } else if let Some(s) = studio.doc.find_shape(li, id) {
            studio.shape_rename = Some((li, id, s.name.clone()));
        }
    }
    if let Some((li, from)) = shape_up {
        if let Some(shapes) = studio.doc.layers.get(li).and_then(|l| l.kind.shapes())
            && from + 1 < shapes.len()
        {
            studio.commit(crate::document::Cmd::ReorderShape {
                layer: li,
                from,
                to: from + 1,
            });
        }
    }
    if let Some((li, from)) = shape_down {
        if from > 0 {
            studio.commit(crate::document::Cmd::ReorderShape {
                layer: li,
                from,
                to: from - 1,
            });
        }
    }
}
