use crate::app::Studio;
use crate::color::{Blend, Rgba};
use crate::document::{Cap, Fill, Join, Stroke as DocStroke};
use crate::geom::Geom;
use crate::ui::theme::{accent, accent_dim, bg_widget, fg_weak};
use eframe::egui::{
    vec2, Color32, ComboBox, Layout, Panel, RichText, ScrollArea, Slider, Stroke, Ui,
};

pub fn right_panel(ui: &mut Ui, studio: &mut Studio) {
    Panel::right("studios")
        .resizable(true)
        .default_size(280.0)
        .size_range(240.0..=420.0)
        .show(ui, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                color_studio(ui, studio);
                ui.add_space(10.0);
                character_studio(ui, studio);
                ui.add_space(10.0);
                stroke_studio(ui, studio);
                ui.add_space(10.0);
                transform_studio(ui, studio);
                ui.add_space(10.0);
                brush_studio(ui, studio);
                ui.add_space(10.0);
                layers_studio(ui, studio);
            });
        });
}

fn section(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).strong().size(12.0).color(accent()));
    ui.add_space(4.0);
}

fn color_studio(ui: &mut Ui, studio: &mut Studio) {
    section(ui, "Colour");
    ui.horizontal(|ui| {
        fill_well(ui, studio);
        stroke_well(ui, studio);
        ui.vertical(|ui| {
            ui.label(RichText::new("Fill / Stroke").small().color(fg_weak()));
            if ui.small_button("Swap  X").clicked() {
                if let Fill::Solid(f) = studio.style.fill {
                    if let Some(st) = &studio.style.stroke {
                        let sc = st.color;
                        studio.set_fill(Fill::Solid(sc));
                        studio.set_stroke_color(f);
                    }
                }
            }
            if ui.small_button("None fill").clicked() {
                studio.set_fill(Fill::None);
            }
        });
    });
    hsv_picker(ui, studio);
    ui.horizontal(|ui| {
        ui.label("#");
        if ui
            .add(eframe::egui::TextEdit::singleline(&mut studio.hex_buf).desired_width(80.0))
            .lost_focus()
        {
            if let Some(c) = Rgba::parse_hex(&studio.hex_buf) {
                studio.set_fill(Fill::Solid(c));
                studio.brush.color = c;
            }
        }
    });
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for c in studio.swatches.clone() {
            let (rect, resp) =
                ui.allocate_exact_size(vec2(16.0, 16.0), eframe::egui::Sense::click());
            ui.painter().rect_filled(rect, 2.0, c.to_egui());
            if resp.clicked() {
                studio.set_fill(Fill::Solid(c));
                studio.brush.color = c;
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
        swatch_btn(ui, a, "A");
        swatch_btn(ui, b, "B");
        if ui.small_button("Linear fill").clicked() {
            studio.set_fill(Fill::Linear {
                from: [0.0, 0.0],
                to: [1.0, 0.0],
                c0: studio.gradient.0,
                c1: studio.gradient.1,
            });
        }
        if ui.small_button("Radial fill").clicked() {
            studio.set_fill(Fill::Radial {
                c0: studio.gradient.0,
                c1: studio.gradient.1,
            });
        }
    });
    palette_ui(ui, studio);
}

fn palette_ui(ui: &mut Ui, studio: &mut Studio) {
    ui.add_space(8.0);
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(RichText::new("Palettes").strong().size(12.0).color(accent()));
        if ui.small_button("＋ New").clicked() {
            let name = if studio.palette_name_buf.trim().is_empty() {
                format!("Palette {}", studio.palettes.len() + 1)
            } else {
                studio.palette_name_buf.trim().to_string()
            };
            if crate::palette::validate_name(&name).is_ok() {
                let pal = crate::palette::Palette::new(name.clone(), vec![]);
                studio.palettes.push(pal);
                studio.palette_idx = studio.palettes.len() - 1;
                studio.palette_name_buf = name;
                let _ = crate::palette::save(&studio.palettes);
                studio.status = "palette created".into();
            } else {
                studio.status = "invalid palette name".into();
            }
        }
    });
    if studio.palettes.is_empty() {
        studio.palettes = crate::palette::load();
    }
    if studio.palette_idx >= studio.palettes.len() {
        studio.palette_idx = 0;
    }
    let names: Vec<String> = studio.palettes.iter().map(|p| p.name.clone()).collect();
    let cur_name = names.get(studio.palette_idx).cloned().unwrap_or_default();
    ComboBox::from_id_salt("palette-select")
        .selected_text(cur_name.clone())
        .width(180.0)
        .show_ui(ui, |ui| {
            for (i, n) in names.iter().enumerate() {
                ui.selectable_value(&mut studio.palette_idx, i, n);
            }
        });
    // Sync name buf when selection changes
    if let Some(p) = studio.palettes.get(studio.palette_idx) {
        if studio.palette_name_buf != p.name && !p.name.is_empty() {
            // keep buf in sync unless user is typing – we only sync on selection change
            // so we compare after a selection change: detect change via idx vs buf
            // For simplicity, always reflect current palette name in buf when idx changes
            // but we can't detect change easily; just leave buf as is unless it's empty.
        }
    }
    ui.horizontal(|ui| {
        ui.label("Name");
        ui.add(
            eframe::egui::TextEdit::singleline(&mut studio.palette_name_buf)
                .desired_width(120.0)
                .hint_text("palette name"),
        );
        if ui.small_button("Rename").clicked() {
            let new_name = studio.palette_name_buf.trim().to_string();
            if crate::palette::validate_name(&new_name).is_ok() {
                if let Some(p) = studio.palettes.get_mut(studio.palette_idx) {
                    p.name = new_name.clone();
                    let _ = crate::palette::save(&studio.palettes);
                    studio.status = format!("renamed to {new_name}");
                }
            } else {
                studio.status = "invalid name".into();
            }
        }
        if ui.small_button("Delete").clicked() && studio.palettes.len() > 1 {
            studio.palettes.remove(studio.palette_idx);
            studio.palette_idx = studio.palette_idx.min(studio.palettes.len() - 1);
            studio.palette_name_buf = studio.palettes[studio.palette_idx].name.clone();
            let _ = crate::palette::save(&studio.palettes);
            studio.status = "palette deleted".into();
        }
    });
    // Active palette colours
    if let Some(pal) = studio.palettes.get(studio.palette_idx).cloned() {
        ui.horizontal_wrapped(|ui| {
            for (idx, c) in pal.colors.iter().cloned().enumerate() {
                let (rect, resp) = ui.allocate_exact_size(vec2(16.0, 16.0), eframe::egui::Sense::click());
                ui.painter().rect_filled(rect, 2.0, c.to_egui());
                if resp.clicked() {
                    studio.set_fill(Fill::Solid(c));
                    studio.brush.color = c;
                }
                if resp.secondary_clicked() {
                    studio.set_stroke_color(c);
                }
                // small “×” on hover via right-click context
                if resp.hovered() && ui.input(|i| i.key_pressed(eframe::egui::Key::Delete)) {
                    // delete via Delete key while hovered (rare) – handled via button below instead
                    let _ = idx;
                }
                // Show delete button on hover – we overlay a tiny x button after
                // For simplicity, secondary click removes; primary applies.
            }
        });
        ui.horizontal(|ui| {
            if ui.small_button("+ Fill").on_hover_text("Add current fill colour to palette").clicked() {
                let col = match studio.style.fill {
                    Fill::Solid(c) => c,
                    Fill::Linear { c0, .. } | Fill::Radial { c0, .. } => c0,
                    Fill::None => studio.brush.color,
                };
                let name_opt = {
                    if let Some(p) = studio.palettes.get_mut(studio.palette_idx) {
                        if !p.colors.contains(&col) {
                            p.colors.push(col);
                            Some(p.name.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };
                if let Some(name) = name_opt {
                    let _ = crate::palette::save(&studio.palettes);
                    studio.status = format!("added {} to {}", col.hex(), name);
                } else if studio.palettes.get(studio.palette_idx).is_some_and(|p| p.colors.contains(&col)) {
                    studio.status = "colour already in palette".into();
                }
            }
            if ui.small_button("+ Stroke").clicked() {
                if let Some(st) = studio.style.stroke.clone() {
                    let col = st.color;
                    let name_opt = {
                        if let Some(p) = studio.palettes.get_mut(studio.palette_idx) {
                            if !p.colors.contains(&col) {
                                p.colors.push(col);
                                Some(p.name.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };
                    if let Some(name) = name_opt {
                        let _ = crate::palette::save(&studio.palettes);
                        studio.status = format!("added {} to {}", col.hex(), name);
                    }
                }
            }
            if ui.small_button("Clear").clicked() {
                if let Some(p) = studio.palettes.get_mut(studio.palette_idx) {
                    p.colors.clear();
                }
                let _ = crate::palette::save(&studio.palettes);
            }
            if ui.small_button("× Last").on_hover_text("Remove last colour").clicked() {
                if let Some(p) = studio.palettes.get_mut(studio.palette_idx) {
                    p.colors.pop();
                }
                let _ = crate::palette::save(&studio.palettes);
            }
        });
        if pal.colors.is_empty() {
            ui.label(RichText::new("No colours yet – “+ Fill” adds the current fill.").small().color(fg_weak()));
        }
    }
    ui.horizontal(|ui| {
        if ui.small_button("Import…").clicked() {
            if let Some(path) = rfd::FileDialog::new().add_filter("JSON", &["json"]).pick_file() {
                if let Ok(s) = std::fs::read_to_string(&path) {
                    if let Ok(imported) = serde_json::from_str::<Vec<crate::palette::Palette>>(&s) {
                        studio.palettes.extend(imported);
                        let _ = crate::palette::save(&studio.palettes);
                        studio.status = format!("imported {}", path.display());
                    } else {
                        studio.status = "import failed – not a palette JSON".into();
                    }
                }
            }
        }
        if ui.small_button("Export…").clicked() {
            if let Some(path) = rfd::FileDialog::new().add_filter("JSON", &["json"]).save_file() {
                let json_path = if path.extension().is_none() {
                    path.with_extension("json")
                } else {
                    path
                };
                match serde_json::to_string_pretty(&studio.palettes) {
                    Ok(s) => {
                        if std::fs::write(&json_path, s).is_ok() {
                            studio.status = format!("exported {}", json_path.display());
                        } else {
                            studio.status = "export failed".into();
                        }
                    }
                    Err(e) => studio.status = format!("export failed: {e}"),
                }
            }
        }
    });
}

fn swatch_btn(ui: &mut Ui, c: Rgba, tip: &str) {
    let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), eframe::egui::Sense::click());
    ui.painter().rect_filled(rect, 3.0, c.to_egui());
    let _ = resp.on_hover_text(tip);
}

fn fill_well(ui: &mut Ui, studio: &mut Studio) {
    let c = match studio.style.fill {
        Fill::Solid(c) => c.to_egui(),
        Fill::Linear { c0, .. } | Fill::Radial { c0, .. } => c0.to_egui(),
        Fill::None => Color32::TRANSPARENT,
    };
    let (rect, _) = ui.allocate_exact_size(vec2(36.0, 36.0), eframe::egui::Sense::hover());
    ui.painter().rect_filled(rect, 4.0, bg_widget());
    ui.painter().rect_filled(rect.shrink(4.0), 3.0, c);
    ui.painter().rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0, accent_dim()),
        eframe::egui::StrokeKind::Middle,
    );
}

fn stroke_well(ui: &mut Ui, studio: &mut Studio) {
    let c = studio
        .style
        .stroke
        .as_ref()
        .map(|s| s.color.to_egui())
        .unwrap_or(Color32::TRANSPARENT);
    let (rect, _) = ui.allocate_exact_size(vec2(28.0, 28.0), eframe::egui::Sense::hover());
    ui.painter().rect_stroke(
        rect.shrink(3.0),
        3.0,
        Stroke::new(3.0, c),
        eframe::egui::StrokeKind::Middle,
    );
}

fn hsv_picker(ui: &mut Ui, studio: &mut Studio) {
    let current = match studio.style.fill {
        Fill::Solid(c) => c,
        Fill::Linear { c0, .. } | Fill::Radial { c0, .. } => c0,
        Fill::None => studio.brush.color,
    };
    let [h, s, v, a] = current.to_hsva();
    let mut hh = h;
    let mut ss = s;
    let mut vv = v;
    let mut aa = a;
    let mut changed = false;
    changed |= ui.add(Slider::new(&mut hh, 0.0..=1.0).text("H")).changed();
    changed |= ui.add(Slider::new(&mut ss, 0.0..=1.0).text("S")).changed();
    changed |= ui.add(Slider::new(&mut vv, 0.0..=1.0).text("V")).changed();
    changed |= ui.add(Slider::new(&mut aa, 0.0..=1.0).text("A")).changed();
    if changed {
        let c = Rgba::from_hsva(hh, ss, vv, aa);
        studio.set_fill(Fill::Solid(c));
        studio.brush.color = c;
    }
}

fn stroke_studio(ui: &mut Ui, studio: &mut Studio) {
    section(ui, "Stroke");
    let mut width = studio.style.stroke.as_ref().map(|s| s.width).unwrap_or(0.0);
    if ui
        .add(Slider::new(&mut width, 0.0..=64.0).text("Width"))
        .changed()
    {
        let mut st = studio.style.stroke.clone().unwrap_or_default();
        st.width = width;
        studio.style.stroke = if width <= 0.01 {
            None
        } else {
            Some(st.clone())
        };
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
    section(ui, "Character");
    let live = studio.selected_type();
    let mut font = live
        .as_ref()
        .map(|t| t.font.clone())
        .unwrap_or_else(|| studio.text_font.clone());
    let label = crate::text::label_for(&font);
    ComboBox::from_id_salt("character-font")
        .selected_text(label)
        .width(220.0)
        .show_ui(ui, |ui| {
            for f in crate::text::all_fonts() {
                let path = f.path.to_string_lossy().to_string();
                ui.selectable_value(&mut font, path, &f.name);
            }
        });
    if live
        .as_ref()
        .map(|t| t.font.clone())
        .unwrap_or_else(|| studio.text_font.clone())
        != font
    {
        let chosen = font.clone();
        studio.patch_type(|t| t.font = chosen);
    }
    // Google Fonts on-demand
    ui.collapsing("Google Fonts  ⤓", |ui| {
        if !studio.google_catalog_loaded {
            studio.google_catalog = crate::google_fonts::catalog();
            studio.google_catalog_loaded = true;
            if studio.google_catalog.is_empty() {
                studio.google_status = "offline – bundled list".into();
            } else {
                studio.google_status = format!("catalogue: {} families", studio.google_catalog.len());
            }
        }
        ui.horizontal(|ui| {
            ui.add(
                eframe::egui::TextEdit::singleline(&mut studio.google_query)
                    .hint_text("Search Inter, mono…")
                    .desired_width(140.0),
            );
            if ui.small_button("⟳").on_hover_text("Refresh catalogue").clicked() {
                studio.google_catalog = crate::google_fonts::catalog();
                studio.google_status = format!("refreshed: {} families", studio.google_catalog.len());
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
        let filtered: Vec<crate::google_fonts::GoogleFont> = crate::google_fonts::search(&studio.google_catalog, &studio.google_query)
            .into_iter()
            .cloned()
            .collect();
        ScrollArea::vertical()
            .max_height(160.0)
            .show(ui, |ui| {
                for f in filtered {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&f.family).small());
                        ui.label(RichText::new(&f.category).small().color(fg_weak()));
                        let installed = crate::google_fonts::is_installed(&f.family, &studio.google_variant);
                        if installed {
                            if ui.small_button("Use").clicked() {
                                // Prefer the dynamically registered font, else the installed file.
                                let found = crate::text::all_fonts()
                                    .iter()
                                    .find(|ff| ff.name.to_ascii_lowercase().contains(&f.family.to_ascii_lowercase()))
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
                            studio.google_status = format!("Downloading {} {}…", f.family, studio.google_variant);
                            let fam = f.family.clone();
                            let var = studio.google_variant.clone();
                            let cat = studio.google_catalog.clone();
                            match crate::google_fonts::download(&fam, &var, &cat) {
                                Ok(p) => {
                                    let chosen = p.to_string_lossy().to_string();
                                    studio.patch_type(|t| t.font = chosen);
                                    studio.google_status = format!("Installed {} → {}", fam, p.display());
                                }
                                Err(e) => {
                                    studio.google_status = e;
                                }
                            }
                        }
                    });
                }
            });
        if !studio.google_status.is_empty() {
            ui.label(RichText::new(&studio.google_status).small().color(accent()));
        }
    });

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
        if let Some(fam) = crate::text::preferred_default_family_name() {
            let cur = crate::text::label_for(&studio.text_font);
            ui.label(
                RichText::new(format!("Default: {cur} (from {fam} in web apps)"))
                    .small()
                    .color(fg_weak()),
            );
        } else {
            ui.label(
                RichText::new("Applies to the next type you place.")
                    .small()
                    .color(fg_weak()),
            );
        }
    } else {
        ui.label(
            RichText::new("Double-click the type to type into it.")
                .small()
                .color(fg_weak()),
        );
    }
}

fn transform_studio(ui: &mut Ui, studio: &mut Studio) {
    section(ui, "Transform");
    if studio.selection.is_empty() {
        ui.label(RichText::new("Nothing selected").small().color(fg_weak()));
        ui.label(
            RichText::new("Polygon sides / star points apply to the next shape you draw.")
                .small()
                .color(fg_weak()),
        );
        ui.add(Slider::new(&mut studio.polygon_sides, 3..=12).text("Sides"));
        ui.add(Slider::new(&mut studio.star_points, 3..=12).text("Star points"));
        ui.add(Slider::new(&mut studio.star_inner, 0.15..=0.8).text("Star inner"));
        ui.add(Slider::new(&mut studio.rect_radius, 0.0..=80.0).text("Corner radius"));
        return;
    }
    let Some((li, id)) = studio.primary() else {
        return;
    };
    let Some(shape) = studio.doc.find_shape(li, id).cloned() else {
        return;
    };
    let b = shape.world_bbox();
    ui.label(
        RichText::new(format!(
            "{}  {:.0} × {:.0}  at {:.0}, {:.0}",
            shape.name,
            b.width(),
            b.height(),
            b.min.x,
            b.min.y
        ))
        .small()
        .color(fg_weak()),
    );
    // Precise numeric transform – works for every geom kind.
    {
        let mut x = b.min.x;
        let mut y = b.min.y;
        let mut w = b.width().max(1.0);
        let mut h = b.height().max(1.0);
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new("X").small().color(fg_weak()));
            changed |= ui
                .add(eframe::egui::DragValue::new(&mut x).speed(1.0).prefix("X: "))
                .changed();
            ui.label(RichText::new("Y").small().color(fg_weak()));
            changed |= ui
                .add(eframe::egui::DragValue::new(&mut y).speed(1.0).prefix("Y: "))
                .changed();
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("W").small().color(fg_weak()));
            changed |= ui
                .add(eframe::egui::DragValue::new(&mut w).speed(1.0).range(1.0..=10000.0).prefix("W: "))
                .changed();
            ui.label(RichText::new("H").small().color(fg_weak()));
            changed |= ui
                .add(eframe::egui::DragValue::new(&mut h).speed(1.0).range(1.0..=10000.0).prefix("H: "))
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
            let delta = (deg - shape.rotation.to_degrees()).to_radians();
            let mut g = shape.geom.clone();
            g.rotate_about(b.center(), delta);
            let rot_after = deg.to_radians();
            studio.commit(crate::document::Cmd::SetGeom {
                layer: li,
                id,
                before: shape.geom.clone(),
                after: g,
                rot_before: shape.rotation,
                rot_after,
            });
        }
        ui.horizontal(|ui| {
            if ui.small_button("Flip H").on_hover_text("Mirror horizontally").clicked() {
                let mut g = shape.geom.clone();
                let src = b;
                let dst = crate::geom::Bounds {
                    min: crate::geom::Pt::new(b.max.x, b.min.y),
                    max: crate::geom::Pt::new(b.min.x, b.max.y),
                };
                g.map_into(src, dst);
                studio.commit(crate::document::Cmd::SetGeom {
                    layer: li,
                    id,
                    before: shape.geom.clone(),
                    after: g,
                    rot_before: shape.rotation,
                    rot_after: shape.rotation,
                });
            }
            if ui.small_button("Flip V").on_hover_text("Mirror vertically").clicked() {
                let mut g = shape.geom.clone();
                let src = b;
                let dst = crate::geom::Bounds {
                    min: crate::geom::Pt::new(b.min.x, b.max.y),
                    max: crate::geom::Pt::new(b.max.x, b.min.y),
                };
                g.map_into(src, dst);
                studio.commit(crate::document::Cmd::SetGeom {
                    layer: li,
                    id,
                    before: shape.geom.clone(),
                    after: g,
                    rot_before: shape.rotation,
                    rot_after: shape.rotation,
                });
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
        Geom::Text(_) => {
            ui.label(
                RichText::new(
                    "Type lives in the Character studio. Double-click the words to edit.",
                )
                .small()
                .color(fg_weak()),
            );
        }
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
        if ui.small_button("L").on_hover_text("Align left").clicked() {
            studio.align_sel(crate::align::Align::Left);
        }
        if ui.small_button("C").clicked() {
            studio.align_sel(crate::align::Align::CenterX);
        }
        if ui.small_button("R").clicked() {
            studio.align_sel(crate::align::Align::Right);
        }
        if ui.small_button("T").clicked() {
            studio.align_sel(crate::align::Align::Top);
        }
        if ui.small_button("M").clicked() {
            studio.align_sel(crate::align::Align::CenterY);
        }
        if ui.small_button("B").clicked() {
            studio.align_sel(crate::align::Align::Bottom);
        }
    });
    // Compound / Pathfinder
    ui.add_space(6.0);
    ui.label(RichText::new("Compound / Pathfinder").small().color(fg_weak()));
    let n = studio.selection.len();
    let is_compound = matches!(&shape.geom, crate::geom::Geom::Poly { contours } if contours.len() > 1);
    ui.horizontal_wrapped(|ui| {
        let can_bool = n >= 2;
        ui.add_enabled(can_bool, eframe::egui::Button::new("Union")).clicked().then(|| studio.apply_boolean_multi(crate::boolean::BoolOp::Union));
        if !can_bool {
            ui.label(RichText::new("needs ≥2").small().color(fg_weak()));
        }
    });
    ui.horizontal_wrapped(|ui| {
        let can_bool = n >= 2;
        if ui.add_enabled(can_bool, eframe::egui::Button::new("Subtract")).clicked() {
            studio.apply_boolean_multi(crate::boolean::BoolOp::Subtract);
        }
        if ui.add_enabled(can_bool, eframe::egui::Button::new("Intersect")).clicked() {
            studio.apply_boolean_multi(crate::boolean::BoolOp::Intersect);
        }
        if ui.add_enabled(can_bool, eframe::egui::Button::new("Xor")).clicked() {
            studio.apply_boolean_multi(crate::boolean::BoolOp::Xor);
        }
    });
    ui.horizontal(|ui| {
        let can_combine = n >= 2;
        if ui.add_enabled(can_combine, eframe::egui::Button::new("Combine")).on_hover_text("Even-odd compound (Ctrl+E)").clicked() {
            studio.combine_selected();
        }
        if ui.add_enabled(is_compound, eframe::egui::Button::new("Release")).on_hover_text("Explode compound (Ctrl+Shift+E)").clicked() {
            studio.release_compound();
        }
        if !can_combine && !is_compound {
            ui.label(RichText::new("select ≥2 or a compound").small().color(fg_weak()));
        }
    });
}

fn brush_studio(ui: &mut Ui, studio: &mut Studio) {
    section(ui, "Brush");
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
        section(ui, "Layers");
        ui.with_layout(Layout::right_to_left(eframe::egui::Align::Center), |ui| {
            if ui.small_button("+Px").clicked() {
                studio.add_layer(true);
            }
            if ui.small_button("+V").clicked() {
                studio.add_layer(false);
            }
            if ui.small_button("–").clicked() {
                studio.delete_layer();
            }
        });
    });

    let mut activate = None;
    let mut vis = None;
    let mut lock = None;
    let mut up = None;
    let mut down = None;
    let n = studio.doc.layers.len();
    for i in (0..n).rev() {
        ui.push_id(studio.doc.layers[i].id, |ui| {
            ui.horizontal(|ui| {
                let layer = &studio.doc.layers[i];
                let on = studio.active_layer == Some(i);
                let eye = if layer.visible { "●" } else { "○" };
                if ui.small_button(eye).clicked() {
                    vis = Some(i);
                }
                let lock_s = if layer.locked { "🔒" } else { " " };
                if ui.small_button(lock_s).clicked() {
                    lock = Some(i);
                }
                let fill = if on { accent_dim() } else { Color32::TRANSPARENT };
                let label = format!("{}  {}", layer.kind.tag(), layer.name);
                if ui
                    .add(eframe::egui::Button::new(RichText::new(label).size(12.0)).fill(fill))
                    .clicked()
                {
                    activate = Some(i);
                }
                if ui.small_button("↑").clicked() {
                    up = Some(i);
                }
                if ui.small_button("↓").clicked() {
                    down = Some(i);
                }
            });
        });
        ui.add(Slider::new(&mut studio.doc.layers[i].opacity, 0.0..=1.0).show_value(false));
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
    if let Some(i) = up {
        if i + 1 < n {
            studio.commit(crate::document::Cmd::ReorderLayer { from: i, to: i + 1 });
            studio.active_layer = Some(i + 1);
        }
    }
    if let Some(i) = down {
        if i > 0 {
            studio.commit(crate::document::Cmd::ReorderLayer { from: i, to: i - 1 });
            studio.active_layer = Some(i - 1);
        }
    }
}
