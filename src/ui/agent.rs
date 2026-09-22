//! Free-form prompts handed to the user's configured Omarchy agent.
use crate::agent::Purpose;
use crate::app::Studio;
use eframe::egui::{self, Id, RichText};
use std::path::PathBuf;

const STATE: &str = "welcome-agent-prompt";
const LAUNCH: &str = "welcome-agent-launch";

#[derive(Clone)]
struct Prompt {
    purpose: Purpose,
    request: String,
    directory: PathBuf,
    message: String,
    focus: bool,
}

pub(super) fn open(ctx: &egui::Context, purpose: Purpose, directory: Option<PathBuf>) {
    let directory = directory.unwrap_or_else(|| {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into()));
        if home.join("Work").is_dir() {
            home.join("Work")
        } else {
            home
        }
    });
    ctx.data_mut(|d| {
        d.insert_temp(
            Id::new(STATE),
            Prompt {
                purpose,
                request: String::new(),
                directory,
                message: String::new(),
                focus: true,
            },
        )
    });
}

pub(super) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<Prompt>(Id::new(STATE)).is_some())
}

pub(super) fn show(ctx: &egui::Context, studio: &mut Studio) {
    let Some(mut state) = ctx.data(|d| d.get_temp::<Prompt>(Id::new(STATE))) else {
        return;
    };
    if let Some(result) = super::jobs::poll::<String>(ctx, LAUNCH) {
        match result {
            Ok(message) => {
                studio.status = message;
                ctx.data_mut(|d| d.remove::<Prompt>(Id::new(STATE)));
                return;
            }
            Err(error) => state.message = error,
        }
    }
    let busy = super::jobs::is_running::<String>(ctx, LAUNCH);
    let mut close = false;
    let title = match state.purpose {
        Purpose::Learn => "Learn with AI",
        Purpose::Create => "Create with agent",
    };
    let response = egui::Modal::new(Id::new("agent-prompt-dialog")).show(ctx, |ui| {
        ui.set_width((ctx.content_rect().width() - 80.).clamp(260., 540.));
        ui.heading(title);
        ui.add_space(8.);
        ui.label(match state.purpose {
            Purpose::Learn => "What would you like to know? Your agent receives Omadesign's Markdown documentation sources.",
            Purpose::Create => "What would you like to create? Your agent receives the Omadesign creation skill and your brief.",
        });
        ui.add_space(10.);
        let field = ui.add_enabled(!busy, egui::TextEdit::multiline(&mut state.request)
            .desired_width(f32::INFINITY).desired_rows(6).hint_text(match state.purpose {
                Purpose::Learn => "Ask anything…",
                Purpose::Create => "Describe the artwork, layout, or project you have in mind…",
            }));
        if state.focus { field.request_focus(); state.focus = false; }
        ui.add_space(8.);
        ui.label(RichText::new(format!("Folder: {}", state.directory.display())).small().color(super::theme::fg_weak()));
        if !state.message.is_empty() {
            ui.add_space(8.);
            ui.label(&state.message);
            ui.small("Agent selection is managed by Omarchy. After choosing one, try again.");
        }
        ui.add_space(12.);
        ui.horizontal(|ui| {
            if ui.add_enabled(!busy, egui::Button::new("Cancel")).clicked() { close = true; }
            if busy { ui.spinner(); ui.label("Opening your agent…"); }
            if ui.add_enabled(!busy && !state.request.trim().is_empty(), egui::Button::new("Open agent").fill(super::theme::accent_soft())).clicked() {
                let request = state.request.clone();
                let directory = state.directory.clone();
                let purpose = state.purpose;
                super::jobs::start(ctx, LAUNCH, move || crate::agent::launch(purpose, &request, &directory));
                state.message.clear();
            }
        });
    });
    if !busy && (close || response.should_close()) {
        ctx.data_mut(|d| d.remove::<Prompt>(Id::new(STATE)));
    } else {
        ctx.data_mut(|d| d.insert_temp(Id::new(STATE), state));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freeform_prompt_takes_keyboard_input_without_editing_the_document() {
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut studio = Studio::new();
        let before = crate::project::encode(&studio.doc).unwrap();
        let directory = PathBuf::from("/tmp/omadesign project");
        for purpose in [Purpose::Learn, Purpose::Create] {
            open(&ctx, purpose, Some(directory.clone()));
            let mut frame = |events| {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(960., 640.),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| crate::ui::run(ui, &mut studio),
                );
                output.textures_delta.clear();
            };
            frame(vec![]);
            frame(vec![]);
            frame(vec![egui::Event::Text(
                "Make a poster — 你好\nwith 'quotes' & $symbols".into(),
            )]);
            let state = ctx.data(|d| d.get_temp::<Prompt>(Id::new(STATE))).unwrap();
            assert_eq!(
                state.request,
                "Make a poster — 你好\nwith 'quotes' & $symbols"
            );
            assert_eq!(state.directory, directory);
            assert_eq!(state.purpose, purpose);
            assert!(!super::super::jobs::is_running::<String>(&ctx, LAUNCH));
            assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
        }
    }
}
