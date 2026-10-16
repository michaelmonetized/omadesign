use super::*;
use crate::ui::welcome::keyboard_tests as keys;
use egui::{Event, Key, Modifiers};

fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
    keys::render(ctx, events, |ui| {
        studio.handle_shortcuts(ui.ctx());
        button(ui, studio);
        panel(ui, studio);
    })
}

fn fixture() -> (egui::Context, Studio) {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    crate::ui::theme::apply(&ctx);
    let mut studio = Studio::new();
    studio.agent.loaded = true;
    studio.show_welcome = false;
    (ctx, studio)
}

fn press(ctx: &egui::Context, studio: &mut Studio, name: &str) -> egui::FullOutput {
    keys::tab_to(ctx, name, |events| frame(ctx, studio, events));
    frame(ctx, studio, keys::key(Key::Enter, Modifiers::NONE));
    frame(ctx, studio, vec![])
}

#[test]
fn keyboard_agent_connection_fields_keep_native_ids_and_gate_malformed_arguments() {
    let (ctx, mut studio) = fixture();
    let before = crate::project::encode(&studio.doc).unwrap();
    frame(&ctx, &mut studio, vec![]);
    press(&ctx, &mut studio, "Agent");
    assert!(studio.agent.visible);
    press(&ctx, &mut studio, "Agent connection settings");
    let output = press(&ctx, &mut studio, "Agent command");
    let folder = keys::named(&output, "Project folder").0;
    let executable = keys::named(&output, "Agent executable").0;
    let arguments = keys::named(&output, "Agent arguments (JSON array)").0;
    keys::tab_to(&ctx, "Agent arguments (JSON array)", |events| {
        frame(&ctx, &mut studio, events)
    });
    keys::replace("[", |events| frame(&ctx, &mut studio, events));
    let output = frame(&ctx, &mut studio, vec![]);
    keys::focused(&ctx, &output, "Agent arguments (JSON array)");
    assert_eq!(
        keys::named(&output, "Agent arguments (JSON array)").0,
        arguments
    );
    assert_eq!(keys::named(&output, "Project folder").0, folder);
    assert_eq!(keys::named(&output, "Agent executable").0, executable);
    let invalid = keys::named(&output, "Agent arguments (JSON array)").1;
    assert_eq!(invalid.invalid(), Some(egui::accesskit::Invalid::True));
    assert!(invalid.error_message().is_some());
    assert!(!invalid.described_by().is_empty());
    assert!(keys::named(&output, "Connect agent").1.is_disabled());
    frame(
        &ctx,
        &mut studio,
        keys::key(Key::Enter, Modifiers::CTRL | Modifiers::COMMAND),
    );
    assert!(studio.agent.connection.is_none());
    keys::replace("[]", |events| frame(&ctx, &mut studio, events));
    let output = frame(&ctx, &mut studio, vec![]);
    assert_ne!(
        keys::named(&output, "Agent arguments (JSON array)")
            .1
            .invalid(),
        Some(egui::accesskit::Invalid::True)
    );
    assert!(!keys::named(&output, "Connect agent").1.is_disabled());
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    assert!(studio.agent.connection.is_none());
}

#[test]
fn keyboard_agent_native_brief_uses_new_document_and_conversation_owners() {
    let (ctx, mut studio) = fixture();
    frame(&ctx, &mut studio, vec![]);
    let output = press(&ctx, &mut studio, "Agent");
    let first = keys::named(&output, "Agent brief").0;
    keys::tab_to(&ctx, "Agent brief", |events| {
        frame(&ctx, &mut studio, events)
    });
    keys::replace("First canvas — 你好", |events| {
        frame(&ctx, &mut studio, events)
    });
    assert_eq!(studio.agent.request, "First canvas — 你好");
    let output = press(&ctx, &mut studio, "New agent conversation");
    let second = keys::named(&output, "Agent brief").0;
    assert_ne!(first, second);
    assert!(studio.agent.request.is_empty());
    keys::tab_to(&ctx, "Agent brief", |events| {
        frame(&ctx, &mut studio, events)
    });
    keys::replace("Second conversation", |events| {
        frame(&ctx, &mut studio, events)
    });
    let before = crate::project::encode(&studio.doc).unwrap();
    let history = studio.history.len();
    press(&ctx, &mut studio, "Agent connection settings");
    studio.agent.busy = true;
    let output = frame(&ctx, &mut studio, vec![]);
    assert!(keys::named(&output, "Agent provider").1.is_disabled());
    press(&ctx, &mut studio, "Stop agent");
    assert_eq!(studio.agent.status, "Stopping…");
    assert!(studio.agent.busy, "The existing Stop route waits for the provider cancellation result");
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    assert_eq!(studio.history.len(), history);
    assert!(studio.agent.connection.is_none());
    studio.open_document(
        crate::document::Document::new("Another native owner", 200., 200., 72.),
        None,
    );
    let output = frame(&ctx, &mut studio, vec![]);
    assert_ne!(keys::named(&output, "Agent brief").0, second);
    assert_eq!(studio.agent.request, "Second conversation");
    press(&ctx, &mut studio, "Hide agent panel");
    assert!(!studio.agent.visible);
}

#[test]
fn keyboard_agent_model_popup_search_filters_without_starting_provider_or_discovery() {
    let (ctx, mut studio) = fixture();
    studio.agent.discovery.providers = vec![crate::agent::discovery::Provider {
        profile: studio.agent.settings.profile.clone(),
        status: crate::agent::discovery::Status::Ready,
        metadata: serde_json::json!({"configOptions":[{
            "id":"model","name":"Model","category":"model","type":"select",
            "currentValue":"first","options":[{"value":"first","name":"First model"},{"value":"second","name":"Second model"}]
        }]}),
        detail: "Owned advertised metadata fixture".into(),
    }];
    frame(&ctx, &mut studio, vec![]);
    press(&ctx, &mut studio, "Agent");
    let output = press(&ctx, &mut studio, "Choose agent model");
    let search = keys::named(&output, "Search agent models").0;
    keys::tab_to(&ctx, "Search agent models", |events| {
        frame(&ctx, &mut studio, events)
    });
    keys::replace("Second", |events| frame(&ctx, &mut studio, events));
    let output = frame(&ctx, &mut studio, vec![]);
    keys::focused(&ctx, &output, "Search agent models");
    assert_eq!(keys::named(&output, "Search agent models").0, search);
    assert!(
        keys::named(&output, "Choose Codex model Second model")
            .1
            .supports_action(egui::accesskit::Action::Click)
    );
    let output = press(&ctx, &mut studio, "Filter agent models: Codex");
    assert_eq!(keys::named(&output, "Search agent models").0, search);
    assert_eq!(studio.agent.discovery.filter.as_deref(), Some("Codex"));
    assert!(studio.agent.settings.selections.is_empty());
    assert!(studio.agent.connection.is_none());
    assert!(!studio.agent.connecting && !studio.agent.busy);
}

#[test]
fn keyboard_agent_native_options_and_favorites_persist_only_in_the_owned_fixture() {
    use std::os::unix::fs::PermissionsExt;
    struct Files(PathBuf);
    impl Drop for Files {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let files = Files(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".artifacts/keyboard-agent-preferences")
            .join(crate::project::new_swap_id()),
    );
    std::fs::create_dir_all(files.0.join("project")).unwrap();
    let (ctx, mut studio) = fixture();
    studio.agent = Workspace::isolated_for_test(files.0.clone());
    studio.agent.discovery.providers = vec![crate::agent::discovery::Provider {
        profile: studio.agent.settings.profile.clone(),
        status: crate::agent::discovery::Status::Ready,
        metadata: serde_json::json!({"configOptions":[
            {"id":"model","name":"Model","category":"model","type":"select","currentValue":"first",
                "options":[{"value":"first","name":"First model"},{"value":"second","name":"Second model"}]},
            {"id":"effort","name":"Effort","category":"thought_level","type":"select","currentValue":"high",
                "options":[{"value":"high","name":"High"},{"value":"low","name":"Low"}]}
        ]}),
        detail: "Owned advertised metadata fixture".into(),
    }];
    frame(&ctx, &mut studio, vec![]);
    press(&ctx, &mut studio, "Agent");
    keys::tab_to(&ctx, "Effort", |events| frame(&ctx, &mut studio, events));
    frame(
        &ctx,
        &mut studio,
        keys::key(Key::ArrowDown, Modifiers::NONE),
    );
    assert_eq!(
        studio
            .agent
            .settings
            .selections
            .get("Codex/effort")
            .map(String::as_str),
        Some("low")
    );
    assert!(studio.agent.thread.is_none());
    press(&ctx, &mut studio, "Choose agent model");
    press(&ctx, &mut studio, "Favorite Codex model Second model");
    assert_eq!(studio.agent.settings.favorites, vec!["Codex/second"]);
    let settings_path = files.0.join("settings.json");
    let saved: crate::agent::config::Settings =
        serde_json::from_slice(&std::fs::read(&settings_path).unwrap()).unwrap();
    assert_eq!(saved.selections, studio.agent.settings.selections);
    assert_eq!(saved.favorites, studio.agent.settings.favorites);
    frame(&ctx, &mut studio, keys::key(Key::Escape, Modifiers::NONE));
    frame(&ctx, &mut studio, vec![]);
    press(&ctx, &mut studio, "New agent conversation");
    let thread = studio.agent.thread.as_ref().unwrap().id.clone();
    keys::tab_to(&ctx, "Effort", |events| frame(&ctx, &mut studio, events));
    frame(&ctx, &mut studio, keys::key(Key::Home, Modifiers::NONE));
    assert_eq!(
        studio
            .agent
            .settings
            .selections
            .get("Codex/effort")
            .map(String::as_str),
        Some("high")
    );
    assert!(studio.agent.connection.is_none() && !studio.agent.connecting && !studio.agent.busy);
    let expected = studio.agent.settings.clone();
    drop(studio);
    let saved: crate::agent::config::Settings =
        serde_json::from_slice(&std::fs::read(&settings_path).unwrap()).unwrap();
    let thread_path = files.0.join("threads").join(format!("{thread}.json"));
    let saved_thread: crate::agent::workspace::Thread =
        serde_json::from_slice(&std::fs::read(&thread_path).unwrap()).unwrap();
    assert_eq!(saved.selections, expected.selections);
    assert_eq!(saved_thread.settings.selections, expected.selections);
    assert_eq!(saved_thread.settings.favorites, expected.favorites);
    for path in [&settings_path, &thread_path] {
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
}
