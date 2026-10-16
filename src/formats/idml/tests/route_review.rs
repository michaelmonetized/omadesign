use super::{
    review::{master_fixture, pack_parts, zip_parts},
    *,
};
use crate::import::Imported;
use std::path::{Path, PathBuf};

fn directory(label: &str) -> (PathBuf, bool) {
    let keep = std::env::var_os("OMADESIGN_IDML_RECEIPTS");
    let directory = keep
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
                ".artifacts/idml-route-test-{}",
                crate::document::next_id()
            ))
        })
        .join(label);
    assert!(directory.starts_with("/home"));
    assert!(!directory.exists(), "preserve existing test evidence");
    std::fs::create_dir_all(&directory).unwrap();
    (directory, keep.is_some())
}
fn open(path: &Path) -> Document {
    let Imported::Document(doc) = crate::import::open_any(path).unwrap() else {
        panic!("expected editable IDML document");
    };
    doc
}
fn pixel_count(doc: &Document) -> usize {
    doc.layers
        .iter()
        .filter(|l| l.kind.pixels().is_some())
        .count()
}
fn external_fixture(uri: &str) -> (Vec<u8>, Vec<u8>) {
    let mut parts = zip_parts(&master_fixture());
    let image = parts.remove("Links/logo.png").unwrap();
    let name = "MasterSpreads/MasterSpread_A.xml";
    let xml = String::from_utf8(parts[name].clone())
        .unwrap()
        .replace("Links/logo.png", &super::super::xml(uri))
        .replace(
            "Self=\"masterLogo\"",
            "Self=\"masterLogo\" Visible=\"false\" Locked=\"false\"",
        );
    parts.insert(name.into(), xml.into_bytes());
    (pack_parts(&parts), image)
}

#[test]
fn round2_source_folder_masters_export_without_original_folder() {
    for mode in ["relative", "file-uri"] {
        let (directory, keep) = directory(&format!("source-folder-{mode}"));
        let source = directory.join("source");
        let delivered = directory.join("delivery");
        std::fs::create_dir_all(source.join("Links")).unwrap();
        std::fs::create_dir_all(&delivered).unwrap();
        let uri = if mode == "relative" {
            "Links/logo.png".to_owned()
        } else {
            url::Url::from_file_path(source.join("Links/logo.png"))
                .unwrap()
                .to_string()
        };
        let (bytes, image) = external_fixture(&uri);
        std::fs::write(source.join("book.idml"), bytes).unwrap();
        std::fs::write(source.join("Links/logo.png"), &image).unwrap();
        let doc = open(&source.join("book.idml"));
        assert_eq!(pixel_count(&doc), 3);
        let previews = doc
            .layers
            .iter()
            .filter(|l| {
                l.kind.pixels().is_some() && doc.publishing.inherited_layers.contains(&l.id)
            })
            .collect::<Vec<_>>();
        assert_eq!(previews.len(), 2);
        assert!(previews.iter().all(|l| !l.visible && l.locked));
        assert!(
            !doc.import_notes
                .iter()
                .any(|n| n.contains(&format!("missing: {uri}")))
        );
        let native = delivered.join("book.oma");
        crate::formats::cli::export(&doc, &native).unwrap();
        std::fs::rename(&source, directory.join("source-unavailable")).unwrap();
        assert!(!source.exists());
        let reopened = crate::formats::cli::document(&native).unwrap();
        let output = delivered.join("book.idml");
        crate::formats::cli::export(&reopened, &output).unwrap();
        assert!(!delivered.join("Links/logo.png").exists());
        assert_eq!(pixel_count(&open(&output)), 3);
        let (detached, notes) = read(&std::fs::read(&output).unwrap(), "No link reader").unwrap();
        assert_eq!(pixel_count(&detached), 3);
        assert!(!notes.iter().any(|n| n.contains(&format!("missing: {uri}"))));
        let master = roxmltree::Document::parse(&detached.publishing.masters[0].xml).unwrap();
        assert!(
            master
                .descendants()
                .any(|n| n.attribute("LinkResourceURI") == Some(uri.as_str()))
        );
        let contents = master
            .descendants()
            .find(|n| n.has_tag_name("Contents"))
            .unwrap();
        assert_eq!(
            base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                contents.text().unwrap()
            )
            .unwrap(),
            image
        );
        if !keep {
            std::fs::remove_dir_all(directory).unwrap();
        }
    }
}

#[test]
fn round2_master_link_loading_keeps_source_folder_boundary() {
    let (directory, keep) = directory("source-boundary");
    let source = directory.join("source");
    let outside = directory.join("outside");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    for (mode, uri) in [
        ("relative", "../outside/logo.png".to_owned()),
        (
            "file-uri",
            url::Url::from_file_path(outside.join("logo.png"))
                .unwrap()
                .to_string(),
        ),
    ] {
        let (bytes, image) = external_fixture(&uri);
        std::fs::write(outside.join("logo.png"), image).unwrap();
        let path = source.join(format!("{mode}.idml"));
        std::fs::write(&path, bytes).unwrap();
        let doc = open(&path);
        assert_eq!(pixel_count(&doc), 1);
        assert!(
            doc.import_notes
                .iter()
                .any(|n| n.contains(&format!("missing: {uri}")))
        );
        let master = roxmltree::Document::parse(&doc.publishing.masters[0].xml).unwrap();
        assert!(!master.descendants().any(|n| n.has_tag_name("Contents")));
        assert!(doc.publishing.master_assets.is_empty());
        crate::formats::cli::export(&doc, &source.join(format!("{mode}.oma"))).unwrap();
    }
    if !keep {
        std::fs::remove_dir_all(directory).unwrap();
    }
}
