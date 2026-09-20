use super::*;

fn studio() -> Studio {
    let mut s = Studio::new();
    s.doc = Document::new("Nesting", 1.0, 1.0, 96.0);
    s.doc.layers = vec![Layer::vector("Icons"), Layer::vector("Layout")];
    s.active_layer = Some(0);
    s.show_welcome = false;
    s.persona = Persona::Layout;
    s
}

fn rect(origin: Pt, parent: Option<u64>) -> Shape {
    let mut shape = Shape::new(
        Geom::Rect {
            origin,
            size: Pt::new(20.0, 30.0),
            radius: 0.0,
        },
        Style::default(),
    );
    shape.layout.parent = parent;
    shape
}

fn contents(s: &Studio) -> Vec<Vec<Shape>> {
    s.doc
        .layers
        .iter()
        .filter_map(|l| l.kind.shapes().map(<[_]>::to_vec))
        .collect()
}

fn corners(doc: &Document, li: usize, id: u64) -> Vec<Pt> {
    let shape = doc.find_shape(li, id).unwrap();
    let mut points: Vec<_> = shape.world_contours(16).into_iter().flatten().collect();
    let mut parent = shape.layout.parent;
    while let Some(id) = parent {
        let frame = doc.find_shape(li, id).unwrap();
        for point in &mut points {
            *point = point.rotate_about(frame.geom.bbox().center(), frame.rotation);
        }
        parent = frame.layout.parent;
    }
    points
}

#[test]
fn cross_layer_reparent_moves_subtree_without_flattening_or_losing_motion() {
    let mut s = studio();
    let frame = layout::make_frame(Pt::ZERO, Pt::splat(300.0));
    let parent = frame.id;
    let nested = layout::make_frame(Pt::splat(20.0), Pt::splat(120.0));
    let nested_id = nested.id;
    let icon = rect(Pt::splat(30.0), Some(nested_id));
    let icon_id = icon.id;
    s.doc.layers[0]
        .kind
        .shapes_mut()
        .unwrap()
        .extend([icon, nested]);
    s.doc.layers[1].kind.shapes_mut().unwrap().push(frame);
    s.doc.motion.set_key(
        icon_id,
        crate::motion::Prop::X,
        1.0,
        20.0,
        crate::motion::Ease::Linear,
    );
    let before = contents(&s);
    let motion = s.doc.motion.clone();
    s.selection = vec![(0, icon_id), (0, nested_id)];
    s.reparent_layout_selection(Some(parent));
    assert!(s.doc.layers[0].kind.shapes().unwrap().is_empty());
    assert_eq!(
        s.doc.find_shape(1, nested_id).unwrap().layout.parent,
        Some(parent)
    );
    assert_eq!(
        s.doc.find_shape(1, icon_id).unwrap().layout.parent,
        Some(nested_id)
    );
    assert_eq!(s.selection, vec![(1, nested_id)]);
    assert_eq!(s.active_layer, Some(1));
    assert_eq!(s.doc.motion, motion);
    assert!(s.doc.validate_hierarchy().is_ok());
    let encoded = crate::project::encode(&s.doc).unwrap();
    assert!(
        crate::project::decode(&encoded)
            .unwrap()
            .validate_hierarchy()
            .is_ok()
    );
    let after = contents(&s);
    s.undo();
    assert_eq!(contents(&s), before);
    assert_eq!(s.doc.motion, motion);
    s.redo();
    assert_eq!(contents(&s), after);
}

#[test]
fn nesting_and_detaching_preserve_world_geometry_through_rotated_ancestors() {
    let mut s = studio();
    let mut outer = layout::make_frame(Pt::splat(10.0), Pt::splat(600.0));
    outer.rotation = 0.7;
    let mut inner = layout::make_frame(Pt::splat(30.0), Pt::splat(280.0));
    inner.layout.parent = Some(outer.id);
    inner.rotation = -0.3;
    let mut nested = layout::make_frame(Pt::splat(50.0), Pt::splat(100.0));
    nested.layout.parent = Some(inner.id);
    nested.rotation = 0.2;
    let nested_id = nested.id;
    let mut child = rect(Pt::splat(60.0), Some(nested.id));
    child.rotation = -0.1;
    let child_id = child.id;
    let mut target = layout::make_frame(Pt::new(60.0, 80.0), Pt::splat(500.0));
    target.rotation = -0.6;
    let target_id = target.id;
    s.doc.layers[0]
        .kind
        .shapes_mut()
        .unwrap()
        .extend([outer, inner, nested, child]);
    s.doc.layers[1].kind.shapes_mut().unwrap().push(target);
    let before: Vec<_> = [nested_id, child_id]
        .into_iter()
        .map(|id| corners(&s.doc, 0, id))
        .collect();
    s.selection = vec![(0, nested_id)];
    s.reparent_layout_selection(Some(target_id));
    for (id, points) in [nested_id, child_id].into_iter().zip(&before) {
        for (a, b) in corners(&s.doc, 1, id).into_iter().zip(points) {
            assert!((a - *b).length() < 0.002, "nest moved {id}: {a:?} != {b:?}");
        }
    }
    s.reparent_layout_selection(None);
    assert_eq!(s.doc.find_shape(1, nested_id).unwrap().layout.parent, None);
    assert_eq!(
        s.doc.find_shape(1, child_id).unwrap().layout.parent,
        Some(nested_id)
    );
    for (id, points) in [nested_id, child_id].into_iter().zip(&before) {
        for (a, b) in corners(&s.doc, 1, id).into_iter().zip(points) {
            assert!((a - *b).length() < 0.002, "detach moved {id}");
        }
    }
}

#[test]
fn invalid_destinations_and_locked_ancestors_reject_the_whole_edit() {
    let mut s = studio();
    let outer = layout::make_frame(Pt::ZERO, Pt::splat(300.0));
    let outer_id = outer.id;
    let mut inner = layout::make_frame(Pt::splat(20.0), Pt::splat(100.0));
    inner.layout.parent = Some(outer_id);
    let inner_id = inner.id;
    let icon = rect(Pt::splat(30.0), None);
    let icon_id = icon.id;
    s.doc.layers[0].kind.shapes_mut().unwrap().push(icon);
    s.doc.layers[1]
        .kind
        .shapes_mut()
        .unwrap()
        .extend([outer, inner]);
    s.selection = vec![(0, icon_id), (1, outer_id)];
    let before = contents(&s);
    s.reparent_layout_selection(Some(inner_id));
    assert_eq!(contents(&s), before);
    assert!(s.status.contains("itself"));
    s.selection = vec![(0, icon_id)];
    s.doc.find_shape_mut(1, outer_id).unwrap().locked = true;
    let before = contents(&s);
    s.reparent_layout_selection(Some(inner_id));
    assert_eq!(contents(&s), before);
    assert!(s.status.contains("unlock"));
    s.reparent_layout_selection(Some(icon_id));
    assert_eq!(contents(&s), before);
    assert!(s.status.contains("existing frame"));
    assert!(!s.dirty);
}

#[test]
fn placed_raster_becomes_embedded_image_and_undo_restores_original_layer() {
    let mut s = studio();
    let pixels =
        crate::document::Pixels::from_rgba(2, 1, vec![255, 0, 0, 255, 0, 128, 255, 128]).unwrap();
    let mut raster = Layer::placed_raster(
        "Placed SVG",
        pixels,
        Pt::new(70.0, 90.0),
        Pt::new(80.0, 40.0),
    );
    raster
        .kind
        .set_raster_xform(Pt::new(70.0, 90.0), Pt::new(80.0, 40.0), 0.2);
    s.doc.layers[0] = raster;
    let frame = layout::make_frame(Pt::ZERO, Pt::splat(300.0));
    let parent = frame.id;
    s.doc.layers[1].kind.shapes_mut().unwrap().push(frame);
    let original_pixels = s.doc.layers[0].kind.pixels().unwrap().data.clone();
    let original_bounds = s.doc.layers[0].kind.raster_bounds().unwrap();
    s.selection = vec![(0, RASTER_ID)];
    s.reparent_layout_selection(Some(parent));
    assert_eq!(s.doc.layers.len(), 1);
    assert_eq!(s.selection[0].0, 0);
    let placed = s.doc.find_shape(0, s.selection[0].1).unwrap();
    assert_eq!(placed.layout.parent, Some(parent));
    assert_eq!(placed.name, "Placed SVG");
    assert_eq!(placed.world_bbox(), original_bounds);
    let fill = placed.layout.image.as_ref().unwrap();
    assert_eq!(fill.fit, crate::layout_images::ImageFit::Stretch);
    let decoded = crate::layout_images::pixmap(fill).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (2, 1));
    assert_eq!(decoded.pixels()[0].red(), 255);
    assert_eq!(decoded.pixels()[1].alpha(), 128);
    let id = placed.id;
    s.undo();
    assert_eq!(s.doc.layers.len(), 2);
    assert_eq!(s.doc.layers[0].kind.pixels().unwrap().data, original_pixels);
    assert!(s.doc.find_shape(1, id).is_none());
    s.redo();
    assert_eq!(s.doc.layers.len(), 1);
    assert_eq!(s.doc.find_shape(0, id).unwrap().layout.parent, Some(parent));
    let bytes = crate::project::encode(&s.doc).unwrap();
    assert!(
        crate::project::decode(&bytes)
            .unwrap()
            .validate_hierarchy()
            .is_ok()
    );
}
