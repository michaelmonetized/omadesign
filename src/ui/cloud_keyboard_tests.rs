use super::*;
use crate::ui::welcome::keyboard_tests as keys;
use egui::{Event, Key, Modifiers};

fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
    keys::render(ctx, events, |ui| modal(ui, studio))
}

fn fixture(kind: CloudModal) -> (egui::Context, Studio) {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    super::super::theme::apply(&ctx);
    let mut studio = Studio::new();
    studio.cloud_modal = kind;
    (ctx, studio)
}

fn edit(ctx: &egui::Context, studio: &mut Studio, name: &str, value: &str) -> egui::FullOutput {
    keys::tab_to(ctx, name, |events| frame(ctx, studio, events));
    keys::replace(value, |events| frame(ctx, studio, events));
    frame(ctx, studio, vec![])
}

fn close(ctx: &egui::Context, studio: &mut Studio) {
    keys::tab_to(ctx, "Close cloud dialog", |events| {
        frame(ctx, studio, events)
    });
    frame(ctx, studio, keys::key(Key::Enter, Modifiers::NONE));
    assert!(studio.cloud_modal == CloudModal::None);
}

#[test]
fn keyboard_cloud_invitation_native_email_validation_focus_and_fresh_dialog_owner() {
    let (ctx, mut studio) = fixture(CloudModal::Invite);
    let before = crate::project::encode(&studio.doc).unwrap();
    let history = studio.history.len();
    frame(&ctx, &mut studio, vec![]);
    frame(&ctx, &mut studio, vec![]);
    let output = frame(&ctx, &mut studio, vec![]);
    let email = keys::named(&output, "Reviewer email").0;
    keys::focused(&ctx, &output, "Reviewer email");
    keys::associated_error(&output, "Reviewer email");
    assert!(
        keys::named(&output, "Send review invitation")
            .1
            .is_disabled()
    );
    let output = edit(&ctx, &mut studio, "Reviewer email", "a @example.org");
    assert_eq!(keys::named(&output, "Reviewer email").0, email);
    keys::associated_error(&output, "Reviewer email");
    let output = edit(&ctx, &mut studio, "Reviewer email", "reviewer@example.org");
    assert!(
        !keys::named(&output, "Send review invitation")
            .1
            .is_disabled()
    );
    assert_ne!(
        keys::named(&output, "Reviewer email").1.invalid(),
        Some(egui::accesskit::Invalid::True)
    );
    assert_eq!(keys::named(&output, "Reviewer email").0, email);
    close(&ctx, &mut studio);
    studio.cloud_modal = CloudModal::Invite;
    frame(&ctx, &mut studio, vec![]);
    let output = frame(&ctx, &mut studio, vec![]);
    assert_ne!(keys::named(&output, "Reviewer email").0, email);
    assert_eq!(studio.invite_email, "reviewer@example.org");
    assert!(!studio.cloud_busy());
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    assert_eq!(studio.history.len(), history);
    close(&ctx, &mut studio);
}

#[test]
fn keyboard_cloud_review_snapshot_identity_utf16_validation_and_stale_native_target_guards() {
    let (ctx, mut studio) = fixture(CloudModal::Review);
    studio.cloud_panel.project_id = "owned-native-ui-fixture".into();
    studio.cloud_panel.files = vec![
        crate::cloud::client::File {
            id: "first-export".into(),
            name: "First export".into(),
            kind: "snapshot".into(),
            version: 1,
            created: 0.,
            width: None,
            height: None,
        },
        crate::cloud::client::File {
            id: "second-export".into(),
            name: "Second export".into(),
            kind: "snapshot".into(),
            version: 2,
            created: 0.,
            width: None,
            height: None,
        },
    ];
    studio.cloud_panel.selected_snapshot = "missing-export".into();
    studio.cloud_panel.annotations = vec![
        json!({"_id":"thread-a","snapshotId":"first-export","authorName":"Alice","body":"First note","resolved":false}),
        json!({"_id":"thread-b","snapshotId":"first-export","authorName":"Bob","body":"Second note","resolved":false}),
    ];
    let before = crate::project::encode(&studio.doc).unwrap();
    frame(&ctx, &mut studio, vec![]);
    let output = frame(&ctx, &mut studio, vec![]);
    keys::associated_error(&output, "Cloud flat export");
    assert!(
        keys::named(&output, "View selected cloud export")
            .1
            .is_disabled()
    );
    keys::tab_to(&ctx, "Cloud flat export", |events| {
        frame(&ctx, &mut studio, events)
    });
    frame(&ctx, &mut studio, keys::key(Key::Home, Modifiers::NONE));
    assert_eq!(studio.cloud_panel.selected_snapshot, "first-export");
    let output = frame(&ctx, &mut studio, vec![]);
    let reply = keys::named(&output, "Cloud reply draft").0;
    let thread = keys::named(&output, "Resolve review thread by Alice: First note").0;
    studio.cloud_panel.annotations.reverse();
    let output = frame(&ctx, &mut studio, vec![]);
    assert_eq!(
        keys::named(&output, "Resolve review thread by Alice: First note").0,
        thread
    );
    assert_eq!(keys::named(&output, "Cloud reply draft").0, reply);
    let output = edit(&ctx, &mut studio, "Showcase work title", &"🟣".repeat(101));
    keys::associated_error(&output, "Showcase work title");
    assert!(
        keys::named(&output, "Publish selected cloud export")
            .1
            .is_disabled()
    );
    edit(&ctx, &mut studio, "Showcase work title", "Native poster");
    let output = edit(
        &ctx,
        &mut studio,
        "Showcase description",
        "A finished design",
    );
    assert!(
        !keys::named(&output, "Publish selected cloud export")
            .1
            .is_disabled()
    );
    keys::tab_to(&ctx, "Cloud flat export", |events| {
        frame(&ctx, &mut studio, events)
    });
    frame(
        &ctx,
        &mut studio,
        keys::key(Key::ArrowDown, Modifiers::NONE),
    );
    assert_eq!(studio.cloud_panel.selected_snapshot, "second-export");
    let output = frame(&ctx, &mut studio, vec![]);
    assert_ne!(keys::named(&output, "Cloud reply draft").0, reply);
    assert!(!studio.cloud_busy());
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    close(&ctx, &mut studio);
}
