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

fn independent_subject() -> Shape {
    let mut s = rectangle(Rgba::rgb(180, 90, 50));
    s.geom = Geom::Rect {
        origin: Pt::new(24., 20.),
        size: Pt::new(48., 40.),
        radius: 4.,
    };
    s.blend = Blend::Overlay;
    s.filters.items = vec![
        crate::filter::Fx::Shadow {
            dx: 10.,
            dy: 8.,
            blur: 3.,
            color: Rgba::rgb(30, 35, 50),
            blend: Blend::Multiply,
            opacity: 0.6,
            knockout: true,
            spread: 0.,
        },
        crate::filter::Fx::InnerShadow {
            dx: 5.,
            dy: 4.,
            blur: 2.,
            color: Rgba::rgb(180, 210, 255),
            blend: Blend::Screen,
            opacity: 0.8,
            choke: 0.,
        },
    ];
    s
}
fn appearance_scene(subject: Shape) -> Document {
    let mut backdrop = rectangle(Rgba::WHITE);
    backdrop.geom = Geom::Rect {
        origin: Pt::ZERO,
        size: Pt::new(96., 80.),
        radius: 0.,
    };
    backdrop.style.fill = Fill::Linear {
        from: [0., 0.],
        to: [1., 0.],
        c0: Rgba::rgb(70, 110, 170),
        c1: Rgba::rgb(220, 180, 100),
    };
    let mut doc = document(vec![backdrop, subject]);
    doc.width = 96.;
    doc.height = 80.;
    doc.artboards.clear();
    doc
}
#[test]
fn independent_effect_reference_golden_overlay_multiply_screen() {
    let scene = appearance_scene(independent_subject());
    let output = render_export(&scene, 1).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/independent-effects.png");
    if std::env::var_os("OMADESIGN_UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        output.save_png(&path).unwrap();
    }
    let golden = Pixmap::load_png(path).unwrap();
    assert_eq!(
        output.data(),
        golden.data(),
        "Independent appearance golden changed"
    );
    // Independent scalar reference at unblurred interiors: object Overlay only.
    let mut backdrop = scene.clone();
    backdrop.layers[0].kind.shapes_mut().unwrap().pop();
    let background = render_export(&backdrop, 1).unwrap();
    let p = background.pixel(48, 40).unwrap();
    let actual = output.pixel(48, 40).unwrap();
    for (b, s, v) in [
        (p.red(), 180, actual.red()),
        (p.green(), 90, actual.green()),
        (p.blue(), 50, actual.blue()),
    ] {
        let b = b as f32 / 255.;
        let s = s as f32 / 255.;
        let expected = if b <= 0.5 {
            2. * b * s
        } else {
            1. - 2. * (1. - b) * (1. - s)
        };
        assert!((v as f32 - expected * 255.).abs() <= 2.);
    }
    // Drop shadow at the offset corner: Multiply at 60% against gradient, no fill.
    let p = background.pixel(76, 60).unwrap();
    let a = output.pixel(76, 60).unwrap();
    assert!(a.red() < p.red() && a.green() < p.green() && a.blue() < p.blue());
    // Inner effect is Screen independently of Overlay.
    let mut no_inner = scene.clone();
    no_inner.layers[0].kind.shapes_mut().unwrap()[1]
        .filters
        .items
        .pop();
    let inner_base = render_export(&no_inner, 1).unwrap();
    assert!(output.pixel(26, 30).unwrap().blue() > inner_base.pixel(26, 30).unwrap().blue());
}
#[test]
fn fill_opacity_knockout_interior_and_object_opacity_are_independent() {
    let mut subject = independent_subject();
    subject.fill_opacity = 0.;
    let id = subject.id;
    let mut scene = appearance_scene(subject);
    let hidden_fill = render_export(&scene, 1).unwrap();
    let mut bare = scene.clone();
    bare.layers[0].kind.shapes_mut().unwrap().pop();
    let bare = render_export(&bare, 1).unwrap();
    assert_eq!(hidden_fill.pixel(48, 40), bare.pixel(48, 40));
    assert_ne!(hidden_fill.pixel(26, 30), bare.pixel(26, 30));
    // Object opacity interpolates the entire contribution, including both effects.
    scene.find_shape_mut(0, id).unwrap().opacity = 0.5;
    let half = render_export(&scene, 1).unwrap();
    for ((a, b), c) in bare.data().iter().zip(hidden_fill.data()).zip(half.data()) {
        assert!((*c as f32 - (*a as f32 + *b as f32) * 0.5).abs() <= 1.);
    }
    scene.find_shape_mut(0, id).unwrap().opacity = 1.;
    scene.find_shape_mut(0, id).unwrap().blend_interior = true;
    assert_ne!(render_export(&scene, 1).unwrap().data(), hidden_fill.data());
    let s = scene.find_shape_mut(0, id).unwrap();
    s.blend_interior = false;
    if let crate::filter::Fx::Shadow { knockout, .. } = &mut s.filters.items[0] {
        *knockout = false;
    }
    assert_ne!(
        render_export(&scene, 1).unwrap().pixel(48, 40),
        hidden_fill.pixel(48, 40)
    );
}
#[test]
fn effect_fields_roundtrip_undo_layer_and_legacy_appearance() {
    let subject = independent_subject();
    let id = subject.id;
    let mut doc = appearance_scene(subject);
    let command = Cmd::Batch(vec![
        Cmd::SetFillOpacity {
            layer: 0,
            id: Some(id),
            before: 1.,
            after: 0.3,
        },
        Cmd::SetBlendInterior {
            layer: 0,
            id: Some(id),
            before: false,
            after: true,
        },
        Cmd::SetFillOpacity {
            layer: 0,
            id: None,
            before: 1.,
            after: 0.8,
        },
    ]);
    let mut history = History::default();
    apply(&mut doc, &command);
    history.push(command);
    let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    assert_eq!(doc.find_shape(0, id), reopened.find_shape(0, id));
    assert_eq!(reopened.layers[0].fill_opacity, 0.8);
    apply(&mut doc, &history.undo().unwrap());
    assert_eq!(doc.find_shape(0, id).unwrap().fill_opacity, 1.);
    apply(&mut doc, &history.redo().unwrap());
    assert_eq!(doc.find_shape(0, id).unwrap().fill_opacity, 0.3);
    let fixture = include_str!("../../tests/fixtures/legacy-effects-v6.oma");
    let old = crate::project::decode(fixture).unwrap();
    let shape = &old.layers[0].kind.shapes().unwrap()[1];
    assert!(shape.filters.legacy_composite);
    assert!(matches!(
        shape.filters.items[0],
        crate::filter::Fx::Shadow {
            blend: Blend::Overlay,
            knockout: false,
            ..
        }
    ));
    let mut expected = old.clone();
    for fx in &mut expected.layers[0].kind.shapes_mut().unwrap()[1]
        .filters
        .items
    {
        if let Some((blend, _)) = fx.appearance_mut() {
            *blend = Blend::Normal;
        }
    }
    assert_eq!(
        render_export(&old, 1).unwrap().data(),
        render_export(&expected, 1).unwrap().data()
    );
    assert_eq!(
        render_export(&old, 1).unwrap().data(),
        render_export(
            &crate::project::decode(&crate::project::encode(&old).unwrap()).unwrap(),
            1
        )
        .unwrap()
        .data()
    );
}
#[test]
fn layer_effects_and_all_new_effects_render_and_export() {
    let mut scene = appearance_scene(independent_subject());
    let subject = scene.layers[0].kind.shapes_mut().unwrap().pop().unwrap();
    let mut layer = Layer::vector("Effect layer");
    let mut content = subject.clone();
    content.filters = Default::default();
    content.blend = Blend::Normal;
    layer.filters = subject.filters.clone();
    layer.blend = subject.blend;
    layer.fill_opacity = 0.4;
    layer.kind.shapes_mut().unwrap().push(content);
    scene.layers.push(layer);
    let rendered = render_export(&scene, 1).unwrap();
    assert_ne!(rendered.pixel(26, 30), rendered.pixel(48, 40));
    for (name, make) in crate::filter::Fx::catalog()
        .iter()
        .filter(|(_, f)| f().appearance().is_some())
    {
        let mut doc = appearance_scene(independent_subject());
        doc.layers[0].kind.shapes_mut().unwrap()[1].filters.items = vec![make()];
        let svg = crate::svg::export(&doc).unwrap();
        assert!(svg.contains("-effect-0"), "{name}");
        assert!(!svg.contains("BackgroundImage"));
        let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).unwrap();
        let mut output = Pixmap::new(96, 80).unwrap();
        resvg::render(&tree, Transform::identity(), &mut output.as_mut());
        assert!(output.data().iter().any(|b| *b > 0));
        assert!(
            crate::filter::export_notes(&doc.layers[0].kind.shapes().unwrap()[1].filters)
                .iter()
                .any(|s| s.contains("separate effect"))
        );
    }
    let psd = crate::formats::psd::encode(&scene, false).unwrap();
    assert!(psd.warnings.iter().any(|w| w.contains("Per-effect blend")));
    assert!(
        crate::formats::pdf::write(&scene)
            .unwrap()
            .1
            .iter()
            .any(|w| w.contains("raster"))
    );
}

#[test]
fn normal_full_strength_effects_keep_the_fast_source_over_output() {
    let mut subject = independent_subject();
    subject.blend = Blend::Normal;
    subject.filters.items.truncate(1);
    if let crate::filter::Fx::Shadow {
        blend,
        opacity,
        knockout,
        ..
    } = &mut subject.filters.items[0]
    {
        *blend = Blend::Normal;
        *opacity = 1.;
        *knockout = false;
    }
    let modern = appearance_scene(subject.clone());
    subject.filters.legacy_composite = true;
    let legacy = appearance_scene(subject);
    let a = render_export(&modern, 1).unwrap();
    let b = render_export(&legacy, 1).unwrap();
    for (a, b) in a.data().iter().zip(b.data()) {
        assert!((*a as i32 - *b as i32).abs() <= 1);
    }
}

#[test]
fn svg_independent_blend_siblings_match_native_without_blur_approximation() {
    let mut subject = independent_subject();
    for effect in &mut subject.filters.items {
        match effect {
            crate::filter::Fx::Shadow { blur, .. }
            | crate::filter::Fx::InnerShadow { blur, .. } => *blur = 0.,
            _ => {}
        }
    }
    for interior in [false, true] {
        subject.blend_interior = interior;
        let doc = appearance_scene(subject.clone());
        let native = render_export(&doc, 1).unwrap();
        let svg = crate::svg::export(&doc).unwrap();
        let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).unwrap();
        let mut output = Pixmap::new(96, 80).unwrap();
        resvg::render(&tree, Transform::identity(), &mut output.as_mut());
        for (x, y) in [(48, 40), (76, 60), (26, 30)] {
            let native = native.pixel(x, y).unwrap();
            let svg = output.pixel(x, y).unwrap();
            for (a, b) in [
                (native.red(), svg.red()),
                (native.green(), svg.green()),
                (native.blue(), svg.blue()),
                (native.alpha(), svg.alpha()),
            ] {
                assert!(
                    (a as i32 - b as i32).abs() <= 2,
                    "interior={interior} at{x},{y}: native={native:?} svg={svg:?}"
                );
            }
        }
    }
}
