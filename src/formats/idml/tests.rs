use super::*;
use crate::{document::Document, geom::Geom};
mod review;
mod route_review;
const FIXTURE: &[u8] = include_bytes!("../../../tests/assets/idml/publishing.idml");
fn texts(doc: &Document) -> Vec<&crate::geom::TypeRun> {
    doc.layers
        .iter()
        .filter_map(|l| l.kind.shapes())
        .flatten()
        .filter_map(|s| {
            if let Geom::Text(t) = &s.geom {
                Some(t)
            } else {
                None
            }
        })
        .collect()
}
#[test]
fn independently_authored_styled_threaded_publishing_package() {
    let (doc, notes) = read(FIXTURE, "Publishing").unwrap();
    assert_eq!(doc.artboards.len(), 2);
    assert_eq!(doc.artboards[0].size, Pt::new(300., 400.));
    assert_eq!(doc.artboards[1].origin, Pt::new(300., 0.));
    assert_eq!(doc.publishing.bleed, [9., 12., 15., 18.]);
    assert_eq!(doc.publishing.masters.len(), 1);
    assert_eq!(doc.publishing.page_masters.len(), 2);
    assert_eq!(doc.publishing.inherited_layers.len(), 2);
    assert_eq!(doc.publishing.links.len(), 2);
    assert_eq!(
        doc.layers
            .iter()
            .filter(|l| l.kind.pixels().is_some())
            .count(),
        1
    );
    let text = texts(&doc);
    assert_eq!(text.len(), 2);
    let head = text
        .iter()
        .find(|t| t.thread.as_ref().unwrap().prev.is_none())
        .unwrap();
    assert!(head.content.starts_with("Café & typography"));
    assert!(head.content.ends_with("\nSecond paragraph."));
    assert!(!head.content.contains("Never flattened"));
    assert_eq!(head.paragraphs[1].align, crate::geom::TextAlign::Center);
    assert_eq!(head.spans[0].tracking, Some(90.));
    assert_eq!(head.frame.as_ref().unwrap().inset, [3., 4., 5., 6.]);
    assert!(
        text[1]
            .layout
            .as_ref()
            .is_some_and(|l| l.visible_start > 0 && l.visible_end > l.visible_start)
    );
    for needle in ["Table", "Note", "missing"] {
        assert!(notes.iter().any(|n| n.contains(needle)), "{notes:?}");
    }
}
#[test]
fn native_reopen_and_idml_roundtrip_preserve_editable_story_and_publishing() {
    let (mut doc, notes) = read(FIXTURE, "Publishing").unwrap();
    doc.import_notes = notes;
    let head = doc
        .layers
        .iter_mut()
        .filter_map(|l| l.kind.shapes_mut())
        .flatten()
        .find_map(|s| {
            if let Geom::Text(t) = &mut s.geom {
                if t.thread.as_ref().unwrap().prev.is_none() {
                    Some(t)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap();
    head.replace_text(0, 4, "Edited", None);
    let source = head.content.clone();
    let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    assert_eq!(reopened.publishing, doc.publishing);
    assert_eq!(reopened.import_notes, doc.import_notes);
    let (bytes, _) = write(&reopened).unwrap();
    let (round, notes) = read(&bytes, "Again").unwrap();
    assert_eq!(round.artboards.len(), 2);
    assert_eq!(round.publishing.bleed, [9., 12., 15., 18.]);
    assert_eq!(round.publishing.masters, doc.publishing.masters);
    assert_eq!(
        round.publishing.links.values().collect::<Vec<_>>(),
        doc.publishing.links.values().collect::<Vec<_>>()
    );
    let head = texts(&round)
        .into_iter()
        .find(|t| t.thread.as_ref().unwrap().prev.is_none())
        .unwrap();
    assert_eq!(head.content, source);
    assert_eq!(head.spans[0].tracking, Some(90.));
    assert_eq!(head.frame.as_ref().unwrap().inset, [3., 4., 5., 6.]);
    assert!(notes.iter().any(|n| n.contains("missing")));
}
#[test]
fn rejects_corrupt_zip_paths_threads_and_style_cycles() {
    assert!(read(b"not a zip", "broken").is_err());
    let replace = |from: &str, to: &str| {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(FIXTURE)).unwrap();
        let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for i in 0..z.len() {
            let mut f = z.by_index(i).unwrap();
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut f, &mut bytes).unwrap();
            let value = if f.name().ends_with(".xml") {
                String::from_utf8(bytes)
                    .unwrap()
                    .replace(from, to)
                    .into_bytes()
            } else {
                bytes
            };
            out.start_file(
                f.name(),
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
            std::io::Write::write_all(&mut out, &value).unwrap();
        }
        out.finish().unwrap().into_inner()
    };
    assert!(
        read(
            &replace("NextTextFrame=\"frame2\"", "NextTextFrame=\"absent\""),
            "missing"
        )
        .unwrap_err()
        .contains("Missing IDML threaded")
    );
    assert!(
        read(
            &replace("PreviousTextFrame=\"n\"", "PreviousTextFrame=\"frame2\""),
            "cycle"
        )
        .is_err()
    );
    assert!(
        read(
            &replace(
                "<BasedOn type=\"object\">ParagraphStyle/Body",
                "<BasedOn type=\"object\">ParagraphStyle/Center"
            ),
            "cycle"
        )
        .unwrap_err()
        .contains("style inheritance")
    );
}
#[test]
fn emits_standard_zip_xml_without_native_sidecar() {
    let (doc, _) = read(FIXTURE, "Publishing").unwrap();
    let (bytes, _) = write(&doc).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let first = zip.by_index(0).unwrap();
    assert_eq!(first.name(), "mimetype");
    assert_eq!(first.compression(), zip::CompressionMethod::Stored);
    drop(first);
    for i in 1..zip.len() {
        let mut f = zip.by_index(i).unwrap();
        assert!(f.name().ends_with(".xml"));
        let mut text = String::new();
        std::io::Read::read_to_string(&mut f, &mut text).unwrap();
        let xml = roxmltree::Document::parse(&text).unwrap();
        assert!(!text.contains("omadesign:Document"));
        if f.name().starts_with("Stories/") {
            assert!(
                xml.descendants()
                    .any(|n| n.has_tag_name("ParagraphStyleRange"))
            );
        }
    }
}

#[test]
fn master_preview_edits_are_never_silently_discarded() {
    let (mut doc, _) = read(FIXTURE, "Publishing").unwrap();
    let id = doc.publishing.inherited_layers[0];
    doc.layers.iter_mut().find(|l| l.id == id).unwrap().opacity = 0.5;
    assert!(
        write(&doc)
            .unwrap_err()
            .contains("master preview was edited")
    );
}

#[test]
fn styled_font_ranges_shape_and_edit_with_native_character_indices() {
    use crate::geom::{CharSpan, TypeRun};
    let font = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/assets/fonts/EBGaramond.ttf"
    );
    let mut run = TypeRun {
        content: "Café bold".into(),
        font: font.into(),
        px: 20.,
        spans: vec![CharSpan {
            start: 5,
            end: 9,
            font: Some(font.into()),
            publishing_style: Some("CharacterStyle/Emphasis".into()),
            hscale: Some(140.),
            ..Default::default()
        }],
        ..Default::default()
    };
    let before = crate::text::compose(&run);
    assert!(
        before[0]
            .glyphs
            .iter()
            .all(|g| !crate::text::glyph_contours(&run, g).is_empty() || g.is_space)
    );
    run.replace_text(0, 4, "Résumé", None);
    assert_eq!(run.spans[0].start, 7);
    assert_eq!(run.spans[0].end, 11);
    assert_eq!(
        run.spans[0].publishing_style.as_deref(),
        Some("CharacterStyle/Emphasis")
    );
}

#[test]
fn idml_open_native_save_reopen_and_both_export_routes() {
    use crate::{
        export::{Format, Settings, Source},
        import::Imported,
    };
    assert_eq!(
        crate::import::classify(std::path::Path::new("book.IDML")),
        "idml"
    );
    let Imported::Document(mut doc) = crate::import::open_any(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/assets/idml/publishing.idml"
    )))
    .unwrap() else {
        panic!("expected editable document")
    };
    let head = doc
        .layers
        .iter_mut()
        .filter_map(|l| l.kind.shapes_mut())
        .flatten()
        .find_map(|s| {
            if let Geom::Text(t) = &mut s.geom {
                if t.thread.as_ref().unwrap().prev.is_none() {
                    Some(t)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap();
    head.replace_text(0, 4, "Edited café", None);
    let keep = std::env::var_os("OMADESIGN_IDML_RECEIPTS");
    let dir = keep
        .clone()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
                ".artifacts/idml-test-{}",
                crate::document::next_id()
            ))
        });
    assert!(dir.starts_with("/home"));
    std::fs::create_dir_all(&dir).unwrap();
    let oma = dir.join("publishing.oma");
    crate::formats::cli::export(&doc, &oma).unwrap();
    let reopened = crate::formats::cli::document(&oma).unwrap();
    assert_eq!(reopened.publishing, doc.publishing);
    assert!(reopened.import_notes.iter().any(|n| n.contains("Table")));
    let output = dir.join("publishing.idml");
    crate::formats::cli::export(&reopened, &output).unwrap();
    let round = crate::formats::cli::document(&output).unwrap();
    assert_eq!(round.artboards.len(), 2);
    assert_eq!(round.publishing.bleed, [9., 12., 15., 18.]);
    assert!(
        texts(&round)
            .iter()
            .any(|t| t.content.starts_with("Edited café"))
    );
    let (bytes, _) = crate::export::encode(
        &Source::Document(reopened),
        Settings {
            format: Format::Idml,
            ..Default::default()
        },
        &crate::ml::NoProgress,
    )
    .unwrap();
    assert_eq!(read(&bytes, "Dialog export").unwrap().0.artboards.len(), 2);
    std::fs::write(dir.join("dialog.idml"), bytes).unwrap();
    if keep.is_none() {
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn idml_story_color_does_not_become_an_opaque_text_frame() {
    let (doc, _) = read(FIXTURE, "Publishing").unwrap();
    let color = crate::color::Rgba::rgb(12, 50, 90);
    let head=doc.layers.iter().filter_map(|l|l.kind.shapes()).flatten().find(|s|matches!(&s.geom,Geom::Text(t) if t.thread.as_ref().is_some_and(|t|t.prev.is_none()))).unwrap();
    assert_eq!(head.style.fill, crate::document::Fill::Solid(color));
    let (bytes, _) = write(&doc).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).unwrap();
        if !f.name().starts_with("Spreads/") {
            continue;
        }
        let mut xml = String::new();
        std::io::Read::read_to_string(&mut f, &mut xml).unwrap();
        let tree = roxmltree::Document::parse(&xml).unwrap();
        for frame in tree.descendants().filter(|n| n.has_tag_name("TextFrame")) {
            assert_eq!(frame.attribute("FillColor"), Some("Swatch/None"));
        }
        assert!(
            tree.descendants()
                .filter(|n| n.has_tag_name("Image"))
                .all(|n| n.descendants().any(|n| n.has_tag_name("GraphicBounds")))
        );
    }
    let (round, _) = read(&bytes, "Again").unwrap();
    assert!(
        round
            .layers
            .iter()
            .filter_map(|l| l.kind.shapes())
            .flatten()
            .any(|s| matches!(&s.geom, Geom::Text(_))
                && s.style.fill == crate::document::Fill::Solid(color))
    );
}

#[test]
fn idml_native_bleed_edits_override_imported_asymmetric_bleed() {
    let (mut doc, _) = read(FIXTURE, "Publishing").unwrap();
    doc.bleed = 24.;
    let (bytes, _) = write(&doc).unwrap();
    let (round, _) = read(&bytes, "Edited bleed").unwrap();
    assert_eq!(round.publishing.bleed, [24.; 4]);
}

#[test]
fn idml_rejects_unsafe_archive_paths_and_a_directory_before_mimetype() {
    use std::io::{Cursor, Write};
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    w.start_file("mimetype", options).unwrap();
    w.write_all(MIMETYPE.as_bytes()).unwrap();
    w.start_file("../designmap.xml", options).unwrap();
    w.write_all(b"<Document/>").unwrap();
    assert!(
        read(&w.finish().unwrap().into_inner(), "unsafe")
            .unwrap_err()
            .contains("Unsafe IDML package path")
    );
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    w.add_directory("folder/", options).unwrap();
    w.start_file("mimetype", options).unwrap();
    w.write_all(MIMETYPE.as_bytes()).unwrap();
    assert!(
        read(&w.finish().unwrap().into_inner(), "directory")
            .unwrap_err()
            .contains("must start with stored mimetype")
    );
}
