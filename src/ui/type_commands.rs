//! Type content menus retain native widgets and captured document/story owners.
use crate::app::{
    Studio,
    type_commands::{
        self as commands, SmartPunctuation, TextCase, TextScope, TypeCommand, TypeCommandTarget,
    },
};
use eframe::egui::{self, Id, Response, Ui};

const ID: &str = "type-smart-punctuation";
#[derive(Clone)]
struct Draft {
    instance: Id,
    owner: String,
    selection: Option<TypeCommandTarget>,
    document: Option<TypeCommandTarget>,
    selection_error: Option<String>,
    document_error: Option<String>,
    scope: TextScope,
    options: SmartPunctuation,
    focus: bool,
}

/// Decorate the real native response with its document-owned semantic identity.
/// Args: UI, ID, name and actual widget. Returns its original response.
fn control(ui: &mut Ui, id: Id, name: &str, widget: impl FnOnce(&mut Ui) -> Response) -> Response {
    let response = super::fields::native_scope(ui, id, widget).inner;
    ui.ctx()
        .accesskit_node_builder(response.id, |node| node.set_label(name));
    super::welcome::focus_control(ui, &response);
    response
}

/// Read whether the captured punctuation dialog is open.
/// Args: context. Returns its real modal lifetime, including a stale destination.
pub(super) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<Draft>(Id::new(ID)).is_some())
}

/// Return a closed punctuation dialog to its actual enabled canvas response.
/// Args: current canvas UI, editor and native response. Returns no value; other modal/document owners retire the request.
pub(super) fn restore_canvas_focus(ui: &Ui, studio: &Studio, canvas: &Response) {
    for dialog in [ID, super::find_replace::ID, super::find_font::ID] { restore_text_dialog_focus(ui, studio, canvas, dialog); }
}
fn restore_text_dialog_focus(ui: &Ui, studio: &Studio, canvas: &Response, dialog: &str) {
    let key = Id::new((dialog, "return-focus"));
    let Some(owner) = ui.ctx().data(|d| d.get_temp::<String>(key)) else {
        return;
    };
    let retire = || ui.ctx().data_mut(|d| d.remove::<String>(key));
    if owner != studio.swap_id
        || studio.show_welcome
        || studio.file_dialog_pending()
        || studio.updates.freezing
    {
        retire();
        return;
    }
    if let Some(modal) = ui.ctx().memory(|m| m.top_modal_layer()) {
        if modal == egui::Modal::default_area(Id::new(dialog)).layer() {
            ui.ctx()
                .request_discard("Finish closing text dialog before restoring canvas focus");
            ui.ctx().request_repaint();
        } else {
            retire();
        }
        return;
    }
    if canvas.enabled() && ui.ctx().memory(|m| m.allows_interaction(canvas.layer_id)) {
        canvas.request_focus();
    }
    retire();
}

/// Open the punctuation dialog without committing a live text draft.
/// Args: actual UI context and editor. Returns no value; Cancel leaves content/history unchanged.
fn open(ctx: &egui::Context, studio: &Studio) {
    let selection = studio.capture_type_command(TextScope::Selection);
    let document = studio.capture_type_command(TextScope::Document);
    let scope = if selection.is_ok() {
        TextScope::Selection
    } else {
        TextScope::Document
    };
    let options = ctx
        .data(|d| d.get_temp::<SmartPunctuation>(Id::new((ID, "options"))))
        .unwrap_or_default();
    let draft = Draft {
        instance: super::keyboard_controls::draft_id(ctx, ID),
        owner: studio.swap_id.clone(),
        selection_error: selection.as_ref().err().cloned(),
        document_error: document.as_ref().err().cloned(),
        selection: selection.ok(),
        document: document.ok(),
        scope,
        options,
        focus: true,
    };
    ctx.data_mut(|d| d.insert_temp(Id::new(ID), draft));
}

/// Add the real reference content commands to the existing Type menu.
/// Args: menu UI and editor. Returns no value; accepted commands share the app-owned route.
pub(super) fn menu(ui: &mut Ui, studio: &mut Studio) {
    let owner = Id::new(("type-content-menu", &studio.swap_id, &studio.selection));
    let selected = studio.capture_type_command(TextScope::Selection).is_ok();
    ui.add_enabled_ui(selected, |ui| {
        ui.menu_button("Change case", |ui| {
            for (case, name) in [
                (TextCase::Upper, "UPPERCASE"),
                (TextCase::Lower, "lowercase"),
                (TextCase::Title, "Title Case"),
                (TextCase::Sentence, "Sentence case"),
            ] {
                if control(ui, owner.with(name), name, |ui| ui.button(name)).clicked() {
                    studio.type_selection_command(TypeCommand::Case(case));
                    ui.close();
                }
            }
        });
        for (name, entries) in [
            ("Insert special character", commands::SPECIAL_CHARACTERS),
            (
                "Insert whitespace character",
                commands::WHITESPACE_CHARACTERS,
            ),
            ("Insert break character", commands::BREAK_CHARACTERS),
        ] {
            ui.menu_button(name, |ui| {
                for &(label, text) in entries {
                    if control(ui, owner.with(label), label, |ui| ui.button(label)).clicked() {
                        studio.type_selection_command(TypeCommand::Insert(text.into()));
                        ui.close();
                    }
                }
            });
        }
    });
    let available = selected || studio.capture_type_command(TextScope::Document).is_ok();
    if control(ui, owner.with("punctuation"), "Smart punctuation…", |ui| {
        ui.add_enabled(available, egui::Button::new("Smart punctuation…"))
    })
    .clicked()
    {
        open(ui.ctx(), studio);
        ui.close();
    }
}

/// Draw configurable punctuation against its captured selection or whole-document scope.
/// Args: context/editor. Returns no value; Apply checks the owner again before the typed command.
pub(super) fn show(ctx: &egui::Context, studio: &mut Studio) {
    let Some(mut draft) = ctx.data(|d| d.get_temp::<Draft>(Id::new(ID))) else {
        return;
    };
    let owner = Id::new((ID, &draft.owner, draft.instance));
    let mut apply = false;
    let mut cancel = false;
    let modal = egui::Modal::new(Id::new(ID)).backdrop_color(super::theme::bg_window().gamma_multiply(0.55)).show(ctx, |ui| {
        ui.set_width((ctx.content_rect().width() - 48.).clamp(160., 350.));
        ui.heading("Smart punctuation");
        ui.horizontal_wrapped(|ui| {
            for (scope, name) in [
                (TextScope::Selection, "Selected text"),
                (TextScope::Document, "Entire document"),
            ] {
                let enabled = if scope == TextScope::Selection {
                    draft.selection.is_some()
                } else {
                    draft.document.is_some()
                };
                if control(ui, owner.with(name), name, |ui| {
                    ui.add_enabled(enabled, egui::RadioButton::new(draft.scope == scope, name))
                })
                .clicked()
                {
                    draft.scope = scope;
                }
            }
        });
        let (target, error) = if draft.scope == TextScope::Selection {
            (&draft.selection, &draft.selection_error)
        } else {
            (&draft.document, &draft.document_error)
        };
        let current = target
            .as_ref()
            .is_some_and(|t| studio.type_command_current(t));
        if !current {
            ui.colored_label(
                ui.visuals().error_fg_color,
                error.as_deref().unwrap_or(
                    "The text destination changed. Cancel and reopen Smart punctuation.",
                ),
            );
        }
        ui.add_enabled_ui(current, |ui| {
            let quotes = control(ui, owner.with("quotes"), "Typographic quotes", |ui| {
                ui.checkbox(&mut draft.options.quotes, "Typographic quotes")
            });
            if draft.focus {
                quotes.request_focus();
                draft.focus = false;
            }
            ui.add_enabled_ui(draft.options.quotes, |ui| {
                for (name, pairs, value) in [
                    (
                        "Double quotation marks",
                        [
                            ['“', '”'],
                            ['„', '“'],
                            ['„', '”'],
                            ['”', '”'],
                            ['«', '»'],
                            ['»', '«'],
                            ['「', '」'],
                            ['"', '"'],
                        ],
                        &mut draft.options.double,
                    ),
                    (
                        "Single quotation marks",
                        [
                            ['‘', '’'],
                            ['‚', '‘'],
                            ['‚', '’'],
                            ['’', '’'],
                            ['‹', '›'],
                            ['›', '‹'],
                            ['『', '』'],
                            ['\'', '\''],
                        ],
                        &mut draft.options.single,
                    ),
                ] {
                    let label = ui.label(name);
                    control(ui, owner.with(name), name, |ui| {
                        egui::ComboBox::from_id_salt(owner.with((name, "choices")))
                            .selected_text(format!("{} … {}", value[0], value[1]))
                            .show_ui(ui, |ui| {
                                for pair in pairs {
                                    ui.selectable_value(
                                        value,
                                        pair,
                                        format!("{} … {}", pair[0], pair[1]),
                                    );
                                }
                            })
                            .response
                            .labelled_by(label.id)
                    });
                }
            });
            control(
                ui,
                owner.with("dashes"),
                "Convert double and triple hyphens",
                |ui| ui.checkbox(&mut draft.options.dashes, "Convert -- and --- to dashes"),
            );
            control(
                ui,
                owner.with("ellipsis"),
                "Convert three dots to ellipsis",
                |ui| ui.checkbox(&mut draft.options.ellipsis, "Convert ... to ellipsis"),
            );
        });
        ui.separator();
        ui.horizontal(|ui| {
            apply = control(ui, owner.with("apply"), "Apply smart punctuation", |ui| {
                ui.add_enabled(current, egui::Button::new("Apply"))
            })
            .clicked();
            cancel = control(ui, owner.with("cancel"), "Cancel smart punctuation", |ui| {
                ui.button("Cancel")
            })
            .clicked();
        });
    });
    cancel |= modal.should_close();
    if apply && !cancel {
        let target = if draft.scope == TextScope::Selection {
            draft.selection.as_ref()
        } else {
            draft.document.as_ref()
        };
        if let Some(target) = target {
            match studio.apply_type_command(target, TypeCommand::Punctuation(draft.options)) {
                Ok(_) => {
                    ctx.data_mut(|d| d.insert_temp(Id::new((ID, "options")), draft.options));
                    cancel = true;
                }
                Err(error) => studio.status = error,
            }
        }
    }
    if cancel {
        ctx.data_mut(|d| d.remove::<Draft>(Id::new(ID)));
        ctx.data_mut(|d| d.insert_temp(Id::new((ID, "return-focus")), draft.owner));
        ctx.request_discard("Finish the punctuation modal before the next canvas input");
        ctx.request_repaint();
    } else {
        ctx.data_mut(|d| d.insert_temp(Id::new(ID), draft));
    }
}

#[cfg(test)]
#[path = "type_commands_tests.rs"]
mod tests;
