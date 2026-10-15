use super::*;
use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
};

fn fixture(edit: impl FnOnce(&mut BTreeMap<String, Vec<u8>>)) -> Vec<u8> {
    let mut parts = zip_parts(FIXTURE);
    edit(&mut parts);
    pack_parts(&parts)
}
pub(super) fn pack_parts(parts: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for name in std::iter::once("mimetype").chain(
        parts
            .keys()
            .filter(|n| n.as_str() != "mimetype")
            .map(String::as_str),
    ) {
        writer
            .start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(&parts[name]).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
fn edit_xml(
    parts: &mut BTreeMap<String, Vec<u8>>,
    name: &str,
    edit: impl FnOnce(String) -> String,
) {
    let xml = String::from_utf8(parts[name].clone()).unwrap();
    parts.insert(name.into(), edit(xml).into_bytes());
}
pub(super) fn master_fixture() -> Vec<u8> {
    fixture(|parts| {
        edit_xml(parts, "designmap.xml", |s| {
            s.replace(
                "<Layer Self=\"layer1\"",
                "<Layer Self=\"u10\" Name=\"Master art\" Visible=\"true\"/><Layer Self=\"layer1\"",
            )
        });
        edit_xml(parts, "MasterSpreads/MasterSpread_A.xml", |s| {
            s.replace("Self=\"masterRule\"","Self=\"masterRule\" ItemLayer=\"u10\"").replace("</MasterSpread>","<Rectangle Self=\"masterLogo\" ItemLayer=\"u10\" ItemTransform=\"1 0 0 1 0 0\" GeometricBounds=\"-160 20 -140 60\"><Image Self=\"masterImage\" ItemTransform=\"1 0 0 1 0 0\"><Link Self=\"masterLink\" LinkResourceURI=\"Links/logo.png\"/></Image></Rectangle></MasterSpread>")
        });
        parts.insert("Links/logo.png".into(), parts["Links/mark.png"].clone());
    })
}
pub(super) fn zip_parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut parts = BTreeMap::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).unwrap();
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes).unwrap();
        parts.insert(f.name().into(), bytes);
    }
    parts
}
fn thread_fixture() -> Vec<u8> {
    fixture(|parts| {
        let text = "word ".repeat(64);
        let story = format!(
            "<Story Self=\"flow\"><ParagraphStyleRange><CharacterStyleRange PointSize=\"8\"><Content>{text}</Content></CharacterStyleRange></ParagraphStyleRange></Story>"
        );
        parts.insert(
            "Stories/Story_flow.xml".into(),
            package("Story", &story).into_bytes(),
        );
        let mut spread="<Spread Self=\"manyFrames\" ItemTransform=\"1 0 0 1 0 0\"><Page Self=\"page1\" Name=\"1\" GeometricBounds=\"0 0 400 300\" ItemTransform=\"1 0 0 1 0 0\"/>".to_owned();
        for i in 0..16 {
            let prev = if i == 0 {
                "n".into()
            } else {
                format!("f{}", i - 1)
            };
            let next = if i == 15 {
                "n".into()
            } else {
                format!("f{}", i + 1)
            };
            spread += &format!(
                "<TextFrame Self=\"f{i}\" ParentStory=\"flow\" PreviousTextFrame=\"{prev}\" NextTextFrame=\"{next}\" ItemTransform=\"1 0 0 1 10 30\" GeometricBounds=\"0 0 30 100\"/>"
            );
        }
        spread.push_str("</Spread>");
        parts.insert(
            "Spreads/Spread_pair.xml".into(),
            package("Spread", &spread).into_bytes(),
        );
    })
}

#[test]
fn review_thread_followers_never_allocate_story_buffers() {
    let (doc, _) = read(&thread_fixture(), "Many frames").unwrap();
    let runs = texts(&doc);
    assert_eq!(runs.len(), 16);
    let followers = runs
        .iter()
        .filter(|t| t.thread.as_ref().is_some_and(|t| t.prev.is_some()))
        .collect::<Vec<_>>();
    assert_eq!(followers.len(), 15);
    let reserved = followers
        .iter()
        .map(|t| {
            t.content.capacity()
                + t.spans.capacity() * std::mem::size_of::<crate::geom::CharSpan>()
                + t.paragraphs.capacity() * std::mem::size_of::<crate::geom::ParagraphStyle>()
        })
        .sum::<usize>();
    assert_eq!(reserved, 0, "followers retain duplicated story allocations");
}

#[test]
fn round2_external_master_bytes_survive_native_reopen_without_link_reader() {
    for properties in [
        "",
        "<Properties/>",
        "<Properties><GraphicBounds Left=\"20\"/></Properties>",
    ] {
        let mut parts = zip_parts(&master_fixture());
        let expected = parts.remove("Links/logo.png").unwrap();
        edit_xml(&mut parts, "MasterSpreads/MasterSpread_A.xml", |s| {
            s.replace(
                "<Link Self=\"masterLink\"",
                &format!("{properties}<Link StoredState=\"Normal\" Self=\"masterLink\""),
            )
        });
        let (doc, _) = read_with_links(&pack_parts(&parts), "External master", &|uri| {
            (uri == "Links/logo.png").then(|| expected.clone())
        })
        .unwrap();
        let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
        let (output, _) = write(&reopened).unwrap();
        let (round, notes) = read(&output, "Detached master").unwrap();
        assert_eq!(
            round
                .layers
                .iter()
                .filter(|l| l.kind.pixels().is_some())
                .count(),
            3
        );
        assert!(!notes.iter().any(|n| n.contains("missing: Links/logo.png")));
        let master = roxmltree::Document::parse(&round.publishing.masters[0].xml).unwrap();
        let image = master
            .descendants()
            .find(|n| n.attribute("Self") == Some("masterImage"))
            .unwrap();
        let contents = image
            .descendants()
            .find(|n| n.has_tag_name("Contents"))
            .unwrap();
        assert!(
            image
                .descendants()
                .any(|n| n.attribute("StoredState") == Some("Embedded"))
        );
        assert_eq!(
            base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                contents.text().unwrap()
            )
            .unwrap(),
            expected
        );
        assert!(
            image
                .descendants()
                .any(|n| n.attribute("LinkResourceURI") == Some("Links/logo.png"))
        );
    }
}

#[test]
fn round2_decoded_rasters_keep_visibility_and_source_lock() {
    for (visible, locked) in [(false, false), (true, true), (false, true), (true, false)] {
        let (mut doc, _) = read(FIXTURE, "Raster state").unwrap();
        let raster = doc
            .layers
            .iter_mut()
            .find(|l| l.kind.pixels().is_some())
            .unwrap();
        raster.visible = visible;
        raster.locked = locked;
        let (output, _) = write(&doc).unwrap();
        let (round, _) = read(&output, "Raster state round trip").unwrap();
        let native = crate::project::decode(&crate::project::encode(&round).unwrap()).unwrap();
        let raster = native
            .layers
            .iter()
            .find(|l| l.kind.pixels().is_some())
            .unwrap();
        assert_eq!((raster.visible, raster.locked), (visible, locked));
        assert_eq!(native.publishing.links[&raster.id], "Links/mark.png");
    }
}
#[test]
fn review_master_item_layers_resolve_after_native_reopen() {
    let (doc, _) = read(&master_fixture(), "Master layer").unwrap();
    let doc = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    let (bytes, _) = write(&doc).unwrap();
    let parts = zip_parts(&bytes);
    let dm =
        roxmltree::Document::parse(std::str::from_utf8(&parts["designmap.xml"]).unwrap()).unwrap();
    for (name, bytes) in parts
        .iter()
        .filter(|(n, _)| n.starts_with("MasterSpreads/"))
    {
        let xml = roxmltree::Document::parse(std::str::from_utf8(bytes).unwrap()).unwrap();
        for layer in xml.descendants().filter_map(|n| n.attribute("ItemLayer")) {
            assert!(
                dm.descendants()
                    .any(|n| n.has_tag_name("Layer") && n.attribute("Self") == Some(layer)),
                "{name}: dangling master layer {layer}"
            );
        }
    }
}

#[test]
fn review_source_layer_collision_remaps_only_generated_layers() {
    let (mut doc, _) = read(&master_fixture(), "Layer collision").unwrap();
    let id = doc
        .layers
        .iter()
        .find(|l| !doc.publishing.inherited_layers.contains(&l.id))
        .unwrap()
        .id;
    let collision = format!("layer{id}");
    let layer = doc
        .publishing
        .master_layers
        .remove("u10")
        .unwrap()
        .replace("u10", &collision);
    doc.publishing
        .master_layers
        .insert(collision.clone(), layer);
    for master in &mut doc.publishing.masters {
        master.xml = master.xml.replace("u10", &collision);
    }
    let (bytes, _) = write(&doc).unwrap();
    let parts = zip_parts(&bytes);
    let dm =
        roxmltree::Document::parse(std::str::from_utf8(&parts["designmap.xml"]).unwrap()).unwrap();
    let ids = dm
        .descendants()
        .filter(|n| n.has_tag_name("Layer"))
        .filter_map(|n| n.attribute("Self"))
        .collect::<Vec<_>>();
    assert_eq!(
        ids.iter()
            .copied()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        ids.len()
    );
    assert!(ids.contains(&collision.as_str()));
    for (name, bytes) in parts
        .iter()
        .filter(|(n, _)| n.starts_with("Spreads/") || n.starts_with("MasterSpreads/"))
    {
        let tree = roxmltree::Document::parse(std::str::from_utf8(bytes).unwrap()).unwrap();
        for layer in tree.descendants().filter_map(|n| n.attribute("ItemLayer")) {
            assert!(ids.contains(&layer), "{name}: {layer}");
        }
    }
}
#[test]
fn review_owned_master_image_payload_survives_native_reopen() {
    let source = master_fixture();
    let expected = zip_parts(&source)["Links/logo.png"].clone();
    let (mut doc, notes) = read(&source, "Master image").unwrap();
    doc.import_notes = notes;
    let keep = std::env::var_os("OMADESIGN_IDML_RECEIPTS");
    let directory = keep
        .clone()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
                ".artifacts/idml-master-test-{}",
                crate::document::next_id()
            ))
        });
    assert!(directory.starts_with("/home"));
    std::fs::create_dir_all(&directory).unwrap();
    let native = directory.join("master-reopened.oma");
    crate::project::save_to(&doc, &native).unwrap();
    let doc = crate::project::load_from(&native).unwrap();
    for note in [
        "IDML Table is unsupported and was omitted; retain the source document.",
        "IDML Note is unsupported and was omitted; retain the source document.",
    ] {
        assert!(doc.import_notes.iter().any(|n| n == note));
    }
    let (bytes, _) = write(&doc).unwrap();
    let parts = zip_parts(&bytes);
    assert_eq!(
        parts.get("Links/logo.png"),
        Some(&expected),
        "package-owned master image payload disappeared"
    );
    let (round, _) = read(&bytes, "Reimported master image").unwrap();
    assert_eq!(round.publishing.master_assets["Links/logo.png"], expected);
    assert_eq!(
        round
            .layers
            .iter()
            .filter(|l| l.kind.pixels().is_some())
            .count(),
        3
    );
    if keep.is_some() {
        std::fs::write(directory.join("master-source.idml"), &source).unwrap();
        std::fs::write(directory.join("master-reopened.idml"), &bytes).unwrap();
    } else {
        std::fs::remove_dir_all(directory).unwrap();
    }
}

#[test]
fn review_thread_cache_budget_rejects_small_over_budget_fixture() {
    let source = thread_fixture();
    let error =
        super::super::read::read_with_budget(&source, "Bounded frames", &|_| None, 16 * 1024)
            .unwrap_err();
    assert!(error.contains("allocation budget"), "{error}");
}

#[test]
fn review_missing_preview_guard_handles_legacy_metadata_and_removed_page() {
    let (mut doc, _) = read(FIXTURE, "Legacy preview").unwrap();
    let id = doc.publishing.inherited_layers[0];
    let page = doc.publishing.inherited_pages[&id];
    doc.layers.retain(|l| l.id != id);
    let mut legacy = doc.clone();
    legacy.publishing.inherited_pages.clear();
    assert!(write(&legacy).unwrap_err().contains("deleted"));
    doc.artboards.retain(|a| a.id != page);
    assert!(
        write(&doc).is_ok(),
        "removing a page must remove its preview obligation"
    );
}

#[test]
fn review_all_justified_final_lines_preserve_full_paragraph_style() {
    for (value, last) in [
        ("LeftJustified", crate::geom::LastLine::Start),
        ("CenterJustified", crate::geom::LastLine::Center),
        ("RightJustified", crate::geom::LastLine::End),
        ("FullyJustified", crate::geom::LastLine::Justify),
    ] {
        let source = fixture(|parts| {
            edit_xml(parts, "Resources/Styles.xml", |s| {
                s.replace(
                    "Justification=\"CenterAlign\"",
                    &format!("Justification=\"{value}\""),
                )
            })
        });
        let (doc, _) = read(&source, value).unwrap();
        let original = texts(&doc)
            .into_iter()
            .find(|t| !t.paragraphs.is_empty())
            .unwrap()
            .paragraphs
            .clone();
        assert_eq!(original[1].align, crate::geom::TextAlign::Justify { last });
        let (bytes, _) = write(&doc).unwrap();
        let (round, _) = read(&bytes, value).unwrap();
        assert_eq!(
            texts(&round)
                .into_iter()
                .find(|t| !t.paragraphs.is_empty())
                .unwrap()
                .paragraphs,
            original,
            "{value}"
        );
    }
}

#[test]
fn review_missing_master_layer_and_unsafe_asset_are_rejected() {
    let (mut doc, _) = read(&master_fixture(), "Invalid dependencies").unwrap();
    doc.publishing
        .master_assets
        .insert("../escape.png".into(), vec![1]);
    assert!(write(&doc).unwrap_err().contains("unsafe"));
    doc.publishing.master_assets.remove("../escape.png");
    doc.publishing.master_layers.clear();
    assert!(
        write(&doc)
            .unwrap_err()
            .contains("Missing retained master layer")
    );
}
#[test]
fn review_deleted_assigned_master_preview_rejects_export() {
    let (mut doc, _) = read(FIXTURE, "Deleted master preview").unwrap();
    let id = doc.publishing.inherited_layers[0];
    doc.layers.retain(|l| l.id != id);
    assert!(
        write(&doc).is_err(),
        "export resurrects a deleted master preview"
    );
}
#[test]
fn review_fully_justified_named_paragraph_roundtrips() {
    let source = fixture(|parts| {
        edit_xml(parts, "Resources/Styles.xml", |s| {
            s.replace(
                "Justification=\"CenterAlign\"",
                "Justification=\"FullyJustified\"",
            )
        })
    });
    let (doc, _) = read(&source, "Fully justified").unwrap();
    let original = texts(&doc)
        .into_iter()
        .find(|t| t.thread.as_ref().unwrap().prev.is_none())
        .unwrap()
        .paragraphs
        .clone();
    assert!(matches!(
        original[1].align,
        crate::geom::TextAlign::Justify {
            last: crate::geom::LastLine::Justify
        }
    ));
    let (bytes, _) = write(&doc).unwrap();
    let (round, _) = read(&bytes, "Round trip").unwrap();
    let paragraphs = &texts(&round)
        .into_iter()
        .find(|t| t.thread.as_ref().unwrap().prev.is_none())
        .unwrap()
        .paragraphs;
    assert_eq!(
        paragraphs, &original,
        "supported paragraph style changed on export"
    );
}
