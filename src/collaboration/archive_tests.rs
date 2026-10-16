use super::*;
use crate::{
    document::{Fill, Layer, Shape, Style},
    geom::{CharSpan, Geom, TypeRun},
    svg_source::{SvgFilterGraph, SvgPattern},
};
use std::{fs, path::PathBuf, process::Command, sync::Arc};

const CHILD: &str = "OMADESIGN_RETAINED_ARCHIVE_CASE";
const ROUTES: [&str; 4] = ["save", "recovery", "preview-save", "preview-recovery"];
const XML: &str = r##"<pattern id="p" width="2" height="2" patternUnits="userSpaceOnUse"><rect width="1" height="2" fill="#28a"/></pattern><filter id="f"><feGaussianBlur stdDeviation="0.25"/></filter>"##;

fn pattern() -> Fill {
    Fill::Pattern(SvgPattern {
        definitions: Arc::from(XML.to_owned()),
        id: "p".into(),
        transform: [1., 0., 0., 1., 0., 0.],
        opacity: 1.,
    })
}

fn rich(fonts: &[String]) -> Document {
    let mut doc = native_tests::fixture();
    doc.color.icc_profile = include_bytes!("../formats/psd/fixtures/fogra39l-cc0.icc").to_vec();
    let source = crate::source_attachment::SourceAttachment::new(
        "original.bin",
        "binary",
        b"\0\xff<definitions>opaque original, not XML</definitions>".to_vec(),
    )
    .unwrap();
    let source_id = doc.retain_source(source);
    let mut layer = Layer::vector("Archived type and paint");
    layer.source_attachment = Some(source_id);
    layer.svg_filter = Some(SvgFilterGraph {
        definitions: Arc::from(XML.to_owned()),
        original_definitions: Arc::from(XML.to_owned()),
        filters: vec!["f".into()],
        transform: [1., 0., 0., 1., 0., 0.],
        reductions: vec![],
    });
    layer.kind.shapes_mut().unwrap().push(Shape::new(
        Geom::Text(TypeRun {
            content: "Archive A Ω".into(),
            font: fonts[0].clone(),
            px: 16.,
            spans: vec![CharSpan {
                start: 8,
                end: 11,
                font: Some(fonts[1].clone()),
                fill: Some(pattern()),
                ..Default::default()
            }],
            ..Default::default()
        }),
        Style {
            fill: pattern(),
            ..Default::default()
        },
    ));
    doc.layers.push(layer);
    doc
}

fn neutral() -> Document {
    let mut doc = Document::new("Retained archive", 32., 32., 72.);
    doc.layers.clear();
    doc.transparent = true;
    doc
}

fn pixel_cells(replica: &Replica) -> Vec<(String, Cell)> {
    replica
        .retained_cells()
        .filter(|(_, cell)| matches!(cell, Cell::Pixels { .. }))
        .map(|(path, cell)| (path.to_owned(), cell.clone()))
        .collect()
}

fn swap(doc: Document, path: &std::path::Path) {
    crate::project::prepare_swap(
        crate::project::SwapMeta {
            id: "retained".into(),
            original: None,
            name: doc.name.clone(),
            saved_at: 1,
            doc,
        },
        path.to_owned(),
    )
    .unwrap()
    .commit()
    .unwrap();
}

#[test]
fn retained_archives_cold_reopen_undo_remote_restore_and_recovery_pruning() {
    let root = std::env::current_dir()
        .unwrap()
        .join(".artifacts")
        .join(format!(
            "live-common-archive-{}",
            crate::document::next_id()
        ));
    fs::create_dir_all(&root).unwrap();
    let originals = root.join("originals");
    fs::create_dir(&originals).unwrap();
    let mut fonts = Vec::new();
    for role in ["run", "span"] {
        let mut bytes = include_bytes!("../../tests/assets/fonts/EBGaramond.ttf").to_vec();
        bytes.extend_from_slice(format!("{role}-{}", crate::document::next_id()).as_bytes());
        let id = format!("omatype:{:032x}", crate::typography::fingerprint(&bytes));
        fs::write(originals.join(format!("{role}.ttf")), &bytes).unwrap();
        crate::text::register_memory_font(&id, role, Arc::new(bytes)).unwrap();
        fonts.push(id);
    }
    fs::write(originals.join("paint.svg"), XML).unwrap();
    let rich = rich(&fonts);
    fs::write(originals.join("original.bin"), rich.sources[0].bytes()).unwrap();
    let expected = snapshot(&rich).unwrap();
    let empty = neutral();
    let mut cases = Vec::new();
    for mode in ["base-deleted", "write-withdrawn"] {
        let directory = root.join(mode);
        fs::create_dir(&directory).unwrap();
        let mut replica = Replica::new(
            "archive".into(),
            "a".repeat(64),
            snapshot(if mode == "base-deleted" {
                &rich
            } else {
                &empty
            })
            .unwrap(),
        );
        if mode == "write-withdrawn" {
            replica.record(&rich).unwrap();
            assert!(replica.undo());
        } else {
            replica.record(&neutral()).unwrap();
        }
        let mut doc = replica.document(&neutral()).unwrap();
        assert!(doc.layers.is_empty() && doc.sources.is_empty());
        assert_eq!(doc.color, Default::default());
        assert_eq!(replica.required_protocol(), 2);
        assert!(replica.document_for_protocol(&doc, 1).is_err());
        let pixels = pixel_cells(&replica);
        doc.collaboration = Some(replica);
        let before = serde_json::to_value(&doc).unwrap();
        let saved = directory.join("retained.oma");
        crate::project::save_to(&doc, &saved).unwrap();
        assert_eq!(serde_json::to_value(&doc).unwrap(), before);
        let json: Value = serde_json::from_slice(&fs::read(&saved).unwrap()).unwrap();
        assert_eq!(json["version"], 24);
        assert_eq!(json["svg_sources"].as_object().unwrap().len(), 1);
        assert_eq!(
            json["svg_sources"]
                .as_object()
                .unwrap()
                .values()
                .next()
                .unwrap(),
            XML
        );
        let decoded = crate::project::load_from(&saved).unwrap();
        assert_eq!(pixel_cells(decoded.collaboration.as_ref().unwrap()), pixels);
        let recovery = directory.join("retained.oma.swp");
        swap(doc.clone(), &recovery);
        swap(doc.clone(), &recovery);
        let archive = recovery.with_extension("swp-fonts").join(".omabrand/fonts");
        assert_eq!(
            fs::read_dir(&archive).unwrap().count(),
            2,
            "history-only faces were pruned"
        );
        let recovery_json: Value = serde_json::from_slice(&fs::read(&recovery).unwrap()).unwrap();
        assert_eq!(recovery_json["version"], 24);
        assert_eq!(recovery_json["svg_sources"].as_object().unwrap().len(), 1);
        fs::write(
            directory.join("expected.json"),
            serde_json::to_vec(&expected).unwrap(),
        )
        .unwrap();
        fs::write(
            directory.join("case.json"),
            serde_json::to_vec(&json!({"mode":mode,"fonts":fonts,"routes":ROUTES})).unwrap(),
        )
        .unwrap();
        cases.push((directory, doc));
    }
    fs::remove_dir_all(&originals).unwrap();
    for (directory, doc) in cases {
        for route in ROUTES {
            let output = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "collaboration::archive_tests::cold_archive_child",
                    "--ignored",
                    "--nocapture",
                ])
                .env(CHILD, &directory)
                .env("OMADESIGN_RETAINED_ARCHIVE_ROUTE", route)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "cold {route} failed: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            println!(
                "cold retained {route}: {}",
                String::from_utf8_lossy(&output.stdout)
            );
        }
        let saved = directory.join("retained.oma");
        let bytes = fs::read(&saved).unwrap();
        let mut missing = doc.clone();
        missing.collaboration.as_mut().unwrap().base.insert(
            "/@entities/4242/geom/Text/font".into(),
            Cell::Value(json!("omatype:missing-retained-face")),
        );
        assert!(crate::typography::validate_document_fonts(&missing).is_err());
        assert!(crate::project::save_to(&missing, &saved).is_err());
        assert_eq!(fs::read(&saved).unwrap(), bytes);
        let mut damaged: Value = serde_json::from_slice(&bytes).unwrap();
        damaged["svg_sources"] = json!({});
        assert!(
            crate::project::decode(&damaged.to_string())
                .unwrap_err()
                .contains("Missing vector source")
        );
        let recovery = directory.join("retained.oma.swp");
        swap(neutral(), &recovery);
        assert!(
            !recovery.with_extension("swp-fonts").exists(),
            "released history kept obsolete recovery fonts"
        );
    }
    println!("retained archive fixtures: {}", root.display());
}

#[test]
#[ignore = "spawned by retained_archives with an isolated cold-process fixture"]
fn cold_archive_child() {
    let directory = PathBuf::from(std::env::var_os(CHILD).expect("parent archive fixture"));
    let case: Value =
        serde_json::from_slice(&fs::read(directory.join("case.json")).unwrap()).unwrap();
    for font in case["fonts"].as_array().unwrap() {
        assert!(
            !crate::text::project_font_loaded(font.as_str().unwrap()),
            "font cache was not cold"
        );
    }
    let route = std::env::var("OMADESIGN_RETAINED_ARCHIVE_ROUTE").unwrap();
    let doc = match route.as_str() {
        "save" => crate::project::load_from(&directory.join("retained.oma")).unwrap(),
        "recovery" => {
            crate::project::load_swap(&directory.join("retained.oma.swp"))
                .unwrap()
                .doc
        }
        "preview-save" => {
            crate::brand::load_preview_document(&directory.join("retained.oma"), false).unwrap()
        }
        "preview-recovery" => {
            crate::brand::load_preview_document(&directory.join("retained.oma.swp"), true).unwrap()
        }
        _ => panic!("Unknown isolated archive route"),
    };
    assert!(doc.layers.is_empty() && doc.sources.is_empty());
    for font in case["fonts"].as_array().unwrap() {
        assert!(crate::text::project_font_loaded(font.as_str().unwrap()));
    }
    let mut replica = doc.collaboration.clone().unwrap();
    assert_eq!(replica.required_protocol(), 2);
    let mut remote = Replica::new(
        replica.session.clone(),
        "b".repeat(64),
        replica.base.clone(),
    );
    remote.receive(replica.changes.clone()).unwrap();
    if case["mode"] == "base-deleted" {
        assert!(replica.undo());
    } else {
        assert!(replica.redo());
    }
    remote.receive(replica.changes.clone()).unwrap();
    let expected: Snapshot =
        serde_json::from_slice(&fs::read(directory.join("expected.json")).unwrap()).unwrap();
    let restored = replica.document(&doc).unwrap();
    assert_eq!(snapshot(&restored).unwrap(), expected);
    assert_eq!(snapshot(&remote.document(&doc).unwrap()).unwrap(), expected);
    assert_eq!(
        restored.sources[0].bytes(),
        b"\0\xff<definitions>opaque original, not XML</definitions>"
    );
    let Geom::Text(run) = &restored.layers.last().unwrap().kind.shapes().unwrap()[0].geom else {
        panic!("live text")
    };
    assert!(!crate::text::shape(run).is_empty());
    let mut saved = restored;
    saved.collaboration = Some(replica);
    let path = directory.join(format!("restored-{route}.oma"));
    crate::project::save_to(&saved, &path).unwrap();
    let reopened = crate::project::load_from(&path).unwrap();
    native_tests::assert_native_equal(&saved, &reopened);
    assert_eq!(snapshot(&reopened).unwrap(), expected);
    println!(
        "cold {route}: exact native bits/ICC/intent/display, span font, pattern/filter, opaque original and remote restore passed"
    );
}

#[test]
fn retained_resource_subtrees_share_content_and_preserve_inline_and_opaque_data() {
    let mut doc = neutral();
    let mut replica = Replica::new("resources".into(), "a".repeat(64), snapshot(&doc).unwrap());
    replica.base.insert("/@entities/42".into(), Cell::Value(json!({
        "style":{"fill":{"Pattern":{"definitions":XML}}},
        "geom":{"Text":{"font":"omatype:run","spans":[{"font":"omatype:span","fill":{"Pattern":{"definitions":XML}}}],
            "svg_text":{"definitions":"inline definitions, not a pooled Pattern"}}},
        "svg_filter":{"definitions":XML,"original_definitions":XML}
    })));
    assert_eq!(
        resources::fonts(&replica),
        BTreeSet::from(["omatype:run".into(), "omatype:span".into()])
    );
    doc.collaboration = Some(replica);
    let before = serde_json::to_value(&doc).unwrap();
    let pool = crate::svg_source::pack_sources(&mut doc);
    assert_eq!(pool.len(), 1);
    assert!(
        serde_json::to_string(&doc)
            .unwrap()
            .contains("inline definitions, not a pooled Pattern")
    );
    crate::svg_source::unpack_sources(&mut doc, &pool).unwrap();
    assert_eq!(serde_json::to_value(&doc).unwrap(), before);
}

#[test]
fn retained_source_integrity_and_future_project_versions_refuse_before_loss() {
    let mut doc = neutral();
    let source =
        crate::source_attachment::SourceAttachment::new("opaque", "binary", vec![0, 255, 42])
            .unwrap();
    let mut replica = Replica::new("sources".into(), "a".repeat(64), snapshot(&doc).unwrap());
    replica.base.insert(
        "/sources".into(),
        Cell::Value(serde_json::to_value(vec![source]).unwrap()),
    );
    doc.collaboration = Some(replica);
    let encoded = crate::project::encode(&doc).unwrap();
    let mut value: Value = serde_json::from_str(&encoded).unwrap();
    for version in [1, 14, 17, 18, 19, 20, 21, 22, 23] {
        value["version"] = version.into();
        assert!(
            crate::project::decode(&value.to_string())
                .unwrap_err()
                .contains("requires version 24")
        );
    }
    value["version"] = 24.into();
    assert!(crate::project::decode(&value.to_string()).is_ok());
    value["version"] = 25.into();
    value["doc"]["collaboration"] = json!({"future_model_that_must_not_be_typed":true});
    assert!(
        crate::project::decode(&value.to_string())
            .unwrap_err()
            .contains("Unsupported")
    );
    let Cell::Value(sources) = doc
        .collaboration
        .as_mut()
        .unwrap()
        .base
        .get_mut("/sources")
        .unwrap()
    else {
        unreachable!()
    };
    sources[0]["data"] = json!("AA==");
    assert!(
        crate::project::encode(&doc)
            .unwrap_err()
            .contains("integrity")
    );
    let mut value: Value = serde_json::from_str(&encoded).unwrap();
    value["doc"]["collaboration"]["base"]["/sources"]["Value"][0]["data"] = json!("AA==");
    assert!(
        crate::project::decode(&value.to_string())
            .unwrap_err()
            .contains("integrity")
    );
}

#[test]
fn withdrawn_pixels_remain_encoded_and_legacy_inline_recovery_still_opens() {
    let mut doc = neutral();
    let mut replica = Replica::new("encoded".into(), "a".repeat(64), snapshot(&doc).unwrap());
    replica.record(&native_tests::fixture()).unwrap();
    assert!(replica.undo());
    let pixel = replica
        .retained_cells_mut()
        .find_map(|(_, cell)| {
            if let Cell::Pixels { data, .. } = cell {
                Some(data)
            } else {
                None
            }
        })
        .unwrap();
    *pixel = "encoded history must not be decoded until it wins".into();
    doc.collaboration = Some(replica);
    let encoded = crate::project::encode(&doc).unwrap();
    let reopened = crate::project::decode(&encoded).unwrap();
    let mut replica = reopened.collaboration.clone().unwrap();
    let before = serde_json::to_value(&reopened).unwrap();
    assert!(replica.document(&reopened).unwrap().layers.is_empty());
    assert!(replica.redo());
    assert!(replica.document(&reopened).is_err());
    assert_eq!(serde_json::to_value(&reopened).unwrap(), before);

    let root = std::env::current_dir()
        .unwrap()
        .join(".artifacts")
        .join(format!("live-legacy-swap-{}", crate::document::next_id()));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("legacy.oma.swp");
    let meta = crate::project::SwapMeta {
        id: "legacy".into(),
        original: None,
        name: "Legacy inline".into(),
        saved_at: 0,
        doc: neutral(),
    };
    fs::write(&path, serde_json::to_vec(&meta).unwrap()).unwrap();
    assert!(
        crate::project::load_swap(&path)
            .unwrap()
            .doc
            .collaboration
            .is_none()
    );
    let mut future = serde_json::to_value(&meta).unwrap();
    future["version"] = json!(25);
    future["doc"]["collaboration"] = json!({"future_model_that_must_not_be_typed":true});
    let future_bytes = serde_json::to_vec(&future).unwrap();
    fs::write(&path, &future_bytes).unwrap();
    assert!(
        crate::project::load_swap(&path)
            .err()
            .unwrap()
            .contains("Unsupported")
    );
    assert!(
        crate::brand::load_preview_document(&path, true)
            .unwrap_err()
            .contains("Unsupported")
    );
    assert_eq!(fs::read(&path).unwrap(), future_bytes);
    fs::remove_dir_all(root).unwrap();
}
