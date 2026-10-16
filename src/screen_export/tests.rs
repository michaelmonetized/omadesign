use super::*;
use crate::{app::Studio, ml::NoProgress};
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn craft_export_sizing_reference_grammar_aspect_ratio_and_legacy_wire() {
    let doc = Document::new("Pixels", 80., 60., 96.);
    for (text, expected, dimensions) in [
        (" 0.5X ", Resolution::Scale(0.5), (40, 30)),
        ("1.5", Resolution::Scale(1.5), (120, 90)),
        ("512w", Resolution::Width(512.), (512, 384)),
        ("512h", Resolution::Height(512.), (683, 512)),
        ("144PPI", Resolution::Ppi(144.), (120, 90)),
    ] {
        let size = Resolution::parse(text).unwrap();
        assert_eq!(size, expected);
        assert_eq!(
            export::dimensions_scaled(
                &Source::Document(doc.clone()),
                Row::default().settings(),
                size.options(&doc).unwrap()
            )
            .unwrap(),
            dimensions
        );
    }
    for text in ["", "0x", "-1w", "NaN", "infppi", "3px", "w", "1e999x"] {
        assert!(Resolution::parse(text).is_err(), "{text}");
    }
    let original = r#"{"id":1,"scale":1,"format":"Png","suffix":""}"#;
    let mut row: Row = serde_json::from_str(original).unwrap();
    assert_eq!(serde_json::to_string(&row).unwrap(), original);
    row.set_resolution(Resolution::Scale(1.5));
    assert_eq!(row.scale, 1);
    assert_eq!(row.export_resolution, Some(Resolution::Scale(1.5)));
    row.set_resolution(Resolution::Scale(2.));
    assert!(row.export_resolution.is_none());
    assert_eq!(row.scale, 2);
    let mut invalid = doc;
    invalid.dpi = 0.;
    assert!(Resolution::Ppi(144.).options(&invalid).is_err());
}

#[test]
fn craft_export_sizing_feature30_current_embedded_retained_and_withdrawn_metadata() {
    let mut studio = fixture();
    let old = crate::project::encode(&studio.doc).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&old).unwrap()["version"],
        29
    );
    assert!(!old.contains("export_resolution"));
    studio.doc.screen_export.rows[0].set_resolution(Resolution::Width(512.));
    for nested in [false, true] {
        let doc = if nested {
            let mut parent = Document::new("Parent", 80., 60., 96.);
            parent
                .motion
                .compositions
                .push(crate::motion::compositions::Composition::new(
                    studio.doc.clone(),
                ));
            parent
        } else {
            studio.doc.clone()
        };
        let encoded = crate::project::encode(&doc).unwrap();
        let restored = crate::project::decode(&encoded).unwrap();
        assert_eq!(crate::project::encode(&restored).unwrap(), encoded);
        let mut wire: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(wire["version"], 30);
        wire["version"] = 29.into();
        assert!(
            crate::project::decode(&wire.to_string())
                .unwrap_err()
                .contains("requires version 30")
        );
    }
    for path in [
        "/screen_export/rows/0/export_resolution",
        "/@compositions/42/document/screen_export/rows/8/export_resolution/Width",
    ] {
        for value in [
            serde_json::json!({"Value":null}),
            serde_json::json!("List"),
            serde_json::json!({"Variant":"Width"}),
            serde_json::json!({"Value":{"Width":512}}),
        ] {
            for base in [false, true] {
                let mut replica =
                    serde_json::json!({"session":"sizes","actor":"a","base":{},"changes":[]});
                if base {
                    replica["base"][path] = value.clone();
                } else {
                    replica["changes"] = serde_json::json!([
                        {"id":{"actor":"a","seq":1},"clock":1,"seen":{},"edit":{"type":"write","writes":[{"path":path,"value":value}]}},
                        {"id":{"actor":"a","seq":2},"clock":2,"seen":{},"edit":{"type":"toggle","target":{"actor":"a","seq":1},"enabled":false}}
                    ]);
                }
                let mut doc = Document::new("Retained", 10., 10., 96.);
                doc.collaboration = Some(serde_json::from_value(replica).unwrap());
                assert!(
                    doc.collaboration
                        .as_ref()
                        .unwrap()
                        .protocol_requirement()
                        .is_err()
                );
                let mut wire: serde_json::Value =
                    serde_json::from_str(&crate::project::encode(&doc).unwrap()).unwrap();
                assert_eq!(wire["version"], 30, "{path} {base}");
                wire["version"] = 29.into();
                assert!(
                    crate::project::validate_retained_json(&wire.to_string())
                        .unwrap_err()
                        .contains("requires version 30")
                );
            }
        }
        let mut doc=Document::new("Deleted resolution",10.,10.,96.);
        doc.collaboration=Some(serde_json::from_value(serde_json::json!({"session":"sizes","actor":"a","base":{},"changes":[
            {"id":{"actor":"a","seq":1},"clock":1,"seen":{},"edit":{"type":"write","writes":[{"path":path,"value":null}]}},
            {"id":{"actor":"a","seq":2},"clock":2,"seen":{},"edit":{"type":"toggle","target":{"actor":"a","seq":1},"enabled":false}}
        ]})).unwrap());
        assert_eq!(doc.collaboration.as_ref().unwrap().protocol_requirement().unwrap(),1);
        let encoded=crate::project::encode(&doc).unwrap();
        assert_eq!(serde_json::from_str::<serde_json::Value>(&encoded).unwrap()["version"],30);
        crate::project::decode(&encoded).unwrap();
        let mut wire:serde_json::Value=serde_json::from_str(&encoded).unwrap();wire["version"]=29.into();
        assert!(crate::project::validate_retained_json(&wire.to_string()).unwrap_err().contains("requires version 30"));
    }
}

#[test]
fn craft_export_sizing_batch_uses_actual_asset_width_height_and_fractional_sources() {
    let mut studio = fixture();
    studio.doc.screen_export.rows = (0..3)
        .map(|i| Row {
            id: i + 1,
            suffix: format!("-{i}"),
            export_resolution: Some(
                [
                    Resolution::Scale(0.5),
                    Resolution::Width(24.),
                    Resolution::Height(24.),
                ][i as usize],
            ),
            ..Default::default()
        })
        .collect();
    let before = crate::project::encode(&studio.doc).unwrap();
    let root = folder();
    let output = Plan::new(
        &studio.doc,
        &[Scope::Asset(studio.doc.screen_export.assets[0].id)],
    )
    .unwrap()
    .save(&root, &NoProgress)
    .unwrap();
    assert_eq!(output.len(), 3);
    for (i, size) in [(0, (8, 6)), (1, (24, 18)), (2, (32, 24))] {
        let image = image::open(root.join(format!("Badge-{i}.png")))
            .unwrap()
            .to_rgba8();
        assert_eq!(image.dimensions(), size);
        assert_eq!(
            image.get_pixel(size.0 / 2, size.1 / 2).0,
            [240, 30, 20, 255]
        );
    }
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    std::fs::remove_dir_all(root).unwrap();
}
fn fixture() -> Studio {
    let mut s = crate::app::screen_export::tests::fixture();
    let target = s.capture_export_assets().unwrap();
    s.collect_export_assets(&target, true).unwrap();
    s
}
fn folder() -> PathBuf {
    let p = std::env::current_dir()
        .unwrap()
        .join(".artifacts")
        .join(format!("screen-export-{}", crate::document::next_id()));
    std::fs::create_dir_all(&p).unwrap();
    p
}
#[test]
fn craft_screen_export_writes_actual_png_scales_and_vector_artwork_without_extra_shapes() {
    let mut s = fixture();
    s.doc.screen_export.rows.extend([
        Row {
            id: 2,
            scale: 2,
            suffix: "@2x".into(),
            ..Default::default()
        },
        Row {
            id: 3,
            format: Format::Svg,
            ..Default::default()
        },
    ]);
    let encoded = crate::project::encode(&s.doc).unwrap();
    let root = folder();
    let outputs = Plan::new(&s.doc, &[Scope::Asset(s.doc.screen_export.assets[0].id)])
        .unwrap()
        .save(&root, &NoProgress)
        .unwrap();
    assert_eq!(outputs.len(), 3);
    for (file, size) in [("Badge.png", (16, 12)), ("Badge@2x.png", (32, 24))] {
        let image = image::open(root.join(file)).unwrap().to_rgba8();
        assert_eq!(image.dimensions(), size);
        assert_eq!(image.get_pixel(4, 4).0, [240, 30, 20, 255]);
    }
    let svg = std::fs::read_to_string(root.join("Badge.svg")).unwrap();
    let (doc, _) = crate::formats::svg::read(&svg, "Roundtrip").unwrap();
    assert_eq!((doc.width, doc.height), (16., 12.));
    let pm = crate::compositor::render_export(&doc, 1).unwrap();
    assert_eq!(pm.pixel(4, 4).unwrap().red(), 240);
    assert_eq!(crate::project::encode(&s.doc).unwrap(), encoded);
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 3);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn craft_screen_export_collisions_cancel_and_refused_formats_publish_nothing() {
    let mut s = fixture();
    let id = s.doc.screen_export.assets[0].id;
    let root = folder();
    std::fs::write(root.join("Badge.png"), b"accepted").unwrap();
    assert!(
        Plan::new(&s.doc, &[Scope::Asset(id)])
            .unwrap()
            .save(&root, &NoProgress)
            .unwrap_err()
            .contains("already exists")
    );
    assert_eq!(std::fs::read(root.join("Badge.png")).unwrap(), b"accepted");
    std::fs::remove_file(root.join("Badge.png")).unwrap();
    struct Cancel(AtomicUsize);
    impl Progress for Cancel {
        fn report(&self, _: &str, _: usize, _: usize) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
        fn cancelled(&self) -> bool {
            self.0.load(Ordering::Relaxed) > 2
        }
    }
    assert!(
        Plan::new(&s.doc, &[Scope::Asset(id)])
            .unwrap()
            .save(&root, &Cancel(AtomicUsize::new(0)))
            .is_err()
    );
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    s.doc.screen_export.rows.push(Row {
        id: 2,
        ..Default::default()
    });
    assert!(
        Plan::new(&s.doc, &[Scope::Asset(id)])
            .err()
            .unwrap()
            .contains("duplicate")
    );
    s.doc.screen_export.rows[1].suffix = "-pdf".into();
    s.doc.screen_export.rows[1].format = Format::Pdf;
    s.doc.motion.scene3d = Some(Default::default());
    assert!(
        Plan::new(&s.doc, &[Scope::Document])
            .unwrap()
            .save(&root, &NoProgress)
            .is_err()
    );
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn craft_screen_export_native_gate_checks_current_nested_retained_and_underdeclared_data() {
    let s = fixture();
    let encoded = crate::project::encode(&s.doc).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(value["version"], 29);
    value["version"] = 28.into();
    assert!(
        crate::project::decode(&value.to_string())
            .unwrap_err()
            .contains("requires version 29")
    );
    let mut parent = Document::new("Nested", 20., 20., 96.);
    parent
        .motion
        .compositions
        .push(crate::motion::compositions::Composition::new(s.doc.clone()));
    let encoded = crate::project::encode(&parent).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&encoded).unwrap()["version"],
        29
    );
    assert_eq!(
        crate::project::decode(&encoded)
            .unwrap()
            .motion
            .compositions[0]
            .document
            .screen_export,
        s.doc.screen_export
    );
    let mut neutral = Document::new("Legacy", 20., 20., 96.);
    let text = crate::project::encode(&neutral).unwrap();
    assert!(!text.contains("screen_export"));
    neutral.collaboration=Some(serde_json::from_value(serde_json::json!({"session":"gate","actor":"a","base":{"/screen_export":{"Value":s.doc.screen_export}},"changes":[]})).unwrap());
    assert!(
        neutral
            .collaboration
            .as_ref()
            .unwrap()
            .protocol_requirement()
            .is_err()
    );
    let retained=serde_json::json!({"version":28,"doc":{"collaboration":{"changes":[{"edit":{"writes":[{"path":"/screen_export","value":{"Value":{"assets":[]}}}]}}]}}}).to_string();
    assert!(
        crate::project::validate_retained_json(&retained)
            .unwrap_err()
            .contains("requires version 29")
    );
}

#[test]
fn craft_screen_export_concurrent_collision_rolls_back_only_own_publication() {
    let mut s = fixture();
    s.doc.screen_export.rows.push(Row {
        id: 2,
        scale: 2,
        suffix: "@2x".into(),
        ..Default::default()
    });
    let root = folder();
    struct Collision {
        root: PathBuf,
        renders: AtomicUsize,
    }
    impl Progress for Collision {
        fn report(&self, stage: &str, _: usize, _: usize) {
            if stage == "Rendering export" && self.renders.fetch_add(1, Ordering::Relaxed) == 1 {
                std::fs::write(self.root.join("Badge@2x.png"), b"independent writer").unwrap();
            }
        }
        fn cancelled(&self) -> bool {
            false
        }
    }
    let plan = Plan::new(&s.doc, &[Scope::Asset(s.doc.screen_export.assets[0].id)]).unwrap();
    assert!(
        plan.save(
            &root,
            &Collision {
                root: root.clone(),
                renders: AtomicUsize::new(0)
            }
        )
        .unwrap_err()
        .contains("Cannot publish")
    );
    assert!(!root.join("Badge.png").exists());
    assert_eq!(
        std::fs::read(root.join("Badge@2x.png")).unwrap(),
        b"independent writer"
    );
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn craft_screen_export_retained_list_markers_and_deleted_nested_fields_require_reader29() {
    for path in [
        "/screen_export",
        "/screen_export/rows/0/scale",
        "/@compositions/42/document/screen_export/assets/0/name",
    ] {
        for cell in [
            serde_json::json!("Object"),
            serde_json::json!("List"),
            serde_json::json!({"Value":null}),
            serde_json::json!({"Value":"retained"}),
        ] {
            for in_base in [true, false] {
                let mut replica = serde_json::json!({"session":"retained-assets","actor":"a","base":{},"changes":[]});
                if in_base {
                    replica["base"][path] = cell.clone();
                } else {
                    replica["changes"] = serde_json::json!([
                        {"id":{"actor":"a","seq":1},"clock":1,"seen":{},"edit":{"type":"write","writes":[{"path":path,"value":cell}]}},
                        {"id":{"actor":"a","seq":2},"clock":2,"seen":{},"edit":{"type":"toggle","target":{"actor":"a","seq":1},"enabled":false}}
                    ]);
                }
                let mut document = Document::new("Retained assets", 10., 10., 96.);
                document.collaboration = Some(serde_json::from_value(replica).unwrap());
                let text = crate::project::encode(&document).unwrap();
                let mut envelope: serde_json::Value = serde_json::from_str(&text).unwrap();
                assert_eq!(envelope["version"], 29, "{path}, {in_base}");
                envelope["version"] = 28.into();
                assert!(
                    crate::project::validate_retained_json(&envelope.to_string())
                        .unwrap_err()
                        .contains("requires version 29")
                );
                assert!(
                    document
                        .collaboration
                        .as_ref()
                        .unwrap()
                        .protocol_requirement()
                        .is_err()
                );
            }
        }
    }
}

#[test]
fn craft_screen_export_keeps_actual_inside_center_outside_strokes_and_blur_tails() {
    use crate::{
        color::Rgba,
        document::{Stroke, StrokeAlignment},
        filter::{FilterStack, Fx},
    };
    for (alignment, dimensions) in [
        (StrokeAlignment::Inside, (16, 12)),
        (StrokeAlignment::Center, (20, 16)),
        (StrokeAlignment::Outside, (24, 20)),
    ] {
        let mut s = fixture();
        let (layer, id) = s.selection[0];
        s.doc.find_shape_mut(layer, id).unwrap().style.stroke = Some(Stroke {
            alignment,
            width: 4.,
            color: Rgba::BLACK,
            ..Default::default()
        });
        let before = crate::project::encode(&s.doc).unwrap();
        let root = folder();
        Plan::new(&s.doc, &[Scope::Asset(s.doc.screen_export.assets[0].id)])
            .unwrap()
            .save(&root, &NoProgress)
            .unwrap();
        let image = image::open(root.join("Badge.png")).unwrap().to_rgba8();
        assert_eq!(image.dimensions(), dimensions, "{alignment:?}");
        assert_eq!(
            image.get_pixel(0, dimensions.1 / 2).0,
            [0, 0, 0, 255],
            "{alignment:?}"
        );
        assert_eq!(
            image.get_pixel(dimensions.0 / 2, dimensions.1 / 2).0,
            [240, 30, 20, 255]
        );
        assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
        std::fs::remove_dir_all(root).unwrap();
    }
    let mut s = fixture();
    let (layer, id) = s.selection[0];
    s.doc.find_shape_mut(layer, id).unwrap().filters = FilterStack {
        enabled: true,
        items: vec![Fx::Blur { std: 1. }],
        ..Default::default()
    };
    let root = folder();
    Plan::new(&s.doc, &[Scope::Asset(s.doc.screen_export.assets[0].id)])
        .unwrap()
        .save(&root, &NoProgress)
        .unwrap();
    let image = image::open(root.join("Badge.png")).unwrap().to_rgba8();
    assert_eq!(image.dimensions(), (40, 36));
    assert!(image.get_pixel(11, 18)[3] > 0);
    assert_eq!(image.get_pixel(0, 18)[3], 0);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn craft_screen_export_graph_overflow_has_an_explicit_asset_bounds_refusal() {
    let (doc,_)=crate::formats::svg::read(r##"<svg xmlns="http://www.w3.org/2000/svg" width="60" height="40"><defs><filter id="blur"><feGaussianBlur stdDeviation="2"/></filter></defs><g filter="url(#blur)"><rect x="10" y="10" width="20" height="20" fill="#f00"/></g></svg>"##,"Graph asset").unwrap();
    let (layer, shape) = doc
        .layers
        .iter()
        .find_map(|layer| {
            layer
                .kind
                .shapes()
                .and_then(|s| s.first())
                .map(|s| (layer.id, s.id))
        })
        .unwrap();
    let asset = Asset {
        id: 100,
        name: "Graph".into(),
        objects: vec![Object { layer, shape }],
    };
    assert!(
        asset
            .document(&doc)
            .unwrap_err()
            .contains("SVG filter graphs")
    );
    assert!(crate::export::encode(&Source::Document(doc), Default::default(), &NoProgress).is_ok());
}

#[test]
fn craft_screen_export_cubic_crop_uses_actual_extrema_not_sampled_points() {
    use crate::geom::{Anchor, Geom, Pt};
    let mut s = fixture();
    let (layer, id) = s.selection[0];
    let mut start = Anchor::corner(Pt::new(10., 10.));
    start.h_out = Pt::new(0., 1000.);
    let end = Anchor::corner(Pt::new(50., 10.));
    s.doc.find_shape_mut(layer, id).unwrap().geom = Geom::Path {
        anchors: vec![start, end],
        closed: true,
    };
    let asset = &s.doc.screen_export.assets[0];
    let doc = asset.document(&s.doc).unwrap();
    assert_eq!(doc.width, 40.);
    assert!((doc.height - 4000. / 9.).abs() < 0.001);
    s.doc.screen_export.rows[0].scale = 8;
    let root = folder();
    Plan::new(&s.doc, &[Scope::Asset(asset.id)])
        .unwrap()
        .save(&root, &NoProgress)
        .unwrap();
    let image = image::open(root.join("Badge.png")).unwrap().to_rgba8();
    assert_eq!(image.dimensions(), (320, 3556));
    assert!(image.rows().skip(3553).flatten().any(|pixel| pixel[3] > 0));
    std::fs::remove_dir_all(root).unwrap();
}
