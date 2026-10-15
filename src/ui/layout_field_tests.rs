use super::*;
use crate::document::{Document, Layer};
use crate::geom::Pt;
use crate::layout_components as components;
use eframe::egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, vec2};

struct Seat {
    ctx: egui::Context,
    time: f64,
    labels: Vec<(String, Rect)>,
}

impl Default for Seat {
    fn default() -> Self {
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        Self {
            ctx,
            time: 0.,
            labels: Vec::new(),
        }
    }
}

impl Seat {
    fn frame(&mut self, studio: &mut Studio, events: Vec<Event>, properties: bool) {
        self.time += 0.1;
        let mut output = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900., 1800.))),
                time: Some(self.time),
                events,
                ..Default::default()
            },
            |ui| {
                studio.handle_shortcuts(ui.ctx());
                if properties {
                    super::super::layout_properties::inspector(ui, studio);
                } else {
                    inspector(ui, studio);
                }
            },
        );
        fn visit(shape: &egui::Shape, labels: &mut Vec<(String, Rect)>) {
            match shape {
                egui::Shape::Text(t) => labels.push((
                    t.galley.text().into(),
                    t.galley.rect.translate(t.pos.to_vec2()),
                )),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| visit(shape, labels)),
                _ => {}
            }
        }
        self.labels.clear();
        output
            .shapes
            .iter()
            .for_each(|shape| visit(&shape.shape, &mut self.labels));
        output.textures_delta.clear();
    }

    fn at(&self, value: &str) -> Pos2 {
        let number = value
            .split_once(' ')
            .and_then(|(number, unit)| number.parse::<f64>().ok().map(|number| (number, unit)));
        self.labels
            .iter()
            .find(|(text, rect)| {
                text == value
                    || number.is_some_and(|(number, unit)| {
                        text.trim().parse::<f64>().ok() == Some(number)
                            && self.labels.iter().any(|(suffix, suffix_rect)| {
                                suffix.trim() == unit
                                    && (suffix_rect.left() - rect.right()).abs() < 1.
                                    && (suffix_rect.center().y - rect.center().y).abs() < 1.
                            })
                    })
            })
            .unwrap_or_else(|| panic!("Missing {value:?}: {:?}", self.labels))
            .1
            .center()
    }

    fn pointer(&mut self, studio: &mut Studio, at: Pos2, pressed: bool, properties: bool) {
        self.frame(
            studio,
            vec![
                Event::PointerMoved(at),
                Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
            properties,
        );
    }

    fn click(&mut self, studio: &mut Studio, value: &str, properties: bool) {
        let at = self.at(value);
        self.pointer(studio, at, true, properties);
        self.pointer(studio, at, false, properties);
        self.frame(studio, vec![], properties);
    }

    fn drag(&mut self, studio: &mut Studio, at: Pos2, properties: bool) {
        let unrelated = studio.doc.layers[0]
            .kind
            .shapes()
            .unwrap()
            .iter()
            .find(|shape| shape.name == "Unrelated artwork")
            .unwrap()
            .clone();
        self.pointer(studio, at, true, properties);
        for x in [12., 24., 36., 48.] {
            self.frame(
                studio,
                vec![Event::PointerMoved(at + vec2(x, 0.))],
                properties,
            );
        }
        self.pointer(studio, at + vec2(48., 0.), false, properties);
        assert_eq!(studio.doc.find_shape(0, unrelated.id), Some(&unrelated));
    }
}

fn fixture() -> (Studio, u64) {
    let mut studio = Studio::new();
    studio.doc = Document::new("Layout field input", 900., 600., 96.);
    studio.doc.layers = vec![Layer::vector("Layout")];
    studio.active_layer = Some(0);
    studio.persona = Persona::Design;
    studio.show_welcome = false;
    studio.tool = Tool::Select;
    let mut root = crate::layout::make_frame(Pt::ZERO, Pt::new(600., 200.));
    root.layout.stack = Some(AutoStack {
        flow: StackFlow::Grid,
        column_tracks: vec![GridTrack {
            size: GridTrackSize::Fixed(80.),
            ..Default::default()
        }],
        gap: 17.,
        padding: [0.; 4],
        ..Default::default()
    });
    let id = root.id;
    let mut child = crate::layout::make_frame(Pt::new(75., 85.), Pt::new(20., 30.));
    child.layout.parent = Some(id);
    child.layout.width = crate::layout::Sizing::Fill;
    let mut unrelated = crate::layout::make_frame(Pt::new(700., 500.), Pt::new(25., 25.));
    unrelated.name = "Unrelated artwork".into();
    studio.doc.layers[0]
        .kind
        .shapes_mut()
        .unwrap()
        .extend([root, child, unrelated]);
    studio.selection = vec![(0, id)];
    (studio, id)
}

fn track(studio: &Studio, id: u64) -> f32 {
    match studio
        .doc
        .find_shape(0, id)
        .unwrap()
        .layout
        .stack
        .as_ref()
        .unwrap()
        .column_tracks[0]
        .size
    {
        GridTrackSize::Fixed(value) => value,
        _ => panic!("Expected fixed track"),
    }
}

fn key(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

#[test]
fn actual_grid_track_is_tab_reachable_and_accepts_keyboard_entry() {
    let (mut studio, id) = fixture();
    let mut seat = Seat::default();
    seat.frame(&mut studio, vec![], false);
    seat.click(&mut studio, "Column tracks", false);
    let at = seat.at("80 px");
    let mut reached = false;
    for _ in 0..80 {
        seat.frame(&mut studio, vec![key(Key::Tab, Modifiers::NONE)], false);
        let focused = seat.ctx.memory(|m| m.focused());
        if focused
            .and_then(|id| seat.ctx.read_response(id))
            .is_some_and(|response| response.rect.contains(at))
        {
            reached = true;
            break;
        }
    }
    assert!(
        reached,
        "Grid numeric control must participate in Tab navigation"
    );
    let field = seat.ctx.memory(|memory| memory.focused());
    seat.frame(
        &mut studio,
        vec![key(Key::A, Modifiers::CTRL), Event::Text("96".into())],
        false,
    );
    assert_eq!(seat.ctx.memory(|memory| memory.focused()), field);
    assert_eq!(track(&studio, id), 96.);
    seat.frame(&mut studio, vec![key(Key::Enter, Modifiers::NONE)], false);
    assert_ne!(seat.ctx.memory(|memory| memory.focused()), field);
    assert_eq!(track(&studio, id), 96.);
    assert_eq!(studio.history.len(), 1);
    studio.undo();
    assert_eq!(track(&studio, id), 80.);
}

#[test]
fn actual_grid_track_drag_coalesces_and_preserves_undo_redo_and_reopen() {
    let (mut studio, id) = fixture();
    let mut seat = Seat::default();
    seat.frame(&mut studio, vec![], false);
    seat.click(&mut studio, "Column tracks", false);
    let at = seat.at("80 px");
    let before_document = crate::project::encode(&studio.doc).unwrap();
    let child = studio.doc.layers[0]
        .kind
        .shapes()
        .unwrap()
        .iter()
        .find(|shape| shape.layout.parent == Some(id))
        .unwrap()
        .clone();
    seat.drag(&mut studio, at, false);
    let after = track(&studio, id);
    assert!(after > 80.);
    assert_ne!(
        studio.doc.find_shape(0, child.id).unwrap().geom.bbox(),
        child.geom.bbox()
    );
    assert_eq!(studio.history.len(), 1);
    let after_document = crate::project::encode(&studio.doc).unwrap();
    studio.undo();
    assert_eq!(track(&studio, id), 80.);
    assert_eq!(
        crate::project::encode(&studio.doc).unwrap(),
        before_document
    );
    studio.redo();
    assert_eq!(track(&studio, id), after);
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), after_document);
    let reopened = crate::project::decode(&crate::project::encode(&studio.doc).unwrap()).unwrap();
    assert_eq!(
        reopened.find_shape(0, id).unwrap().layout,
        studio.doc.find_shape(0, id).unwrap().layout
    );
}

#[test]
fn actual_grid_track_drag_cannot_cross_selection_or_accept_escape() {
    let (mut studio, id) = fixture();
    let mut other = studio.doc.find_shape(0, id).unwrap().clone();
    other.id += 1000;
    let second = other.id;
    studio.doc.layers[0].kind.shapes_mut().unwrap().push(other);
    let mut seat = Seat::default();
    seat.frame(&mut studio, vec![], false);
    seat.click(&mut studio, "Column tracks", false);
    let at = seat.at("80 px");
    seat.pointer(&mut studio, at, true, false);
    seat.frame(
        &mut studio,
        vec![Event::PointerMoved(at + vec2(24., 0.))],
        false,
    );
    let first = track(&studio, id);
    assert!(first > 80.);
    studio.selection = vec![(0, second)];
    seat.frame(
        &mut studio,
        vec![Event::PointerMoved(at + vec2(48., 0.))],
        false,
    );
    seat.pointer(&mut studio, at + vec2(48., 0.), false, false);
    assert_eq!(track(&studio, second), 80.);
    assert_eq!(track(&studio, id), first);
    assert_eq!(studio.history.len(), 1);
    seat.click(&mut studio, "Column tracks", false);
    let at = seat.at("80 px");
    seat.pointer(&mut studio, at, true, false);
    seat.frame(
        &mut studio,
        vec![
            Event::PointerMoved(at + vec2(24., 0.)),
            Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
        ],
        false,
    );
    seat.pointer(&mut studio, at + vec2(24., 0.), false, false);
    assert_eq!(track(&studio, second), 80.);
    assert_eq!(studio.history.len(), 1);
}

#[test]
fn actual_named_opacity_drag_keeps_instance_override_and_one_undo() {
    let (mut studio, main) = fixture();
    components::make_component(&mut studio.doc, 0, main).unwrap();
    components::expose_property(
        &mut studio.doc,
        0,
        main,
        main,
        "Card opacity",
        components::PropertyKind::Opacity,
    )
    .unwrap();
    let instance =
        components::insert_instance(&mut studio.doc, main, 0, Pt::new(0., 250.), None).unwrap();
    studio.doc.find_shape_mut(0, instance).unwrap().opacity = 0.4;
    studio.selection = vec![(0, instance)];
    let mut seat = Seat::default();
    seat.frame(&mut studio, vec![], true);
    let at = seat.at("40 %");
    let before_document = crate::project::encode(&studio.doc).unwrap();
    let child = studio.doc.layers[0]
        .kind
        .shapes()
        .unwrap()
        .iter()
        .find(|shape| shape.layout.parent == Some(instance))
        .unwrap()
        .clone();
    seat.drag(&mut studio, at, true);
    let after = studio.doc.find_shape(0, instance).unwrap().opacity;
    assert!(after > 0.4);
    assert_ne!(
        studio.doc.find_shape(0, child.id).unwrap().geom.bbox(),
        child.geom.bbox()
    );
    assert_eq!(studio.doc.find_shape(0, main).unwrap().opacity, 1.);
    assert_eq!(studio.history.len(), 1);
    let after_document = crate::project::encode(&studio.doc).unwrap();
    studio.undo();
    assert_eq!(studio.doc.find_shape(0, instance).unwrap().opacity, 0.4);
    assert_eq!(
        crate::project::encode(&studio.doc).unwrap(),
        before_document
    );
    studio.redo();
    assert_eq!(studio.doc.find_shape(0, instance).unwrap().opacity, after);
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), after_document);
}
