use super::*;
use crate::{
    color::Rgba,
    document::{Document, Layer},
    geom::{ParagraphStyle, TextAlign},
};

fn editor(texts: &[&str]) -> (Studio, Vec<(usize, u64)>) {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.persona = Persona::Layout;
    s.doc = Document::new("Find and Replace", 500., 240., 96.);
    s.doc.workspace = Some(Persona::Layout);
    s.doc.layers = vec![Layer::vector("Live type")];
    let mut hits = Vec::new();
    for (i, text) in texts.iter().enumerate() {
        let run = TypeRun {
            content: (*text).into(),
            origin: Pt::new(30., 65. + i as f32 * 65.),
            px: 24.,
            font: concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/assets/fonts/EBGaramond.ttf"
            )
            .into(),
            ..Default::default()
        };
        let mut shape = Shape::new(
            Geom::Text(run),
            Style {
                fill: Fill::Solid(Rgba::from_hex(0x174a80)),
                stroke: None,
            },
        );
        crate::text::fill_contours(&mut shape.geom);
        hits.push((0, shape.id));
        s.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
    }
    s.selection = hits.first().copied().into_iter().collect();
    s.active_layer = Some(0);
    s.mark();
    s.history.clear();
    (s, hits)
}
fn run(s: &Studio, hit: (usize, u64)) -> &TypeRun {
    let Geom::Text(run) = &s.doc.find_shape(hit.0, hit.1).unwrap().geom else {
        panic!("editable type")
    };
    run
}
fn wire(s: &Studio) -> String {
    crate::project::encode(&s.doc).unwrap()
}
fn query(find: &str) -> Query {
    Query {
        find: find.into(),
        ..Default::default()
    }
}
fn replace(s: &mut Studio, q: &Query, text: &str) -> Replaced {
    let target = s.capture_text_search(TextScope::Document).unwrap();
    s.replace_text_search(&target, q, text, None).unwrap()
}
fn reopen(s: &Studio) {
    let bytes = wire(s);
    let doc = crate::project::decode(&bytes).unwrap();
    assert_eq!(crate::project::encode(&doc).unwrap(), bytes);
    assert_eq!(
        crate::compositor::render_export(&doc, 1).unwrap().data(),
        crate::compositor::render_export(&s.doc, 1).unwrap().data()
    );
}
fn emit(name: &str, s: &Studio) {
    if let Some(root) = std::env::var_os("OMADESIGN_FIND_REPLACE_QA") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(std::path::Path::new(&root).join(name), wire(s)).unwrap();
    }
}

#[test]
fn craft_find_replace_reference_literal_case_whole_words_styles_and_noop_redo() {
    let (mut s, hits) = editor(&["Cat cat catalog", "the CAT"]);
    let mut q = query("cat");
    q.whole_word = true;
    let before = wire(&s);
    emit("before.oma", &s);
    assert_eq!(
        replace(&mut s, &q, "dog"),
        Replaced {
            matches: 3,
            stories: 2
        }
    );
    assert_eq!(run(&s, hits[0]).content, "dog dog catalog");
    assert_eq!(run(&s, hits[1]).content, "the dog");
    assert_eq!(s.history.len(), 1);
    reopen(&s);
    emit("after.oma", &s);
    let after = wire(&s);
    s.undo();
    assert_eq!(wire(&s), before);
    s.redo();
    assert_eq!(wire(&s), after);
    q.find = "Dog".into();
    q.match_case = true;
    assert_eq!(replace(&mut s, &q, "x").matches, 0);
    assert_eq!(wire(&s), after);
    s.undo();
    let n = s.history.len();
    assert!(s.history.can_redo());
    q = query("CAT");
    q.whole_word = true;
    assert_eq!(
        replace(&mut s, &q, "cat"),
        Replaced {
            matches: 3,
            stories: 2
        }
    ); // Only the existing capitalized occurrences change.
    assert_eq!(s.history.len(), n + 1);
    s.undo();
    let target = s.capture_text_search(TextScope::Document).unwrap();
    let n = s.history.len();
    q = query("missing");
    let original = wire(&s);
    assert_eq!(
        s.replace_text_search(&target, &q, "", None)
            .unwrap()
            .stories,
        0
    );
    assert_eq!(wire(&s), original);
    assert!(s.history.can_redo());
    assert_eq!(s.history.len(), n);
    let (mut s, _) = editor(&["cat cat"]);
    s.commit(Cmd::SetOpacity {
        layer: 0,
        id: s.selection[0].1,
        before: 1.,
        after: 0.7,
    });
    s.undo();
    let n = s.history.len();
    let original = wire(&s);
    assert_eq!(
        replace(&mut s, &query("CAT"), "cat"),
        Replaced {
            matches: 2,
            stories: 0
        }
    );
    assert_eq!(wire(&s), original);
    assert_eq!(s.history.len(), n);
    assert!(s.history.can_redo());
}

#[test]
fn craft_find_replace_unicode_style_leaves_and_overlapping_patterns_keep_native_ranges() {
    let (mut s, hits) = editor(&["Äpfel äPFEL catcat cat catalog _cat cat_ Σσς café cafe\u{301}"]);
    let target = s.capture_text_search(TextScope::Document).unwrap();
    assert_eq!(
        s.text_search_matches(&target, &query("äpfel"))
            .unwrap()
            .len(),
        2
    );
    let mut q = query("cat");
    q.whole_word = true;
    assert_eq!(s.text_search_matches(&target, &q).unwrap().len(), 1);
    let mut run = TypeRun {
        content: "catcat".into(),
        ..Default::default()
    };
    run.set_character_style(0, 3, |style| {
        style.fill = Some(Fill::Solid(Rgba::from_hex(0xff0000)))
    });
    assert!(ranges(&run, 0..6, &query("catcat")).is_empty());
    assert_eq!(ranges(&run, 0..6, &query("cat")), vec![0..3, 3..6]);
    let red = run.character_style(0).fill.clone();
    run.rewrite_text_ranges(&rewrites(
        &run,
        0..6,
        &Replacement {
            query: query("cat"),
            text: "🐈".into(),
            only: None,
        },
    ))
    .unwrap();
    assert_eq!(run.content, "🐈🐈");
    assert_eq!(run.character_style(0).fill, red);
    assert_ne!(run.character_style(1).fill, red);
    let long = TypeRun {
        content: format!("{}b", "a".repeat(40_000)),
        ..Default::default()
    };
    let needle = format!("{}b", "a".repeat(2000));
    assert_eq!(
        ranges(&long, 0..40_001, &query(&needle)),
        vec![38_000..40_001]
    );
    let _ = &mut s;
    assert_eq!(run.content.chars().count(), 2);
    assert!(s.doc.find_shape(hits[0].0, hits[0].1).is_some());
}

#[test]
fn craft_find_replace_paragraph_insert_delete_metadata_and_empty_live_commit() {
    let (mut s, hits) = editor(&["cat\nX"]);
    if let Geom::Text(run) = &mut s.doc.find_shape_mut(0, hits[0].1).unwrap().geom {
        run.paragraphs = vec![ParagraphStyle {
            start: 4,
            align: TextAlign::End,
            ..Default::default()
        }];
        run.manual_kern.insert(5, 1.25);
    }
    s.mark();
    let before = wire(&s);
    assert_eq!(replace(&mut s, &query("cat"), "dog\nnew").matches, 1);
    assert_eq!(run(&s, hits[0]).content, "dog\nnew\nX");
    assert_eq!(run(&s, hits[0]).paragraphs[0].start, 8);
    assert_eq!(run(&s, hits[0]).paragraphs[0].align, TextAlign::End);
    assert_eq!(run(&s, hits[0]).manual_kern.get(&9), Some(&1.25));
    reopen(&s);
    s.undo();
    assert_eq!(wire(&s), before);
    assert_eq!(replace(&mut s, &query("\n"), "").matches, 1);
    assert_eq!(run(&s, hits[0]).content, "catX");
    assert!(run(&s, hits[0]).paragraphs.is_empty());
    reopen(&s);
    let (mut s, hits) = editor(&["cat"]);
    let before = wire(&s);
    s.begin_type_edit(hits[0], Pt::new(30., 65.));
    {
        let e = s.type_edit.as_mut().unwrap();
        e.anchor = 0;
        e.caret = 3;
    }
    let target = s.capture_text_search(TextScope::Selection).unwrap();
    assert_eq!(
        s.replace_text_search(&target, &query("cat"), "", None)
            .unwrap(),
        Replaced {
            matches: 1,
            stories: 1
        }
    );
    assert_eq!(run(&s, hits[0]).content, "");
    assert_eq!(s.history.len(), 0);
    s.commit_type_edit();
    assert_eq!(run(&s, hits[0]).content, "");
    assert_eq!(s.history.len(), 1);
    reopen(&s);
    s.undo();
    assert_eq!(wire(&s), before);
}

#[test]
fn craft_find_replace_visible_scope_selection_one_match_and_stale_targets_are_atomic() {
    let (mut s, hits) = editor(&["cat cat", "cat hidden", "cat locked"]);
    s.doc.find_shape_mut(0, hits[1].1).unwrap().visible = false;
    s.doc.find_shape_mut(0, hits[2].1).unwrap().locked = true;
    s.mark();
    let target = s.capture_text_search(TextScope::Document).unwrap();
    let q = query("cat");
    let found = s.text_search_matches(&target, &q).unwrap();
    assert_eq!(found.len(), 2);
    let before = wire(&s);
    let wrong = TextMatch {
        hit: hits[1],
        range: 0..3,
    };
    assert!(
        s.replace_text_search(&target, &q, "dog", Some(&wrong))
            .is_err()
    );
    assert_eq!(wire(&s), before);
    assert_eq!(
        s.replace_text_search(&target, &q, "dog", Some(&found[1]))
            .unwrap()
            .matches,
        1
    );
    assert_eq!(run(&s, hits[0]).content, "cat dog");
    assert_eq!(run(&s, hits[1]).content, "cat hidden");
    assert_eq!(run(&s, hits[2]).content, "cat locked");
    assert!(s.replace_text_search(&target, &q, "x", None).is_err());
    let after = wire(&s);
    let target = s.capture_text_search(TextScope::Selection).unwrap();
    s.selection = vec![hits[1]];
    assert!(s.replace_text_search(&target, &q, "x", None).is_err());
    assert_eq!(wire(&s), after);
    s.selection = vec![hits[0]];
    let target = s.capture_text_search(TextScope::Selection).unwrap();
    s.doc.find_shape_mut(0, hits[0].1).unwrap().locked = true;
    let before = wire(&s);
    assert!(s.replace_text_search(&target, &q, "x", None).is_err());
    assert_eq!(wire(&s), before);
}

#[test]
fn craft_find_replace_live_reversed_selection_and_svg_leaves_keep_history_and_styles() {
    let (mut s, hits) = editor(&["cat cat cat"]);
    let before = wire(&s);
    s.begin_type_edit(hits[0], Pt::new(30., 65.));
    {
        let e = s.type_edit.as_mut().unwrap();
        e.anchor = 7;
        e.caret = 0;
    }
    let target = s.capture_text_search(TextScope::Selection).unwrap();
    assert_eq!(
        s.text_search_matches(&target, &query("cat")).unwrap().len(),
        2
    );
    assert_eq!(
        s.replace_text_search(&target, &query("cat"), "🐈", None)
            .unwrap()
            .matches,
        2
    );
    assert_eq!(run(&s, hits[0]).content, "🐈 🐈 cat");
    let e = s.type_edit.as_ref().unwrap();
    assert_eq!((e.caret, e.anchor), (0, 3));
    assert_eq!(s.history.len(), 0);
    s.commit_type_edit();
    assert_eq!(s.history.len(), 1);
    reopen(&s);
    s.undo();
    assert_eq!(wire(&s), before);
    let(doc,_)=crate::formats::svg::read(r#"<svg xmlns="http://www.w3.org/2000/svg" width="300" height="90"><text x="10" y="50" font-size="28"><tspan fill="red">cat</tspan><tspan fill="blue">cat</tspan></text></svg>"#,"Search").unwrap();
    s.doc = doc;
    s.type_edit = None;
    s.selection.clear();
    s.mark();
    s.history.clear();
    let target = s.capture_text_search(TextScope::Document).unwrap();
    assert!(
        s.text_search_matches(&target, &query("catcat"))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        s.replace_text_search(&target, &query("cat"), "dog", None)
            .unwrap()
            .matches,
        2
    );
    reopen(&s);
}

#[test]
fn craft_find_replace_next_story_wraps_and_search_alone_does_not_record_history() {
    let (mut s, hits) = editor(&["apple", "banana", "pineapple"]);
    s.selection.clear();
    let original = wire(&s);
    for expected in [hits[0], hits[2], hits[0]] {
        let found = s.find_next_text_story(&query("APPLE")).unwrap().unwrap();
        assert_eq!(found.hit, expected);
        assert_eq!(s.selection, vec![expected]);
        assert_eq!(wire(&s), original);
        assert_eq!(s.history.len(), 0);
    }
    assert!(s.find_next_text_story(&query("zzz")).unwrap().is_none());
    assert!(s.find_next_text_story(&query("")).is_err());
    assert_eq!(wire(&s), original);
}

#[test]
fn craft_find_replace_malformed_range_and_paragraph_metadata_refuse_without_mutation() {
    let mut run = TypeRun {
        content: "cat\ncat".into(),
        ..Default::default()
    };
    let before = run.clone();
    assert!(
        run.rewrite_text_ranges(&[
            CharacterRewrite {
                start: 4,
                end: 7,
                text: "x".into()
            },
            CharacterRewrite {
                start: 0,
                end: 3,
                text: "y".into()
            }
        ])
        .is_err()
    );
    assert_eq!(run, before);
    run.paragraphs.push(ParagraphStyle {
        start: 500,
        ..Default::default()
    });
    let before = run.clone();
    assert!(
        run.rewrite_text_ranges(&[CharacterRewrite {
            start: 0,
            end: 3,
            text: "x\ny".into()
        }])
        .is_err()
    );
    assert_eq!(run, before);
}

#[test]
fn craft_find_replace_linked_story_is_replaced_once_and_locked_members_refuse() {
    let (mut s, hits) = editor(&["cat cat cat"]);
    s.convert_text_frame(true);
    let next = s.empty_thread_frame(hits[0], Pt::new(270., 30.)).unwrap();
    s.thread_text_frames(hits[0], next).unwrap();
    s.history.clear();
    s.selection = vec![next];
    let before = wire(&s);
    let target = s.capture_text_search(TextScope::Document).unwrap();
    let matches = s.text_search_matches(&target, &query("cat")).unwrap();
    assert_eq!(matches.len(), 3);
    assert!(matches.iter().all(|m| m.hit == hits[0]));
    assert_eq!(
        s.replace_text_search(&target, &query("cat"), "dog", None)
            .unwrap(),
        Replaced {
            matches: 3,
            stories: 1
        }
    );
    assert_eq!(run(&s, hits[0]).content, "dog dog dog");
    assert_eq!(s.history.len(), 1);
    reopen(&s);
    s.undo();
    assert_eq!(wire(&s), before);
    let target = s.capture_text_search(TextScope::Selection).unwrap();
    s.doc.find_shape_mut(next.0, next.1).unwrap().locked = true;
    let locked = wire(&s);
    assert!(
        s.replace_text_search(&target, &query("cat"), "x", None)
            .is_err()
    );
    assert_eq!(wire(&s), locked);
    assert!(s.capture_text_search(TextScope::Document).is_err());
}

#[test]
fn craft_find_replace_whole_word_selection_uses_actual_word_and_style_boundaries() {
    let (mut s, hits) = editor(&["catalog cat"]);
    s.begin_type_edit(hits[0], Pt::new(30., 65.));
    {
        let e = s.type_edit.as_mut().unwrap();
        e.anchor = 0;
        e.caret = 3;
    }
    let target = s.capture_text_search(TextScope::Selection).unwrap();
    let q = Query {
        find: "cat".into(),
        whole_word: true,
        match_case: false,
    };
    let before = wire(&s);
    assert!(s.text_search_matches(&target, &q).unwrap().is_empty());
    assert_eq!(
        s.replace_text_search(&target, &q, "dog", None)
            .unwrap()
            .matches,
        0
    );
    assert_eq!(wire(&s), before);
    let plain = Query {
        whole_word: false,
        ..q.clone()
    };
    assert_eq!(s.text_search_matches(&target, &plain).unwrap().len(), 1);
    let mut run = TypeRun {
        content: "catalog".into(),
        ..Default::default()
    };
    run.set_character_style(0, 3, |style| {
        style.fill = Some(Fill::Solid(Rgba::from_hex(0xff0000)))
    });
    assert_eq!(
        ranges(&run, 0..7, &q),
        vec![0..3],
        "true style-run edges retain the reference contract"
    );
}

#[test]
fn craft_find_replace_batched_paragraph_rewrites_match_existing_literal_edit_semantics() {
    let mut source = TypeRun {
        content: "AB\nCD\nEF".into(),
        ..Default::default()
    };
    source.paragraphs = vec![
        ParagraphStyle {
            start: 0,
            align: TextAlign::Center,
            ..Default::default()
        },
        ParagraphStyle {
            start: 3,
            align: TextAlign::End,
            ..Default::default()
        },
        ParagraphStyle {
            start: 6,
            keep_with_next: 2,
            ..Default::default()
        },
    ];
    source.set_character_style(0, 3, |style| {
        style.fill = Some(Fill::Solid(Rgba::from_hex(0xdd2255)))
    });
    source.set_character_style(3, 6, |style| style.px = Some(32.));
    source.manual_kern.insert(5, 1.5);
    let edits = [
        CharacterRewrite {
            start: 1,
            end: 4,
            text: "x\ny".into(),
        },
        CharacterRewrite {
            start: 6,
            end: 8,
            text: "\nlast".into(),
        },
    ];
    let mut expected = source.clone();
    for edit in edits.iter().rev() {
        let inherited = expected.character_style(edit.start);
        expected.replace_text(edit.start, edit.end, &edit.text, Some(inherited));
    }
    source.rewrite_text_ranges(&edits).unwrap();
    assert_eq!(source.content, expected.content);
    assert_eq!(source.paragraphs, expected.paragraphs);
    assert_eq!(source.manual_kern, expected.manual_kern);
    for i in 0..source.content.chars().count() {
        assert_eq!(
            source.character_style(i),
            expected.character_style(i),
            "scalar style {i}"
        );
    }
}
