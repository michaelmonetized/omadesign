use super::*;
use crate::document::{Pixels, StrokeAlignment};

fn studio() -> Studio {
    let mut s = Studio::new();
    s.doc = Document::new("QA edges", 200., 160., 72.);
    s.doc.transparent = true;
    s.doc.layers = vec![Layer::vector("Artwork")];
    s.active_layer = Some(0);
    s.selection.clear();
    s.history = History::default();
    s
}
fn rect(x: f32, y: f32, w: f32, h: f32) -> Shape {
    Shape::new(
        Geom::Rect {
            origin: Pt::new(x, y),
            size: Pt::new(w, h),
            radius: 0.,
        },
        Style {
            fill: Fill::Solid(Rgba::WHITE),
            stroke: None,
        },
    )
}
#[test]
fn group_align_duplicate_and_individual_edit_preserve_internal_offsets() {
    let mut s = studio();
    let a = rect(10., 20., 20., 20.);
    let b = rect(50., 40., 30., 20.);
    let ids = [a.id, b.id];
    *s.doc.layers[0].kind.shapes_mut().unwrap() = vec![a, b];
    s.selection = ids.iter().map(|&id| (0, id)).collect();
    s.group_selected();
    let hits = s.selection.clone();
    let before = s.doc.find_shape(hits[0].0, ids[0]).unwrap().geom.bbox().min;
    s.align_sel(Align::CenterX);
    let a = s.doc.find_shape(hits[0].0, ids[0]).unwrap().geom.bbox();
    let b = s.doc.find_shape(hits[1].0, ids[1]).unwrap().geom.bbox();
    assert_eq!(a.union(b).center().x, 100.);
    assert_eq!(b.min - a.min, Pt::new(40., 20.));
    s.undo();
    assert_eq!(
        s.doc.find_shape(hits[0].0, ids[0]).unwrap().geom.bbox().min,
        before
    );
    s.redo();
    s.doc
        .motion
        .set_key(hits[0].1, Prop::X, 0., 12., Ease::Linear);
    let original = s.doc.layers.len();
    s.duplicate_selection();
    assert_eq!(s.doc.layers.len(), original * 2);
    assert!(s.doc.validate_hierarchy().is_ok());
    let copied = s.selection.clone();
    assert!(s.doc.motion.has_shape(copied[0].1));
    assert_eq!(
        s.doc
            .find_shape(copied[0].0, copied[0].1)
            .unwrap()
            .geom
            .bbox(),
        a
    );
    s.enter_group_item(copied[0]);
    assert_eq!(s.selection_for_hit(copied[0]), vec![copied[0]]);
    s.deselect_all();
    assert_eq!(s.selection_for_hit(copied[0]).len(), 2);
}
#[test]
fn grouped_raster_and_vector_align_together_with_an_outside_object() {
    let mut s = studio();
    let a = rect(10., 10., 20., 20.);
    let id = a.id;
    *s.doc.layers[0].kind.shapes_mut().unwrap() = vec![a];
    s.doc.layers.push(Layer::placed_raster(
        "Pixels",
        Pixels::new(2, 2),
        Pt::new(50., 20.),
        Pt::splat(20.),
    ));
    s.selection = vec![(0, id), (1, RASTER_ID)];
    s.group_selected();
    let grouped = s.selection.clone();
    s.align_sel(Align::CenterX);
    let vector = s.doc.find_shape(grouped[0].0, id).unwrap().geom.bbox();
    let raster = s.doc.layers[grouped[1].0].kind.raster_bounds().unwrap();
    assert_eq!(raster.min - vector.min, Pt::new(40., 10.));
    assert_eq!(vector.union(raster).center().x, 100.);
    let extra = rect(150., 100., 20., 20.);
    let extra_id = extra.id;
    let mut l = Layer::vector("Outside");
    l.kind.shapes_mut().unwrap().push(extra);
    s.doc.layers.push(l);
    s.selection.push((s.doc.layers.len() - 1, extra_id));
    s.selected_layer = None;
    s.align_sel(Align::CenterX);
    let vector = s.doc.find_shape(grouped[0].0, id).unwrap().geom.bbox();
    let raster = s.doc.layers[grouped[1].0].kind.raster_bounds().unwrap();
    assert_eq!(raster.min - vector.min, Pt::new(40., 10.));
    assert_eq!(
        vector.union(raster).center().x,
        s.doc
            .find_shape(s.doc.layers.len() - 1, extra_id)
            .unwrap()
            .geom
            .bbox()
            .center()
            .x
    );
}
#[test]
fn item_outline_masks_other_layers_and_objects_in_world_space_and_roundtrips() {
    let mut s = studio();
    let source = rect(40., 30., 20., 40.);
    let src = source.id;
    let target = rect(20., 20., 80., 80.);
    let dst = target.id;
    *s.doc.layers[0].kind.shapes_mut().unwrap() = vec![source, target];
    s.select_item_outline(0, Some(src));
    s.mask_object_from_selection(0, dst);
    let object = s.doc.find_shape(0, dst).unwrap();
    assert!(object.mask.is_some());
    let alpha = compositor::item_alpha(
        &s.doc,
        0,
        Some(dst),
        tiny_skia::Transform::identity(),
        200,
        160,
    )
    .unwrap();
    assert_eq!(alpha[40 * 200 + 50], 255);
    assert_eq!(alpha[40 * 200 + 80], 0);
    let encoded = crate::project::encode(&s.doc).unwrap();
    let reopened = crate::project::decode(&encoded).unwrap();
    assert_eq!(
        compositor::item_alpha(
            &reopened,
            0,
            Some(dst),
            tiny_skia::Transform::identity(),
            200,
            160
        )
        .unwrap(),
        alpha
    );
    let svg = crate::svg::export(&reopened).unwrap();
    assert!(svg.contains("oma-object-mask-"));
    s.undo();
    assert!(s.doc.find_shape(0, dst).unwrap().mask.is_none());
    s.redo();
    let pixels = Pixels::from_rgba(40, 40, [255; 4].repeat(1600)).unwrap();
    s.doc.layers.push(Layer::placed_raster(
        "Rotated target",
        pixels,
        Pt::new(20., 20.),
        Pt::splat(80.),
    ));
    s.doc.layers[1].kind.set_raster_xform(
        Pt::new(20., 20.),
        Pt::splat(80.),
        std::f32::consts::FRAC_PI_2,
    );
    s.activate_layer_tree(1);
    assert!(s.pixel_sel.is_some());
    s.mask_from_selection(1);
    let alpha = compositor::item_alpha(&s.doc, 1, None, tiny_skia::Transform::identity(), 200, 160)
        .unwrap();
    assert!(alpha[40 * 200 + 50] > 250);
    assert_eq!(alpha[40 * 200 + 80], 0);
    s.begin_item_mask(0, Some(src));
    assert!(s.apply_pending_item_mask(1, None));
    assert!(s.pending_item_mask.is_none());
}
#[test]
fn aligned_strokes_render_and_expand_on_the_requested_side() {
    for alignment in [
        StrokeAlignment::Inside,
        StrokeAlignment::Center,
        StrokeAlignment::Outside,
    ] {
        let mut s = studio();
        let mut shape = rect(60., 50., 60., 60.);
        let id = shape.id;
        shape.style.fill = Fill::None;
        shape.style.stroke = Some(Stroke {
            width: 12.,
            color: Rgba::WHITE,
            alignment,
            join: Join::Miter,
            ..Default::default()
        });
        s.doc.layers[0]
            .kind
            .shapes_mut()
            .unwrap()
            .push(shape.clone());
        let alpha = compositor::item_alpha(
            &s.doc,
            0,
            Some(id),
            tiny_skia::Transform::identity(),
            200,
            160,
        )
        .unwrap();
        let (outer, inner) = (alpha[80 * 200 + 55], alpha[80 * 200 + 65]);
        match alignment {
            StrokeAlignment::Inside => {
                assert_eq!(outer, 0);
                assert_eq!(inner, 255);
            }
            StrokeAlignment::Outside => {
                assert_eq!(outer, 255);
                assert_eq!(inner, 0);
            }
            StrokeAlignment::Center => {
                assert_eq!(outer, 255);
                assert_eq!(inner, 255);
            }
        }
        let outline = crate::outline::expand(&shape).unwrap();
        assert_eq!(outline.contains(Pt::new(55.5, 80.5)), outer > 128);
        assert_eq!(outline.contains(Pt::new(65.5, 80.5)), inner > 128);
        let svg = crate::svg::export(&s.doc).unwrap();
        let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).unwrap();
        let mut pm = tiny_skia::Pixmap::new(200, 160).unwrap();
        resvg::render(&tree, tiny_skia::Transform::identity(), &mut pm.as_mut());
        assert_eq!(pm.pixel(55, 80).unwrap().alpha(), outer);
        assert_eq!(pm.pixel(65, 80).unwrap().alpha(), inner);
    }
}
#[test]
fn guides_start_locked_and_clear_all_is_reversible() {
    let mut s = studio();
    assert!(s.doc.ruler.guides_locked);
    let mut guide = rect(10., 10., 20., 20.);
    guide.guide = true;
    let id = guide.id;
    s.doc.layers[0].kind.shapes_mut().unwrap().push(guide);
    s.doc.guides.push(crate::document::Guide {
        vertical: true,
        pos: 40.,
    });
    assert_eq!(s.doc.hit_test(Pt::new(10., 20.), 2.), None);
    s.set_guides_locked(false);
    assert_eq!(s.doc.hit_test(Pt::new(10., 20.), 2.), Some((0, id)));
    s.clear_guides();
    assert!(s.doc.guides.is_empty());
    assert!(s.doc.layers[0].kind.shapes().unwrap().is_empty());
    s.undo();
    assert_eq!(s.doc.guides.len(), 1);
    assert!(s.doc.find_shape(0, id).unwrap().guide);
}
#[test]
fn design_eyedropper_samples_only_the_active_raster() {
    let mut s = studio();
    let red = Pixels::from_rgba(2, 2, [200, 10, 20, 255].repeat(4)).unwrap();
    s.doc.layers.push(Layer::placed_raster(
        "Active",
        red,
        Pt::new(20., 20.),
        Pt::splat(20.),
    ));
    s.active_layer = Some(1);
    s.persona = Persona::Design;
    s.eyedrop(Pt::new(25., 25.));
    assert_eq!(s.brush.color, Rgba::rgb(200, 10, 20));
    s.active_layer = Some(0);
    s.brush.color = Rgba::BLACK;
    s.eyedrop(Pt::new(25., 25.));
    assert_eq!(s.brush.color, Rgba::BLACK);
}

#[test]
fn a_mask_on_a_layout_frame_covers_its_entire_subtree_in_native_and_svg() {
    let mut s = studio();
    let mut frame = crate::layout::make_frame(Pt::new(40., 40.), Pt::new(80., 60.));
    frame.style.fill = Fill::None;
    frame.style.stroke = None;
    frame.mask = Pixels::from_rgba(2, 1, vec![255, 255, 255, 255, 0, 0, 0, 255]);
    let mut child = rect(40., 40., 80., 60.);
    child.layout.parent = Some(frame.id);
    *s.doc.layers[0].kind.shapes_mut().unwrap() = vec![frame, child];
    let native =
        compositor::item_alpha(&s.doc, 0, None, tiny_skia::Transform::identity(), 200, 160)
            .unwrap();
    let svg = crate::svg::export(&s.doc).unwrap();
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).unwrap();
    let mut rendered = tiny_skia::Pixmap::new(200, 160).unwrap();
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut rendered.as_mut(),
    );
    assert_eq!(native[70 * 200 + 50], 255);
    assert_eq!(native[70 * 200 + 110], 0);
    assert_eq!(rendered.pixel(50, 70).unwrap().alpha(), 255);
    assert_eq!(rendered.pixel(110, 70).unwrap().alpha(), 0);
}

#[test]
fn moving_a_group_preserves_hidden_and_locked_child_offsets() {
    let mut s = studio();
    let a = rect(10., 10., 20., 20.);
    let b = rect(50., 10., 20., 20.);
    let ids = [a.id, b.id];
    *s.doc.layers[0].kind.shapes_mut().unwrap() = vec![a, b];
    s.selection = ids.map(|id| (0, id)).to_vec();
    s.group_selected();
    let selected = s.selection.clone();
    let second = s.doc.layers[selected[1].0].find_mut(selected[1].1).unwrap();
    second.visible = false;
    second.locked = true;
    s.deselect_all();
    s.selection = s.selection_for_hit(selected[0]);
    assert_eq!(s.selection.len(), 2);
    s.nudge(10., 20.);
    assert_eq!(
        s.doc
            .find_shape(selected[0].0, ids[0])
            .unwrap()
            .geom
            .bbox()
            .min,
        Pt::new(20., 30.)
    );
    assert_eq!(
        s.doc
            .find_shape(selected[1].0, ids[1])
            .unwrap()
            .geom
            .bbox()
            .min,
        Pt::new(60., 30.)
    );
}

#[test]
fn grouped_canvas_rasters_are_movable_and_advanced_saves_require_a_new_reader() {
    let mut s = studio();
    s.doc.layers = vec![
        Layer::raster("Paint 1", 20, 20),
        Layer::raster("Paint 2", 20, 20),
    ];
    s.selection = vec![(0, RASTER_ID), (1, RASTER_ID)];
    s.group_selected();
    assert_eq!(s.selection.len(), 2);
    s.nudge(15., 25.);
    for &(li, _) in &s.selection {
        assert_eq!(
            s.doc.layers[li].kind.raster_xform().unwrap().0,
            Pt::new(15., 25.)
        );
    }
    let mut plain = Document::new("Compatibility", 40., 40., 72.);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&crate::project::encode(&plain).unwrap())
            .unwrap()["version"],
        5
    );
    let mut shape = rect(5., 5., 20., 20.);
    shape.mask = Some(Pixels::new(2, 2));
    plain.layers.push(Layer::vector("Masked"));
    plain
        .layers
        .last_mut()
        .unwrap()
        .kind
        .shapes_mut()
        .unwrap()
        .push(shape);
    let encoded = crate::project::encode(&plain).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&encoded).unwrap()["version"],
        6
    );
    assert!(crate::project::decode(&encoded).is_ok());
}
