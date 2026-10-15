use super::*;
use crate::ui::welcome::keyboard_tests as keys;
use eframe::egui::{self, Event, Key, Modifiers};

fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
    keys::render(ctx, events, |ui| {
        studio.handle_shortcuts(ui.ctx());
        crate::ui::panel_workspace::show(ui, studio);
        crate::ui::keyboard_authoring::show(ctx, studio);
    })
}

fn press(ctx: &egui::Context, studio: &mut Studio, name: &str) {
    keys::tab_to(ctx, name, |events| frame(ctx, studio, events));
    frame(ctx, studio, keys::key(Key::Enter, Modifiers::NONE));
    frame(ctx, studio, vec![]);
}

fn edit(ctx: &egui::Context, studio: &mut Studio, name: &str, text: &str) {
    keys::tab_to(ctx, name, |events| frame(ctx, studio, events));
    keys::replace(text, |events| frame(ctx, studio, events));
    frame(ctx, studio, keys::key(Key::Enter, Modifiers::NONE));
}

#[test]
fn keyboard_motion_native_timing_and_preset_authoring_undo_and_save_reopen() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    crate::ui::theme::apply(&ctx);
    let mut studio = Studio::new();
    studio.doc = crate::document::Document::new("Native motion", 500., 400., 72.);
    studio.persona = Persona::Motion;
    studio.show_welcome = false;
    assert!(studio.selection.is_empty());
    frame(&ctx, &mut studio, vec![]);
    for key in [Key::F8, Key::R, Key::Enter] {
        frame(&ctx, &mut studio, keys::key(key, Modifiers::NONE));
    }
    frame(&ctx, &mut studio, vec![]);
    frame(
        &ctx,
        &mut studio,
        keys::key(Key::Enter, Modifiers::CTRL | Modifiers::COMMAND),
    );
    frame(&ctx, &mut studio, vec![]);
    let (_, target) = studio.primary().unwrap();
    let before = studio.doc.motion.clone();
    keys::tab_to(&ctx, "Motion duration (s)", |events| {
        frame(&ctx, &mut studio, events)
    });
    let output = keys::replace("-", |events| frame(&ctx, &mut studio, events));
    keys::associated_error(&output, "Motion duration (s)");
    assert_eq!(studio.doc.motion, before);
    keys::replace("3.25", |events| frame(&ctx, &mut studio, events));
    frame(&ctx, &mut studio, keys::key(Key::ArrowUp, Modifiers::NONE));
    let output = frame(&ctx, &mut studio, vec![]);
    keys::focused(&ctx, &output, "Motion duration (s)");
    assert!((studio.doc.motion.duration - 3.3).abs() < 0.0001);
    frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
    edit(&ctx, &mut studio, "Motion frame rate (fps)", "24");
    keys::tab_to(&ctx, "Motion frame rate (fps)", |events| {
        frame(&ctx, &mut studio, events)
    });
    frame(&ctx, &mut studio, keys::key(Key::ArrowUp, Modifiers::NONE));
    assert_eq!(studio.doc.motion.fps, 25.);
    frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
    let history = studio.history.len();
    let before = studio.doc.motion.clone();
    keys::tab_to(&ctx, "Motion preset duration (s)", |events| {
        frame(&ctx, &mut studio, events)
    });
    let output = keys::replace("-", |events| frame(&ctx, &mut studio, events));
    keys::associated_error(&output, "Motion preset duration (s)");
    let output = frame(&ctx, &mut studio, vec![]);
    assert!(keys::named(&output, "Fade in").1.is_disabled());
    assert_eq!(studio.doc.motion, before);
    keys::replace("1.25", |events| frame(&ctx, &mut studio, events));
    frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
    press(&ctx, &mut studio, "Timing & energy");
    for (name, text) in [
        ("Motion preset delay (s)", "0.35"),
        ("Motion preset stagger (s)", "0.07"),
        ("Motion preset intensity", "2.25"),
    ] {
        edit(&ctx, &mut studio, name, text);
    }
    assert_eq!(
        studio.history.len(),
        history,
        "Preset drafts do not edit artwork"
    );
    studio.updates.freezing = true;
    frame(&ctx, &mut studio, vec![]);
    studio.updates.freezing = false;
    let output = frame(&ctx, &mut studio, vec![]);
    assert!(!keys::named(&output, "Fade in").1.is_disabled());
    press(&ctx, &mut studio, "Fade in");
    assert_eq!(studio.history.len(), history + 1);
    let authored = studio.doc.motion.clone();
    let track = authored
        .tracks
        .iter()
        .find(|track| track.shape == target && track.prop == Prop::Opacity)
        .unwrap();
    assert_eq!(track.keys.last().unwrap().value, 1.);
    assert!((track.keys.last().unwrap().t - 1.6).abs() < 0.0001);
    frame(&ctx, &mut studio, keys::key(Key::F8, Modifiers::NONE));
    frame(
        &ctx,
        &mut studio,
        keys::key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND),
    );
    assert_eq!(studio.doc.motion, before);
    frame(
        &ctx,
        &mut studio,
        keys::key(
            Key::Z,
            Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
        ),
    );
    assert_eq!(studio.doc.motion, authored);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".artifacts")
        .join(format!(
            "keyboard-motion-fields-{}-{}",
            std::process::id(),
            crate::document::next_id()
        ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("motion.oma");
    crate::project::save_to(&studio.doc, &path).unwrap();
    assert_eq!(crate::project::load_from(&path).unwrap().motion, authored);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn keyboard_motion_disabled_native_timing_rejects_accessibility_changes() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut studio = Studio::new();
    studio.persona = Persona::Motion;
    studio.show_welcome = false;
    let output = frame(&ctx, &mut studio, vec![]);
    let field = keys::named(&output, "Motion duration (s)").0;
    let before = studio.doc.motion.clone();
    studio.updates.freezing = true;
    let output = frame(&ctx, &mut studio, vec![]);
    assert!(keys::named(&output, "Motion duration (s)").1.is_disabled());
    frame(
        &ctx,
        &mut studio,
        keys::action(
            field,
            egui::accesskit::Action::SetValue,
            Some(egui::accesskit::ActionData::NumericValue(20.)),
        ),
    );
    assert_eq!(studio.doc.motion, before);
}
