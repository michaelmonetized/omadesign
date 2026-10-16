use super::*;
use crate::motion::compositions::Composition;

fn plain() -> Document {
    let mut doc = Document::new("Live Motion", 32., 32., 72.);
    doc.layers.clear();
    doc.artboards.clear();
    doc.artboardless = true;
    doc.transparent = true;
    doc
}

fn nested() -> Document {
    let mut doc = plain();
    for _ in 0..2 {
        let mut child = native_tests::fixture();
        child.motion.scene3d = Some(crate::scene3d::Scene::default());
        doc.motion.compositions.push(Composition::new(child));
    }
    doc
}

#[test]
fn motion_live_child_atomic_pixels_merge_reorder_and_author_undo() {
    let doc = nested();
    let seed = snapshot(&doc).unwrap();
    assert!(seed.keys().any(|path| path.starts_with("/@compositions/")));
    let Cell::Value(comps) = &seed["/motion/compositions"] else {
        panic!("atomic composition list")
    };
    let child = &comps[0]["document"];
    assert!(
        child["layers"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|layer| layer.pointer("/kind/Raster/pixels"))
            .all(|pixels| pixels["data"].as_array().unwrap().is_empty()
                && pixels.get("native").is_none_or(Value::is_null))
    );
    let mut alice = Replica::new("motion".into(), "a".repeat(64), seed.clone());
    let mut bob = Replica::new("motion".into(), "b".repeat(64), seed);
    let mut left = doc.clone();
    native_tests::edit(&mut left.motion.compositions[0].document, "Alice");
    alice.record(&left).unwrap();
    let mut right = doc.clone();
    native_tests::edit(&mut right.motion.compositions[1].document, "Bob");
    bob.record(&right).unwrap();
    alice.receive(bob.changes.clone()).unwrap();
    bob.receive(alice.changes.clone()).unwrap();
    let merged = alice.document(&doc).unwrap();
    assert_eq!(
        snapshot(&merged).unwrap(),
        snapshot(&bob.document(&doc).unwrap()).unwrap()
    );
    native_tests::assert_native_equal(
        &merged.motion.compositions[0].document,
        &left.motion.compositions[0].document,
    );
    native_tests::assert_native_equal(
        &merged.motion.compositions[1].document,
        &right.motion.compositions[1].document,
    );
    assert!(alice.undo());
    let undone = alice.document(&doc).unwrap();
    native_tests::assert_native_equal(
        &undone.motion.compositions[0].document,
        &doc.motion.compositions[0].document,
    );
    native_tests::assert_native_equal(
        &undone.motion.compositions[1].document,
        &right.motion.compositions[1].document,
    );
    let mut reordered = merged;
    reordered.motion.compositions.reverse();
    bob.record(&reordered).unwrap();
    let restored = bob.document(&doc).unwrap();
    assert_eq!(snapshot(&restored).unwrap(), snapshot(&reordered).unwrap());
}

#[test]
fn motion_live_withdrawn_child_pixels_remain_encoded_until_winning() {
    let empty = plain();
    let mut replica = Replica::new(
        "withdrawn-motion".into(),
        "a".repeat(64),
        snapshot(&empty).unwrap(),
    );
    replica.record(&nested()).unwrap();
    assert!(replica.undo());
    let encoded = replica
        .retained_cells_mut()
        .find_map(|(path, cell)| {
            if path.starts_with("/@compositions/")
                && let Cell::Pixels { data, .. } = cell
            {
                Some(data)
            } else {
                None
            }
        })
        .unwrap();
    *encoded = "retained child pixel stream deliberately undecodable".into();
    let mut doc = empty.clone();
    doc.collaboration = Some(replica);
    let text = crate::project::encode(&doc).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["version"], 27);
    let reopened = crate::project::decode(&text).unwrap();
    let mut replica = reopened.collaboration.clone().unwrap();
    assert!(
        replica
            .document(&empty)
            .unwrap()
            .motion
            .compositions
            .is_empty()
    );
    assert!(replica.redo());
    assert!(replica.document(&empty).is_err());
    assert!(reopened.motion.compositions.is_empty());
}

#[test]
fn motion_live_child_pixel_preflight_is_bounded_before_typed_materialization() {
    let doc = nested();
    let mut cells = snapshot(&doc).unwrap();
    let path = cells
        .keys()
        .find(|path| {
            path.starts_with("/@compositions/") && matches!(cells[*path], Cell::Pixels { .. })
        })
        .unwrap()
        .clone();
    if let Cell::Pixels { w, h, .. } = cells.get_mut(&path).unwrap() {
        *w = u32::MAX;
        *h = u32::MAX;
    }
    assert!(preflight_pixels(&inflate("", &cells).unwrap(), &cells, PIXEL_BUDGET).is_err());
    let mut inline = inflate("", &snapshot(&doc).unwrap()).unwrap();
    inline["motion"]["compositions"][0]["document"]["layers"][0]["kind"]["Raster"]["pixels"]["native"] =
        json!({"unadmitted":true});
    assert!(preflight_pixels(&inline, &Snapshot::new(), PIXEL_BUDGET).is_err());
    let mut deep = plain();
    for _ in 0..9 {
        let mut parent = plain();
        parent.motion.compositions.push(Composition::new(deep));
        deep = parent;
    }
    let serialized = serde_json::to_value(&deep).unwrap();
    assert!(
        preflight_pixels(&serialized, &Snapshot::new(), PIXEL_BUDGET)
            .unwrap_err()
            .contains("CompositionDepth")
    );
}

#[test]
fn motion_retained_cells_reject_composition_depth_and_count_without_replay() {
    let mut replica = Replica::new("bounds".into(), "a".repeat(64), snapshot(&plain()).unwrap());
    let mut doc = plain();
    for _ in 0..9 {
        let mut parent = plain();
        parent.motion.compositions.push(Composition::new(doc));
        doc = parent;
    }
    let mut value = serde_json::to_value(&doc).unwrap();
    replica.base.insert(
        "/motion/compositions".into(),
        Cell::Value(value["motion"]["compositions"].take()),
    );
    assert!(
        resources::validate(&replica)
            .unwrap_err()
            .contains("CompositionDepth")
    );
    replica.base.insert(
        "/motion/compositions".into(),
        Cell::Value(Value::Array(vec![json!({"document":{}}); 129])),
    );
    assert!(
        resources::validate(&replica)
            .unwrap_err()
            .contains("CompositionLimit")
    );
}
