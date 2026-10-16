use super::*;
use crate::ui::welcome::keyboard_tests as keys;
use egui::{Event, Key, Modifiers};
fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    super::super::theme::apply(&ctx);
    ctx
}
#[test]
fn craft_screen_export_native_keys_collect_rename_save_and_undo_reopen() {
    let ctx = context();
    let mut studio = crate::app::screen_export::tests::fixture();
    let mut frame = |events| {
        keys::render(&ctx, events, |ui| {
            ui.set_max_width(288.);
            show(ui, &mut studio);
        })
    };
    frame(vec![]);
    frame(vec![]);
    keys::tab_to(&ctx, "Collect selection as separate assets", &mut frame);
    frame(keys::key(Key::Space, Modifiers::NONE));
    frame(vec![]);
    keys::tab_to(&ctx, "Export asset name", &mut frame);
    frame(keys::key(Key::A, Modifiers::CTRL | Modifiers::COMMAND));
    frame(vec![Event::Paste("Native asset Ω".into())]);
    frame(keys::key(Key::Enter, Modifiers::NONE));
    frame(vec![]);
    keys::tab_to(&ctx, "Export filename prefix", &mut frame);
    keys::replace("web-", &mut frame);
    keys::tab_to(&ctx, "Save export settings", &mut frame);
    frame(keys::key(Key::Space, Modifiers::NONE));
    frame(vec![]);
    drop(frame);
    assert_eq!(studio.doc.screen_export.assets[0].name, "Native asset Ω");
    assert_eq!(studio.doc.screen_export.prefix, "web-");
    let saved = crate::project::encode(&studio.doc).unwrap();
    let reopened = crate::project::decode(&saved).unwrap();
    assert_eq!(reopened.screen_export, studio.doc.screen_export);
    studio.undo();
    assert_eq!(studio.doc.screen_export.prefix, "");
    assert_eq!(studio.doc.screen_export.assets[0].name, "Native asset Ω");
    jobs::cancel::<Thumb>(&ctx, THUMB_JOB);
}
#[test]
fn craft_screen_export_native_worker_uses_captured_rows_and_retires_stale_owner() {
    let ctx = context();
    let mut studio = crate::app::screen_export::tests::fixture();
    let target = studio.capture_export_assets().unwrap();
    let ids = studio.collect_export_assets(&target, true).unwrap();
    let root = std::env::current_dir()
        .unwrap()
        .join(".artifacts")
        .join(format!("screen-ui-export-{}", crate::document::next_id()));
    std::fs::create_dir_all(&root).unwrap();
    let state = State {
        selected: ids,
        folder: root.to_string_lossy().into_owned(),
        ..Default::default()
    };
    start(&ctx, &studio, &state).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while ctx.data(|d| d.get_temp::<Run>(run_id())).is_some() {
        poll(&ctx, &mut studio);
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(root.join("Badge.png").is_file(), "{}", studio.status);
    assert!(studio.status.contains("Exported 1"));
    let valid = Arc::new(AtomicBool::new(true));
    ctx.data_mut(|d| {
        d.insert_temp(
            run_id(),
            Run {
                document: studio.swap_id.clone(),
                revision: studio.canvas_gen,
                valid: valid.clone(),
            },
        )
    });
    studio.canvas_gen += 1;
    poll(&ctx, &mut studio);
    assert!(!valid.load(Ordering::Relaxed));
    ctx.data_mut(|d| d.remove::<Run>(run_id()));
    std::fs::remove_dir_all(root).unwrap();
}
