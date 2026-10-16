use super::*;
use crate::ui::welcome::keyboard_tests as keys;
use egui::{Event, Key, Modifiers};

fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
    frame_with_modal(ctx, studio, events, false)
}

fn frame_with_modal(
    ctx: &egui::Context,
    studio: &mut Studio,
    events: Vec<Event>,
    modal: bool,
) -> egui::FullOutput {
    keys::render(ctx, events, |ui| {
        if !before_shortcuts(ui.ctx(), studio) {
            crate::ui::guides::handle_shortcuts(ui.ctx(), studio);
            studio.handle_shortcuts(ui.ctx());
        }
        crate::ui::chrome::left_toolbar(ui, studio);
        crate::ui::panel_workspace::show(ui, studio);
        show_shape_browser(ui, studio);
        show_asset_browser(ui, studio);
        if modal {
            egui::Modal::new(egui::Id::new("keyboard-browser-blocking-modal")).show(
                ui.ctx(),
                |ui| {
                    ui.heading("Browser blocking modal fixture");
                let _ = ui.button("Keep browser open");
                },
            );
        }
    })
}

fn press(ctx: &egui::Context, studio: &mut Studio, label: &str) -> egui::FullOutput {
    keys::tab_to(ctx, label, |events| frame(ctx, studio, events));
    frame(ctx, studio, keys::key(Key::Enter, Modifiers::NONE));
    frame(ctx, studio, vec![])
}

fn fixture() -> (egui::Context, Studio) {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    theme::apply(&ctx);
    let mut studio = Studio::new();
    studio.doc = crate::document::Document::new("Native library artwork", 500., 400., 72.);
    studio.persona = crate::tools::Persona::Design;
    studio.show_welcome = false;
    assert!(studio.selection.is_empty());
    (ctx, studio)
}

#[test]
fn keyboard_browser_places_bundled_shape_and_undo_saves_reopens_without_pointer_setup() {
    let (ctx, mut studio) = fixture();
    frame(&ctx, &mut studio, vec![]);
    press(&ctx, &mut studio, "Shape library");
    assert!(studio.show_shape_browser);
    let output = keys::tab(true, |events| {
        frame(&ctx, &mut studio, events)
    });
    keys::focused(&ctx, &output, "Shape collection");
    frame(&ctx, &mut studio, keys::key(Key::Home, Modifiers::NONE));
    frame(
        &ctx,
        &mut studio,
        keys::key(Key::ArrowDown, Modifiers::NONE),
    );
    assert_eq!(studio.shape_lib, "Phosphor");
    keys::tab_to(&ctx, "Search shapes", |events| {
        frame(&ctx, &mut studio, events)
    });
    keys::replace("camera", |events| frame(&ctx, &mut studio, events));
    let before = crate::project::encode(&studio.doc).unwrap();
    let history = studio.history.len();
    press(&ctx, &mut studio, "Add Phosphor shape camera");
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while studio.show_shape_browser {
        assert!(
            std::time::Instant::now() < until,
            "Bundled native shape job did not finish"
        );
        frame(&ctx, &mut studio, vec![]);
        std::thread::yield_now();
    }
    let output = frame(&ctx, &mut studio, vec![]);
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(egui::Id::new("studio-canvas"))
    );
    assert_eq!(
        keys::named(&output, "Artwork canvas").1.role(),
        egui::accesskit::Role::Canvas
    );
    let (layer, target) = studio.primary().unwrap();
    let shape = studio.doc.find_shape(layer, target).unwrap();
    assert_eq!(shape.name, "camera");
    assert!(shape.geom.is_closed());
    assert_eq!(studio.history.len(), history + 1);
    let authored = crate::project::encode(&studio.doc).unwrap();
    let selection = studio.selection.clone();
    let history = studio.history.len();
    press(&ctx, &mut studio, "Shape library");
    keys::tab_to(&ctx, "Search shapes", |events| {
        frame(&ctx, &mut studio, events)
    });
    frame(&ctx, &mut studio, keys::key(Key::Escape, Modifiers::NONE));
    assert!(!studio.show_shape_browser);
    assert_eq!(
        studio.selection, selection,
        "Browser Escape reached canvas selection handling"
    );
    assert_eq!(studio.history.len(), history);
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), authored);
    for _ in 0..3 {
        frame(&ctx, &mut studio, vec![]);
    }
    let focused = ctx
        .memory(|memory| memory.focused())
        .expect("Closed browser must restore a native panel");
    let response = ctx.read_response(focused).unwrap();
    assert!(response.enabled());
    assert!(ctx.memory(|memory| memory.allows_interaction(response.layer_id)));
    frame(&ctx, &mut studio, keys::key(Key::F8, Modifiers::NONE));
    frame(
        &ctx,
        &mut studio,
        keys::key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND),
    );
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    frame(
        &ctx,
        &mut studio,
        keys::key(
            Key::Z,
            Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
        ),
    );
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), authored);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".artifacts")
        .join(format!(
            "keyboard-browser-{}-{}",
            std::process::id(),
            crate::document::next_id()
        ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("library.oma");
    crate::project::save_to(&studio.doc, &path).unwrap();
    let reopened = crate::project::load_from(&path).unwrap();
    assert_eq!(crate::project::encode(&reopened).unwrap(), authored);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn keyboard_browser_native_provider_query_clear_escape_and_fresh_owner_do_not_start_network_jobs() {
    let (ctx, mut studio) = fixture();
    frame(&ctx, &mut studio, vec![]);
    press(&ctx, &mut studio, "Free photos");
    let output = frame(&ctx, &mut studio, vec![]);
    let id = keys::named(&output, "Search photos").0;
    keys::tab_to(&ctx, "Photo provider", |events| {
        frame(&ctx, &mut studio, events)
    });
    frame(&ctx, &mut studio, keys::key(Key::End, Modifiers::NONE));
    assert_eq!(studio.asset_provider, "Picsum");
    keys::tab_to(&ctx, "Search photos", |events| {
        frame(&ctx, &mut studio, events)
    });
    keys::replace("A garden", |events| frame(&ctx, &mut studio, events));
    let output = frame(&ctx, &mut studio, vec![]);
    assert_eq!(keys::named(&output, "Search photos").0, id);
    keys::focused(&ctx, &output, "Search photos");
    press(&ctx, &mut studio, "Clear");
    assert!(studio.asset_query.is_empty());
    keys::tab_to(&ctx, "Search photos", |events| {
        frame(&ctx, &mut studio, events)
    });
    frame(&ctx, &mut studio, keys::key(Key::Escape, Modifiers::NONE));
    assert!(!studio.show_asset_browser);
    for _ in 0..3 {
        frame(&ctx, &mut studio, vec![]);
    }
    press(&ctx, &mut studio, "Free photos");
    let output = frame(&ctx, &mut studio, vec![]);
    assert_ne!(keys::named(&output, "Search photos").0, id);
    assert!(!jobs::is_running::<Vec<crate::asset_browser::AssetHit>>(
        &ctx, SEARCH
    ));
    assert!(!jobs::is_running::<PhotoResult>(&ctx, PHOTO));
}

#[test]
fn keyboard_browser_escape_respects_the_actual_blocking_modal_layer() {
    let (ctx, mut studio) = fixture();
    frame(&ctx, &mut studio, vec![]);
    press(&ctx, &mut studio, "Shape library");
    keys::tab_to(&ctx, "Search shapes", |events| {
        frame(&ctx, &mut studio, events)
    });
    frame_with_modal(&ctx, &mut studio, vec![], true);
    keys::tab_to(&ctx, "Keep browser open", |events| {
        frame_with_modal(&ctx, &mut studio, events, true)
    });
    let before = crate::project::encode(&studio.doc).unwrap();
    let history = studio.history.len();
    frame_with_modal(
        &ctx,
        &mut studio,
        keys::key(Key::Escape, Modifiers::NONE),
        true,
    );
    assert!(
        studio.show_shape_browser,
        "Blocked browser stole another modal's Escape"
    );
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    assert_eq!(studio.history.len(), history);
}
