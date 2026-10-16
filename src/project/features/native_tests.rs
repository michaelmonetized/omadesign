use super::*;
use crate::deep_color::{Depth, cms};
use crate::document::{Layer, Pixels};
use crate::geom::Pt;
use crate::source_attachment::SourceAttachment;
use base64::Engine as _;
use serde_json::{Value, json};

fn native() -> Document {
    let mut doc = Document::new("Native gate", 2., 1., 72.);
    doc.transparent = true;
    doc.layers = vec![Layer::placed_raster(
        "Samples",
        Pixels::from_samples(
            2,
            1,
            Depth::U16,
            vec![0.12345, 0.2, 0.3, 1., 0.12346, 0.2, 0.3, 0.5],
        )
        .unwrap(),
        Pt::ZERO,
        Pt::new(2., 1.),
    )];
    doc
}

fn roundtrip_gate(doc: &Document, expected: u32) {
    let encoded = crate::project::encode(doc).unwrap();
    let mut envelope: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(envelope["version"], expected);
    let restored = crate::project::decode(&encoded).unwrap();
    assert_eq!(required_version(&restored), expected);
    envelope["version"] = (expected - 1).into();
    assert!(
        crate::project::decode(&envelope.to_string())
            .unwrap_err()
            .contains(&format!("requires version {expected}"))
    );
}

#[test]
fn integrated_native_source_color_type_smart_feature_gates_preserve_originals() {
    let mut doc = native();
    roundtrip_gate(&doc, NATIVE);
    doc.layers[0].visible = false;
    roundtrip_gate(&doc, NATIVE);
    doc.color.icc_profile = include_bytes!("../../formats/psd/fixtures/srgb-cc0.icc").to_vec();
    roundtrip_gate(&doc, COLOR);
    doc.color.icc_profile.clear();
    doc.color.intent = 1;
    roundtrip_gate(&doc, COLOR);
    doc.color = Default::default();
    let embedded = crate::project::encode(&doc).unwrap().into_bytes();
    let original = SourceAttachment::new("embedded.oma", "oma", embedded.clone()).unwrap();
    let mut retained = Document::new("Only original remains", 1., 1., 72.);
    retained.layers.clear();
    retained.retain_source(original);
    roundtrip_gate(&retained, NATIVE);
    let restored = crate::project::decode(&crate::project::encode(&retained).unwrap()).unwrap();
    assert_eq!(restored.sources[0].bytes(), embedded);
    retained.sources.clear();
    retained.retain_source(SourceAttachment::new("source.bin", "binary", vec![1, 2, 3]).unwrap());
    roundtrip_gate(&retained, SOURCE);
    for (bytes, gate) in [
        (
            include_bytes!("../../formats/psd/fixtures/independent-type.psd").as_slice(),
            PSD_TYPE,
        ),
        (
            include_bytes!("../../formats/psd/fixtures/independent-smart.psd").as_slice(),
            SMART,
        ),
    ] {
        let imported = crate::formats::psd::decode(bytes, "Independent gate").unwrap();
        roundtrip_gate(&imported.document, gate);
    }
    let mut inks = native();
    inks.color.model = cms::Model::Cmyk;
    inks.color.icc_profile = include_bytes!("../../formats/psd/fixtures/fogra39l-cc0.icc").to_vec();
    roundtrip_gate(&inks, COLOR);
}

#[test]
fn integrated_retained_cells_scan_base_changes_atomic_color_and_disabled_history() {
    let samples = serde_json::to_value(
        native().layers[0]
            .kind
            .pixels()
            .unwrap()
            .native
            .as_ref()
            .unwrap(),
    )
    .unwrap();
    let mut wire = samples.clone();
    let pack = |bytes: &[u8]| {
        base64::engine::general_purpose::STANDARD
            .encode(zstd::stream::encode_all(bytes, 3).unwrap())
    };
    wire["icc_profile"] = json!(pack(&[]));
    wire["icc_bytes"] = json!(0);
    let cell = json!({"Pixels":{"w":2,"h":1,"data":pack(native().layers[0].kind.pixels().unwrap().data.as_slice()),"native":wire}});
    let path = "/@entities/42/kind/Raster/pixels";
    let mut replica = json!({"session":"retained","actor":"a","base":{(path):cell.clone()},"changes":[{"id":{"actor":"a","seq":1},"clock":1,"seen":{},"edit":{"type":"write","writes":[{"path":path,"value":null}]}},{"id":{"actor":"a","seq":2},"clock":2,"seen":{},"edit":{"type":"toggle","target":{"actor":"a","seq":1},"enabled":false}}]});
    assert_eq!(
        crate::project::retained_feature_version(&replica).unwrap(),
        NATIVE
    );
    replica["base"] = json!({});
    replica["changes"][0]["edit"]["writes"][0]["value"] = cell;
    assert_eq!(
        crate::project::retained_feature_version(&replica).unwrap(),
        NATIVE
    );
    replica["changes"][0]["edit"]["writes"][0]["value"]["Pixels"]["native"]["intent"] = json!(0);
    assert_eq!(
        crate::project::retained_feature_version(&replica).unwrap(),
        COLOR
    );
    replica["changes"][0]["edit"]["writes"][0]["value"]["Pixels"]["native"]["intent"] = json!(1);
    let profile = include_bytes!("../../formats/psd/fixtures/srgb-cc0.icc");
    replica["changes"][0]["edit"]["writes"][0]["value"]["Pixels"]["native"]["icc_profile"] =
        json!(pack(profile));
    replica["changes"][0]["edit"]["writes"][0]["value"]["Pixels"]["native"]["icc_bytes"] =
        json!(profile.len());
    assert_eq!(
        crate::project::retained_feature_version(&replica).unwrap(),
        COLOR
    );
    replica["changes"][0]["edit"]["writes"][0] = json!({"path":"/color","value":{"Value":{"model":"Rgb","depth":"U8","icc_profile":[],"intent":2}}});
    assert_eq!(
        crate::project::retained_feature_version(&replica).unwrap(),
        COLOR
    );
    replica["changes"][0]["edit"]["writes"][0] =
        json!({"path":"/@entities/42/source_attachment","value":{"Value":"original-uuid"}});
    assert_eq!(
        crate::project::retained_feature_version(&replica).unwrap(),
        SOURCE
    );
    let plain = Document::new("Old reader", 1., 1., 72.);
    let mut envelope: Value =
        serde_json::from_str(&crate::project::encode(&plain).unwrap()).unwrap();
    envelope["doc"]["collaboration"] = replica;
    for declared in [5, 18, 19, 20, 23] {
        envelope["version"] = declared.into();
        assert!(
            crate::project::decode(&envelope.to_string())
                .unwrap_err()
                .contains("requires version 24")
        );
    }
    envelope["version"] = 23.into();
    let directory = std::env::current_dir()
        .unwrap()
        .join(".artifacts")
        .join(format!(
            "native-preview-gate-{}",
            crate::document::next_id()
        ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("retained.oma");
    std::fs::write(&path, envelope.to_string()).unwrap();
    assert!(
        crate::brand::load_preview_document(&path, false)
            .unwrap_err()
            .contains("requires version 24")
    );
    std::fs::remove_dir_all(directory).unwrap();
}
