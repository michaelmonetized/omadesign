//! Rulers and guides share one interaction path, independent of the active drawing tool.
use crate::app::{Studio, from_egui};
use crate::document::{Guide, RulerUnit};
use crate::geom::Pt;
use crate::ui::theme::{accent, bg_panel, border, fg_weak};
use eframe::egui::{
    self, Align2, CursorIcon, FontId, Id, Key, Modifiers, Painter, PointerButton, Pos2, Rect,
    Sense, Stroke, Ui, pos2, vec2,
};

pub const RULER_SIZE: f32 = 22.0;
const GUIDE_REACH: f32 = 5.0;

#[derive(Clone, Copy)]
enum Drag {
    Guide {
        index: Option<usize>,
        vertical: bool,
        world: Pt,
    },
    Origin {
        world: Pt,
    },
}

#[derive(Clone, Default)]
struct Interaction {
    document: String,
    selected: Option<usize>,
    drag: Option<Drag>,
    guide_grab_offset: f32,
}

fn state_id() -> Id {
    Id::new("ruler-guide-interaction")
}

fn state(ctx: &egui::Context, studio: &Studio) -> Interaction {
    let mut state = ctx
        .data(|d| d.get_temp::<Interaction>(state_id()))
        .unwrap_or_default();
    if state.document != studio.swap_id {
        state = Interaction {
            document: studio.swap_id.clone(),
            ..Default::default()
        };
    }
    if state.selected.is_some_and(|i| i >= studio.doc.guides.len())
        || !studio.doc.ruler.guides_visible
    {
        state.selected = None;
    }
    state
}

fn store(ctx: &egui::Context, state: Interaction) {
    ctx.data_mut(|d| d.insert_temp(state_id(), state));
}

fn content_rect(rect: Rect, rulers: bool) -> Rect {
    Rect::from_min_max(
        rect.min
            + if rulers {
                vec2(RULER_SIZE, RULER_SIZE)
            } else {
                egui::Vec2::ZERO
            },
        rect.max,
    )
}

fn screen(rect: Rect, studio: &Studio, world: Pt) -> Pos2 {
    let p = studio
        .view
        .world_to_window(Pt::new(rect.min.x, rect.min.y), world);
    pos2(p.x, p.y)
}

fn world(rect: Rect, studio: &Studio, point: Pos2) -> Pt {
    studio
        .view
        .pointer_to_world(from_egui(rect.min), from_egui(point))
}

fn guide_hit(studio: &Studio, rect: Rect, pointer: Pos2) -> Option<usize> {
    if !studio.doc.ruler.guides_visible || !content_rect(rect, studio.show_rulers).contains(pointer)
    {
        return None;
    }
    studio
        .doc
        .guides
        .iter()
        .enumerate()
        .filter_map(|(index, guide)| {
            let p = screen(rect, studio, Pt::new(guide.pos, guide.pos));
            let distance = if guide.vertical {
                (p.x - pointer.x).abs()
            } else {
                (p.y - pointer.y).abs()
            };
            (distance <= GUIDE_REACH).then_some((index, distance))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, _)| index)
}

/// Run before global shortcuts, so Delete on a selected guide cannot delete artwork.
pub fn handle_shortcuts(ctx: &egui::Context, studio: &mut Studio) {
    if studio.show_welcome
        || studio.pending_nav.is_some()
        || studio.type_edit.is_some()
        || ctx.memory(|m| {
            m.top_modal_layer().is_some()
                || m.focused().is_some_and(|id| id != Id::new("studio-canvas"))
        })
        || egui::Popup::is_any_open(ctx)
    {
        return;
    }
    let mut state = state(ctx, studio);
    if (state.drag.is_some() || state.selected.is_some())
        && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
    {
        state.drag = None;
        state.selected = None;
    } else if let Some(index) = state.selected
        && ctx.input_mut(|i| {
            i.consume_key(Modifiers::NONE, Key::Delete)
                || i.consume_key(Modifiers::NONE, Key::Backspace)
        })
    {
        studio.remove_guide(index);
        state.drag = None;
        state.selected = None;
    }
    store(ctx, state);
}

/// Returns true while a ruler or guide owns the pointer; canvas tools must then stand down.
pub fn handle_input(ui: &mut Ui, studio: &mut Studio, rect: Rect) -> bool {
    let ctx = ui.ctx().clone();
    let mut state = state(&ctx, studio);
    let (pointer, pressed, released, focus_lost) = ctx.input(|i| {
        (
            i.pointer.latest_pos(),
            i.pointer.button_pressed(PointerButton::Primary),
            i.pointer.button_released(PointerButton::Primary),
            i.events
                .iter()
                .any(|e| matches!(e, egui::Event::WindowFocused(false))),
        )
    });
    let modal_open = studio.pending_nav.is_some() || ctx.memory(|m| m.top_modal_layer().is_some());
    let pointer_available = !modal_open && !egui::Popup::is_any_open(&ctx);
    if focus_lost || modal_open {
        state.drag = None;
    }
    let corner = Rect::from_min_size(rect.min, vec2(RULER_SIZE, RULER_SIZE));
    let top = Rect::from_min_max(
        pos2(rect.min.x + RULER_SIZE, rect.min.y),
        pos2(rect.max.x, rect.min.y + RULER_SIZE),
    );
    let left = Rect::from_min_max(
        pos2(rect.min.x, rect.min.y + RULER_SIZE),
        pos2(rect.min.x + RULER_SIZE, rect.max.y),
    );
    let on_ruler = studio.show_rulers
        && pointer.is_some_and(|p| corner.contains(p) || top.contains(p) || left.contains(p));
    let hit = pointer.and_then(|p| guide_hit(studio, rect, p));
    let was_dragging = state.drag.is_some();
    if pressed && pointer_available && state.drag.is_none() && studio.op.is_none() {
        state.selected = None;
        state.guide_grab_offset = 0.0;
        if let Some(pointer) = pointer {
            let world = world(rect, studio, pointer);
            state.drag = if studio.show_rulers && corner.contains(pointer) {
                Some(Drag::Origin { world })
            } else if studio.show_rulers && top.contains(pointer) {
                Some(Drag::Guide {
                    index: None,
                    vertical: false,
                    world,
                })
            } else if studio.show_rulers && left.contains(pointer) {
                Some(Drag::Guide {
                    index: None,
                    vertical: true,
                    world,
                })
            } else if let Some(index) = hit {
                state.selected = Some(index);
                let guide = studio.doc.guides[index];
                state.guide_grab_offset =
                    guide.pos - if guide.vertical { world.x } else { world.y };
                Some(Drag::Guide {
                    index: Some(index),
                    vertical: guide.vertical,
                    world,
                })
            } else {
                None
            };
            if state.drag.is_some() {
                ctx.memory_mut(|m| m.request_focus(Id::new("studio-canvas")));
            }
        }
    }
    if let Some(drag) = &mut state.drag {
        if let Some(pointer) = pointer {
            let point = world(rect, studio, pointer);
            match drag {
                Drag::Guide {
                    world, vertical, ..
                } => {
                    *world = point;
                    if *vertical {
                        world.x += state.guide_grab_offset;
                    } else {
                        world.y += state.guide_grab_offset;
                    }
                }
                Drag::Origin { world } => *world = point,
            }
        }
        ctx.request_repaint();
    }
    if released && let Some(drag) = state.drag.take() {
        let inside = pointer.is_some_and(|p| content_rect(rect, studio.show_rulers).contains(p));
        match drag {
            Drag::Guide {
                index: Some(index),
                vertical,
                world,
            } => {
                if inside {
                    studio.move_guide(index, if vertical { world.x } else { world.y });
                } else {
                    studio.remove_guide(index);
                    state.selected = None;
                }
            }
            Drag::Guide {
                index: None,
                vertical,
                world,
            } if inside => {
                if !studio.doc.ruler.guides_visible {
                    studio.toggle_guides();
                }
                studio.add_guide(vertical, if vertical { world.x } else { world.y });
                state.selected = Some(studio.doc.guides.len() - 1);
            }
            Drag::Origin { world } if inside => studio.set_ruler_origin(world),
            _ => {}
        }
    }
    if studio.show_rulers {
        for (name, area, help) in [
            (
                "top-ruler",
                top,
                "Drag down to add a horizontal guide · right-click for ruler options",
            ),
            (
                "left-ruler",
                left,
                "Drag right to add a vertical guide · right-click for ruler options",
            ),
            (
                "ruler-origin",
                corner,
                "Drag to set ruler zero · double-click to reset",
            ),
        ] {
            let response = ui
                .interact(area, Id::new(name), Sense::click_and_drag())
                .on_hover_text(help);
            if name == "ruler-origin" && response.double_clicked() {
                studio.set_ruler_origin(Pt::ZERO);
            }
            response.context_menu(|ui| ruler_menu(ui, studio));
        }
    }
    if studio.doc.ruler.guides_visible {
        let content = content_rect(rect, studio.show_rulers);
        for index in 0..studio.doc.guides.len() {
            let guide = studio.doc.guides[index];
            let p = screen(rect, studio, Pt::new(guide.pos, guide.pos));
            let area = if guide.vertical {
                Rect::from_min_max(
                    pos2(p.x - GUIDE_REACH, content.min.y),
                    pos2(p.x + GUIDE_REACH, content.max.y),
                )
            } else {
                Rect::from_min_max(
                    pos2(content.min.x, p.y - GUIDE_REACH),
                    pos2(content.max.x, p.y + GUIDE_REACH),
                )
            }
            .intersect(content);
            if area.is_positive() {
                ui.interact(area, Id::new(("guide", index)), Sense::click_and_drag())
                    .on_hover_text("Drag to move · drag outside the canvas to remove · Delete removes a selected guide")
                    .context_menu(|ui| {
                        if ui.button("Remove guide").clicked() { studio.remove_guide(index); state.selected = None; ui.close(); }
                        ui.separator();
                        ruler_menu(ui, studio);
                    });
            }
            // A context-menu action may have removed this guide or cleared the list.
            if index + 1 >= studio.doc.guides.len() {
                break;
            }
        }
    }
    let owns = was_dragging
        || state.drag.is_some()
        || (studio.op.is_none() && (on_ruler || hit.is_some()));
    if owns {
        let cursor = match state.drag {
            Some(Drag::Origin { .. }) => CursorIcon::Crosshair,
            Some(Drag::Guide { vertical: true, .. }) => CursorIcon::ResizeHorizontal,
            Some(Drag::Guide {
                vertical: false, ..
            }) => CursorIcon::ResizeVertical,
            None => {
                if let Some(index) = hit.filter(|i| *i < studio.doc.guides.len()) {
                    if studio.doc.guides[index].vertical {
                        CursorIcon::ResizeHorizontal
                    } else {
                        CursorIcon::ResizeVertical
                    }
                } else {
                    CursorIcon::Crosshair
                }
            }
        };
        ctx.set_cursor_icon(cursor);
    }
    store(&ctx, state);
    owns
}

pub fn ruler_menu(ui: &mut Ui, studio: &mut Studio) {
    ui.label(egui::RichText::new("Ruler units").small().color(fg_weak()));
    for unit in RulerUnit::ALL {
        if ui
            .selectable_label(studio.doc.ruler.unit == unit, unit.label())
            .clicked()
        {
            studio.set_ruler_unit(unit);
            ui.close();
        }
    }
    ui.separator();
    if ui.button("Reset ruler zero").clicked() {
        studio.set_ruler_origin(Pt::ZERO);
        ui.close();
    }
    if ui
        .button(if studio.doc.ruler.guides_visible {
            "Hide guides  Ctrl+;"
        } else {
            "Show guides  Ctrl+;"
        })
        .clicked()
    {
        studio.toggle_guides();
        ui.close();
    }
    if ui
        .add_enabled(
            !studio.doc.guides.is_empty(),
            egui::Button::new("Clear guides"),
        )
        .clicked()
    {
        studio.clear_guides();
        ui.close();
    }
}

pub fn draw(painter: &Painter, rect: Rect, studio: &Studio) {
    let state = state(painter.ctx(), studio);
    let content = content_rect(rect, studio.show_rulers);
    let guides = painter.with_clip_rect(content);
    if studio.doc.ruler.guides_visible {
        for (index, guide) in studio.doc.guides.iter().enumerate() {
            if matches!(state.drag, Some(Drag::Guide { index: Some(i), .. }) if i == index) {
                continue;
            }
            draw_guide(
                &guides,
                rect,
                content,
                studio,
                *guide,
                state.selected == Some(index),
            );
        }
    }
    if let Some(Drag::Guide {
        vertical, world, ..
    }) = state.drag
    {
        draw_guide(
            &guides,
            rect,
            content,
            studio,
            Guide {
                vertical,
                pos: if vertical { world.x } else { world.y },
            },
            true,
        );
        let unit = studio.doc.ruler.unit;
        let pos = if vertical {
            world.x - studio.doc.ruler.origin.x
        } else {
            world.y - studio.doc.ruler.origin.y
        };
        let label = format!(
            "{:.2} {}",
            pos / unit.pixels_per_unit(studio.doc.dpi),
            unit.suffix()
        );
        let at = screen(rect, studio, world) + vec2(12.0, 12.0);
        let galley = painter.layout_no_wrap(label, FontId::proportional(11.0), accent());
        let size = galley.size() + vec2(12.0, 8.0);
        let at = pos2(
            at.x.clamp(content.min.x, (content.max.x - size.x).max(content.min.x)),
            at.y.clamp(content.min.y, (content.max.y - size.y).max(content.min.y)),
        );
        painter.rect_filled(Rect::from_min_size(at, size), 4.0, bg_panel());
        painter.galley(at + vec2(6.0, 4.0), galley, accent());
    }
    let origin = match state.drag {
        Some(Drag::Origin { world }) => world,
        _ => studio.doc.ruler.origin,
    };
    if matches!(state.drag, Some(Drag::Origin { .. })) {
        draw_guide(
            &guides,
            rect,
            content,
            studio,
            Guide {
                vertical: true,
                pos: origin.x,
            },
            true,
        );
        draw_guide(
            &guides,
            rect,
            content,
            studio,
            Guide {
                vertical: false,
                pos: origin.y,
            },
            true,
        );
    }
    if studio.show_rulers {
        draw_rulers(painter, rect, studio, origin);
    }
}

fn draw_guide(
    p: &Painter,
    rect: Rect,
    content: Rect,
    studio: &Studio,
    guide: Guide,
    selected: bool,
) {
    let at = screen(rect, studio, Pt::new(guide.pos, guide.pos));
    let line = if guide.vertical {
        [pos2(at.x, content.min.y), pos2(at.x, content.max.y)]
    } else {
        [pos2(content.min.x, at.y), pos2(content.max.x, at.y)]
    };
    if (guide.vertical && at.x >= content.min.x && at.x <= content.max.x)
        || (!guide.vertical && at.y >= content.min.y && at.y <= content.max.y)
    {
        p.line_segment(
            line,
            Stroke::new(
                if selected { 1.5 } else { 1.0 },
                accent().gamma_multiply(if selected { 1.0 } else { 0.65 }),
            ),
        );
    }
}

fn nice_step(raw: f32) -> f32 {
    let power = 10f32.powf(raw.max(1e-8).log10().floor());
    let n = raw / power;
    power
        * if n <= 1.0 {
            1.0
        } else if n <= 2.0 {
            2.0
        } else if n <= 5.0 {
            5.0
        } else {
            10.0
        }
}

fn tick_label(value: f32, step: f32) -> String {
    let decimals = (-step.log10().floor()).max(0.0) as usize;
    let value = if value.abs() < step * 0.01 {
        0.0
    } else {
        value
    };
    format!("{value:.decimals$}", decimals = decimals.min(6))
}

fn draw_rulers(p: &Painter, rect: Rect, studio: &Studio, ruler_origin: Pt) {
    let top = Rect::from_min_max(rect.min, pos2(rect.max.x, rect.min.y + RULER_SIZE));
    let left = Rect::from_min_max(rect.min, pos2(rect.min.x + RULER_SIZE, rect.max.y));
    p.rect_filled(top, 0.0, bg_panel());
    p.rect_filled(left, 0.0, bg_panel());
    let scale = studio.view.scale.max(0.001);
    let pixels = studio.doc.ruler.unit.pixels_per_unit(studio.doc.dpi);
    let step = nice_step(70.0 / (scale * pixels));
    let spacing = step * pixels * scale;
    let origin = screen(rect, studio, ruler_origin);
    let font = FontId::proportional(9.0);
    for vertical in [false, true] {
        let painter = p.with_clip_rect(if vertical { left } else { top });
        let (start, end, zero) = if vertical {
            (rect.min.y + RULER_SIZE, rect.max.y, origin.y)
        } else {
            (rect.min.x + RULER_SIZE, rect.max.x, origin.x)
        };
        let first = ((start - zero) / spacing).ceil();
        let count = ((end - start) / spacing).ceil() as usize + 1;
        for index in 0..count {
            let tick = first + index as f32;
            let at = zero + tick * spacing;
            if at > end {
                break;
            }
            let text = tick_label(tick * step, step);
            if vertical {
                painter.line_segment(
                    [
                        pos2(rect.min.x + RULER_SIZE - 5.0, at),
                        pos2(rect.min.x + RULER_SIZE, at),
                    ],
                    Stroke::new(1.0, border()),
                );
                let galley = p.layout_no_wrap(text, font.clone(), fg_weak());
                painter.add(
                    egui::epaint::TextShape::new(
                        pos2(rect.min.x + 3.0, at - 3.0),
                        galley,
                        fg_weak(),
                    )
                    .with_angle(-std::f32::consts::FRAC_PI_2),
                );
            } else {
                painter.line_segment(
                    [
                        pos2(at, rect.min.y + RULER_SIZE - 5.0),
                        pos2(at, rect.min.y + RULER_SIZE),
                    ],
                    Stroke::new(1.0, border()),
                );
                painter.text(
                    pos2(at + 3.0, rect.min.y + 3.0),
                    Align2::LEFT_TOP,
                    text,
                    font.clone(),
                    fg_weak(),
                );
            }
            for minor in 1..5 {
                let at = at - spacing * minor as f32 / 5.0;
                if at < start {
                    continue;
                }
                let line = if vertical {
                    [
                        pos2(rect.min.x + RULER_SIZE - 2.0, at),
                        pos2(rect.min.x + RULER_SIZE, at),
                    ]
                } else {
                    [
                        pos2(at, rect.min.y + RULER_SIZE - 2.0),
                        pos2(at, rect.min.y + RULER_SIZE),
                    ]
                };
                painter.line_segment(line, Stroke::new(1.0, border()));
            }
        }
    }
    let corner = Rect::from_min_size(rect.min, vec2(RULER_SIZE, RULER_SIZE));
    p.rect_filled(corner, 0.0, bg_panel());
    p.text(
        corner.center(),
        Align2::CENTER_CENTER,
        studio.doc.ruler.unit.suffix(),
        FontId::proportional(8.0),
        fg_weak(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, RawInput};

    fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) -> bool {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0));
        let mut owned = false;
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(rect),
                events,
                ..Default::default()
            },
            |ui| {
                handle_shortcuts(ui.ctx(), studio);
                studio.handle_shortcuts(ui.ctx());
                owned = handle_input(ui, studio, rect);
                draw(ui.painter(), rect, studio);
            },
        );
        output.textures_delta.clear();
        owned
    }

    fn button(x: f32, y: f32, pressed: bool) -> Vec<Event> {
        vec![
            Event::PointerMoved(pos2(x, y)),
            Event::PointerButton {
                pos: pos2(x, y),
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            },
        ]
    }

    fn setup() -> (egui::Context, Studio) {
        let ctx = egui::Context::default();
        let mut studio = Studio::new();
        studio.show_welcome = false;
        studio.view.scale = 1.0;
        studio.view.offset = Pt::ZERO;
        studio.doc.guides.clear();
        frame(&ctx, &mut studio, vec![]);
        (ctx, studio)
    }

    #[test]
    fn ruler_pointer_drags_create_move_remove_and_restore_guides() {
        let (ctx, mut studio) = setup();
        assert!(frame(&ctx, &mut studio, button(110.0, 10.0, true)));
        assert!(
            studio.doc.guides.is_empty(),
            "drag preview must not mutate history"
        );
        assert!(frame(
            &ctx,
            &mut studio,
            vec![Event::PointerMoved(pos2(110.0, 80.0))]
        ));
        frame(&ctx, &mut studio, button(110.0, 80.0, false));
        assert_eq!(
            studio.doc.guides,
            vec![Guide {
                vertical: false,
                pos: 80.0
            }]
        );
        assert_eq!(studio.history.len(), 1);
        frame(&ctx, &mut studio, button(10.0, 120.0, true));
        frame(&ctx, &mut studio, button(150.0, 120.0, false));
        assert_eq!(
            studio.doc.guides[1],
            Guide {
                vertical: true,
                pos: 150.0
            }
        );
        frame(&ctx, &mut studio, button(152.0, 180.0, true));
        frame(&ctx, &mut studio, button(152.0, 180.0, false));
        assert_eq!(
            studio.doc.guides[1].pos, 150.0,
            "a forgiving hit target must not shift a clicked guide"
        );
        frame(&ctx, &mut studio, button(152.0, 180.0, true));
        frame(&ctx, &mut studio, button(192.0, 180.0, false));
        assert_eq!(studio.doc.guides[1].pos, 190.0);
        studio.undo();
        assert_eq!(studio.doc.guides[1].pos, 150.0);
        frame(&ctx, &mut studio, button(150.0, 180.0, true));
        frame(&ctx, &mut studio, button(450.0, 180.0, false));
        assert_eq!(studio.doc.guides.len(), 1);
        studio.undo();
        assert_eq!(
            studio.doc.guides[1],
            Guide {
                vertical: true,
                pos: 150.0
            }
        );
    }

    #[test]
    fn origin_drag_and_escape_cancel_are_undoable_without_creating_guides() {
        let (ctx, mut studio) = setup();
        frame(&ctx, &mut studio, button(10.0, 10.0, true));
        frame(&ctx, &mut studio, button(120.0, 90.0, false));
        assert_eq!(studio.doc.ruler.origin, Pt::new(120.0, 90.0));
        assert!(studio.doc.guides.is_empty());
        studio.undo();
        assert_eq!(studio.doc.ruler.origin, Pt::ZERO);
        frame(&ctx, &mut studio, button(120.0, 10.0, true));
        frame(
            &ctx,
            &mut studio,
            vec![
                Event::PointerMoved(pos2(120.0, 80.0)),
                Event::Key {
                    key: Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        frame(&ctx, &mut studio, button(120.0, 80.0, false));
        assert!(studio.doc.guides.is_empty());
    }

    #[test]
    fn selected_guide_delete_does_not_delete_selected_artwork_and_hidden_guides_do_not_capture_pointer()
     {
        let (ctx, mut studio) = setup();
        studio.finish_create(
            crate::app::CreateKind::Rect,
            Pt::new(180.0, 120.0),
            Pt::new(220.0, 170.0),
        );
        let shape = studio.selection[0];
        studio.add_guide(true, 100.0);
        frame(&ctx, &mut studio, button(100.0, 160.0, true));
        frame(&ctx, &mut studio, button(100.0, 160.0, false));
        frame(
            &ctx,
            &mut studio,
            vec![Event::Key {
                key: Key::Delete,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        assert!(studio.doc.guides.is_empty());
        assert!(studio.doc.find_shape(shape.0, shape.1).is_some());
        studio.undo();
        studio.toggle_guides();
        assert!(!frame(
            &ctx,
            &mut studio,
            vec![Event::PointerMoved(pos2(100.0, 160.0))]
        ));
    }

    #[test]
    fn ruler_ticks_remain_bounded_at_tiny_zoom_and_use_fractional_units() {
        let (ctx, mut studio) = setup();
        studio.view.scale = 0.00001;
        studio.view.offset = Pt::new(-10_000.0, -10_000.0);
        studio.doc.ruler.unit = RulerUnit::Inches;
        studio.doc.dpi = 300.0;
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0));
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ui| draw(ui.painter(), rect, &studio),
        );
        output.textures_delta.clear();
        assert!(output.shapes.len() < 160);
        assert_eq!(tick_label(0.25, 0.05), "0.25");
        assert_eq!(tick_label(-0.00001, 0.2), "0.0");
    }
}
