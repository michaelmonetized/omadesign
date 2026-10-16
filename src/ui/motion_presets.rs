use crate::app::Studio;
use crate::motion_presets::Preset;
use crate::ui::theme::{fg, fg_weak};
use eframe::egui::{self, RichText, Ui};

pub fn inspector(ui: &mut Ui, studio: &mut Studio) {
    let owner = egui::Id::new(("motion-preset-options", &studio.swap_id, &studio.selection));
    let available = !studio.file_dialog_pending() && !studio.updates.freezing;
    super::fields::native_scope(ui, owner, |ui| {
        ui.add_enabled_ui(available, |ui| controls(ui, studio));
    });
}

fn controls(ui: &mut Ui, studio: &mut Studio) {
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
    let owners = ["duration", "delay", "stagger", "intensity"].map(|name| ui.id().with(name));
    super::keyboard_controls::motion_number(
        ui,
        owners[0],
        ("Duration", "Motion preset duration (s)"),
        &mut studio.motion_preset_options.duration,
        0.08..=30.0,
        0.02,
        " s",
        Some(2),
    );
    let valid = owners
        .iter()
        .all(|owner| super::keyboard_controls::motion_number_is_valid(ui.ctx(), *owner));
    let mut chosen = None;
    let width = (ui.available_width() - ui.spacing().item_spacing.x) * 0.5;
    for row in Preset::ALL.chunks(2) {
        ui.horizontal(|ui| {
            for preset in row {
                let count = studio.motion_preset_count(*preset);
                let response = ui
                    .add_enabled_ui(count > 0 && valid, |ui| {
                        ui.add_sized([width, 28.0], egui::Button::new(preset.name()))
                    })
                    .inner;
                super::welcome::focus_control(ui, &response);
                if response.on_hover_text(preset.hint()).clicked() {
                    chosen = Some(*preset);
                }
            }
        });
    }
    ui.add_space(7.0);
    let options = &mut studio.motion_preset_options;
    egui::CollapsingHeader::new("Timing & energy").show(ui, |ui| {
        for (owner, label, value, range, suffix) in [
            (
                owners[1],
                ("Delay", "Motion preset delay (s)"),
                &mut options.delay,
                0.0..=60.0,
                " s",
            ),
            (
                owners[2],
                ("Stagger", "Motion preset stagger (s)"),
                &mut options.stagger,
                0.0..=5.0,
                " s",
            ),
            (
                owners[3],
                ("Intensity", "Motion preset intensity"),
                &mut options.intensity,
                0.1..=4.0,
                "×",
            ),
        ] {
            super::keyboard_controls::motion_number(
                ui,
                owner,
                label,
                value,
                range,
                0.02,
                suffix,
                Some(2),
            );
        }
        let response = ui.checkbox(&mut options.start_at_playhead, "Start at playhead");
        super::welcome::focus_control(ui, &response);
    });
    if let Some(preset) = chosen
        && owners
            .iter()
            .all(|owner| super::keyboard_controls::motion_number_is_valid(ui.ctx(), *owner))
    {
        studio.apply_motion_preset(preset);
    }
}
