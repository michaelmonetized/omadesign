//! Compact, direct manipulation gradient editor for the Appearance studio.
use crate::{
    geom::Bounds,
    gradient::{Gradient, GradientKind, GradientStop},
};
use eframe::egui::{self, Color32, Sense, Stroke, Ui, pos2, vec2};

#[derive(Clone, Default)]
struct Stops {
    selected: usize,
    identities: Vec<u64>,
    next: u64,
    expected: Vec<GradientStop>,
}

impl Stops {
    /// Follow native stop identities through sorting.
    /// Args: gradient. Returns after preserving even identical stops as separate controls.
    fn sort(&mut self, gradient: &mut Gradient) {
        let selected = self.identities[self.selected];
        let mut order: Vec<_> = (0..gradient.stops.len()).collect();
        order.sort_by(|&a, &b| {
            gradient.stops[a]
                .offset
                .total_cmp(&gradient.stops[b].offset)
        });
        gradient.stops = order.iter().map(|&i| gradient.stops[i].clone()).collect();
        self.identities = order.iter().map(|&i| self.identities[i]).collect();
        self.selected = self
            .identities
            .iter()
            .position(|&id| id == selected)
            .unwrap();
    }

    /// Add a stop through the existing sampled color semantics.
    /// Args: gradient and offset. Returns after selecting the inserted stop.
    fn add(&mut self, gradient: &mut Gradient, offset: f32) {
        let color = gradient.sample(offset);
        gradient.stops.push(GradientStop { offset, color });
        self.identities.push(self.next);
        self.next += 1;
        self.selected = gradient.stops.len() - 1;
        self.sort(gradient);
    }
}

pub(super) fn editor(ui: &mut Ui, gradient: &mut Gradient, bounds: Bounds) -> bool {
    gradient.normalize();
    let mut changed = false;
    if gradient.kind == GradientKind::Radial {
        ui.collapsing("Radial geometry", |ui| {
            for (label, value) in [
                ("Focus X", &mut gradient.focal[0]),
                ("Inner radius", &mut gradient.inner_radius),
            ] {
                ui.horizontal(|ui| {
                    ui.label(label);
                    changed |= ui.add(egui::DragValue::new(value).speed(0.01)).changed();
                });
            }
            ui.horizontal(|ui| {
                ui.label("Focus Y");
                changed |= ui
                    .add(egui::DragValue::new(&mut gradient.focal[1]).speed(0.01))
                    .changed();
            });
            if let Some(axis) = &mut gradient.radial_axis {
                ui.horizontal(|ui| {
                    ui.label("Ellipse axis");
                    for value in axis {
                        changed |= ui.add(egui::DragValue::new(value).speed(0.01)).changed();
                    }
                });
            }
            gradient.inner_radius = gradient.inner_radius.clamp(0., 0.9999);
        });
    }
    ui.horizontal(|ui| {
        ui.label("Spread");
        for mode in [
            crate::gradient::Spread::Pad,
            crate::gradient::Spread::Repeat,
            crate::gradient::Spread::Reflect,
        ] {
            changed |= ui
                .selectable_value(&mut gradient.spread, mode, mode.svg())
                .changed();
        }
    });
    let id = ui.make_persistent_id("gradient-stop");
    let mut stops = ui
        .data(|data| data.get_temp::<Stops>(id))
        .unwrap_or_default();
    if stops.expected != gradient.stops || stops.identities.len() != gradient.stops.len() {
        stops.selected = stops.selected.min(gradient.stops.len() - 1);
        stops.identities = (stops.next..stops.next + gradient.stops.len() as u64).collect();
        stops.next += gradient.stops.len() as u64;
    }
    let label = ui.label("Gradient stop");
    let mut chosen = false;
    let picker = egui::ComboBox::from_id_salt("gradient-stop-picker")
        .selected_text(format!(
            "Stop {} of {}",
            stops.selected + 1,
            gradient.stops.len()
        ))
        .show_ui(ui, |ui| {
            for (index, stop) in gradient.stops.iter().enumerate() {
                ui.push_id(stops.identities[index], |ui| {
                    let response = ui.selectable_label(
                        stops.selected == index,
                        format!(
                            "Stop {} · {:.2}% · {}",
                            index + 1,
                            stop.offset * 100.,
                            stop.color.hex()
                        ),
                    );
                    crate::ui::welcome::focus_control(ui, &response);
                    if response.clicked() {
                        stops.selected = index;
                        chosen = true;
                        ui.close();
                    }
                });
            }
        });
    if chosen {
        picker.response.request_focus();
    }
    crate::ui::welcome::name_control(ui, &picker.response, label.id, "Gradient stop");
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 32.), Sense::CLICK);
    let ramp = egui::Rect::from_min_max(rect.min, pos2(rect.max.x, rect.max.y - 9.));
    for y in 0..3 {
        for x in 0..((ramp.width() / 8.).ceil() as usize) {
            let tile = egui::Rect::from_min_size(
                ramp.min + vec2(x as f32 * 8., y as f32 * 8.),
                vec2(8., 8.),
            )
            .intersect(ramp);
            ui.painter().rect_filled(
                tile,
                0.,
                if (x + y) % 2 == 0 {
                    Color32::from_gray(175)
                } else {
                    Color32::from_gray(100)
                },
            );
        }
    }
    let steps = ramp.width().ceil() as usize;
    for i in 0..steps {
        let x = ramp.left() + i as f32;
        ui.painter().rect_filled(
            egui::Rect::from_min_max(pos2(x, ramp.top()), pos2(x + 1.1, ramp.bottom())),
            0.,
            gradient.sample(i as f32 / steps.max(1) as f32).to_egui(),
        );
    }
    ui.painter().rect_stroke(
        ramp,
        2.,
        Stroke::new(1., crate::ui::theme::border()),
        egui::StrokeKind::Inside,
    );
    let mut dragging = ui
        .data(|d| d.get_temp::<bool>(id.with("dragging")))
        .unwrap_or(false);
    for (index, stop) in gradient.stops.iter().enumerate() {
        let center = pos2(ramp.left() + stop.offset * ramp.width(), ramp.bottom() + 4.);
        let hit = ui.interact(
            egui::Rect::from_center_size(center, vec2(14., 17.)),
            id.with(stops.identities[index]),
            Sense::click_and_drag(),
        );
        hit.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Button,
                hit.enabled(),
                index == stops.selected,
                format!("Select gradient stop {}", index + 1),
            )
        });
        crate::ui::welcome::focus_control(ui, &hit);
        ui.painter().circle_filled(center, 5., stop.color.to_egui());
        ui.painter().circle_stroke(
            center,
            5.,
            Stroke::new(
                if index == stops.selected { 2. } else { 1. },
                if index == stops.selected {
                    crate::ui::theme::accent()
                } else {
                    Color32::WHITE
                },
            ),
        );
        if hit.clicked() || hit.drag_started() {
            stops.selected = index;
        }
        if hit.drag_started() {
            dragging = true;
        }
    }
    // Track the selected stop through re-sorting, including crossing another stop.
    let drag = if dragging {
        ui.input(|i| i.pointer.interact_pos()).map(|p| {
            (
                stops.selected,
                ((p.x - ramp.left()) / ramp.width()).clamp(0., 1.),
            )
        })
    } else {
        None
    };
    dragging &= ui.input(|i| i.pointer.primary_down());
    ui.data_mut(|d| d.insert_temp(id.with("dragging"), dragging));
    if let Some((index, offset)) = drag {
        gradient.stops[index].offset = offset;
        stops.sort(gradient);
        changed = true;
    }
    if response.clicked()
        && let Some(pointer) = response.interact_pointer_pos()
        && ramp.contains(pointer)
    {
        let offset = ((pointer.x - ramp.left()) / ramp.width()).clamp(0., 1.);
        stops.add(gradient, offset);
        changed = true;
    }
    response.on_hover_text("Click the gradient to add a stop. Drag a stop to move it.");
    ui.push_id(
        ("gradient-stop-properties", stops.identities[stops.selected]),
        |ui| {
            let stop = &mut gradient.stops[stops.selected];
            changed |= crate::ui::color_picker::color_edit(ui, "stop-color", &mut stop.color);
            let mut position = stop.offset * 100.;
            let response = crate::ui::fields::number(
                ui,
                "position",
                "Gradient stop position (%)",
                &mut position,
                0.0..=100.0,
                "%",
                None,
                Default::default(),
            );
            crate::ui::welcome::focus_control(ui, &response.response);
            if response.changed {
                stop.offset = position / 100.;
                changed = true;
            }
        },
    );
    stops.sort(gradient);
    ui.horizontal(|ui| {
        let remove = ui.add_enabled(
            gradient.stops.len() > 2,
            egui::Button::new("Remove gradient stop"),
        );
        crate::ui::welcome::focus_control(ui, &remove);
        if remove.clicked() {
            gradient.stops.remove(stops.selected);
            stops.identities.remove(stops.selected);
            stops.selected = stops.selected.min(gradient.stops.len() - 1);
            changed = true;
        }
        let add = ui
            .button("Add gradient stop")
            .on_hover_text("Add stop halfway to the next stop");
        crate::ui::welcome::focus_control(ui, &add);
        if add.clicked() {
            let offset = (gradient.stops[stops.selected].offset
                + gradient
                    .stops
                    .get(stops.selected + 1)
                    .map(|s| s.offset)
                    .unwrap_or(1.))
                * 0.5;
            stops.add(gradient, offset);
            changed = true;
        }
    });
    if gradient.kind != GradientKind::Shape {
        let mut angle = gradient.angle(bounds);
        let response = crate::ui::fields::number(
            ui,
            "angle",
            "Gradient angle (°)",
            &mut angle,
            -f32::MAX..=f32::MAX,
            "°",
            None,
            Default::default(),
        );
        crate::ui::welcome::focus_control(ui, &response.response);
        if response.changed {
            gradient.set_angle(angle, bounds);
            changed = true;
        }
    }
    ui.horizontal(|ui| {
        let reverse = ui.button("Reverse");
        crate::ui::welcome::focus_control(ui, &reverse);
        if reverse.clicked() {
            for stop in &mut gradient.stops {
                stop.offset = 1. - stop.offset;
            }
            stops.sort(gradient);
            changed = true;
        }
        let even = ui.button("Even").on_hover_text("Distribute stops evenly");
        crate::ui::welcome::focus_control(ui, &even);
        if even.clicked() {
            let n = gradient.stops.len() - 1;
            for (i, stop) in gradient.stops.iter_mut().enumerate() {
                stop.offset = i as f32 / n as f32;
            }
            changed = true;
        }
    });
    if gradient.kind == GradientKind::Shape {
        ui.small("Follows the object's silhouette, including holes.");
    } else {
        ui.small("Drag the Gradient tool on canvas to place the gradient.");
    }
    stops.expected = gradient.stops.clone();
    ui.data_mut(|data| data.insert_temp(id, stops));
    changed
}

#[cfg(test)]
mod keyboard_tests {
    use super::*;
    use crate::{color::Rgba, ui::welcome::keyboard_tests as keys};
    use egui::{Event, Key, Modifiers};

    fn frame(
        ctx: &egui::Context,
        gradient: &mut Gradient,
        owner: &str,
        enabled: bool,
        events: Vec<Event>,
    ) -> egui::FullOutput {
        keys::render(ctx, events, |ui| {
            ui.set_max_width(320.);
            ui.push_id(owner, |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    editor(
                        ui,
                        gradient,
                        Bounds::from_min_size(crate::geom::Pt::ZERO, crate::geom::Pt::splat(64.)),
                    );
                });
            });
        })
    }

    fn action(ctx: &egui::Context, gradient: &mut Gradient, label: &str) {
        keys::tab_to(ctx, label, |events| {
            frame(ctx, gradient, "document-A", true, events)
        });
        frame(
            ctx,
            gradient,
            "document-A",
            true,
            keys::key(Key::Enter, Modifiers::NONE),
        );
        frame(ctx, gradient, "document-A", true, vec![]);
    }

    #[test]
    fn keyboard_gradient_add_select_sort_and_duplicate_stops_keep_native_identity() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::ui::theme::apply(&ctx);
        let mut gradient = Gradient::new(GradientKind::Linear, Rgba::BLACK, Rgba::WHITE);
        action(&ctx, &mut gradient, "Select gradient stop 2");
        action(&ctx, &mut gradient, "Add gradient stop");
        action(&ctx, &mut gradient, "Add gradient stop");
        assert_eq!(gradient.stops.len(), 4);
        assert_eq!(
            gradient
                .stops
                .iter()
                .filter(|stop| stop.offset == 1. && stop.color == Rgba::WHITE)
                .count(),
            3
        );
        let output = keys::tab_to(&ctx, "Gradient stop position (%)", |events| {
            frame(&ctx, &mut gradient, "document-A", true, events)
        });
        keys::focused(&ctx, &output, "Gradient stop position (%)");
        let position = keys::named(&output, "Gradient stop position (%)").0;
        action(&ctx, &mut gradient, "Reverse");
        let output = keys::tab_to(&ctx, "Gradient stop position (%)", |events| {
            frame(&ctx, &mut gradient, "document-A", true, events)
        });
        assert_eq!(
            keys::named(&output, "Gradient stop position (%)").0,
            position
        );
        keys::replace("75", |events| {
            frame(&ctx, &mut gradient, "document-A", true, events)
        });
        let output = frame(&ctx, &mut gradient, "document-A", true, vec![]);
        keys::focused(&ctx, &output, "Gradient stop position (%)");
        assert_eq!(
            keys::named(&output, "Gradient stop position (%)").0,
            position
        );
        assert_eq!(
            gradient
                .stops
                .iter()
                .filter(|stop| stop.color == Rgba::WHITE && stop.offset == 0.)
                .count(),
            2
        );
        assert_eq!(
            gradient
                .stops
                .iter()
                .filter(|stop| stop.color == Rgba::WHITE && stop.offset == 0.75)
                .count(),
            1
        );
        let before = gradient.clone();
        frame(
            &ctx,
            &mut gradient,
            "document-A",
            false,
            vec![Event::Text("50".into())],
        );
        assert_eq!(gradient, before);
        let output = frame(&ctx, &mut gradient, "document-B", true, vec![]);
        assert_ne!(
            keys::named(&output, "Gradient stop position (%)").0,
            position
        );
    }
}
