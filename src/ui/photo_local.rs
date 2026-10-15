use super::slider;
use crate::{
    app::Studio,
    photo::local::{BrushStroke, LocalAdjustment, LocalMask, source_uv},
};
use eframe::egui::{self, Color32, Id, Rect, Response, Ui, pos2};

#[derive(Clone)]
struct State {
    selected: usize,
    paint: bool,
    overlay: bool,
    erase: bool,
    radius: f32,
    hardness: f32,
    flow: f32,
    stroke: Option<(usize, usize, usize)>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            selected: 0,
            paint: false,
            overlay: true,
            erase: false,
            radius: 0.05,
            hardness: 0.5,
            flow: 1.,
            stroke: None,
        }
    }
}
fn id() -> Id {
    Id::new("photo-local-adjustments")
}
fn read(ui: &Ui) -> State {
    ui.data(|d| d.get_temp(id()).unwrap_or_default())
}
fn write(ui: &Ui, state: State) {
    ui.data_mut(|d| d.insert_temp(id(), state));
}

pub(super) fn controls(ui: &mut Ui, adjustments: &mut Vec<LocalAdjustment>) -> bool {
    let before = adjustments.clone();
    let mut state = read(ui);
    ui.label("Local light and color");
    ui.label(
        egui::RichText::new("Masks stay editable in saved settings.")
            .small()
            .weak(),
    );
    ui.add_enabled_ui(adjustments.len() < 32, |ui| {
        ui.horizontal_wrapped(|ui| {
            for label in ["Radial", "Gradient", "Brush"] {
                if ui.button(format!("+ {label}")).clicked() {
                    let mask = match label {
                        "Gradient" => LocalMask::Gradient {
                            start: [0.5, 0.25],
                            end: [0.5, 0.75],
                        },
                        "Brush" => LocalMask::Brush { strokes: vec![] },
                        _ => LocalAdjustment::default().mask,
                    };
                    adjustments.push(LocalAdjustment {
                        name: format!("{label} {}", adjustments.len() + 1),
                        mask,
                        ..Default::default()
                    });
                    state.selected = adjustments.len() - 1;
                    state.paint = label == "Brush";
                    state.stroke = None;
                }
            }
        });
    });
    state.selected = state.selected.min(adjustments.len().saturating_sub(1));
    egui::ComboBox::from_id_salt("local-adjustment")
        .selected_text(
            adjustments
                .get(state.selected)
                .map_or("Choose a mask", |a| a.name.as_str()),
        )
        .show_ui(ui, |ui| {
            for (index, a) in adjustments.iter().enumerate() {
                if ui
                    .selectable_value(&mut state.selected, index, &a.name)
                    .changed()
                {
                    state.stroke = None;
                }
            }
        });
    if let Some(a) = adjustments.get_mut(state.selected) {
        ui.horizontal(|ui| {
            ui.checkbox(&mut a.enabled, "Enabled");
            ui.checkbox(&mut state.overlay, "Show mask");
        });
        ui.add(egui::TextEdit::singleline(&mut a.name).char_limit(256));
        if a.name.is_empty() {
            a.name = "Local adjustment".into();
        }
        ui.checkbox(&mut a.invert, "Invert mask");
        slider(ui, "Mask strength", &mut a.opacity, 0., 1.);
        ui.collapsing("Mask shape", |ui| match &mut a.mask {
            LocalMask::Radial {
                center,
                radius,
                angle,
                feather,
            } => {
                slider(ui, "Center X", &mut center[0], 0., 1.);
                slider(ui, "Center Y", &mut center[1], 0., 1.);
                slider(ui, "Radius X", &mut radius[0], 0.001, 2.);
                slider(ui, "Radius Y", &mut radius[1], 0.001, 2.);
                slider(ui, "Angle", angle, -180., 180.);
                slider(ui, "Feather", feather, 0., 1.);
            }
            LocalMask::Gradient { start, end } => {
                slider(ui, "Start X", &mut start[0], 0., 1.);
                slider(ui, "Start Y", &mut start[1], 0., 1.);
                slider(ui, "End X", &mut end[0], 0., 1.);
                slider(ui, "End Y", &mut end[1], 0., 1.);
                if *start == *end {
                    end[1] = (start[1] + 0.001).min(1.);
                    if *start == *end {
                        end[1] -= 0.001;
                    }
                }
            }
            LocalMask::Brush { strokes } => {
                ui.checkbox(&mut state.paint, "Paint on photo");
                ui.checkbox(&mut state.erase, "Erase");
                slider(ui, "Brush radius", &mut state.radius, 0.001, 0.5);
                slider(ui, "Hardness", &mut state.hardness, 0., 1.);
                slider(ui, "Flow", &mut state.flow, 0., 1.);
                ui.label(format!("{} editable strokes", strokes.len()));
                if ui.button("Undo last stroke").clicked() {
                    strokes.pop();
                    state.stroke = None;
                }
                if ui.button("Clear brush").clicked() {
                    strokes.clear();
                    state.stroke = None;
                }
            }
        });
        slider(ui, "Local exposure", &mut a.exposure, -8., 8.);
        slider(ui, "Local temperature", &mut a.temperature, -100., 100.);
        slider(ui, "Local tint", &mut a.tint, -100., 100.);
        slider(ui, "Local contrast", &mut a.contrast, 0., 4.);
        slider(ui, "Local saturation", &mut a.saturation, 0., 2.);
        slider(ui, "Local highlights", &mut a.highlights, -1., 1.);
        slider(ui, "Local shadows", &mut a.shadows, -1., 1.);
        ui.horizontal(|ui| {
            if ui.button("Remove mask").clicked() {
                adjustments.remove(state.selected);
                state.stroke = None;
            }
        });
    }
    write(ui, state);
    *adjustments != before
}

pub(super) fn viewer(
    ui: &Ui,
    studio: &mut Studio,
    dest: Rect,
    response: &Response,
    panning: bool,
) -> bool {
    let mut state = read(ui);
    let Some(image) = studio.photo.selected() else {
        return false;
    };
    let Some(adjustment) = image.develop.local_adjustments.get(state.selected) else {
        return false;
    };
    if studio.photo.show_original {
        return false;
    }
    let aspect = image.dimensions().0 as f32 / image.dimensions().1 as f32;
    if state.overlay && adjustment.enabled {
        let painter = ui.painter().with_clip_rect(response.rect);
        for y in 0..48 {
            for x in 0..64 {
                let point = source_uv(
                    [(x as f32 + 0.5) / 64., (y as f32 + 0.5) / 48.],
                    &image.develop,
                );
                let coverage = adjustment.mask.coverage(point, aspect);
                let coverage = adjustment.opacity
                    * if adjustment.invert {
                        1. - coverage
                    } else {
                        coverage
                    };
                if coverage > 0.01 {
                    let rect = Rect::from_min_max(
                        pos2(
                            dest.min.x + dest.width() * x as f32 / 64.,
                            dest.min.y + dest.height() * y as f32 / 48.,
                        ),
                        pos2(
                            dest.min.x + dest.width() * (x + 1) as f32 / 64.,
                            dest.min.y + dest.height() * (y + 1) as f32 / 48.,
                        ),
                    );
                    painter.rect_filled(
                        rect,
                        0.,
                        Color32::from_rgba_unmultiplied(255, 40, 80, (coverage * 90.) as u8),
                    );
                }
            }
        }
    }
    let painting = state.paint && matches!(adjustment.mask, LocalMask::Brush { .. }) && !panning;
    if !ui.is_enabled() || !painting {
        state.stroke = None;
        write(ui, state);
        return false;
    }
    let active = ui.input(|i| i.pointer.primary_down());
    let point = response
        .interact_pointer_pos()
        .filter(|p| dest.contains(*p))
        .map(|p| {
            source_uv(
                [
                    (p.x - dest.min.x) / dest.width(),
                    (p.y - dest.min.y) / dest.height(),
                ],
                &image.develop,
            )
        });
    let index = studio.photo.selected.unwrap();
    let before = image.develop.clone();
    if active && let Some(point) = point {
        let params = &mut studio.photo.images[index].develop;
        if let Some(LocalAdjustment {
            mask: LocalMask::Brush { strokes },
            ..
        }) = params.local_adjustments.get_mut(state.selected)
        {
            if ui.input(|i| i.pointer.primary_pressed())
                && strokes.len() < 1024
                && strokes.iter().map(|s| s.points.len()).sum::<usize>() < 32768
            {
                strokes.push(BrushStroke {
                    points: vec![point],
                    radius: state.radius,
                    hardness: state.hardness,
                    flow: state.flow,
                    erase: state.erase,
                });
                state.stroke = Some((index, state.selected, strokes.len() - 1));
            } else if let Some((photo, mask, stroke)) = state.stroke
                && photo == index
                && mask == state.selected
                && strokes.iter().map(|s| s.points.len()).sum::<usize>() < 32768
                && let Some(stroke) = strokes.get_mut(stroke)
            {
                if stroke
                    .points
                    .last()
                    .is_some_and(|last| (last[0] - point[0]).hypot(last[1] - point[1]) > 0.001)
                {
                    stroke.points.push(point);
                }
            }
        }
        studio.photo.record_edit(before, true);
    }
    if !active {
        state.stroke = None;
    }
    write(ui, state);
    painting
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Context, Event, Modifiers, PointerButton, RawInput, Vec2};

    #[test]
    fn painting_records_editable_strokes_and_undo_redo() {
        let ctx = Context::default();
        let mut studio = Studio::new();
        studio.photo.import_image(
            "mask.png".into(),
            crate::photo::RgbaImage::new(80, 60, [80, 100, 120, 255].repeat(4800)).unwrap(),
        );
        studio.photo.images[0]
            .develop
            .local_adjustments
            .push(LocalAdjustment {
                mask: LocalMask::Brush { strokes: vec![] },
                exposure: 1.,
                ..Default::default()
            });
        let frame = |studio: &mut Studio, events: Vec<Event>| {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0., 0.), Vec2::new(400., 300.))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    let dest = Rect::from_min_max(pos2(20., 20.), pos2(380., 280.));
                    let response = ui.interact(
                        dest,
                        Id::new("test-local-paint"),
                        egui::Sense::click_and_drag(),
                    );
                    let mut state = read(ui);
                    state.paint = true;
                    state.overlay = false;
                    write(ui, state);
                    viewer(ui, studio, dest, &response, false);
                },
            );
            output.textures_delta.clear();
        };
        frame(&mut studio, vec![]);
        frame(
            &mut studio,
            vec![
                Event::PointerMoved(pos2(150., 140.)),
                Event::PointerButton {
                    pos: pos2(150., 140.),
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        frame(&mut studio, vec![Event::PointerMoved(pos2(250., 160.))]);
        frame(
            &mut studio,
            vec![Event::PointerButton {
                pos: pos2(250., 160.),
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
        );
        studio.photo.finish_edit();
        let painted = studio.photo.images[0].develop.clone();
        let LocalMask::Brush { strokes } = &painted.local_adjustments[0].mask else {
            panic!("brush")
        };
        assert_eq!(strokes.len(), 1);
        assert!(strokes[0].points.len() >= 2);
        studio.photo.undo();
        let LocalMask::Brush { strokes } =
            &studio.photo.images[0].develop.local_adjustments[0].mask
        else {
            panic!("brush")
        };
        assert!(strokes.is_empty());
        studio.photo.redo();
        assert_eq!(studio.photo.images[0].develop, painted);
    }
}
