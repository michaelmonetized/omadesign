use crate::{
    app::Studio,
    document::{Document, Layer, Shape, Style},
    geom::{Geom, Pt},
    ui::{self, icons::ph},
};
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, Rect};
fn fixture() -> (egui::Context, Studio) {
    let ctx = egui::Context::default();
    ui::theme::apply(&ctx);
    let mut s = Studio::new();
    s.show_welcome = false;
    s.show_key_hud = false;
    s.need_fit = false;
    s.doc = Document::new("UI align", 800., 600., 72.);
    s.doc.layers = vec![Layer::vector("Artwork")];
    s.selection.clear();
    s.active_layer = Some(0);
    for (x, y) in [(60., 60.), (270., 160.), (600., 400.)] {
        let sh = Shape::new(
            Geom::Rect {
                origin: Pt::new(x, y),
                size: Pt::new(30., 40.),
                radius: 0.,
            },
            Style::default(),
        );
        s.selection.push((0, sh.id));
        s.doc.layers[0].kind.shapes_mut().unwrap().push(sh);
    }
    (ctx, s)
}
fn frame(
    ctx: &egui::Context,
    s: &mut Studio,
    width: f32,
    events: Vec<Event>,
) -> Vec<(String, Pos2)> {
    s.last_input = std::time::Instant::now();
    s.dirty = false;
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 1000.))),
            events,
            ..Default::default()
        },
        |ui| crate::ui::run(ui, s),
    );
    output.textures_delta.clear();
    fn visit(shape: &egui::Shape, labels: &mut Vec<(String, Pos2)>) {
        match shape {
            egui::Shape::Text(t) => {
                labels.push((t.galley.job.text.clone(), t.pos + t.galley.size() * 0.5))
            }
            egui::Shape::Vec(v) => {
                for shape in v {
                    visit(shape, labels)
                }
            }
            _ => {}
        }
    }
    let mut labels = vec![];
    for sh in output.shapes {
        visit(&sh.shape, &mut labels);
    }
    labels
}
fn click(ctx: &egui::Context, s: &mut Studio, label: &str, modifiers: Modifiers) {
    let labels = frame(ctx, s, 1600., vec![]);
    let p = labels
        .iter()
        .find(|(text, _)| text == label)
        .unwrap_or_else(|| panic!("missing {label}"))
        .1;
    for pressed in [true, false] {
        frame(
            ctx,
            s,
            1600.,
            vec![
                Event::ModifiersChanged(modifiers),
                Event::PointerMoved(p),
                Event::PointerButton {
                    pos: p,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers,
                },
            ],
        );
    }
}
#[test]
fn inspector_routes_all_alignment_distribution_and_order_controls() {
    let (ctx, mut s) = fixture();
    for _ in 0..3 {
        frame(&ctx, &mut s, 1600., vec![]);
    }
    for glyph in [
        ph::ALIGN_LEFT,
        ph::ALIGN_CENTER_H,
        ph::ALIGN_RIGHT,
        ph::ALIGN_TOP,
        ph::ALIGN_CENTER_V,
        ph::ALIGN_BOTTOM,
    ] {
        let labels = frame(&ctx, &mut s, 1600., vec![]);
        assert_eq!(
            labels.iter().filter(|(text, _)| text == glyph).count(),
            1,
            "single Align home"
        );
        let before = crate::project::encode(&s.doc).unwrap();
        let h = s.history.len();
        click(&ctx, &mut s, glyph, Modifiers::NONE);
        assert_eq!(s.history.len(), h + 1);
        s.undo();
        assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    }
    for label in ["Distribute H", "Distribute V"] {
        let h = s.history.len();
        click(&ctx, &mut s, label, Modifiers::SHIFT);
        assert_eq!(s.history.len(), h + 1);
        s.undo();
    }
    let ids = s.selection.clone();
    s.selection = vec![ids[1]];
    for label in ["To front", "To back", "Forward", "Backward"] {
        let h = s.history.len();
        click(&ctx, &mut s, label, Modifiers::NONE);
        assert_eq!(s.history.len(), h + 1);
    }
}
#[test]
fn inspector_single_layer_and_shift_align_are_live_and_arrange_menu_is_gone() {
    let (ctx, mut s) = fixture();
    for _ in 0..3 {
        frame(&ctx, &mut s, 1600., vec![]);
    }
    let labels = frame(&ctx, &mut s, 1600., vec![]);
    assert!(
        !labels
            .iter()
            .any(|(text, p)| text == "Arrange" && p.y < 50.)
    );
    let ids = s.selection.clone();
    s.selection.truncate(1);
    click(&ctx, &mut s, ph::ALIGN_RIGHT, Modifiers::NONE);
    assert_eq!(
        s.doc.find_shape(0, ids[0].1).unwrap().world_bbox().max.x,
        800.
    );
    s.undo();
    s.activate_layer_tree(0);
    click(&ctx, &mut s, ph::ALIGN_LEFT, Modifiers::NONE);
    assert_eq!(
        s.doc.find_shape(0, ids[0].1).unwrap().world_bbox().min.x,
        0.
    );
    s.undo();
    s.selection = ids.clone();
    s.selected_layer = None;
    click(&ctx, &mut s, ph::ALIGN_CENTER_H, Modifiers::SHIFT);
    for &(li, id) in &ids {
        assert_eq!(
            s.doc.find_shape(li, id).unwrap().world_bbox().center().x,
            400.
        );
    }
    s.undo();
    s.selection.clear();
    s.selected_layer = None;
    let h = s.history.len();
    click(&ctx, &mut s, ph::ALIGN_LEFT, Modifiers::NONE);
    assert_eq!(s.history.len(), h);
    frame(&ctx, &mut s, 900., vec![]);
    let labels = frame(&ctx, &mut s, 900., vec![]);
    let p = labels.iter().find(|(text, _)| text == "Menu").unwrap().1;
    for pressed in [true, false] {
        frame(
            &ctx,
            &mut s,
            900.,
            vec![
                Event::PointerMoved(p),
                Event::PointerButton {
                    pos: p,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    let labels = frame(&ctx, &mut s, 900., vec![]);
    assert!(
        !labels
            .iter()
            .any(|(text, p)| text == "Arrange" && p.x < 600.)
    );
    frame(
        &ctx,
        &mut s,
        900.,
        vec![Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
}

#[test]
fn ordering_shortcuts_work_in_every_drawing_persona_without_arrange_menu() {
    for persona in [crate::tools::Persona::Design,crate::tools::Persona::Layout,crate::tools::Persona::Pixel,crate::tools::Persona::Motion] {
        let (ctx,mut s)=fixture();s.persona=persona;
        let id=s.selection[1].1;s.selection=vec![(0,id)];
        for (key,shift,expected) in [(Key::CloseBracket,false,2),(Key::OpenBracket,true,0),(Key::OpenBracket,false,0),(Key::CloseBracket,true,2)] {
            let modifiers=Modifiers {ctrl:true,command:true,shift,..Modifiers::NONE};
            let mut output=ctx.run_ui(egui::RawInput {events:vec![Event::Key {key,physical_key:Some(key),pressed:true,repeat:false,modifiers}],..Default::default()},|ui|s.handle_shortcuts(ui.ctx()));
            output.textures_delta.clear();assert_eq!(s.doc.layers[0].kind.shapes().unwrap()[expected].id,id,"{persona:?} {key:?} shift={shift}");
        }
    }
}
