use super::*;
use crate::{app::Studio, ml::NoProgress};
use std::sync::atomic::{AtomicUsize, Ordering};
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
