use super::*;
use crate::{
    deep_color::Depth,
    document::{Layer, Pixels},
    geom::Pt,
};
fn plane(values: &[f32]) -> Plane {
    Plane {
        width: values.len() as u32,
        height: 1,
        transform: [1., 0., 0., 1., 0., 0.],
        samples: storage::collect(values.iter().copied()).unwrap(),
    }
}
pub(crate) fn fixture() -> Document {
    let mut doc = Document::new("Channels", 8., 4., 96.);
    doc.transparent = true;
    doc.layers.clear();
    let pixels = Pixels::from_rgba(8, 4, [64, 128, 191, 128].repeat(32)).unwrap();
    doc.layers
        .push(Layer::placed_raster("Pixels", pixels, Pt::ZERO, Pt::ZERO));
    doc
}
#[test]
fn craft_channels_scalar_native32_preserves_exact_samples_aliases_and_legacy_default() {
    let mut doc = fixture();
    let old = crate::project::encode(&doc).unwrap();
    assert!(!old.contains("saved_channels"));
    let samples = [0., -0., f32::from_bits(0x3e800001), 0.5, 1.];
    doc.channels.push(Channel {
        id: crate::document::next_id(),
        name: "Alpha Δ".into(),
        plane: plane(&samples),
    });
    let lease = Arc::downgrade(doc.channels[0].plane.samples.lease());
    let clone = doc.channels[0].clone();
    assert!(Arc::ptr_eq(
        &clone.plane.samples,
        &doc.channels[0].plane.samples
    ));
    assert!(
        crate::collaboration::snapshot(&doc)
            .unwrap_err()
            .contains("SavedChannelsUnsupported")
    );
    let encoded = crate::project::encode(&doc).unwrap();
    let value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(value["version"], 32);
    let reopened = crate::project::decode(&encoded).unwrap();
    assert!(
        reopened.channels[0]
            .plane
            .samples
            .iter()
            .zip(samples)
            .all(|(a, b)| a.to_bits() == b.to_bits())
    );
    assert_eq!(crate::project::encode(&reopened).unwrap(), encoded);
    assert!(
        !crate::deep_color::has_native(&doc),
        "auxiliary scalar channels do not change painted RGB precision"
    );
    let error = match crate::formats::psd::encode_admitted(&doc, false) {
        Ok(_) => panic!("Saved channels were silently dropped"),
        Err(error) => error,
    };
    assert!(error.contains("SavedChannelsUnsupported"));
    let without = crate::compositor::export_png(&fixture(), 1).unwrap();
    let with = crate::compositor::export_png(&doc, 1).unwrap();
    assert_eq!(
        with, without,
        "channel storage alone never paints into artwork"
    );
    if let Some(root) = std::env::var_os("OMADESIGN_CHANNELS_QA") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(std::path::Path::new(&root).join("channels.oma"), encoded).unwrap();
    }
    drop(doc);
    assert!(lease.upgrade().is_some());
    drop(clone);
    assert!(lease.upgrade().is_none());
}
#[test]
fn craft_channels_current_embedded_retained_deleted_and_underdeclared_readers_refuse_loss() {
    let mut doc = fixture();
    doc.channels.push(Channel {
        id: crate::document::next_id(),
        name: "Alpha 1".into(),
        plane: plane(&[0., 0.5, 1.]),
    });
    let mut parent = Document::new("Embedded channels", 32., 16., 96.);
    parent
        .motion
        .compositions
        .push(crate::motion::compositions::Composition::new(doc.clone()));
    for d in [&doc, &parent] {
        assert!(crate::collaboration::snapshot(d).is_err());
        let error = match crate::formats::psd::encode_admitted(d, false) {
            Ok(_) => panic!("Dropped embedded channels"),
            Err(error) => error,
        };
        assert!(error.contains("SavedChannelsUnsupported"));
        let encoded = crate::project::encode(d).unwrap();
        let mut v: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(v["version"], 32);
        v["version"] = 31.into();
        assert!(
            crate::project::decode(&v.to_string())
                .unwrap_err()
                .contains("requires version 32")
        );
    }
    for path in [
        "/saved_channels",
        "/@compositions/42/document/saved_channels/1/plane/samples",
    ] {
        for value in [
            serde_json::Value::Null,
            serde_json::json!({"Value":null}),
            serde_json::json!({"Value":[]}),
        ] {
            let mut retained = Document::new("Retained channel", 1., 1., 96.);
            retained.collaboration=Some(serde_json::from_value(serde_json::json!({"session":"channels","actor":"a","base":{},"changes":[{"id":{"actor":"a","seq":1},"clock":1,"seen":{},"edit":{"type":"write","writes":[{"path":path,"value":value}]}},{"id":{"actor":"a","seq":2},"clock":2,"seen":{},"edit":{"type":"toggle","target":{"actor":"a","seq":1},"enabled":false}}]})).unwrap());
            let encoded = crate::project::encode(&retained).unwrap();
            let mut v: serde_json::Value = serde_json::from_str(&encoded).unwrap();
            assert_eq!(v["version"], 32);
            v["version"] = 31.into();
            assert!(crate::project::decode(&v.to_string()).is_err());
        }
    }
}
#[test]
fn craft_channels_invalid_scalar_payloads_names_and_transforms_preserve_source_and_pool() {
    let original = plane(&[0., 0.25, 1.]);
    let pool = resources::global();
    let before = (pool.usage().0, pool.pending_bytes());
    for values in [vec![f32::NAN], vec![f32::INFINITY], vec![-0.01], vec![1.01]] {
        let mut bad = original.clone();
        bad.width = 1;
        bad.samples = storage::adopt(values).unwrap();
        assert!(bad.validate().is_err());
    }
    let mut bad = original.clone();
    bad.width = 4;
    assert!(bad.validate().is_err());
    bad = original.clone();
    for transform in [[0.; 6], [1e-40, 0., 0., 1., 0., 0.]] {
        bad.transform = transform;
        assert!(bad.validate().is_err());
    }
    bad.transform = [-1., 0., 0., 1., 4., 0.];
    assert!(bad.validate().is_ok());
    for name in ["", "  ", "bad\nname"] {
        assert!(
            validate(&[Channel {
                id: 1,
                name: name.into(),
                plane: original.clone()
            }])
            .is_err()
        );
    }
    assert!(
        validate(&[
            Channel {
                id: 1,
                name: "Alpha".into(),
                plane: original.clone()
            },
            Channel {
                id: 1,
                name: "Copy".into(),
                plane: original.clone()
            }
        ])
        .is_err()
    );
    assert_eq!((pool.usage().0, pool.pending_bytes()), before);
    assert_eq!(original.samples.as_slice(), [0., 0.25, 1.]);
}
#[test]
fn craft_channels_actual_rgb_and_transparency_use_full_artwork_not_thumbnail_pixels() {
    let doc = fixture();
    let before = crate::project::encode(&doc).unwrap();
    for (target, expected) in [
        (Target::Transparency, 128. / 255.),
        (Target::Color(0), 64. / 255.),
        (Target::Color(1), 128. / 255.),
        (Target::Color(2), 191. / 255.),
    ] {
        let output = render(&doc, target, &storage::check).unwrap();
        assert_eq!((output.width, output.height), (8, 4));
        assert!(
            output
                .samples
                .iter()
                .all(|v| (*v - expected).abs() <= 1. / 255.),
            "{target:?}: {:?}",
            output.samples
        );
    }
    let output = render(&doc, Target::Composite, &storage::check).unwrap();
    let expected = (64. * 0.2126 + 128. * 0.7152 + 191. * 0.0722) / 255.;
    assert!(
        output
            .samples
            .iter()
            .all(|v| (*v - expected).abs() <= 1. / 255.)
    );
    assert!(render(&doc, Target::Color(3), &storage::check).is_err());
    assert_eq!(crate::project::encode(&doc).unwrap(), before);
}
#[test]
fn craft_channels_native_float_color_and_transparency_are_read_before_byte_quantization() {
    let mut doc = Document::new("Native channel", 2., 1., 96.);
    doc.transparent = true;
    doc.color.depth = Depth::F32;
    let values = vec![
        0.250001, 0.500001, 0.750001, 0.125001, 1.25, -0.125, 0.500002, 1.,
    ];
    doc.layers = vec![Layer::placed_raster(
        "Native",
        Pixels::from_samples(2, 1, Depth::F32, values.clone()).unwrap(),
        Pt::ZERO,
        Pt::ZERO,
    )];
    let before = crate::project::encode(&doc).unwrap();
    let alpha = render(&doc, Target::Transparency, &storage::check).unwrap();
    assert!((alpha.samples[0] - 0.125001).abs() < 1e-7);
    assert_eq!(alpha.samples[1], 1.);
    let red = render(&doc, Target::Color(0), &storage::check).unwrap();
    assert!((red.samples[0] - 0.250001).abs() < 1e-6);
    assert_eq!(
        red.samples[1], 1.,
        "selection coverage is unit bounded, original HDR stays untouched"
    );
    assert_eq!(crate::project::encode(&doc).unwrap(), before);
}
#[test]
fn craft_channels_cancellation_keeps_shared_sources_and_releases_generated_allocations() {
    let p = plane(&[0., 0.25, 0.5, 1.]);
    let before = p.samples.clone();
    let pool = resources::global();
    let usage = (pool.usage().0, pool.pending_bytes());
    let stop = || Err("cancelled".into());
    assert!(p.inverted(&stop).is_err());
    assert!(p.mask(&stop).is_err());
    assert_eq!((pool.usage().0, pool.pending_bytes()), usage);
    assert!(Arc::ptr_eq(&p.samples, &before));
    let inverted = p.inverted(&storage::check).unwrap();
    assert_eq!(inverted.samples.as_slice(), [1., 0.75, 0.5, 0.]);
    assert_eq!(p.samples.as_slice(), [0., 0.25, 0.5, 1.]);
}

#[test]
fn craft_channels_native_reader_validates_unit_coverage_before_normalization() {
    let mut doc = fixture();
    doc.channels.push(Channel {
        id: crate::document::next_id(),
        name: "Alpha".into(),
        plane: plane(&[0., 0.5, 1.]),
    });
    let encoded = crate::project::encode(&doc).unwrap();
    let mut wire: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    let mut invalid = doc.channels[0].plane.clone();
    invalid.samples = storage::adopt(vec![0., 1.25, 1.]).unwrap();
    wire["doc"]["saved_channels"][0]["plane"] = serde_json::to_value(invalid).unwrap();
    let damaged = wire.to_string();
    let before = damaged.clone();
    let error = crate::project::decode(&damaged).unwrap_err();
    assert!(error.contains("Channel coverage"), "{error}");
    assert_eq!(damaged, before);
    let reopened = crate::project::decode(&encoded).unwrap();
    assert_eq!(reopened.channels[0].plane.samples.as_slice(), [0., 0.5, 1.]);
}

#[test]
fn craft_channels_cmyk_components_use_authoritative_ink_planes_with_unchanged_icc() {
    let profile = include_bytes!("../formats/psd/fixtures/fogra39l-cc0.icc").to_vec();
    let pixels = Pixels::from_cmyk(
        2,
        1,
        Depth::F32,
        vec![0.125, 0.25, 0.375, 0.5, 1., 0.875, 0.75, 0.5, 0.25, 1.],
        profile.clone(),
        1,
    )
    .unwrap();
    let mut doc = Document::new("Ink channels", 2., 1., 96.);
    doc.transparent = true;
    doc.color.model = crate::deep_color::cms::Model::Cmyk;
    doc.color.depth = Depth::F32;
    doc.color.icc_profile = profile;
    doc.color.intent = 1;
    doc.layers = vec![Layer::placed_raster("CMYKA", pixels, Pt::ZERO, Pt::ZERO)];
    let original = crate::project::encode(&doc).unwrap();
    for (index, expected) in [[0.125, 0.875], [0.25, 0.75], [0.375, 0.5], [0.5, 0.25]]
        .into_iter()
        .enumerate()
    {
        let plane = render(&doc, Target::Color(index), &storage::check).unwrap();
        assert_eq!((plane.width, plane.height), (2, 1));
        assert!(
            plane
                .samples
                .iter()
                .zip(expected)
                .all(|(a, b)| (*a - b).abs() < 1e-7),
            "{:?}",
            plane.samples
        );
    }
    assert_eq!(crate::project::encode(&doc).unwrap(), original);
}
