use crate::app::{
    Studio,
    keyboard_authoring::{ArtworkDraft, ArtworkKind},
};
use eframe::egui::{self, Id, Key, Modifiers};

const ID: &str = "keyboard-create-artwork";

#[derive(Clone)]
struct Dialog {
    draft: ArtworkDraft,
    focus: bool,
    error: Option<String>,
}

pub(super) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<Dialog>(Id::new(ID)).is_some())
}

/// Open numeric artwork creation without a canvas gesture.
/// Args: context and editor. Returns after capturing the current document target.
pub(crate) fn open(ctx: &egui::Context, studio: &mut Studio) {
    if !matches!(
        studio.persona,
        crate::tools::Persona::Design
            | crate::tools::Persona::Layout
            | crate::tools::Persona::Motion
    ) || studio.show_welcome
        || studio.op.is_some()
        || studio.type_edit.is_some()
    {
        return;
    }
    let kind = ArtworkKind::for_tool(studio.tool).unwrap_or(ArtworkKind::Rectangle);
    let draft = ArtworkDraft::new(studio, kind);
    ctx.data_mut(|data| {
        data.insert_temp(
            Id::new(ID),
            Dialog {
                draft,
                focus: true,
                error: None,
            },
        )
    });
}

/// Draw a named input and associate its validation failure.
/// Args: UI, identity, label, draft text, error and multiline mode. Returns the native widget response.
fn input(
    ui: &mut egui::Ui,
    id: &str,
    label: &str,
    text: &mut String,
    error: Option<&str>,
    multiline: bool,
) -> egui::Response {
    ui.push_id((ID, id), |ui| {
        let label = ui.label(label);
        let edit = if multiline {
            egui::TextEdit::multiline(text).desired_rows(3)
        } else {
            egui::TextEdit::singleline(text)
        };
        let response = ui.add(edit.id(Id::new((ID, id)))).labelled_by(label.id);
        if response.has_focus() {
            ui.painter().rect_stroke(
                response.rect.expand(2.),
                3.,
                ui.visuals().selection.stroke,
                egui::StrokeKind::Outside,
            );
        }
        if let Some(error) = error {
            let message = ui.colored_label(ui.visuals().error_fg_color, error);
            ui.ctx().accesskit_node_builder(response.id, |node| {
                node.set_invalid(egui::accesskit::Invalid::True);
                node.set_error_message(message.id.accesskit_id());
            });
            ui.ctx().accesskit_node_builder(message.id, |node| {
                node.set_live(egui::accesskit::Live::Polite)
            });
        }
        response
    })
    .inner
}

pub(super) fn show(ctx: &egui::Context, studio: &mut Studio) {
    let Some(mut dialog) = ctx.data(|data| data.get_temp::<Dialog>(Id::new(ID))) else {
        return;
    };
    let current = dialog.draft.current(studio);
    let available = current && !studio.file_dialog_pending() && !studio.updates.freezing;
    let mut create = false;
    let mut cancel = false;
    let response = egui::Modal::new(Id::new(ID)).show(ctx, |ui| {
        ui.set_width(360.);
        ui.heading("Create artwork");
        let kind_label = ui.label("Artwork type");
        egui::ComboBox::from_id_salt((ID, "kind"))
            .selected_text(dialog.draft.kind.label())
            .show_ui(ui, |ui| {
                for kind in ArtworkKind::ALL {
                    ui.selectable_value(&mut dialog.draft.kind, kind, kind.label());
                }
            })
            .response
            .labelled_by(kind_label.id);
        for (index, (id, label)) in [
            ("x", "Position X (px)"),
            ("y", "Position Y (px)"),
            ("width", "Width (px)"),
            ("height", "Height (px)"),
        ]
        .into_iter()
        .enumerate()
        {
            if index >= 2 && matches!(dialog.draft.kind, ArtworkKind::Path | ArtworkKind::Text) {
                continue;
            }
            let error = dialog.draft.number(index).err();
            let field = input(
                ui,
                id,
                label,
                &mut dialog.draft.coordinates[index],
                error.as_deref(),
                false,
            );
            if dialog.focus && index == 0 {
                field.request_focus();
            }
        }
        if dialog.draft.kind == ArtworkKind::Text {
            let error = dialog
                .draft
                .text
                .trim()
                .is_empty()
                .then_some("Text: enter some text");
            input(ui, "text", "Text", &mut dialog.draft.text, error, true);
        }
        if dialog.draft.kind == ArtworkKind::Path {
            let error = dialog
                .draft
                .validate()
                .err()
                .filter(|s| s.starts_with("Path points:"));
            input(
                ui,
                "points",
                "Path points relative to position (x,y; x,y)",
                &mut dialog.draft.points,
                error.as_deref(),
                true,
            );
            ui.checkbox(&mut dialog.draft.closed, "Close path");
        }
        if !current {
            let warning = ui.colored_label(
                ui.visuals().error_fg_color,
                "The document or selection changed. Cancel and open Create artwork again.",
            );
            ui.ctx().accesskit_node_builder(warning.id, |node| {
                node.set_live(egui::accesskit::Live::Polite)
            });
        }
        if let Some(error) = &dialog.error {
            let warning = ui.colored_label(ui.visuals().error_fg_color, error);
            ui.ctx().accesskit_node_builder(warning.id, |node| {
                node.set_live(egui::accesskit::Live::Polite)
            });
        }
        let valid = dialog.draft.validate().is_ok();
        ui.horizontal(|ui| {
            create = ui
                .add_enabled(
                    available && valid,
                    egui::Button::new("Create").shortcut_text("Ctrl+Enter"),
                )
                .clicked();
            cancel = ui.button("Cancel").clicked();
        });
        if available
            && valid
            && ui.input_mut(|input| {
                input.consume_key(Modifiers::CTRL, Key::Enter)
                    || input.consume_key(Modifiers::COMMAND, Key::Enter)
            })
        {
            create = true;
        }
    });
    dialog.focus = false;
    let created = if create {
        match studio.create_keyboard_artwork(&dialog.draft) {
            Ok(()) => true,
            Err(error) => {
                dialog.error = Some(error);
                false
            }
        }
    } else {
        false
    };
    let close = created || cancel || response.should_close();
    ctx.data_mut(|data| {
        if close {
            data.remove::<Dialog>(Id::new(ID));
        } else {
            data.insert_temp(Id::new(ID), dialog);
        }
    });
    if close {
        ctx.memory_mut(|memory| memory.request_focus(Id::new("studio-canvas")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        geom::Geom,
        tools::{Persona, Tool},
    };

    fn key(key: Key, modifiers: Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn frame(
        ctx: &egui::Context,
        studio: &mut Studio,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let events = events.into_iter().flat_map(|event| {
            if let egui::Event::Key { key, physical_key, pressed: true, modifiers, .. } = &event {
                let release = egui::Event::Key { key: *key, physical_key: *physical_key, pressed: false, repeat: false, modifiers: *modifiers };
                vec![event, release]
            } else {
                vec![event]
            }
        }).collect();
        let mut output = ctx.run_ui(
            egui::RawInput {
                events,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 720.),
                )),
                ..Default::default()
            },
            |ui| {
                studio.handle_shortcuts(ui.ctx());
                ui.interact(
                    ui.max_rect(),
                    Id::new("studio-canvas"),
                    egui::Sense::click(),
                );
                show(ui.ctx(), studio);
            },
        );
        output.textures_delta.clear();
        output
    }

    fn editor() -> (egui::Context, Studio) {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut studio = Studio::new();
        studio.doc = crate::document::Document::new("Keyboard artwork", 128., 96., 72.);
        studio.show_welcome = false;
        (ctx, studio)
    }

    fn chord() -> Modifiers {
        Modifiers::CTRL | Modifiers::COMMAND
    }

    fn replace(ctx: &egui::Context, studio: &mut Studio, value: &str) {
        frame(
            ctx,
            studio,
            vec![key(Key::A, chord()), egui::Event::Text(value.into())],
        );
    }

    #[test]
    fn keyboard_authoring_creates_selects_edits_and_reopens_without_pointer_setup() {
        let (ctx, mut studio) = editor();
        assert!(studio.selection.is_empty());
        frame(
            &ctx,
            &mut studio,
            vec![
                key(Key::R, Modifiers::NONE),
                key(Key::Enter, Modifiers::NONE),
            ],
        );
        frame(&ctx, &mut studio, vec![]);
        assert!(is_open(&ctx));
        for (index, value) in ["20", "30", "80", "40"].into_iter().enumerate() {
            replace(&ctx, &mut studio, value);
            if index < 3 {
                frame(&ctx, &mut studio, vec![key(Key::Tab, Modifiers::NONE)]);
            }
        }
        frame(&ctx, &mut studio, vec![key(Key::Enter, chord())]);
        assert!(!is_open(&ctx));
        frame(&ctx, &mut studio, vec![]);
        let first = studio.primary().unwrap();
        let Geom::Rect { origin, size, .. } = studio.doc.find_shape(first.0, first.1).unwrap().geom
        else {
            panic!("rectangle");
        };
        assert_eq!(origin, crate::geom::Pt::new(20., 30.));
        assert_eq!(size, crate::geom::Pt::new(80., 40.));
        frame(
            &ctx,
            &mut studio,
            vec![key(Key::ArrowRight, Modifiers::SHIFT)],
        );
        assert_eq!(
            studio
                .doc
                .find_shape(first.0, first.1)
                .unwrap()
                .geom
                .bbox()
                .min
                .x,
            30.
        );
        frame(
            &ctx,
            &mut studio,
            vec![
                key(Key::O, Modifiers::NONE),
                key(Key::Enter, Modifiers::NONE),
            ],
        );
        frame(&ctx, &mut studio, vec![key(Key::Enter, chord())]);
        let second = studio.primary().unwrap();
        frame(&ctx, &mut studio, vec![]);
        assert_ne!(first, second);
        frame(&ctx, &mut studio, vec![key(Key::PageUp, Modifiers::NONE)]);
        assert_eq!(studio.selection, vec![first]);
        frame(
            &ctx,
            &mut studio,
            vec![key(Key::PageDown, Modifiers::SHIFT)],
        );
        assert_eq!(studio.selection, vec![first, second]);
        frame(
            &ctx,
            &mut studio,
            vec![
                key(Key::T, Modifiers::NONE),
                key(Key::Enter, Modifiers::NONE),
            ],
        );
        frame(&ctx, &mut studio, vec![key(Key::Enter, chord())]);
        frame(&ctx, &mut studio, vec![]);
        frame(
            &ctx,
            &mut studio,
            vec![
                key(Key::A, chord()),
                egui::Event::Text("Keyboard artwork".into()),
                key(Key::Escape, Modifiers::NONE),
            ],
        );
        assert!(studio.type_edit.is_none());
        let text = studio.primary().unwrap();
        assert!(
            matches!(&studio.doc.find_shape(text.0, text.1).unwrap().geom, Geom::Text(run) if run.content == "Keyboard artwork")
        );
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".artifacts")
            .join(format!(
                "keyboard-authoring-test-{}-{}",
                std::process::id(),
                crate::document::next_id()
            ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("keyboard.oma");
        crate::project::save_to(&studio.doc, &path).unwrap();
        let reopened = crate::project::load_from(&path).unwrap();
        assert_eq!(
            crate::project::encode(&reopened).unwrap(),
            crate::project::encode(&studio.doc).unwrap()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn keyboard_authoring_invalid_field_keeps_value_and_publishes_error_association() {
        let (ctx, mut studio) = editor();
        frame(
            &ctx,
            &mut studio,
            vec![key(Key::K, chord() | Modifiers::SHIFT)],
        );
        frame(&ctx, &mut studio, vec![]);
        replace(&ctx, &mut studio, "NaN");
        let output = frame(&ctx, &mut studio, vec![key(Key::Enter, chord())]);
        assert!(studio.selection.is_empty());
        assert!(is_open(&ctx));
        let tree = output.platform_output.accesskit_update.unwrap();
        let (_, field) = tree
            .nodes
            .iter()
            .find(|(id, _)| *id == Id::new((ID, "x")).accesskit_id())
            .unwrap();
        assert_eq!(field.invalid(), Some(egui::accesskit::Invalid::True));
        assert!(field.error_message().is_some());
        replace(&ctx, &mut studio, "25");
        frame(&ctx, &mut studio, vec![key(Key::Enter, chord())]);
        assert!(!is_open(&ctx));
        assert_eq!(studio.selection.len(), 1);
    }

    #[test]
    fn keyboard_authoring_rejects_stale_busy_locked_and_modal_or_field_shortcuts() {
        let (ctx, mut studio) = editor();
        let draft = ArtworkDraft::new(&mut studio, ArtworkKind::Rectangle);
        let target = studio.vector_target().unwrap();
        studio.doc.layers[target].locked = true;
        assert!(
            studio
                .create_keyboard_artwork(&draft)
                .unwrap_err()
                .contains("locked")
        );
        studio.doc.layers[target].locked = false;
        studio.updates.freezing = true;
        assert!(studio.create_keyboard_artwork(&draft).is_err());
        studio.updates.freezing = false;
        studio.canvas_gen += 1;
        assert!(
            studio
                .create_keyboard_artwork(&draft)
                .unwrap_err()
                .contains("changed")
        );
        frame(
            &ctx,
            &mut studio,
            vec![key(Key::K, chord() | Modifiers::SHIFT)],
        );
        frame(
            &ctx,
            &mut studio,
            vec![
                key(Key::O, Modifiers::NONE),
                key(Key::PageDown, Modifiers::NONE),
            ],
        );
        assert_eq!(studio.tool, Tool::Select);
        assert!(studio.selection.is_empty());
        frame(&ctx, &mut studio, vec![key(Key::Escape, Modifiers::NONE)]);
        assert!(!is_open(&ctx));
        studio.persona = Persona::Photo;
        frame(
            &ctx,
            &mut studio,
            vec![key(Key::K, chord() | Modifiers::SHIFT)],
        );
        assert!(!is_open(&ctx));
    }
}
