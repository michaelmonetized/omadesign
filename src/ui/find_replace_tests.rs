use super::*;
use crate::{
    document::{Cmd, Document, Fill, Layer, Shape, Style},
    geom::TypeRun,
    tools::Persona,
    ui::welcome::keyboard_tests as keys,
};
use egui::{Event, Key, Modifiers, PointerButton, Pos2};

fn editor() -> Studio {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.persona = Persona::Layout;
    s.doc = Document::new("Native Find and Replace", 500., 220., 96.);
    s.doc.workspace = Some(Persona::Layout);
    s.doc.layers = vec![Layer::vector("Editable stories")];
    for (i, text) in ["Cat cat catalog", "the CAT"].into_iter().enumerate() {
        let mut run = TypeRun {
            content: text.into(),
            origin: Pt::new(35., 75. + i as f32 * 70.),
            px: 30.,
            font: concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/assets/fonts/EBGaramond.ttf"
            )
            .into(),
            ..Default::default()
        };
        run.set_character_style(0, 3, |span| {
            span.fill = Some(Fill::Solid(crate::color::Rgba::from_hex(0xd6224a)))
        });
        let mut shape = Shape::new(Geom::Text(run), Style::default());
        shape.name = format!("Story {}", i + 1);
        crate::text::fill_contours(&mut shape.geom);
        s.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
    }
    let id = s.doc.layers[0].kind.shapes().unwrap()[0].id;
    s.selection = vec![(0, id)];
    s.active_layer = Some(0);
    s.mark();
    s.history.clear();
    s
}
fn wire(s: &Studio) -> String {
    crate::project::encode(&s.doc).unwrap()
}
fn texts(s: &Studio) -> Vec<String> {
    s.doc
        .layers
        .iter()
        .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
        .filter_map(|s| {
            if let Geom::Text(r) = &s.geom {
                Some(r.content.clone())
            } else {
                None
            }
        })
        .collect()
}
fn emit(name: &str, s: &Studio) {
    if let Some(root) = std::env::var_os("OMADESIGN_FIND_REPLACE_QA") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(std::path::Path::new(&root).join(name), wire(s)).unwrap();
    }
}
struct Seat {
    ctx: egui::Context,
    time: f64,
    out: egui::FullOutput,
    size: egui::Vec2,
}
impl Seat {
    fn new() -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::ui::theme::apply(&ctx);
        ctx.all_styles_mut(|style| style.animation_time = 0.);
        Self {
            ctx,
            time: 0.,
            out: Default::default(),
            size: egui::vec2(1600., 1000.),
        }
    }
    fn frame(&mut self, s: &mut Studio, mut events: Vec<Event>) -> egui::FullOutput {
        self.time += 0.2;
        let modifiers = events
            .iter()
            .find_map(|e| {
                if let Event::Key { modifiers, .. } = e {
                    Some(*modifiers)
                } else {
                    None
                }
            })
            .unwrap_or_default();
        events.insert(0, Event::ModifiersChanged(modifiers));
        let mut out = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, self.size)),
                time: Some(self.time),
                events,
                ..Default::default()
            },
            |ui| crate::ui::run(ui, s),
        );
        out.textures_delta.clear();
        self.out = out.clone();
        out
    }
    fn click(&mut self, s: &mut Studio, name: &str) {
        for _ in 0..3 {
            self.frame(s, vec![]);
        }
        let b = keys::named(&self.out, name).1.bounds().unwrap();
        let pos = egui::pos2(((b.x0 + b.x1) * 0.5) as f32, ((b.y0 + b.y1) * 0.5) as f32);
        self.frame(s, vec![Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            self.frame(
                s,
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        self.frame(s, vec![]);
    }
    fn invoke(&mut self, s: &mut Studio, name: &str) {
        let ctx = self.ctx.clone();
        keys::tab_to(&ctx, name, |e| self.frame(s, e));
        self.frame(s, keys::key(Key::Space, Modifiers::NONE));
        self.frame(s, vec![]);
    }
    fn type_field(&mut self, s: &mut Studio, name: &str, text: &str) {
        let ctx = self.ctx.clone();
        keys::tab_to(&ctx, name, |e| self.frame(s, e));
        self.frame(s, keys::key(Key::A, Modifiers::CTRL | Modifiers::COMMAND));
        self.frame(s, vec![Event::Paste(text.into())]);
        self.frame(s, vec![]);
        keys::focused(&self.ctx, &self.out, name);
    }
    fn open(&mut self, s: &mut Studio) {
        self.frame(s, vec![]);
        self.click(s, "Edit");
        self.invoke(s, "Find and Replace…");
        assert!(is_open(&self.ctx));
        self.frame(s, vec![]);
        keys::focused(&self.ctx, &self.out, "Find text");
    }
    fn canvas_focus(&mut self, s: &mut Studio) {
        for _ in 0..3 {
            self.frame(s, vec![]);
        }
        let response = self.ctx.read_response(Id::new("studio-canvas")).unwrap();
        assert_eq!(self.ctx.memory(|m| m.focused()), Some(response.id));
        assert_eq!(
            self.out
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .focus,
            response.id.accesskit_id()
        );
    }
}

#[test]
fn craft_find_replace_native_menu_paste_case_word_replace_all_one_undo_and_reopen() {
    let mut s = editor();
    let mut seat = Seat::new();
    let before = wire(&s);
    emit("native-before.oma", &s);
    seat.open(&mut s);
    assert_eq!(wire(&s), before);
    assert_eq!(s.history.len(), 0);
    seat.type_field(&mut s, "Find text", "cat");
    seat.type_field(&mut s, "Replacement text", "dog");
    seat.invoke(&mut s, "Whole words");
    seat.invoke(&mut s, "Find next");
    let d = seat
        .ctx
        .data(|data| data.get_temp::<Draft>(Id::new(ID)))
        .unwrap();
    assert_eq!(d.matches.len(), 3);
    assert_eq!(d.selected, Some(0));
    assert_eq!(wire(&s), before);
    seat.invoke(&mut s, "Replace all");
    assert_eq!(texts(&s), ["dog dog catalog", "the dog"]);
    assert_eq!(s.history.len(), 1);
    let after = wire(&s);
    emit("native-after.oma", &s);
    seat.invoke(&mut s, "Close Find and Replace");
    seat.canvas_focus(&mut s);
    assert!(!is_open(&seat.ctx));
    seat.frame(
        &mut s,
        keys::key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND),
    );
    assert_eq!(wire(&s), before);
    seat.frame(
        &mut s,
        keys::key(
            Key::Z,
            Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
        ),
    );
    assert_eq!(wire(&s), after);
    let cold = crate::project::decode(&after).unwrap();
    assert_eq!(crate::project::encode(&cold).unwrap(), after);
}

#[test]
fn craft_find_replace_native_cancel_keeps_live_draft_and_blocks_background_history() {
    let mut s = editor();
    let mut seat = Seat::new();
    seat.frame(&mut s, vec![]);
    let hit = s.selection[0];
    s.commit(Cmd::SetOpacity {
        layer: 0,
        id: hit.1,
        before: 1.,
        after: 0.8,
    });
    s.begin_type_edit(hit, Pt::new(35., 75.));
    s.type_edit.as_mut().unwrap().caret = 0;
    s.type_edit.as_mut().unwrap().anchor = 0;
    s.type_insert("prefix ");
    let before = wire(&s);
    let original = s.type_edit.as_ref().unwrap().before.clone();
    let n = s.history.len();
    seat.open(&mut s);
    seat.type_field(&mut s, "Find text", "cat");
    seat.frame(
        &mut s,
        keys::key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND),
    );
    assert_eq!(wire(&s), before);
    assert_eq!(s.history.len(), n);
    assert_eq!(s.type_edit.as_ref().unwrap().before, original);
    seat.frame(&mut s, keys::key(Key::Escape, Modifiers::NONE));
    seat.canvas_focus(&mut s);
    assert!(!is_open(&seat.ctx));
    assert_eq!(wire(&s), before);
    assert_eq!(s.history.len(), n);
    assert_eq!(s.type_edit.as_ref().unwrap().before, original);
}

#[test]
fn craft_find_replace_native_shortcut_next_story_and_stale_owner_refusal() {
    let mut s = editor();
    let mut seat = Seat::new();
    seat.frame(&mut s, vec![]);
    seat.frame(&mut s, keys::key(Key::F8, Modifiers::NONE));
    let before = wire(&s);
    let first = s.selection[0];
    seat.frame(
        &mut s,
        keys::key(Key::F, Modifiers::CTRL | Modifiers::COMMAND),
    );
    seat.frame(&mut s, vec![]);
    assert!(is_open(&seat.ctx));
    seat.type_field(&mut s, "Find text", "cat");
    seat.invoke(&mut s, "Find next");
    seat.invoke(&mut s, "Close Find and Replace");
    seat.canvas_focus(&mut s);
    seat.frame(&mut s, keys::key(Key::F3, Modifiers::NONE));
    assert_ne!(s.selection, [first]);
    assert_eq!(wire(&s), before);
    assert_eq!(s.history.len(), 0);
    let second = s.selection[0];
    seat.frame(&mut s, keys::key(Key::F3, Modifiers::NONE));
    assert_eq!(s.selection, [first]);
    assert_ne!(first, second);
    seat.frame(
        &mut s,
        keys::key(Key::F, Modifiers::CTRL | Modifiers::COMMAND),
    );
    seat.frame(&mut s, vec![]);
    seat.type_field(&mut s, "Replacement text", "changed");
    s.swap_id = "another-document-owner".into();
    seat.frame(&mut s, vec![]);
    let (_, button) = keys::named(&seat.out, "Replace all");
    assert!(button.is_disabled());
    keys::associated_error(&seat.out, "Find text");
    assert_eq!(wire(&s), before);
    assert_eq!(s.history.len(), 0);
    seat.click(&mut s, "Close Find and Replace");
    assert!(!is_open(&seat.ctx));
    assert_eq!(wire(&s), before);
}

#[test]
fn craft_find_replace_native_footer_keeps_single_line_buttons_when_the_window_narrows() {
    let mut s = editor();
    let mut seat = Seat::new();
    let before = wire(&s);
    seat.open(&mut s);
    seat.type_field(&mut s, "Find text", "cat");
    seat.invoke(&mut s, "Find next");
    for width in [1600., 800., 420.] {
        seat.size = egui::vec2(width, 700.);
        for _ in 0..3 {
            seat.frame(&mut s, vec![]);
        }
        let height = seat.ctx.global_style().spacing.interact_size.y as f64 + 4.;
        for name in [
            "Find next",
            "Replace",
            "Replace all",
            "Select match",
            "Close Find and Replace",
        ] {
            let (_, node) = keys::named(&seat.out, name);
            let b = node.bounds().unwrap();
            let label = if name == "Close Find and Replace" {
                "Close"
            } else {
                name
            };
            fn visible(shape: &egui::Shape, clip: egui::Rect, label: &str) -> bool {
                match shape {
                    egui::Shape::Text(text) if text.galley.text() == label => {
                        clip.contains_rect(egui::Rect::from_min_size(text.pos, text.galley.size()))
                    }
                    egui::Shape::Vec(shapes) => shapes.iter().any(|s| visible(s, clip, label)),
                    _ => false,
                }
            }
            assert!(
                seat.out
                    .shapes
                    .iter()
                    .any(|s| visible(&s.shape, s.clip_rect, label)),
                "{width}px {name} is not fully painted inside its real scroll clip"
            );
            assert!(
                b.y1 - b.y0 <= height,
                "{width}px {name} squeezed to multiple lines: {b:?}"
            );
            assert!(
                b.x0 >= 0. && b.x1 <= width as f64,
                "{width}px {name} outside actual viewport: {b:?}"
            );
        }
        assert_eq!(wire(&s), before);
        assert_eq!(s.history.len(), 0);
    }
    seat.invoke(&mut s, "Close Find and Replace");
    assert!(!is_open(&seat.ctx));
}
