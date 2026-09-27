//! Supplemental native clipboard QA. No window, mocks, or fabricated Paste payload.
use omadesign::{
    clipboard::type_style::{self, RichText},
    geom::TypeRun,
};
use std::io::Write;
use std::process::{Command, Stdio};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() != Some("--native") {
        return Err("Pass --native to exercise the actual OS clipboard".into());
    }
    let source = TypeRun {
        content: "fi AB 12 é".into(),
        liga: false,
        smcp: true,
        tnum: true,
        features: vec![(*b"ss01", 1)],
        ..Default::default()
    };
    let rich = RichText::from_run(&source, 0, source.content.chars().count());
    let ctx = eframe::egui::Context::default();
    omadesign::ui::theme::apply(&ctx);
    let mut copied = omadesign::app::Studio::new();
    copied.show_welcome = false;
    copied.active_layer = Some(1);
    copied.place_text(omadesign::geom::Pt::ZERO);
    copied.patch_type(|run| *run = source.clone());
    {
        let edit = copied.type_edit.as_mut().unwrap();
        edit.anchor = 0;
        edit.caret = source.content.chars().count();
    }
    shortcut(&ctx, &mut copied, eframe::egui::Event::Copy);
    let mut target_app = omadesign::app::Studio::new();
    target_app.show_welcome = false;
    target_app.active_layer = Some(1);
    target_app.place_text(omadesign::geom::Pt::ZERO);
    target_app.patch_type(|run| run.features = vec![(*b"ss02", 1)]);
    native_paste(&ctx, &mut target_app, &source.content);
    let offered = Command::new("wl-paste").arg("--list-types").output()?;
    assert!(offered.status.success());
    let offered = String::from_utf8(offered.stdout)?;
    assert!(offered.lines().any(|line| line == type_style::MIME));
    assert!(offered.lines().any(|line| line == "text/html"));
    let plain = Command::new("wl-paste")
        .args(["--no-newline", "--type", "text/plain;charset=utf-8"])
        .output()?;
    assert!(plain.status.success());
    assert_eq!(String::from_utf8(plain.stdout)?, source.content);
    let paste = type_style::read(None)?;
    assert_eq!(paste.rich, Some(rich.clone()));
    let mut target = TypeRun {
        content: paste.text,
        features: vec![(*b"ss02", 1)],
        ..Default::default()
    };
    paste.rich.unwrap().apply_styles(&mut target, 0);
    assert_eq!(target_app.selected_type().unwrap().spans, target.spans);
    let pasted = target_app.selected_type().unwrap();
    target_app.commit_type_edit();
    target_app.undo();
    assert!(target_app.selected_type().unwrap().content.is_empty());
    target_app.redo();
    assert_eq!(target_app.selected_type().unwrap().spans, pasted.spans);
    for at in 0..source.content.chars().count() {
        for tag in [*b"liga", *b"smcp", *b"tnum", *b"ss01", *b"ss02"] {
            assert_eq!(
                omadesign::text::feature_value(&source, at, tag),
                omadesign::text::feature_value(&target, at, tag)
            );
        }
    }
    let mut external = Command::new("wl-copy")
        .args(["--type", "text/plain;charset=utf-8"])
        .stdin(Stdio::piped())
        .spawn()?;
    external
        .stdin
        .take()
        .unwrap()
        .write_all(source.content.as_bytes())?;
    assert!(external.wait()?.success());
    let paste = type_style::read(None)?;
    assert_eq!(paste.text, source.content);
    assert!(paste.rich.is_none());
    let mut plain_app = omadesign::app::Studio::new();
    plain_app.show_welcome = false;
    plain_app.active_layer = Some(1);
    plain_app.place_text(omadesign::geom::Pt::ZERO);
    native_paste(&ctx, &mut plain_app, &source.content);
    assert_eq!(
        omadesign::text::feature_value(&plain_app.selected_type().unwrap(), 0, *b"smcp"),
        0
    );
    let receipt = serde_json::json!({"status":"passed","errors":[],"backend":"native Wayland clipboard","offered_formats":offered.lines().collect::<Vec<_>>(),"checks":["app-owned typed format plus exact ordinary UTF-8 text","typed format read from current native owner","actual app Copy and asynchronous Ctrl+V preserve effective features over different run defaults", "paste survives one-step undo and redo","independent wl-copy owner replaces identical plain text and clears rich provenance"]});
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    Ok(())
}

fn shortcut(
    ctx: &eframe::egui::Context,
    studio: &mut omadesign::app::Studio,
    event: eframe::egui::Event,
) {
    let mut output = ctx.run_ui(
        eframe::egui::RawInput {
            focused: true,
            events: vec![event],
            ..Default::default()
        },
        |ui| studio.handle_shortcuts(ui.ctx()),
    );
    output.textures_delta.clear();
}
fn native_paste(ctx: &eframe::egui::Context, studio: &mut omadesign::app::Studio, expected: &str) {
    use eframe::egui::{Event, Key, Modifiers};
    shortcut(
        ctx,
        studio,
        Event::Key {
            key: Key::V,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::CTRL,
        },
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut frame = eframe::Frame::_new_kittest();
    while studio.selected_type().unwrap().content != expected
        && std::time::Instant::now() < deadline
    {
        let mut output = ctx.run_ui(
            eframe::egui::RawInput {
                focused: true,
                screen_rect: Some(eframe::egui::Rect::from_min_size(
                    eframe::egui::Pos2::ZERO,
                    eframe::egui::vec2(1600., 900.),
                )),
                ..Default::default()
            },
            |ui| eframe::App::ui(studio, ui, &mut frame),
        );
        output.textures_delta.clear();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        studio.selected_type().unwrap().content,
        expected,
        "native async paste: {}",
        studio.status
    );
}
