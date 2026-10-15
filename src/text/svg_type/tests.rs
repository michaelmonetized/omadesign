use super::*;
use crate::{document::Document, geom::Geom};

fn types(doc: &Document) -> Vec<&TypeRun> {
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
fn render(source: &str) -> tiny_skia::Pixmap {
    let tree = usvg::Tree::from_str(&renderer_source(source), &options()).unwrap();
    let mut pm =
        tiny_skia::Pixmap::new(tree.size().width() as u32, tree.size().height() as u32).unwrap();
    resvg::render(&tree, Transform::identity(), &mut pm.as_mut());
    pm
}
fn error(a: &tiny_skia::Pixmap, b: &tiny_skia::Pixmap) -> f32 {
    assert_eq!(a.data().len(), b.data().len());
    a.data()
        .iter()
        .zip(b.data())
        .map(|(a, b)| a.abs_diff(*b) as f32)
        .sum::<f32>()
        / a.data().len() as f32
}

fn diagnostic(name: &str, image: &tiny_skia::Pixmap, source: &str) {
    if let Some(directory) = std::env::var_os("OMA_SVG_TYPE_DIAGNOSTICS") {
        let directory = std::path::Path::new(&directory);
        image
            .save_png(directory.join(format!("{name}.png")))
            .unwrap();
        std::fs::write(directory.join(format!("{name}.svg")), source).unwrap();
    }
}

#[test]
fn svg_type_rotated_reflected_mixed_positioned_rtl_vertical_and_path_roundtrip() {
    for text in [
        r##"<text x="15" y="55" font-size="25" fill="#248" transform="matrix(.9 .25 -.2 1.1 30 4)">Affine <tspan font-size="36" fill="#d24" font-weight="bold">type</tspan></text>"##,
        r##"<style>.ink{font-family:DejaVu Sans;fill:#248;font-size:28px}</style><g class="ink"><text x="20" y="55" fill="inherit" font-size="inherit" text-anchor="end" transform="translate(180 0)"><tspan>Full</tspan> <tspan fill="#d24">styles</tspan> </text></g>"##,
        r##"<text x="15 52 95" y="55 68 55" dx="0 3 -2" dy="0 -2 1" rotate="0 22 -15" font-size="32" fill="#248" transform="translate(245 0) scale(-1 1)">ABC</text>"##,
        r##"<text x="220" y="55" font-size="32" fill="#248" direction="rtl" unicode-bidi="bidi-override">مرحبا بالعالم</text>"##,
        r##"<text x="100" y="25" font-size="25" fill="#248" writing-mode="tb">Vertical</text>"##,
        r##"<defs><path id="curve" d="M10 80 Q130 -15 250 80"/></defs><text font-size="25" fill="#248"><textPath href="#curve" startOffset="12">Editable path text</textPath></text>"##,
    ] {
        let source = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="280" height="150">{text}</svg>"#
        );
        let (doc, notes) = crate::formats::svg::read(&source, "Type acceptance").unwrap();
        assert!(!notes.iter().any(|n| n.contains("outlines")), "{notes:?}");
        let runs = types(&doc);
        assert_eq!(runs.len(), 1);
        assert!(runs[0].svg_text.is_some());
        assert!(!runs[0].contours.is_empty());
        let expected = render(&source);
        let actual = crate::compositor::render_export(&doc, 1).unwrap();
        assert!(
            error(&actual, &expected) < 0.1,
            "{}: {}",
            runs[0].content,
            error(&actual, &expected)
        );
        let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(&doc).unwrap(),
            serde_json::to_value(&reopened).unwrap()
        );
        assert!(
            error(
                &actual,
                &crate::compositor::render_export(&reopened, 1).unwrap()
            ) < 0.001
        );
        let svg = crate::svg::export(&reopened).unwrap();
        assert!(svg.contains("<text"));
        assert!(error(&render(&svg), &expected) < 0.1);
        let again = crate::formats::svg::read(&svg, "Exported SVG").unwrap().0;
        assert_eq!(
            types(&again)
                .iter()
                .map(|t| t.content.as_str())
                .collect::<Vec<_>>(),
            runs.iter().map(|t| t.content.as_str()).collect::<Vec<_>>()
        );
    }
}

#[test]
fn svg_type_edit_style_positions_matrix_undo_and_font_substitution() {
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="280" height="150"><text x="15" y="55" font-family="Deliberately Missing SVG Font" font-size="25" transform="rotate(12 20 50)">Keep <tspan fill="#d24" font-size="32">style</tspan></text></svg>"##;
    let (mut doc, notes) = crate::formats::svg::read(source, "Editing").unwrap();
    assert!(notes.iter().any(|n| n.contains("substituted")), "{notes:?}");
    let shape = doc
        .layers
        .iter_mut()
        .filter_map(|l| l.kind.shapes_mut())
        .flatten()
        .find(|s| matches!(s.geom, Geom::Text(_)))
        .unwrap();
    let before = shape.geom.clone();
    let Geom::Text(run) = &mut shape.geom else {
        unreachable!()
    };
    run.replace_text(5, 10, "editable", None);
    run.set_character_style(5, 13, |s| {
        s.fill = Some(crate::document::Fill::Solid(crate::color::Rgba::rgb(
            20, 170, 60,
        )));
        s.px = Some(36.);
    });
    let after = crate::text::shape(run);
    assert!(!after.is_empty());
    assert_ne!(after, run.contours);
    assert_eq!(run.content, "Keep editable");
    assert_eq!(crate::text::hit_char(run, crate::text::caret_pt(run, 0)), 0);
    assert!(
        !crate::text_geometry::selection_quads(run, 5, 13)
            .unwrap()
            .is_empty()
    );
    let (top, bottom) = crate::text::caret_segment(run, 5);
    assert!((top.x - bottom.x).abs() > 1.);
    shape.geom = before;
    let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    assert_eq!(types(&reopened)[0].content, "Keep style");
}

#[test]
fn svg_type_flow_retains_story_reflows_edits_and_exports_standard_text() {
    for source in [
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="280" height="180"><defs><rect id="frame" x="15" y="10" width="150" height="140"/></defs><text font-size="22" fill="#248" style="shape-inside:url(#frame);line-height:28px">Editable flowing words <tspan fill="#d24">retain their style</tspan> after reflow.</text></svg>"##,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="280" height="180"><flowRoot font-size="22" fill="#248"><flowRegion><rect x="15" y="10" width="150" height="140"/></flowRegion><flowPara>Editable flowing words retain their style after reflow.</flowPara></flowRoot></svg>"##,
    ] {
        let (mut doc, notes) = crate::formats::svg::read(source, "Flow").unwrap();
        assert!(!notes.iter().any(|n| n.contains("outlines")), "{notes:?}");
        assert_eq!(types(&doc).len(), 1);
        let run = types(&doc)[0];
        assert!(run.svg_text.as_ref().unwrap().flow.is_some());
        assert!(run.content.contains("Editable flowing words"));
        let old = crate::compositor::render_export(&doc, 1).unwrap();
        let shape = doc
            .layers
            .iter_mut()
            .filter_map(|l| l.kind.shapes_mut())
            .flatten()
            .find(|s| matches!(s.geom, Geom::Text(_)))
            .unwrap();
        let Geom::Text(run) = &mut shape.geom else {
            unreachable!()
        };
        run.replace_text(0, 8, "Short", None);
        let font = run.font.clone();
        run.font = crate::text::default_path()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        run.set_character_style(0, 5, |span| {
            span.font = Some(font.clone());
            span.px = Some(29.);
            span.fill = Some(crate::document::Fill::Solid(crate::color::Rgba::rgb(
                20, 170, 60,
            )));
        });
        run.contours = crate::text::shape(run);
        let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
        let current = crate::compositor::render_export(&reopened, 1).unwrap();
        assert!(error(&old, &current) > 0.1);
        let svg = crate::svg::export(&reopened).unwrap();
        assert!(svg.contains("data-oma-flow"));
        assert!(error(&render(&svg), &current) < 0.1);
        let again = crate::formats::svg::read(&svg, "Flow SVG").unwrap().0;
        assert_eq!(types(&again)[0].content, types(&reopened)[0].content);
        assert!(types(&again)[0].svg_text.as_ref().unwrap().flow.is_some());
        assert!(
            error(
                &crate::compositor::render_export(&again, 1).unwrap(),
                &current
            ) < 0.1
        );
    }
}

#[test]
fn svg_type_independent_appearance_and_editable_export() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let source = include_str!("../../../tests/assets/svg-type/affine-styles.svg");
    fn independent(source: &str, full_svg: bool) -> tiny_skia::Pixmap {
        let mut command = Command::new(if full_svg { "inkscape" } else { "rsvg-convert" });
        if full_svg {
            command.args(["--pipe", "--export-type=png", "--export-filename=-"]);
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Inkscape and librsvg are required for independent SVG type qualification");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(source.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        tiny_skia::Pixmap::decode_png(&output.stdout).unwrap()
    }
    let (doc, notes) = crate::formats::svg::read(source, "Independent type").unwrap();
    assert!(!notes.iter().any(|n| n.contains("outlines")), "{notes:?}");
    assert_eq!(types(&doc).len(), 5);
    let expected = independent(source, true);
    let native = crate::compositor::render_export(&doc, 1).unwrap();
    diagnostic(
        "independent-native",
        &native,
        &crate::svg::export(&doc).unwrap(),
    );
    for (i, run) in types(&doc).iter().enumerate() {
        let source = super::source(run).unwrap();
        let rendered = render(&source);
        diagnostic(&format!("independent-object-{i}"), &rendered, &source);
        if i == 0 {
            assert!(error(&rendered, &independent(&source, false)) < 1.5);
        }
    }
    assert!(
        error(&native, &expected) < 1.5,
        "independent native mean={}",
        error(&native, &expected)
    );
    let exported = crate::svg::export(&doc).unwrap();
    assert_eq!(exported.matches("<text ").count(), 5);
    assert!(
        error(&independent(&exported, true), &expected) < 1.5,
        "independent exported mean={}",
        error(&independent(&exported, true), &expected)
    );
}

#[test]
fn svg_type_rtl_repeated_letters_keep_logical_carets_and_source_direction() {
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="300" height="100"><text x="270" y="55" font-family="DejaVu Sans" font-size="30" direction="rtl" unicode-bidi="bidi-override">مرحبا بالعالم</text></svg>"##;
    let (doc, _) = crate::formats::svg::read(source, "Logical RTL").unwrap();
    let run = types(&doc)[0];
    assert_eq!(run.content, "مرحبا بالعالم");
    let first = crate::text::caret_pt(run, 0);
    let last = crate::text::caret_pt(run, run.content.chars().count());
    assert!(first.x > last.x, "{first:?}, {last:?}");
    assert_eq!(crate::text::hit_char(run, first), 0);
    let exported = crate::svg::export(&doc).unwrap();
    assert!(exported.contains("direction=\"rtl\""));
    assert!(!exported.contains('\u{202e}'));
    assert_eq!(
        types(&crate::formats::svg::read(&exported, "RTL").unwrap().0)[0].content,
        run.content
    );
}

#[test]
fn svg_type_saved_metadata_cannot_add_executable_svg_content() {
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"><text x="20" y="50" transform="rotate(10)">Safe type</text></svg>"##;
    let (mut doc, _) = crate::formats::svg::read(source, "Safe type").unwrap();
    let shape = doc
        .layers
        .iter_mut()
        .filter_map(|l| l.kind.shapes_mut())
        .flatten()
        .find(|s| matches!(s.geom, Geom::Text(_)))
        .unwrap();
    let Geom::Text(run) = &mut shape.geom else {
        unreachable!()
    };
    let svg = run.svg_text.as_mut().unwrap();
    svg.definitions
        .push_str("<script>alert(1)</script><image href=\"https://invalid.example/private.png\"/>");
    let TextNode::Element {
        attributes,
        children,
        ..
    } = &mut svg.root
    else {
        unreachable!()
    };
    attributes.insert("onload".into(), "alert(1)".into());
    children.push(TextNode::Element {
        name: "script".into(),
        attributes: BTreeMap::new(),
        children: vec![],
    });
    let exported = crate::svg::export(&doc).unwrap();
    let xml = roxmltree::Document::parse(&exported).unwrap();
    assert!(!xml.descendants().any(|n| n.has_tag_name("script")
        || n.attribute("onload").is_some()
        || n.has_tag_name("image")));
}

#[test]
fn svg_type_nondefault_variable_axis_keeps_live_type_and_portable_font() {
    let bytes = Arc::new(include_bytes!("../../../tests/assets/fonts/EBGaramond.ttf").to_vec());
    let id = format!("omatype:{:032x}", crate::typography::fingerprint(&bytes));
    crate::text::register_memory_font(&id, "Variable type", bytes).unwrap();
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="300" height="120"><text x="20" y="65" font-family="DejaVu Sans" font-size="42" transform="rotate(8 20 65)" style="font-variation-settings:'wght' 400">Variable type</text></svg>"##;
    let (mut doc, _) = crate::formats::svg::read(source, "Variable type").unwrap();
    let shape = doc
        .layers
        .iter_mut()
        .filter_map(|l| l.kind.shapes_mut())
        .flatten()
        .find(|s| matches!(s.geom, Geom::Text(_)))
        .unwrap();
    let Geom::Text(run) = &mut shape.geom else {
        unreachable!()
    };
    run.set_character_style(0, run.content.chars().count(), |s| {
        s.font = Some(id.clone())
    });
    let light = crate::text::shape(run);
    assert!(!light.is_empty());
    let TextNode::Element { attributes, .. } = &mut run.svg_text.as_mut().unwrap().root else {
        unreachable!()
    };
    attributes.insert("font-variation-settings".into(), "'wght' 800".into());
    let heavy = crate::text::shape(run);
    assert_ne!(light, heavy);
    run.contours = heavy;
    crate::typography::validate_document_fonts(&doc).unwrap();
    let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    assert_eq!(types(&reopened)[0].content, "Variable type");
    assert!(types(&reopened)[0].svg_text.is_some());
    let exported = crate::svg::export(&reopened).unwrap();
    assert!(
        !exported.contains("<text"),
        "Portable project faces follow the existing outlined SVG delivery policy"
    );
    assert!(
        error(
            &render(&exported),
            &crate::compositor::render_export(&reopened, 1).unwrap()
        ) < 0.1
    );
}

#[test]
fn svg_type_reviewed_viewport_carets_file_fonts_and_native_paints() {
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="280" height="150"><text x="50%" y="50%" font-family="DejaVu Sans" font-size="25" transform="rotate(8 140 75)">Percent</text></svg>"##;
    let (doc, _) = crate::formats::svg::read(source, "Percentage type").unwrap();
    assert!(
        error(
            &crate::compositor::render_export(&doc, 1).unwrap(),
            &render(source)
        ) < 0.1
    );
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="280" height="150"><text x="20 150" y="75" font-family="DejaVu Sans" font-size="30" transform="rotate(8 20 75)">AB</text></svg>"##;
    let (mut doc, _) = crate::formats::svg::read(source, "Positioned edits").unwrap();
    let run = types(&doc)[0];
    assert!((crate::text::caret_pt(run, 1) - run.map_point(Pt::new(150., 75.))).length() < 0.01);
    let shape = doc
        .layers
        .iter_mut()
        .filter_map(|l| l.kind.shapes_mut())
        .flatten()
        .find(|s| matches!(s.geom, Geom::Text(_)))
        .unwrap();
    let Geom::Text(run) = &mut shape.geom else {
        unreachable!()
    };
    let before = crate::text::shape(run);
    run.set_character_style(0, 2, |s| {
        s.font = Some(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/assets/fonts/EBGaramond.ttf"
            )
            .into(),
        )
    });
    run.contours = crate::text::shape(run);
    assert_ne!(before, run.contours);
    shape.style.fill = crate::document::Fill::Solid(crate::color::Rgba::rgb(20, 170, 60));
    shape.style.stroke = Some(crate::document::Stroke {
        width: 3.,
        color: crate::color::Rgba::rgb(230, 20, 40),
        ..Default::default()
    });
    let rendered = crate::compositor::render_export(&doc, 1).unwrap();
    assert!(
        rendered
            .pixels()
            .iter()
            .any(|p| p.green().saturating_sub(p.blue()) > 40)
    );
    assert!(
        rendered
            .pixels()
            .iter()
            .any(|p| p.red().saturating_sub(p.green()) > 40)
    );
    let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    assert!(
        error(
            &rendered,
            &crate::compositor::render_export(&reopened, 1).unwrap()
        ) < 0.001
    );
    assert!(error(&rendered, &render(&crate::svg::export(&doc).unwrap())) < 0.1);
}

#[test]
fn svg_type_reviewed_viewport_units_opacity_gradient_edits_and_flow_leading() {
    for source in [
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100" viewBox="0 0 100 50"><text x="50%" y="50%" font-size="12" transform="rotate(8 50 25)">Viewport</text></svg>"##,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="2in" height="1in"><text x="50%" y="50%" font-size="20" transform="rotate(8 96 48)">Physical</text></svg>"##,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="240" height="120"><text x="20" y="65" font-size="32" transform="rotate(8 20 65)">A<tspan opacity="0">B</tspan></text></svg>"##,
    ] {
        let (doc, _) = crate::formats::svg::read(source, "Reviewed SVG").unwrap();
        let expected = render(source);
        assert!(
            error(
                &crate::compositor::render_export(&doc, 1).unwrap(),
                &expected
            ) < 0.1
        );
        assert!(error(&render(&crate::svg::export(&doc).unwrap()), &expected) < 0.1);
    }
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="260" height="140"><defs><linearGradient id="ink"><stop stop-color="red"/><stop offset="1" stop-color="blue"/></linearGradient></defs><text x="20" y="80" font-family="DejaVu Sans" font-size="45" transform="rotate(8 20 80)" fill="url(#ink)" stroke="#248" stroke-width="12" paint-order="stroke fill">Paint</text></svg>"##;
    let (mut doc, _) = crate::formats::svg::read(source, "Paint source").unwrap();
    let image = crate::compositor::render_export(&doc, 1).unwrap();
    let shape = doc
        .layers
        .iter_mut()
        .filter_map(|l| l.kind.shapes_mut())
        .flatten()
        .find(|s| matches!(s.geom, Geom::Text(_)))
        .unwrap();
    assert!(!shape.style.fill.is_none());
    shape.style.fill = crate::document::Fill::None;
    let no_fill = crate::compositor::render_export(&doc, 1).unwrap();
    assert!(error(&image, &no_fill) > 0.1);
    let shape = doc
        .layers
        .iter_mut()
        .filter_map(|l| l.kind.shapes_mut())
        .flatten()
        .find(|s| matches!(s.geom, Geom::Text(_)))
        .unwrap();
    shape.style.fill = crate::document::Fill::Solid(crate::color::Rgba::rgb(20, 170, 60));
    let current = crate::compositor::render_export(&doc, 1).unwrap();
    assert!(error(&current, &render(&crate::svg::export(&doc).unwrap())) < 0.1);
    let Geom::Text(run) = &mut doc
        .layers
        .iter_mut()
        .filter_map(|l| l.kind.shapes_mut())
        .flatten()
        .find(|s| matches!(s.geom, Geom::Text(_)))
        .unwrap()
        .geom
    else {
        unreachable!()
    };
    run.set_character_style(0, 1, |s| {
        s.fill = Some(crate::document::Fill::Linear {
            from: [0., 0.],
            to: [1., 0.],
            c0: crate::color::Rgba::rgb(255, 0, 0),
            c1: crate::color::Rgba::rgb(0, 0, 255),
        })
    });
    let svg = crate::text::svg_type::source(run).unwrap();
    assert!(svg.contains("oma-range-paint-"));
    let gradient = crate::text::shape(run);
    assert!(!gradient.is_empty());
    let painted = crate::compositor::render_export(&doc, 1).unwrap();
    assert!(error(&painted, &current) > 0.1);
    assert!(error(&painted, &render(&crate::svg::export(&doc).unwrap())) < 0.1);
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="220" height="160"><text x="10" y="25" font-size="20" fill="#248" style="inline-size:100px;line-height:1.2">Flow words wrap across several rows.</text></svg>"##;
    let (doc, _) = crate::formats::svg::read(source, "Unitless flow").unwrap();
    let run = types(&doc)[0];
    assert!(
        (run.svg_text
            .as_ref()
            .unwrap()
            .flow
            .as_ref()
            .unwrap()
            .leading
            - 24.)
            .abs()
            < 0.001
    );
}

#[test]
fn svg_type_flow_effects_inherited_metrics_inline_patterns_and_typing_opacity() {
    use base64::Engine;
    let hidden = r##"<svg xmlns="http://www.w3.org/2000/svg" width="260" height="180"><g font-family="DejaVu Sans" font-size="40"><text y="40" opacity="0" style="inline-size:180px">Flow words with inherited metrics wrap.</text></g></svg>"##;
    let (hidden_doc, _) = crate::formats::svg::read(hidden, "Hidden flow").unwrap();
    assert!(
        crate::compositor::render_export(&hidden_doc, 1)
            .unwrap()
            .pixels()
            .iter()
            .all(|p| p.alpha() == 0)
    );
    let visible = hidden.replace("opacity=\"0\"", "opacity=\"1\"");
    let (doc, _) = crate::formats::svg::read(&visible, "Inherited flow").unwrap();
    let flow = types(&doc)[0]
        .svg_text
        .as_ref()
        .unwrap()
        .flow
        .as_ref()
        .unwrap();
    assert!((flow.leading - 48.).abs() < 0.01);
    assert!((flow.y - 0.).abs() < 0.01);
    let mut tile = tiny_skia::Pixmap::new(8, 8).unwrap();
    tile.fill(tiny_skia::Color::from_rgba8(20, 170, 60, 255));
    let data = base64::engine::general_purpose::STANDARD.encode(tile.encode_png().unwrap());
    let source = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="260" height="140"><defs><pattern id="tile" patternUnits="userSpaceOnUse" width="8" height="8"><image href="data:image/png;base64,{data}" width="8" height="8"/></pattern></defs><text x="20" y="80" font-family="DejaVu Sans" font-size="40" transform="rotate(8 20 80)" fill="url(#tile)">Pattern</text></svg>"##
    );
    let (doc, _) = crate::formats::svg::read(&source, "Image pattern type").unwrap();
    assert_eq!(types(&doc).len(), 1);
    let expected = render(&source);
    let native = crate::compositor::render_export(&doc, 1).unwrap();
    assert!(error(&native, &expected) < 0.1);
    let exported = crate::svg::export(&doc).unwrap();
    assert!(error(&render(&exported), &expected) < 0.1);
    let again = crate::formats::svg::read(&exported, "Pattern type again")
        .unwrap()
        .0;
    assert!(
        error(
            &crate::compositor::render_export(&again, 1).unwrap(),
            &expected
        ) < 0.1
    );
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="260" height="140"><text x="20" y="80" font-family="DejaVu Sans" font-size="40" transform="rotate(8 20 80)" fill="#248" fill-opacity=".5">AA</text></svg>"##;
    let (mut doc, _) = crate::formats::svg::read(source, "Typing opacity").unwrap();
    let shape = doc
        .layers
        .iter_mut()
        .filter_map(|l| l.kind.shapes_mut())
        .flatten()
        .find(|s| matches!(s.geom, Geom::Text(_)))
        .unwrap();
    let Geom::Text(run) = &mut shape.geom else {
        unreachable!()
    };
    let pending = run.character_style(1);
    run.replace_text(2, 2, "A", Some(pending));
    run.contours = crate::text::shape(run);
    let expected = render(&source.replace(">AA</text>", ">AAA</text>"));
    assert!(
        error(
            &crate::compositor::render_export(&doc, 1).unwrap(),
            &expected
        ) < 0.1
    );
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="260" height="140"><text x="20" y="80" font-family="DejaVu Sans" font-size="40" transform="rotate(8 20 80)" fill="#248">A<tspan fill="#d24">B</tspan></text></svg>"##;
    let (mut doc, _) = crate::formats::svg::read(source, "Object paint inheritance").unwrap();
    doc.layers
        .iter_mut()
        .filter_map(|l| l.kind.shapes_mut())
        .flatten()
        .find(|s| matches!(s.geom, Geom::Text(_)))
        .unwrap()
        .style
        .fill = crate::document::Fill::Solid(crate::color::Rgba::rgb(20, 170, 60));
    let expected = render(&source.replace("fill=\"#248\"", "fill=\"#14aa3c\""));
    assert!(
        error(
            &crate::compositor::render_export(&doc, 1).unwrap(),
            &expected
        ) < 0.1
    );
}
