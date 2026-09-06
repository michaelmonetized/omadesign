//! HUD regressions exercise egui input and production layout, not a parallel key map.
use super::{key_hud, theme};
use crate::app::{Op, PendingNav, Studio};
use crate::document::{Document, Layer, Shape, Style};
use crate::geom::{Geom, Pt};
use crate::tools::{Persona, Tool};
use eframe::egui::{
    self, Event, Id, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, TextEdit, Vec2, vec2,
};

fn fixture() -> (egui::Context, Studio) {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut studio = Studio::new();
    studio.show_welcome = false;
    studio.show_key_hud = true;
    studio.show_rulers = false;
    studio.need_fit = false;
    studio.doc = Document::new("HUD input fixture", 640.0, 480.0, 72.0);
    studio.doc.layers = vec![Layer::vector("Artwork")];
    studio.doc.layers[0]
        .kind
        .shapes_mut()
        .unwrap()
        .push(Shape::new(
            Geom::Rect {
                origin: Pt::new(20.0, 20.0),
                size: Pt::new(40.0, 40.0),
                radius: 0.0,
            },
            Style::default(),
        ));
    studio.active_layer = Some(0);
    studio.selection.clear();
    studio.snap.enabled = false;
    studio.dirty = false;
    (ctx, studio)
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

fn button(at: Pos2, pressed: bool, modifiers: Modifiers) -> Event {
    Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers,
    }
}

fn frame(
    ctx: &egui::Context,
    studio: &mut Studio,
    size: Vec2,
    events: Vec<Event>,
) -> egui::FullOutput {
    // Layout/input tests must not schedule desktop recovery writes.
    studio.last_input = std::time::Instant::now();
    studio.dirty = false;
    let mut output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ui| super::run(ui, studio),
    );
    output.textures_delta.clear();
    output
}

fn canvas_rect(ctx: &egui::Context, persona: Persona) -> Rect {
    ctx.read_response(Id::new(if persona == Persona::Photo {
        "studio-photo-canvas"
    } else {
        "studio-canvas"
    }))
    .unwrap()
    .rect
}

fn active(studio: &Studio, ctx: &egui::Context, key: &str) -> bool {
    studio
        .key_hints(ctx)
        .gestures
        .iter()
        .any(|hint| hint.keys == key && hint.active)
}

fn handles(studio: &Studio) -> (Pt, Pt) {
    let Some(Op::Pen { anchors, .. }) = &studio.op else {
        panic!("expected the real Pen gesture")
    };
    (anchors.last().unwrap().h_in, anchors.last().unwrap().h_out)
}

#[test]
fn live_pen_modifiers_update_hints_and_geometry_without_moving_the_canvas() {
    let (ctx, mut studio) = fixture();
    studio.tool = Tool::Pen;
    let size = vec2(960.0, 640.0);
    for _ in 0..3 {
        frame(&ctx, &mut studio, size, vec![]);
    }
    let canvas = canvas_rect(&ctx, Persona::Design);
    let start = canvas.min + vec2(80.0, 80.0);
    let end = start + vec2(80.0, 40.0);
    frame(&ctx, &mut studio, size, vec![Event::PointerMoved(start)]);
    frame(
        &ctx,
        &mut studio,
        size,
        vec![button(start, true, Modifiers::NONE)],
    );
    frame(&ctx, &mut studio, size, vec![Event::PointerMoved(end)]);
    assert_eq!(handles(&studio).1, Pt::new(80.0, 40.0));
    frame(
        &ctx,
        &mut studio,
        size,
        vec![Event::ModifiersChanged(Modifiers::SHIFT)],
    );
    assert!(active(&studio, &ctx, "Shift"));
    assert_eq!(canvas_rect(&ctx, Persona::Design), canvas);
    let (incoming, outgoing) = handles(&studio);
    assert!((outgoing.x - outgoing.y).abs() < 0.001);
    assert!((incoming + outgoing).length() < 0.001);
    frame(
        &ctx,
        &mut studio,
        size,
        vec![Event::ModifiersChanged(Modifiers::ALT)],
    );
    assert!(active(&studio, &ctx, "Alt"));
    assert!(!active(&studio, &ctx, "Shift"));
    assert_eq!(handles(&studio).0, incoming);
    assert_eq!(handles(&studio).1, Pt::new(80.0, 40.0));
    let control = Modifiers::CTRL | Modifiers::COMMAND;
    frame(
        &ctx,
        &mut studio,
        size,
        vec![Event::ModifiersChanged(control | Modifiers::ALT)],
    );
    assert!(active(&studio, &ctx, "Ctrl"));
    assert!(studio.snap_override);
    assert_eq!(canvas_rect(&ctx, Persona::Design), canvas);
    frame(
        &ctx,
        &mut studio,
        size,
        vec![Event::ModifiersChanged(Modifiers::NONE)],
    );
    assert!(!active(&studio, &ctx, "Ctrl") && !active(&studio, &ctx, "Alt"));
    assert!(!studio.snap_override);
    assert!((handles(&studio).0 + handles(&studio).1).length() < 0.001);
    frame(
        &ctx,
        &mut studio,
        size,
        vec![button(end, false, Modifiers::NONE)],
    );
    let next = start + vec2(150.0, 95.0);
    frame(
        &ctx,
        &mut studio,
        size,
        vec![
            Event::PointerMoved(next),
            button(next, true, Modifiers::NONE),
        ],
    );
    frame(
        &ctx,
        &mut studio,
        size,
        vec![button(next, false, Modifiers::NONE)],
    );
    frame(
        &ctx,
        &mut studio,
        size,
        vec![key(Key::Enter, Modifiers::NONE)],
    );
    assert!(
        studio.op.is_none(),
        "the HUD must not consume Pen's Enter key"
    );
    assert!(
        studio.doc.layers[0].kind.shapes().unwrap().iter().any(
            |shape| matches!(&shape.geom,Geom::Path{anchors,closed:false} if anchors.len()==2)
        )
    );
    frame(
        &ctx,
        &mut studio,
        size,
        vec![Event::ModifiersChanged(control)],
    );
    assert!(active(&studio, &ctx, "Ctrl"));
    frame(&ctx, &mut studio, size, vec![Event::WindowFocused(false)]);
    assert!(
        studio
            .key_hints(&ctx)
            .gestures
            .iter()
            .all(|hint| !hint.active)
    );
}

#[test]
fn hud_is_passive_and_text_fields_and_modals_keep_keyboard_ownership() {
    let (ctx, mut studio) = fixture();
    studio.tool = Tool::Pen;
    let before = serde_json::to_value(&studio.doc).unwrap();
    let field = Id::new("hud-focused-field");
    let mut value = String::from("Original field value");
    let control = Modifiers::CTRL | Modifiers::COMMAND;
    for events in [
        vec![],
        vec![Event::ModifiersChanged(control), key(Key::A, control)],
        vec![Event::Paste("Field replacement".into())],
        vec![
            Event::ModifiersChanged(Modifiers::NONE),
            Event::Text("!".into()),
        ],
    ] {
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(960.0, 640.0))),
                events,
                ..Default::default()
            },
            |ui| {
                studio.handle_shortcuts(ui.ctx());
                let events = ui.ctx().input(|input| input.events.clone());
                key_hud::show(ui, &studio);
                assert_eq!(
                    ui.ctx().input(|input| input.events.clone()),
                    events,
                    "drawing hints consumed an input event"
                );
                ui.add(TextEdit::singleline(&mut value).id(field))
                    .request_focus();
                super::canvas::show(ui, &mut studio);
            },
        );
        output.textures_delta.clear();
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(field));
        assert!(
            studio.key_hints(&ctx).gestures.is_empty(),
            "canvas gestures must disappear while typing in a field"
        );
    }
    assert_eq!(value, "Field replacement!");
    assert_eq!(serde_json::to_value(&studio.doc).unwrap(), before);

    let (ctx, mut studio) = fixture();
    studio.tool = Tool::Pen;
    studio.pending_nav = Some(PendingNav::Quit);
    let before = serde_json::to_value(&studio.doc).unwrap();
    let size = vec2(960.0, 640.0);
    frame(&ctx, &mut studio, size, vec![]);
    assert!(ctx.memory(|memory| memory.top_modal_layer().is_some()));
    frame(
        &ctx,
        &mut studio,
        size,
        vec![
            Event::ModifiersChanged(control | Modifiers::SHIFT),
            key(Key::D, control),
            key(Key::Delete, Modifiers::NONE),
        ],
    );
    assert!(studio.key_hints(&ctx).gestures.is_empty());
    assert_eq!(serde_json::to_value(&studio.doc).unwrap(), before);
    assert!(studio.pending_nav.is_some());
}

#[test]
fn modifier_rows_stay_bounded_at_both_window_sizes_in_every_persona() {
    let modifiers = [
        Modifiers::SHIFT,
        Modifiers::ALT,
        Modifiers::CTRL | Modifiers::COMMAND,
        Modifiers::SHIFT | Modifiers::ALT | Modifiers::CTRL | Modifiers::COMMAND,
        Modifiers::NONE,
    ];
    for size in [vec2(960.0, 640.0), vec2(1600.0, 1000.0)] {
        for persona in [
            Persona::Design,
            Persona::Pixel,
            Persona::Photo,
            Persona::Motion,
        ] {
            for shown in [true, false] {
                let (ctx, mut studio) = fixture();
                studio.persona = persona;
                studio.tool = match persona {
                    Persona::Design => Tool::Pen,
                    Persona::Pixel => Tool::Heal,
                    Persona::Photo => Tool::Crop,
                    Persona::Motion => Tool::Select,
                };
                studio.show_key_hud = shown;
                for _ in 0..3 {
                    frame(&ctx, &mut studio, size, vec![]);
                }
                let canvas = canvas_rect(&ctx, persona);
                assert!(
                    canvas.width() > 150.0 && canvas.height() > 150.0,
                    "{persona:?} has no usable canvas at {size:?}"
                );
                let hud = egui::containers::panel::PanelState::load(&ctx, Id::new("shortcut-hud"));
                if shown {
                    let rect = hud.unwrap().outer_rect;
                    assert!((rect.height() - key_hud::HEIGHT).abs() < 0.01);
                    assert!(
                        rect.min.x >= -0.01
                            && rect.max.x <= size.x + 0.01
                            && rect.max.y <= size.y + 0.01
                    );
                } else {
                    assert!(hud.is_none(), "a hidden HUD still allocated its panel");
                }
                for modifiers in modifiers {
                    frame(
                        &ctx,
                        &mut studio,
                        size,
                        vec![Event::ModifiersChanged(modifiers)],
                    );
                    assert_eq!(
                        canvas_rect(&ctx, persona),
                        canvas,
                        "{persona:?} canvas shifted at {size:?} with {modifiers:?}, shown={shown}"
                    );
                }
            }
        }
    }
}

#[test]
fn hud_toggle_and_document_shortcuts_deliver_their_original_key_events() {
    let (ctx, mut studio) = fixture();
    let id = studio.doc.layers[0].kind.shapes().unwrap()[0].id;
    studio.selection = vec![(0, id)];
    let size = vec2(960.0, 640.0);
    let control = Modifiers::CTRL | Modifiers::COMMAND;
    for _ in 0..3 {
        frame(&ctx, &mut studio, size, vec![]);
    }
    frame(
        &ctx,
        &mut studio,
        size,
        vec![Event::ModifiersChanged(control), key(Key::D, control)],
    );
    assert_eq!(studio.doc.layers[0].kind.shapes().unwrap().len(), 2);
    frame(&ctx, &mut studio, size, vec![key(Key::Slash, control)]);
    assert!(!studio.show_key_hud);
    frame(&ctx, &mut studio, size, vec![key(Key::Z, control)]);
    assert_eq!(studio.doc.layers[0].kind.shapes().unwrap().len(), 1);
    frame(&ctx, &mut studio, size, vec![key(Key::Slash, control)]);
    assert!(studio.show_key_hud);
    assert_eq!(studio.doc.layers[0].kind.shapes().unwrap()[0].id, id);
}
