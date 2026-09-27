use crate::app::{
    Studio,
    selection::{Same, With},
};
use eframe::egui::{self, Ui};

/// Keep arrangement next to the selection properties in every drawing mode.
pub(super) fn arrange_panel(ui: &mut Ui, studio: &mut Studio) {
    use super::icons::{self, ph};
    use crate::align::{Align, Distribute};
    let selected = !studio.selection.is_empty() || studio.selected_layer.is_some();
    egui::CollapsingHeader::new("Arrange")
        .default_open(true)
        .show(ui, |ui| {
            ui.add_enabled_ui(selected, |ui| {
                egui::Grid::new("arrange-order")
                    .num_columns(2)
                    .spacing([6.0, 4.0])
                    .show(ui, |ui| {
                        if ui
                            .button("To front")
                            .on_hover_text("Ctrl+Shift+]")
                            .clicked()
                        {
                            studio.bring_to_front();
                        }
                        if ui.button("Forward").on_hover_text("Ctrl+]").clicked() {
                            studio.bring_forward();
                        }
                        ui.end_row();
                        if ui.button("To back").on_hover_text("Ctrl+Shift+[").clicked() {
                            studio.send_to_back();
                        }
                        if ui.button("Backward").on_hover_text("Ctrl+[").clicked() {
                            studio.send_backward();
                        }
                        ui.end_row();
                    });
            });

        });
    egui::CollapsingHeader::new("Align")
        .default_open(true)
        .show(ui, |ui| {
            let count = studio.alignment_item_count();
            ui.add_enabled_ui(studio.can_align_selection(), |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for (icon, label, alignment) in [
                        (ph::ALIGN_LEFT, "Align left", Align::Left),
                        (
                            ph::ALIGN_CENTER_H,
                            "Align horizontal centers",
                            Align::CenterX,
                        ),
                        (ph::ALIGN_RIGHT, "Align right", Align::Right),
                        (ph::ALIGN_TOP, "Align top", Align::Top),
                        (ph::ALIGN_CENTER_V, "Align vertical centers", Align::CenterY),
                        (ph::ALIGN_BOTTOM, "Align bottom", Align::Bottom),
                    ] {
                        let hint = if count <= 1 { format!("{label} to artboard") } else { format!("{label} · Shift: each to its artboard") };
                        if icons::icon_button(ui, icon, &hint, false) {
                            let reference = if ui.input(|i| i.modifiers.shift) { crate::align::AlignTo::EachToArtboard } else { crate::align::AlignTo::Auto };
                            studio.align_sel_to(alignment, reference);
                        }
                    }
                });
            }).response.on_disabled_hover_text("Select an object, group or layer to align");
            ui.add_enabled_ui(count >= 3, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (label, hint, distribution) in [
                        (
                            "Distribute H",
                            "Distribute horizontally",
                            Distribute::Horizontal,
                        ),
                        (
                            "Distribute V",
                            "Distribute vertically",
                            Distribute::Vertical,
                        ),
                    ] {
                        if ui.button(label).on_hover_text(hint).clicked() {
                            studio.distribute_sel(distribution);
                        }
                    }
                });
            });
        });
}

/// The same Transform submenu in the menubar, canvas, and object rows.
pub(super) fn transform_menu(ui: &mut Ui, enabled: bool) -> Option<bool> {
    ui.menu_button("Transform", |ui| {
        for (horizontal, label, shortcut) in [
            (true, "Flip Horizontal", "Shift+H"),
            (false, "Flip Vertical", "Shift+V"),
        ] {
            if ui
                .add_enabled(enabled, egui::Button::new(label).shortcut_text(shortcut))
                .on_disabled_hover_text(FLIP_DISABLED)
                .clicked()
            {
                ui.close();
                return Some(horizontal);
            }
        }
        None
    }).inner.flatten()
}

const FLIP_DISABLED: &str =
    "Select an unlocked vector object. Convert text to paths before flipping.";

/// One inspector home, beside Rotate.
pub(super) fn flip_icons(ui: &mut Ui, studio: &mut Studio) {
    use super::icons::{self, ph};
    ui.add_enabled_ui(studio.can_flip_selection(), |ui| {
        ui.horizontal(|ui| {
            for (horizontal, glyph, tip) in [
                (true, ph::FLIP_HORIZONTAL, "Flip horizontal (Shift+H)"),
                (false, ph::FLIP_VERTICAL, "Flip vertical (Shift+V)"),
            ] {
                if icons::tiny_icon(ui, glyph, tip, false) {
                    studio.flip_selection(horizontal);
                }
            }
        });
    }).response.on_disabled_hover_text(FLIP_DISABLED);
}

pub fn menu(ui: &mut Ui, studio: &mut Studio) {
    if studio.persona == crate::tools::Persona::Pixel {
        super::pixel_selection::menu(ui, studio);
        return;
    }
    if studio.persona == crate::tools::Persona::Photo {
        ui.add_enabled_ui(!studio.photo.is_batching(), |ui| {
            if ui
                .add(egui::Button::new("All photos").shortcut_text("Ctrl+A"))
                .clicked()
            {
                studio.photo.select_all_images();
                ui.close();
            }
            if ui
                .add_enabled(
                    studio.photo.selected_count() > 0,
                    egui::Button::new("Deselect all photos"),
                )
                .clicked()
            {
                studio.photo.deselect_all_images();
                ui.close();
            }
            if ui.button("Invert photo selection").clicked() {
                studio.photo.selection = (0..studio.photo.images.len())
                    .filter(|i| !studio.photo.selection.contains(i))
                    .collect();
                ui.close();
            }
        });
        return;
    }
    if ui.button("All objects  Ctrl+A").clicked() {
        studio.select_all();
        ui.close();
    }
    if ui
        .add_enabled(
            !studio.selection.is_empty(),
            egui::Button::new("Deselect all"),
        )
        .clicked()
    {
        studio.deselect_all();
        ui.close();
    }
    if ui.button("Invert selection").clicked() {
        studio.invert_selection();
        ui.close();
    }
    ui.separator();
    ui.add_enabled_ui(!studio.selection.is_empty(), |ui| {
        ui.menu_button("Same as selected", |ui| {
            for (label, property) in [
                ("Fill", Same::Fill),
                ("Stroke", Same::Stroke),
                ("Effects", Same::Effects),
            ] {
                if ui.button(label).clicked() {
                    studio.select_same(property);
                    ui.close();
                }
            }
        })
    });
    ui.menu_button("With", |ui| {
        for (label, property) in [
            ("A fill", With::Fill),
            ("A stroke", With::Stroke),
            ("Effects", With::Effects),
        ] {
            if ui.button(label).clicked() {
                studio.select_with(property);
                ui.close();
            }
        }
    });
    ui.menu_button("Without", |ui| {
        for (label, property) in [
            ("Fill", With::NoFill),
            ("Stroke", With::NoStroke),
            ("Effects", With::NoEffects),
        ] {
            if ui.button(label).clicked() {
                studio.select_with(property);
                ui.close();
            }
        }
    });
}

pub fn pathfinder_menu(ui: &mut Ui, studio: &mut Studio) {
    if ui
        .add_enabled(
            studio.selection_has_path(),
            egui::Button::new("Simplify"),
        )
        .on_hover_text("Fit each selected path. Corners stay sharp.")
        .clicked()
    {
        studio.simplify_selection();
        ui.close();
    }
    for operation in crate::boolean::BoolOp::all() {
        if ui
            .add_enabled(
                studio.selection.len() >= 2,
                egui::Button::new(operation.name()),
            )
            .clicked()
        {
            studio.pathfinder(operation);
            ui.close();
        }
    }
    if ui
        .add_enabled(studio.selection.len() >= 2, egui::Button::new("Divide"))
        .clicked()
    {
        studio.divide_selection();
        ui.close();
    }
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
