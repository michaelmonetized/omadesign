use super::*;
use crate::{
    color::Rgba,
    document::{Document, Layer},
    geom::{ParagraphStyle, TextAlign},
};

fn editor(text: &str) -> (Studio, (usize, u64)) {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.persona = Persona::Layout;
    s.doc = Document::new("Type content", 500., 240., 96.);
    s.doc.workspace = Some(Persona::Layout);
    s.doc.layers = vec![Layer::vector("Editable text")];
    let mut run = TypeRun {
        content: text.into(),
        origin: Pt::new(35., 65.),
        px: 28.,
        font: concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/assets/fonts/EBGaramond.ttf"
        )
        .into(),
        ..Default::default()
    };
    run.set_character_style(0, text.chars().count().min(6), |span| {
        span.fill = Some(Fill::Solid(Rgba::rgb(210, 30, 70)))
    });
    let mut shape = Shape::new(
        Geom::Text(run),
        Style {
            fill: Fill::Solid(Rgba::rgb(25, 110, 185)),
            stroke: None,
        },
    );
    shape.name = "Styled text".into();
    crate::text::fill_contours(&mut shape.geom);
    let hit = (0, shape.id);
    s.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
    s.selection = vec![hit];
    s.active_layer = Some(0);
    s.mark();
    s.history.clear();
    (s, hit)
}
fn run(s: &Studio, hit: (usize, u64)) -> &TypeRun {
    match &s.doc.find_shape(hit.0, hit.1).unwrap().geom {
        Geom::Text(r) => r,
        _ => panic!("lost live text"),
    }
}
fn wire(s: &Studio) -> String {
    crate::project::encode(&s.doc).unwrap()
}
fn apply(s: &mut Studio, op: TypeCommand) -> usize {
    let target = s.capture_type_command(TextScope::Selection).unwrap();
    s.apply_type_command(&target, op).unwrap()
}
fn reopen(s: &Studio) {
    let saved = wire(s);
    let doc = crate::project::decode(&saved).unwrap();
    assert_eq!(crate::project::encode(&doc).unwrap(), saved);
    assert!(
        crate::compositor::render_export(&doc, 1).unwrap().data()
            == crate::compositor::render_export(&s.doc, 1).unwrap().data()
    );
}
fn emit(name: &str, s: &Studio) {
    if let Some(root) = std::env::var_os("OMADESIGN_TYPE_CONTENT_QA") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(std::path::Path::new(&root).join(name), wire(s)).unwrap();
    }
}

#[test]
fn craft_type_content_unicode_case_keeps_styles_paragraphs_and_one_undo() {
    let (mut s, hit) = editor("straße café. NEXT!\nagain");
    if let Geom::Text(r) = &mut s.doc.find_shape_mut(hit.0, hit.1).unwrap().geom {
        r.paragraphs = vec![ParagraphStyle {
            start: 19,
            align: TextAlign::End,
            ..Default::default()
        }];
        r.manual_kern.insert(21, -1.5);
    }
    s.mark();
    let before = wire(&s);
    emit("case-before.oma", &s);
    assert_eq!(apply(&mut s, TypeCommand::Case(TextCase::Upper)), 1);
    assert_eq!(run(&s, hit).content, "STRASSE CAFÉ. NEXT!\nAGAIN");
    assert_eq!(run(&s, hit).paragraphs[0].start, 20);
    assert_eq!(run(&s, hit).paragraphs[0].align, TextAlign::End);
    for at in 0..7 {
        assert_eq!(
            run(&s, hit).character_style(at).fill,
            Some(Fill::Solid(Rgba::rgb(210, 30, 70)))
        );
    }
    assert_eq!(s.history.len(), 1);
    reopen(&s);
    let after = wire(&s);
    emit("case-after.oma", &s);
    s.undo();
    assert_eq!(wire(&s), before);
    s.redo();
    assert_eq!(wire(&s), after);
    for (case, expected) in [
        (TextCase::Lower, "strasse café. next!\nagain"),
        (TextCase::Title, "Strasse Café. Next!\nAgain"),
        (TextCase::Sentence, "Strasse café. Next!\nAgain"),
    ] {
        apply(&mut s, TypeCommand::Case(case));
        assert_eq!(run(&s, hit).content, expected);
    }
}

#[test]
fn craft_type_content_live_reversed_range_retains_caret_and_draft_history() {
    let (mut s, hit) = editor("straße / untouched");
    s.begin_type_edit(hit, Pt::new(35., 65.));
    {
        let edit = s.type_edit.as_mut().unwrap();
        edit.anchor = 6;
        edit.caret = 0;
    }
    let before = wire(&s);
    apply(&mut s, TypeCommand::Case(TextCase::Upper));
    assert_eq!(run(&s, hit).content, "STRASSE / untouched");
    let edit = s.type_edit.as_ref().unwrap();
    assert_eq!((edit.caret, edit.anchor), (0, 7));
    assert_eq!(s.history.len(), 0);
    s.commit_type_edit();
    assert_eq!(s.history.len(), 1);
    let after = wire(&s);
    reopen(&s);
    s.undo();
    assert_eq!(wire(&s), before);
    s.redo();
    assert_eq!(wire(&s), after);
}

#[test]
fn craft_type_content_punctuation_keeps_style_boundaries_paragraphs_and_kerning() {
    let (mut s, hit) = editor("\"Hi\" it's 1--2 and 3---4...\nkeep me");
    if let Geom::Text(r) = &mut s.doc.find_shape_mut(hit.0, hit.1).unwrap().geom {
        r.spans.clear();
        r.set_character_style(1, 3, |span| {
            span.fill = Some(Fill::Solid(Rgba::rgb(210, 30, 70)))
        });
        let start = r.content.find('\n').unwrap() + 1;
        r.paragraphs = vec![ParagraphStyle {
            start,
            align: TextAlign::Center,
            ..Default::default()
        }];
        r.manual_kern.insert(start + 3, 2.5);
    }
    s.mark();
    let before = wire(&s);
    apply(
        &mut s,
        TypeCommand::Punctuation(SmartPunctuation::default()),
    );
    let expected = "“Hi” it’s 1–2 and 3—4…\nkeep me";
    assert_eq!(run(&s, hit).content, expected);
    let at = expected.chars().position(|c| c == '\n').unwrap() + 1;
    assert_eq!(run(&s, hit).paragraphs[0].start, at);
    assert_eq!(run(&s, hit).manual_kern.get(&(at + 3)), Some(&2.5));
    for n in 1..3 {
        assert_eq!(
            run(&s, hit).character_style(n).fill,
            Some(Fill::Solid(Rgba::rgb(210, 30, 70)))
        );
    }
    reopen(&s);
    emit("punctuation.oma", &s);
    s.undo();
    assert_eq!(wire(&s), before);
    let shape = s.doc.find_shape_mut(hit.0, hit.1).unwrap();
    let Geom::Text(r) = &mut shape.geom else {
        unreachable!()
    };
    r.content = "--...".into();
    r.spans.clear();
    r.paragraphs.clear();
    r.manual_kern.clear();
    r.set_character_style(1, 2, |span| span.px = Some(31.));
    s.mark();
    apply(
        &mut s,
        TypeCommand::Punctuation(SmartPunctuation {
            quotes: false,
            ..Default::default()
        }),
    );
    assert_eq!(run(&s, hit).content, "--…");
}

#[test]
fn craft_type_content_all_insertions_use_native_unicode_and_live_pending_style() {
    let (mut s, hit) = editor("A");
    let text: String = SPECIAL_CHARACTERS
        .iter()
        .chain(WHITESPACE_CHARACTERS)
        .chain(BREAK_CHARACTERS)
        .map(|(_, text)| *text)
        .collect();
    assert_eq!(
        text.chars().map(u32::from).collect::<Vec<_>>(),
        vec![
            0x2022, 0x00a9, 0x00ae, 0x2122, 0x2026, 0x00b6, 0x00a7, 0x2014, 0x2013, 0x00ad, 0x2011,
            0x201c, 0x201d, 0x2018, 0x2019, 0x2003, 0x2002, 0x200a, 0x2006, 0x2009, 0x00a0, 0x2007,
            0x2008, 0x2004, 0x2005, 0x0009, 0x000a, 0x000a
        ]
    );
    assert_eq!(
        SPECIAL_CHARACTERS.len() + WHITESPACE_CHARACTERS.len() + BREAK_CHARACTERS.len(),
        28
    );
    let before = wire(&s);
    apply(&mut s, TypeCommand::Insert(text.clone()));
    assert_eq!(run(&s, hit).content, format!("A{text}"));
    assert_eq!(s.history.len(), 1);
    reopen(&s);
    s.undo();
    assert_eq!(wire(&s), before);
    s.begin_type_edit(hit, Pt::new(35., 65.));
    {
        let e = s.type_edit.as_mut().unwrap();
        e.caret = 1;
        e.anchor = 1;
    }
    s.patch_character(|span| span.fill = Some(Fill::Solid(Rgba::rgb(20, 170, 80))));
    apply(&mut s, TypeCommand::Insert("©".into()));
    assert!(s.type_edit.as_ref().unwrap().pending_style.is_none());
    assert_eq!(run(&s, hit).content, "A©");
    assert_eq!(
        s.type_edit.as_ref().map(|e| (e.caret, e.anchor)),
        Some((2, 2))
    );
    assert_eq!(
        run(&s, hit).character_style(1).fill,
        Some(Fill::Solid(Rgba::rgb(20, 170, 80)))
    );
    s.commit_type_edit();
    reopen(&s);
}

#[test]
fn craft_type_content_document_deduplicates_hidden_linked_stories_and_stale_targets() {
    let (mut s, hit) = editor("\"First\" -- one...");
    s.convert_text_frame(true);
    let next = s.empty_thread_frame(hit, Pt::new(250., 25.)).unwrap();
    s.thread_text_frames(hit, next).unwrap();
    let (other, _) = editor("\"Other\" --- two...");
    let mut second = other.doc.layers[0].kind.shapes().unwrap()[0].clone();
    second.id = crate::document::next_id();
    second.visible = false;
    s.doc.layers[0].kind.shapes_mut().unwrap().push(second);
    s.selection = vec![next];
    s.mark();
    s.history.clear();
    let before = wire(&s);
    let target = s.capture_type_command(TextScope::Document).unwrap();
    assert_eq!(target.hits.len(), 2);
    assert_eq!(
        s.apply_type_command(
            &target,
            TypeCommand::Punctuation(SmartPunctuation::default())
        )
        .unwrap(),
        2
    );
    assert!(run(&s, hit).content.starts_with("“First” –"));
    assert_eq!(s.history.len(), 1);
    reopen(&s);
    assert!(
        s.apply_type_command(&target, TypeCommand::Case(TextCase::Upper))
            .is_err()
    );
    s.undo();
    assert_eq!(wire(&s), before);
    let target = s.capture_type_command(TextScope::Selection).unwrap();
    s.doc.find_shape_mut(hit.0, hit.1).unwrap().locked = true;
    assert!(!s.type_command_current(&target));
    let locked = wire(&s);
    assert!(
        s.apply_type_command(&target, TypeCommand::Case(TextCase::Upper))
            .is_err()
    );
    assert_eq!(wire(&s), locked);
}

#[test]
fn craft_type_content_noop_keeps_live_ime_and_redo_while_changed_owners_refuse() {
    let (mut s, hit) = editor("ALREADY UPPER");
    s.commit(Cmd::SetOpacity {
        layer: 0,
        id: hit.1,
        before: 1.,
        after: 0.7,
    });
    s.undo();
    s.begin_type_edit(hit, Pt::new(35., 65.));
    s.type_edit.as_mut().unwrap().ime.preedit = Some(super::super::type_ime::Preedit {
        text: "ni".into(),
        active: Some(0..2),
    });
    let before = wire(&s);
    assert_eq!(apply(&mut s, TypeCommand::Case(TextCase::Upper)), 0);
    assert_eq!(wire(&s), before);
    assert!(s.history.can_redo());
    assert_eq!(
        s.type_edit
            .as_ref()
            .unwrap()
            .ime
            .preedit
            .as_ref()
            .unwrap()
            .text,
        "ni"
    );
    let target = s.capture_type_command(TextScope::Selection).unwrap();
    s.type_edit.as_mut().unwrap().caret += 1;
    assert!(
        s.apply_type_command(&target, TypeCommand::Insert("x".into()))
            .is_err()
    );
    assert_eq!(wire(&s), before);
    s.type_edit.as_mut().unwrap().caret -= 1;
    s.updates.freezing = true;
    assert!(s.capture_type_command(TextScope::Document).is_err());
}

#[test]
fn craft_type_content_imported_svg_retains_tspans_paints_and_positions() {
    let (mut s, _) = editor("unused");
    let (doc,_)=crate::formats::svg::read(r##"<svg xmlns="http://www.w3.org/2000/svg" width="260" height="100"><text x="10" y="50" font-size="28" fill="#cc2244">straße <tspan fill="#2266cc" dx="3">café</tspan></text></svg>"##,"Styled SVG").unwrap();
    s.doc = doc;
    s.doc.workspace = Some(Persona::Layout);
    let hit = s
        .doc
        .layers
        .iter()
        .enumerate()
        .find_map(|(layer, l)| {
            l.kind
                .shapes()
                .unwrap_or(&[])
                .iter()
                .find(|s| matches!(s.geom, Geom::Text(_)))
                .map(|s| (layer, s.id))
        })
        .unwrap();
    s.selection = vec![hit];
    s.active_layer = Some(hit.0);
    s.mark();
    s.history.clear();
    let before = wire(&s);
    apply(&mut s, TypeCommand::Case(TextCase::Upper));
    assert_eq!(run(&s, hit).content, "STRASSE CAFÉ");
    assert!(run(&s, hit).svg_text.is_some());
    let exported = crate::svg::export(&s.doc).unwrap();
    assert!(exported.contains("tspan"));
    assert!(exported.contains("dx=\"3\""));
    let parsed = crate::text::svg_type::layout(run(&s, hit)).unwrap();
    assert!(
        parsed
            .styles
            .iter()
            .any(|(_, style)| style.fill == Some(Fill::Solid(Rgba::rgb(34, 102, 204))))
    );
    reopen(&s);
    s.undo();
    assert_eq!(wire(&s), before);
}

#[test]
fn craft_type_content_batch_rewrite_refuses_invalid_ranges_and_preserves_large_style_runs() {
    let mut run = TypeRun {
        content: "aß\nabc".into(),
        ..Default::default()
    };
    run.set_character_style(1, 2, |s| s.px = Some(35.));
    for edits in [
        vec![CharacterRewrite {
            start: 2,
            end: 3,
            text: "x".into(),
        }],
        vec![
            CharacterRewrite {
                start: 0,
                end: 2,
                text: "X".into(),
            },
            CharacterRewrite {
                start: 1,
                end: 2,
                text: "Y".into(),
            },
        ],
        vec![CharacterRewrite {
            start: 0,
            end: 100,
            text: "x".into(),
        }],
    ] {
        let before = run.clone();
        assert!(run.rewrite_characters(&edits).is_err());
        assert_eq!(run, before);
    }
    run.rewrite_characters(&[CharacterRewrite {
        start: 1,
        end: 2,
        text: "SS".into(),
    }])
    .unwrap();
    assert_eq!(run.content, "aSS\nabc");
    for at in 1..3 {
        assert_eq!(run.character_style(at).px, Some(35.));
    }
    run.content = "abß ".repeat(5000);
    run.spans.clear();
    run.set_character_style(0, 20000, |s| s.px = Some(24.));
    let edits = rewrites(
        &run,
        0..run.content.chars().count(),
        &TypeCommand::Case(TextCase::Upper),
    );
    run.rewrite_characters(&edits).unwrap();
    assert_eq!(run.content, "ABSS ".repeat(5000));
    assert_eq!(run.spans.len(), 1);
    assert_eq!(run.spans[0].end, 25000);
}

#[test]
fn craft_type_content_rotated_point_area_path_and_affine_type_keep_the_original_frame() {
    for mode in 0..4 {
        let (mut s, hit) = editor("small mixed letters become wider");
        if mode == 1 {
            s.convert_text_frame(true);
        }
        if mode == 2 {
            let guide = Shape::new(
                Geom::Line {
                    a: Pt::new(30., 150.),
                    b: Pt::new(450., 105.),
                },
                Style {
                    fill: Fill::None,
                    stroke: None,
                },
            );
            let id = guide.id;
            s.doc.layers[0].kind.shapes_mut().unwrap().push(guide);
            s.selection = vec![hit, (0, id)];
            s.attach_text_path();
            s.selection = vec![hit];
        }
        let shape = s.doc.find_shape_mut(hit.0, hit.1).unwrap();
        shape.rotation = 0.37;
        if mode == 3
            && let Geom::Text(r) = &mut shape.geom
        {
            r.apply_affine([1.1, 0.13, 0.2, 0.9, 3., -4.]);
        }
        s.mark();
        s.history.clear();
        let source = s.doc.find_shape(hit.0, hit.1).unwrap().clone();
        let Geom::Text(r) = &source.geom else {
            unreachable!()
        };
        let point = r.origin;
        let anchor = r
            .map_point(point)
            .rotate_about(source.geom.bbox().center(), source.rotation);
        let before = wire(&s);
        apply(&mut s, TypeCommand::Case(TextCase::Upper));
        let after = s.doc.find_shape(hit.0, hit.1).unwrap();
        let Geom::Text(r) = &after.geom else {
            unreachable!()
        };
        let actual = r
            .map_point(point)
            .rotate_about(after.geom.bbox().center(), after.rotation);
        assert!(
            (anchor - actual).length() < 0.0001,
            "mode {mode}: {anchor:?} → {actual:?}"
        );
        assert_eq!(after.rotation, source.rotation);
        assert_eq!(after.style, source.style);
        assert_eq!(s.history.len(), 1);
        reopen(&s);
        s.undo();
        assert_eq!(wire(&s), before);
    }
}
