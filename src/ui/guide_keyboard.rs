//! Numeric alternatives for the existing ruler guide commands.
use crate::{app::Studio, document::Guide, geom::Pt, tools::Persona};
use eframe::egui::{self, Id, Response, Ui};
use std::cell::RefCell;

const ID: &str = "ruler-guide-positions";

#[derive(Clone)]
struct Draft {
    document: String,
    revision: u64,
    guides: Vec<Guide>,
    instance: Id,
    selected: Option<usize>,
    position: f32,
    origin: Pt,
    focus: bool,
}

impl Draft {
    /// Check the captured document and guide-list lifetime.
    /// Args: current editor. Returns false after replacement, removal, Undo or another edit.
    fn current(&self, studio: &Studio) -> bool {
        self.document == studio.swap_id
            && self.revision == studio.canvas_gen
            && self.guides == studio.doc.guides
    }
}

/// Check ordinary document availability without touching guide or canvas state.
/// Args: editor. Returns whether a new numeric guide draft may be opened.
fn available(studio: &Studio) -> bool {
    !studio.show_welcome
        && studio.persona != Persona::Photo
        && !studio.file_dialog_pending()
        && !studio.updates.freezing
        && studio.op.is_none()
        && studio.type_edit.is_none()
}

/// Read whether the app-owned guide dialog is open.
/// Args: UI context. Returns the current modal lifetime, including stale drafts.
pub(super) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<Draft>(Id::new(ID)).is_some())
}

/// Add the numeric alternative to the existing Guides menu.
/// Args: menu UI and editor. Returns nothing; opening captures a new document-owned draft.
pub(super) fn menu(ui: &mut Ui, studio: &Studio) {
    let response = control(ui, Id::new((ID, "open")), "Guide positions…", |ui| {
        ui.add_enabled(available(studio), egui::Button::new("Guide positions…"))
    });
    if response.clicked() {
        let draft = Draft {
            document: studio.swap_id.clone(),
            revision: studio.canvas_gen,
            guides: studio.doc.guides.clone(),
            instance: super::keyboard_controls::draft_id(ui.ctx(), "guide-positions"),
            selected: None,
            position: 0.,
            origin: studio.doc.ruler.origin,
            focus: true,
        };
        ui.ctx()
            .data_mut(|data| data.insert_temp(Id::new(ID), draft));
        ui.close();
    }
}

/// Decorate the existing native control in an explicit semantic scope.
/// Args: UI, stable owner, purpose and native widget. Returns the actual response.
fn control(
    ui: &mut Ui,
    owner: Id,
    name: &str,
    widget: impl FnOnce(&mut Ui) -> Response,
) -> Response {
    let response = super::fields::native_scope(ui, owner, widget).inner;
    ui.ctx()
        .accesskit_node_builder(response.id, |node| node.set_label(name));
    super::welcome::focus_control(ui, &response);
    response
}

/// Draw a finite guide coordinate while retaining its exact f32 text on focus and blur.
/// Args: UI, property owner, name and draft coordinate. Returns its native response and validity.
fn coordinate(ui: &mut Ui, owner: Id, name: &str, value: &mut f32) -> (Response, bool) {
    let before = *value;
    let attempt = RefCell::new(None);
    let label = ui.label(name);
    let response = control(ui, owner, name, |ui| {
        ui.add(
            egui::DragValue::new(value)
                .speed(0.1)
                .custom_formatter(|value, _| (value as f32).to_string())
                .custom_parser(|text| super::welcome::parse_number(&attempt, text)),
        )
        .labelled_by(label.id)
    });
    let valid = super::welcome::native_number_valid(
        ui,
        &response,
        owner,
        attempt.into_inner(),
        *value as f64,
        f64::NEG_INFINITY..=f64::INFINITY,
    );
    if !valid {
        *value = before;
    }
    if !valid && response.enabled() {
        super::welcome::control_error(ui, &[&response], "Enter a finite coordinate in pixels");
    }
    (response, valid)
}

#[derive(Clone, Copy)]
enum Action {
    Add(bool),
    Move,
    Remove,
    Origin,
}

/// Show the app-owned modal and apply only existing typed guide commands.
/// Args: context and editor. Returns nothing; each accepted command ends this captured list lifetime.
pub(super) fn show(ctx: &egui::Context, studio: &mut Studio) {
    let Some(mut draft) = ctx.data(|data| data.get_temp::<Draft>(Id::new(ID))) else {
        return;
    };
    let current = draft.current(studio);
    let enabled = current && available(studio);
    let guides_enabled =
        enabled && studio.doc.ruler.guides_visible && !studio.doc.ruler.guides_locked;
    let owner = Id::new((ID, &draft.document, draft.instance));
    let mut action = None;
    let mut close = false;
    let modal = egui::Modal::new(Id::new(ID)).show(ctx, |ui| {
        ui.set_width(380.);
        ui.heading("Guide positions");
        if !current {
            ui.label("The document or guides changed. Close and reopen Guide positions.");
        } else if !studio.doc.ruler.guides_visible || studio.doc.ruler.guides_locked {
            ui.label("Show and unlock guides to add, move or remove them.");
        }
        ui.add_enabled_ui(guides_enabled, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(owner.with("list"))
                .max_height(130.)
                .show(ui, |ui| {
                    for (index, guide) in draft.guides.iter().enumerate() {
                        let name = format!(
                            "{} guide {}",
                            if guide.vertical {
                                "Vertical"
                            } else {
                                "Horizontal"
                            },
                            index + 1
                        );
                        let response = control(ui, owner.with(("guide", index)), &name, |ui| {
                            ui.selectable_label(
                                draft.selected == Some(index),
                                format!("{name} · {} px", guide.pos),
                            )
                        });
                        if response.clicked() {
                            draft.selected = Some(index);
                            draft.position = guide.pos;
                        }
                    }
                });
            let field_owner = owner.with(("position", draft.selected));
            let (position, valid) =
                coordinate(ui, field_owner, "Guide position", &mut draft.position);
            if draft.focus {
                position.request_focus();
                draft.focus = false;
            }
            ui.horizontal_wrapped(|ui| {
                for (vertical, label) in [
                    (true, "Add vertical guide"),
                    (false, "Add horizontal guide"),
                ] {
                    let button = control(ui, owner.with(("add", vertical)), label, |ui| {
                        ui.add_enabled(valid, egui::Button::new(label))
                    });
                    if button.clicked() && guides_enabled && valid {
                        action = Some(Action::Add(vertical));
                    }
                }
                let selected = draft
                    .selected
                    .is_some_and(|index| draft.guides.get(index).is_some());
                let move_button = control(ui, owner.with("move"), "Move selected guide", |ui| {
                    ui.add_enabled(selected && valid, egui::Button::new("Move selected guide"))
                });
                if move_button.clicked() && guides_enabled && selected && valid {
                    action = Some(Action::Move);
                }
                let remove = control(ui, owner.with("remove"), "Remove selected guide", |ui| {
                    ui.add_enabled(selected, egui::Button::new("Remove selected guide"))
                });
                if remove.clicked() && guides_enabled && selected {
                    action = Some(Action::Remove);
                }
            });
        });
        ui.separator();
        ui.add_enabled_ui(enabled, |ui| {
            let (_, x_valid) = coordinate(
                ui,
                owner.with("origin-x"),
                "Ruler zero X",
                &mut draft.origin.x,
            );
            let (_, y_valid) = coordinate(
                ui,
                owner.with("origin-y"),
                "Ruler zero Y",
                &mut draft.origin.y,
            );
            let button = control(ui, owner.with("origin-apply"), "Set ruler zero", |ui| {
                ui.add_enabled(x_valid && y_valid, egui::Button::new("Set ruler zero"))
            });
            if button.clicked() && enabled && x_valid && y_valid {
                action = Some(Action::Origin);
            }
        });
        let cancel = control(ui, owner.with("close"), "Close guide positions", |ui| {
            ui.button("Close")
        });
        if !current && !cancel.has_focus() {
            cancel.request_focus();
        }
        close = cancel.clicked();
    });
    close |= modal.should_close();
    if !close && draft.current(studio) && available(studio) {
        match action {
            Some(Action::Add(vertical)) if guides_enabled && draft.position.is_finite() => {
                studio.add_guide(vertical, draft.position);
                close = true;
            }
            Some(Action::Move) if guides_enabled && draft.position.is_finite() => {
                if let Some(index) = draft.selected {
                    studio.move_guide(index, draft.position);
                    close = true;
                }
            }
            Some(Action::Remove) if guides_enabled => {
                if let Some(index) = draft.selected {
                    studio.remove_guide(index);
                    close = true;
                }
            }
            Some(Action::Origin) if draft.origin.x.is_finite() && draft.origin.y.is_finite() => {
                studio.set_ruler_origin(draft.origin);
                close = true;
            }
            _ => {}
        }
    }
    if close {
        ctx.data_mut(|data| data.remove::<Draft>(Id::new(ID)));
        ctx.memory_mut(|memory| memory.request_focus(Id::new("studio-canvas")));
    } else {
        ctx.data_mut(|data| data.insert_temp(Id::new(ID), draft));
    }
}

#[cfg(test)]
mod tests {
    use super::super::welcome::keyboard_tests as keys;
    use super::*;
    use egui::{Event, Key, Modifiers};

    fn fixture() -> (egui::Context, Studio) {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        super::super::theme::apply(&ctx);
        let mut studio = Studio::new();
        studio.show_welcome = false;
        studio.doc = crate::document::Document::new("Native guide routes", 640., 480., 72.);
        studio.selection.clear();
        (ctx, studio)
    }

    fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
        keys::render(ctx, events, |ui| super::super::run(ui, studio))
    }

    fn press(ctx: &egui::Context, studio: &mut Studio, name: &str) -> egui::FullOutput {
        keys::tab_to(ctx, name, |events| frame(ctx, studio, events));
        frame(ctx, studio, keys::key(Key::Enter, Modifiers::NONE));
        frame(ctx, studio, vec![])
    }

    fn open(ctx: &egui::Context, studio: &mut Studio) -> egui::FullOutput {
        press(ctx, studio, "View");
        press(ctx, studio, "Guides");
        press(ctx, studio, "Guide positions…");
        frame(ctx, studio, vec![])
    }

    fn edit(ctx: &egui::Context, studio: &mut Studio, name: &str, text: &str) -> egui::FullOutput {
        keys::tab_to(ctx, name, |events| frame(ctx, studio, events));
        keys::replace(text, |events| frame(ctx, studio, events));
        frame(ctx, studio, vec![])
    }

    #[test]
    fn keyboard_guides_empty_document_native_add_target_move_remove_origin_undo_and_reopen() {
        let (ctx, mut studio) = fixture();
        frame(&ctx, &mut studio, vec![]);
        frame(&ctx, &mut studio, vec![]);
        assert!(studio.doc.guides.is_empty());
        assert!(studio.selection.is_empty());
        press(&ctx, &mut studio, "View");
        press(&ctx, &mut studio, "Guides");
        press(&ctx, &mut studio, "Unlock all guides");
        open(&ctx, &mut studio);
        let output = edit(&ctx, &mut studio, "Guide position", "18.125");
        keys::focused(&ctx, &output, "Guide position");
        press(&ctx, &mut studio, "Add vertical guide");
        assert_eq!(
            studio.doc.guides,
            vec![Guide {
                vertical: true,
                pos: 18.125
            }]
        );
        open(&ctx, &mut studio);
        edit(&ctx, &mut studio, "Guide position", "8.5");
        press(&ctx, &mut studio, "Add horizontal guide");
        assert_eq!(
            studio.doc.guides[1],
            Guide {
                vertical: false,
                pos: 8.5
            }
        );
        open(&ctx, &mut studio);
        press(&ctx, &mut studio, "Vertical guide 1");
        edit(&ctx, &mut studio, "Guide position", "42.75");
        press(&ctx, &mut studio, "Move selected guide");
        assert_eq!(studio.doc.guides[0].pos, 42.75);
        assert_eq!(studio.doc.guides[1].pos, 8.5);
        open(&ctx, &mut studio);
        press(&ctx, &mut studio, "Horizontal guide 2");
        press(&ctx, &mut studio, "Remove selected guide");
        assert_eq!(studio.doc.guides.len(), 1);
        open(&ctx, &mut studio);
        edit(&ctx, &mut studio, "Ruler zero X", "5.125");
        edit(&ctx, &mut studio, "Ruler zero Y", "-2.5");
        press(&ctx, &mut studio, "Set ruler zero");
        assert_eq!(studio.doc.ruler.origin, Pt::new(5.125, -2.5));
        let expected = crate::project::encode(&studio.doc).unwrap();
        frame(&ctx, &mut studio, keys::key(Key::F8, Modifiers::NONE));
        frame(
            &ctx,
            &mut studio,
            keys::key(
                Key::Z,
                Modifiers {
                    ctrl: true,
                    command: true,
                    ..Modifiers::NONE
                },
            ),
        );
        assert_eq!(studio.doc.ruler.origin, Pt::ZERO);
        frame(
            &ctx,
            &mut studio,
            keys::key(
                Key::Z,
                Modifiers {
                    ctrl: true,
                    command: true,
                    shift: true,
                    ..Modifiers::NONE
                },
            ),
        );
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), expected);
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".artifacts")
            .join(format!("keyboard-guides-{}", crate::document::next_id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("guides.oma");
        crate::project::save_to(&studio.doc, &path).unwrap();
        let reopened = crate::project::load_from(&path).unwrap();
        assert_eq!(crate::project::encode(&reopened).unwrap(), expected);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn keyboard_guides_invalid_cancel_and_stale_list_never_rebind_indices_or_native_drafts() {
        let (ctx, mut studio) = fixture();
        studio.doc.ruler.guides_locked = false;
        open(&ctx, &mut studio);
        let output = edit(&ctx, &mut studio, "Guide position", "-");
        let old = keys::named(&output, "Guide position").0;
        keys::associated_error(&output, "Guide position");
        assert!(keys::named(&output, "Add vertical guide").1.is_disabled());
        press(&ctx, &mut studio, "Close guide positions");
        let output = open(&ctx, &mut studio);
        assert_ne!(keys::named(&output, "Guide position").0, old);
        assert_ne!(
            keys::named(&output, "Guide position").1.invalid(),
            Some(egui::accesskit::Invalid::True)
        );
        edit(&ctx, &mut studio, "Guide position", "1.234567");
        press(&ctx, &mut studio, "Add vertical guide");
        open(&ctx, &mut studio);
        press(&ctx, &mut studio, "Vertical guide 1");
        let output = edit(&ctx, &mut studio, "Guide position", "7");
        let stale = keys::named(&output, "Guide position").0;
        let stale_move = keys::named(&output, "Move selected guide").0;
        studio.add_guide(false, 20.);
        let before = crate::project::encode(&studio.doc).unwrap();
        let history = studio.history.len();
        let output = frame(&ctx, &mut studio, vec![]);
        assert!(keys::named(&output, "Move selected guide").1.is_disabled());
        frame(
            &ctx,
            &mut studio,
            keys::action(
                stale,
                egui::accesskit::Action::SetValue,
                Some(egui::accesskit::ActionData::NumericValue(900.)),
            ),
        );
        frame(
            &ctx,
            &mut studio,
            keys::action(stale_move, egui::accesskit::Action::Click, None),
        );
        frame(&ctx, &mut studio, keys::key(Key::ArrowUp, Modifiers::NONE));
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
        assert_eq!(studio.history.len(), history);
        press(&ctx, &mut studio, "Close guide positions");
        open(&ctx, &mut studio);
        press(&ctx, &mut studio, "Vertical guide 1");
        let output = keys::tab_to(&ctx, "Guide position", |events| {
            frame(&ctx, &mut studio, events)
        });
        assert_ne!(keys::named(&output, "Guide position").0, stale);
        assert_eq!(
            keys::named(&output, "Guide position").1.numeric_value(),
            Some(1.234567_f32 as f64)
        );
        frame(&ctx, &mut studio, keys::key(Key::Tab, Modifiers::NONE));
        let output = frame(&ctx, &mut studio, vec![]);
        assert_eq!(
            keys::named(&output, "Guide position").1.numeric_value(),
            Some(1.234567_f32 as f64)
        );
        studio.doc.ruler.guides_locked = true;
        let output = frame(&ctx, &mut studio, vec![]);
        assert!(keys::named(&output, "Add horizontal guide").1.is_disabled());
        assert!(
            keys::named(&output, "Remove selected guide")
                .1
                .is_disabled()
        );
        studio.doc.ruler.guides_locked = false;
        studio.doc.ruler.guides_visible = false;
        let output = frame(&ctx, &mut studio, vec![]);
        assert!(keys::named(&output, "Move selected guide").1.is_disabled());
        assert_eq!(studio.doc.guides[0].pos, 1.234567);
        assert_eq!(studio.doc.guides[1].pos, 20.);
        assert_eq!(studio.history.len(), history);
        press(&ctx, &mut studio, "Close guide positions");
    }
}
