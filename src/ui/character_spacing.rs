use crate::{
    app::Studio,
    geom::{CharSpan, KernMode, Leading},
};
use eframe::egui::{self, Ui};
fn common<T: PartialEq + Copy>(
    styles: &[CharSpan],
    get: impl Fn(&CharSpan) -> Option<T>,
) -> Option<T> {
    let first = get(styles.first()?);
    styles
        .iter()
        .all(|s| get(s) == first)
        .then_some(first)
        .flatten()
}
fn numeric(
    ui: &mut Ui,
    label: &str,
    value: Option<f32>,
    fallback: f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
) -> Option<f32> {
    let mut n = value.unwrap_or(fallback);
    ui.label(label).on_hover_text(if label == "Tracking" {
        "Letter spacing (tracking), in thousandths of an em"
    } else {
        label
    });
    let response = ui.add(
        egui::DragValue::new(&mut n)
            .range(range)
            .speed(1.)
            .suffix(suffix)
            .custom_formatter(move |v, _| {
                if value.is_none() {
                    "—".into()
                } else {
                    format!("{v:.1}")
                }
            }),
    );
    response.changed().then_some(n)
}
pub fn show(ui: &mut Ui, studio: &mut Studio) {
    let values = studio.selected_character_metrics();
    let run = studio
        .selected_type()
        .unwrap_or_else(|| studio.type_defaults());
    let lead = common(&values, |s| s.leading);
    let kern = common(&values, |s| s.kerning);
    egui::Grid::new("character_spacing")
        .num_columns(2)
        .spacing([8., 7.])
        .show(ui, |ui| {
            ui.label("Leading");
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("leading_mode")
                    .width(70.)
                    .selected_text(match lead {
                        Some(Leading::Auto(_)) => "Auto",
                        Some(Leading::Fixed(_)) => "Fixed",
                        None => "Mixed",
                    })
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(matches!(lead, Some(Leading::Auto(_))), "Auto")
                            .clicked()
                        {
                            studio.patch_character(|s| s.leading = Some(Leading::Auto(120.)));
                            restore_edit_focus(ui, studio);
                        }
                        if ui
                            .selectable_label(matches!(lead, Some(Leading::Fixed(_))), "Fixed")
                            .clicked()
                        {
                            studio.patch_character(|s| {
                                s.leading = Some(Leading::Fixed(run.line_height()))
                            });
                            restore_edit_focus(ui, studio);
                        }
                    });
                let mut number = match lead {
                    Some(Leading::Auto(v) | Leading::Fixed(v)) => v,
                    None => 120.,
                };
                if ui
                    .add(
                        egui::DragValue::new(&mut number)
                            .range(1.0..=1000.0)
                            .custom_formatter(move |value, _| {
                                if lead.is_none() {
                                    "—".into()
                                } else {
                                    format!("{value:.1}")
                                }
                            })
                            .suffix(if matches!(lead, Some(Leading::Auto(_))) {
                                " %"
                            } else {
                                " px"
                            }),
                    )
                    .changed()
                {
                    studio.patch_character(|s| {
                        s.leading = Some(if matches!(lead, Some(Leading::Auto(_))) {
                            Leading::Auto(number)
                        } else {
                            Leading::Fixed(number)
                        })
                    });
                }
            });
            ui.end_row();
            ui.label("Kerning");
            egui::ComboBox::from_id_salt("kerning_mode")
                .selected_text(match kern {
                    Some(KernMode::Metrics) => "Metrics",
                    Some(KernMode::Optical) => "Optical",
                    Some(KernMode::None) => "None",
                    None => "Mixed",
                })
                .show_ui(ui, |ui| {
                    for (name, mode) in [
                        ("Metrics", KernMode::Metrics),
                        ("Optical", KernMode::Optical),
                        ("None", KernMode::None),
                    ] {
                        if ui.selectable_label(kern == Some(mode), name).clicked() {
                            studio.patch_character(|s| s.kerning = Some(mode));
                            restore_edit_focus(ui, studio);
                        }
                    }
                });
            ui.end_row();
            if let Some(edit) = studio.type_edit.clone().filter(|e| {
                e.caret == e.anchor && e.caret > 0 && e.caret < run.content.chars().count()
            }) {
                if let Some(value) = numeric(
                    ui,
                    "Pair kerning",
                    Some(*run.manual_kern.get(&edit.caret).unwrap_or(&0.)),
                    0.,
                    -1000.0..=10000.0,
                    " /1000 em",
                ) {
                    studio.patch_type(|r| {
                        r.manual_kern.insert(edit.caret, value);
                    });
                }
                ui.end_row();
            }
            if let Some(value) = numeric(
                ui,
                "Tracking",
                common(&values, |s| s.tracking),
                0.,
                -1000.0..=10000.0,
                " /1000 em",
            ) {
                studio.patch_character(|s| s.tracking = Some(value));
            }
            ui.end_row();
            if let Some(value) = numeric(
                ui,
                "Vertical scale",
                common(&values, |s| s.vscale),
                100.,
                1.0..=1000.0,
                " %",
            ) {
                studio.patch_character(|s| s.vscale = Some(value));
            }
            ui.end_row();
            if let Some(value) = numeric(
                ui,
                "Horizontal scale",
                common(&values, |s| s.hscale),
                100.,
                1.0..=1000.0,
                " %",
            ) {
                studio.patch_character(|s| s.hscale = Some(value));
            }
            ui.end_row();
            if let Some(value) = numeric(
                ui,
                "Baseline shift",
                common(&values, |s| s.baseline_shift),
                0.,
                -1000.0..=1000.0,
                " px",
            ) {
                studio.patch_character(|s| s.baseline_shift = Some(value));
            }
            ui.end_row();
        });
}

fn restore_edit_focus(ui: &mut Ui, studio: &Studio) {
    if studio.type_edit.is_some() {
        ui.memory_mut(|m| m.request_focus(egui::Id::new("studio-canvas")));
    }
}
