use super::{welcome::keyboard_tests as keys, *};
use crate::{
    app::Studio,
    document::{Document, Layer, Shape, Style},
    geom::{Geom, Pt},
};
use eframe::egui::{self, Event, Key, Modifiers};

fn fixture() -> Studio {
    let mut studio = Studio::new();
    studio.show_welcome = false;
    studio.doc = Document::new("Workspace QA", 400.0, 300.0, 72.0);
    let mut layer = Layer::vector("Artwork");
    let shape = Shape::new(
        Geom::Rect {
            origin: Pt::new(30.0, 40.0),
            size: Pt::new(100.0, 80.0),
            radius: 0.0,
        },
        Style::default(),
    );
    studio.selection = vec![(0, shape.id)];
    layer.kind.shapes_mut().unwrap().push(shape);
    studio.doc.layers = vec![layer];
    studio.active_layer = Some(0);
    studio
}
fn point(output: &egui::FullOutput, name: &str) -> egui::Pos2 {
    let (_, node) = keys::named(output, name);
    let r = node.bounds().unwrap();
    egui::pos2(((r.x0 + r.x1) * 0.5) as f32, ((r.y0 + r.y1) * 0.5) as f32)
}
fn pointer(pos: egui::Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn human_qa_layer_double_click_focuses_rename_paste_commit_undo_reopen() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    theme::apply(&ctx);
    let mut studio = fixture();
    let mut frame =
        |events| keys::render(&ctx, events, |ui| studios::layers_content(ui, &mut studio));
    frame(vec![]);
    let output = frame(vec![]);
    let at = point(&output, "Artwork");
    for _ in 0..2 {
        frame(vec![Event::PointerMoved(at), pointer(at, true)]);
        frame(vec![pointer(at, false)]);
    }
    let output = frame(vec![]);
    let (field, _) = keys::named(&output, "Rename layer");
    assert_eq!(
        ctx.memory(|m| m.focused()).map(|id| id.accesskit_id()),
        Some(field)
    );
    frame(keys::key(Key::A, Modifiers::CTRL | Modifiers::COMMAND));
    frame(vec![Event::Paste("Pasted layer name".into())]);
    frame(keys::key(Key::Enter, Modifiers::NONE));
    drop(frame);
    assert_eq!(studio.doc.layers[0].name, "Pasted layer name");
    assert!(studio.layer_rename.is_none());
    let saved = crate::project::encode(&studio.doc).unwrap();
    assert_eq!(
        crate::project::decode(&saved).unwrap().layers[0].name,
        "Pasted layer name"
    );
    studio.undo();
    assert_eq!(studio.doc.layers[0].name, "Artwork");
    studio.redo();
    assert_eq!(studio.doc.layers[0].name, "Pasted layer name");
}

#[test]
fn human_qa_native_dimension_lock_preserves_ratio_and_one_undo() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    theme::apply(&ctx);
    let mut studio = fixture();
    let before = crate::project::encode(&studio.doc).unwrap();
    let mut frame = |events| {
        keys::render(&ctx, events, |ui| {
            ui.set_max_width(304.0);
            panes::show(ui, &mut studio, "transform");
        })
    };
    frame(vec![]);
    let output = frame(vec![]);
    let at = point(&output, "Constrain proportions");
    assert!(at.x < 304.0, "the actual lock must fit the dock: {at:?}");
    frame(vec![Event::PointerMoved(at), pointer(at, true)]);
    frame(vec![pointer(at, false)]);
    keys::tab_to(&ctx, "W", &mut frame);
    frame(keys::key(Key::A, Modifiers::CTRL | Modifiers::COMMAND));
    frame(vec![Event::Paste("200".into())]);
    frame(keys::key(Key::Enter, Modifiers::NONE));
    drop(frame);
    let hit = studio.selection[0];
    let size = studio
        .doc
        .find_shape(hit.0, hit.1)
        .unwrap()
        .geom
        .bbox()
        .size();
    assert_eq!(size, Pt::new(200.0, 160.0));
    let after = crate::project::encode(&studio.doc).unwrap();
    studio.undo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    studio.redo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), after);
    assert_eq!(
        crate::project::encode(&crate::project::decode(&after).unwrap()).unwrap(),
        after
    );
}

#[test]
fn human_qa_parallel_paint_panes_have_independent_native_widget_ids() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    theme::apply(&ctx);
    let mut studio = fixture();
    let hit = studio.selection[0];
    studio.doc.find_shape_mut(hit.0, hit.1).unwrap().style.fill =
        crate::document::Fill::Gradient(crate::gradient::Gradient::new(
            crate::gradient::GradientKind::Linear,
            crate::color::Rgba::BLACK,
            crate::color::Rgba::WHITE,
        ));
    let before = crate::project::encode(&studio.doc).unwrap();
    let mut frame = || {
        keys::render(&ctx, vec![], |ui| {
            ui.columns(2, |columns| {
                panes::show(&mut columns[0], &mut studio, "appearance");
                panes::show(&mut columns[1], &mut studio, "gradients");
            });
        })
    };
    frame();
    let output = frame();
    drop(frame);
    let controls: Vec<_> = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .filter(|(_, node)| {
            node.label() == Some("Fill type") && node.role() == egui::accesskit::Role::ComboBox
        })
        .collect();
    assert_eq!(controls.len(), 2, "both real paint selectors must exist");
    assert_ne!(controls[0].0, controls[1].0);
    assert_eq!(before, crate::project::encode(&studio.doc).unwrap());
}

#[test]
fn human_qa_photo_panes_keep_import_actions_without_duplicate_headings_or_slogans() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    theme::apply(&ctx);
    let mut studio = fixture();
    studio.persona = crate::tools::Persona::Photo;
    let before = crate::project::encode(&studio.doc).unwrap();
    let output = keys::render(&ctx, vec![], |ui| {
        ui.columns(2, |columns| {
            panes::show(&mut columns[0], &mut studio, "photo-library");
            panes::show(&mut columns[1], &mut studio, "photo-develop");
        });
    });
    let nodes = &output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes;
    assert!(
        nodes
            .iter()
            .any(|(_, node)| node.label() == Some("Open photo"))
    );
    assert!(
        !nodes
            .iter()
            .filter_map(|(_, node)| node.label())
            .any(|label| label == "Library"
                || label == "Develop"
                || label.starts_with("A little light")
                || label.starts_with("Open a photo to make"))
    );
    assert_eq!(before, crate::project::encode(&studio.doc).unwrap());
}
