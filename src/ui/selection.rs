use crate::app::{
    Studio,
    selection::{Same, With},
};
use eframe::egui::{self, Ui};

pub fn menu(ui: &mut Ui, studio: &mut Studio) {
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
