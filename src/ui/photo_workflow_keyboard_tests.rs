use super::*;
use crate::ui::welcome::keyboard_tests as keys;
use egui::{Event, Key, Modifiers};

fn fixture() -> (Context, Studio, std::path::PathBuf) {
    let ctx = Context::default();
    ctx.enable_accesskit();
    crate::ui::theme::apply(&ctx);
    let mut studio = Studio::new();
    studio.persona = crate::tools::Persona::Photo;
    studio.show_welcome = false;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".artifacts")
        .join(format!(
            "keyboard-photo-workflow-{}-{}",
            std::process::id(),
            crate::document::next_id()
        ));
    std::fs::create_dir_all(&root).unwrap();
    studio
        .photo
        .set_preset_path_for_test(root.join("looks.omapreset"));
    for name in ["Transfer source", "Transfer target"] {
        studio
            .photo
            .images
            .push(crate::photo::PhotoImage::from_full(
                name.into(),
                crate::photo::RgbaImage::new(16, 16, vec![128; 16 * 16 * 4]).unwrap(),
            ));
    }
    assert!(studio.photo.selected.is_none());
    (ctx, studio, root)
}

fn frame(ctx: &Context, studio: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
    keys::render(ctx, events, |ui| {
        studio.handle_shortcuts(ui.ctx());
        crate::ui::photo::show(ui, studio);
    })
}

fn press(ctx: &Context, studio: &mut Studio, name: &str) -> egui::FullOutput {
    keys::tab_to(ctx, name, |events| frame(ctx, studio, events));
    frame(ctx, studio, keys::key(Key::Enter, Modifiers::NONE));
    frame(ctx, studio, vec![])
}

fn settle(ctx: &Context, studio: &mut Studio) -> egui::FullOutput {
    let mut output = frame(ctx, studio, vec![]);
    for _ in 0..2 {
        output = frame(ctx, studio, vec![]);
    }
    output
}

fn wait_presets(ctx: &Context, studio: &mut Studio) -> egui::FullOutput {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while studio.photo.is_loading_presets() || studio.photo.is_saving_presets() {
        assert!(
            std::time::Instant::now() < until,
            "Native preset job did not finish"
        );
        frame(ctx, studio, vec![]);
        std::thread::yield_now();
    }
    frame(ctx, studio, vec![])
}

fn expose(ctx: &Context, studio: &mut Studio, text: &str) {
    keys::tab_to(ctx, "Exposure", |events| frame(ctx, studio, events));
    keys::replace(text, |events| frame(ctx, studio, events));
    frame(ctx, studio, keys::key(Key::Enter, Modifiers::NONE));
}

#[test]
fn keyboard_photo_workflow_transfers_adjustments_and_undo_without_pointer_or_preselection() {
    let (ctx, mut studio, root) = fixture();
    frame(&ctx, &mut studio, vec![]);
    press(&ctx, &mut studio, "Photo Transfer source");
    expose(&ctx, &mut studio, "1.25");
    press(&ctx, &mut studio, "Copy adjustments");
    assert_eq!(
        studio
            .photo
            .copied_adjustments
            .as_ref()
            .unwrap()
            .params
            .exposure,
        1.25
    );
    press(&ctx, &mut studio, "Photo Transfer target");
    assert_eq!(studio.photo.selected, Some(1));
    press(&ctx, &mut studio, "Paste…");
    assert!(matches!(read(&ctx).dialog, Some(Dialog::Apply(_))));
    press(&ctx, &mut studio, "Apply to 1 photos");
    let output = settle(&ctx, &mut studio);
    keys::focused(&ctx, &output, "Photo develop");
    assert!(read(&ctx).dialog.is_none());
    assert_eq!(studio.photo.images[0].develop.exposure, 1.25);
    assert_eq!(studio.photo.images[1].develop.exposure, 1.25);
    frame(&ctx, &mut studio, keys::key(Key::F8, Modifiers::NONE));
    frame(
        &ctx,
        &mut studio,
        keys::key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND),
    );
    assert_eq!(studio.photo.images[0].develop.exposure, 1.25);
    assert_eq!(studio.photo.images[1].develop.exposure, 0.);
    frame(
        &ctx,
        &mut studio,
        keys::key(
            Key::Z,
            Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
        ),
    );
    assert_eq!(studio.photo.images[1].develop.exposure, 1.25);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn keyboard_photo_workflow_preset_native_fields_save_reopen_and_stale_owner_refusal() {
    let (ctx, mut studio, root) = fixture();
    frame(&ctx, &mut studio, vec![]);
    press(&ctx, &mut studio, "Photo Transfer source");
    expose(&ctx, &mut studio, "1.25");
    keys::tab_to(&ctx, "Presets…", |events| {
        frame(&ctx, &mut studio, events)
    });
    frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
    let opening = frame(&ctx, &mut studio, vec![]);
    let name_id = keys::named(&opening, "Preset name").0;
    let filter_id = keys::named(&opening, "Filter presets").0;
    let crop_id = keys::named(&opening, "Crop").0;
    let output = wait_presets(&ctx, &mut studio);
    assert_eq!(keys::named(&output, "Preset name").0, name_id);
    assert_eq!(keys::named(&output, "Filter presets").0, filter_id);
    assert_eq!(keys::named(&output, "Crop").0, crop_id);
    keys::tab_to(&ctx, "Preset name", |events| {
        frame(&ctx, &mut studio, events)
    });
    let output = keys::replace(&"x".repeat(121), |events| frame(&ctx, &mut studio, events));
    keys::associated_error(&output, "Preset name");
    assert!(keys::named(&output, "Save preset").1.is_disabled());
    assert_eq!(keys::named(&output, "Preset name").0, name_id);
    keys::replace("Warm", |events| frame(&ctx, &mut studio, events));
    let corrected = frame(&ctx, &mut studio, vec![]);
    assert_eq!(keys::named(&corrected, "Crop").0, crop_id);
    assert_eq!(keys::named(&corrected, "Preset name").0, name_id);
    keys::tab_to(&ctx, "Crop", |events| frame(&ctx, &mut studio, events));
    frame(&ctx, &mut studio, keys::key(Key::Space, Modifiers::NONE));
    press(&ctx, &mut studio, "Save preset");
    wait_presets(&ctx, &mut studio);
    let path = root.join("looks.omapreset");
    let saved = crate::photo::presets::load_file(&path).unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].name, "Warm");
    assert!(saved[0].categories.crop);
    assert_eq!(saved[0].params.exposure, 1.25);
    let mut reopened = crate::app::PhotoSession::new();
    reopened.set_preset_path_for_test(path.clone());
    reopened.ensure_presets_loaded();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while reopened.is_loading_presets() {
        assert!(std::time::Instant::now() < until);
        reopened.poll(&ctx);
        std::thread::yield_now();
    }
    assert_eq!(reopened.presets, saved);
    press(&ctx, &mut studio, "Done");
    settle(&ctx, &mut studio);
    press(&ctx, &mut studio, "Presets…");
    let output = frame(&ctx, &mut studio, vec![]);
    assert_ne!(keys::named(&output, "Preset name").0, name_id);
    keys::tab_to(&ctx, "Filter presets", |events| {
        frame(&ctx, &mut studio, events)
    });
    keys::replace("Warm", |events| frame(&ctx, &mut studio, events));
    press(&ctx, &mut studio, "Warm");
    let unchanged = std::fs::read(&path).unwrap();
    studio.photo.select_image(1);
    let output = frame(&ctx, &mut studio, vec![]);
    keys::associated_error(&output, "Save preset");
    let save = keys::named(&output, "Save preset");
    assert!(save.1.is_disabled());
    frame(
        &ctx,
        &mut studio,
        keys::action(save.0, egui::accesskit::Action::Click, None),
    );
    assert!(!studio.photo.is_saving_presets());
    assert_eq!(std::fs::read(&path).unwrap(), unchanged);
    press(&ctx, &mut studio, "Done");
    let output = settle(&ctx, &mut studio);
    keys::focused(&ctx, &output, "Photo develop");
    std::fs::remove_dir_all(root).unwrap();
}
