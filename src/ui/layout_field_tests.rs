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

fn native_frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
    crate::ui::welcome::keyboard_tests::render(ctx, events, |ui| crate::ui::run(ui, studio))
}
fn native_press(ctx: &egui::Context, studio: &mut Studio, name: &str) {
    use crate::ui::welcome::keyboard_tests as keys;
    keys::tab_to(ctx, name, |events| native_frame(ctx, studio, events));
    native_frame(ctx, studio, keys::key(Key::Enter, Modifiers::NONE));
    native_frame(ctx, studio, vec![]);
}
fn native_edit(
    ctx: &egui::Context,
    studio: &mut Studio,
    name: &str,
    value: &str,
) -> egui::FullOutput {
    use crate::ui::welcome::keyboard_tests as keys;
    keys::tab_to(ctx, name, |events| native_frame(ctx, studio, events));
    keys::replace(value, |events| native_frame(ctx, studio, events))
}

#[test]
fn keyboard_layout_inspector_empty_document_grid_named_property_and_variable_authoring_reopens() {
    use crate::ui::welcome::keyboard_tests as keys;
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    crate::ui::theme::apply(&ctx);
    let mut studio = Studio::new();
    studio.doc = Document::new("Keyboard Layout", 900., 700., 96.);
    studio.show_welcome = false;
    studio.persona = Persona::Layout;
    assert!(studio.selection.is_empty());
    native_frame(&ctx, &mut studio, vec![]);
    for key in [Key::F8, Key::F, Key::Enter] {
        native_frame(&ctx, &mut studio, keys::key(key, Modifiers::NONE));
    }
    native_frame(&ctx, &mut studio, vec![]);
    native_frame(
        &ctx,
        &mut studio,
        keys::key(Key::Enter, Modifiers::CTRL | Modifiers::COMMAND),
    );
    native_frame(&ctx, &mut studio, vec![]);
    let (layer, root) = studio.selected_frame().unwrap();
    let mut properties_focused = false;
    for _ in 0..6 {
        let output = native_frame(&ctx, &mut studio, keys::key(Key::F6, Modifiers::NONE));
        if ctx
            .memory(|memory| memory.focused())
            .map(|id| id.accesskit_id())
            == Some(keys::named(&output, "Properties").0)
        {
            properties_focused = true;
            break;
        }
    }
    assert!(
        properties_focused,
        "Native F6 must reach the Properties panel before authoring its fields"
    );
    native_press(&ctx, &mut studio, "Arrange children automatically");
    native_press(&ctx, &mut studio, "Grid");
    native_press(&ctx, &mut studio, "Column tracks");
    native_press(&ctx, &mut studio, "Add Column track");
    assert_eq!(
        studio
            .doc
            .find_shape(layer, root)
            .unwrap()
            .layout
            .stack
            .as_ref()
            .unwrap()
            .column_tracks,
        vec![GridTrack::default()]
    );
    let mut output = native_frame(&ctx, &mut studio, vec![]);
    for _ in 0..3 {
        output = keys::tab(true, |events| native_frame(&ctx, &mut studio, events));
    }
    keys::focused(&ctx, &output, "Column 0 share");
    native_edit(&ctx, &mut studio, "Column 0 share", "2.25");
    native_frame(&ctx, &mut studio, keys::key(Key::Tab, Modifiers::NONE));
    assert_eq!(
        studio
            .doc
            .find_shape(layer, root)
            .unwrap()
            .layout
            .stack
            .as_ref()
            .unwrap()
            .column_tracks[0]
            .size,
        GridTrackSize::Fraction(2.25)
    );
    native_press(&ctx, &mut studio, "Create component");
    native_press(&ctx, &mut studio, "Named properties");
    native_edit(&ctx, &mut studio, "New property name", "Card opacity");
    native_press(&ctx, &mut studio, "Expose Opacity");
    let output = native_edit(&ctx, &mut studio, "Card opacity", "70.25");
    keys::focused(&ctx, &output, "Card opacity");
    let bounds = keys::named(&output, "Card opacity").1.bounds().unwrap();
    let extent = studio.panel_workspaces.tasks["Layout"]
        .groups
        .iter()
        .find(|group| group.panels.iter().any(|panel| panel == "properties"))
        .unwrap()
        .extent;
    assert!(
        bounds.x0 >= f64::from(ctx.content_rect().right() - extent)
            && bounds.x1 <= f64::from(ctx.content_rect().right()),
        "Named property escaped its real dock: {bounds:?}"
    );
    native_frame(&ctx, &mut studio, keys::key(Key::Tab, Modifiers::NONE));
    assert!((studio.doc.find_shape(layer, root).unwrap().opacity - 0.7025).abs() < 0.00001);
    native_press(&ctx, &mut studio, "Design variables");
    native_edit(&ctx, &mut studio, "Variable name", "Space");
    native_press(&ctx, &mut studio, "+ Number");
    let before = crate::project::encode(&studio.doc).unwrap();
    native_edit(&ctx, &mut studio, "Variable Space", "24.125001");
    native_frame(&ctx, &mut studio, keys::key(Key::Tab, Modifiers::NONE));
    assert_eq!(
        studio.doc.layout_tokens[0].value,
        crate::layout_tokens::TokenValue::Number(24.125001_f32)
    );
    let after = crate::project::encode(&studio.doc).unwrap();
    assert_ne!(before, after);
    native_frame(&ctx, &mut studio, keys::key(Key::F8, Modifiers::NONE));
    native_frame(
        &ctx,
        &mut studio,
        keys::key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND),
    );
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    native_frame(
        &ctx,
        &mut studio,
        keys::key(
            Key::Z,
            Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
        ),
    );
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), after);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".artifacts")
        .join(format!("keyboard-layout-{}", crate::document::next_id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("authored.oma");
    crate::project::save_to(&studio.doc, &path).unwrap();
    assert_eq!(
        crate::project::encode(&crate::project::load_from(&path).unwrap()).unwrap(),
        after
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn keyboard_layout_inspector_index_rows_retire_invalid_drafts_after_remove_undo_and_locked_owner() {
    use crate::ui::welcome::keyboard_tests as keys;
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    crate::ui::theme::apply(&ctx);
    let (mut studio, root) = fixture();
    studio
        .doc
        .find_shape_mut(0, root)
        .unwrap()
        .layout
        .stack
        .as_mut()
        .unwrap()
        .column_tracks
        .push(GridTrack {
            size: GridTrackSize::Fixed(160.),
            ..Default::default()
        });
    let mut render =
        |studio: &mut Studio, events| keys::render(&ctx, events, |ui| inspector(ui, studio));
    render(&mut studio, vec![]);
    keys::tab_to(&ctx, "Column tracks", |events| render(&mut studio, events));
    render(&mut studio, keys::key(Key::Enter, Modifiers::NONE));
    render(&mut studio, vec![]);
    let output = keys::tab_to(&ctx, "Column 0 size", |events| render(&mut studio, events));
    let old = keys::named(&output, "Column 0 size").0;
    let second = keys::named(&output, "Column 1 size").0;
    let output = keys::replace("-", |events| render(&mut studio, events));
    keys::associated_error(&output, "Column 0 size");
    keys::tab_to(&ctx, "Remove Column track 0", |events| {
        render(&mut studio, events)
    });
    render(&mut studio, keys::key(Key::Enter, Modifiers::NONE));
    let output = render(&mut studio, vec![]);
    let current = keys::named(&output, "Column 0 size");
    assert_ne!(current.0, old);
    assert_ne!(current.0, second);
    assert_eq!(current.1.numeric_value(), Some(160.));
    assert!(current.1.invalid().is_none());
    let before = crate::project::encode(&studio.doc).unwrap();
    render(
        &mut studio,
        keys::action(
            old,
            egui::accesskit::Action::SetValue,
            Some(egui::accesskit::ActionData::NumericValue(999.)),
        ),
    );
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    studio.undo();
    let output = render(&mut studio, vec![]);
    assert_ne!(keys::named(&output, "Column 0 size").0, old);
    assert_eq!(track(&studio, root), 80.);
    assert!(keys::named(&output, "Column 0 size").1.invalid().is_none());
    studio.doc.layers[0].locked = true;
    let output = render(&mut studio, vec![]);
    assert!(keys::named(&output, "Column 0 size").1.is_disabled());
    let before = crate::project::encode(&studio.doc).unwrap();
    render(
        &mut studio,
        keys::action(
            keys::named(&output, "Column 0 size").0,
            egui::accesskit::Action::SetValue,
            Some(egui::accesskit::ActionData::NumericValue(200.)),
        ),
    );
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
}

#[test]
fn keyboard_layout_inspector_native_breakpoint_precision_conditional_ids_and_retired_errors() {
    use crate::ui::welcome::keyboard_tests as keys;
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    crate::ui::theme::apply(&ctx);
    let (mut studio, root) = fixture();
    studio
        .doc
        .find_shape_mut(0, root)
        .unwrap()
        .layout
        .breakpoints = vec![
        crate::layout::LayoutBreakpoint {
            max_width: 600.1235,
            ..Default::default()
        },
        crate::layout::LayoutBreakpoint {
            max_width: 1024.,
            ..Default::default()
        },
    ];
    let bits = studio.doc.find_shape(0, root).unwrap().layout.breakpoints[0]
        .max_width
        .to_bits();
    let mut render =
        |studio: &mut Studio, events| keys::render(&ctx, events, |ui| inspector(ui, studio));
    render(&mut studio, vec![]);
    keys::tab_to(&ctx, "Breakpoint overrides", |events| {
        render(&mut studio, events)
    });
    render(&mut studio, keys::key(Key::Enter, Modifiers::NONE));
    render(&mut studio, vec![]);
    let output = keys::tab_to(&ctx, "Breakpoint 0 width (px)", |events| {
        render(&mut studio, events)
    });
    let width = keys::named(&output, "Breakpoint 0 width (px)").0;
    assert_eq!(
        keys::named(&output, "Breakpoint 0 width (px)")
            .1
            .numeric_value(),
        Some(f64::from(f32::from_bits(bits)))
    );
    keys::tab_to(&ctx, "Set breakpoint 0 gap", |events| {
        render(&mut studio, events)
    });
    render(&mut studio, keys::key(Key::Enter, Modifiers::NONE));
    let output = render(&mut studio, vec![]);
    assert_eq!(keys::named(&output, "Breakpoint 0 width (px)").0, width);
    assert_eq!(
        studio.doc.find_shape(0, root).unwrap().layout.breakpoints[0]
            .max_width
            .to_bits(),
        bits
    );
    keys::tab_to(&ctx, "Breakpoint 0 width (px)", |events| {
        render(&mut studio, events)
    });
    let output = keys::replace("-", |events| render(&mut studio, events));
    keys::associated_error(&output, "Breakpoint 0 width (px)");
    assert_eq!(
        studio.doc.find_shape(0, root).unwrap().layout.breakpoints[0]
            .max_width
            .to_bits(),
        bits
    );
    keys::tab_to(&ctx, "Remove breakpoint 0", |events| {
        render(&mut studio, events)
    });
    render(&mut studio, keys::key(Key::Enter, Modifiers::NONE));
    let output = render(&mut studio, vec![]);
    assert_ne!(keys::named(&output, "Breakpoint 0 width (px)").0, width);
    assert_eq!(
        keys::named(&output, "Breakpoint 0 width (px)")
            .1
            .numeric_value(),
        Some(1024.)
    );
    assert!(
        keys::named(&output, "Breakpoint 0 width (px)")
            .1
            .invalid()
            .is_none()
    );
    studio.undo();
    let output = render(&mut studio, vec![]);
    assert_eq!(
        studio.doc.find_shape(0, root).unwrap().layout.breakpoints[0]
            .max_width
            .to_bits(),
        bits
    );
    assert_ne!(keys::named(&output, "Breakpoint 0 width (px)").0, width);
    assert!(
        keys::named(&output, "Breakpoint 0 width (px)")
            .1
            .invalid()
            .is_none()
    );
}

#[test]
fn keyboard_layout_mixed_primary_equal_native_intent_keeps_other_members_and_one_undo() {
    use crate::ui::welcome::keyboard_tests as keys;
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    crate::ui::theme::apply(&ctx);
    let (mut studio, first) = fixture();
    studio
        .doc
        .find_shape_mut(0, first)
        .unwrap()
        .layout
        .stack
        .as_mut()
        .unwrap()
        .gap = 12.;
    let mut second = crate::layout::make_frame(Pt::new(700., 0.), Pt::new(200., 160.));
    let second_id = second.id;
    let mut stack = studio
        .doc
        .find_shape(0, first)
        .unwrap()
        .layout
        .stack
        .clone()
        .unwrap();
    stack.gap = 24.;
    stack.padding = [7., 11., 13., 17.];
    stack.column_tracks[0].size = GridTrackSize::Fixed(160.);
    stack.column_tracks[0].min = Some(30.);
    stack.column_tracks[0].max = Some(200.);
    second.layout.stack = Some(stack.clone());
    studio.doc.layers[0].kind.shapes_mut().unwrap().push(second);
    studio.selection = vec![(0, first), (0, second_id)];
    let render =
        |studio: &mut Studio, events| keys::render(&ctx, events, |ui| inspector(ui, studio));
    let before = crate::project::encode(&studio.doc).unwrap();
    let output = keys::tab_to(&ctx, "Gap", |events| render(&mut studio, events));
    assert_eq!(
        keys::named(&output, "Gap").1.state_description(),
        Some("Mixed")
    );
    assert_eq!(keys::named(&output, "Gap").1.numeric_value(), None);
    assert_eq!(studio.history.len(), 0);
    keys::replace("12", |events| render(&mut studio, events));
    render(&mut studio, keys::key(Key::Tab, Modifiers::NONE));
    let first_after = studio
        .doc
        .find_shape(0, first)
        .unwrap()
        .layout
        .stack
        .as_ref()
        .unwrap();
    let second_after = studio
        .doc
        .find_shape(0, second_id)
        .unwrap()
        .layout
        .stack
        .as_ref()
        .unwrap();
    assert_eq!(first_after.gap, 12.);
    assert_eq!(second_after.gap, 12.);
    let mut expected = stack.clone();
    expected.gap = 12.;
    assert_eq!(*second_after, expected);
    assert_eq!(studio.history.len(), 1);
    let after = crate::project::encode(&studio.doc).unwrap();
    studio.undo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    studio.redo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), after);
    keys::tab_to(&ctx, "Column tracks", |events| render(&mut studio, events));
    render(&mut studio, keys::key(Key::Enter, Modifiers::NONE));
    let output = keys::tab_to(&ctx, "Column 0 size", |events| render(&mut studio, events));
    assert_eq!(
        keys::named(&output, "Column 0 size").1.state_description(),
        Some("Mixed")
    );
    assert!(keys::named(&output, "Add Column track").1.is_disabled());
    assert!(
        keys::named(&output, "Remove Column track 0")
            .1
            .is_disabled()
    );
    keys::replace("80", |events| render(&mut studio, events));
    render(&mut studio, keys::key(Key::Tab, Modifiers::NONE));
    let second_after = studio
        .doc
        .find_shape(0, second_id)
        .unwrap()
        .layout
        .stack
        .as_ref()
        .unwrap();
    expected.column_tracks[0].size = GridTrackSize::Fixed(80.);
    assert_eq!(*second_after, expected);
    let current = crate::project::encode(&studio.doc).unwrap();
    studio.doc.layers[0].locked = true;
    let output = render(&mut studio, vec![]);
    assert!(keys::named(&output, "Gap").1.is_disabled());
    let history = studio.history.len();
    render(
        &mut studio,
        keys::action(
            keys::named(&output, "Gap").0,
            egui::accesskit::Action::SetValue,
            Some(egui::accesskit::ActionData::NumericValue(90.)),
        ),
    );
    assert_eq!(studio.history.len(), history);
    studio.doc.layers[0].locked = false;
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), current);
    assert_eq!(
        crate::project::encode(&crate::project::decode(&current).unwrap()).unwrap(),
        current
    );
}
