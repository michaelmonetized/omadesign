use crate::app::Studio;
use crate::presets;
use crate::ui::theme::{ACCENT, BG_EXTREME, BG_WIDGET, FG_WEAK};
use eframe::egui::{Align, Color32, Layout, RichText, ScrollArea, Stroke, Ui, vec2};

pub fn show(ui: &mut Ui, studio: &mut Studio) {
    ui.vertical_centered(|ui| {
        ui.add_space(48.0);
        ui.label(RichText::new("omadesign").size(36.0).color(ACCENT).strong());
        ui.label(
            RichText::new("design  ·  paint  ·  photograph")
                .size(14.0)
                .color(FG_WEAK),
        );
        ui.add_space(8.0);
        ui.label(
            RichText::new("A native Linux studio. Shortcuts match what you already know.")
                .size(13.0)
                .color(FG_WEAK),
        );
        ui.add_space(28.0);
    });

    ui.horizontal(|ui| {
        ui.add_space(ui.available_width() * 0.08);
        ui.vertical(|ui| {
            ui.set_max_width(920.0);
            ui.horizontal(|ui| {
                if ui
                    .add_sized(vec2(160.0, 36.0), eframe::egui::Button::new("Open…  Ctrl+O"))
                    .clicked()
                {
                    studio.open();
                }
                if ui
                    .add_sized(vec2(200.0, 36.0), eframe::egui::Button::new("Load demo document"))
                    .clicked()
                {
                    studio.seed_demo();
                }
                if ui
                    .add_sized(vec2(180.0, 36.0), eframe::egui::Button::new("Photo samples"))
                    .clicked()
                {
                    studio.show_welcome = false;
                    studio.persona = crate::tools::Persona::Photo;
                    studio.photo.import_samples();
                }
            });
            ui.add_space(18.0);
            ui.label(RichText::new("New document").strong().size(15.0));
            ui.add_space(8.0);
            ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                for group in presets::groups() {
                    ui.add_space(6.0);
                    ui.label(RichText::new(group).small().color(FG_WEAK));
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        for p in presets::all().iter().filter(|p| p.group == group) {
                            let btn = eframe::egui::Button::new(
                                RichText::new(format!(
                                    "{}\n{:.0} × {:.0}",
                                    p.name, p.w, p.h
                                ))
                                .size(12.0),
                            )
                            .fill(BG_WIDGET)
                            .stroke(Stroke::new(1.0, Color32::from_rgb(0x2C, 0x31, 0x3A)));
                            if ui.add_sized(vec2(150.0, 52.0), btn).clicked() {
                                studio.new_from_preset(*p);
                            }
                        }
                    });
                }
            });
            ui.add_space(16.0);
            ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                ui.label(
                    RichText::new("F1 always shows the key list. Space pans. Ctrl+scroll zooms.")
                        .small()
                        .color(FG_WEAK),
                );
            });
        });
    });
    let _ = BG_EXTREME;
}
