//! Placement assertions use real imported artwork and the editor's undo path.
use super::*;

fn fixture(origin: Pt) -> (Studio, u64, u64) {
    let mut studio = Studio::new();
    studio.doc = Document::new("Asset placement", 1.0, 1.0, 96.0);
    studio.doc.width = 640.0;
    studio.doc.height = 480.0;
    studio.doc.layers = vec![
        Layer::vector("Frame artwork"),
        Layer::vector("Other artwork"),
    ];
    studio.persona = Persona::Layout;
    studio.show_welcome = false;
    let frame = crate::layout::make_frame(origin, Pt::new(240.0, 180.0));
    let frame_id = frame.id;
    let mut child = Shape::new(
        Geom::Rect {
            origin: origin + Pt::new(20.0, 20.0),
            size: Pt::new(24.0, 24.0),
            radius: 0.0,
        },
        Style::default(),
    );
    child.layout.parent = Some(frame_id);
    let child_id = child.id;
    studio.doc.layers[0]
        .kind
        .shapes_mut()
        .unwrap()
        .extend([frame, child]);
    studio.selection = vec![(0, child_id)];
    // Layer selection can lag object selection; the selected frame owns placement.
    studio.active_layer = Some(1);
    (studio, frame_id, child_id)
}

fn icon() -> Geom {
    Geom::Rect {
        origin: Pt::ZERO,
        size: Pt::new(256.0, 256.0),
        radius: 0.0,
    }
}

fn svg() -> Imported {
    Imported::Svg {
        name: "Two-part mark".into(),
        svg: r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20"><path fill="#ff0000" d="M0 0H10V20H0Z"/><path fill="#0000ff" d="M30 0H40V20H30Z"/></svg>"##.into(),
    }
}

#[test]
fn selected_child_places_palette_shape_in_its_nearest_frame_and_owning_layer() {
    let (mut studio, outer, _) = fixture(Pt::new(20.0, 20.0));
    let mut inner = crate::layout::make_frame(Pt::new(60.0, 60.0), Pt::new(100.0, 100.0));
    inner.layout.parent = Some(outer);
    let inner_id = inner.id;
    let mut selected = Shape::new(icon(), Style::default());
    selected.layout.parent = Some(inner_id);
    let selected_id = selected.id;
    studio.doc.layers[0]
        .kind
        .shapes_mut()
        .unwrap()
        .extend([inner, selected]);
    studio.selection = vec![(0, selected_id)];
    let target = studio.asset_frame_target();
    assert_eq!(target, Some((0, inner_id)));
    let before = serde_json::to_value(&studio.doc).unwrap();
    studio.place_palette_shape(icon(), "Phosphor icon".into(), target);
    let (layer, placed_id) = studio.primary().unwrap();
    assert_eq!(layer, 0);
    let placed = studio.doc.find_shape(layer, placed_id).unwrap();
    assert_eq!(placed.layout.parent, Some(inner_id));
    assert_eq!(placed.name, "Phosphor icon");
    assert!((placed.geom.bbox().center() - Pt::new(110.0, 110.0)).length() < 0.01);
    assert_eq!(studio.doc.layers[1].kind.shapes().unwrap().len(), 0);
    studio.doc.validate_hierarchy().unwrap();
    assert_eq!(studio.history.len(), 1);
    studio.undo();
    assert_eq!(serde_json::to_value(&studio.doc).unwrap(), before);
}

#[test]
fn multi_path_svg_is_one_auto_layout_item_with_intact_internal_geometry_and_one_undo() {
    let (mut studio, frame, child) = fixture(Pt::new(20.0, 20.0));
    studio.doc.find_shape_mut(0, frame).unwrap().layout.stack =
        Some(crate::layout::AutoStack::default());
    crate::layout::reflow_roots(&mut studio.doc, 0);
    let before = serde_json::to_value(&studio.doc).unwrap();
    studio
        .place_brand_imported_in_frame(svg(), Pt::new(140.0, 110.0), Some((0, frame)))
        .unwrap();
    let (layer, root) = studio.primary().unwrap();
    assert_eq!(layer, 0);
    assert_eq!(
        crate::layout::children(&studio.doc, 0, frame),
        vec![child, root]
    );
    let shapes = studio.doc.layers[0].kind.shapes().unwrap();
    let red = shapes
        .iter()
        .find(|shape| shape.style.fill == Fill::Solid(Rgba::from_hex(0xff0000)))
        .unwrap()
        .geom
        .bbox();
    let blue = shapes
        .iter()
        .find(|shape| shape.style.fill == Fill::Solid(Rgba::from_hex(0x0000ff)))
        .unwrap()
        .geom
        .bbox();
    assert!((red.min.y - blue.min.y).abs() < 0.01);
    assert!((blue.min.x - red.min.x - 30.0).abs() < 0.01);
    assert!((red.width() - 10.0).abs() < 0.01);
    assert_eq!(studio.history.len(), 1);
    studio.doc.validate_hierarchy().unwrap();
    let placed = serde_json::to_value(&studio.doc).unwrap();
    studio.undo();
    assert_eq!(serde_json::to_value(&studio.doc).unwrap(), before);
    studio.redo();
    assert_eq!(serde_json::to_value(&studio.doc).unwrap(), placed);
}

#[test]
fn assets_can_be_placed_in_frames_outside_the_original_paper() {
    let (mut studio, frame, _) = fixture(Pt::new(1800.0, -700.0));
    let target = studio.asset_frame_target();
    let bounds = studio.doc.find_shape(0, frame).unwrap().geom.bbox();
    studio.place_palette_shape(icon(), "Outside icon".into(), target);
    let (_, icon) = studio.primary().unwrap();
    assert!(
        (studio.doc.find_shape(0, icon).unwrap().geom.bbox().center() - bounds.center()).length()
            < 0.01
    );
    studio
        .place_brand_imported_in_frame(svg(), bounds.center(), target)
        .unwrap();
    let (_, root) = studio.primary().unwrap();
    let placed = studio.doc.find_shape(0, root).unwrap();
    assert_eq!(placed.layout.parent, Some(frame));
    assert!((placed.geom.bbox().center() - bounds.center()).length() < 0.01);
    assert!(placed.geom.bbox().min.x > studio.doc.width);
    assert!(placed.geom.bbox().max.y < 0.0);
    studio.doc.validate_hierarchy().unwrap();
}

#[test]
fn raster_asset_is_embedded_in_the_frame_and_survives_document_serialization() {
    let (mut studio, frame, _) = fixture(Pt::new(20.0, 20.0));
    let pixels = vec![255, 0, 0, 255, 0, 80, 255, 128];
    studio
        .place_brand_imported_in_frame(
            Imported::Raster {
                name: "Brand photo".into(),
                image: RgbaImage::new(2, 1, pixels).unwrap(),
            },
            Pt::new(140.0, 110.0),
            Some((0, frame)),
        )
        .unwrap();
    assert_eq!(studio.doc.layers.len(), 2);
    assert!(
        studio
            .doc
            .layers
            .iter()
            .all(|layer| layer.kind.shapes().is_some())
    );
    let root = studio.primary().unwrap().1;
    let descendants = crate::layout::descendants(&studio.doc, 0, root);
    let embedded = descendants
        .iter()
        .find_map(|&id| studio.doc.find_shape(0, id).unwrap().layout.image.as_ref())
        .unwrap();
    assert_eq!(crate::layout_images::pixmap(embedded).unwrap().width(), 2);
    let json = serde_json::to_vec(&studio.doc).unwrap();
    let reopened: Document = serde_json::from_slice(&json).unwrap();
    reopened.validate_hierarchy().unwrap();
    let reopened_image = reopened.layers[0]
        .kind
        .shapes()
        .unwrap()
        .iter()
        .find_map(|shape| shape.layout.image.as_ref())
        .unwrap();
    assert_eq!(
        crate::layout_images::pixmap(reopened_image).unwrap().data(),
        crate::layout_images::pixmap(embedded).unwrap().data()
    );
    assert_eq!(studio.history.len(), 1);
}

#[test]
fn removed_or_locked_captured_frame_rejects_late_assets_without_changing_artwork() {
    for condition in ["removed", "locked", "hidden", "layer locked"] {
        let (mut studio, frame, _) = fixture(Pt::new(20.0, 20.0));
        let target = studio.asset_frame_target();
        match condition {
            "removed" => studio.doc.layers[0].kind.shapes_mut().unwrap().clear(),
            "locked" => studio.doc.find_shape_mut(0, frame).unwrap().locked = true,
            "hidden" => studio.doc.find_shape_mut(0, frame).unwrap().visible = false,
            "layer locked" => studio.doc.layers[0].locked = true,
            _ => unreachable!(),
        }
        let before = serde_json::to_value(&studio.doc).unwrap();
        studio.place_palette_shape(icon(), "Late icon".into(), target);
        assert_eq!(
            serde_json::to_value(&studio.doc).unwrap(),
            before,
            "{condition}"
        );
        assert!(
            studio
                .place_brand_imported_in_frame(svg(), Pt::new(140.0, 110.0), target)
                .is_err(),
            "{condition}"
        );
        assert_eq!(
            serde_json::to_value(&studio.doc).unwrap(),
            before,
            "{condition}"
        );
        assert_eq!(studio.history.len(), 0, "{condition}");
    }
}

#[test]
fn click_placement_retains_original_target_when_preview_is_clamped_over_another_frame() {
    for click_inside_frame in [true, false] {
        let (mut studio, outer, _) = fixture(Pt::new(20.0, 20.0));
        let mut inner = crate::layout::make_frame(Pt::new(80.0, 60.0), Pt::new(100.0, 100.0));
        inner.layout.parent = Some(outer);
        let inner_id = inner.id;
        studio.doc.layers[0].kind.shapes_mut().unwrap().push(inner);
        studio.selection.clear();
        studio.pending_place = Some(PendingPlace::Svg {
            name: "Wide mark".into(),
            svg: r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 120"><path fill="#dd513b" d="M0 0H200V120H0Z"/></svg>"##.into(),
        });
        studio.pending_place_frame = None;
        let click = if click_inside_frame {
            Pt::new(21.0, 21.0)
        } else {
            Pt::ZERO
        };
        let target = click_inside_frame.then_some((0, outer));
        assert_eq!(studio.asset_frame_at(click), target);
        let preview = studio.pending_preview_rect(click).unwrap();
        assert_eq!(studio.asset_frame_at(preview.center()), Some((0, inner_id)));
        studio.commit_place_at(click);
        let (layer, id) = studio.primary().unwrap();
        assert_eq!(
            studio.doc.find_shape(layer, id).unwrap().layout.parent,
            target.map(|(_, id)| id)
        );
        studio.doc.validate_hierarchy().unwrap();
    }
}

#[test]
fn click_and_drag_ghost_corners_match_placement_inside_nested_rotated_frames() {
    for drag in [false, true] {
        let (mut studio, outer, _) = fixture(Pt::new(20.0, 20.0));
        let outer_center = studio
            .doc
            .find_shape(0, outer)
            .unwrap()
            .geom
            .bbox()
            .center();
        let outer_rotation = 0.37;
        studio.doc.find_shape_mut(0, outer).unwrap().rotation = outer_rotation;
        let mut inner = crate::layout::make_frame(Pt::new(80.0, 60.0), Pt::new(120.0, 100.0));
        inner.layout.parent = Some(outer);
        inner.rotation = -0.81;
        let inner_id = inner.id;
        let inner_center = inner.geom.bbox().center();
        let inner_rotation = inner.rotation;
        studio.doc.layers[0].kind.shapes_mut().unwrap().push(inner);
        studio.pending_place = Some(PendingPlace::Svg {
            name: "Rotated mark".into(),
            svg: r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20"><path fill="#dd513b" d="M0 0H40V20H0Z"/></svg>"##.into(),
        });
        studio.pending_place_frame = Some((0, inner_id));
        let world = |point: Pt| {
            point
                .rotate_about(inner_center, inner_rotation)
                .rotate_about(outer_center, outer_rotation)
        };
        let preview = if drag {
            let center = world(inner_center);
            let start = center - Pt::new(30.0, 20.0);
            let end = center + Pt::new(30.0, 20.0);
            let preview = studio.pending_drag_preview_corners(start, end).unwrap();
            studio.commit_place_rect(start, end);
            preview
        } else {
            // Near the local corner, the preview center must clamp before rotation.
            let at = world(Pt::new(81.0, 61.0));
            let preview = studio.pending_preview_corners(at).unwrap();
            studio.commit_place_at(at);
            preview
        };
        let (layer, id) = studio.primary().unwrap();
        let placed = studio.doc.find_shape(layer, id).unwrap();
        assert_eq!(placed.layout.parent, Some(inner_id));
        let bounds = placed.geom.bbox();
        let actual = [
            bounds.min,
            Pt::new(bounds.max.x, bounds.min.y),
            bounds.max,
            Pt::new(bounds.min.x, bounds.max.y),
        ]
        .map(world);
        for (preview, actual) in preview.into_iter().zip(actual) {
            assert!(
                (preview - actual).length() < 0.01,
                "drag={drag}: {preview:?} != {actual:?}"
            );
        }
        assert!((preview[1].y - preview[0].y).abs() > 1.0);
        studio.doc.validate_hierarchy().unwrap();
    }
}
