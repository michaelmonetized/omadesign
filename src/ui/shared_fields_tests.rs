use super::fields::*;
use eframe::egui;

/// Run a headless UI frame and consume its texture updates.
/// Args: context, input and UI body. Returns frame output without pending renderer work.
pub(super) fn headless(
    ctx: &egui::Context,
    input: egui::RawInput,
    body: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    let mut output = ctx.run_ui(input, body);
    output.textures_delta.clear();
    output
}

mod tests {
    use super::*;
    #[test]
    fn shared_fields_fit_scales_and_selection_states() {
        for scale in [1.0, 1.5, 2.0] {
            for state in [
                FieldState::default(),
                FieldState {
                    mixed: true,
                    ..Default::default()
                },
                FieldState {
                    locked: true,
                    ..Default::default()
                },
                FieldState {
                    error: Some("Value out of range"),
                    ..Default::default()
                },
            ] {
                let ctx = egui::Context::default();
                ctx.set_pixels_per_point(scale);
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(320.0, 600.0),
                    )),
                    ..Default::default()
                };
                let mut value = 50.0;
                let _ = headless(&ctx, input, |ui| {
                    let rect = ui.max_rect();
                    let result = number(
                        ui,
                        "gain",
                        "Long property label that must wrap",
                        &mut value,
                        0.0..=100.0,
                        " %",
                        Some(100.0),
                        state,
                    );
                    assert!(rect.contains_rect(result.response.rect));
                    assert!(!result.changed);
                    assert_eq!(result.response.enabled(), !state.locked);
                });
                assert_eq!(value, 50.0);
            }
        }
    }
}

#[test]
fn shared_field_drag_reports_preview_then_final_commit() {
    let ctx = egui::Context::default();
    let mut value = 10.0;
    let mut time = 0.0;
    let mut frame = |events| {
        time += 0.02;
        let mut rect = egui::Rect::NOTHING;
        let mut flags = [false; 3];
        let _ = headless(
            &ctx,
            egui::RawInput {
                time: Some(time),
                events,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(320.0, 200.0),
                )),
                ..Default::default()
            },
            |ui| {
                let result = number(
                    ui,
                    "drag-contract",
                    "Position",
                    &mut value,
                    -100.0..=100.0,
                    " px",
                    None,
                    Default::default(),
                );
                rect = result.response.rect;
                flags[0] |= result.changed;
                flags[1] |= result.committed;
                flags[2] |= result.cancelled;
            },
        );
        (rect, flags)
    };
    let (rect, _) = frame(vec![]);
    let start = rect.center();
    frame(vec![
        egui::Event::PointerMoved(start),
        egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        },
    ]);
    let end = start + egui::vec2(30.0, 0.0);
    let (_, flags) = frame(vec![egui::Event::PointerMoved(end)]);
    assert!(flags[0], "drag must preview a changed value");
    assert!(!flags[1], "preview cannot commit every frame");
    let (_, flags) = frame(vec![egui::Event::PointerButton {
        pos: end,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    }]);
    assert!(flags[1], "release must commit the gesture");
    assert!(!flags[2]);
}

#[test]
fn shared_fields_never_clamp_existing_large_or_locked_mixed_values_on_render() {
    for (original, range) in [
        (12_000_000.0, -100000.0..=100000.0),
        (-6_000_000.0, -100000.0..=100000.0),
        (2_000_000.0, 1.0..=20000.0),
        (720.0, -180.0..=180.0),
    ] {
        for state in [
            FieldState::default(),
            FieldState {
                mixed: true,
                ..Default::default()
            },
            FieldState {
                mixed: true,
                locked: true,
                ..Default::default()
            },
        ] {
            let ctx = egui::Context::default();
            let mut value = original;
            for _ in 0..3 {
                let _ = headless(&ctx, egui::RawInput::default(), |ui| {
                    let result = number(
                        ui,
                        "large-existing",
                        "Imported value",
                        &mut value,
                        range.clone(),
                        " px",
                        None,
                        state,
                    );
                    assert!(!result.changed);
                    assert!(!result.committed);
                });
                assert_eq!(value, original);
            }
        }
    }
}
