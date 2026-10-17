use super::*;
use crate::{deep_color::storage, document::Document};
fn mask() -> QuickMask {
    QuickMask {
        channel: Channel {
            id: crate::document::next_id(),
            name: "Quick Mask".into(),
            plane: Plane {
                width: 5,
                height: 1,
                transform: [1., 0., 0., 1., 0., 0.],
                samples: storage::collect(
                    [-0., f32::from_bits(0x3e800001), 0.5, 0.75, 1.].into_iter(),
                )
                .unwrap(),
            },
        },
        unselected: false,
        options: Options::default(),
    }
}
#[test]
fn craft_quick_mask_native33_preserves_scalar_bits_aliases_and_refuses_old_readers() {
    let mut doc = crate::channels::tests::fixture();
    let legacy = crate::project::encode(&doc).unwrap();
    assert!(!legacy.contains("quick_mask"));
    doc.quick_mask = Some(mask());
    let encoded = crate::project::encode(&doc).unwrap();
    let mut envelope: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(envelope["version"], 33);
    let back = crate::project::decode(&encoded).unwrap();
    let actual = &back.quick_mask.as_ref().unwrap().channel.plane;
    for (v, bits) in actual.samples.iter().zip([
        (-0.0f32).to_bits(),
        0x3e800001,
        0.5f32.to_bits(),
        0.75f32.to_bits(),
        1.0f32.to_bits(),
    ]) {
        assert_eq!(v.to_bits(), bits);
    }
    assert_eq!(crate::project::encode(&back).unwrap(), encoded);
    envelope["version"] = 32.into();
    envelope["doc"]["quick_mask"]["channel"]["plane"]["samples"] =
        "damaged bytes that must not be decoded first".into();
    assert!(
        crate::project::decode(&envelope.to_string())
            .unwrap_err()
            .contains("requires version 33")
    );
    let mut future: serde_json::Value = serde_json::from_str(&legacy).unwrap();
    future["version"] = 34.into();
    assert!(
        crate::project::decode(&future.to_string())
            .unwrap_err()
            .contains("Unsupported")
    );
    assert!(
        crate::collaboration::snapshot(&doc)
            .unwrap_err()
            .contains("QuickMaskUnsupported")
    );
    assert!(
        crate::formats::psd::encode_admitted(&doc, false)
            .err()
            .unwrap()
            .contains("QuickMaskUnsupported")
    );
    let mut without = doc.clone();
    without.quick_mask = None;
    assert_eq!(
        crate::compositor::export_png(&doc, 1).unwrap(),
        crate::compositor::export_png(&without, 1).unwrap()
    );
    if let Some(root) = std::env::var_os("OMADESIGN_QUICK_MASK_QA") {
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("scalar-mask.oma"), encoded).unwrap();
    }
}
#[test]
fn craft_quick_mask_embedded_retained_withdrawn_and_deleted_state_requires33() {
    let mut child = crate::channels::tests::fixture();
    child.quick_mask = Some(mask());
    let mut parent = Document::new("Embedded Quick Mask", 32., 16., 96.);
    parent
        .motion
        .compositions
        .push(crate::motion::compositions::Composition::new(child));
    let mut value: serde_json::Value =
        serde_json::from_str(&crate::project::encode(&parent).unwrap()).unwrap();
    assert_eq!(value["version"], 33);
    value["version"] = 32.into();
    assert!(
        crate::project::decode(&value.to_string())
            .unwrap_err()
            .contains("requires version 33")
    );
    assert!(
        crate::collaboration::snapshot(&parent)
            .unwrap_err()
            .contains("QuickMaskUnsupported")
    );
    for path in [
        "/quick_mask",
        "/@compositions/42/document/quick_mask/channel/plane/samples",
    ] {
        for value in [
            serde_json::Value::Null,
            serde_json::json!({"Value":null}),
            serde_json::json!({"Value":{}}),
        ] {
            let mut doc = Document::new("Retained Quick Mask", 1., 1., 96.);
            doc.collaboration=Some(serde_json::from_value(serde_json::json!({"session":"quick","actor":"a","base":{},"changes":[{"id":{"actor":"a","seq":1},"clock":1,"seen":{},"edit":{"type":"write","writes":[{"path":path,"value":value}]}},{"id":{"actor":"a","seq":2},"clock":2,"seen":{},"edit":{"type":"toggle","target":{"actor":"a","seq":1},"enabled":false}}]})).unwrap());
            let mut v: serde_json::Value =
                serde_json::from_str(&crate::project::encode(&doc).unwrap()).unwrap();
            assert_eq!(v["version"], 33);
            v["version"] = 32.into();
            assert!(crate::project::decode(&v.to_string()).is_err());
        }
    }
}
#[test]
fn craft_quick_mask_options_empty_state_and_validation_do_not_change_selection_meaning() {
    let mut q = mask();
    assert_eq!(q.options.coverage(0.), 0.5);
    assert_eq!(q.options.coverage(1.), 0.);
    q.options.indicates = Indicates::Selected;
    assert_eq!(q.options.coverage(0.), 0.);
    assert_eq!(q.options.coverage(1.), 0.5);
    for value in [f32::NAN, f32::INFINITY, -0.01, 1.01] {
        let mut options = Options::default();
        options.opacity = value;
        assert!(options.validate().is_err());
    }
    let mut doc = crate::channels::tests::fixture();
    q.options = Options::default();
    doc.channels.push(q.channel.clone());
    assert!(q.validate(&doc.channels).unwrap_err().contains("distinct"));
    doc.channels.clear();
    q.channel.plane.samples = storage::collect([0.; 5].into_iter()).unwrap();
    assert!(q.leaves_unselected());
    q.channel.plane.samples = storage::collect([1.; 5].into_iter()).unwrap();
    assert!(!q.leaves_unselected());
    q.unselected = true;
    assert!(q.leaves_unselected());
    q.options.indicates = Indicates::Selected;
    assert!(!q.leaves_unselected());
}
