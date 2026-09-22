use super::*;
use crate::{
    document::{self, Document, Layer, Pixels, Shape, Style},
    geom::{Geom, Pt},
};
fn fixture() -> (Document, Vec<(usize, u64)>) {
    let mut doc = Document::new("Plugin test", 320., 240., 72.);
    doc.layers = vec![Layer::vector("Artwork")];
    let shape = Shape::new(
        Geom::Rect {
            origin: Pt::new(20., 20.),
            size: Pt::new(70., 50.),
            radius: 0.,
        },
        Style::default(),
    );
    let id = shape.id;
    doc.layers[0].kind.shapes_mut().unwrap().push(shape);
    (doc, vec![(0, id)])
}
fn starter() -> Plugin {
    inspect(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins/studio-starter/main.lua"))
        .unwrap()
}
fn exec(
    plugin: &Plugin,
    id: &str,
    doc: Document,
    selection: Vec<(usize, u64)>,
) -> Result<Output, String> {
    run(
        plugin,
        id,
        doc,
        selection,
        Some(0),
        serde_json::json!({}),
        None,
        Arc::new(AtomicBool::new(false)),
    )
}
fn temp() -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "oma-plugin-test-{}-{}",
        std::process::id(),
        crate::document::next_id()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}
fn script(root: &std::path::Path, body: &str) -> Plugin {
    let path = root.join("main.lua");
    std::fs::write(&path,format!("return {{api=1,id='test.plugin',name='Test',version='1',actions={{{{id='test',name='Test',run=function(ctx,p) {body} end}}}}}}" )).unwrap();
    inspect(&path).unwrap()
}
#[test]
fn starter_manifest_covers_every_contribution_category() {
    let p = starter();
    assert_eq!(p.actions.len(), 12);
    for category in [
        "Filters",
        "Effects",
        "Icons",
        "Brushes",
        "Tools",
        "Behaviors",
        "Batch",
        "Patterns",
        "Gradients",
        "Swatches",
    ] {
        assert!(
            p.actions.iter().any(|a| a.category == category),
            "{category}"
        );
    }
}
#[test]
fn plugin_batch_is_one_undoable_edit_and_roundtrips() {
    let (doc, selection) = fixture();
    let before = serde_json::to_string(&doc).unwrap();
    let out = exec(&starter(), "nudge", doc.clone(), selection.clone()).unwrap();
    let mut studio = crate::app::Studio::new();
    studio.doc = doc;
    studio.history.clear();
    studio.commit(Cmd::Batch(out.commands));
    assert_eq!(studio.history.len(), 1);
    assert_eq!(
        studio
            .doc
            .find_shape(0, selection[0].1)
            .unwrap()
            .geom
            .bbox()
            .min
            .x,
        36.
    );
    let saved = serde_json::to_string(&studio.doc).unwrap();
    let reopened: Document = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        reopened.find_shape(0, selection[0].1).unwrap().geom,
        studio.doc.find_shape(0, selection[0].1).unwrap().geom
    );
    studio.undo();
    assert_eq!(serde_json::to_string(&studio.doc).unwrap(), before);
    studio.redo();
    assert_eq!(serde_json::to_string(&studio.doc).unwrap(), saved);
}
#[test]
fn failure_after_an_edit_leaves_input_document_unchanged() {
    let root = temp();
    let p = script(&root, "oma.add_shape{kind='rect'}; error('stop here')");
    let (doc, selection) = fixture();
    let before = serde_json::to_string(&doc).unwrap();
    assert!(
        exec(&p, "test", doc.clone(), selection)
            .unwrap_err()
            .contains("stop here")
    );
    assert_eq!(serde_json::to_string(&doc).unwrap(), before);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn sandbox_has_no_process_network_files_or_binary_loading() {
    let root = temp();
    let p = script(
        &root,
        "assert(io==nil and os==nil and package==nil and require==nil and debug==nil and dofile==nil and loadfile==nil and load==nil and pcall==nil and xpcall==nil and coroutine==nil); oma.message('isolated')",
    );
    let (doc, selection) = fixture();
    assert_eq!(
        exec(&p, "test", doc, selection).unwrap().message,
        "isolated"
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn cancellation_interrupts_infinite_lua_and_releases_worker() {
    let root = temp();
    let p = script(&root, "while true do end");
    let (doc, selection) = fixture();
    let cancel = Arc::new(AtomicBool::new(true));
    let now = Instant::now();
    let result = run(
        &p,
        "test",
        doc,
        selection,
        Some(0),
        serde_json::json!({}),
        None,
        cancel,
    );
    assert!(result.is_err());
    assert!(now.elapsed() < Duration::from_secs(2));
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn plugin_pixel_filter_preserves_alpha_and_is_undoable() {
    let mut doc = Document::new("Pixels", 2., 1., 72.);
    let before = vec![255, 0, 0, 128, 100, 200, 255, 0];
    doc.layers = vec![Layer::placed_raster(
        "Pixels",
        Pixels::from_rgba(2, 1, before.clone()).unwrap(),
        Pt::ZERO,
        Pt::new(2., 1.),
    )];
    let out = exec(&starter(), "duotone", doc.clone(), vec![]).unwrap();
    let command = Cmd::Batch(out.commands);
    document::apply(&mut doc, &command);
    let after = &doc.layers[0].kind.pixels().unwrap().data;
    assert_ne!(after, &before);
    assert_eq!((after[3], after[7]), (128, 0));
    let mut history = document::History::default();
    history.push(command);
    document::apply(&mut doc, &history.undo().unwrap());
    assert_eq!(doc.layers[0].kind.pixels().unwrap().data, before);
}
#[test]
fn generated_patterns_svg_icons_gradients_and_effects_are_native() {
    let (doc, selection) = fixture();
    let p = starter();
    for action in ["dot-field", "icon-orbit", "aurora", "soft-shadow"] {
        let out = exec(&p, action, doc.clone(), selection.clone()).unwrap();
        assert!(!out.commands.is_empty(), "{action}");
        let mut changed = doc.clone();
        document::apply(&mut changed, &Cmd::Batch(out.commands));
        assert!(crate::compositor::export_png(&changed, 1).is_ok());
    }
}
#[test]
fn brush_palette_and_canvas_tool_outputs_are_real() {
    let (doc, selection) = fixture();
    let p = starter();
    let out = exec(&p, "soft-ink", doc.clone(), selection.clone()).unwrap();
    assert_eq!(out.brush.unwrap().hardness, 0.25);
    let out = exec(&p, "night-palette", doc.clone(), selection.clone()).unwrap();
    let root = temp();
    let path = root.join("palette.omacolors");
    crate::palette::save_file(&path, &out.palettes).unwrap();
    assert_eq!(crate::palette::load_file(&path).unwrap(), out.palettes);
    let out = run(
        &p,
        "ribbon",
        doc,
        selection,
        Some(0),
        serde_json::json!({}),
        Some(Gesture {
            points: vec![[10., 20.], [40., 50.], [80., 40.]],
            ..Gesture::default()
        }),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    assert!(matches!(out.commands[0], Cmd::AddShape { .. }));
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn installer_preserves_previous_plugin_when_update_is_invalid() {
    let root = temp();
    let source = root.join("source");
    std::fs::create_dir(&source).unwrap();
    let p = script(&source, "oma.message('installed')");
    let installed = install::install_at(&source, &root.join("plugins")).unwrap();
    assert_eq!(installed.id, p.id);
    let previous = std::fs::read(&installed.path).unwrap();
    std::fs::write(source.join("main.lua"), "not valid lua").unwrap();
    assert!(install::install_at(&source, &root.join("plugins")).is_err());
    assert_eq!(std::fs::read(&installed.path).unwrap(), previous);
    assert_eq!(install::discover_at(&root.join("plugins")).plugins.len(), 1);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn plugins_cannot_read_outside_their_bundle_or_mutate_locked_shapes() {
    let root = temp();
    let bundle = root.join("plugin");
    std::fs::create_dir(&bundle).unwrap();
    std::fs::write(root.join("private.txt"), "private").unwrap();
    let p = script(&bundle, "oma.read_asset('../private.txt')");
    let (doc, selection) = fixture();
    assert!(exec(&p, "test", doc.clone(), selection.clone()).is_err());
    let p = script(
        &bundle,
        "oma.translate(ctx.selection[1].layer,ctx.selection[1].id,1,1)",
    );
    let mut doc = doc;
    doc.layers[0].locked = true;
    assert!(exec(&p, "test", doc, selection).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn invalid_parameters_and_external_svg_references_are_rejected() {
    let (doc, selection) = fixture();
    assert!(
        run(
            &starter(),
            "nudge",
            doc.clone(),
            selection.clone(),
            Some(0),
            serde_json::json!({"dx":"bad"}),
            None,
            Arc::new(AtomicBool::new(false))
        )
        .is_err()
    );
    let root = temp();
    let p = script(
        &root,
        r##"oma.svg('<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><defs><linearGradient id="g"/></defs><rect width="20" height="20" fill="url(#g)" stroke="url(file:///tmp/private.svg)"/></svg>',0,0,20)"##,
    );
    assert!(
        exec(&p, "test", doc, selection)
            .unwrap_err()
            .contains("External SVG")
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn archive_traversal_rejected_and_valid_archive_installable() {
    use std::io::Write;
    let root = temp();
    let source = root.join("source");
    std::fs::create_dir(&source).unwrap();
    script(&source, "oma.message('zip')");
    let bundle = root.join("good.omaplug");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&bundle).unwrap());
    zip.start_file("main.lua", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&std::fs::read(source.join("main.lua")).unwrap())
        .unwrap();
    zip.finish().unwrap();
    assert!(install::install_at(&bundle, &root.join("plugins")).is_ok());
    let bundle = root.join("bad.omaplug");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&bundle).unwrap());
    zip.start_file("../escape.lua", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"escape").unwrap();
    zip.finish().unwrap();
    assert!(install::install_at(&bundle, &root.join("plugins")).is_err());
    assert!(!root.join("escape.lua").exists());
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn neighborhood_filter_reads_the_unchanged_source_during_a_pass() {
    let root = temp();
    let p = script(
        &root,
        "oma.map_pixels(0,function(r,g,b,a,x,y) return oma.pixel(0,1-x,y) end)",
    );
    let mut doc = Document::new("Pixels", 2., 1., 72.);
    doc.layers = vec![Layer::placed_raster(
        "Pixels",
        Pixels::from_rgba(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]).unwrap(),
        Pt::ZERO,
        Pt::new(2., 1.),
    )];
    let out = exec(&p, "test", doc.clone(), vec![]).unwrap();
    document::apply(&mut doc, &Cmd::Batch(out.commands));
    assert_eq!(
        doc.layers[0].kind.pixels().unwrap().data,
        vec![5, 6, 7, 8, 1, 2, 3, 4]
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn plugin_edits_respect_hidden_shapes_and_locked_ancestors() {
    let root = temp();
    let p = script(
        &root,
        "local s=ctx.selection[1];oma.translate(s.layer,s.id,5,5)",
    );
    for hidden in [false, true] {
        let (mut doc, selection) = fixture();
        if hidden {
            doc.find_shape_mut(0, selection[0].1).unwrap().visible = false;
        } else {
            let mut parent = Shape::new(
                Geom::Rect {
                    origin: Pt::ZERO,
                    size: Pt::new(300., 200.),
                    radius: 0.,
                },
                Style::default(),
            );
            parent.layout.frame = true;
            parent.locked = true;
            doc.find_shape_mut(0, selection[0].1).unwrap().layout.parent = Some(parent.id);
            doc.layers[0].kind.shapes_mut().unwrap().push(parent);
        }
        assert!(exec(&p, "test", doc, selection).is_err());
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn batch_translation_preserves_live_text_in_real_documents() {
    let doc = crate::formats::cli::document(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("site/public/media/ai/poster.oma"),
    )
    .unwrap();
    let mut texts = vec![];
    let mut selection = vec![];
    for (layer, l) in doc.layers.iter().enumerate() {
        for s in l.kind.shapes().unwrap_or(&[]) {
            if host::shape_editable(&doc, layer, s.id) {
                selection.push((layer, s.id));
                if let Geom::Text(t) = &s.geom {
                    texts.push((layer, s.id, t.clone()));
                }
            }
        }
    }
    assert!(!texts.is_empty());
    let out = exec(&starter(), "nudge", doc.clone(), selection).unwrap();
    let mut changed = doc;
    document::apply(&mut changed, &Cmd::Batch(out.commands));
    for (layer, id, before) in texts {
        let Geom::Text(after) = &changed.find_shape(layer, id).unwrap().geom else {
            panic!("text must remain live");
        };
        assert_eq!(after.origin, before.origin + Pt::new(16., 0.));
        assert_eq!(after.content, before.content);
        assert_eq!(after.font, before.font);
    }
}
