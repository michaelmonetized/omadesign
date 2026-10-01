//! Headless regression check through the real Studio UI; native Wayland QA is separate.
use eframe::egui::{self, Event, ImeEvent, Key, Modifiers};
use omadesign::{app::Studio, geom::Pt, tools::Persona};
use serde_json::json;

fn input(events: Vec<Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1440.0, 900.0),
        )),
        events,
        ..Default::default()
    }
}
fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
    let mut out = ctx.run_ui(input(events), |ui| omadesign::ui::run(ui, studio));
    out.textures_delta.clear();
    out
}
fn main() {
    let mut rows = Vec::new();
    for persona in [Persona::Design, Persona::Pixel, Persona::Layout] {
        eprintln!("probing {persona:?}");
        let ctx = egui::Context::default();
        omadesign::ui::theme::apply(&ctx);
        let mut studio = Studio::new();
        studio.show_welcome = false;
        studio.persona = persona;
        studio.active_layer = Some(1);
        studio.place_text(Pt::new(40.0, 80.0));
        frame(&ctx, &mut studio, vec![]);
        let active = frame(&ctx, &mut studio, vec![]);
        let before = studio.selected_type().unwrap().content;
        let focused = ctx.memory(|m| m.focused() == Some(egui::Id::new("studio-canvas")));
        frame(
            &ctx,
            &mut studio,
            vec![Event::Ime(ImeEvent::Commit("中文日本語한국어".into()))],
        );
        let after_ime = studio.selected_type().unwrap().content;
        frame(
            &ctx,
            &mut studio,
            vec![Event::Text("中文日本語한국어".into())],
        );
        let after_text = studio.selected_type().unwrap().content;
        assert!(focused, "probe must have the canvas focus");
        assert!(
            active.platform_output.ime.is_some(),
            "canvas must request IME"
        );
        assert_eq!(after_ime, "中文日本語한국어", "IME commit inserts Unicode");
        assert_eq!(
            after_text, "中文日本語한국어中文日本語한국어",
            "control: plain Unicode event must work"
        );
        rows.push(
            json!({"mode":format!("{persona:?}"),"canvas_focused":focused,
            "ime_requested":active.platform_output.ime.is_some(),"before":before,
            "after_ime_commit":after_ime,"after_plain_text_event":after_text}),
        );
    }
    let ctx = egui::Context::default();
    let id = egui::Id::new("inspector-control");
    let mut value = String::new();
    let mut field = |events: Vec<Event>| {
        let mut out = ctx.run_ui(input(events), |ui| {
            ui.add(egui::TextEdit::singleline(&mut value).id(id))
                .request_focus();
        });
        out.textures_delta.clear();
        out
    };
    field(vec![]);
    let active = field(vec![]);
    field(vec![Event::Ime(ImeEvent::Commit(
        "中文日本語한국어".into(),
    ))]);
    assert!(
        active.platform_output.ime.is_some(),
        "standard TextEdit requests IME"
    );
    assert_eq!(value, "中文日本語한국어");
    let key = |key| Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::CTRL | Modifiers::COMMAND,
    };
    let mut select = ctx.run_ui(input(vec![key(Key::A)]), |ui| {
        ui.add(egui::TextEdit::singleline(&mut value).id(id));
    });
    select.textures_delta.clear();
    let mut copy = ctx.run_ui(input(vec![Event::Copy]), |ui| {
        ui.add(egui::TextEdit::singleline(&mut value).id(id));
    });
    copy.textures_delta.clear();
    let copied = copy.platform_output.commands.iter().find_map(|c| match c {
        egui::OutputCommand::CopyText(v) => Some(v.clone()),
        _ => None,
    });
    let mut paste = ctx.run_ui(
        input(vec![Event::Paste("剪贴板貼り付け붙여넣기".into())]),
        |ui| {
            ui.add(egui::TextEdit::singleline(&mut value).id(id));
        },
    );
    paste.textures_delta.clear();
    assert_eq!(copied.as_deref(), Some("中文日本語한국어"));
    assert_eq!(value, "剪贴板貼り付け붙여넣기");
    println!("{}",serde_json::to_string_pretty(&json!({
        "source_base_commit":"0a6bdfe54927c360c8f438002d30940a0fede4d8",
        "scope":"real Studio UI and egui event delivery, without a native Wayland window or fcitx/Rime session",
        "canvas":rows,"standard_textedit":{"ime_requested":true,"ime_commit_inserted":true,"unicode_copy_paste":true},
        "result":"canvas IME and standard TextEdit regression checks passed"
    })).unwrap());
}
