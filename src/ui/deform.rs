use crate::app::{Studio, from_egui, to_egui};
use crate::deform::Mode;
use crate::geom::Pt;
use crate::ui::theme::{accent, accent_soft, bg_panel, fg_weak};
use eframe::egui::{
    self, Align2, FontId, Painter, PointerButton, Rect, Response, RichText, Stroke, Ui,
};

pub fn inspector(ui: &mut Ui, studio: &mut Studio) {
    let active = studio.deformation.as_ref().map(|session| session.cage.mode);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Reshape").strong().size(12.0));
        if active.is_some() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Done").clicked() {
                    studio.end_deform(false);
                }
            });
        }
    });
    ui.add_space(5.0);
    ui.add_enabled_ui(studio.can_deform(), |ui| {
        let width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
        for row in Mode::ALL.chunks(2) {
            ui.horizontal(|ui| {
                for mode in row {
                    let button = egui::Button::new(mode.label()).selected(active == Some(*mode));
                    if ui.add_sized([width, 27.0], button).clicked() {
                        if active == Some(*mode) {
                            studio.end_deform(false);
                        } else {
                            studio.begin_deform(*mode);
                        }
                    }
                }
            });
        }
    });
    if let Some(mode) = active {
        ui.add_space(6.0);
        ui.label(RichText::new(mode.hint()).size(11.0).color(fg_weak()));
        ui.label(
            RichText::new("Shift constrains · Enter finishes · Esc cancels this drag")
                .size(10.0)
                .color(fg_weak()),
        );
    }
    if studio.selection.iter().any(|(layer, id)| {
        studio
            .doc
            .find_shape(*layer, *id)
            .is_some_and(|shape| !matches!(shape.geom, crate::geom::Geom::Poly { .. }))
    }) {
        ui.label(
            RichText::new(
                "Moving a handle turns text and shapes into paths. Undo brings them back.",
            )
            .size(10.0)
            .color(fg_weak()),
        );
    }
}

/// Call before the canvas tool dispatcher. An active envelope owns primary
/// pointer gestures, while the canvas retains its normal pan and zoom handling.
pub fn input(studio: &mut Studio, response: &Response, rect: Rect, panning: bool) -> bool {
    if studio.deformation.is_none() {
        return false;
    }
    let panning = panning
        || response
            .ctx
            .input(|i| i.pointer.button_down(PointerButton::Middle))
        || response.dragged_by(PointerButton::Middle);
    let (pointer, pressed, down, released, shift) = response.ctx.input(|i| {
        (
            i.pointer.interact_pos(),
            i.pointer.button_pressed(PointerButton::Primary),
            i.pointer.button_down(PointerButton::Primary),
            i.pointer.button_released(PointerButton::Primary),
            i.modifiers.shift,
        )
    });
    let world = pointer.map(|p| studio.view.to_world(from_egui(p) - from_egui(rect.min)));
    if !panning
        && pressed
        && response.contains_pointer()
        && let (Some(pointer), Some(world)) = (pointer, world)
    {
        let handle = studio.deformation.as_ref().and_then(|session| {
            session
                .cage
                .handles()
                .iter()
                .enumerate()
                .filter_map(|(i, point)| {
                    let screen = to_egui(studio.view.world_to_window(from_egui(rect.min), *point));
                    let distance = screen.distance(pointer);
                    (distance <= 11.0).then_some((i, distance))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        });
        if let Some(handle) = handle {
            studio.deformation_drag_start(handle, world);
            response.request_focus();
        }
    }
    let dragging = studio
        .deformation
        .as_ref()
        .is_some_and(|session| session.dragging());
    if dragging {
        if !panning && let Some(world) = world {
            studio.deformation_drag_to(world, shift);
        }
        if released || !down {
            studio.deformation_drag_finish();
        }
    }
    !panning
}

/// Draw after ordinary canvas outlines, or hide ordinary selection handles
/// while this envelope is active to keep the two editing modes unambiguous.
pub fn paint(painter: &Painter, rect: Rect, studio: &Studio) {
    let Some(session) = &studio.deformation else {
        return;
    };
    let screen = |world: Pt| to_egui(studio.view.world_to_window(from_egui(rect.min), world));
    if let Some(mapper) = session.cage.mapper() {
        for (index, line) in mapper.grid_lines(2, 32).into_iter().enumerate() {
            let boundary = index % 3 != 1;
            let color = if boundary {
                accent()
            } else {
                accent().gamma_multiply(0.4)
            };
            painter.add(egui::Shape::line(
                line.into_iter().map(screen).collect(),
                Stroke::new(1.0, color),
            ));
        }
    }
    for (index, handle) in session.cage.handles().into_iter().enumerate() {
        let selected = session.active_handle() == Some(index);
        let point = screen(handle);
        painter.circle_filled(
            point,
            if selected { 6.0 } else { 4.5 },
            if selected { accent() } else { bg_panel() },
        );
        painter.circle_stroke(
            point,
            if selected { 6.0 } else { 4.5 },
            Stroke::new(1.5, accent()),
        );
    }
    let title = format!("{}  ·  Enter to finish", session.cage.mode.label());
    let font = FontId::proportional(11.0);
    let galley = painter.layout_no_wrap(title, font.clone(), accent());
    let size = galley.size() + egui::vec2(20.0, 12.0);
    let label = Rect::from_min_size(rect.left_bottom() + egui::vec2(16.0, -size.y - 14.0), size);
    painter.rect_filled(label, 7.0, bg_panel());
    painter.rect_stroke(
        label,
        7.0,
        Stroke::new(1.0, accent_soft()),
        egui::StrokeKind::Inside,
    );
    painter.text(
        label.center(),
        Align2::CENTER_CENTER,
        galley.text(),
        font,
        accent(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Document, Layer, Shape, Style};
    use crate::geom::Geom;
    use egui::{Event, Modifiers, Pos2, RawInput, Sense, pos2, vec2};

    fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) -> bool {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0));
        let mut owned = false;
        let mut output = ctx.run_ui(
            RawInput {
                events,
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ui| {
                let response = ui.interact(
                    rect,
                    egui::Id::new("test-deform-canvas"),
                    Sense::click_and_drag(),
                );
                owned = input(studio, &response, rect, false);
                paint(ui.painter(), rect, studio);
            },
        );
        output.textures_delta.clear();
        owned
    }

    fn pointer(at: Pos2, pressed: bool) -> Vec<Event> {
        vec![
            Event::PointerMoved(at),
            Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            },
        ]
    }

    fn studio(mode: Mode) -> Studio {
        let mut studio = Studio::new();
        studio.doc = Document::new("Handles", 400.0, 300.0, 72.0);
        studio.doc.layers = vec![Layer::vector("Shape")];
        let shape = Shape::new(
            Geom::Rect {
                origin: Pt::new(100.0, 100.0),
                size: Pt::new(100.0, 100.0),
                radius: 0.0,
            },
            Style::default(),
        );
        studio.selection = vec![(0, shape.id)];
        studio.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
        studio.begin_deform(mode);
        studio
    }

    #[test]
    fn real_pointer_events_drive_every_mode_and_commit_release_outside_canvas() {
        for mode in Mode::ALL {
            let ctx = egui::Context::default();
            let mut studio = studio(mode);
            let before = studio.doc.layers[0].kind.shapes().unwrap()[0].clone();
            frame(&ctx, &mut studio, vec![]);
            let at = to_egui(studio.deformation.as_ref().unwrap().cage.handles()[0]);
            frame(&ctx, &mut studio, pointer(at, true));
            assert!(studio.deformation.as_ref().unwrap().dragging(), "{mode:?}");
            frame(
                &ctx,
                &mut studio,
                vec![Event::PointerMoved(at - vec2(20.0, 15.0))],
            );
            assert_ne!(studio.doc.layers[0].kind.shapes().unwrap()[0], before);
            assert_eq!(studio.history.len(), 0, "a preview must not fill history");
            frame(&ctx, &mut studio, pointer(pos2(-20.0, -20.0), false));
            assert!(!studio.deformation.as_ref().unwrap().dragging());
            assert_eq!(
                studio.history.len(),
                1,
                "release outside must close the drag"
            );
        }
    }

    #[test]
    fn middle_button_panning_remains_available_while_reshaping() {
        let ctx = egui::Context::default();
        let mut studio = studio(Mode::Mesh);
        frame(&ctx, &mut studio, vec![]);
        let before = studio.doc.layers[0].kind.shapes().unwrap()[0].clone();
        let owned = frame(
            &ctx,
            &mut studio,
            vec![
                Event::PointerMoved(pos2(100.0, 100.0)),
                Event::PointerButton {
                    pos: pos2(100.0, 100.0),
                    button: PointerButton::Middle,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        assert!(
            !owned,
            "the regular canvas pan handler must receive the gesture"
        );
        assert!(!studio.deformation.as_ref().unwrap().dragging());
        assert_eq!(studio.doc.layers[0].kind.shapes().unwrap()[0], before);
        assert_eq!(studio.history.len(), 0);
    }

    #[test]
    fn forgiving_handle_hit_does_not_jump_and_shift_constrains_the_live_drag() {
        let ctx = egui::Context::default();
        let mut studio = studio(Mode::Distort);
        studio.snap.enabled = false;
        frame(&ctx, &mut studio, vec![]);
        frame(&ctx, &mut studio, pointer(pos2(104.0, 104.0), true));
        frame(
            &ctx,
            &mut studio,
            vec![Event::PointerMoved(pos2(74.0, 84.0))],
        );
        let corner = studio.deformation.as_ref().unwrap().cage.handles()[0];
        assert_eq!(corner, Pt::new(70.0, 80.0));
        frame(
            &ctx,
            &mut studio,
            vec![Event::ModifiersChanged(Modifiers::SHIFT)],
        );
        let delta = studio.deformation.as_ref().unwrap().cage.handles()[0] - Pt::new(100.0, 100.0);
        assert!((delta.x - delta.y).abs() < 0.001);
    }
}
