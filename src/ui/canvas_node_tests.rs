use super::tests::{canvas_frame, pen_input_studio, pen_pointer_button};
use super::*;
use crate::document::{Cmd, Shape, Style};
use eframe::egui::{Event, Modifiers};

fn fixture(rotation: f32) -> (eframe::egui::Context, Studio, u64) {
    let (ctx, mut studio) = pen_input_studio();
    studio.view.scale = 1.0;
    let mut shape = Shape::new(
        Geom::Path {
            anchors: vec![
                Anchor::corner(Pt::new(90.0, 90.0)),
                Anchor::smooth(Pt::new(220.0, 110.0), Pt::new(30.0, 20.0)),
                Anchor::corner(Pt::new(210.0, 190.0)),
                Anchor::corner(Pt::new(100.0, 175.0)),
            ],
            closed: true,
        },
        Style {
            fill: Fill::Linear {
                from: [0.0, 0.2],
                to: [1.0, 0.8],
                c0: crate::color::Rgba::rgb(26, 77, 230),
                c1: crate::color::Rgba::rgb(230, 77, 26),
            },
            ..Style::default()
        },
    );
    if let Geom::Path { anchors, .. } = &mut shape.geom {
        anchors[0].radius = 7.0;
    }
    let id = shape.id;
    studio.commit(Cmd::AddShape { layer: 0, shape });
    studio.selection = vec![(0, id)];
    studio.tool = Tool::Node;
    studio.node_sel = BTreeSet::from([0, 1]);
    let before = studio.doc.find_shape(0, id).unwrap().geom.clone();
    // Same command as the Inspector, while Node and its selection remain active.
    studio.commit(Cmd::SetGeom {
        layer: 0,
        id,
        before: before.clone(),
        after: before,
        rot_before: 0.0,
        rot_after: rotation,
    });
    canvas_frame(&ctx, &mut studio, vec![]);
    (ctx, studio, id)
}

fn close(a: Pt, b: Pt) {
    assert!((a - b).length() < 0.002, "{a:?} != {b:?}");
}

fn pointer_drag(
    ctx: &eframe::egui::Context,
    studio: &mut Studio,
    from: Pt,
    to: Pt,
    mods: Modifiers,
) {
    let rect = studio.canvas_rect.unwrap();
    let from = win(rect, studio.view, from);
    let to = win(rect, studio.view, to);
    canvas_frame(
        ctx,
        studio,
        vec![
            Event::PointerMoved(from),
            pen_pointer_button(from, true, mods),
        ],
    );
    // Deliberately leave the final few pixels for the release event.
    canvas_frame(
        ctx,
        studio,
        vec![
            Event::ModifiersChanged(mods),
            Event::PointerMoved(from.lerp(to, 0.8)),
        ],
    );
    canvas_frame(
        ctx,
        studio,
        vec![Event::PointerMoved(to), pen_pointer_button(to, false, mods)],
    );
}

#[test]
fn rotated_nodes_draw_pick_and_drag_in_one_world_frame() {
    for rotation in [std::f32::consts::FRAC_PI_2, std::f32::consts::PI, 0.37] {
        let (ctx, mut studio, id) = fixture(rotation);
        let before = studio.doc.find_shape(0, id).unwrap().clone();
        let anchors = before.world_anchors().unwrap();
        let history = studio.history.len();
        let pixels =
            compositor::render_view(&studio.doc, studio.view, 400, 300, Draft::none()).unwrap();
        let shapes = canvas_frame(&ctx, &mut studio, vec![]);
        for anchor in &anchors {
            let at = win(studio.canvas_rect.unwrap(), studio.view, anchor.pt);
            assert!(
                shapes
                    .iter()
                    .any(|item| matches!(&item.shape, eframe::egui::Shape::Rect(rect)
                if rect.fill == select() && rect.rect.center().distance(at) < 0.002
                    && (rect.rect.width() == 6.0 || rect.rect.width() == 8.0))),
                "node overlay must follow rotation"
            );
        }
        let handle = win(
            studio.canvas_rect.unwrap(),
            studio.view,
            anchors[1].pt + anchors[1].h_out,
        );
        assert!(
            shapes.iter().any(
                |item| matches!(&item.shape, eframe::egui::Shape::Circle(circle)
            if circle.radius == 3.0 && circle.center.distance(handle) < 0.002)
            ),
            "Bezier overlay must follow rotation"
        );
        node_press(&mut studio, anchors[0].pt, anchors[0].pt, false, false);
        end_drag(&mut studio, anchors[0].pt, false, false, false);
        assert_eq!(
            studio.history.len(),
            history,
            "selecting a rotated path is not an edit"
        );
        assert_eq!(studio.doc.find_shape(0, id).unwrap(), &before);
        assert_eq!(
            compositor::render_view(&studio.doc, studio.view, 400, 300, Draft::none())
                .unwrap()
                .data(),
            pixels.data(),
            "selection must preserve gradient and corner rendering"
        );

        let delta = Pt::new(-19.0, 12.0);
        pointer_drag(
            &ctx,
            &mut studio,
            anchors[0].pt,
            anchors[0].pt + delta,
            Modifiers::NONE,
        );
        let after = studio.doc.find_shape(0, id).unwrap().clone();
        let moved = after.world_anchors().unwrap();
        assert_eq!(studio.node_sel, BTreeSet::from([0, 1]));
        for index in 0..anchors.len() {
            close(
                moved[index].pt,
                anchors[index].pt + if index < 2 { delta } else { Pt::ZERO },
            );
            close(moved[index].h_out, anchors[index].h_out);
            assert_eq!(moved[index].radius, anchors[index].radius);
        }
        assert_eq!(after.rotation, rotation);
        assert_eq!(after.style, before.style);
        assert_eq!(studio.history.len(), history + 1);
        studio.undo();
        assert_eq!(studio.doc.find_shape(0, id).unwrap(), &before);
        studio.redo();
        assert_eq!(studio.doc.find_shape(0, id).unwrap(), &after);
    }
}

#[test]
fn rotated_bezier_handle_drag_and_shift_constraint_preserve_other_points() {
    for rotation in [std::f32::consts::FRAC_PI_2, std::f32::consts::PI, 0.37] {
        for modifiers in [
            Modifiers::NONE,
            Modifiers::ALT,
            Modifiers {
                alt: true,
                shift: true,
                ..Modifiers::NONE
            },
        ] {
            let (ctx, mut studio, id) = fixture(rotation);
            let before = studio.doc.find_shape(0, id).unwrap().clone();
            let anchors = before.world_anchors().unwrap();
            let from = anchors[1].pt + anchors[1].h_out;
            let target_vector = anchors[1].h_out + Pt::new(29.0, -19.0);
            pointer_drag(
                &ctx,
                &mut studio,
                from,
                anchors[1].pt + target_vector,
                modifiers,
            );
            let after = studio.doc.find_shape(0, id).unwrap().clone();
            let moved = after.world_anchors().unwrap();
            let expected = if modifiers.shift {
                crate::geom::constrain_45(target_vector)
            } else {
                target_vector
            };
            for (a, b) in anchors.iter().zip(&moved) {
                close(a.pt, b.pt);
            }
            close(moved[1].h_out, expected);
            close(
                moved[1].h_in,
                if modifiers.alt {
                    anchors[1].h_in
                } else {
                    -expected
                },
            );
            assert_eq!(after.rotation, rotation);
            studio.undo();
            assert_eq!(studio.doc.find_shape(0, id).unwrap(), &before);
            studio.redo();
            assert_eq!(studio.doc.find_shape(0, id).unwrap(), &after);
        }
    }
}

#[test]
fn rotated_segment_insertion_and_node_removal_preserve_the_outline() {
    for rotation in [std::f32::consts::FRAC_PI_2, std::f32::consts::PI] {
        let (_, mut studio, id) = fixture(rotation);
        let before = studio.doc.find_shape(0, id).unwrap().clone();
        let anchors = before.world_anchors().unwrap();
        let point = anchors[2].pt.lerp(anchors[3].pt, 0.5);
        let count = studio.history.len();
        node_click(&mut studio, point, false);
        let inserted = studio
            .doc
            .find_shape(0, id)
            .unwrap()
            .world_anchors()
            .unwrap();
        assert_eq!(inserted.len(), anchors.len() + 1);
        close(inserted[3].pt, point);
        for (old, new) in
            anchors
                .iter()
                .zip([&inserted[0], &inserted[1], &inserted[2], &inserted[4]])
        {
            close(old.pt, new.pt);
        }
        assert_eq!(studio.history.len(), count + 1);
        studio.delete_node();
        let removed = studio
            .doc
            .find_shape(0, id)
            .unwrap()
            .world_anchors()
            .unwrap();
        for (old, new) in anchors.iter().zip(removed) {
            close(old.pt, new.pt);
        }
        studio.undo();
        studio.undo();
        assert_eq!(studio.doc.find_shape(0, id).unwrap(), &before);

        studio.node_sel = BTreeSet::from([0]);
        studio.delete_node();
        let remaining = studio
            .doc
            .find_shape(0, id)
            .unwrap()
            .world_anchors()
            .unwrap();
        for (old, new) in anchors[1..].iter().zip(remaining) {
            close(old.pt, new.pt);
        }
    }
}

#[test]
fn rotate_gesture_turns_explicit_geometry_once_and_retains_node_selection() {
    for angle in [std::f32::consts::FRAC_PI_2, std::f32::consts::PI] {
        let (_, mut studio, id) = fixture(0.37);
        let before = studio.doc.find_shape(0, id).unwrap().clone();
        let anchors = before.world_anchors().unwrap();
        let center = before.geom.bbox().center() + Pt::new(11.0, 9.0);
        studio.op = Some(Op::Rotate {
            orig: snapshot(&studio),
            center,
            start_angle: 0.0,
        });
        continue_drag(
            &mut studio,
            center + Pt::new(70.0, 0.0).rotate(angle),
            false,
            false,
        );
        end_drag(&mut studio, center, false, false, false);
        let after = studio.doc.find_shape(0, id).unwrap().clone();
        for (a, b) in anchors.iter().zip(after.world_anchors().unwrap()) {
            close(b.pt, a.pt.rotate_about(center, angle));
            close(b.h_out, a.h_out.rotate(angle));
        }
        assert_eq!(studio.node_sel, BTreeSet::from([0, 1]));
        studio.undo();
        assert_eq!(studio.doc.find_shape(0, id).unwrap(), &before);
        studio.redo();
        assert_eq!(studio.doc.find_shape(0, id).unwrap(), &after);
    }
}

#[test]
fn canvas_context_flip_targets_the_clicked_object_and_preserves_multi_selection() {
    fn frame(
        ctx: &eframe::egui::Context,
        studio: &mut Studio,
        events: Vec<Event>,
    ) -> Vec<eframe::egui::epaint::ClippedShape> {
        let mut output = ctx.run_ui(
            eframe::egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 900.0))),
                events,
                ..Default::default()
            },
            |ui| show(ui, studio),
        );
        output.textures_delta.clear();
        output.shapes
    }
    for multi in [false, true] {
        let (ctx, mut studio, first) = fixture(0.37);
        let mut second = studio.doc.find_shape(0, first).unwrap().clone();
        second.id = crate::document::next_id();
        second.geom.translate(Pt::new(280.0, 30.0));
        let id = second.id;
        studio.commit(Cmd::AddShape {
            layer: 0,
            shape: second.clone(),
        });
        studio.selection = if multi {
            vec![(0, first), (0, id)]
        } else {
            vec![(0, first)]
        };
        studio.selected_layer = Some(0);
        studio.artboard_sel = vec![studio.doc.artboards[0].id];
        studio.tool = Tool::Select;
        let first_shape = studio.doc.find_shape(0, first).unwrap().clone();
        let history = studio.history.len();
        studio.dirty = false;
        frame(&ctx, &mut studio, vec![]);
        let at = win(
            studio.canvas_rect.unwrap(),
            studio.view,
            second.world_bbox().center(),
        );
        frame(
            &ctx,
            &mut studio,
            vec![
                Event::PointerMoved(at),
                Event::PointerButton {
                    pos: at,
                    button: PointerButton::Secondary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        frame(
            &ctx,
            &mut studio,
            vec![Event::PointerButton {
                pos: at,
                button: PointerButton::Secondary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
        );
        let menu = frame(&ctx, &mut studio, vec![]);
        assert_eq!(
            studio.history.len(),
            history,
            "opening a menu is not an edit"
        );
        assert!(!studio.dirty);
        assert_eq!(
            studio.selection,
            if multi {
                vec![(0, first), (0, id)]
            } else {
                vec![(0, id)]
            }
        );
        if !multi {
            assert!(studio.node_sel.is_empty());
            assert!(studio.artboard_sel.is_empty());
            assert!(studio.selected_layer.is_none());
        }
        let button = menu
            .iter()
            .find_map(|item| match &item.shape {
                eframe::egui::Shape::Text(text) if text.galley.job.text == "Flip horizontal" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .expect("visible Flip horizontal menu action");
        frame(
            &ctx,
            &mut studio,
            vec![
                Event::PointerMoved(button),
                pen_pointer_button(button, true, Modifiers::NONE),
            ],
        );
        frame(
            &ctx,
            &mut studio,
            vec![pen_pointer_button(button, false, Modifiers::NONE)],
        );
        assert_eq!(
            studio.history.len(),
            history + 1,
            "one undo for the whole flip"
        );
        assert_ne!(studio.doc.find_shape(0, id).unwrap(), &second);
        assert_eq!(
            studio.doc.find_shape(0, first).unwrap() == &first_shape,
            !multi
        );
        studio.undo();
        assert_eq!(studio.doc.find_shape(0, id).unwrap(), &second);
        assert_eq!(studio.doc.find_shape(0, first).unwrap(), &first_shape);
    }
}
