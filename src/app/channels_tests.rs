use super::*;
use crate::deep_color::storage;
fn studio() -> Studio {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.doc = crate::channels::tests::fixture();
    s.active_layer = Some(0);
    s
}
#[test]
fn craft_channels_commands_save_rename_invert_duplicate_delete_full_undo_and_reopen() {
    let mut s = studio();
    let source = crate::project::encode(&s.doc).unwrap();
    let history = s.history.len();
    let transform = tiny_skia::Transform::from_translate(12., 8.).pre_rotate(30.);
    let plane = Plane::from_mask(&[0, 64, 128, 255], 4, 1, transform, &storage::check).unwrap();
    let target = s.capture_channels().unwrap();
    let id = s.save_channel(&target, plane, "").unwrap();
    assert_eq!(s.history.len(), history + 1);
    assert_eq!(s.doc.channels[0].name, "Alpha 1");
    let target = s.capture_channels().unwrap();
    s.rename_channel(&target, id, "Alpha 1", "Precise alpha")
        .unwrap();
    let before = crate::project::encode(&s.doc).unwrap();
    let target = s.capture_channels().unwrap();
    let copy = s.duplicate_channel(&target, id).unwrap();
    assert_ne!(copy, id);
    assert!(Arc::ptr_eq(
        &s.doc.channels[0].plane.samples,
        &s.doc.channels[1].plane.samples
    ));
    let target = s.capture_channels().unwrap();
    let mut after = s.doc.channels.clone();
    after[1].plane = after[1].plane.inverted(&storage::check).unwrap();
    s.set_channels(&target, after, "Inverted").unwrap();
    assert_eq!(s.doc.channels[0].plane.samples[0], 0.);
    assert_eq!(s.doc.channels[1].plane.samples[0], 1.);
    let target = s.capture_channels().unwrap();
    s.remove_channel(&target, copy).unwrap();
    assert_eq!(s.doc.channels.len(), 1);
    s.undo();
    s.undo();
    s.undo();
    assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    s.redo();
    s.redo();
    s.redo();
    let saved = crate::project::encode(&s.doc).unwrap();
    assert_eq!(
        crate::project::encode(&crate::project::decode(&saved).unwrap()).unwrap(),
        saved
    );
    for _ in 0..5 {
        s.undo();
    }
    assert_eq!(crate::project::encode(&s.doc).unwrap(), source);
}
#[test]
fn craft_channels_soft_combine_preserves_transformed_selection_and_owned_buffers() {
    let mut s = studio();
    let space = SelectionSpace {
        w: 4,
        h: 1,
        transform: tiny_skia::Transform::from_translate(10., 20.),
    };
    s.replace_pixel_selection(vec![80, 128, 200, 255], space);
    let original = s.pixel_sel.clone().unwrap();
    let lease = Arc::downgrade(original.lease());
    let plane =
        Plane::from_mask(&[32, 128, 240, 0], 4, 1, space.transform, &storage::check).unwrap();
    for (operation, expected) in [
        (Combine::Add, [80, 128, 240, 255]),
        (Combine::Subtract, [48, 0, 0, 255]),
        (Combine::Intersect, [32, 128, 200, 0]),
    ] {
        let (values, actual) = selection(
            &plane,
            Some((original.clone(), space)),
            operation,
            &storage::check,
        )
        .unwrap();
        assert_eq!(actual, space);
        assert_eq!(values.as_slice(), expected);
    }
    let target = s.capture_channels().unwrap();
    let (values, actual) = selection(&plane, None, Combine::Replace, &storage::check).unwrap();
    let expected_pointer = values.as_ptr();
    s.load_channel_selection(&target, values, actual).unwrap();
    assert_eq!(s.pixel_sel.as_ref().unwrap().as_ptr(), expected_pointer);
    assert_eq!(s.pixel_sel_space, Some(space));
    assert!(lease.upgrade().is_some());
    drop(original);
    assert!(lease.upgrade().is_none());
    assert!(selection(&plane, None, Combine::Subtract, &storage::check).is_err());
}
#[test]
fn craft_channels_stale_busy_live_and_invalid_changes_preserve_document_and_selection() {
    let mut s = studio();
    s.replace_pixel_selection(
        vec![0, 64, 128, 255],
        SelectionSpace {
            w: 4,
            h: 1,
            transform: tiny_skia::Transform::identity(),
        },
    );
    let plane = Plane::from_mask(
        &[0, 64, 128, 255],
        4,
        1,
        tiny_skia::Transform::identity(),
        &storage::check,
    )
    .unwrap();
    let target = s.capture_channels().unwrap();
    let before = crate::project::encode(&s.doc).unwrap();
    let mask = s.pixel_sel.clone();
    s.pixel_sel_gen += 1;
    assert!(s.save_channel(&target, plane.clone(), "Alpha").is_err());
    assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
    assert_eq!(s.pixel_sel, mask);
    let target = s.capture_channels().unwrap();
    assert!(s.save_channel(&target, plane.clone(), "Bad\nname").is_err());
    assert!(s.doc.channels.is_empty());
    s.updates.freezing = true;
    assert!(s.capture_channels().is_err());
    s.updates.freezing = false;
    let target = s.capture_channels().unwrap();
    s.doc.collaboration = Some(
        serde_json::from_value(
            serde_json::json!({"session":"channels","actor":"a","base":{},"changes":[]}),
        )
        .unwrap(),
    );
    assert!(
        s.save_channel(&target, plane, "Alpha")
            .unwrap_err()
            .contains("Live")
    );
    assert!(s.doc.channels.is_empty());
}
#[test]
fn craft_channels_selection_lease_travels_with_tabs_and_last_alias_then_releases() {
    let mut s = studio();
    let space = SelectionSpace {
        w: 4,
        h: 1,
        transform: tiny_skia::Transform::identity(),
    };
    let values = storage::adopt(vec![0, 64, 128, 255]).unwrap();
    let lease = Arc::downgrade(values.lease());
    s.replace_pixel_selection_admitted(values.clone(), space);
    s.ensure_tabs();
    s.new_tab();
    assert!(lease.upgrade().is_some());
    s.switch_tab(0);
    assert_eq!(s.pixel_sel.as_ref().unwrap().as_slice(), [0, 64, 128, 255]);
    s.set_pixel_sel(None);
    assert!(lease.upgrade().is_some());
    drop(values);
    assert!(lease.upgrade().is_none());
}
