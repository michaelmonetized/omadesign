use crate::app::Studio;
use crate::motion_presets::Preset;
use crate::ui::theme::{fg, fg_weak};
use eframe::egui::{self, RichText, Ui};

pub fn inspector(ui: &mut Ui, studio: &mut Studio) {
    ui.label(
        RichText::new("Make it move")
            .strong()
            .size(12.0)
            .color(fg()),
    );
    ui.label(
        RichText::new("Pick a motion. Tweak the keys.")
            .size(10.0)
            .color(fg_weak()),
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Duration").size(11.0).color(fg_weak()));
        ui.add(
            egui::DragValue::new(&mut studio.motion_preset_options.duration)
                .speed(0.02)
                .range(0.08..=30.0)
                .max_decimals(2)
                .suffix(" s"),
        );
    });
    let width = (ui.available_width() - ui.spacing().item_spacing.x) * 0.5;
    for row in Preset::ALL.chunks(2) {
        ui.horizontal(|ui| {
            for preset in row {
                let count = studio.motion_preset_count(*preset);
                let response = ui
                    .add_enabled_ui(count > 0, |ui| {
                        ui.add_sized([width, 28.0], egui::Button::new(preset.name()))
                    })
                    .inner;
                if response.on_hover_text(preset.hint()).clicked() {
                    studio.apply_motion_preset(*preset);
                }
            }
        });
    }
    ui.add_space(7.0);
    let options = &mut studio.motion_preset_options;
    egui::CollapsingHeader::new("Timing & energy").show(ui, |ui| {
        egui::Grid::new("motion-preset-options")
            .num_columns(2)
            .spacing([12.0, 5.0])
            .show(ui, |ui| {
                for (label, value, range, suffix) in [
                    ("Delay", &mut options.delay, 0.0..=60.0, " s"),
                    ("Stagger", &mut options.stagger, 0.0..=5.0, " s"),
                    ("Intensity", &mut options.intensity, 0.1..=4.0, "×"),
                ] {
                    ui.label(RichText::new(label).size(11.0).color(fg_weak()));
                    ui.add(
                        egui::DragValue::new(value)
                            .speed(0.02)
                            .range(range)
                            .max_decimals(2)
                            .suffix(suffix),
                    );
                    ui.end_row();
                }
            });
        ui.checkbox(&mut options.start_at_playhead, "Start at playhead");
    });
}
