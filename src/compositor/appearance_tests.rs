use super::*;
use crate::{
    color::Blend,
    document::{Cmd, History, Stroke, Style, apply},
};

fn rectangle(color: Rgba) -> Shape {
    Shape::new(
        Geom::Rect {
            origin: Pt::new(4., 4.),
            size: Pt::new(24., 24.),
            radius: 0.,
        },
        Style {
            fill: Fill::Solid(color),
            stroke: None,
        },
    )
}

fn document(shapes: Vec<Shape>) -> Document {
    let mut doc = Document::new("Appearance", 32., 32., 72.);
    doc.transparent = true;
    let mut layer = Layer::vector("Artwork");
    *layer.kind.shapes_mut().unwrap() = shapes;
    doc.layers = vec![layer];
    doc
}

#[test]
fn object_blends_against_underlying_artwork_and_roundtrips_with_undo() {
    let bottom = rectangle(Rgba::rgb(255, 0, 0));
    let top = rectangle(Rgba::rgb(0, 0, 255));
    let id = top.id;
    let mut doc = document(vec![bottom, top]);
    let command = Cmd::SetBlend {
        layer: 0,
        id,
        before: Blend::Normal,
        after: Blend::Multiply,
    };
    let mut history = History::default();
    apply(&mut doc, &command);
    history.push(command);
    let output = render_export(&doc, 1).unwrap();
    let color = output.pixel(16, 16).unwrap();
    assert_eq!(
        [color.red(), color.green(), color.blue(), color.alpha()],
        [0, 0, 0, 255]
    );
    let serialized = serde_json::to_vec(&doc).unwrap();
    let reopened: Document = serde_json::from_slice(&serialized).unwrap();
    assert_eq!(reopened.find_shape(0, id).unwrap().blend, Blend::Multiply);
    apply(&mut doc, &history.undo().unwrap());
    assert_eq!(
        render_export(&doc, 1)
            .unwrap()
            .pixel(16, 16)
            .unwrap()
            .blue(),
        255
    );
    apply(&mut doc, &history.redo().unwrap());
    assert_eq!(doc.find_shape(0, id).unwrap().blend, Blend::Multiply);
    let mut old_shape = serde_json::to_value(doc.find_shape(0, id).unwrap()).unwrap();
    old_shape.as_object_mut().unwrap().remove("blend");
    assert_eq!(
        serde_json::from_value::<Shape>(old_shape).unwrap().blend,
        Blend::Normal
    );
}

#[test]
fn object_opacity_is_applied_once_to_overlapping_fill_and_stroke() {
    let mut shape = rectangle(Rgba::WHITE);
    shape.style.stroke = Some(Stroke {
        color: Rgba::WHITE,
        width: 8.,
        ..Default::default()
    });
    shape.opacity = 0.5;
    let output = render_export(&document(vec![shape]), 1).unwrap();
    for (x, y) in [(16, 16), (6, 16), (16, 6)] {
        assert!((output.pixel(x, y).unwrap().alpha() as i32 - 128).abs() <= 1);
    }
}

#[test]
fn layer_opacity_is_applied_once_to_overlapping_objects() {
    let mut doc = document(vec![rectangle(Rgba::WHITE), rectangle(Rgba::WHITE)]);
    doc.layers[0].opacity = 0.5;
    assert!(
        (render_export(&doc, 1)
            .unwrap()
            .pixel(16, 16)
            .unwrap()
            .alpha() as i32
            - 128)
            .abs()
            <= 1
    );
}

#[test]
fn frame_blend_applies_to_the_whole_subtree() {
    let bottom = rectangle(Rgba::rgb(255, 0, 0));
    let mut frame = rectangle(Rgba::WHITE);
    frame.layout = crate::layout::FrameLayout::frame();
    frame.blend = Blend::Multiply;
    let mut child = rectangle(Rgba::rgb(0, 0, 255));
    child.layout.parent = Some(frame.id);
    let output = render_export(&document(vec![bottom, frame, child]), 1).unwrap();
    let p = output.pixel(16, 16).unwrap();
    assert_eq!([p.red(), p.green(), p.blue(), p.alpha()], [0, 0, 0, 255]);
}

fn center_rgba(doc: &Document) -> [u8; 4] {
    let image = render_export(doc, 1).unwrap();
    let p = image.pixel(16, 16).unwrap();
    [p.red(), p.green(), p.blue(), p.alpha()]
}

fn assert_isolated_blue_across_opacity(mut doc: Document, target: Option<u64>) {
    for opacity in [1.0, 0.999, 0.5] {
        if let Some(id) = target {
            doc.find_shape_mut(0, id).unwrap().opacity = opacity;
        } else {
            doc.layers[1].opacity = opacity;
        }
        let expected = [
            ((1.0 - opacity) * 255.0).round() as u8,
            0,
            (opacity * 255.0).round() as u8,
            255,
        ];
        let actual = center_rgba(&doc);
        for channel in 0..4 {
            assert!(
                (actual[channel] as i32 - expected[channel] as i32).abs() <= 1,
                "opacity={opacity}, got {actual:?}, expected {expected:?}"
            );
        }
        let saved = crate::project::encode(&doc).unwrap();
        let restored = crate::project::decode(&saved).unwrap();
        assert_eq!(center_rgba(&restored), actual);
        let svg = crate::svg::export(&doc).unwrap();
        let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).unwrap();
        let mut image = Pixmap::new(32, 32).unwrap();
        resvg::render(&tree, Transform::identity(), &mut image.as_mut());
        let p = image.pixel(16, 16).unwrap();
        let exported = [p.red(), p.green(), p.blue(), p.alpha()];
        for channel in 0..4 {
            assert!(
                (exported[channel] as i32 - actual[channel] as i32).abs() <= 1,
                "SVG opacity={opacity}, got {exported:?}, native {actual:?}"
            );
        }
    }
}

#[test]
fn regular_layer_child_blend_isolation_is_stable_at_every_opacity_and_after_save() {
    let mut doc = document(vec![rectangle(Rgba::rgb(255, 0, 0))]);
    let mut top = rectangle(Rgba::rgb(0, 0, 255));
    top.blend = Blend::Multiply;
    let mut layer = Layer::vector("Isolated foreground");
    layer.kind.shapes_mut().unwrap().push(top);
    doc.layers.push(layer);
    assert_isolated_blue_across_opacity(doc, None);
}

#[test]
fn frame_child_blend_isolation_is_stable_at_every_opacity_and_after_save() {
    let mut frame = rectangle(Rgba::TRANSPARENT);
    frame.style.fill = Fill::None;
    frame.layout = crate::layout::FrameLayout::frame();
    let id = frame.id;
    let mut child = rectangle(Rgba::rgb(0, 0, 255));
    child.blend = Blend::Multiply;
    child.layout.parent = Some(id);
    let doc = document(vec![rectangle(Rgba::rgb(255, 0, 0)), frame, child]);
    assert_isolated_blue_across_opacity(doc, Some(id));
}

#[test]
fn nested_frame_blend_ancestors_are_isolated_and_html_isolates_only_frames() {
    let mut outer = rectangle(Rgba::TRANSPARENT);
    outer.style.fill = Fill::None;
    outer.layout = crate::layout::FrameLayout::frame();
    let outer_id = outer.id;
    let mut inner = outer.clone();
    inner.id += 10_000;
    inner.layout.parent = Some(outer_id);
    let inner_id = inner.id;
    let mut child = rectangle(Rgba::rgb(0, 0, 255));
    child.blend = Blend::Multiply;
    child.layout.parent = Some(inner_id);
    let child_id = child.id;
    let doc = document(vec![rectangle(Rgba::rgb(255, 0, 0)), outer, inner, child]);
    assert_eq!(center_rgba(&doc), [0, 0, 255, 255]);
    let html = crate::layout_export::export_html(&doc, 0, outer_id).unwrap();
    for (id, isolated) in [(outer_id, true), (inner_id, true), (child_id, false)] {
        let tag = html
            .split(&format!("<div id=\"oma-{id}\""))
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert_eq!(tag.contains("isolation:isolate"), isolated);
    }
}
