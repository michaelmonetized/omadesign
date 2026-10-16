use super::*;
pub(crate) fn fixture() -> Studio {
    let mut studio = Studio::new();
    studio.persona = Persona::Design;
    studio.doc = Document::new("Screen collection", 80., 60., 96.);
    let mut layer = Layer::vector("Artwork");
    let mut shape = Shape::new(
        Geom::Rect {
            origin: Pt::new(12., 10.),
            size: Pt::new(16., 12.),
            radius: 0.,
        },
        Style {
            fill: Fill::Solid(Rgba::rgb(240, 30, 20)),
            stroke: None,
        },
    );
    shape.name = "Badge".into();
    studio.selection = vec![(0, shape.id)];
    studio.selected_layer = Some(layer.id);
    studio.active_layer = Some(0);
    layer.kind.shapes_mut().unwrap().push(shape);
    studio.doc.layers = vec![layer];
    studio
}
#[test]
fn craft_screen_assets_collect_live_references_undo_and_native_reopen() {
    let mut studio = fixture();
    let before = crate::project::encode(&studio.doc).unwrap();
    let target = studio.capture_export_assets().unwrap();
    let ids = studio.collect_export_assets(&target, true).unwrap();
    let saved = crate::project::encode(&studio.doc).unwrap();
    let reopened = crate::project::decode(&saved).unwrap();
    assert_eq!(reopened.screen_export, studio.doc.screen_export);
    assert_eq!(ids.len(), 1);
    assert_eq!(studio.history.len(), 1);
    let id = studio.selection[0].1;
    studio.doc.find_shape_mut(0, id).unwrap().style.fill = Fill::Solid(Rgba::rgb(20, 160, 40));
    let asset = &studio.doc.screen_export.assets[0];
    let output = asset.document(&studio.doc).unwrap();
    let pm = crate::compositor::render_export(&output, 1).unwrap();
    assert_eq!(pm.pixel(5, 5).unwrap().green(), 160);
    studio.doc.find_shape_mut(0, id).unwrap().style.fill = Fill::Solid(Rgba::rgb(240, 30, 20));
    studio.undo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    studio.redo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), saved);
}
#[test]
fn craft_screen_assets_refuse_stale_locked_live_and_conflicting_metadata_without_mutation() {
    let mut studio = fixture();
    let target = studio.capture_export_assets().unwrap();
    studio.canvas_gen += 1;
    assert!(
        studio
            .collect_export_assets(&target, false)
            .unwrap_err()
            .contains("changed")
    );
    let id = studio.selection[0].1;
    studio.doc.find_shape_mut(0, id).unwrap().locked = true;
    assert!(studio.capture_export_assets().is_err());
    studio.doc.find_shape_mut(0, id).unwrap().locked = false;
    let target = studio.capture_export_assets().unwrap();
    studio.collect_export_assets(&target, false).unwrap();
    let before = studio.doc.screen_export.clone();
    let mut invalid = before.clone();
    invalid.assets.push(invalid.assets[0].clone());
    let count = studio.history.len();
    assert!(studio.set_screen_export(&before, invalid).is_err());
    assert_eq!(studio.history.len(), count);
    let mut changed = before.clone();
    changed.prefix = "out-".into();
    studio.set_screen_export(&before, changed.clone()).unwrap();
    assert!(studio.set_screen_export(&before, before.clone()).is_err());
    studio.doc.collaboration = Some(crate::collaboration::Replica::new(
        "fixture".into(),
        "actor".into(),
        Default::default(),
    ));
    assert!(studio.capture_export_assets().unwrap_err().contains("Live"));
    assert!(
        studio
            .set_screen_export(&changed, before)
            .unwrap_err()
            .contains("Live")
    );
    assert_eq!(studio.doc.screen_export, changed);
}
#[test]
fn craft_screen_assets_collect_multiple_unique_names_and_missing_references_stay_explicit() {
    let mut studio = fixture();
    let mut duplicate = studio.doc.layers[0].kind.shapes().unwrap()[0].clone();
    duplicate.id = crate::document::next_id();
    studio.selection.push((0, duplicate.id));
    studio.doc.layers[0]
        .kind
        .shapes_mut()
        .unwrap()
        .push(duplicate);
    let target = studio.capture_export_assets().unwrap();
    studio.collect_export_assets(&target, false).unwrap();
    assert_eq!(
        studio
            .doc
            .screen_export
            .assets
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>(),
        ["Badge", "Badge 2"]
    );
    let asset = studio.doc.screen_export.assets[1].clone();
    studio.doc.layers[0].kind.shapes_mut().unwrap().pop();
    assert!(
        asset
            .document(&studio.doc)
            .unwrap_err()
            .contains("removed object")
    );
    let reopened = crate::project::decode(&crate::project::encode(&studio.doc).unwrap()).unwrap();
    assert_eq!(reopened.screen_export, studio.doc.screen_export);
}

#[test]
fn craft_screen_assets_whole_layer_uses_stable_identity_and_refuses_changed_owner() {
    let mut studio = fixture();
    studio.selection.clear();
    let layer = studio.doc.layers[0].id;
    let mut unrelated = Layer::vector("Other layer");
    unrelated.kind.shapes_mut().unwrap().push(Shape::new(
        Geom::Rect {
            origin: Pt::ZERO,
            size: Pt::splat(5.),
            radius: 0.,
        },
        Style::default(),
    ));
    studio.doc.layers.insert(0, unrelated);
    studio.active_layer = Some(1);
    let target = studio.capture_export_assets().unwrap();
    studio.collect_export_assets(&target, true).unwrap();
    assert!(
        studio.doc.screen_export.assets[0]
            .objects
            .iter()
            .all(|o| o.layer == layer)
    );
    let target = studio.capture_export_assets().unwrap();
    studio.active_layer = Some(0);
    assert!(
        studio
            .collect_export_assets(&target, true)
            .unwrap_err()
            .contains("selection changed")
    );
    studio.selected_layer = Some(u64::MAX - 1);
    assert!(
        studio
            .capture_export_assets()
            .unwrap_err()
            .contains("no longer exists")
    );
}
