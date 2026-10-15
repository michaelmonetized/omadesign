use super::*;
use crate::document::{Document, Layer, Shape, Style};
use crate::geom::Pt;
use eframe::egui::{self, Event, Key, Modifiers, OutputCommand, PointerButton, Pos2, Rect};

mod backend {
    use eframe::egui;
    include!("../../vendor/egui-winit/src/clipboard_commands.rs");
}

struct Seat {
    ctx: egui::Context,
    time: f64,
    labels: Vec<(String, Rect)>,
    copied: Vec<String>,
}

impl Default for Seat {
    fn default() -> Self {
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        Self {
            ctx,
            time: 0.,
            labels: Vec::new(),
            copied: Vec::new(),
        }
    }
}

impl Seat {
    fn frame(&mut self, studio: &mut Studio, events: Vec<Event>, color: bool) {
        self.time += 0.5;
        let mut output = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(320., 1000.))),
                time: Some(self.time),
                events,
                ..Default::default()
            },
            |ui| {
                studio.handle_shortcuts(ui.ctx());
                if color {
                    color_row(ui, studio, true);
                } else {
                    transform_studio(ui, studio, false);
                }
            },
        );
        self.labels.clear();
        fn visit(shape: &egui::Shape, out: &mut Vec<(String, Rect)>) {
            match shape {
                egui::Shape::Text(t) => out.push((
                    t.galley.text().into(),
                    t.galley.rect.translate(t.pos.to_vec2()),
                )),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        visit(shape, out);
                    }
                }
                _ => {}
            }
        }
        for shape in &output.shapes {
            visit(&shape.shape, &mut self.labels);
        }
        self.copied = output
            .platform_output
            .commands
            .iter()
            .filter_map(|command| match command {
                OutputCommand::CopyText(text) => Some(text.clone()),
                _ => None,
            })
            .collect();
        output.textures_delta.clear();
    }

    fn text(&self, text: &str) -> Pos2 {
        let number = text.parse::<f64>().ok();
        self.labels
            .iter()
            .find(|(t, _)| {
                t == text || number.is_some_and(|number| t.parse::<f64>().ok() == Some(number))
            })
            .unwrap_or_else(|| panic!("{text:?} missing: {:?}", self.labels))
            .1
            .center()
    }

    fn click(&mut self, studio: &mut Studio, pos: Pos2, color: bool) {
        for pressed in [true, false] {
            self.frame(
                studio,
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
                color,
            );
        }
        self.frame(studio, vec![], color);
    }
}

fn fixture() -> Studio {
    let mut studio = Studio::new();
    studio.doc = Document::new("Field editing", 640., 480., 72.);
    let mut layer = Layer::vector("Editable sizes");
    let shape = Shape::new(
        Geom::Rect {
            origin: Pt::new(10., 20.),
            size: Pt::new(120., 35.),
            radius: 0.,
        },
        Style {
            fill: Fill::Solid(crate::color::Rgba::from_hex(0xAA3311)),
            ..Default::default()
        },
    );
    studio.selection = vec![(0, shape.id)];
    layer.kind.shapes_mut().unwrap().push(shape);
    studio.doc.layers = vec![layer];
    studio.active_layer = Some(0);
    studio.show_welcome = false;
    studio.persona = Persona::Design;
    studio.tool = Tool::Select;
    studio
}

fn key(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn clipboard(key: Key, modifiers: Modifiers, text: &str) -> Vec<Event> {
    let mut events = Vec::new();
    assert!(backend::push_clipboard_command(
        &mut events,
        key,
        Some(key),
        modifiers,
        || Some(text.into())
    ));
    events
}

fn bounds(studio: &Studio) -> crate::geom::Bounds {
    studio
        .doc
        .find_shape(0, studio.selection[0].1)
        .unwrap()
        .geom
        .bbox()
}

#[test]
fn field_input_actual_size_drag_has_one_undo_and_redo() {
    let mut seat = Seat::default();
    let mut studio = fixture();
    seat.frame(&mut studio, vec![], false);
    let at = seat.text("120");
    seat.frame(
        &mut studio,
        vec![
            Event::PointerMoved(at),
            Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
        false,
    );
    for dx in [12., 24., 36., 48.] {
        seat.frame(
            &mut studio,
            vec![Event::PointerMoved(at + vec2(dx, 0.))],
            false,
        );
    }
    seat.frame(
        &mut studio,
        vec![Event::PointerButton {
            pos: at + vec2(48., 0.),
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
        false,
    );
    let final_bounds = bounds(&studio);
    assert!(final_bounds.width() > 120.);
    assert_eq!(studio.history.len(), 1);
    seat.frame(&mut studio, vec![key(Key::Z, Modifiers::CTRL)], false);
    assert_eq!(bounds(&studio).width(), 120.);
    seat.frame(
        &mut studio,
        vec![key(Key::Z, Modifiers::CTRL | Modifiers::SHIFT)],
        false,
    );
    assert_eq!(bounds(&studio), final_bounds);
}

#[test]
fn field_input_size_gesture_stays_with_its_selection_and_document() {
    let mut seat = Seat::default();
    let mut studio = fixture();
    let original = studio.doc.clone();
    let first = studio.selection[0];
    let mut second = studio.doc.find_shape(first.0, first.1).unwrap().clone();
    second.id += 100;
    let second_id = second.id;
    studio.doc.layers[0].kind.shapes_mut().unwrap().push(second);
    seat.frame(&mut studio, vec![], false);
    let at = seat.text("120");
    seat.frame(
        &mut studio,
        vec![
            Event::PointerMoved(at),
            Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
        false,
    );
    seat.frame(
        &mut studio,
        vec![Event::PointerMoved(at + vec2(20., 0.))],
        false,
    );
    let first_final = bounds(&studio);
    studio.selection = vec![(0, second_id)];
    seat.frame(
        &mut studio,
        vec![Event::PointerMoved(at + vec2(40., 0.))],
        false,
    );
    assert_eq!(
        bounds(&studio).width(),
        120.,
        "old drag cannot edit the new selection"
    );
    assert_eq!(studio.history.len(), 1);
    seat.frame(
        &mut studio,
        vec![Event::PointerButton {
            pos: at + vec2(40., 0.),
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
        false,
    );
    let at = seat.text("120");
    seat.frame(
        &mut studio,
        vec![
            Event::PointerMoved(at),
            Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
        false,
    );
    seat.frame(
        &mut studio,
        vec![Event::PointerMoved(at + vec2(20., 0.))],
        false,
    );
    seat.frame(
        &mut studio,
        vec![Event::PointerButton {
            pos: at + vec2(20., 0.),
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
        false,
    );
    assert_eq!(studio.history.len(), 2);
    studio.undo();
    assert_eq!(bounds(&studio).width(), 120.);
    assert_eq!(
        studio.doc.find_shape(first.0, first.1).unwrap().geom.bbox(),
        first_final
    );
    studio.new_tab();
    studio.doc = original;
    studio.selection = vec![first];
    studio.active_layer = Some(0);
    studio.show_welcome = false;
    seat.frame(&mut studio, vec![], false);
    assert_eq!(studio.history.len(), 0);
    let at = seat.text("120");
    seat.frame(
        &mut studio,
        vec![
            Event::PointerMoved(at),
            Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
        false,
    );
    seat.frame(
        &mut studio,
        vec![Event::PointerMoved(at + vec2(60., 0.))],
        false,
    );
    seat.frame(
        &mut studio,
        vec![Event::PointerButton {
            pos: at + vec2(60., 0.),
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
        false,
    );
    assert_eq!(studio.history.len(), 1);
    studio.undo();
    assert_eq!(bounds(&studio).width(), 120.);
    studio.switch_tab(0);
    studio.selection = vec![first];
    assert_eq!(bounds(&studio), first_final);
    studio.undo();
    assert_eq!(bounds(&studio).width(), 120.);
}

#[test]
fn field_input_numeric_text_redo_cannot_cross_selection_or_document_owners() {
    for another_document in [false, true] {
        for modifiers in [Modifiers::CTRL, Modifiers::MAC_CMD] {
            let mut seat = Seat::default();
            let mut studio = fixture();
            let original = studio.doc.clone();
            let first = studio.selection[0];
            let mut second = studio.doc.find_shape(first.0, first.1).unwrap().clone();
            second.id += 100;
            let second_id = second.id;
            studio.doc.layers[0].kind.shapes_mut().unwrap().push(second);
            seat.frame(&mut studio, vec![], false);
            let at = seat.text("35");
            seat.click(&mut studio, at, false);
            let first_field = seat.ctx.memory(|memory| memory.focused()).unwrap();
            seat.frame(&mut studio, vec![key(Key::A, modifiers)], false);
            seat.frame(&mut studio, clipboard(Key::V, modifiers, "75"), false);
            assert_eq!(bounds(&studio).height(), 75.);
            seat.frame(&mut studio, vec![key(Key::Z, modifiers)], false);
            assert_eq!(bounds(&studio).height(), 35.);
            if another_document {
                studio.new_tab();
                studio.doc = original;
                studio.selection = vec![first];
                studio.active_layer = Some(0);
                studio.show_welcome = false;
            } else {
                studio.selection = vec![(0, second_id)];
            }
            let history = studio.history.len();
            seat.frame(&mut studio, vec![], false);
            let at = seat.text("35");
            seat.click(&mut studio, at, false);
            assert_ne!(
                seat.ctx.memory(|memory| memory.focused()).unwrap(),
                first_field
            );
            seat.frame(
                &mut studio,
                vec![key(Key::Z, modifiers | Modifiers::SHIFT)],
                false,
            );
            assert_eq!(
                bounds(&studio).height(),
                35.,
                "foreign text redo must not restore 75 into this owner"
            );
            assert_eq!(studio.history.len(), history);
            seat.frame(&mut studio, vec![key(Key::A, modifiers)], false);
            seat.frame(&mut studio, clipboard(Key::V, modifiers, "90"), false);
            assert_eq!(bounds(&studio).height(), 90.);
            seat.frame(&mut studio, vec![key(Key::Z, modifiers)], false);
            assert_eq!(
                bounds(&studio).height(),
                35.,
                "new owner retains normal text Undo"
            );
        }
    }
}

#[test]
fn field_input_external_document_change_splits_a_live_size_gesture() {
    let mut seat = Seat::default();
    let mut studio = fixture();
    seat.frame(&mut studio, vec![], false);
    let at = seat.text("120");
    seat.frame(
        &mut studio,
        vec![
            Event::PointerMoved(at),
            Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
        false,
    );
    seat.frame(
        &mut studio,
        vec![Event::PointerMoved(at + vec2(20., 0.))],
        false,
    );
    let before_external = bounds(&studio);
    studio.doc.layers[0].name = "External layer edit".into();
    studio.mark();
    seat.frame(
        &mut studio,
        vec![Event::PointerMoved(at + vec2(40., 0.))],
        false,
    );
    assert_eq!(studio.history.len(), 2);
    studio.undo();
    assert_eq!(bounds(&studio), before_external);
    assert_eq!(studio.doc.layers[0].name, "External layer edit");
}

#[test]
fn field_input_actual_size_copy_cut_paste_and_local_undo_for_control_and_command() {
    for modifiers in [Modifiers::CTRL, Modifiers::MAC_CMD] {
        let mut seat = Seat::default();
        let mut studio = fixture();
        seat.frame(&mut studio, vec![], false);
        let at = seat.text("120");
        seat.click(&mut studio, at, false);
        seat.frame(&mut studio, vec![key(Key::A, modifiers)], false);
        seat.frame(&mut studio, clipboard(Key::C, modifiers, ""), false);
        assert_eq!(seat.copied.len(), 1, "selected source size");
        assert_eq!(seat.copied[0].parse::<f32>().unwrap(), 120.);
        let copied = seat.copied[0].clone();
        let at = seat.text("35");
        seat.click(&mut studio, at, false);
        seat.frame(&mut studio, vec![key(Key::A, modifiers)], false);
        seat.frame(&mut studio, clipboard(Key::V, modifiers, &copied), false);
        assert_eq!(bounds(&studio).height(), 120.);
        assert_eq!(bounds(&studio).width(), 120.);
        seat.frame(&mut studio, vec![key(Key::Z, modifiers)], false);
        assert_eq!(bounds(&studio).height(), 35., "field-local text undo");
        assert_eq!(
            studio.history.len(),
            1,
            "field undo must not consume document history"
        );
        seat.frame(
            &mut studio,
            vec![key(Key::Z, modifiers | Modifiers::SHIFT)],
            false,
        );
        assert_eq!(bounds(&studio).height(), 120.);
        seat.frame(&mut studio, vec![key(Key::A, modifiers)], false);
        seat.frame(&mut studio, clipboard(Key::V, modifiers, "120"), false);
        seat.frame(
            &mut studio,
            vec![key(Key::ArrowLeft, Modifiers::SHIFT)],
            false,
        );
        seat.frame(&mut studio, clipboard(Key::X, modifiers, ""), false);
        assert_eq!(seat.copied, ["0"], "cut selected digit only");
        assert_eq!(bounds(&studio).height(), 12.);
        seat.frame(&mut studio, clipboard(Key::V, modifiers, "0"), false);
        assert_eq!(bounds(&studio).height(), 120.);
        seat.frame(&mut studio, vec![key(Key::Enter, Modifiers::NONE)], false);
        studio.undo();
        assert_eq!(bounds(&studio).height(), 35.);
        studio.redo();
        assert_eq!(bounds(&studio).height(), 120.);
        assert_eq!(studio.doc.layers.len(), 1);
        assert_eq!(studio.doc.layers[0].kind.shapes().unwrap().len(), 1);
    }
}

#[test]
fn field_input_actual_hex_preserves_selection_browser_paste_and_text_undo() {
    for modifiers in [Modifiers::CTRL, Modifiers::MAC_CMD] {
        let mut seat = Seat::default();
        let mut studio = fixture();
        seat.frame(&mut studio, vec![], true);
        let at = seat.text("#AA3311");
        seat.click(&mut studio, at, true);
        seat.frame(&mut studio, vec![key(Key::A, modifiers)], true);
        seat.frame(&mut studio, clipboard(Key::V, modifiers, "#12abEF"), true);
        assert_eq!(
            inspected_style(&studio)
                .fill
                .clone()
                .solid_or(crate::color::Rgba::TRANSPARENT),
            crate::color::Rgba::from_hex(0x12ABEF)
        );
        seat.frame(
            &mut studio,
            vec![key(Key::ArrowLeft, Modifiers::SHIFT)],
            true,
        );
        seat.frame(&mut studio, clipboard(Key::C, modifiers, ""), true);
        assert_eq!(
            seat.copied,
            ["F"],
            "hex copy must respect selected text and preserve case"
        );
        seat.frame(&mut studio, clipboard(Key::X, modifiers, ""), true);
        assert_eq!(seat.copied, ["F"]);
        seat.frame(&mut studio, clipboard(Key::V, modifiers, "F"), true);
        let rect = seat
            .labels
            .iter()
            .find(|(text, _)| text == "#12abEF")
            .unwrap()
            .1;
        let end = egui::pos2(rect.right() + 1., rect.center().y);
        let start = egui::pos2(rect.left() - 1., rect.center().y);
        seat.frame(
            &mut studio,
            vec![
                Event::PointerMoved(end),
                Event::PointerButton {
                    pos: end,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
            true,
        );
        seat.frame(&mut studio, vec![Event::PointerMoved(start)], true);
        seat.frame(
            &mut studio,
            vec![Event::PointerButton {
                pos: start,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
            true,
        );
        seat.frame(&mut studio, clipboard(Key::C, modifiers, ""), true);
        assert_eq!(seat.copied, ["#12abEF"], "pointer text selection");
        seat.frame(&mut studio, clipboard(Key::V, modifiers, " #445566 "), true);
        assert_eq!(
            inspected_style(&studio)
                .fill
                .clone()
                .solid_or(crate::color::Rgba::TRANSPARENT),
            crate::color::Rgba::from_hex(0x445566)
        );
        seat.frame(&mut studio, vec![key(Key::Z, modifiers)], true);
        assert_eq!(
            inspected_style(&studio)
                .fill
                .clone()
                .solid_or(crate::color::Rgba::TRANSPARENT),
            crate::color::Rgba::from_hex(0x12ABEF)
        );
        assert_eq!(studio.doc.layers.len(), 1);
        assert_eq!(studio.doc.layers[0].kind.shapes().unwrap().len(), 1);
    }
}

#[test]
fn field_input_integrated_mixed_frames_keep_children_locks_and_gesture_endpoints() {
    for lock in 0..3 {
        let mut seat = Seat::default();
        let mut studio = fixture();
        let first = studio.selection[0].1;
        let mut second = studio.doc.find_shape(0, first).unwrap().clone();
        second.id += 100;
        second.geom = Geom::Rect {
            origin: Pt::new(200., 20.),
            size: Pt::new(80., 35.),
            radius: 0.,
        };
        second.locked = lock == 1;
        let second_id = second.id;
        studio.doc.layers[0].kind.shapes_mut().unwrap().push(second);
        studio.doc.layers[0].locked = lock == 2;
        studio.selection.push((0, second_id));
        let mut children = Vec::new();
        for id in [first, second_id] {
            let frame = studio.doc.find_shape_mut(0, id).unwrap();
            frame.layout.frame = true;
            let bounds = frame.geom.bbox();
            let mut child = Shape::new(
                Geom::Rect {
                    origin: bounds.min + Pt::new(10., 10.),
                    size: Pt::new(20., 20.),
                    radius: 0.,
                },
                Default::default(),
            );
            child.layout.parent = Some(id);
            child.layout.constraint_x = crate::layout::Constraint::Stretch;
            children.push((child.id, bounds.width()));
            studio.doc.layers[0].kind.shapes_mut().unwrap().push(child);
        }
        let initial = crate::project::encode(&studio.doc).unwrap();
        let mut snapshots = vec![initial.clone()];
        for _ in 0..2 {
            seat.frame(&mut studio, vec![], false);
            let label = seat
                .labels
                .iter()
                .find(|(text, _)| text == "Width")
                .unwrap()
                .1;
            let at = seat
                .labels
                .iter()
                .filter(|(_, rect)| rect.top() >= label.bottom())
                .min_by(|(_, a), (_, b)| a.top().total_cmp(&b.top()))
                .unwrap()
                .1
                .center();
            seat.frame(
                &mut studio,
                vec![
                    Event::PointerMoved(at),
                    Event::PointerButton {
                        pos: at,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: Modifiers::NONE,
                    },
                ],
                false,
            );
            for dx in [12., 24., 36.] {
                seat.frame(
                    &mut studio,
                    vec![Event::PointerMoved(at + vec2(dx, 0.))],
                    false,
                );
            }
            seat.frame(
                &mut studio,
                vec![Event::PointerButton {
                    pos: at + vec2(36., 0.),
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                }],
                false,
            );
            snapshots.push(crate::project::encode(&studio.doc).unwrap());
        }
        if lock != 0 {
            assert_eq!(snapshots[2], initial, "locked selection must not change");
            assert_eq!(studio.history.len(), 0);
            continue;
        }
        assert_eq!(studio.history.len(), 2);
        assert_ne!(snapshots[1], snapshots[2]);
        let width = studio.doc.find_shape(0, first).unwrap().geom.bbox().width();
        assert!(width > 120.);
        assert_eq!(
            studio
                .doc
                .find_shape(0, second_id)
                .unwrap()
                .geom
                .bbox()
                .width(),
            width
        );
        for (id, original_width) in children {
            let child_width = studio.doc.find_shape(0, id).unwrap().geom.bbox().width();
            assert!((child_width - (20. + width - original_width)).abs() < 0.01);
        }
        studio.undo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), snapshots[1]);
        studio.undo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), initial);
        studio.redo();
        studio.redo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), snapshots[2]);
    }
}
