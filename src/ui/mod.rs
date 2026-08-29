mod browsers;
mod chrome;
mod canvas;
mod icons;
mod photo;
mod studios;
pub mod theme;
mod welcome;

use crate::app::Studio;
use crate::tools::Persona;
use eframe::egui::Ui;

pub fn run(ui: &mut Ui, studio: &mut Studio) {
    let ctx = ui.ctx().clone();
    studio.handle_shortcuts(&ctx);

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
        .resizable(false)
        .anchor(eframe::egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ui.ctx(), |ui| {
            ui.label(crate::tools::shortcuts_markdown());
            if ui.button("Close").clicked() {
                studio.show_shortcuts = false;
            }
        });
}
