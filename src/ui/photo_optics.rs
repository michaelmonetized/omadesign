use super::slider;
use crate::photo::optics::OpticalSettings;
use eframe::egui::{RichText, Ui};

pub(super) fn controls(ui: &mut Ui, optics: &mut OpticalSettings) -> bool {
    let before = optics.clone();
    ui.collapsing("Lens and perspective", |ui| {
        ui.checkbox(&mut optics.enabled, "Enable corrections");
        ui.label(
            RichText::new("Manual calibration · saved with this photo")
                .small()
                .weak(),
        );
        ui.add_enabled_ui(optics.enabled, |ui| {
            ui.add(
                eframe::egui::TextEdit::singleline(&mut optics.profile.name)
                    .hint_text("Calibration name")
                    .char_limit(256),
            );
            ui.collapsing("Camera and lens", |ui| {
                ui.add(
                    eframe::egui::TextEdit::singleline(&mut optics.profile.camera)
                        .hint_text("Camera")
                        .char_limit(256),
                );
                ui.add(
                    eframe::egui::TextEdit::singleline(&mut optics.profile.lens)
                        .hint_text("Lens")
                        .char_limit(256),
                );
                slider(
                    ui,
                    "Focal length (mm)",
                    &mut optics.profile.focal_length,
                    0.,
                    3000.,
                );
                slider(ui, "Aperture", &mut optics.profile.aperture, 0., 128.);
            });
            slider(ui, "Distortion", &mut optics.profile.distortion[0], -1., 1.);
            ui.collapsing("Advanced calibration", |ui| {
                slider(
                    ui,
                    "Distortion r⁴",
                    &mut optics.profile.distortion[1],
                    -1.,
                    1.,
                );
                slider(
                    ui,
                    "Distortion r⁶",
                    &mut optics.profile.distortion[2],
                    -1.,
                    1.,
                );
                slider(
                    ui,
                    "Optical center X",
                    &mut optics.profile.center[0],
                    0.,
                    1.,
                );
                slider(
                    ui,
                    "Optical center Y",
                    &mut optics.profile.center[1],
                    0.,
                    1.,
                );
            });
            slider(
                ui,
                "Red fringe scale",
                &mut optics.profile.chromatic_aberration[0],
                -0.05,
                0.05,
            );
            slider(
                ui,
                "Blue fringe scale",
                &mut optics.profile.chromatic_aberration[1],
                -0.05,
                0.05,
            );
            slider(
                ui,
                "Corner light (stops)",
                &mut optics.profile.vignette[0],
                -4.,
                4.,
            );
            slider(
                ui,
                "Corner light r⁴",
                &mut optics.profile.vignette[1],
                -4.,
                4.,
            );
            slider(
                ui,
                "Horizontal perspective",
                &mut optics.perspective[0],
                -0.5,
                0.5,
            );
            slider(
                ui,
                "Vertical perspective",
                &mut optics.perspective[1],
                -0.5,
                0.5,
            );
            slider(
                ui,
                "Straighten (degrees)",
                &mut optics.straighten,
                -45.,
                45.,
            );
            slider(ui, "Scale to fill", &mut optics.scale, 0.25, 4.);
            if ui.button("Reset corrections").clicked() {
                *optics = Default::default();
            }
        });
    });
    *optics != before
}
