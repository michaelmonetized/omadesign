use super::welcome::keyboard_tests as keys;
use crate::{app::Studio, document::Fill};
use eframe::egui::{self, Event, Key, Modifiers};

fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
    keys::render(ctx, events, |ui| super::run(ui, studio))
}
fn imported(source: &str) -> (egui::Context, Studio) {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    super::theme::apply(&ctx);
    let (doc, _) = crate::formats::svg::read(source, "Native retained source").unwrap();
    let mut studio = Studio::new();
    studio.open_document(doc, None);
    studio.show_welcome = false;
    assert!(studio.selection.is_empty());
    frame(&ctx, &mut studio, vec![]);
    frame(&ctx, &mut studio, keys::key(Key::F8, Modifiers::NONE));
    frame(&ctx, &mut studio, keys::key(Key::PageDown, Modifiers::NONE));
    frame(&ctx, &mut studio, vec![]);
    assert!(!studio.selection.is_empty());
    (ctx, studio)
}
fn press(ctx: &egui::Context, studio: &mut Studio, name: &str) -> egui::FullOutput {
    keys::tab_to(ctx, name, |events| frame(ctx, studio, events));
    frame(ctx, studio, keys::key(Key::Enter, Modifiers::NONE));
    frame(ctx, studio, vec![])
}
fn edit(ctx: &egui::Context, studio: &mut Studio, name: &str, text: &str) -> egui::FullOutput {
    keys::tab_to(ctx, name, |events| frame(ctx, studio, events));
    keys::replace(text, |events| frame(ctx, studio, events));
    frame(ctx, studio, vec![])
}
fn pattern(studio: &Studio) -> crate::svg_source::SvgPattern {
    let Fill::Pattern(pattern) = &super::studios::inspected_style(studio).fill else {
        panic!("retained pattern");
    };
    pattern.clone()
}
fn undo(ctx: &egui::Context, studio: &mut Studio, redo: bool) {
    frame(ctx, studio, keys::key(Key::F8, Modifiers::NONE));
    frame(
        ctx,
        studio,
        keys::key(
            Key::Z,
            Modifiers {
                ctrl: true,
                command: true,
                shift: redo,
                ..Modifiers::NONE
            },
        ),
    );
}
fn reopen(doc: &crate::document::Document) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".artifacts")
        .join(format!("keyboard-svg-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("retained.oma");
    crate::project::save_to(doc, &path).unwrap();
    assert_eq!(
        crate::project::encode(doc).unwrap(),
        crate::project::encode(&crate::project::load_from(&path).unwrap()).unwrap()
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn keyboard_svg_pattern_native_selection_scalars_draft_apply_errors_undo_and_reopen() {
    let (ctx, mut studio) = imported(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="80"><defs><pattern id="p" width="8" height="8" patternUnits="userSpaceOnUse"><rect width="4" height="4" fill="red"/></pattern></defs><rect x="5" y="5" width="80" height="60" fill="url(#p)"/></svg>"##,
    );
    let target = studio.primary();
    press(&ctx, &mut studio, "Vector pattern");
    let output = edit(&ctx, &mut studio, "Pattern transform e", "1.234567");
    let coordinate = keys::named(&output, "Pattern transform e").0;
    keys::focused(&ctx, &output, "Pattern transform e");
    assert_eq!(pattern(&studio).transform[4], 1.234567);
    let output = edit(&ctx, &mut studio, "Pattern opacity", "0.625");
    assert_eq!(pattern(&studio).opacity, 0.625);
    assert_eq!(keys::named(&output, "Pattern transform e").0, coordinate);
    let before = crate::project::encode(&studio.doc).unwrap();
    edit(&ctx, &mut studio, "Vector definitions", "<pattern>");
    frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
    assert_eq!(
        crate::project::encode(&studio.doc).unwrap(),
        before,
        "Plain Enter only inserts a draft newline"
    );
    let output = press(&ctx, &mut studio, "Apply vector source");
    keys::associated_error(&output, "Vector definitions");
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    let id = pattern(&studio).id;
    edit(
        &ctx,
        &mut studio,
        "Vector definitions",
        &format!(
            "<pattern id=\"{id}\" width=\"8\" height=\"8\" patternUnits=\"userSpaceOnUse\"><rect width=\"4\" height=\"4\" fill=\"blue\"/></pattern>"
        ),
    );
    press(&ctx, &mut studio, "Apply vector source");
    assert!(pattern(&studio).definitions.contains("blue"));
    assert_eq!(pattern(&studio).transform[4], 1.234567);
    assert_eq!(pattern(&studio).opacity, 0.625);
    assert_eq!(studio.primary(), target);
    let after = crate::project::encode(&studio.doc).unwrap();
    undo(&ctx, &mut studio, false);
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    undo(&ctx, &mut studio, true);
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), after);
    reopen(&studio.doc);
}

#[test]
fn keyboard_svg_filter_native_source_respects_graph_owner_lock_and_retires_undo_drafts() {
    let (ctx, mut studio) = imported(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="80"><defs><filter id="f"><feOffset dx="2" dy="3"/></filter></defs><g filter="url(#f)"><rect x="10" y="10" width="40" height="30" fill="red"/></g></svg>"##,
    );
    let index = studio
        .doc
        .layers
        .iter()
        .position(|layer| layer.svg_filter.is_some())
        .unwrap();
    press(&ctx, &mut studio, "SVG filter graph");
    let before = crate::project::encode(&studio.doc).unwrap();
    edit(&ctx, &mut studio, "Filter source", "<filter>");
    let output = press(&ctx, &mut studio, "Apply filter source");
    keys::associated_error(&output, "Filter source");
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    let id = studio.doc.layers[index]
        .svg_filter
        .as_ref()
        .unwrap()
        .filters[0]
        .clone();
    edit(
        &ctx,
        &mut studio,
        "Filter source",
        &format!("<filter id=\"{id}\"><feOffset dx=\"5\" dy=\"3\"/></filter>"),
    );
    let output = press(&ctx, &mut studio, "Apply filter source");
    let stale = keys::named(&output, "Filter source").0;
    let original = studio.doc.layers[index]
        .svg_filter
        .as_ref()
        .unwrap()
        .original_definitions
        .clone();
    assert!(
        studio.doc.layers[index]
            .svg_filter
            .as_ref()
            .unwrap()
            .definitions
            .contains("dx=\"5\"")
    );
    let after = crate::project::encode(&studio.doc).unwrap();
    undo(&ctx, &mut studio, false);
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    let output = frame(&ctx, &mut studio, vec![]);
    assert_ne!(keys::named(&output, "Filter source").0, stale);
    frame(
        &ctx,
        &mut studio,
        keys::action(
            stale,
            egui::accesskit::Action::SetValue,
            Some(egui::accesskit::ActionData::Value("old draft".into())),
        ),
    );
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    undo(&ctx, &mut studio, true);
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), after);
    let output = edit(&ctx, &mut studio, "Filter source", "<filter/>");
    let apply = keys::named(&output, "Apply filter source").0;
    studio.doc.layers[index].locked = true;
    let locked = crate::project::encode(&studio.doc).unwrap();
    let output = frame(&ctx, &mut studio, vec![]);
    assert!(keys::named(&output, "Apply filter source").1.is_disabled());
    frame(
        &ctx,
        &mut studio,
        keys::action(apply, egui::accesskit::Action::Click, None),
    );
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), locked);
    assert_eq!(
        studio.doc.layers[index]
            .svg_filter
            .as_ref()
            .unwrap()
            .original_definitions,
        original
    );
    reopen(&studio.doc);
}
