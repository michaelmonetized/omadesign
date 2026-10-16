//! Color relationships, editable theme libraries and explicit artwork recoloring.
use super::{
    fields,
    icons::{self, ph},
    welcome,
};
use crate::{
    app::{Studio, recolor::RecolorTarget},
    color::{
        Rgba,
        harmony::{self, Harmony, Variation},
    },
    document::Fill,
    palette::Palette,
};
use eframe::egui::{self, Id, Response, Ui};
mod library;

#[derive(Clone)]
struct Guide {
    base: Rgba,
    rule: Harmony,
    variation: Variation,
    steps: u32,
    amount: f32,
    selected: Vec<(usize, i32)>,
    limit: Option<usize>,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Theme {
    id: u64,
    name: String,
    colors: Vec<Rgba>,
    rule: Option<Harmony>,
}
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
struct Themes {
    next: u64,
    entries: Vec<Theme>,
    #[serde(skip)]
    stamp: Option<u64>,
    #[serde(skip)]
    error: String,
    #[serde(skip)]
    loaded: bool,
}
#[derive(Clone)]
struct ThemeDraft {
    colors: Vec<Rgba>,
    rule: Option<Harmony>,
    selected: usize,
    name: String,
    editing: Option<u64>,
    linked: bool,
}
#[derive(Clone)]
struct Recolor {
    target: RecolorTarget,
    colors: Vec<Rgba>,
    message: String,
}

fn named(ui: &mut Ui, id: Id, name: &str, draw: impl FnOnce(&mut Ui) -> Response) -> Response {
    let response = fields::native_scope(ui, id, draw).inner;
    welcome::name_control(ui, &response, id.with("label"), name);
    welcome::focus_control(ui, &response);
    response
}
fn icon(ui: &mut Ui, id: Id, glyph: &str, name: &str) -> bool {
    named(ui, id, name, |ui| {
        ui.add(egui::Button::new(egui::RichText::new(glyph).font(icons::font(16.))).frame(false))
    })
    .on_hover_text(name)
    .clicked()
}
fn swatch(ui: &mut Ui, id: Id, color: Rgba, on: bool, name: &str, size: f32) -> Response {
    let response = named(ui, id, name, |ui| {
        let (_, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), on, name)
        });
        response
    });
    let rect = response.rect.shrink(2.);
    let cell = (size / 4.).max(3.);
    for y in 0..4 {
        for x in 0..4 {
            let tile = egui::Rect::from_min_size(
                rect.min + egui::vec2(x as f32 * cell, y as f32 * cell),
                egui::vec2(cell, cell),
            )
            .intersect(rect);
            ui.painter().rect_filled(
                tile,
                0.,
                if (x + y) % 2 == 0 {
                    ui.visuals().faint_bg_color
                } else {
                    ui.visuals().extreme_bg_color
                },
            );
        }
    }
    ui.painter().rect_filled(rect, 0., color.to_egui());
    if on {
        ui.painter().rect_stroke(
            response.rect.shrink(0.5),
            0.,
            egui::Stroke::new(2., super::theme::accent()),
            egui::StrokeKind::Inside,
        );
    }
    response.on_hover_text(color.hex())
}
fn current(studio: &Studio) -> Rgba {
    let style = super::studios::inspected_style(studio);
    if !studio.fill_active {
        return style
            .stroke
            .as_ref()
            .map_or(studio.brush.color, |s| s.color);
    }
    match &style.fill {
        Fill::Solid(c) | Fill::Linear { c0: c, .. } | Fill::Radial { c0: c, .. } => *c,
        Fill::Gradient(g) => g.stops.first().map_or(studio.brush.color, |s| s.color),
        _ => studio.brush.color,
    }
}
fn apply(studio: &mut Studio, color: Rgba) {
    if let Err(error) = studio.apply_palette_color(color) {
        studio.status = error;
    }
}
fn target(ui: &mut Ui, studio: &mut Studio, id: Id) {
    ui.horizontal(|ui| {
        named(ui, id.with("fill"), "Apply guide colors to fill", |ui| {
            ui.selectable_value(&mut studio.fill_active, true, "Fill")
        });
        named(
            ui,
            id.with("stroke"),
            "Apply guide colors to stroke",
            |ui| ui.selectable_value(&mut studio.fill_active, false, "Stroke"),
        );
    });
}
fn rule(ui: &mut Ui, id: Id, value: &mut Option<Harmony>, custom: bool) {
    let name = value.map_or("Custom", Harmony::name);
    let response = egui::ComboBox::from_id_salt(id)
        .selected_text(name)
        .width(ui.available_width().max(40.))
        .show_ui(ui, |ui| {
            if custom {
                ui.selectable_value(value, None, "Custom");
            }
            for item in Harmony::ALL {
                ui.selectable_value(value, Some(item), item.name());
            }
        })
        .response;
    welcome::name_control(ui, &response, id.with("label"), "Color harmony rule");
}
fn save_swatches(ui: &mut Ui, studio: &mut Studio, name: String, colors: Vec<Rgba>) {
    match super::library::save_generated_palette(ui.ctx(), studio, Palette::new(name, colors)) {
        Ok(()) => studio.status = "Saving colors to the current swatch collection".into(),
        Err(error) => {
            studio.status = error;
            super::panel_workspace::open(ui.ctx(), studio, "swatches");
        }
    }
}

/// Show a registered color workflow using real paint commands and guarded library saves.
/// Args: pane UI, current editor and registered pane ID. Returns nothing.
pub(super) fn show(ui: &mut Ui, studio: &mut Studio, panel: &str) {
    match panel {
        "color-themes" => themes(ui, studio),
        "recolor" => recolor(ui, studio),
        _ => guide(ui, studio),
    }
}

fn guide(ui: &mut Ui, studio: &mut Studio) {
    let id = Id::new(("color-guide", &studio.swap_id));
    let mut state = ui
        .data(|d| d.get_temp::<Guide>(id))
        .unwrap_or_else(|| Guide {
            base: current(studio),
            rule: Harmony::Complementary,
            variation: Variation::default(),
            steps: 4,
            amount: 50.,
            selected: vec![],
            limit: None,
        });
    let old = (
        state.base,
        state.rule,
        state.variation,
        state.steps,
        state.amount,
        state.limit,
    );
    let palettes = studio.libraries.draft().palettes.clone();
    if state.limit.is_some_and(|i| i >= palettes.len()) {
        state.limit = None;
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.;
        named(ui, id.with("fill"), "Apply guide colors to fill", |ui| {
            ui.selectable_value(&mut studio.fill_active, true, "Fill")
        });
        named(
            ui,
            id.with("stroke"),
            "Apply guide colors to stroke",
            |ui| ui.selectable_value(&mut studio.fill_active, false, "Stroke"),
        );
        super::color_picker::color_edit_named(
            ui,
            id.with("base"),
            &mut state.base,
            "Guide base color",
        );
        if icon(
            ui,
            id.with("capture"),
            ph::EYEDROPPER,
            "Use current paint as the guide base",
        ) {
            state.base = current(studio);
        }
        if icon(
            ui,
            id.with("themes"),
            ph::PALETTE,
            "Edit harmony as a five-color theme",
        ) {
            ui.data_mut(|d| {
                d.insert_temp(
                    Id::new("color-theme-draft"),
                    ThemeDraft {
                        colors: state.rule.theme(state.base),
                        rule: Some(state.rule),
                        selected: 0,
                        name: String::new(),
                        editing: None,
                        linked: true,
                    },
                )
            });
            super::panel_workspace::open(ui.ctx(), studio, "color-themes");
        }
        let response = ui
            .menu_button("···", |ui| {
                ui.set_max_width(270.);
                for variation in Variation::ALL {
                    named(
                        ui,
                        id.with(("variation", variation as u8)),
                        variation.name(),
                        |ui| ui.selectable_value(&mut state.variation, variation, variation.name()),
                    );
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Steps");
                    named(ui, id.with("steps"), "Guide steps per side", |ui| {
                        ui.add(egui::DragValue::new(&mut state.steps).range(1..=20))
                    });
                    ui.label("Reach");
                    named(ui, id.with("reach"), "Guide variation reach", |ui| {
                        ui.add(
                            egui::DragValue::new(&mut state.amount)
                                .range(0.0..=100.0)
                                .suffix("%"),
                        )
                    });
                });
                ui.separator();
                ui.label("Limit to swatches");
                named(ui, id.with("unlimited"), "No guide library limit", |ui| {
                    ui.selectable_value(&mut state.limit, None, "None")
                });
                egui::ScrollArea::vertical()
                    .max_height(180.)
                    .show(ui, |ui| {
                        for (i, palette) in palettes.iter().enumerate() {
                            named(
                                ui,
                                id.with(("library", i)),
                                &format!("Limit to {}", palette.name),
                                |ui| ui.selectable_value(&mut state.limit, Some(i), &palette.name),
                            );
                        }
                    });
            })
            .response;
        welcome::name_control(
            ui,
            &response,
            id.with("options-label"),
            "Color Guide options",
        );
    });
    let mut harmony = Some(state.rule);
    rule(ui, id.with("rule"), &mut harmony, false);
    state.rule = harmony.unwrap_or_default();
    if old
        != (
            state.base,
            state.rule,
            state.variation,
            state.steps,
            state.amount,
            state.limit,
        )
    {
        state.selected.clear();
    }
    let colors = state.rule.colors(state.base);
    let palette = state
        .limit
        .and_then(|i| palettes.get(i))
        .map_or(&[][..], |p| p.colors.as_slice());
    let guide_color = |base, step: i32| {
        harmony::nearest(
            state.variation.color(
                base,
                step as f32 / state.steps.max(1) as f32 * (0.1 + 0.8 * state.amount / 100.),
            ),
            palette,
        )
    };
    egui::ScrollArea::horizontal()
        .id_salt(id.with("grid"))
        .animated(false)
        .show(ui, |ui| {
            for (row, color) in colors.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.;
                    for step in -(state.steps as i32)..=state.steps as i32 {
                        let color = guide_color(*color, step);
                        if swatch(
                            ui,
                            id.with((row, step)),
                            color,
                            state.selected.contains(&(row, step)),
                            &format!("Guide color {} variation {step}: {}", row + 1, color.hex()),
                            22.,
                        )
                        .clicked()
                        {
                            if !ui.input(|i| {
                                i.modifiers.shift || i.modifiers.command || i.modifiers.ctrl
                            }) {
                                state.selected.clear();
                                apply(studio, color);
                            }
                            if let Some(i) = state.selected.iter().position(|p| *p == (row, step)) {
                                state.selected.remove(i);
                            } else {
                                state.selected.push((row, step));
                            }
                        }
                    }
                });
            }
        });
    ui.horizontal(|ui| {
        if icon(
            ui,
            id.with("save"),
            ph::FLOPPY_DISK,
            "Save guide colors as swatches",
        ) {
            let colors = if state.selected.is_empty() {
                colors
                    .iter()
                    .map(|c| harmony::nearest(*c, palette))
                    .collect()
            } else {
                state
                    .selected
                    .iter()
                    .filter_map(|&(row, step)| colors.get(row).map(|c| guide_color(*c, step)))
                    .collect()
            };
            save_swatches(
                ui,
                studio,
                format!("{} {}", state.rule.name(), state.base.hex()),
                colors,
            );
        }
        if icon(
            ui,
            id.with("recolor"),
            ph::PAINT_BRUSH,
            "Recolor selected artwork with this harmony",
        ) {
            open_recolor(
                ui,
                studio,
                &colors
                    .iter()
                    .map(|c| harmony::nearest(*c, palette))
                    .collect::<Vec<_>>(),
            );
        }
    });
    ui.data_mut(|d| d.insert_temp(id, state));
}

fn themes(ui: &mut Ui, studio: &mut Studio) {
    let key = Id::new("color-themes-library");
    let draft_id = Id::new("color-theme-draft");
    let file = library::path(ui.ctx());
    let mut library = ui
        .data(|d| d.get_temp::<Themes>(key))
        .unwrap_or_else(|| library::load(&file));
    let previous = library.clone();
    library.entries.retain(|entry| entry.colors.len() == 5);
    library.entries.truncate(256);
    library.next = library
        .entries
        .iter()
        .map(|e| e.id)
        .max()
        .unwrap_or(0)
        .max(library.next);
    let mut draft = ui
        .data(|d| d.get_temp::<ThemeDraft>(draft_id))
        .unwrap_or_else(|| ThemeDraft {
            colors: Harmony::default().theme(current(studio)),
            rule: Some(Harmony::default()),
            selected: 0,
            name: String::new(),
            editing: None,
            linked: true,
        });
    target(ui, studio, draft_id);
    let old_rule = draft.rule;
    rule(ui, draft_id.with("rule"), &mut draft.rule, true);
    if old_rule != draft.rule
        && let Some(rule) = draft.rule
    {
        draft.colors = rule.theme(draft.colors[draft.selected]);
        draft.selected = 0;
    }
    ui.horizontal(|ui| {
        for (i, color) in draft.colors.iter().enumerate() {
            if swatch(
                ui,
                draft_id.with(i),
                *color,
                draft.selected == i,
                &format!("Theme color {}: {}", i + 1, color.hex()),
                28.,
            )
            .clicked()
            {
                draft.selected = i;
            }
        }
    });
    let mut color = draft.colors[draft.selected];
    ui.horizontal(|ui| {
        if super::color_picker::color_edit_named(
            ui,
            draft_id.with("edit-color"),
            &mut color,
            "Selected theme color",
        ) {
            harmony::move_color(&mut draft.colors, draft.selected, color, draft.linked);
            if !draft.linked {
                draft.rule = None;
            }
        }
        named(
            ui,
            draft_id.with("linked"),
            "Move theme colors together",
            |ui| ui.checkbox(&mut draft.linked, "Link"),
        );
        if icon(
            ui,
            draft_id.with("apply"),
            ph::PAINT_BRUSH,
            "Apply selected theme color",
        ) {
            apply(studio, color);
        }
    });
    wheel(ui, &mut draft, draft_id);
    let response = ui.add(
        egui::TextEdit::singleline(&mut draft.name)
            .id(draft_id.with("name"))
            .hint_text("Theme name")
            .desired_width(ui.available_width()),
    );
    welcome::name_control(
        ui,
        &response,
        draft_id.with("name-label"),
        "Color theme name",
    );
    ui.horizontal(|ui| {
        ui.add_enabled_ui(
            library.loaded
                && library.next < u64::MAX
                && (library.entries.len() < 256 || draft.editing.is_some()),
            |ui| {
                if icon(
                    ui,
                    draft_id.with("save"),
                    ph::FLOPPY_DISK,
                    "Save color theme",
                ) {
                    let name = if draft.name.trim().is_empty() {
                        format!("Theme {}", library.next + 1)
                    } else {
                        draft.name.trim().chars().take(64).collect()
                    };
                    let id = draft
                        .editing
                        .filter(|id| library.entries.iter().any(|e| e.id == *id))
                        .unwrap_or_else(|| {
                            library.next += 1;
                            library.next
                        });
                    let entry = Theme {
                        id,
                        name: name.clone(),
                        colors: draft.colors.clone(),
                        rule: draft.rule,
                    };
                    if let Some(existing) = library.entries.iter_mut().find(|e| e.id == id) {
                        *existing = entry;
                    } else {
                        library.entries.push(entry);
                    }
                    draft.editing = Some(id);
                    draft.name = name;
                }
            },
        );
        if icon(
            ui,
            draft_id.with("new"),
            ph::PLUS,
            "Create a new color theme",
        ) {
            draft.editing = None;
            draft.name.clear();
        }
        if icon(
            ui,
            draft_id.with("swatches"),
            ph::PALETTE,
            "Save theme to swatches",
        ) {
            save_swatches(
                ui,
                studio,
                if draft.name.trim().is_empty() {
                    "Color theme".into()
                } else {
                    draft.name.clone()
                },
                draft.colors.clone(),
            );
        }
        if icon(
            ui,
            draft_id.with("recolor"),
            ph::PAINT_BRUSH,
            "Recolor artwork with theme",
        ) {
            open_recolor(ui, studio, &draft.colors);
        }
    });
    let mut remove = None;
    for theme in &library.entries {
        ui.push_id(theme.id, |ui| {
            ui.horizontal_wrapped(|ui| {
                if named(
                    ui,
                    key.with((theme.id, "select")),
                    &format!("Edit theme {}", theme.name),
                    |ui| ui.selectable_label(draft.editing == Some(theme.id), &theme.name),
                )
                .clicked()
                {
                    draft = ThemeDraft {
                        colors: theme.colors.clone(),
                        rule: theme.rule,
                        selected: 0,
                        name: theme.name.clone(),
                        editing: Some(theme.id),
                        linked: true,
                    };
                }
                if icon(
                    ui,
                    key.with((theme.id, "delete")),
                    ph::TRASH,
                    &format!("Delete theme {}", theme.name),
                ) {
                    remove = Some(theme.id);
                }
            });
            ui.horizontal(|ui| {
                for (i, color) in theme.colors.iter().enumerate() {
                    if swatch(
                        ui,
                        key.with((theme.id, i)),
                        *color,
                        false,
                        &format!("{} color {}: {}", theme.name, i + 1, color.hex()),
                        22.,
                    )
                    .clicked()
                    {
                        apply(studio, *color);
                    }
                }
            });
        });
    }
    if let Some(id) = remove {
        library.entries.retain(|e| e.id != id);
        if draft.editing == Some(id) {
            draft.editing = None;
        }
    }
    if serde_json::to_vec(&library).ok() != serde_json::to_vec(&previous).ok() {
        match library::save(&file, &mut library) {
            Ok(()) => library.error.clear(),
            Err(error) => {
                library = previous;
                library.error = error;
            }
        }
    }
    if !library.error.is_empty() {
        studio.status = library.error.clone();
        ui.colored_label(ui.visuals().error_fg_color, &library.error);
        if icon(
            ui,
            key.with("reload"),
            ph::ARROW_CLOCKWISE,
            "Reload color theme library",
        ) {
            library = library::load(&file);
            draft.editing = None;
        }
    }
    ui.data_mut(|d| {
        d.insert_temp(key, library);
        d.insert_temp(draft_id, draft);
    });
}

fn wheel(ui: &mut Ui, draft: &mut ThemeDraft, id: Id) {
    let edge = ui.available_width().min(170.);
    let response = named(
        ui,
        id.with("wheel"),
        "Theme hue and saturation wheel",
        |ui| ui.allocate_response(egui::vec2(edge, edge), egui::Sense::click_and_drag()),
    );
    let radius = edge * 0.43;
    let center = response.rect.center();
    let value = draft.colors[draft.selected].to_hsva()[2];
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(center, Rgba::from_hsva(0., 0., value, 1.).to_egui());
    for i in 0..=72 {
        let turn = i as f32 / 72.;
        let angle = turn * std::f32::consts::TAU;
        mesh.colored_vertex(
            center + egui::vec2(angle.cos(), angle.sin()) * radius,
            Rgba::from_hsva(turn, 1., value, 1.).to_egui(),
        );
        if i > 0 {
            mesh.add_triangle(0, i, i + 1);
        }
    }
    ui.painter().add(egui::Shape::mesh(mesh));
    for (i, color) in draft.colors.iter().enumerate() {
        let [h, s, _, _] = color.to_hsva();
        let a = h * std::f32::consts::TAU;
        let point = center + egui::vec2(a.cos(), a.sin()) * radius * s;
        ui.painter().circle_filled(
            point,
            if i == draft.selected { 6. } else { 4. },
            color.to_egui(),
        );
        ui.painter().circle_stroke(
            point,
            if i == draft.selected { 6. } else { 4. },
            egui::Stroke::new(1.5, ui.visuals().text_color()),
        );
    }
    let mut hsva = draft.colors[draft.selected].to_hsva();
    let mut changed = false;
    if (response.clicked() || response.dragged())
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let offset = pointer - center;
        hsva[0] = offset.y.atan2(offset.x) / std::f32::consts::TAU;
        hsva[1] = (offset.length() / radius).min(1.);
        changed = true;
        response.request_focus();
    }
    if response.has_focus() {
        let direction = ui.input(|i| {
            f32::from(i.key_pressed(egui::Key::ArrowRight) as u8)
                - f32::from(i.key_pressed(egui::Key::ArrowLeft) as u8)
        });
        let saturation = ui.input(|i| {
            f32::from(i.key_pressed(egui::Key::ArrowUp) as u8)
                - f32::from(i.key_pressed(egui::Key::ArrowDown) as u8)
        });
        hsva[0] += direction / 360.;
        hsva[1] = (hsva[1] + saturation * 0.01).clamp(0., 1.);
        changed |= direction != 0. || saturation != 0.;
    }
    if changed {
        if !draft.linked {
            draft.rule = None;
        }
        harmony::move_color(
            &mut draft.colors,
            draft.selected,
            Rgba::from_hsva(hsva[0], hsva[1], hsva[2], hsva[3]),
            draft.linked,
        );
    }
}

fn open_recolor(ui: &mut Ui, studio: &mut Studio, palette: &[Rgba]) {
    match studio.recolor_target() {
        Ok(target) => {
            let colors = target
                .colors
                .iter()
                .map(|c| harmony::nearest(*c, palette))
                .collect();
            ui.data_mut(|d| {
                d.insert_temp(
                    Id::new(("recolor-draft", &studio.swap_id)),
                    Recolor {
                        target,
                        colors,
                        message: String::new(),
                    },
                )
            });
            super::panel_workspace::open(ui.ctx(), studio, "recolor");
        }
        Err(error) => studio.status = error,
    }
}
fn recolor(ui: &mut Ui, studio: &mut Studio) {
    let id = Id::new(("recolor-draft", &studio.swap_id));
    let mut draft = ui.data(|d| d.get_temp::<Recolor>(id));
    ui.horizontal(|ui| {
        if icon(
            ui,
            id.with("refresh"),
            ph::ARROW_CLOCKWISE,
            "Refresh recolor selection",
        ) || draft.is_none()
        {
            draft = match studio.recolor_target() {
                Ok(target) => Some(Recolor {
                    colors: target.colors.clone(),
                    target,
                    message: String::new(),
                }),
                Err(error) => {
                    ui.label(error);
                    None
                }
            };
        }
        if icon(ui, id.with("guide"), ph::PALETTE, "Open Color Guide") {
            super::panel_workspace::open(ui.ctx(), studio, "color-guide");
        }
    });
    let Some(mut draft) = draft else {
        return;
    };
    if draft.target.patterns > 0 {
        ui.label(format!(
            "{} pattern paints remain unchanged",
            draft.target.patterns
        ));
    }
    for (index, (before, after)) in draft
        .target
        .colors
        .iter()
        .zip(draft.colors.iter_mut())
        .enumerate()
    {
        ui.horizontal(|ui| {
            swatch(
                ui,
                id.with((index, "original")),
                *before,
                false,
                &format!("Original color {}: {}", index + 1, before.hex()),
                22.,
            );
            ui.label("→");
            super::color_picker::color_edit_named(
                ui,
                id.with((index, "replacement")),
                after,
                &format!("Replacement color {}", index + 1),
            );
            after.a = before.a;
            ui.label(before.hex());
        });
    }
    if named(ui, id.with("apply"), "Apply recolor artwork", |ui| {
        ui.add_enabled(!draft.colors.is_empty(), egui::Button::new("Apply"))
    })
    .clicked()
    {
        match studio.recolor_artwork(&draft.target, &draft.colors) {
            Ok(_) => {
                if let Ok(target) = studio.recolor_target() {
                    draft = Recolor {
                        colors: target.colors.clone(),
                        target,
                        message: String::new(),
                    };
                }
            }
            Err(error) => draft.message = error,
        }
    }
    if !draft.message.is_empty() {
        ui.colored_label(ui.visuals().error_fg_color, &draft.message);
    }
    ui.data_mut(|d| d.insert_temp(id, draft));
}

#[cfg(test)]
mod tests {
    use super::super::welcome::keyboard_tests as keys;
    use super::*;
    pub(super) fn context() -> egui::Context {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let ctx = egui::Context::default();
        let root = std::path::PathBuf::from(
            std::env::var("TMPDIR").expect("Run through the serialized helper"),
        );
        let file = root.join(format!(
            "color-themes-{}-{}.json",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        ctx.data_mut(|d| d.insert_temp(Id::new("color-theme-file"), file));
        ctx
    }
    use crate::{
        document::{Document, Layer, Shape, Style},
        geom::{Geom, Pt},
    };
    use egui::{Event, Key, Modifiers};

    fn studio() -> Studio {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.doc = Document::new("Colors", 240., 180., 72.);
        let mut layer = Layer::vector("Art");
        let shape = Shape::new(
            Geom::Rect {
                origin: Pt::new(10., 10.),
                size: Pt::new(40., 30.),
                radius: 0.,
            },
            Style {
                fill: Fill::Solid(Rgba::rgb(255, 0, 0)),
                stroke: None,
            },
        );
        s.selection = vec![(0, shape.id)];
        layer.kind.shapes_mut().unwrap().push(shape);
        s.doc.layers = vec![layer];
        s.active_layer = Some(0);
        s
    }
    fn click(
        ctx: &egui::Context,
        frame: &mut impl FnMut(Vec<Event>) -> egui::FullOutput,
        name: &str,
    ) {
        let output = frame(vec![]);
        let (_, node) = keys::named(&output, name);
        let rect = node.bounds().unwrap();
        let pos = egui::pos2(
            ((rect.x0 + rect.x1) * 0.5) as f32,
            ((rect.y0 + rect.y1) * 0.5) as f32,
        );
        for pressed in [true, false] {
            frame(vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ]);
        }
    }
    #[test]
    fn craft_color_guide_real_swatch_and_theme_wheel_native_focus_save_and_reopen() {
        let ctx = context();
        ctx.enable_accesskit();
        super::super::theme::apply(&ctx);
        let mut studio = studio();
        let before = crate::project::encode(&studio.doc).unwrap();
        let mut frame = |events| {
            keys::render(&ctx, events, |ui| {
                ui.set_max_width(304.);
                show(ui, &mut studio, "color-guide");
            })
        };
        frame(vec![]);
        frame(vec![]);
        click(&ctx, &mut frame, "Guide color 1 variation 1: #FF2020");
        drop(frame);
        assert_eq!(
            studio
                .doc
                .find_shape(0, studio.selection[0].1)
                .unwrap()
                .style
                .fill,
            Fill::Solid(Rgba::rgb(255, 32, 32))
        );
        assert_eq!(studio.history.len(), 1);
        let saved = crate::project::encode(&studio.doc).unwrap();
        assert_eq!(
            crate::project::encode(&crate::project::decode(&saved).unwrap()).unwrap(),
            saved
        );
        studio.undo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
        let mut frame = |events| {
            keys::render(&ctx, events, |ui| {
                ui.set_max_width(304.);
                show(ui, &mut studio, "color-themes");
            })
        };
        frame(vec![]);
        frame(vec![]);
        keys::tab_to(&ctx, "Theme hue and saturation wheel", &mut frame);
        let initial = ctx
            .data(|d| d.get_temp::<ThemeDraft>(Id::new("color-theme-draft")))
            .unwrap()
            .colors;
        frame(keys::key(Key::ArrowRight, Modifiers::NONE));
        let moved = ctx
            .data(|d| d.get_temp::<ThemeDraft>(Id::new("color-theme-draft")))
            .unwrap()
            .colors;
        assert_ne!(initial, moved);
        click(&ctx, &mut frame, "Save color theme");
        drop(frame);
        let library = ctx
            .data(|d| d.get_temp::<Themes>(Id::new("color-themes-library")))
            .unwrap();
        assert_eq!(library.entries.len(), 1);
        assert_eq!(library.entries[0].colors, moved);
        let reopened: Themes =
            serde_json::from_str(&serde_json::to_string(&library).unwrap()).unwrap();
        assert_eq!(reopened.entries[0].colors, moved);
        let reloaded = library::load(&library::path(&ctx));
        assert!(reloaded.loaded);
        assert_eq!(reloaded.entries[0].colors, moved);
        assert_eq!(
            crate::project::encode(&studio.doc).unwrap(),
            before,
            "theme editing does not change artwork"
        );
    }

    #[test]
    fn craft_color_recolor_native_hex_apply_undo_and_selection_retirement() {
        let ctx = context();
        ctx.enable_accesskit();
        super::super::theme::apply(&ctx);
        let mut studio = studio();
        let before = crate::project::encode(&studio.doc).unwrap();
        let mut frame = |events| {
            keys::render(&ctx, events, |ui| {
                ui.set_max_width(304.);
                show(ui, &mut studio, "recolor");
            })
        };
        frame(vec![]);
        frame(vec![]);
        click(&ctx, &mut frame, "Replacement color 1");
        keys::tab_to(&ctx, "Hex color", &mut frame);
        frame(keys::key(Key::A, Modifiers::CTRL | Modifiers::COMMAND));
        frame(vec![Event::Paste("#12AB34".into())]);
        frame(keys::key(Key::Escape, Modifiers::NONE));
        frame(vec![]);
        click(&ctx, &mut frame, "Apply recolor artwork");
        drop(frame);
        assert_eq!(
            studio
                .doc
                .find_shape(0, studio.selection[0].1)
                .unwrap()
                .style
                .fill,
            Fill::Solid(Rgba::rgb(18, 171, 52))
        );
        let after = crate::project::encode(&studio.doc).unwrap();
        assert_ne!(before, after);
        assert_eq!(studio.history.len(), 1);
        studio.undo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
        studio.redo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), after);
        let target = studio.recolor_target().unwrap();
        studio.selection.clear();
        assert!(studio.recolor_artwork(&target, &target.colors).is_err());
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), after);
    }

    #[test]
    fn craft_color_compact_dock_keeps_swatches_visible_and_options_operable() {
        let ctx = context();
        ctx.enable_accesskit();
        super::super::theme::apply(&ctx);
        let mut studio = studio();
        let before = crate::project::encode(&studio.doc).unwrap();
        let owner = Id::new(("color-guide", &studio.swap_id));
        let mut frame = |events| {
            keys::render(&ctx, events, |ui| {
                ui.set_max_width(304.);
                egui::ScrollArea::vertical()
                    .max_height(172.)
                    .min_scrolled_height(0.)
                    .show(ui, |ui| show(ui, &mut studio, "color-guide"));
            })
        };
        frame(vec![]);
        let output = frame(vec![]);
        for name in [
            "Guide color 1 variation 1: #FF2020",
            "Save guide colors as swatches",
        ] {
            let (_, node) = keys::named(&output, name);
            let rect = node.bounds().unwrap();
            assert!(
                rect.y0 >= 0. && rect.y1 <= 172.,
                "{name} must be visible in the real short dock: {rect:?}"
            );
        }
        click(&ctx, &mut frame, "Color Guide options");
        click(&ctx, &mut frame, "Vivid / muted");
        frame(keys::key(Key::Escape, Modifiers::NONE));
        assert_eq!(
            ctx.data(|d| d.get_temp::<Guide>(owner)).unwrap().variation,
            Variation::VividMuted
        );
        drop(frame);
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    }

    #[test]
    fn craft_color_pane_body_can_open_another_pane_in_the_actual_workspace() {
        let ctx = context();
        ctx.enable_accesskit();
        super::super::theme::apply(&ctx);
        let mut studio = studio();
        let before = crate::project::encode(&studio.doc).unwrap();
        assert!(super::super::panel_workspace::open(
            &ctx,
            &mut studio,
            "color-guide"
        ));
        let mut frame = |events| {
            keys::render(&ctx, events, |ui| {
                super::super::panel_workspace::panels(ui, &mut studio);
            })
        };
        frame(vec![]);
        frame(vec![]);
        click(&ctx, &mut frame, "Edit harmony as a five-color theme");
        frame(vec![]);
        let output = frame(vec![]);
        keys::named(&output, "Selected theme color");
        drop(frame);
        let design = &studio.panel_workspaces.tasks["Design"];
        assert!(design.groups.iter().any(|group| {
            group
                .panels
                .get(group.active)
                .is_some_and(|p| p == "color-themes")
        }));
        assert!(
            design
                .groups
                .iter()
                .any(|group| group.panels.iter().any(|p| p == "layers"))
        );
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    }
}
