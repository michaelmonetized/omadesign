mod browsers;
mod chrome;
mod canvas;
mod icons;
mod photo;
mod studios;
mod timeline;
pub mod theme;
mod welcome;

use crate::app::Studio;
use crate::tools::Persona;
use eframe::egui::Ui;

pub fn run(ui: &mut Ui, studio: &mut Studio) {
    let ctx = ui.ctx().clone();
    studio.handle_shortcuts(&ctx);
    studio.tick_motion(&ctx);

    chrome::top_bar(ui, studio);

    if studio.show_welcome {
        welcome::show(ui, studio);
        chrome::status_bar(ui, studio);
        return;
    }

    if studio.persona == Persona::Photo {
        chrome::left_toolbar(ui, studio);
        photo::show(ui, studio);
        chrome::status_bar(ui, studio);
        return;
    }

    chrome::left_toolbar(ui, studio);
    studios::right_panel(ui, studio);
    chrome::status_bar(ui, studio);
    timeline::show(ui, studio);
    canvas::show(ui, studio);

    browsers::show_shape_browser(ui, studio);
    browsers::show_asset_browser(ui, studio);

    if studio.show_shortcuts {
        egui_shortcuts(ui, studio);
    }
}

fn egui_shortcuts(ui: &mut Ui, studio: &mut Studio) {
    eframe::egui::Window::new("Keys")
        .collapsible(false)
        .resizable(true)
        .default_size([640.0, 560.0])
        .anchor(eframe::egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ui.ctx(), |ui| {
            eframe::egui::ScrollArea::vertical().show(ui, |ui| {
                let groups = crate::tools::shortcut_groups();
                ui.columns(2, |cols| {
                    let mid = groups.len().div_ceil(2);
                    for (col, slice) in [(0, &groups[..mid]), (1, &groups[mid..])] {
                        for (group, rows) in slice.iter() {
                            cols[col].add_space(6.0);
                            cols[col].label(
                                eframe::egui::RichText::new(*group)
                                    .strong()
                                    .color(crate::ui::theme::accent()),
                            );
                            eframe::egui::Grid::new(*group)
                                .num_columns(2)
                                .spacing([16.0, 3.0])
                                .show(&mut cols[col], |ui| {
                                    for row in *rows {
                                        ui.label(row.action);
                                        ui.label(
                                            eframe::egui::RichText::new(row.keys)
                                                .monospace()
                                                .small()
                                                .color(crate::ui::theme::fg_weak()),
                                        );
                                        ui.end_row();
                                    }
                                });
                        }
                    }
                });
            });
            ui.add_space(8.0);
            if ui.button("Close").clicked() {
                studio.show_shortcuts = false;
            }
        });
}
