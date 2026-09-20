use super::*;
use eframe::egui::{Event, Modifiers};

fn setup() -> (eframe::egui::Context, Studio) {
    let (ctx, mut studio) = super::tests::pen_input_studio();
    studio.persona = Persona::Layout;
    studio.tool = Tool::Select;
    studio.view.offset = Pt::ZERO;
    studio.view.scale = 1.0;
    studio.doc.layers = vec![
        crate::document::Layer::vector("Destination"),
        crate::document::Layer::vector("Imported icons"),
    ];
    studio.history.clear();
    (ctx, studio)
}

fn drag(ctx: &eframe::egui::Context, studio: &mut Studio, from: Pt, to: Pt) {
    super::tests::canvas_frame(ctx, studio, vec![]);
    let origin = studio.canvas_rect.unwrap().min.to_vec2();
    let start = to_egui(from) + origin;
    let end = to_egui(to) + origin;
    super::tests::canvas_frame(
        ctx,
        studio,
        vec![
            Event::PointerMoved(start),
            super::tests::pen_pointer_button(start, true, Modifiers::NONE),
        ],
    );
    assert!(
        matches!(studio.op, Some(Op::Move { .. })),
        "pointer did not pick a move: {:?}",
        studio.selection
    );
    super::tests::canvas_frame(
        ctx,
        studio,
        vec![Event::PointerMoved(start + (end - start) * 0.8)],
    );
    // The final pointer sample arrives with release, as with a fast real drag.
    super::tests::canvas_frame(
        ctx,
        studio,
        vec![
            Event::PointerMoved(end),
            super::tests::pen_pointer_button(end, false, Modifiers::NONE),
        ],
    );
    assert!(studio.op.is_none());
}

#[test]
fn pointer_drop_moves_icons_across_layers_into_deepest_frame_with_one_undo() {
    for rotation in [0.0, std::f32::consts::FRAC_PI_2] {
        let (ctx, mut s) = setup();
        let destination = crate::layout::make_frame(Pt::new(175.0, 25.0), Pt::new(200.0, 240.0));
        let destination_id = destination.id;
        let mut inner = crate::layout::make_frame(Pt::new(225.0, 60.0), Pt::new(100.0, 160.0));
        inner.layout.parent = Some(destination_id);
        let inner_id = inner.id;
        s.doc.layers[0]
            .kind
            .shapes_mut()
            .unwrap()
            .extend([inner, destination]);
        let mut source = crate::layout::make_frame(Pt::new(20.0, 50.0), Pt::new(100.0, 140.0));
        source.rotation = rotation;
        source.layout.stack = Some(crate::layout::AutoStack::default());
        let source_id = source.id;
        let mut icon = crate::document::Shape::new(
            Geom::Rect {
                origin: Pt::new(40.0, 70.0),
                size: Pt::splat(30.0),
                radius: 0.0,
            },
            crate::document::Style::default(),
        );
        icon.layout.parent = Some(source_id);
        let id = icon.id;
        s.doc.layers[1]
            .kind
            .shapes_mut()
            .unwrap()
            .extend([source, icon]);
        crate::layout::reflow_roots(&mut s.doc, 1);
        let point = s.doc.find_shape(1, id).unwrap().geom.bbox().center();
        let start = s.doc.find_shape(1, source_id).unwrap().world_point(point);
        let original = serde_json::to_value(&s.doc).unwrap();
        s.selection = vec![(1, id)];
        let end = Pt::new(270.0, 120.0);
        drag(&ctx, &mut s, start, end);
        let moved = s
            .doc
            .find_shape(0, id)
            .expect("icon crossed into destination layer");
        assert_eq!(moved.layout.parent, Some(inner_id));
        assert!(
            (moved.geom.bbox().center() - end).length() < 0.01,
            "released icon moved away from pointer"
        );
        assert!((moved.rotation - rotation).abs() < 0.001);
        assert!(s.doc.find_shape(1, id).is_none());
        assert_eq!(s.history.len(), 1);
        assert!(s.doc.validate_hierarchy().is_ok());
        let moved = serde_json::to_value(&s.doc).unwrap();
        s.undo();
        assert_eq!(serde_json::to_value(&s.doc).unwrap(), original);
        s.redo();
        assert_eq!(serde_json::to_value(&s.doc).unwrap(), moved);
    }
}

#[test]
fn pointer_drop_ignores_the_moving_frame_and_retains_its_children() {
    let (ctx, mut s) = setup();
    let destination = crate::layout::make_frame(Pt::new(180.0, 40.0), Pt::new(200.0, 220.0));
    let parent = destination.id;
    s.doc.layers[0].kind.shapes_mut().unwrap().push(destination);
    let moving = crate::layout::make_frame(Pt::new(20.0, 20.0), Pt::new(100.0, 120.0));
    let root = moving.id;
    let mut child = crate::document::Shape::new(
        Geom::Rect {
            origin: Pt::splat(50.0),
            size: Pt::splat(25.0),
            radius: 0.0,
        },
        crate::document::Style::default(),
    );
    child.layout.parent = Some(root);
    let child_id = child.id;
    s.doc.layers[1]
        .kind
        .shapes_mut()
        .unwrap()
        .extend([child, moving]);
    s.selection = vec![(1, root)];
    let original = serde_json::to_value(&s.doc).unwrap();
    drag(&ctx, &mut s, Pt::new(70.0, 110.0), Pt::new(290.0, 160.0));
    assert_eq!(
        s.doc.find_shape(0, root).unwrap().layout.parent,
        Some(parent)
    );
    assert_eq!(
        s.doc.find_shape(0, child_id).unwrap().layout.parent,
        Some(root)
    );
    assert_eq!(
        s.doc.find_shape(0, child_id).unwrap().geom.bbox().min,
        Pt::new(270.0, 100.0)
    );
    assert_eq!(s.history.len(), 1);
    assert!(s.doc.validate_hierarchy().is_ok());
    s.undo();
    assert_eq!(serde_json::to_value(&s.doc).unwrap(), original);
}
