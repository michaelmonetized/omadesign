use super::*;
use crate::{
    document::{Document, Layer, Shape, Style},
    geom::Pt,
    ui::{
        panes,
        welcome::keyboard_tests as keys,
        workspace::{Place, Workspace},
    },
};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, vec2};

struct Seat {
    ctx: egui::Context,
    dock: Workspace,
    screen: egui::Vec2,
    clip: Rect,
    time: f64,
    output: egui::FullOutput,
}
impl Seat {
    fn new() -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::ui::theme::apply(&ctx);
        let mut dock = Workspace::preset(&[("layout", Place::Right)]);
        dock.groups[0].extent = 256.;
        Self {
            ctx,
            dock,
            screen: vec2(1200., 800.),
            clip: Rect::NOTHING,
            time: 0.,
            output: Default::default(),
        }
    }
    fn frame(&mut self, s: &mut Studio, mut events: Vec<Event>) -> egui::FullOutput {
        self.time += 0.2;
        let modifiers = events
            .iter()
            .find_map(|e| match e {
                Event::Key { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        events.insert(0, Event::ModifiersChanged(modifiers));
        let mut output = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.screen)),
                time: Some(self.time),
                events,
                ..Default::default()
            },
            |ui| {
                self.dock.show(
                    ui,
                    "layout-sizing-dock",
                    &[("layout", "Auto Layout")],
                    |ui, _| {
                        self.clip = ui.clip_rect();
                        egui::ScrollArea::vertical()
                            .id_salt(("pane-body", "layout"))
                            .min_scrolled_height(0.)
                            .auto_shrink([false, false])
                            .show(ui, |ui| panes::show(ui, s, "layout"));
                    },
                );
            },
        );
        output.textures_delta.clear();
        self.output = output.clone();
        output
    }
    fn settle(&mut self, s: &mut Studio) {
        for _ in 0..4 {
            self.frame(s, vec![]);
        }
    }
    fn bounds(&self, name: &str) -> Rect {
        let b = keys::named(&self.output, name).1.bounds().unwrap();
        Rect::from_min_max(
            egui::pos2(b.x0 as f32, b.y0 as f32),
            egui::pos2(b.x1 as f32, b.y1 as f32),
        )
    }
}
fn editor() -> (Studio, u64) {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.persona = Persona::Layout;
    s.doc = Document::new("Responsive Auto Layout sizing", 500., 300., 96.);
    s.doc.workspace = Some(Persona::Layout);
    let mut root = crate::layout::make_frame(Pt::new(40., 40.), Pt::new(240., 160.));
    root.name = "Sizing frame".into();
    root.layout.stack = Some(AutoStack {
        padding: [16.; 4],
        ..Default::default()
    });
    let id = root.id;
    let mut child = Shape::new(
        Geom::Rect {
            origin: Pt::new(56., 56.),
            size: Pt::new(80., 48.),
            radius: 0.,
        },
        Style::default(),
    );
    child.name = "Frame content".into();
    child.layout.parent = Some(id);
    let mut unrelated = Shape::new(
        Geom::Rect {
            origin: Pt::new(350., 220.),
            size: Pt::new(30., 30.),
            radius: 0.,
        },
        Style::default(),
    );
    unrelated.name = "Unrelated artwork".into();
    let mut layer = Layer::vector("Layout artwork");
    layer
        .kind
        .shapes_mut()
        .unwrap()
        .extend([root, child, unrelated]);
    s.doc.layers = vec![layer];
    s.active_layer = Some(0);
    s.selection = vec![(0, id)];
    s.mark();
    s.history.clear();
    (s, id)
}
fn saved(s: &Studio) -> String {
    crate::project::encode(&s.doc).unwrap()
}
fn visible(shape: &egui::Shape, clip: Rect, label: &str) -> bool {
    match shape {
        egui::Shape::Text(t) if t.galley.text() == label => {
            clip.contains_rect(Rect::from_min_size(t.pos, t.galley.size()))
        }
        egui::Shape::Vec(shapes) => shapes.iter().any(|s| visible(s, clip, label)),
        _ => false,
    }
}
#[test]
fn craft_layout_sizing_controls_fit_actual_resized_docks_without_mutation() {
    let (mut s, _) = editor();
    let before = saved(&s);
    let mut seat = Seat::new();
    let mut ids = None;
    for (screen, width) in [
        (vec2(1200., 800.), 256.),
        (vec2(900., 650.), 288.),
        (vec2(1400., 900.), 420.),
        (vec2(900., 650.), 256.),
    ] {
        seat.screen = screen;
        seat.dock.groups[0].extent = width;
        seat.settle(&mut s);
        let current = [
            keys::named(&seat.output, "Width sizing").0,
            keys::named(&seat.output, "Height sizing").0,
        ];
        if let Some(old) = ids {
            assert_eq!(current, old, "resizing changed actual native owners");
        }
        ids = Some(current);
        for name in ["Width sizing", "Height sizing"] {
            let r = seat.bounds(name);
            assert!(
                seat.clip.contains_rect(r),
                "{width}px {name} outside real dock: {r:?}, clip={:?}",
                seat.clip
            );
            assert!(
                r.width() >= 64. && r.height() >= 20.,
                "unusable sizing control {r:?}"
            );
            assert!(
                seat.output.shapes.iter().any(|p| visible(
                    &p.shape,
                    p.clip_rect.intersect(seat.clip),
                    name
                )),
                "{width}px sizing label not fully painted: {name}"
            );
        }
        assert_eq!(saved(&s), before);
        assert_eq!(s.history.len(), 0);
    }
}
fn emit(name: &str, text: &str) {
    if let Some(root) = std::env::var_os("OMADESIGN_LAYOUT_SIZING_QA") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(std::path::Path::new(&root).join(name), text).unwrap();
    }
}
fn reopen(s: &Studio) {
    let wire = saved(s);
    let doc = crate::project::decode(&wire).unwrap();
    assert_eq!(crate::project::encode(&doc).unwrap(), wire);
    assert_eq!(
        crate::compositor::export_png(&doc, 1).unwrap(),
        crate::compositor::export_png(&s.doc, 1).unwrap()
    );
}
#[test]
fn craft_layout_sizing_native_choices_keep_focus_one_undo_and_reopen() {
    let (mut s, id) = editor();
    let before = saved(&s);
    let unrelated = s.doc.layers[0]
        .kind
        .shapes()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    let mut seat = Seat::new();
    seat.settle(&mut s);
    let ctx = seat.ctx.clone();
    keys::tab_to(&ctx, "Width sizing", |events| seat.frame(&mut s, events));
    keys::focused(&ctx, &seat.output, "Width sizing");
    seat.frame(&mut s, keys::key(Key::ArrowDown, Modifiers::NONE));
    seat.frame(&mut s, vec![]);
    keys::focused(&ctx, &seat.output, "Width sizing");
    assert_eq!(s.doc.find_shape(0, id).unwrap().layout.width, Sizing::Hug);
    assert_eq!(
        s.doc.find_shape(0, id).unwrap().layout.height,
        Sizing::Fixed
    );
    assert_eq!(s.history.len(), 1);
    let width = saved(&s);
    reopen(&s);
    keys::tab_to(&ctx, "Height sizing", |events| seat.frame(&mut s, events));
    seat.frame(&mut s, keys::key(Key::End, Modifiers::NONE));
    seat.frame(&mut s, vec![]);
    keys::focused(&ctx, &seat.output, "Height sizing");
    assert_eq!(s.doc.find_shape(0, id).unwrap().layout.width, Sizing::Hug);
    assert_eq!(s.doc.find_shape(0, id).unwrap().layout.height, Sizing::Fill);
    assert_eq!(s.history.len(), 2);
    assert_eq!(s.doc.find_shape(0, unrelated.id), Some(&unrelated));
    let after = saved(&s);
    reopen(&s);
    s.undo();
    assert_eq!(saved(&s), width);
    s.undo();
    assert_eq!(saved(&s), before);
    s.redo();
    assert_eq!(saved(&s), width);
    s.redo();
    assert_eq!(saved(&s), after);
    emit("before.oma", &before);
    emit("width-hug.oma", &width);
    emit("both.oma", &after);
    s.doc.layers[0].locked = true;
    seat.settle(&mut s);
    for name in ["Width sizing", "Height sizing"] {
        assert!(keys::named(&seat.output, name).1.is_disabled());
    }
    let locked = saved(&s);
    seat.frame(&mut s, keys::key(Key::Home, Modifiers::NONE));
    assert_eq!(saved(&s), locked);
}

#[test]
fn craft_layout_sizing_pointer_opener_escape_keeps_actual_combo_focus_for_arrows() {
    let (mut s, id) = editor();
    let before = saved(&s);
    let mut seat = Seat::new();
    seat.settle(&mut s);
    let at = seat.bounds("Width sizing").center();
    seat.frame(&mut s, vec![Event::PointerMoved(at)]);
    for pressed in [true, false] {
        seat.frame(
            &mut s,
            vec![
                Event::PointerMoved(at),
                Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    assert!(egui::Popup::is_any_open(&seat.ctx));
    seat.frame(&mut s, keys::key(Key::Escape, Modifiers::NONE));
    seat.frame(&mut s, vec![]);
    assert!(!egui::Popup::is_any_open(&seat.ctx));
    keys::focused(&seat.ctx, &seat.output, "Width sizing");
    assert_eq!(saved(&s), before);
    assert_eq!(s.history.len(), 0);
    seat.frame(&mut s, keys::key(Key::ArrowDown, Modifiers::NONE));
    seat.frame(&mut s, vec![]);
    keys::focused(&seat.ctx, &seat.output, "Width sizing");
    assert_eq!(s.doc.find_shape(0, id).unwrap().layout.width, Sizing::Hug);
    assert_eq!(
        s.doc.find_shape(0, id).unwrap().geom.bbox().min,
        Pt::new(40., 40.)
    );
    assert_eq!(s.history.len(), 1);
    s.undo();
    assert_eq!(saved(&s), before);
}
