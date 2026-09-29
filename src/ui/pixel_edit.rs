use crate::{
    app::{
        Studio, from_egui,
        pixel_edit::{Drag, Geometry, ModeKind},
        to_egui,
    },
    geom::{Anchor, Geom, Pt},
    tools::{Persona, Tool},
};
use eframe::egui::{self, PointerButton, Rect, Response, Stroke};

type Contours = std::sync::Arc<Vec<Vec<Pt>>>;
type OutlineJob = std::sync::Arc<std::sync::Mutex<Option<Contours>>>;
#[derive(Clone, Default)]
struct Outlines {
    generation: Option<u64>,
    lines: Contours,
    job: Option<(u64, OutlineJob)>,
}
pub fn outlines(
    ctx: &egui::Context,
    mask: &[u8],
    w: u32,
    h: u32,
    generation: u64,
    preview: bool,
    document: &str,
) -> Contours {
    let id = egui::Id::new(("pixel-ants", preview, document));
    let mut state = ctx.data(|d| d.get_temp::<Outlines>(id)).unwrap_or_default();
    let done = state
        .job
        .as_ref()
        .and_then(|(_, job)| job.lock().ok()?.take());
    if let Some(lines) = done {
        let (rendered, _) = state.job.take().unwrap();
        if rendered == generation {
            state.lines = lines;
            state.generation = Some(generation);
        }
    }
    if state.generation != Some(generation) && state.job.is_none() {
        let input = mask.to_vec();
        let job = std::sync::Arc::new(std::sync::Mutex::new(None));
        state.job = Some((generation, job.clone()));
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let step = ((w as f64 * h as f64 / 4_000_000.0).sqrt().ceil() as u32).max(1);
            let sw = w.div_ceil(step);
            let sh = h.div_ceil(step);
            let values = (0..sh)
                .flat_map(|y| {
                    let input = &input;
                    (0..sw).map(move |x| u8::from(input[(y * step * w + x * step) as usize] >= 128))
                })
                .collect::<Vec<_>>();
            let contours = crate::trace::trace_mask(&values, sw, sh)
                .into_iter()
                .map(|line| line.into_iter().map(|p| p * step as f32).collect())
                .collect();
            *job.lock().unwrap() = Some(std::sync::Arc::new(contours));
            repaint.request_repaint();
        });
    }
    let result = if state.generation == Some(generation) {
        state.lines.clone()
    } else {
        Default::default()
    };
    ctx.data_mut(|d| d.insert_temp(id, state));
    result
}

fn screen(
    studio: &Studio,
    rect: Rect,
    space: crate::app::masking::SelectionSpace,
    p: Pt,
) -> egui::Pos2 {
    let mut p = tiny_skia::Point::from_xy(p.x, p.y);
    space.transform.map_point(&mut p);
    to_egui(
        studio
            .view
            .world_to_window(from_egui(rect.min), Pt::new(p.x, p.y)),
    )
}
pub fn input(studio: &mut Studio, response: &Response, rect: Rect, panning: bool) -> bool {
    if studio.persona != Persona::Pixel {
        return false;
    }
    if studio.tool == Tool::BezierLasso && studio.pixel_edit.is_none() {
        studio.begin_pixel_edit(ModeKind::Bezier);
    }
    let Some(mut edit) = studio.pixel_edit.take() else {
        return false;
    };
    if edit.document != studio.swap_id || edit.generation != studio.pixel_sel_gen {
        return false;
    }
    let (pointer, pressed, down, released, mods, middle) = response.ctx.input(|i| {
        (
            i.pointer.interact_pos(),
            i.pointer.button_pressed(PointerButton::Primary),
            i.pointer.button_down(PointerButton::Primary),
            i.pointer.button_released(PointerButton::Primary),
            i.modifiers,
            i.pointer.button_down(PointerButton::Middle),
        )
    });
    if panning || middle {
        studio.pixel_edit = Some(edit);
        return false;
    }
    let world = pointer.and_then(|p| {
        let world = studio.view.to_world(from_egui(p) - from_egui(rect.min));
        let mut point = tiny_skia::Point::from_xy(world.x, world.y);
        edit.space.transform.invert()?.map_point(&mut point);
        Some(Pt::new(point.x, point.y))
    });
    let near = |a: Pt, p: egui::Pos2| screen(studio, rect, edit.space, a).distance(p) <= 9.0;
    if pressed
        && response.contains_pointer()
        && let (Some(p), Some(world)) = (pointer, world)
    {
        response.request_focus();
        let before = edit.geometry.clone();
        match &mut edit.geometry {
            Geometry::Cage(cage) => {
                // Skew uses the same linked sides as Object > Reshape, grabbed at
                // corners here so all four requested selection corners are usable.
                let handles = cage.corner_handles();
                let handle = handles.iter().position(|a| near(*a, p));
                let inside = cage
                    .mapper()
                    .and_then(|m| {
                        let mut bounds = cage.bounds;
                        for p in &handles {
                            bounds.union_pt(*p);
                        }
                        m.unmap(world, bounds)
                    })
                    .is_some_and(|p| cage.bounds.contains(p));
                if let Some(handle) = handle.or_else(|| inside.then_some(usize::MAX)) {
                    edit.drag = Some(Drag {
                        handle,
                        point: world,
                        before,
                    });
                }
            }
            Geometry::Path { anchors, closed } => {
                let node = anchors.iter().position(|a| near(a.pt, p));
                if *closed {
                    if let Some(i) = node {
                        if mods.alt {
                            if anchors.len() > 3 {
                                anchors.remove(i);
                            }
                        } else if mods.ctrl || mods.command {
                            if anchors[i].is_corner() {
                                let tangent = (anchors[(i + 1) % anchors.len()].pt
                                    - anchors[(i + anchors.len() - 1) % anchors.len()].pt)
                                    / 6.0;
                                anchors[i].h_out = tangent;
                                anchors[i].h_in = -tangent;
                            } else {
                                anchors[i].make_corner();
                            }
                        } else {
                            edit.drag = Some(Drag {
                                handle: i * 3,
                                point: world,
                                before: before.clone(),
                            });
                        }
                    } else if mods.shift {
                        let unit = (screen(studio, rect, edit.space, world + Pt::new(1.0, 0.0))
                            - screen(studio, rect, edit.space, world))
                        .length()
                        .max(0.001);
                        crate::geom::insert_anchor(anchors, true, world, 10.0 / unit);
                    } else {
                        let handle = anchors.iter().enumerate().find_map(|(i, a)| {
                            if !a.is_corner() && near(a.pt + a.h_in, p) {
                                Some(i * 3 + 1)
                            } else if !a.is_corner() && near(a.pt + a.h_out, p) {
                                Some(i * 3 + 2)
                            } else {
                                None
                            }
                        });
                        if let Some(handle) = handle {
                            edit.drag = Some(Drag {
                                handle,
                                point: world,
                                before: before.clone(),
                            });
                        }
                    }
                } else if node == Some(0) && anchors.len() >= 3 {
                    *closed = true;
                } else if anchors.len() < 4096 {
                    anchors.push(Anchor::corner(world));
                    edit.drag = Some(Drag {
                        handle: (anchors.len() - 1) * 3,
                        point: world,
                        before: before.clone(),
                    });
                }
                if edit.drag.is_none() {
                    edit.record(before);
                }
            }
        }
    }
    if let Some(drag) = edit.drag.clone() {
        if let Some(world) = world {
            let mut delta = world - drag.point;
            if mods.shift {
                if delta.x.abs() > delta.y.abs() {
                    delta.y = 0.0;
                } else {
                    delta.x = 0.0;
                }
            }
            match (&drag.before, &mut edit.geometry) {
                (Geometry::Cage(before), Geometry::Cage(cage)) => {
                    let next = if drag.handle == usize::MAX {
                        before.translated(delta)
                    } else if edit.mode == ModeKind::Transform {
                        before.resized(drag.handle, world - drag.point, mods.shift)
                    } else {
                        before.dragged(drag.handle, delta)
                    };
                    if let Some(next) = next {
                        *cage = next;
                    }
                }
                (
                    Geometry::Path {
                        anchors: old,
                        closed: was_closed,
                    },
                    Geometry::Path { anchors, closed },
                ) => {
                    let i = drag.handle / 3;
                    if !*was_closed && !*closed {
                        if let Some(a) = anchors.get_mut(i) {
                            crate::geom::apply_pen_smooth(
                                a,
                                world - drag.point,
                                studio.view.scale,
                                mods.alt,
                                mods.shift,
                            );
                        }
                    } else if let (Some(original), Some(a)) = (old.get(i), anchors.get_mut(i)) {
                        *a = original.clone();
                        match drag.handle % 3 {
                            0 => a.pt += delta,
                            1 => {
                                a.h_in += delta;
                                if !mods.alt {
                                    a.h_out = -a.h_in;
                                }
                            }
                            _ => {
                                a.h_out += delta;
                                if !mods.alt {
                                    a.h_in = -a.h_out;
                                }
                            }
                        }
                    }
                }
                _ => (),
            }
        }
        if released || !down {
            edit.drag = None;
            edit.record(drag.before);
        }
    }
    studio.pixel_edit = Some(edit);
    true
}

pub fn paint(p: &egui::Painter, rect: Rect, studio: &Studio) {
    let Some(edit) = &studio.pixel_edit else {
        return;
    };
    let screen = |point| screen(studio, rect, edit.space, point);
    let accent = super::theme::accent();
    let points = match &edit.geometry {
        Geometry::Cage(cage) => {
            if let Some(mapper) = cage.mapper() {
                for line in mapper.grid_lines(2, 24) {
                    p.add(egui::Shape::line(
                        line.into_iter().map(screen).collect(),
                        Stroke::new(1.0, accent.gamma_multiply(0.6)),
                    ));
                }
            }
            cage.corner_handles()
        }
        Geometry::Path { anchors, closed } => {
            for line in (Geom::Path {
                anchors: anchors.clone(),
                closed: *closed,
            })
            .contours(64)
            {
                p.add(egui::Shape::line(
                    line.into_iter().map(screen).collect(),
                    Stroke::new(1.3, accent),
                ));
            }
            for a in anchors {
                for delta in [a.h_in, a.h_out] {
                    if delta.length_sq() > 0.25 {
                        p.line_segment(
                            [screen(a.pt), screen(a.pt + delta)],
                            Stroke::new(1.0, accent.gamma_multiply(0.65)),
                        );
                        p.circle_filled(screen(a.pt + delta), 3.5, accent);
                    }
                }
            }
            anchors.iter().map(|a| a.pt).collect()
        }
    };
    for point in points {
        let r = Rect::from_center_size(screen(point), egui::vec2(9.0, 9.0));
        p.rect_filled(r, 2.0, super::theme::bg_panel());
        p.rect_stroke(r, 2.0, Stroke::new(1.5, accent), egui::StrokeKind::Inside);
    }
    let text = match edit.mode {
        ModeKind::Bezier => {
            "Bézier lasso · Enter closes · Shift-edge inserts · Alt-node deletes · Ctrl-node curves"
        }
        ModeKind::Transform => {
            "Move / resize selection · Drag inside or a corner · Shift keeps proportions · Enter finishes · Esc cancels"
        }
        ModeKind::Reshape(_) => "Reshape selection · Drag a handle · Enter finishes · Esc cancels",
    };
    p.text(
        rect.left_bottom() + egui::vec2(12.0, -12.0),
        egui::Align2::LEFT_BOTTOM,
        text,
        egui::FontId::proportional(11.0),
        accent,
    );
    if edit.pending() {
        p.ctx()
            .request_repaint_after(std::time::Duration::from_millis(16));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Modifiers, Pos2};
    fn fixture() -> Studio {
        let mut s = Studio::new();
        s.doc = crate::document::Document::new("Bezier gestures", 400., 300., 96.);
        s.doc.layers = vec![crate::document::Layer::raster("Pixels", 400, 300)];
        s.active_layer = Some(0);
        s.persona = Persona::Pixel;
        s.tool = Tool::BezierLasso;
        s.view.scale = 1.;
        s.view.offset = Pt::ZERO;
        s
    }
    fn frame(ctx: &egui::Context, s: &mut Studio, mut events: Vec<Event>, modifiers: Modifiers) {
        events.insert(0, Event::ModifiersChanged(modifiers));
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(500., 400.))),
                events,
                ..Default::default()
            },
            |ui| {
                let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(400., 300.));
                let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
                input(s, &response, rect, false);
            },
        );
        output.textures_delta.clear();
        s.poll_pixel_edit(ctx);
    }
    fn click(ctx: &egui::Context, s: &mut Studio, p: Pos2, mods: Modifiers) {
        frame(ctx, s, vec![Event::PointerMoved(p)], mods);
        frame(
            ctx,
            s,
            vec![Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: mods,
            }],
            mods,
        );
        frame(
            ctx,
            s,
            vec![Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: mods,
            }],
            mods,
        );
    }
    fn settle(ctx: &egui::Context, s: &mut Studio) {
        for _ in 0..200 {
            frame(ctx, s, vec![], Modifiers::NONE);
            if s.pixel_edit
                .as_ref()
                .is_some_and(|e| !e.pending() && e.job.is_none())
            {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        panic!("Render timeout");
    }
    fn anchors(s: &Studio) -> &[Anchor] {
        let Geometry::Path { anchors, .. } = &s.pixel_edit.as_ref().unwrap().geometry else {
            panic!()
        };
        anchors
    }
    #[test]
    fn real_pointer_closes_lasso_inserts_deletes_curves_and_drags_nodes() {
        let ctx = egui::Context::default();
        let mut s = fixture();
        let pixels = crate::project::encode(&s.doc).unwrap();
        frame(&ctx, &mut s, vec![], Modifiers::NONE);
        for (x, y) in [
            (80., 80.),
            (280., 80.),
            (280., 220.),
            (80., 220.),
            (80., 80.),
        ] {
            click(&ctx, &mut s, Pos2::new(x, y), Modifiers::NONE);
        }
        settle(&ctx, &mut s);
        assert!(s.pixel_edit.as_ref().unwrap().closed());
        assert_eq!(anchors(&s).len(), 4);
        assert!(s.pixel_sel.as_ref().unwrap()[150 * 400 + 150] > 240);
        click(
            &ctx,
            &mut s,
            Pos2::new(180., 80.),
            Modifiers {
                shift: true,
                ..Modifiers::NONE
            },
        );
        assert_eq!(anchors(&s).len(), 5);
        let was_corner = anchors(&s)[1].is_corner();
        click(
            &ctx,
            &mut s,
            Pos2::new(180., 80.),
            Modifiers {
                ctrl: true,
                command: true,
                ..Modifiers::NONE
            },
        );
        assert_ne!(anchors(&s)[1].is_corner(), was_corner);
        click(
            &ctx,
            &mut s,
            Pos2::new(180., 80.),
            Modifiers {
                ctrl: true,
                command: true,
                ..Modifiers::NONE
            },
        );
        assert_eq!(anchors(&s)[1].is_corner(), was_corner);
        click(
            &ctx,
            &mut s,
            Pos2::new(180., 80.),
            Modifiers {
                alt: true,
                ..Modifiers::NONE
            },
        );
        assert_eq!(anchors(&s).len(), 4);
        let p = Pos2::new(280., 80.);
        frame(
            &ctx,
            &mut s,
            vec![
                Event::PointerMoved(p),
                Event::PointerButton {
                    pos: p,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
            Modifiers::NONE,
        );
        let p = Pos2::new(310., 100.);
        frame(&ctx, &mut s, vec![Event::PointerMoved(p)], Modifiers::NONE);
        frame(
            &ctx,
            &mut s,
            vec![Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
            Modifiers::NONE,
        );
        settle(&ctx, &mut s);
        assert_eq!(anchors(&s)[1].pt, Pt::new(310., 100.));
        assert_eq!(crate::project::encode(&s.doc).unwrap(), pixels);
        assert!(s.pixel_edit_history(false));
        settle(&ctx, &mut s);
        assert_eq!(anchors(&s)[1].pt, Pt::new(280., 80.));
    }
    #[test]
    fn actual_ants_include_holes_and_non_rectangular_edges() {
        let ctx = egui::Context::default();
        let mut mask = vec![0; 100 * 100];
        for y in 10..90 {
            for x in 10..=y {
                mask[y * 100 + x] = 255;
            }
        }
        for y in 40..50 {
            for x in 20..30 {
                mask[y * 100 + x] = 0;
            }
        }
        let mut lines = outlines(&ctx, &mask, 100, 100, 1, false, "test");
        for _ in 0..100 {
            if !lines.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            lines = outlines(&ctx, &mask, 100, 100, 1, false, "test");
        }
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().any(|l| l.len() > 100));
    }
}
