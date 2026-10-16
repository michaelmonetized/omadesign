use super::{
    review::{master_fixture, pack_parts, zip_parts},
    *,
};
use crate::{
    color::Rgba,
    document::Fill,
    geom::{Leading, ParagraphStyle, TextAlign, TypeRun},
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

const BLUE: [u8; 4] = [12, 50, 90, 255];
const RED: [u8; 4] = [200, 32, 55, 255];
const GREEN: [u8; 4] = [20, 135, 65, 255];
const AUTHORED: &str = "Café & typography keep two real font families, sizes and colors across an affine threaded page. More words keep the first frame full.\nSecond paragraph keeps its named style and green paint while the story flows in order onto the next page.";

/// Resolve a named installed family without accepting the system fallback.
/// Args: exact family/style label. Returns its installed file path or fails the fixture.
fn face(name: &str) -> String {
    crate::text::fonts()
        .iter()
        .find(|font| font.name == name)
        .unwrap_or_else(|| panic!("required named font is absent: {name}"))
        .path
        .to_string_lossy()
        .into_owned()
}

/// Inspect the same cached bytes used by the shared composer.
/// Args: installed path or portable font ID. Returns the actual family and face names.
fn names(font: &str) -> (String, String) {
    let bytes = crate::text::font_bytes_for(&TypeRun {
        font: font.into(),
        ..Default::default()
    })
    .unwrap();
    let face = rustybuzz::ttf_parser::Face::parse(&bytes, 0).unwrap();
    let name = |id| {
        face.names()
            .into_iter()
            .filter(|name| name.name_id == id)
            .find_map(|name| name.to_string())
            .unwrap()
    };
    (name(1), name(2))
}

/// Address an installed face through the existing portable font archive API.
/// Args: independently resolved installed path. Returns a content ID registered in the shared cache.
fn portable(font: &str) -> String {
    let bytes = crate::text::font_bytes_for(&TypeRun {
        font: font.into(),
        ..Default::default()
    })
    .unwrap();
    let id = format!("omatype:{:032x}", crate::typography::fingerprint(&bytes));
    crate::text::register_memory_font(
        &id,
        &crate::text::font_name(&bytes).unwrap(),
        Arc::clone(&bytes),
    )
    .unwrap();
    id
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Character {
    character: char,
    family: String,
    face: String,
    px: f32,
    color: [u8; 4],
    hscale: f32,
    vscale: f32,
    tracking: f32,
    leading: f32,
    baseline_shift: f32,
    paragraph_style: Option<String>,
    character_style: Option<String>,
}

/// Read effective style at logical Unicode character indices.
/// Args: complete story. Returns character, actual face, size, paint and named style evidence.
fn characters(run: &TypeRun) -> Vec<Character> {
    run.content
        .chars()
        .enumerate()
        .map(|(at, character)| {
            let span = run.character_style(at);
            let px = span.px.unwrap_or(run.px);
            let (family, face) = names(span.font.as_deref().unwrap_or(&run.font));
            let Fill::Solid(color) = span.fill.unwrap_or(Fill::Solid(Rgba::rgb(12, 50, 90))) else {
                panic!("fixture requires solid type")
            };
            Character {
                character,
                family,
                face,
                px,
                color: [color.r, color.g, color.b, color.a],
                hscale: span.hscale.unwrap_or(100.),
                vscale: span.vscale.unwrap_or(100.),
                tracking: span.tracking.unwrap_or(run.tracking / px * 1000.),
                leading: span.leading.map_or(run.line_height(), |l| l.pixels(px)),
                baseline_shift: span.baseline_shift.unwrap_or(0.),
                paragraph_style: crate::text::paragraph_style(run, at).publishing_style,
                character_style: span
                    .publishing_style
                    .filter(|id| id != "CharacterStyle/$ID/[No character style]"),
            }
        })
        .collect()
}

/// Hand-author mixed style XML independently of the production writer.
/// Args: source-folder-only master flag. Returns standard package bytes and exact master image bytes.
fn mixed_source(external: bool) -> (Vec<u8>, Vec<u8>) {
    let mut parts = zip_parts(&master_fixture());
    let image = parts["Links/logo.png"].clone();
    if external {
        parts.remove("Links/logo.png");
        let key = "MasterSpreads/MasterSpread_A.xml";
        let xml = String::from_utf8(parts[key].clone()).unwrap().replace(
            "Self=\"masterLogo\"",
            "Self=\"masterLogo\" Visible=\"false\"",
        );
        parts.insert(key.into(), xml.into_bytes());
    }
    parts.insert("Resources/Graphic.xml".into(), package("Graphic", "<Color Self=\"Color/Accent\" Name=\"Blue\" Space=\"RGB\" ColorValue=\"12 50 90\"/><Color Self=\"Color/Red\" Name=\"Red\" Space=\"RGB\" ColorValue=\"200 32 55\"/><Color Self=\"Color/Green\" Name=\"Green\" Space=\"RGB\" ColorValue=\"20 135 65\"/>").into_bytes());
    parts.insert("Resources/Styles.xml".into(), package("Styles", "<RootParagraphStyleGroup Self=\"pg\"><ParagraphStyle Self=\"ParagraphStyle/Body\" Name=\"Body\" PointSize=\"18\" FillColor=\"Color/Accent\" FontStyle=\"Regular\" Leading=\"24\"><Properties><AppliedFont type=\"string\">Liberation Sans</AppliedFont></Properties></ParagraphStyle><ParagraphStyle Self=\"ParagraphStyle/Center\" Name=\"Center\" PointSize=\"21\" FillColor=\"Color/Green\" Justification=\"CenterAlign\"><Properties><BasedOn type=\"object\">ParagraphStyle/Body</BasedOn></Properties></ParagraphStyle></RootParagraphStyleGroup><RootCharacterStyleGroup Self=\"cg\"><CharacterStyle Self=\"CharacterStyle/Emphasis\" Name=\"Emphasis\" FontStyle=\"Italic\" PointSize=\"26\" FillColor=\"Color/Red\" HorizontalScale=\"113\" VerticalScale=\"94\" Tracking=\"45\" Leading=\"33\" BaselineShift=\"2\"><Properties><AppliedFont type=\"string\">Liberation Serif</AppliedFont></Properties></CharacterStyle></RootCharacterStyleGroup>").into_bytes());
    parts.insert("Stories/Story_flow.xml".into(), package("Story", "<Story Self=\"flow\"><ParagraphStyleRange AppliedParagraphStyle=\"ParagraphStyle/Body\"><CharacterStyleRange><Content>Café &amp; </Content></CharacterStyleRange><CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/Emphasis\"><Content>typography</Content></CharacterStyleRange><CharacterStyleRange><Content> keeps independent source formatting.</Content><Br/></CharacterStyleRange></ParagraphStyleRange><ParagraphStyleRange AppliedParagraphStyle=\"ParagraphStyle/Center\"><CharacterStyleRange><Content>Second paragraph.</Content></CharacterStyleRange></ParagraphStyleRange><Table><Content>Never flattened</Content></Table><Note><Content>Private note omitted</Content></Note></Story>").into_bytes());
    (pack_parts(&parts), image)
}

#[test]
fn common_import_resolves_two_named_families_and_native_size_paint_runs() {
    assert_ne!(face("Liberation Sans"), face("Liberation Serif Italic"));
    let (doc, notes) = read(&mixed_source(false).0, "Independent mixed source").unwrap();
    assert!(
        !notes
            .iter()
            .any(|note| note.contains("not installed") || note.contains("Mixed IDML text colors")),
        "{notes:?}"
    );
    let run = texts(&doc)
        .into_iter()
        .find(|run| !run.content.is_empty())
        .unwrap();
    let evidence = characters(run);
    assert_eq!(
        (&evidence[0].family, &evidence[0].face),
        (&"Liberation Sans".to_string(), &"Regular".to_string())
    );
    assert_eq!((evidence[0].px, evidence[0].color), (18., BLUE));
    assert_eq!(
        (&evidence[7].family, &evidence[7].face),
        (&"Liberation Serif".to_string(), &"Italic".to_string())
    );
    assert_eq!(
        (
            evidence[7].px,
            evidence[7].color,
            evidence[7].hscale,
            evidence[7].vscale
        ),
        (26., RED, 113., 94.)
    );
    let second = run.content.chars().position(|c| c == '\n').unwrap() + 1;
    assert_eq!((evidence[second].px, evidence[second].color), (21., GREEN));
    assert_eq!(
        run.paragraphs[1].publishing_style.as_deref(),
        Some("ParagraphStyle/Center")
    );
}

/// Return frames in reciprocal story order, independent of document layer order.
/// Args: publishing document. Returns ordered frame runs and their names.
fn frames(doc: &Document) -> Vec<(&str, &TypeRun)> {
    let shapes: Vec<_> = doc
        .layers
        .iter()
        .filter_map(|l| l.kind.shapes())
        .flatten()
        .filter_map(|s| {
            if let Geom::Text(t) = &s.geom {
                Some((s, t))
            } else {
                None
            }
        })
        .collect();
    let mut next = Some(
        shapes
            .iter()
            .find(|(_, t)| t.thread.as_ref().unwrap().prev.is_none())
            .unwrap()
            .0
            .id,
    );
    let mut result = vec![];
    while let Some(id) = next {
        assert!(result.len() < shapes.len(), "cyclic fixture");
        let (shape, run) = shapes.iter().find(|(s, _)| s.id == id).unwrap();
        result.push((shape.name.as_str(), *run));
        next = run.thread.as_ref().unwrap().next;
    }
    result
}

/// Capture frame order, page-relative affine placement and actual flow intervals.
/// Args: editable document. Returns independent-checker frame evidence.
fn frame_evidence(doc: &Document) -> Vec<serde_json::Value> {
    frames(doc).into_iter().map(|(name, run)| {
        let bounds = crate::text_geometry::frame_bounds(run).unwrap();
        let page = doc.artboards.iter().position(|a| a.local_bounds().contains(bounds.center())).unwrap();
        let mut transform = run.transform.unwrap_or(Matrix::ID.0);
        transform[4] -= doc.artboards[page].origin.x;
        transform[5] -= doc.artboards[page].origin.y;
        let frame = run.frame.as_ref().unwrap();
        let top = run.origin.y - run.px * 0.85;
        let layout = run.layout.as_ref().unwrap();
        assert!(!layout.contours.is_empty());
        assert!(layout.paint_ranges.iter().all(|(_, r)| r.end <= layout.contours.len()));
        serde_json::json!({"name":name,"page":page,"transform":transform,"bounds":[top,run.origin.x,top+frame.size.y,run.origin.x+frame.size.x],"visible_start":layout.visible_start,"visible_end":layout.visible_end,"overflow":layout.overflow})
    }).collect()
}

/// Compare saved native frames at the model's exact float precision.
/// Args: reopened document and saved evidence. Returns nothing; asserts frame order and geometry.
fn assert_native_frames(doc: &Document, expected: &serde_json::Value) {
    let actual = frame_evidence(doc);
    let expected = expected.as_array().unwrap();
    assert_eq!(actual.len(), expected.len());
    for (a, b) in actual.iter().zip(expected) {
        for key in ["name", "page", "visible_start", "visible_end", "overflow"] {
            assert_eq!(a[key], b[key], "native frame {key}");
        }
        for key in ["transform", "bounds"] {
            let actual: Vec<f32> = serde_json::from_value(a[key].clone()).unwrap();
            let expected: Vec<f32> = serde_json::from_value(b[key].clone()).unwrap();
            assert_eq!(actual, expected, "native frame {key}");
        }
    }
}

/// Render publication pages without conflating spread-workspace spacing with page appearance.
/// Args: document. Returns one actual native raster per page.
fn pages(doc: &Document) -> Vec<tiny_skia::Pixmap> {
    let image = crate::compositor::render_export_at(doc, 1., 0., true).unwrap();
    doc.artboards
        .iter()
        .map(|page| {
            image
                .clone_rect(
                    tiny_skia::IntRect::from_xywh(
                        page.origin.x.round() as i32,
                        page.origin.y.round() as i32,
                        page.size.x.round() as u32,
                        page.size.y.round() as u32,
                    )
                    .unwrap(),
                )
                .unwrap()
        })
        .collect()
}

/// Bound visible raster error while also refusing a blank rendering.
/// Args: reference and roundtrip page renders. Returns measured mean channel errors.
fn appearance(expected: &[tiny_skia::Pixmap], actual: &[tiny_skia::Pixmap]) -> Vec<f32> {
    assert_eq!(expected.len(), actual.len());
    expected
        .iter()
        .zip(actual)
        .map(|(a, b)| {
            assert_eq!(a.data().len(), b.data().len());
            assert!(a.pixels().iter().filter(|p| p.alpha() > 0).count() > 500);
            let error = a
                .data()
                .iter()
                .zip(b.data())
                .map(|(a, b)| a.abs_diff(*b) as f32)
                .sum::<f32>()
                / a.data().len() as f32;
            assert!(error < 0.1, "page appearance changed: {error}/255");
            error
        })
        .collect()
}

/// Author native mixed ranges and two differently affine threaded frames.
/// Args: imported document and two portable font IDs. Returns nothing; updates editable source.
fn author(doc: &mut Document, sans: &str, serif: &str) {
    assert_eq!(frames(doc).len(), 2);
    let paragraph = AUTHORED.chars().position(|c| c == '\n').unwrap() + 1;
    for shape in doc
        .layers
        .iter_mut()
        .filter_map(|l| l.kind.shapes_mut())
        .flatten()
    {
        let Geom::Text(run) = &mut shape.geom else {
            continue;
        };
        let head = run.thread.as_ref().unwrap().prev.is_none();
        run.font = sans.into();
        run.px = 18.;
        run.origin = Pt::new(22., 20. + run.px * 0.85);
        run.frame.as_mut().unwrap().size = Pt::new(245., if head { 90. } else { 290. });
        run.transform = Some(if head {
            [0.96, 0.12, -0.08, 1.02, 6., 4.]
        } else {
            [1.02, -0.08, 0.10, 0.98, 305., 10.]
        });
        shape.name = if head { "MixedHead" } else { "MixedTail" }.into();
        shape.style.fill = Fill::Solid(Rgba::rgb(12, 50, 90));
        if head {
            run.replace_text(0, run.content.chars().count(), AUTHORED, None);
            run.spans.clear();
            run.set_character_style(0, AUTHORED.chars().count(), |span| {
                span.px = Some(18.);
                span.fill = Some(Fill::Solid(Rgba::rgb(12, 50, 90)));
                span.leading = Some(Leading::Fixed(24.));
            });
            run.set_character_style(7, 17, |span| {
                span.font = Some(serif.into());
                span.px = Some(27.);
                span.fill = Some(Fill::Solid(Rgba::rgb(200, 32, 55)));
                span.hscale = Some(119.);
                span.vscale = Some(94.);
                span.tracking = Some(45.);
                span.leading = Some(Leading::Fixed(33.));
                span.baseline_shift = Some(2.);
                span.publishing_style = Some("CharacterStyle/Emphasis".into());
            });
            run.set_character_style(paragraph, AUTHORED.chars().count(), |span| {
                span.px = Some(21.);
                span.fill = Some(Fill::Solid(Rgba::rgb(20, 135, 65)));
                span.hscale = Some(97.);
                span.vscale = Some(105.);
                span.leading = Some(Leading::Fixed(27.));
            });
            run.paragraphs = vec![
                ParagraphStyle {
                    publishing_style: Some("ParagraphStyle/Body".into()),
                    ..Default::default()
                },
                ParagraphStyle {
                    start: paragraph,
                    align: TextAlign::Center,
                    publishing_style: Some("ParagraphStyle/Center".into()),
                    ..Default::default()
                },
            ];
        }
    }
    crate::text_geometry::reflow(doc);
}

/// Verify actual native disk loading, detached export and reimport, including a cold font cache.
/// Args: native file and expected evidence directory. Returns a compact runtime receipt.
fn reopen(native: &Path, cold: bool) -> serde_json::Value {
    let directory = native.parent().unwrap();
    let expected: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("expected.json")).unwrap()).unwrap();
    let encoded: serde_json::Value =
        serde_json::from_slice(&std::fs::read(native).unwrap()).unwrap();
    assert_eq!(encoded["version"], 23);
    let ids: Vec<_> = encoded["doc"]["layers"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|l| {
            l["kind"]["Vector"]["shapes"]
                .as_array()
                .into_iter()
                .flatten()
        })
        .filter_map(|s| s["geom"]["Text"].as_object())
        .flat_map(|t| {
            t.get("spans")
                .and_then(|spans| spans.as_array())
                .into_iter()
                .flatten()
        })
        .filter_map(|span| span["font"].as_str())
        .filter(|font| font.starts_with("omatype:"))
        .collect();
    assert!(!ids.is_empty(), "portable span-only font is required");
    if cold {
        assert!(
            ids.iter().all(|id| !crate::text::project_font_loaded(id)),
            "fresh process already has the portable span font"
        );
    }
    let doc = crate::project::load_from(native).unwrap();
    let source = frames(&doc)[0].1;
    assert_eq!(
        serde_json::to_value(characters(source)).unwrap(),
        expected["characters"]
    );
    assert_eq!(
        serde_json::to_value(&source.paragraphs).unwrap(),
        expected["paragraphs"]
    );
    assert_native_frames(&doc, &expected["frames"]);
    let before: Vec<_> = (0..2)
        .map(|i| tiny_skia::Pixmap::load_png(directory.join(format!("authored-{i}.png"))).unwrap())
        .collect();
    let native_error = appearance(&before, &pages(&doc));
    let output = directory.join("book.idml");
    crate::formats::cli::export(&doc, &output).unwrap();
    let (round, notes) =
        read(&std::fs::read(&output).unwrap(), "Detached mixed-font IDML").unwrap();
    assert!(
        !notes.iter().any(|note| note.contains("not installed")
            || note.contains("Mixed IDML text colors")
            || note.contains("Rotated or sheared")),
        "{notes:?}"
    );
    assert_eq!(
        serde_json::to_value(characters(frames(&round)[0].1)).unwrap(),
        expected["characters"]
    );
    assert_eq!(
        serde_json::to_value(&frames(&round)[0].1.paragraphs).unwrap(),
        expected["paragraphs"]
    );
    let round_frames = frame_evidence(&round);
    for (a, b) in round_frames
        .iter()
        .zip(expected["frames"].as_array().unwrap())
    {
        for key in ["name", "page", "visible_start", "visible_end", "overflow"] {
            assert_eq!(a[key], b[key], "frame {key}");
        }
        for key in ["transform", "bounds"] {
            for (a, b) in a[key]
                .as_array()
                .unwrap()
                .iter()
                .zip(b[key].as_array().unwrap())
            {
                assert!((a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 0.0001);
            }
        }
    }
    let round_pages = pages(&round);
    let idml_error = appearance(&before, &round_pages);
    for (i, page) in round_pages.iter().enumerate() {
        page.save_png(directory.join(format!("reimported-{i}.png")))
            .unwrap();
    }
    for note in [
        "IDML Table is unsupported and was omitted; retain the source document.",
        "IDML Note is unsupported and was omitted; retain the source document.",
    ] {
        assert!(doc.import_notes.iter().any(|saved| saved == note));
    }
    assert_eq!(
        round
            .layers
            .iter()
            .filter(|l| l.kind.pixels().is_some())
            .count(),
        3
    );
    serde_json::json!({"cold_font_cache":cold,"native_version":23,"characters":source.content.chars().count(),"native_mean_channel_error":native_error,"idml_mean_channel_error":idml_error,"maximum_allowed_mean_channel_error":0.1,"frames":round_frames,"native":native,"output":output})
}

#[test]
fn common_mixed_fonts_affine_threaded_pages_native_disk_and_portable_span_archive() {
    if let Some(native) = std::env::var_os("OMADESIGN_IDML_COMMON_REOPEN") {
        let native = PathBuf::from(native);
        let receipt = reopen(&native, true);
        std::fs::write(
            native.parent().unwrap().join("cold-reopen.json"),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
        return;
    }
    let sans = portable(&face("Liberation Sans"));
    let serif = portable(&face("Liberation Serif Italic"));
    assert_ne!(sans, serif);
    let keep = std::env::var_os("OMADESIGN_IDML_RECEIPTS");
    let directory = keep
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
                ".artifacts/idml-common-{}",
                crate::document::next_id()
            ))
        })
        .join("mixed-font");
    assert!(
        !directory.exists(),
        "preserve existing qualification evidence"
    );
    assert!(directory.starts_with("/home"));
    std::fs::create_dir_all(directory.join(".omabrand")).unwrap();
    for mode in ["package", "source-folder"] {
        let case = directory.join(mode);
        let source = case.join("source");
        let delivery = case.join("delivery");
        std::fs::create_dir_all(source.join("Links")).unwrap();
        std::fs::create_dir_all(&delivery).unwrap();
        let (bytes, image) = mixed_source(mode == "source-folder");
        let input = source.join("book.idml");
        std::fs::write(&input, bytes).unwrap();
        if mode == "source-folder" {
            std::fs::write(source.join("Links/logo.png"), image).unwrap();
        }
        let crate::import::Imported::Document(mut doc) = crate::import::open_any(&input).unwrap()
        else {
            panic!("editable source required")
        };
        author(&mut doc, &sans, &serif);
        let frames = frame_evidence(&doc);
        assert_eq!(frames[0]["visible_start"], 0);
        assert_eq!(frames[0]["visible_end"], frames[1]["visible_start"]);
        assert!(frames[0]["visible_end"].as_u64().unwrap() > 0);
        assert_eq!(frames[1]["visible_end"], AUTHORED.chars().count());
        assert_eq!(frames[1]["overflow"], false);
        let authored = pages(&doc);
        let mut uniform = doc.clone();
        for shape in uniform
            .layers
            .iter_mut()
            .filter_map(|l| l.kind.shapes_mut())
            .flatten()
        {
            if let Geom::Text(run) = &mut shape.geom {
                for span in &mut run.spans {
                    span.font = None;
                }
            }
        }
        crate::text_geometry::reflow(&mut uniform);
        let uniform_pages = pages(&uniform);
        let family_ink_difference = authored
            .iter()
            .zip(&uniform_pages)
            .flat_map(|(a, b)| a.data().iter().zip(b.data()))
            .map(|(a, b)| a.abs_diff(*b) as f32)
            .sum::<f32>()
            / authored.iter().map(|a| a.data().len()).sum::<usize>() as f32;
        assert!(
            family_ink_difference > 0.02,
            "second resolved family did not change rendered glyphs"
        );
        for (i, page) in authored.iter().enumerate() {
            page.save_png(delivery.join(format!("authored-{i}.png")))
                .unwrap();
        }
        for color in [BLUE, RED, GREEN] {
            assert!(
                authored
                    .iter()
                    .flat_map(|p| p.pixels())
                    .filter(|p| p.alpha() > 200
                        && p.red().abs_diff(color[0]) < 4
                        && p.green().abs_diff(color[1]) < 4
                        && p.blue().abs_diff(color[2]) < 4)
                    .count()
                    > 30,
                "actual range paint absent: {color:?}"
            );
        }
        let run = texts(&doc)
            .into_iter()
            .find(|run| !run.content.is_empty())
            .unwrap();
        let expected = serde_json::json!({"characters":characters(run),"paragraphs":run.paragraphs,"frames":frames,"portable_font_ids":[sans,serif],"source_folder":source,"family_ink_difference":family_ink_difference});
        std::fs::write(
            delivery.join("expected.json"),
            serde_json::to_vec_pretty(&expected).unwrap(),
        )
        .unwrap();
        let native = delivery.join("book.oma");
        crate::formats::cli::export(&doc, &native).unwrap();
        std::fs::rename(&source, case.join("source-unavailable")).unwrap();
        let warm = reopen(&native, false);
        std::fs::write(
            delivery.join("warm-reopen.json"),
            serde_json::to_vec_pretty(&warm).unwrap(),
        )
        .unwrap();
        let child = std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","formats::idml::tests::common::common_mixed_fonts_affine_threaded_pages_native_disk_and_portable_span_archive","--nocapture"]).env("OMADESIGN_IDML_COMMON_REOPEN",&native).output().unwrap();
        let output = format!(
            "{}\n{}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
        std::fs::write(delivery.join("cold-reopen.log"), &output).unwrap();
        assert!(
            child.status.success(),
            "cold native reopen/export failed: {output}"
        );
    }
    let archive = directory.join(".omabrand/fonts");
    assert_eq!(
        std::fs::read_dir(&archive).unwrap().count(),
        2,
        "span-only portable family must share the existing native archive"
    );
    for id in [&sans, &serif] {
        let bytes = crate::text::project_font_bytes(id).unwrap();
        assert_eq!(
            std::fs::read(archive.join(format!("{}.ttf", id.trim_start_matches("omatype:"))))
                .unwrap(),
            *bytes
        );
    }
    if keep.is_none() {
        std::fs::remove_dir_all(directory).unwrap();
    }
}
