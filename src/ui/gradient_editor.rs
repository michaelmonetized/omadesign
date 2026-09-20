//! Compact, direct manipulation gradient editor for the Appearance studio.
use crate::{
    geom::Bounds,
    gradient::{Gradient, GradientKind, GradientStop},
};
use eframe::egui::{self, Color32, Sense, Stroke, Ui, pos2, vec2};

pub(super) fn editor(ui: &mut Ui, gradient: &mut Gradient, bounds: Bounds) -> bool {
    gradient.normalize();
    let mut changed = false;
    let id = ui.make_persistent_id("gradient-stop");
    let mut selected = ui
        .data(|d| d.get_temp::<usize>(id))
        .unwrap_or(0)
        .min(gradient.stops.len() - 1);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 32.), Sense::click());
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
            id.with(index),
            Sense::click_and_drag(),
        );
        ui.painter().circle_filled(center, 5., stop.color.to_egui());
        ui.painter().circle_stroke(
            center,
            5.,
            Stroke::new(
                if index == selected { 2. } else { 1. },
                if index == selected {
                    crate::ui::theme::accent()
                } else {
                    Color32::WHITE
                },
            ),
        );
        if hit.clicked() || hit.drag_started() {
            selected = index;
        }
        if hit.drag_started() {
            dragging = true;
        }
    }
    // Track the selected stop through re-sorting, including crossing another stop.
    let drag = if dragging {
        ui.input(|i| i.pointer.interact_pos())
            .map(|p| (selected, ((p.x - ramp.left()) / ramp.width()).clamp(0., 1.)))
    } else {
        None
    };
    dragging &= ui.input(|i| i.pointer.primary_down());
    ui.data_mut(|d| d.insert_temp(id.with("dragging"), dragging));
    if let Some((index, offset)) = drag {
        gradient.stops[index].offset = offset;
        let active = gradient.stops[index].clone();
        gradient.normalize();
        selected = gradient
            .stops
            .iter()
            .position(|s| *s == active)
            .unwrap_or(index);
        changed = true;
    }
    if response.clicked()
        && let Some(pointer) = response.interact_pointer_pos()
        && ramp.contains(pointer)
    {
        let offset = ((pointer.x - ramp.left()) / ramp.width()).clamp(0., 1.);
        let color = gradient.sample(offset);
        gradient.stops.push(GradientStop { offset, color });
        gradient.normalize();
        selected = gradient
            .stops
            .iter()
            .position(|s| s.offset == offset && s.color == color)
            .unwrap_or(0);
        changed = true;
    }
    response.on_hover_text("Click the gradient to add a stop. Drag a stop to move it.");
    ui.horizontal(|ui| {
        ui.label(format!("Stop {}", selected + 1));
        let stop = &mut gradient.stops[selected];
        changed |= crate::ui::color_picker::color_edit(ui, "stop-color", &mut stop.color);
        let mut position = stop.offset * 100.;
        if ui
            .add(
                egui::DragValue::new(&mut position)
                    .range(0.0..=100.0)
                    .speed(0.2)
                    .suffix("%"),
            )
            .changed()
        {
            stop.offset = position / 100.;
            changed = true;
        }
        if ui
            .add_enabled(gradient.stops.len() > 2, egui::Button::new("−"))
            .on_hover_text("Remove selected stop")
            .clicked()
        {
            gradient.stops.remove(selected);
            selected = selected.min(gradient.stops.len() - 1);
            changed = true;
        }
        if ui
            .button("+")
            .on_hover_text("Add stop halfway to the next stop")
            .clicked()
        {
            let offset = (gradient.stops[selected].offset
                + gradient
                    .stops
                    .get(selected + 1)
                    .map(|s| s.offset)
                    .unwrap_or(1.))
                * 0.5;
            let color = gradient.sample(offset);
            gradient.stops.push(GradientStop { offset, color });
            gradient.normalize();
            selected = gradient
                .stops
                .iter()
                .position(|s| s.offset == offset && s.color == color)
                .unwrap_or(0);
            changed = true;
        }
    });
    ui.horizontal(|ui| {
        if gradient.kind != GradientKind::Shape {
            ui.label("Angle");
            let mut angle = gradient.angle(bounds);
            if ui
                .add(egui::DragValue::new(&mut angle).speed(0.5).suffix("°"))
                .changed()
            {
                gradient.set_angle(angle, bounds);
                changed = true;
            }
        }
        if ui.button("Reverse").clicked() {
            for stop in &mut gradient.stops {
                stop.offset = 1. - stop.offset;
            }
            gradient.normalize();
            selected = gradient.stops.len() - selected - 1;
            changed = true;
        }
        if ui
            .button("Even")
            .on_hover_text("Distribute stops evenly")
            .clicked()
        {
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
    if changed {
        let active = gradient.stops[selected].clone();
        gradient.normalize();
        selected = gradient
            .stops
            .iter()
            .position(|s| *s == active)
            .unwrap_or(0);
    }
    ui.data_mut(|d| d.insert_temp(id, selected));
    changed
}
