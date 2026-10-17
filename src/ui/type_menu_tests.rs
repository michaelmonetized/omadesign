use super::{pixel_menu_tests::Seat, *};
use crate::{
    document::{Document, Fill, Layer, Shape, Style},
    geom::{CharSpan, Geom, Pt, TypeRun},
    text_geometry::TextFrame,
    ui::welcome::keyboard_tests as keys,
};
type Hit = (usize, u64);

fn editor(count: usize) -> (Studio, Vec<Hit>) {
    let mut studio = Studio::new();
    studio.show_welcome = false;
    studio.persona = Persona::Layout;
    studio.doc = Document::new("Type command workflow", 600., 280., 96.);
    studio.doc.workspace = Some(Persona::Layout);
    studio.doc.layers = vec![Layer::vector("Editable type")];
    let mut ids = Vec::new();
    for index in 0..count {
        let mut run = TypeRun {
            origin: Pt::new(24. + index as f32 * 184., 54.),
            content: if index == 0 {
                "Café keeps every character and its colour when this story flows across three linked frames. ".repeat(3)
            } else {
                String::new()
            },
            font: concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/assets/fonts/EBGaramond.ttf"
            )
            .into(),
            px: 20.,
            frame: Some(TextFrame {
                size: Pt::new(152., 90.),
                ..Default::default()
            }),
            ..Default::default()
        };
        if index == 0 {
            run.set_character_style(5, 20, |style| {
                style.fill = Some(Fill::Solid(crate::color::Rgba::rgb(215, 44, 83)));
                style.tracking = Some(18.);
            });
        }
        let mut shape = Shape::new(Geom::Text(run), Style::default());
        shape.name = format!("Text frame {}", index + 1);
        crate::text::fill_contours(&mut shape.geom);
        ids.push((0, shape.id));
        studio.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
    }
    studio.active_layer = Some(0);
    studio.selection = vec![ids[0]];
    studio.mark();
    studio.history.clear();
    (studio, ids)
}
fn threaded() -> (Studio, Vec<Hit>) {
    let (mut studio, ids) = editor(3);
    studio.thread_text_frames(ids[0], ids[1]).unwrap();
    studio.thread_text_frames(ids[1], ids[2]).unwrap();
    studio.selection = vec![ids[1]];
    studio.history.clear();
    (studio, ids)
}
fn run(studio: &Studio, hit: Hit) -> &TypeRun {
    let Geom::Text(run) = &studio.doc.find_shape(hit.0, hit.1).unwrap().geom else {
        panic!("text target lost");
    };
    run
}
fn styles(run: &TypeRun) -> Vec<CharSpan> {
    (0..run.content.chars().count())
        .map(|index| {
            let mut style = crate::text::character_metrics(run, index);
            style.start = 0;
            style.end = 0;
            style
        })
        .collect()
}
fn saved(studio: &Studio) -> String {
    crate::project::encode(&studio.doc).unwrap()
}
fn reopen(studio: &Studio) {
    let output = saved(studio);
    let doc = crate::project::decode(&output).unwrap();
    assert_eq!(crate::project::encode(&doc).unwrap(), output);
    assert_eq!(
        crate::compositor::export_png(&doc, 1).unwrap(),
        crate::compositor::export_png(&studio.doc, 1).unwrap()
    );
}
fn emit(name: &str, studio: &Studio) {
    if let Some(root) = std::env::var_os("OMADESIGN_TYPE_MENUS_QA") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(std::path::Path::new(&root).join(name), saved(studio)).unwrap();
    }
}

#[test]
fn craft_type_command_menus_linked_point_conversion_preserves_story_in_one_undo() {
    let (mut studio, ids) = threaded();
    let original = run(&studio, ids[0]).content.clone();
    let original_styles = styles(run(&studio, ids[0]));
    assert!(run(&studio, ids[1]).layout.as_ref().unwrap().visible_end > 0);
    let before = saved(&studio);
    studio.convert_text_frame(false);
    assert_eq!(studio.history.len(), 1);
    assert!(run(&studio, ids[1]).frame.is_none());
    assert!(run(&studio, ids[1]).thread.is_none());
    assert_eq!(
        run(&studio, ids[1]).contours,
        crate::text::shape(run(&studio, ids[1]))
    );
    assert_eq!(
        ids.iter()
            .map(|&hit| run(&studio, hit).content.as_str())
            .collect::<String>(),
        original
    );
    assert_eq!(
        ids.iter()
            .flat_map(|&hit| styles(run(&studio, hit)))
            .collect::<Vec<_>>(),
        original_styles
    );
    let after = saved(&studio);
    reopen(&studio);
    studio.undo();
    assert_eq!(saved(&studio), before);
    studio.redo();
    assert_eq!(saved(&studio), after);
}

#[test]
fn craft_type_command_menus_structure_refuses_locked_hidden_missing_and_busy_stories() {
    for case in 0..5 {
        let (mut studio, ids) = threaded();
        studio.commit(crate::document::Cmd::SetOpacity {
            layer: ids[0].0,
            id: ids[0].1,
            before: 1.,
            after: 0.7,
        });
        studio.undo();
        assert!(studio.history.can_redo());
        studio.begin_type_edit(ids[1], Pt::new(210., 62.));
        studio.type_insert("pending ");
        match case {
            0 => {
                studio
                    .doc
                    .find_shape_mut(ids[2].0, ids[2].1)
                    .unwrap()
                    .locked = true
            }
            1 => {
                studio
                    .doc
                    .find_shape_mut(ids[2].0, ids[2].1)
                    .unwrap()
                    .visible = false
            }
            2 => studio.doc.layers[0].locked = true,
            3 => studio.updates.freezing = true,
            _ => studio.doc.layers[0]
                .kind
                .shapes_mut()
                .unwrap()
                .retain(|shape| shape.id != ids[2].1),
        }
        let before = serde_json::to_value(&studio.doc).unwrap();
        let edit = studio.type_edit.clone().unwrap();
        let history = studio.history.len();
        assert!(!studio.text_structure_editable(ids[1]));
        studio.convert_text_frame(false);
        studio.break_text_thread(ids[1]);
        assert_eq!(serde_json::to_value(&studio.doc).unwrap(), before);
        assert_eq!(studio.history.len(), history);
        assert!(studio.history.can_redo());
        let current = studio.type_edit.as_ref().unwrap();
        assert_eq!(
            (current.layer, current.id, current.anchor, current.caret),
            (edit.layer, edit.id, edit.anchor, edit.caret)
        );
    }
}

#[test]
fn craft_type_command_menus_top_menu_converts_and_reopens_linked_text_without_object_submenu() {
    let (mut studio, ids) = threaded();
    let before = saved(&studio);
    emit("thread-before.oma", &studio);
    let mut seat = Seat::new();
    seat.frame(&mut studio, vec![]);
    seat.click(&mut studio, "Type");
    seat.invoke(&mut studio, "Convert to point text");
    assert!(run(&studio, ids[1]).frame.is_none());
    assert_eq!(studio.history.len(), 1);
    let point = saved(&studio);
    emit("thread-point.oma", &studio);
    reopen(&studio);
    seat.click(&mut studio, "Type");
    seat.invoke(&mut studio, "Convert to area text");
    assert!(run(&studio, ids[1]).frame.is_some());
    assert_eq!(studio.history.len(), 2);
    reopen(&studio);
    studio.undo();
    assert_eq!(saved(&studio), point);
    studio.undo();
    assert_eq!(saved(&studio), before);
    studio.redo();
    assert_eq!(saved(&studio), point);
}

#[test]
fn craft_type_command_menus_area_to_path_is_one_undo_and_release_remains_editable() {
    let (mut studio, ids) = editor(1);
    let guide = Shape::new(
        Geom::Line {
            a: Pt::new(24., 190.),
            b: Pt::new(570., 190.),
        },
        Style::default(),
    );
    let guide_id = guide.id;
    studio.doc.layers[0].kind.shapes_mut().unwrap().push(guide);
    studio.selection = vec![ids[0], (0, guide_id)];
    let before = saved(&studio);
    let content = run(&studio, ids[0]).content.clone();
    let mut seat = Seat::new();
    seat.frame(&mut studio, vec![]);
    seat.click(&mut studio, "Type");
    seat.invoke(&mut studio, "Attach text to path");
    let text = run(&studio, ids[0]);
    assert!(text.frame.is_none());
    assert_eq!(text.on_path.as_ref().unwrap().path_id, guide_id);
    assert_eq!(text.content, content);
    assert_eq!(studio.history.len(), 1);
    let attached = saved(&studio);
    emit("path-attached.oma", &studio);
    studio.undo();
    assert_eq!(saved(&studio), before);
    studio.redo();
    assert_eq!(saved(&studio), attached);
    seat.click(&mut studio, "Type");
    seat.invoke(&mut studio, "Release text from path");
    assert!(run(&studio, ids[0]).on_path.is_none());
    assert_eq!(run(&studio, ids[0]).content, content);
    assert_eq!(studio.history.len(), 2);
    let released = saved(&studio);
    reopen(&studio);
    studio.undo();
    assert_eq!(saved(&studio), attached);
    studio.redo();
    assert_eq!(saved(&studio), released);
}

#[test]
fn craft_type_command_menus_locked_story_disables_structural_top_menu_choices() {
    let (mut studio, ids) = threaded();
    studio
        .doc
        .find_shape_mut(ids[2].0, ids[2].1)
        .unwrap()
        .locked = true;
    let before = saved(&studio);
    let mut seat = Seat::new();
    seat.frame(&mut studio, vec![]);
    seat.click(&mut studio, "Type");
    assert!(
        keys::named(&seat.output, "Convert to point text")
            .1
            .is_disabled()
    );
    assert!(!studio.text_structure_editable(ids[1]));
    assert_eq!(saved(&studio), before);
    assert_eq!(studio.history.len(), 0);
}

#[test]
fn craft_painted_outlines_native_type_menu_creates_editable_paths_and_reopens() {
    let (mut studio, ids) = editor(1);
    let before = saved(&studio);
    emit("native-outlines-before.oma", &studio);
    let mut seat = Seat::new();
    seat.frame(&mut studio, vec![]);
    seat.click(&mut studio, "Type");
    seat.invoke(&mut studio, "Create outlines");
    assert!(
        studio
            .doc
            .find_shape(ids[0].0, ids[0].1)
            .unwrap()
            .layout
            .frame
    );
    assert!(
        studio.doc.layers[0]
            .kind
            .shapes()
            .unwrap()
            .iter()
            .all(|s| !matches!(s.geom, Geom::Text(_)))
    );
    assert!(
        studio.doc.layers[0]
            .kind
            .shapes()
            .unwrap()
            .iter()
            .any(|s| s.style.fill == Fill::Solid(crate::color::Rgba::rgb(215, 44, 83)))
    );
    assert_eq!(studio.history.len(), 1);
    let after = saved(&studio);
    reopen(&studio);
    emit("native-outlines-after.oma", &studio);
    studio.undo();
    assert_eq!(saved(&studio), before);
    studio.redo();
    assert_eq!(saved(&studio), after);
}

#[test]
fn craft_painted_outlines_native_type_menu_disables_locked_story_members() {
    let (mut studio, ids) = threaded();
    studio
        .doc
        .find_shape_mut(ids[2].0, ids[2].1)
        .unwrap()
        .locked = true;
    let before = saved(&studio);
    let mut seat = Seat::new();
    seat.frame(&mut studio, vec![]);
    seat.click(&mut studio, "Type");
    assert!(keys::named(&seat.output, "Create outlines").1.is_disabled());
    assert_eq!(saved(&studio), before);
    assert_eq!(studio.history.len(), 0);
}
