use super::*;
use crate::ml::NoProgress;
use std::sync::atomic::{AtomicUsize, Ordering};
fn fixture() -> Document {
    let mut studio = crate::app::screen_export::tests::fixture();
    let target = studio.capture_export_assets().unwrap();
    studio.collect_export_assets(&target, true).unwrap();
    studio.doc
}
fn folder() -> PathBuf {
    let root = std::env::current_dir()
        .unwrap()
        .join(".artifacts")
        .join(format!("screen-presets-{}", crate::document::next_id()));
    std::fs::create_dir_all(&root).unwrap();
    root
}
fn plan(doc: &Document) -> Plan {
    Plan::new(doc, &[Scope::Asset(doc.screen_export.assets[0].id)]).unwrap()
}

#[test]
fn craft_export_presets_mobile_and_density_write_exact_sizes_and_shared_settings() {
    let mut doc = fixture();
    doc.screen_export.prefix = "ui-".into();
    for (preset, expected) in [
        (
            Preset::Mobile,
            vec![("", 1., ""), ("", 2., "@2x"), ("", 3., "@3x")],
        ),
        (
            Preset::Density,
            vec![
                ("ldpi", 0.75, ""),
                ("mdpi", 1., ""),
                ("hdpi", 1.5, ""),
                ("xhdpi", 2., ""),
                ("xxhdpi", 3., ""),
                ("xxxhdpi", 4., ""),
            ],
        ),
    ] {
        let prior = doc.screen_export.clone();
        doc.screen_export = preset.catalog(&doc.screen_export);
        assert_eq!(doc.screen_export.prefix, prior.prefix);
        assert_eq!(doc.screen_export.assets, prior.assets);
        assert_eq!(doc.screen_export.rows.len(), expected.len());
        let ids: HashSet<_> = doc.screen_export.rows.iter().map(|row| row.id).collect();
        assert_eq!(ids.len(), expected.len());
        let root = folder();
        let before = crate::project::encode(&doc).unwrap();
        let outputs = plan(&doc).save(&root, &NoProgress).unwrap();
        assert_eq!(outputs.len(), expected.len());
        for (row, (folder, scale, suffix)) in doc.screen_export.rows.iter().zip(expected) {
            assert_eq!(
                (row.folder.as_str(), row.resolution(), row.suffix.as_str()),
                (folder, Resolution::Scale(scale), suffix)
            );
            let file = root.join(folder).join(format!("ui-Badge{suffix}.png"));
            let image = image::open(&file).unwrap().to_rgba8();
            let dimensions = ((16. * scale).round() as u32, (12. * scale).round() as u32);
            assert_eq!(image.dimensions(), dimensions);
            assert_eq!(
                image.get_pixel(dimensions.0 / 2, dimensions.1 / 2).0,
                [240, 30, 20, 255]
            );
            if let Some(retained) = std::env::var_os("OMADESIGN_EXPORT_PRESETS_QA") {
                let destination = Path::new(&retained)
                    .join(if folder.is_empty() {
                        "mobile"
                    } else {
                        "density"
                    })
                    .join(folder);
                std::fs::create_dir_all(&destination).unwrap();
                std::fs::copy(&file, destination.join(file.file_name().unwrap())).unwrap();
            }
        }
        assert_eq!(crate::project::encode(&doc).unwrap(), before);
        assert_eq!(
            std::fs::read_dir(&root).unwrap().count(),
            if doc.screen_export.subfolders { 6 } else { 3 }
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn craft_export_presets_native31_gate_keeps_old_rows_and_all_retained_folder_paths() {
    let mut doc = fixture();
    let old = crate::project::encode(&doc).unwrap();
    assert!(!old.contains("export_folder"));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&old).unwrap()["version"],
        29
    );
    doc.screen_export = Preset::Density.catalog(&doc.screen_export);
    let mut parent = Document::new("Embedded", 30., 20., 96.);
    parent
        .motion
        .compositions
        .push(crate::motion::compositions::Composition::new(doc.clone()));
    for source in [&doc, &parent] {
        let encoded = crate::project::encode(source).unwrap();
        assert_eq!(
            crate::project::encode(&crate::project::decode(&encoded).unwrap()).unwrap(),
            encoded
        );
        let mut wire: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(wire["version"], 31);
        wire["version"] = 30.into();
        assert!(
            crate::project::decode(&wire.to_string())
                .unwrap_err()
                .contains("requires version 31")
        );
    }
    for path in [
        "/screen_export/export_subfolders",
        "/screen_export/rows/1/export_folder",
        "/@compositions/42/document/screen_export/rows/1/export_folder",
    ] {
        for value in [
            serde_json::json!(null),
            serde_json::json!({"Value":null}),
            serde_json::json!({"Value":"mdpi"}),
        ] {
            let mut retained = Document::new("Retained folders", 8., 8., 96.);
            retained.collaboration=Some(serde_json::from_value(serde_json::json!({"session":"folders","actor":"a","base":{},"changes":[{"id":{"actor":"a","seq":1},"clock":1,"seen":{},"edit":{"type":"write","writes":[{"path":path,"value":value}]}},{"id":{"actor":"a","seq":2},"clock":2,"seen":{},"edit":{"type":"toggle","target":{"actor":"a","seq":1},"enabled":false}}]})).unwrap());
            let encoded = crate::project::encode(&retained).unwrap();
            let mut wire: serde_json::Value = serde_json::from_str(&encoded).unwrap();
            assert_eq!(wire["version"], 31);
            crate::project::decode(&encoded).unwrap();
            wire["version"] = 30.into();
            assert!(
                crate::project::validate_retained_json(&wire.to_string())
                    .unwrap_err()
                    .contains("requires version 31")
            );
        }
    }
    for invalid in [
        "..",
        ".",
        "../outside",
        "/absolute",
        "a/b",
        "a\\b",
        "trailing.",
    ] {
        let mut catalog = doc.screen_export.clone();
        catalog.rows[0].folder = invalid.into();
        assert!(catalog.validate().is_err());
    }
}

#[test]
fn craft_export_presets_cancel_after_publication_and_concurrent_collision_preserve_foreign_files() {
    let mut doc = fixture();
    doc.screen_export = Preset::Density.catalog(&doc.screen_export);
    let root = folder();
    struct Cancel(PathBuf);
    impl Progress for Cancel {
        fn report(&self, _: &str, _: usize, _: usize) {}
        fn cancelled(&self) -> bool {
            self.0.join("ldpi/Badge.png").exists()
        }
    }
    assert!(plan(&doc).save(&root, &Cancel(root.clone())).is_err());
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    struct Collision {
        root: PathBuf,
        renders: AtomicUsize,
    }
    impl Progress for Collision {
        fn report(&self, stage: &str, _: usize, _: usize) {
            if stage == "Rendering export" && self.renders.fetch_add(1, Ordering::Relaxed) == 1 {
                std::fs::write(self.root.join("mdpi/Badge.png"), b"independent").unwrap();
            }
        }
        fn cancelled(&self) -> bool {
            false
        }
    }
    assert!(
        plan(&doc)
            .save(
                &root,
                &Collision {
                    root: root.clone(),
                    renders: AtomicUsize::new(0)
                }
            )
            .unwrap_err()
            .contains("Cannot publish")
    );
    assert_eq!(
        std::fs::read(root.join("mdpi/Badge.png")).unwrap(),
        b"independent"
    );
    assert!(!root.join("ldpi").exists());
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn craft_export_presets_symlink_and_replaced_directories_never_receive_output_or_foreign_cleanup() {
    use std::os::unix::fs::symlink;
    let mut doc = fixture();
    doc.screen_export = Preset::Density.catalog(&doc.screen_export);
    let root = folder();
    let outside = folder();
    std::fs::write(outside.join("keep"), b"keep").unwrap();
    symlink(&outside, root.join("ldpi")).unwrap();
    assert!(
        plan(&doc)
            .save(&root, &NoProgress)
            .unwrap_err()
            .contains("real directory")
    );
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 1);
    std::fs::remove_file(root.join("ldpi")).unwrap();
    struct Replace {
        root: PathBuf,
        renders: AtomicUsize,
    }
    impl Progress for Replace {
        fn report(&self, stage: &str, _: usize, _: usize) {
            if stage == "Rendering export" && self.renders.fetch_add(1, Ordering::Relaxed) == 0 {
                std::fs::rename(self.root.join("ldpi"), self.root.join("moved")).unwrap();
                std::fs::create_dir(self.root.join("ldpi")).unwrap();
                std::fs::write(self.root.join("ldpi/keep"), b"independent").unwrap();
            }
        }
        fn cancelled(&self) -> bool {
            false
        }
    }
    assert!(
        plan(&doc)
            .save(
                &root,
                &Replace {
                    root: root.clone(),
                    renders: AtomicUsize::new(0)
                }
            )
            .unwrap_err()
            .contains("subfolder changed")
    );
    assert_eq!(
        std::fs::read(root.join("ldpi/keep")).unwrap(),
        b"independent"
    );
    assert!(!root.join("moved/Badge.png").exists());
    assert_eq!(std::fs::read(outside.join("keep")).unwrap(), b"keep");
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(outside).unwrap();
}
