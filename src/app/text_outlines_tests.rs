use super::*;
use crate::{
    color::Rgba,
    document::{Document, Layer, Pixels},
    geom::TypeRun,
};

fn fixture() -> (Studio, (usize, u64)) {
    let mut studio = Studio::new();
    studio.show_welcome = false;
    studio.persona = Persona::Layout;
    studio.doc = Document::new("Painted outlines", 480., 220., 96.);
    studio.doc.transparent = true;
    studio.doc.workspace = Some(Persona::Layout);
    studio.doc.layers = vec![Layer::vector("Editable lettering")];
    let mut run = TypeRun {
        origin: Pt::new(40., 105.),
        content: "Colour & curves".into(),
        px: 38.,
        font: concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/assets/fonts/EBGaramond.ttf"
        )
        .into(),
        ..Default::default()
    };
    run.set_character_style(0, 6, |span| {
        span.fill = Some(Fill::Solid(Rgba::rgb(220, 40, 75)));
        span.tracking = Some(25.);
    });
    run.set_character_style(9, 15, |span| {
        span.fill = Some(Fill::Solid(Rgba::rgb(20, 140, 210)));
        span.px = Some(42.);
    });
    let mut shape = Shape::new(
        Geom::Text(run),
        Style {
            fill: Fill::Solid(Rgba::rgb(30, 80, 40)),
            stroke: None,
        },
    );
    shape.name = "Painted title".into();
    crate::text::fill_contours(&mut shape.geom);
    let hit = (0, shape.id);
    studio.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
    studio.active_layer = Some(0);
    studio.selection = vec![hit];
    studio.mark();
    studio.history.clear();
    (studio, hit)
}
fn wire(studio: &Studio) -> String {
    crate::project::encode(&studio.doc).unwrap()
}
fn image(doc: &Document) -> tiny_skia::Pixmap {
    crate::compositor::render_export(doc, 1).unwrap()
}
fn same_paint(before: &tiny_skia::Pixmap, after: &tiny_skia::Pixmap) {
    let mut sum = 0usize;
    let mut count = 0usize;
    let mut max = 0u8;
    for (a, b) in before
        .data()
        .chunks_exact(4)
        .zip(after.data().chunks_exact(4))
    {
        if a[3] == 0 && b[3] == 0 {
            continue;
        }
        for (a, b) in a.iter().zip(b) {
            let d = a.abs_diff(*b);
            sum += usize::from(d);
            max = max.max(d);
            count += 1;
        }
    }
    let mean = sum as f64 / count.max(1) as f64;
    if let Some(root) = std::env::var_os("OMADESIGN_OUTLINES_QA") {
        std::fs::create_dir_all(&root).unwrap();
        let name = std::thread::current()
            .name()
            .unwrap_or("outline")
            .replace("::", "-");
        before
            .save_png(std::path::Path::new(&root).join(format!("{name}-before.png")))
            .unwrap();
        after
            .save_png(std::path::Path::new(&root).join(format!("{name}-after.png")))
            .unwrap();
    }
    assert!(
        mean <= 0.1 && max <= 16,
        "visible outline paint changed: mean={mean}, max={max}, components={count}"
    );
}
fn reopen(studio: &Studio) {
    let saved = wire(studio);
    let doc = crate::project::decode(&saved).unwrap();
    assert_eq!(crate::project::encode(&doc).unwrap(), saved);
    assert_eq!(image(&doc).data(), image(&studio.doc).data());
    assert!(
        doc.layers
            .iter()
            .filter_map(|l| l.kind.shapes())
            .flatten()
            .all(|s| !matches!(s.geom, Geom::Text(_)))
    );
}
fn emit(name: &str, bytes: &[u8]) {
    if let Some(root) = std::env::var_os("OMADESIGN_OUTLINES_QA") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(std::path::Path::new(&root).join(name), bytes).unwrap();
    }
}

#[test]
fn craft_painted_outlines_keep_colours_stroke_parent_rotation_and_one_undo() {
    let (mut studio, hit) = fixture();
    let root = studio.doc.find_shape_mut(hit.0, hit.1).unwrap();
    root.rotation = 0.23;
    root.style.stroke = Some(Stroke {
        width: 1.25,
        color: Rgba::rgb(30, 20, 60),
        ..Default::default()
    });
    let mut parent = crate::layout::make_frame(Pt::new(10., 10.), Pt::new(450., 190.));
    parent.style.fill = Fill::None;
    parent.style.stroke = None;
    parent.rotation = -0.13;
    parent.layout.clip = false;
    studio
        .doc
        .find_shape_mut(hit.0, hit.1)
        .unwrap()
        .layout
        .parent = Some(parent.id);
    studio.doc.layers[0]
        .kind
        .shapes_mut()
        .unwrap()
        .insert(0, parent);
    studio.mark();
    let before = wire(&studio);
    let picture = image(&studio.doc);
    emit("colours-before.oma", before.as_bytes());
    studio.create_text_outlines();
    assert_eq!(studio.history.len(), 1);
    let root = studio.doc.find_shape(hit.0, hit.1).unwrap();
    assert!(root.layout.frame);
    assert_eq!(root.rotation, 0.23);
    assert!(root.style.fill.is_none());
    let children: Vec<_> = studio.doc.layers[0]
        .kind
        .shapes()
        .unwrap()
        .iter()
        .filter(|s| s.layout.parent == Some(hit.1))
        .collect();
    assert!(children.len() >= 4);
    assert_eq!(
        children.iter().filter(|s| s.style.stroke.is_some()).count(),
        1
    );
    assert!(children.last().unwrap().style.stroke.is_some());
    assert!(children.iter().all(|s| !matches!(s.geom, Geom::Text(_))));
    same_paint(&picture, &image(&studio.doc));
    let after = wire(&studio);
    reopen(&studio);
    emit("colours-outlined.oma", after.as_bytes());
    studio.undo();
    assert_eq!(wire(&studio), before);
    studio.redo();
    assert_eq!(wire(&studio), after);
}

#[test]
fn craft_painted_outlines_preserve_relative_linear_radial_conic_and_pattern_space() {
    let mut radial = Gradient::new(
        GradientKind::Radial,
        Rgba::rgb(240, 70, 30),
        Rgba::rgb(20, 80, 230),
    );
    radial.from = [0.2, 0.4];
    radial.to = [0.9, 0.55];
    radial.radial_axis = Some([-0.1, 0.85]);
    radial.spread = crate::gradient::Spread::Reflect;
    radial.focal = [0.2, 0.1];
    radial.inner_radius = 0.1;
    let pattern = crate::svg_source::SvgPattern {
        definitions: std::sync::Arc::from(
            r##"<pattern id="stripes" patternUnits="userSpaceOnUse" width=".05" height=".2"><rect width=".025" height=".2" fill="#dd2255"/><rect x=".025" width=".025" height=".2" fill="#22aadd"/></pattern>"##,
        ),
        id: "stripes".into(),
        transform: [1., 0., 0., 1., 0., 0.],
        opacity: 0.85,
    };
    for (index, fill) in [
        Fill::Linear {
            from: [-0.2, 0.1],
            to: [0.9, 0.8],
            c0: Rgba::rgb(220, 20, 40),
            c1: Rgba::rgb(20, 130, 230),
        },
        Fill::Radial {
            c0: Rgba::rgb(240, 30, 60),
            c1: Rgba::rgb(20, 130, 230),
        },
        Fill::Gradient(radial),
        Fill::Gradient(Gradient::new(
            GradientKind::Conic,
            Rgba::rgb(240, 30, 60),
            Rgba::rgb(20, 130, 230),
        )),
        Fill::Pattern(pattern),
    ]
    .into_iter()
    .enumerate()
    {
        let (mut studio, hit) = fixture();
        let Geom::Text(run) = &mut studio.doc.find_shape_mut(hit.0, hit.1).unwrap().geom else {
            unreachable!()
        };
        run.spans.clear();
        run.set_character_style(0, 6, |span| span.fill = Some(fill.clone()));
        studio.mark();
        let before = image(&studio.doc);
        studio.create_text_outlines();
        assert_eq!(studio.history.len(), 1, "fill{index}: {}", studio.status);
        same_paint(&before, &image(&studio.doc));
        reopen(&studio);
        emit(&format!("paint-{index}.oma"), wire(&studio).as_bytes());
    }
}

#[test]
fn craft_painted_outlines_keep_object_mask_opacity_and_effects_together() {
    let (mut studio, hit) = fixture();
    let shape = studio.doc.find_shape_mut(hit.0, hit.1).unwrap();
    shape.opacity = 0.6;
    shape.fill_opacity = 0.8;
    shape
        .filters
        .items
        .push(crate::filter::Fx::Blur { std: 1.25 });
    shape.mask = Some(Pixels::from_rgba(2, 1, vec![255, 255, 255, 255, 90, 90, 90, 255]).unwrap());
    studio.mark();
    let before = image(&studio.doc);
    let original = studio.doc.find_shape(hit.0, hit.1).unwrap().clone();
    studio.create_text_outlines();
    assert_eq!(studio.history.len(), 1);
    same_paint(&before, &image(&studio.doc));
    let root = studio.doc.find_shape(hit.0, hit.1).unwrap();
    assert_eq!(root.opacity, original.opacity);
    assert_eq!(root.fill_opacity, original.fill_opacity);
    assert_eq!(root.filters, original.filters);
    assert_eq!(
        root.mask.as_ref().unwrap().data,
        original.mask.as_ref().unwrap().data
    );
    assert!(
        studio.doc.layers[0]
            .kind
            .shapes()
            .unwrap()
            .iter()
            .filter(|s| s.id != hit.1)
            .all(|s| s.filters.is_empty() && s.mask.is_none() && s.opacity == 1.)
    );
    reopen(&studio);
}

#[test]
fn craft_painted_outlines_split_linked_story_without_changing_other_frames() {
    let (mut studio, head) = fixture();
    let Geom::Text(run) = &mut studio.doc.find_shape_mut(head.0, head.1).unwrap().geom else {
        unreachable!()
    };
    run.px = 22.;
    run.content = "Every character remains editable in the other frames. ".repeat(4);
    run.frame = Some(crate::text_geometry::TextFrame {
        size: Pt::new(110., 120.),
        ..Default::default()
    });
    let mut ids = vec![head];
    for i in 1..3 {
        let mut shape = studio.doc.find_shape(head.0, head.1).unwrap().clone();
        shape.id = crate::document::next_id();
        let Geom::Text(run) = &mut shape.geom else {
            unreachable!()
        };
        run.content.clear();
        run.origin.x += i as f32 * 140.;
        let id = shape.id;
        studio.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
        ids.push((0, id));
    }
    studio.mark();
    studio.thread_text_frames(ids[0], ids[1]).unwrap();
    studio.thread_text_frames(ids[1], ids[2]).unwrap();
    studio.selection = vec![ids[1]];
    studio.history.clear();
    let before = wire(&studio);
    let picture = image(&studio.doc);
    studio.create_text_outlines();
    assert_eq!(studio.history.len(), 1);
    same_paint(&picture, &image(&studio.doc));
    assert!(
        studio
            .doc
            .find_shape(ids[1].0, ids[1].1)
            .unwrap()
            .layout
            .frame
    );
    assert!(matches!(
        &studio.doc.find_shape(ids[0].0, ids[0].1).unwrap().geom,
        Geom::Text(_)
    ));
    assert!(matches!(
        &studio.doc.find_shape(ids[2].0, ids[2].1).unwrap().geom,
        Geom::Text(_)
    ));
    let after = wire(&studio);
    let reopened = crate::project::decode(&after).unwrap();
    assert_eq!(crate::project::encode(&reopened).unwrap(), after);
    assert_eq!(image(&reopened).data(), image(&studio.doc).data());
    studio.undo();
    assert_eq!(wire(&studio), before);
    studio.redo();
    assert_eq!(wire(&studio), after);
    emit("linked-outlined.oma", after.as_bytes());
}

#[test]
fn craft_painted_outlines_reject_unsupported_or_locked_owners_before_draft_and_redo() {
    for mode in 0..5 {
        let (mut studio, hit) = fixture();
        studio.commit(Cmd::SetOpacity {
            layer: hit.0,
            id: hit.1,
            before: 1.,
            after: 0.7,
        });
        studio.undo();
        studio.begin_type_edit(hit, Pt::new(60., 105.));
        studio.type_insert("draft");
        let draft = studio
            .type_edit
            .as_ref()
            .map(|edit| (edit.caret, edit.anchor));
        assert!(draft.is_some());
        assert!(studio.history.can_redo());
        let shape = studio.doc.find_shape_mut(hit.0, hit.1).unwrap();
        if mode == 0 {
            shape.locked = true;
        }
        if mode == 1 {
            let Geom::Text(run) = &mut shape.geom else {
                unreachable!()
            };
            run.set_character_style(0, 6, |s| {
                s.fill = Some(Fill::Gradient(Gradient::new(
                    GradientKind::Shape,
                    Rgba::BLACK,
                    Rgba::WHITE,
                )))
            });
        }
        if mode == 2 {
            studio.updates.freezing = true;
        }
        if mode == 4 {
            let Geom::Text(run) = &mut shape.geom else {
                unreachable!()
            };
            run.spans.clear();
            shape.style.fill =
                Fill::Gradient(Gradient::new(GradientKind::Shape, Rgba::BLACK, Rgba::WHITE));
        }
        if mode == 3 {
            studio
                .doc
                .motion
                .set_color_key(hit.1, 0., Rgba::BLACK, crate::motion::Ease::Linear);
        }
        studio.mark();
        let before = wire(&studio);
        studio.create_text_outlines();
        assert_eq!(wire(&studio), before);
        assert_eq!(studio.history.len(), 0);
        assert_eq!(
            studio
                .type_edit
                .as_ref()
                .map(|edit| (edit.caret, edit.anchor)),
            draft
        );
        assert!(studio.history.can_redo());
    }
}

#[test]
fn craft_painted_outlines_multi_object_keeps_non_text_and_all_exports_are_vector() {
    let (mut studio, hit) = fixture();
    let mut second = studio.doc.find_shape(hit.0, hit.1).unwrap().clone();
    second.id = crate::document::next_id();
    second.geom.translate(Pt::new(0., 70.));
    let other = second.id;
    studio.doc.layers[0].kind.shapes_mut().unwrap().push(second);
    let rect = Shape::new(
        Geom::Rect {
            origin: Pt::new(420., 20.),
            size: Pt::new(15., 15.),
            radius: 0.,
        },
        Style {
            fill: Fill::Solid(Rgba::rgb(30, 180, 50)),
            stroke: None,
        },
    );
    let rect_id = rect.id;
    studio.doc.layers[0]
        .kind
        .shapes_mut()
        .unwrap()
        .push(rect.clone());
    studio.selection = vec![hit, (0, other), (0, rect_id)];
    studio.mark();
    let before = wire(&studio);
    let picture = image(&studio.doc);
    emit("multi-before.oma", before.as_bytes());
    studio.create_text_outlines();
    assert_eq!(studio.history.len(), 1);
    assert_eq!(studio.doc.find_shape(0, rect_id), Some(&rect));
    same_paint(&picture, &image(&studio.doc));
    let svg = crate::svg::export(&studio.doc).unwrap();
    assert!(!svg.contains("<text"));
    assert!(svg.contains("<path"));
    assert!(!svg.contains("<image"));
    let (pdf, _) = crate::formats::pdf::write(&studio.doc).unwrap();
    let p = lopdf::Document::load_mem(&pdf).unwrap();
    assert!(!p.objects.values().any(|o| {
        o.as_stream()
            .ok()
            .and_then(|s| s.dict.get(b"Subtype").ok())
            .and_then(|o| o.as_name().ok())
            == Some(b"Image".as_slice())
    }));
    let (idml, _) = crate::formats::idml::write(&studio.doc).unwrap();
    emit("outlines.svg", svg.as_bytes());
    emit("outlines.pdf", &pdf);
    emit("outlines.idml", &idml);
    emit("outlines.oma", wire(&studio).as_bytes());
    reopen(&studio);
    studio.undo();
    assert_eq!(wire(&studio), before);
}

#[test]
fn craft_painted_outlines_keep_unpainted_glyphs_editable() {
    let (mut studio, hit) = fixture();
    let shape = studio.doc.find_shape_mut(hit.0, hit.1).unwrap();
    let Geom::Text(run) = &mut shape.geom else {
        unreachable!()
    };
    run.spans.clear();
    run.set_character_style(0, 6, |s| s.fill = Some(Fill::None));
    shape.style.fill = Fill::None;
    crate::text::fill_contours(&mut shape.geom);
    studio.mark();
    let before = wire(&studio);
    studio.create_text_outlines();
    assert_eq!(studio.history.len(), 1);
    assert!(image(&studio.doc).pixels().iter().all(|p| p.alpha() == 0));
    let paths: Vec<_> = studio.doc.layers[0]
        .kind
        .shapes()
        .unwrap()
        .iter()
        .filter(|s| s.layout.parent == Some(hit.1))
        .collect();
    assert!(!paths.is_empty());
    assert!(paths.iter().all(|s| s.geom.anchors().next().is_some()));
    let id = paths[0].id;
    let saved = wire(&studio);
    studio.doc.find_shape_mut(0, id).unwrap().style.fill = Fill::Solid(Rgba::BLACK);
    assert!(image(&studio.doc).pixels().iter().any(|p| p.alpha() > 0));
    studio.doc = crate::project::decode(&saved).unwrap();
    reopen(&studio);
    studio.undo();
    assert_eq!(wire(&studio), before);
}

#[test]
fn craft_painted_outlines_keep_transform_animation_and_original_owner() {
    let (mut studio, hit) = fixture();
    for (prop, first, last) in [
        (crate::motion::Prop::X, 0., 12.),
        (crate::motion::Prop::Y, 0., -8.),
        (crate::motion::Prop::Rotation, 0., 0.15),
        (crate::motion::Prop::Scale, 1., 0.95),
        (crate::motion::Prop::Width, 1., 0.8),
        (crate::motion::Prop::Height, 1., 1.2),
        (crate::motion::Prop::Opacity, 1., 0.6),
    ] {
        for (time, value) in [(0., first), (1., last)] {
            studio
                .doc
                .motion
                .set_key(hit.1, prop, time, value, crate::motion::Ease::Linear);
        }
    }
    studio.mark();
    let before = studio.doc.clone();
    let before_wire = wire(&studio);
    studio.create_text_outlines();
    assert_eq!(studio.doc.motion, before.motion);
    assert!(studio.doc.find_shape(hit.0, hit.1).unwrap().layout.frame);
    for time in [0., 0.5, 1.] {
        same_paint(
            &crate::compositor::render_export_at(&before, 1., time, true).unwrap(),
            &crate::compositor::render_export_at(&studio.doc, 1., time, true).unwrap(),
        );
    }
    let after = wire(&studio);
    reopen(&studio);
    studio.undo();
    assert_eq!(wire(&studio), before_wire);
    studio.redo();
    assert_eq!(wire(&studio), after);
}

#[test]
fn craft_painted_outlines_native_path_text_keeps_visible_curves_and_paint() {
    let (mut studio, hit) = fixture();
    let guide = Shape::new(
        Geom::Line {
            a: Pt::new(30., 160.),
            b: Pt::new(440., 80.),
        },
        Style {
            fill: Fill::None,
            stroke: None,
        },
    );
    let guide_id = guide.id;
    studio.doc.layers[0].kind.shapes_mut().unwrap().push(guide);
    studio.selection = vec![hit, (0, guide_id)];
    studio.attach_text_path();
    studio.selection = vec![hit];
    studio.history.clear();
    assert!(studio.selected_type().unwrap().on_path.is_some());
    let before = wire(&studio);
    let picture = image(&studio.doc);
    studio.create_text_outlines();
    assert_eq!(studio.history.len(), 1);
    same_paint(&picture, &image(&studio.doc));
    reopen(&studio);
    let after = wire(&studio);
    studio.undo();
    assert_eq!(wire(&studio), before);
    studio.redo();
    assert_eq!(wire(&studio), after);
    emit("path-outlined.oma", after.as_bytes());
}
