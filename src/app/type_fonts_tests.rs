use super::*;
use crate::{
    color::Rgba,
    document::{Document, Fill, Layer, Shape, Style},
    geom::{ParagraphStyle, TextAlign},
    text::font_inventory,
};
const BASE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/assets/fonts/EBGaramond.ttf"
);
fn alternate() -> String {
    let source = crate::typography::fingerprint(&std::fs::read(BASE).unwrap());
    let fonts = crate::text::all_fonts_cached();
    fonts
        .iter()
        .filter(|font| font.name == "DejaVu Sans")
        .chain(fonts.iter().filter(|font| font.name != "DejaVu Sans"))
        .find_map(|font| {
            let path = font.path.to_string_lossy().into_owned();
            let data = crate::text::font_bytes_for(&TypeRun {
                font: path.clone(),
                ..Default::default()
            })?;
            let face = rustybuzz::Face::from_slice(&data, 0)?;
            (face.glyph_index('A').is_some_and(|id| id.0 > 0)
                && crate::typography::fingerprint(&data) != source)
                .then_some(path)
        })
        .expect("A real alternate installed Latin font")
}
fn editor() -> (Studio, (usize, u64), String) {
    let other = alternate();
    let mut s = Studio::new();
    s.show_welcome = false;
    s.persona = Persona::Layout;
    s.doc = Document::new("Find Font core", 500., 260., 96.);
    s.doc.workspace = Some(Persona::Layout);
    let mut layer = Layer::vector("Font users");
    for (index, text) in ["AA bb.\nCC", "Other A"].into_iter().enumerate() {
        let mut run = TypeRun {
            content: text.into(),
            font: BASE.into(),
            origin: Pt::new(30., 60. + index as f32 * 90.),
            px: 28.,
            ..Default::default()
        };
        if index == 0 {
            run.set_character_style(0, 2, |span| {
                span.fill = Some(Fill::Solid(Rgba::rgb(210, 30, 70)))
            });
            run.set_character_style(3, 5, |span| span.font = Some(other.clone()));
            run.paragraphs = vec![ParagraphStyle {
                start: 7,
                align: TextAlign::End,
                ..Default::default()
            }];
            run.manual_kern.insert(1, -1.25);
        }
        let mut shape = Shape::new(Geom::Text(run), Style::default());
        shape.name = format!("Font story {}", index + 1);
        crate::text::fill_contours(&mut shape.geom);
        layer.kind.shapes_mut().unwrap().push(shape);
    }
    s.doc.layers = vec![layer];
    s.active_layer = Some(0);
    let hit = (0, s.doc.layers[0].kind.shapes().unwrap()[0].id);
    s.selection = vec![hit];
    s.mark();
    s.history.clear();
    (s, hit, other)
}
fn run(s: &Studio, hit: (usize, u64)) -> &TypeRun {
    match &s.doc.find_shape(hit.0, hit.1).unwrap().geom {
        Geom::Text(run) => run,
        _ => panic!("Lost text"),
    }
}
fn wire(s: &Studio) -> String {
    crate::project::encode(&s.doc).unwrap()
}
fn from() -> Vec<Request> {
    vec![Request::Stored(BASE.into())]
}
fn reopen(s: &Studio) {
    let encoded = wire(s);
    let doc = crate::project::decode(&encoded).unwrap();
    assert_eq!(crate::project::encode(&doc).unwrap(), encoded);
    assert_eq!(
        crate::compositor::export_png(&doc, 1).unwrap(),
        crate::compositor::export_png(&s.doc, 1).unwrap()
    );
}
#[test]
fn craft_find_font_core_reports_real_faces_runs_and_unique_missing_glyphs_without_editing() {
    let (mut s, hit, _) = editor();
    let id = s.doc.layers[0].kind.shapes().unwrap()[1].id;
    let bytes = std::sync::Arc::new(std::fs::read(BASE).unwrap());
    let managed = format!("omatype:{:032x}", crate::typography::fingerprint(&bytes));
    crate::text::register_memory_font(&managed, "Native font alias", bytes).unwrap();
    if let Geom::Text(r) = &mut s.doc.find_shape_mut(0, id).unwrap().geom {
        r.font = managed;
        r.content.push('\u{10ffff}');
    }
    if let Geom::Text(r) = &mut s.doc.find_shape_mut(hit.0, hit.1).unwrap().geom {
        r.content.push('\u{10ffff}');
    }
    s.doc.find_shape_mut(0, id).unwrap().visible = false;
    s.doc.find_shape_mut(0, id).unwrap().locked = true;
    s.mark();
    let before = wire(&s);
    let rows = s.font_usage().unwrap();
    assert_eq!(rows.len(), 2);
    let base = rows
        .iter()
        .find(|r| r.from.contains(&Request::Stored(BASE.into())))
        .unwrap();
    assert_eq!(base.runs, 4);
    assert_eq!(base.stories.len(), 2);
    assert_eq!(base.from.len(), 2);
    assert_eq!(base.info.status, Status::Exact);
    assert!(!base.info.family.is_empty());
    assert!(!base.info.style.is_empty());
    assert_eq!(base.info.missing_glyphs, 1);
    assert_eq!(wire(&s), before);
    assert_eq!(s.history.len(), 0);
}
#[test]
fn craft_find_font_core_replaces_selection_preserving_other_styles_one_undo_and_native_reopen() {
    let (mut s, hit, other) = editor();
    let before = wire(&s);
    let original = run(&s, hit).clone();
    let before_png = crate::compositor::export_png(&s.doc, 1).unwrap();
    let untouched = s.doc.layers[0].kind.shapes().unwrap()[1].clone();
    let target = s
        .capture_font_change(TextScope::Selection, &from())
        .unwrap();
    assert_eq!(s.replace_font_usage(&target, &other).unwrap(), 1);
    assert_eq!(run(&s, hit).font, other);
    assert_eq!(run(&s, hit).content, original.content);
    assert_eq!(run(&s, hit).spans, original.spans);
    assert_eq!(run(&s, hit).paragraphs, original.paragraphs);
    assert_eq!(run(&s, hit).manual_kern, original.manual_kern);
    assert_eq!(run(&s, hit).origin, original.origin);
    assert_ne!(run(&s, hit).contours, original.contours);
    assert_eq!(s.doc.find_shape(0, untouched.id), Some(&untouched));
    assert_eq!(s.history.len(), 1);
    reopen(&s);
    let after = wire(&s);
    let after_png = crate::compositor::export_png(&s.doc, 1).unwrap();
    assert_ne!(before_png, after_png);
    s.undo();
    assert_eq!(wire(&s), before);
    s.redo();
    assert_eq!(wire(&s), after);
    if let Some(root) = std::env::var_os("OMADESIGN_FIND_FONT_QA") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(std::path::Path::new(&root).join("font-before.oma"), before).unwrap();
        std::fs::write(std::path::Path::new(&root).join("font-selected.oma"), after).unwrap();
        std::fs::write(
            std::path::Path::new(&root).join("font-before.png"),
            before_png,
        )
        .unwrap();
        std::fs::write(
            std::path::Path::new(&root).join("font-selected.png"),
            after_png,
        )
        .unwrap();
    }
}
#[test]
fn craft_find_font_core_noop_and_invalid_destination_preserve_redo_and_stale_owners() {
    let (mut s, _, other) = editor();
    let target = s
        .capture_font_change(TextScope::Selection, &from())
        .unwrap();
    s.replace_font_usage(&target, &other).unwrap();
    let after = wire(&s);
    s.undo();
    let before = wire(&s);
    let target = s
        .capture_font_change(TextScope::Selection, &from())
        .unwrap();
    assert_eq!(s.replace_font_usage(&target, BASE).unwrap(), 0);
    assert_eq!(wire(&s), before);
    assert!(
        s.replace_font_usage(&target, "/not-a-real-font/absent.ttf")
            .is_err()
    );
    assert_eq!(wire(&s), before);
    s.redo();
    assert_eq!(wire(&s), after);
    s.undo();
    let target = s.capture_font_change(TextScope::Document, &from()).unwrap();
    s.selection.clear();
    assert!(!s.font_target_current(&target));
    assert!(s.replace_font_usage(&target, &other).is_err());
    assert_eq!(wire(&s), before);
}
#[test]
fn craft_find_font_core_find_includes_locked_hidden_and_empty_results_without_granting_write_access()
 {
    let (mut s, hit, other) = editor();
    let second = s.doc.layers[0].kind.shapes().unwrap()[1].id;
    s.doc.find_shape_mut(0, second).unwrap().locked = true;
    assert!(s.capture_font_change(TextScope::Document, &from()).is_err());
    let target = s.capture_font_find(&from()).unwrap();
    let before = wire(&s);
    assert_eq!(s.select_font_usage(&target).unwrap(), 2);
    assert_eq!(s.selection, vec![hit, (0, second)]);
    assert_eq!(wire(&s), before);
    assert!(s.replace_font_usage(&target, &other).is_err());
    if let Geom::Text(r) = &mut s.doc.find_shape_mut(0, second).unwrap().geom {
        r.font = other.clone();
    }
    s.mark();
    let target = s.capture_font_change(TextScope::Document, &from()).unwrap();
    assert_eq!(s.replace_font_usage(&target, &other).unwrap(), 1);
    s.doc.find_shape_mut(hit.0, hit.1).unwrap().visible = false;
    let before = wire(&s);
    let target = s.capture_font_find(&[Request::Stored(other)]).unwrap();
    assert_eq!(s.select_font_usage(&target).unwrap(), 2);
    assert_eq!(wire(&s), before);
    let target = s
        .capture_font_find(&[Request::Stored("/not-used-in-this-document.ttf".into())])
        .unwrap();
    let steps = s.history.len();
    assert_eq!(s.select_font_usage(&target).unwrap(), 0);
    assert!(s.selection.is_empty());
    assert_eq!(s.history.len(), steps);
    assert_eq!(wire(&s), before);
}
#[test]
fn craft_find_font_core_live_story_replacement_retains_existing_draft_caret_and_history_boundary() {
    let (mut s, hit, other) = editor();
    let before = wire(&s);
    s.begin_type_edit(hit, Pt::new(30., 60.));
    {
        let edit = s.type_edit.as_mut().unwrap();
        edit.anchor = 7;
        edit.caret = 2;
    }
    let target = s
        .capture_font_change(TextScope::Selection, &from())
        .unwrap();
    assert_eq!(s.replace_font_usage(&target, &other).unwrap(), 1);
    assert_eq!(run(&s, hit).font, other);
    assert_eq!(
        s.type_edit.as_ref().map(|e| (e.caret, e.anchor)),
        Some((2, 7))
    );
    assert_eq!(s.history.len(), 0);
    s.commit_type_edit();
    assert_eq!(s.history.len(), 1);
    reopen(&s);
    s.undo();
    assert_eq!(wire(&s), before);
}
#[test]
fn craft_find_font_core_svg_authored_missing_requests_are_distinct_from_real_fallback_and_native_overrides()
 {
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="300" height="160"><text x="20" y="60" font-size="28" font-family="Oma Deliberately Missing Find Font">A<tspan fill="#d24" font-family="DejaVu Sans">B</tspan>C</text></svg>"##;
    let (doc, _) = crate::formats::svg::read(source, "Authored font requests").unwrap();
    let mut s = Studio::new();
    s.show_welcome = false;
    s.persona = Persona::Design;
    s.doc = doc;
    let hit = s
        .doc
        .layers
        .iter()
        .enumerate()
        .find_map(|(l, layer)| {
            layer
                .kind
                .shapes()?
                .iter()
                .find_map(|shape| matches!(shape.geom, Geom::Text(_)).then_some((l, shape.id)))
        })
        .unwrap();
    s.selection = vec![hit];
    s.active_layer = Some(hit.0);
    s.mark();
    s.history.clear();
    let original = run(&s, hit).clone();
    let before = wire(&s);
    let rows = s.font_usage().unwrap();
    let missing = rows
        .iter()
        .find(|r| r.info.family == "Oma Deliberately Missing Find Font")
        .unwrap();
    assert_eq!(missing.info.status, Status::Missing);
    assert_eq!(missing.runs, 2);
    assert!(missing.info.resolved.is_some());
    let target = s
        .capture_font_change(TextScope::Document, &missing.from)
        .unwrap();
    let other = alternate();
    assert_eq!(s.replace_font_usage(&target, &other).unwrap(), 1);
    assert_eq!(run(&s, hit).svg_text, original.svg_text);
    assert_eq!(run(&s, hit).content, "ABC");
    assert_eq!(run(&s, hit).character_style(0).font, Some(other.clone()));
    assert_eq!(run(&s, hit).character_style(2).font, Some(other));
    assert_eq!(run(&s, hit).character_style(1), original.character_style(1));
    reopen(&s);
    s.undo();
    assert_eq!(wire(&s), before);
}
#[test]
fn craft_find_font_core_malformed_ranges_and_changed_locks_refuse_before_any_story_changes() {
    let (mut s, hit, other) = editor();
    let target = s.capture_font_change(TextScope::Document, &from()).unwrap();
    let second = s.doc.layers[0].kind.shapes().unwrap()[1].id;
    if let Geom::Text(r) = &mut s.doc.find_shape_mut(0, second).unwrap().geom {
        r.spans = vec![CharSpan {
            start: 0,
            end: 9999,
            font: Some(BASE.into()),
            ..Default::default()
        }];
    }
    let before = serde_json::to_value(&s.doc).unwrap();
    assert!(s.replace_font_usage(&target, &other).is_err());
    assert_eq!(serde_json::to_value(&s.doc).unwrap(), before);
    assert_eq!(s.history.len(), 0);
    if let Geom::Text(r) = &mut s.doc.find_shape_mut(0, second).unwrap().geom {
        r.spans.clear();
    }
    let target = s
        .capture_font_change(TextScope::Selection, &from())
        .unwrap();
    s.doc.find_shape_mut(hit.0, hit.1).unwrap().locked = true;
    assert!(!s.font_target_current(&target));
    let before = serde_json::to_value(&s.doc).unwrap();
    assert!(s.replace_font_usage(&target, &other).is_err());
    assert_eq!(serde_json::to_value(&s.doc).unwrap(), before);
}
#[test]
fn craft_find_font_core_empty_text_keeps_its_real_face_and_does_not_invent_fallback_identity() {
    let mut r = TypeRun {
        font: BASE.into(),
        ..Default::default()
    };
    let uses = font_inventory::uses(&r).unwrap();
    assert_eq!(uses.len(), 1);
    assert_eq!(uses[0].request, Request::Stored(BASE.into()));
    assert!(!font_inventory::replace(&mut r, &from(), BASE).unwrap());
    let other = alternate();
    assert!(font_inventory::replace(&mut r, &from(), &other).unwrap());
    assert_eq!(r.font, other);
    assert!(r.content.is_empty());
    let info = font_inventory::describe(
        &Request::Stored("/missing/EBGaramond.ttf".into()),
        BASE,
        &['A', '\n', '\u{10ffff}'].into_iter().collect(),
    );
    assert_eq!(info.status, Status::Missing);
    assert_eq!(info.family, "EBGaramond");
    assert!(info.identity.is_none());
    assert_eq!(info.missing_glyphs, 1);
}
