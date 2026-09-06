//! Palette collection controls and swatch editing.
use super::*;

pub(super) fn palettes(ui: &mut Ui, studio: &mut Studio, s: &mut Libraries) {
    let compact = ui.ctx().viewport_rect().height() < 650.0;
    if !compact {
        ui.label(RichText::new("Your colors.").size(21.).strong());
        note(ui, "A little collection. A consistent identity.");
        ui.add_space(10.);
    }
    ui.horizontal(|ui| {
        ui.selectable_value(&mut s.project_scope, false, "Personal");
        ui.selectable_value(&mut s.project_scope, true, "Project");
    });
    if s.project_scope {
        project_picker(ui, studio, s);
        if s.root.is_none() {
            note(
                ui,
                "Choose a project folder to load or start its .omacolors collection.",
            );
            return;
        }
    }
    let root = if s.project_scope {
        s.root.clone()
    } else {
        None
    };
    let path = s.palette_path().unwrap();
    let busy = jobs::is_running::<PaletteResult>(ui.ctx(), PALETTE_ACTION);
    let d = s.draft();
    ui.add_space(8.);
    ui.add(
        egui::TextEdit::singleline(&mut d.query)
            .hint_text("Filter names or hex colors…")
            .desired_width(f32::INFINITY),
    );
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                d.palettes.len() < crate::palette::MAX_PALETTES,
                egui::Button::new("+ Palette"),
            )
            .on_disabled_hover_text("This collection already contains the maximum 256 palettes.")
            .clicked()
        {
            let mut n = d.palettes.len() + 1;
            while d
                .palettes
                .iter()
                .any(|p| p.name.eq_ignore_ascii_case(&format!("Palette {n}")))
            {
                n += 1;
            }
            d.palettes
                .push(Palette::new(format!("Palette {n}"), vec![]));
            d.selected = d.palettes.len() - 1;
            d.selected_name();
            d.dirty = true;
            d.query.clear();
        }
        if ui
            .add_enabled(
                !busy && d.loaded && !d.conflict,
                egui::Button::new(if d.dirty { "Save •" } else { "Save" }),
            )
            .clicked()
        {
            let saved = d.palettes.clone();
            let expected = d.stamp;
            let root = root.clone();
            let path = path.clone();
            start_palette(ui.ctx(), root, move || {
                if file_stamp(&path)? != expected {
                    return Err("Changed on disk. Reload or export a copy before saving.".into());
                }
                crate::palette::save_file(&path, &saved)?;
                Ok(PaletteChange::Saved(saved, file_stamp(&path)?))
            });
        }
        ui.menu_button("···", |ui| {
            if ui
                .add_enabled(!busy, egui::Button::new("Load palettes…"))
                .clicked()
            {
                ui.close();
                if let Some(file) = rfd::FileDialog::new()
                    .add_filter("Color palettes", &["omacolors", "json"])
                    .pick_file()
                {
                    let root = root.clone();
                    start_palette(ui.ctx(), root, move || {
                        Ok(PaletteChange::Imported(crate::palette::load_file(&file)?))
                    });
                }
            }
            if ui
                .add_enabled(
                    !busy && !d.palettes.is_empty(),
                    egui::Button::new("Export collection…"),
                )
                .clicked()
            {
                ui.close();
                if let Some(file) = rfd::FileDialog::new()
                    .set_file_name(".omacolors")
                    .add_filter("Color palettes", &["omacolors", "json"])
                    .save_file()
                {
                    let saved = d.palettes.clone();
                    let root = root.clone();
                    start_palette(ui.ctx(), root, move || {
                        crate::palette::save_file(file, &saved)?;
                        Ok(PaletteChange::Exported)
                    });
                }
            }
            if ui
                .add_enabled(
                    !busy && d.palettes.get(d.selected).is_some(),
                    egui::Button::new("Export selected palette…"),
                )
                .clicked()
            {
                ui.close();
                if let Some(file) = rfd::FileDialog::new()
                    .set_file_name("palette.omacolors")
                    .save_file()
                {
                    let saved = vec![d.palettes[d.selected].clone()];
                    let root = root.clone();
                    start_palette(ui.ctx(), root, move || {
                        crate::palette::save_file(file, &saved)?;
                        Ok(PaletteChange::Exported)
                    });
                }
            }
            if ui
                .button("Reload saved colors")
                .on_hover_text("Discard unsaved palette edits and reload the file")
                .clicked()
            {
                d.dirty = false;
                d.loaded = false;
                d.conflict = false;
                ui.close();
            }
            if ui
                .add_enabled(
                    !d.palettes.is_empty(),
                    egui::Button::new("Duplicate palette"),
                )
                .clicked()
            {
                let p = d.palettes[d.selected].clone();
                match crate::palette::merge(&mut d.palettes, vec![p]) {
                    Ok(_) => {
                        d.selected = d.palettes.len() - 1;
                        d.selected_name();
                        d.dirty = true;
                    }
                    Err(e) => d.message = e,
                }
                ui.close();
            }
        });
    });
    if d.conflict {
        note(
            ui,
            "This file changed on disk. Your edits are safe here. Reload saved colors or export a copy.",
        );
    }
    note(ui, &d.message);
    if d.dirty {
        note(ui, "Unsaved collection · Save writes this library to disk.");
    }
    ui.add_space(8.);
    let query = d.query.trim().to_lowercase();
    let matches: Vec<_> = d
        .palettes
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            p.name.to_lowercase().contains(&query)
                || p.colors
                    .iter()
                    .any(|c| c.hex().to_lowercase().contains(&query))
        })
        .map(|(i, _)| i)
        .collect();
    egui::ScrollArea::vertical()
        .id_salt("palette-library-scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("palette-library-names")
                .max_height(if compact { 68. } else { 96. })
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for &index in &matches {
                        let palette = &d.palettes[index];
                        let response = ui.add(
                            egui::Button::selectable(
                                d.selected == index,
                                format!("{}   ·   {}", palette.name, palette.colors.len()),
                            )
                            .truncate(),
                        );
                        if response.clicked() {
                            d.selected = index;
                            d.selected_name();
                        }
                    }
                });
            if matches.is_empty() {
                note(
                    ui,
                    if d.palettes.is_empty() {
                        "Start a palette, then add a color from your artwork or type its hex value."
                    } else {
                        "No matching palettes. Try a name or a hex color."
                    },
                );
            }
            if let Some(palette) = d.palettes.get(d.selected).cloned() {
                palette_editor(ui, studio, d, &palette);
            }
        });
}

fn palette_editor(ui: &mut Ui, studio: &mut Studio, draft: &mut PaletteDraft, palette: &Palette) {
    let compact = ui.ctx().viewport_rect().height() < 650.0;
    ui.add_space(if compact { 6. } else { 14. });
    ui.separator();
    ui.add_space(if compact { 4. } else { 8. });
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let rename = ui.small_button("Rename").clicked();
            ui.add(
                egui::TextEdit::singleline(&mut draft.name)
                    .hint_text("Palette name")
                    .desired_width(ui.available_width()),
            );
            if rename {
                let name = draft.name.trim().to_string();
                match crate::palette::validate_name(&name) {
                    Ok(())
                        if !draft.palettes.iter().enumerate().any(|(index, palette)| {
                            index != draft.selected && palette.name.eq_ignore_ascii_case(&name)
                        }) =>
                    {
                        draft.palettes[draft.selected].name = name;
                        draft.dirty = true;
                        draft.message.clear();
                    }
                    Ok(()) => draft.message = "That palette name is already in use.".into(),
                    Err(error) => draft.message = error,
                }
            }
        });
    });
    ui.horizontal(|ui| {
        ui.selectable_value(&mut studio.fill_active, true, "Fill");
        ui.selectable_value(&mut studio.fill_active, false, "Stroke");
    });
    let mut remove = None;
    let mut replace = None;
    let spacing = ui.spacing().item_spacing.x;
    let columns = if ui.available_width() >= 3. * 68. + 2. * spacing {
        3
    } else {
        2
    };
    let width = (ui.available_width() - (columns - 1) as f32 * spacing) / columns as f32;
    for row in palette.colors.chunks(columns).enumerate() {
        ui.horizontal(|ui| {
            for (column, color) in row.1.iter().copied().enumerate() {
                let index = row.0 * columns + column;
                ui.push_id(index, |ui| {
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(width, 32.), Sense::click());
                        if ui.is_rect_visible(rect) {
                            checker(ui, rect);
                            ui.painter().rect_filled(rect, 6., color.to_egui());
                            ui.painter().rect_stroke(
                                rect,
                                6.,
                                Stroke::new(1., theme::border()),
                                egui::StrokeKind::Inside,
                            );
                        }
                        let response =
                            response.on_hover_text("Apply color · right-click applies stroke");
                        if response.clicked() {
                            apply_colour(studio, color, studio.fill_active);
                        }
                        if response.secondary_clicked() {
                            apply_colour(studio, color, false);
                        }
                        ui.horizontal(|ui| {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.spacing_mut().button_padding = vec2(3., 2.);
                                    ui.menu_button("⋯", |ui| {
                                        if ui.button("Use current color").clicked() {
                                            replace = Some(index);
                                            ui.close();
                                        }
                                        if ui.button("Copy hex").clicked() {
                                            ui.ctx().copy_text(color.hex());
                                            ui.close();
                                        }
                                        if ui.button("Remove swatch").clicked() {
                                            remove = Some(index);
                                            ui.close();
                                        }
                                    });
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(color.hex()).monospace().size(10.),
                                        )
                                        .truncate(),
                                    )
                                    .on_hover_text(color.hex());
                                },
                            );
                        });
                    });
                });
            }
        });
    }
    if let Some(index) = remove {
        draft.palettes[draft.selected].colors.remove(index);
        draft.dirty = true;
    }
    if let Some(index) = replace {
        draft.palettes[draft.selected].colors[index] = current_colour(studio);
        draft.dirty = true;
    }
    ui.add_space(8.);
    ui.horizontal_wrapped(|ui| {
        if ui.button("+ Current color").clicked() {
            add_colour(draft, current_colour(studio));
        }
        if ui
            .button("From selection")
            .on_hover_text("Collect unique fill and stroke colors from selected objects")
            .clicked()
        {
            for color in selection_colours(studio) {
                add_colour(draft, color);
            }
        }
    });
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let add = ui
                .add_enabled(
                    Rgba::parse_hex(&draft.hex).is_some(),
                    egui::Button::new("+"),
                )
                .on_hover_text("Add hex color")
                .clicked();
            ui.add(
                egui::TextEdit::singleline(&mut draft.hex)
                    .hint_text("#D97C5B or #D97C5B80")
                    .desired_width(ui.available_width()),
            );
            if add {
                let color = Rgba::parse_hex(&draft.hex).unwrap();
                add_colour(draft, color);
                draft.hex.clear();
            }
        });
    });
    ui.add_space(10.);
    ui.collapsing("Manage palette", |ui| {
        if ui
            .button("Remove this palette")
            .on_hover_text("Reload saved colors restores it until you save")
            .clicked()
        {
            draft.palettes.remove(draft.selected);
            draft.selected_name();
            draft.dirty = true;
        }
    });
}

fn add_colour(draft: &mut PaletteDraft, color: Rgba) {
    let Some(palette) = draft.palettes.get(draft.selected) else {
        return;
    };
    if palette.colors.contains(&color) {
        return;
    }
    if palette.colors.len() >= crate::palette::MAX_COLORS_PER_PALETTE {
        draft.message = "This palette already contains the maximum 4096 colors.".into();
        return;
    }
    if draft.palettes.iter().map(|p| p.colors.len()).sum::<usize>()
        >= crate::palette::MAX_TOTAL_COLORS
    {
        draft.message = "This collection already contains the maximum 65536 colors.".into();
        return;
    }
    draft.palettes[draft.selected].colors.push(color);
    draft.dirty = true;
}

fn selection_colours(studio: &Studio) -> Vec<Rgba> {
    let mut colors = Vec::new();
    for &(layer, id) in &studio.selection {
        if let Some(shape) = studio.doc.find_shape(layer, id) {
            match shape.style.fill {
                Fill::Solid(color) => colors.push(color),
                Fill::Linear { c0, c1, .. } | Fill::Radial { c0, c1 } => colors.extend([c0, c1]),
                Fill::None => {}
            }
            if let Some(stroke) = &shape.style.stroke {
                colors.push(stroke.color);
            }
        }
    }
    colors
}

fn current_colour(studio: &Studio) -> Rgba {
    let style = crate::ui::studios::inspected_style(studio);
    if !studio.fill_active {
        return style
            .stroke
            .as_ref()
            .map(|s| s.color)
            .unwrap_or(studio.brush.color);
    }
    match style.fill {
        Fill::Solid(c) => c,
        Fill::Linear { c0, .. } | Fill::Radial { c0, .. } => c0,
        Fill::None => studio.brush.color,
    }
}
fn apply_colour(studio: &mut Studio, color: Rgba, fill: bool) {
    if fill {
        studio.set_fill(Fill::Solid(color));
        studio.brush.color = color;
    } else {
        studio.style = crate::ui::studios::inspected_style(studio).clone();
        studio.set_stroke_color(color);
    }
}
