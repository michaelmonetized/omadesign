use crate::{
    app::Studio,
    document::{Document, Layer, Shape, Style},
    geom::{Geom, Pt, TypeRun},
    text_geometry::TextFrame,
    tools::Persona,
    ui::{
        panes,
        welcome::keyboard_tests as keys,
        workspace::{Place, Workspace},
    },
};
use eframe::egui::{self, Event, Key, Modifiers, Pos2, Rect, vec2};

struct Seat {
    ctx: egui::Context,
    dock: Workspace,
    time: f64,
    screen: egui::Vec2,
    clip: Rect,
    output: egui::FullOutput,
}
impl Seat {
    fn new() -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::ui::theme::apply(&ctx);
        let mut dock = Workspace::preset(&[("character", Place::Right)]);
        dock.groups[0].extent = 288.;
        Self {
            ctx,
            dock,
            time: 0.,
            screen: vec2(1200., 800.),
            clip: Rect::NOTHING,
            output: Default::default(),
        }
    }
    fn frame(&mut self, studio: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
        self.time += 0.2;
        let modifiers = events
            .iter()
            .find_map(|e| match e {
                Event::Key { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        let mut events = events;
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
                    "type-inspector-resize",
                    &[("character", "Character")],
                    |ui, _| {
                        self.clip = ui.clip_rect();
                        egui::ScrollArea::vertical()
                            .id_salt("pane-body")
                            .min_scrolled_height(0.)
                            .auto_shrink([false, false])
                            .show(ui, |ui| panes::show(ui, studio, "character"));
                    },
                );
            },
        );
        output.textures_delta.clear();
        self.output = output.clone();
        output
    }
    fn settle(&mut self, studio: &mut Studio) {
        for _ in 0..4 {
            self.frame(studio, vec![]);
        }
    }
    fn bounds(&self, name: &str) -> Rect {
        let b = keys::named(&self.output, name).1.bounds().unwrap();
        Rect::from_min_max(
            egui::pos2(b.x0 as f32, b.y0 as f32),
            egui::pos2(b.x1 as f32, b.y1 as f32),
        )
    }
    fn edit(&mut self, studio: &mut Studio, name: &str, text: &str) {
        let ctx = self.ctx.clone();
        keys::tab_to(&ctx, name, |events| self.frame(studio, events));
        keys::focused(&ctx, &self.output, name);
        self.frame(
            studio,
            keys::key(Key::A, Modifiers::CTRL | Modifiers::COMMAND),
        );
        for character in text.chars() {
            self.frame(studio, vec![Event::Text(character.to_string())]);
        }
        self.frame(studio, keys::key(Key::Enter, Modifiers::NONE));
        self.frame(studio, vec![]);
    }
}
fn editor() -> Studio {
    let mut studio = Studio::new();
    studio.show_welcome = false;
    studio.persona = Persona::Layout;
    studio.doc = Document::new("Responsive area text", 480., 260., 96.);
    let mut shape=Shape::new(Geom::Text(TypeRun {
        content:"An editable area text frame keeps its characters and formatting when its width and height change.".into(),
        origin:Pt::new(30.,55.),px:20.,
        font:concat!(env!("CARGO_MANIFEST_DIR"),"/tests/assets/fonts/EBGaramond.ttf").into(),
        frame:Some(TextFrame{size:Pt::new(152.25,90.75),..Default::default()}),..Default::default()
    }),Style::default());
    crate::text::fill_contours(&mut shape.geom);
    let id = shape.id;
    let mut layer = Layer::vector("Type");
    layer.kind.shapes_mut().unwrap().push(shape);
    studio.doc.layers = vec![layer];
    studio.active_layer = Some(0);
    studio.selection = vec![(0, id)];
    studio.mark();
    studio.history.clear();
    studio
}
fn saved(studio: &Studio) -> String {
    crate::project::encode(&studio.doc).unwrap()
}
fn reopen(studio: &Studio) {
    let wire = saved(studio);
    let doc = crate::project::decode(&wire).unwrap();
    assert_eq!(crate::project::encode(&doc).unwrap(), wire);
    assert_eq!(
        crate::compositor::export_png(&doc, 1).unwrap(),
        crate::compositor::export_png(&studio.doc, 1).unwrap()
    );
}
#[test]
fn craft_type_inspector_resizes_without_hiding_frame_controls_or_rewriting_values() {
    let mut studio = editor();
    let before = saved(&studio);
    let mut seat = Seat::new();
    for (screen, width) in [
        (vec2(1200., 800.), 256.),
        (vec2(900., 650.), 288.),
        (vec2(1400., 900.), 420.),
        (vec2(900., 650.), 256.),
    ] {
        seat.screen = screen;
        seat.dock.groups[0].extent = width;
        seat.settle(&mut studio);
        let w = seat.bounds("Text frame width (px)");
        let h = seat.bounds("Text frame height (px)");
        assert!(
            (w.top() - h.top()).abs() < 1.,
            "width/height must share the real row: {w:?} {h:?}"
        );
        assert!(w.right() < h.left(), "frame fields overlap: {w:?} {h:?}");
        for name in [
            "Text frame width (px)",
            "Text frame height (px)",
            "Text frame Inset top (px)",
            "Text frame Inset right (px)",
            "Text frame Inset bottom (px)",
            "Text frame Inset left (px)",
        ] {
            let r = seat.bounds(name);
            assert!(
                seat.clip.contains_rect(r),
                "{name} outside actual resized dock: {r:?} clip={:?}",
                seat.clip
            );
            assert!(
                r.width() > 40. && r.height() >= 20.,
                "usable native field {name}: {r:?}"
            );
        }
        assert!(
            seat.bounds("Font family").top() < seat.clip.bottom(),
            "ordinary Character controls were pushed out of view"
        );
        assert_eq!(saved(&studio), before);
        assert_eq!(studio.history.len(), 0);
    }
}
#[test]
fn craft_type_inspector_native_keyboard_edits_each_frame_dimension_as_one_undo() {
    let mut studio = editor();
    let mut seat = Seat::new();
    seat.settle(&mut studio);
    let before = saved(&studio);
    seat.edit(&mut studio, "Text frame width (px)", "176.5");
    assert_eq!(
        studio.selected_type().unwrap().frame.unwrap().size,
        Pt::new(176.5, 90.75)
    );
    assert_eq!(studio.history.len(), 1);
    let width = saved(&studio);
    reopen(&studio);
    seat.edit(&mut studio, "Text frame height (px)", "112.5");
    assert_eq!(
        studio.selected_type().unwrap().frame.unwrap().size,
        Pt::new(176.5, 112.5)
    );
    assert_eq!(studio.history.len(), 2);
    let both = saved(&studio);
    reopen(&studio);
    studio.undo();
    assert_eq!(saved(&studio), width);
    studio.undo();
    assert_eq!(saved(&studio), before);
    studio.redo();
    assert_eq!(saved(&studio), width);
    studio.redo();
    assert_eq!(saved(&studio), both);
    if let Some(root) = std::env::var_os("OMADESIGN_TYPE_INSPECTOR_QA") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(std::path::Path::new(&root).join("frame-before.oma"), before).unwrap();
        std::fs::write(std::path::Path::new(&root).join("frame-edited.oma"), both).unwrap();
    }
}
