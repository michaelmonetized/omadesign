//! Native Quick Mask controls edit the same stored mask and display options.
use super::{
    fields,
    icons::{self, ph},
    keyboard_controls, theme, welcome,
};
use crate::{
    app::Studio,
    channels::quick_mask::{Indicates, Options},
    tools::Persona,
};
use eframe::egui::{self, Id, Ui};

#[derive(Clone)]
struct Draft {
    document: String,
    generation: u64,
    mask: Option<u64>,
    scope: Id,
    options: Options,
    error: Option<String>,
}
fn id() -> Id {
    Id::new("quick-mask-options-dialog")
}
pub(super) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<Draft>(id()).is_some())
}
/// Open actual mask display settings without committing a focus or preview change.
/// Args: context and current document. Returns nothing; stale sources retire the draft.
pub(super) fn options(ctx: &egui::Context, studio: &Studio) {
    if is_open(ctx) || studio.capture_channels().is_err() {
        return;
    }
    let scope = keyboard_controls::draft_id(ctx, "quick-mask-options");
    let mask = studio.doc.quick_mask.as_ref();
    let draft = Draft {
        document: studio.swap_id.clone(),
        generation: studio.canvas_gen,
        mask: mask.map(|m| m.channel.id),
        scope,
        options: mask.map_or(studio.quick_mask_defaults, |m| m.options),
        error: None,
    };
    ctx.data_mut(|d| d.insert_temp(id(), draft));
}
/// Toggle the real document mask from an icon in the existing tool well.
/// Args: pane and editor. Returns nothing; busy/refused modes report their actual error.
pub(super) fn button(ui: &mut Ui, studio: &mut Studio) {
    let on = studio.doc.quick_mask.is_some();
    let key = Id::new(("quick-mask-button", &studio.swap_id));
    let response = fields::native_scope(ui, key, |ui| {
        ui.add_enabled(
            studio.capture_channels().is_ok() && studio.doc.collaboration.is_none(),
            egui::Button::new(egui::RichText::new(ph::SELECTION).font(icons::font(19.)))
                .selected(on)
                .min_size(egui::vec2(32., 30.)),
        )
    })
    .inner;
    welcome::name_control(ui, &response, key.with("name"), "Quick Mask mode");
    if response.clone().on_hover_text("Quick Mask · Q").clicked() {
        if let Err(error) = studio.set_quick_mask(!on) {
            studio.status = error;
        }
    }
    response.context_menu(|ui| {
        if ui
            .add_enabled(on, egui::Button::new("Quick Mask options…"))
            .clicked()
        {
            options(ui.ctx(), studio);
            ui.close();
        }
    });
}
/// Add mask mode and options to the existing Select menu.
/// Args: menu UI and editor. Returns nothing; the actual typed commands own mutation.
pub(super) fn menu(ui: &mut Ui, studio: &mut Studio) {
    if studio.persona == Persona::Photo {
        return;
    }
    let on = studio.doc.quick_mask.is_some();
    let enabled = studio.capture_channels().is_ok() && studio.doc.collaboration.is_none();
    if ui
        .add_enabled(
            enabled,
            egui::Button::new("Quick Mask")
                .selected(on)
                .shortcut_text("Q"),
        )
        .clicked()
    {
        if let Err(error) = studio.set_quick_mask(!on) {
            studio.status = error;
        }
        ui.close();
    }
    if ui
        .add_enabled(enabled && on, egui::Button::new("Quick Mask options…"))
        .clicked()
    {
        options(ui.ctx(), studio);
        ui.close();
    }
    ui.separator();
}
/// Draw native color, opacity and indication controls for one captured mask.
/// Args: context and editor. Returns nothing; only Apply creates a command.
pub(super) fn show(ctx: &egui::Context, studio: &mut Studio) {
    let Some(mut draft) = ctx.data_mut(|d| {
        let draft = d.get_temp::<Draft>(id());
        d.remove::<Draft>(id());
        draft
    }) else {
        return;
    };
    if draft.document != studio.swap_id
        || draft.generation != studio.canvas_gen
        || studio.doc.quick_mask.as_ref().map(|mask| mask.channel.id) != draft.mask
        || studio.capture_channels().is_err()
    {
        return;
    }
    let mut apply = false;
    let mut cancel = false;
    let modal = egui::Modal::new(draft.scope.with("modal"))
        .backdrop_color(theme::bg_window().gamma_multiply(0.65))
        .show(ctx, |ui| {
            ui.set_width(300.0_f32.min((ctx.viewport_rect().width() - 40.).max(180.)));
            ui.strong("Quick Mask options");
            ui.horizontal(|ui| {
                ui.label("Color");
                super::color_picker::color_edit_named(
                    ui,
                    draft.scope.with("color"),
                    &mut draft.options.color,
                    "Quick Mask color",
                );
            });
            let mut opacity = draft.options.opacity * 100.;
            let (field, valid) = keyboard_controls::motion_number(
                ui,
                draft.scope.with("opacity"),
                ("Opacity", "Quick Mask opacity"),
                &mut opacity,
                0.0..=100.,
                1.,
                "%",
                Some(6),
            );
            if field.changed {
                draft.options.opacity = opacity / 100.;
            }
            ui.horizontal(|ui| {
                let label = ui.label("Color indicates");
                let response = fields::native_scope(ui, draft.scope.with("indicates"), |ui| {
                    egui::ComboBox::from_id_salt("indicates")
                        .selected_text(if draft.options.indicates == Indicates::Masked {
                            "Masked areas"
                        } else {
                            "Selected areas"
                        })
                        .show_ui(ui, |ui| {
                            for (value, name) in [
                                (Indicates::Masked, "Masked areas"),
                                (Indicates::Selected, "Selected areas"),
                            ] {
                                if ui
                                    .selectable_value(&mut draft.options.indicates, value, name)
                                    .clicked()
                                {
                                    ui.close();
                                }
                            }
                        })
                })
                .inner
                .response
                .labelled_by(label.id);
                welcome::name_control(
                    ui,
                    &response,
                    draft.scope.with("indicates-name"),
                    "Quick Mask color indicates",
                );
            });
            if let Some(error) = &draft.error {
                welcome::control_error(ui, &[&field.response], error);
            }
            ui.horizontal(|ui| {
                apply = ui
                    .add_enabled(
                        valid && draft.options.validate().is_ok(),
                        egui::Button::new("Apply"),
                    )
                    .clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
    if cancel || modal.should_close() {
        return;
    }
    if apply {
        match studio.set_quick_mask_options(draft.options) {
            Ok(_) => return,
            Err(error) => draft.error = Some(error),
        }
    }
    ctx.data_mut(|d| d.insert_temp(id(), draft));
}

#[cfg(test)]
mod tests {
    use super::super::welcome::keyboard_tests as keys;
    use super::*;
    use egui::{Event, Key, Modifiers};
    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        theme::apply(&ctx);
        ctx
    }
    fn frame(ctx: &egui::Context, s: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
        keys::render(ctx, events, |ui| {
            ui.set_max_width(320.);
            if !is_open(ctx) {
                s.handle_shortcuts(ctx);
            }
            button(ui, s);
            super::super::channels::show(ui, s);
            show(ctx, s);
        })
    }
    fn invoke(ctx: &egui::Context, s: &mut Studio, name: &str) {
        keys::tab_to(ctx, name, |events| frame(ctx, s, events));
        frame(ctx, s, keys::key(Key::Space, Modifiers::NONE));
        frame(ctx, s, vec![]);
    }
    #[test]
    fn craft_quick_mask_native_mode_options_cancel_apply_undo_and_selection_return() {
        let ctx = context();
        let mut s = crate::app::quick_mask::tests::fixture();
        let original = crate::project::encode(&s.doc).unwrap();
        let history = s.history.len();
        frame(&ctx, &mut s, vec![]);
        frame(&ctx, &mut s, vec![]);
        invoke(&ctx, &mut s, "Quick Mask mode");
        assert!(s.quick_mask_painting());
        assert_eq!(s.history.len(), history + 1);
        let entered = crate::project::encode(&s.doc).unwrap();
        invoke(&ctx, &mut s, "Quick Mask options");
        assert!(is_open(&ctx));
        keys::tab_to(&ctx, "Quick Mask opacity", |events| {
            frame(&ctx, &mut s, events)
        });
        keys::replace("35.125", |events| frame(&ctx, &mut s, events));
        frame(&ctx, &mut s, keys::key(Key::Enter, Modifiers::NONE));
        invoke(&ctx, &mut s, "Cancel");
        assert_eq!(crate::project::encode(&s.doc).unwrap(), entered);
        invoke(&ctx, &mut s, "Quick Mask options");
        keys::tab_to(&ctx, "Quick Mask opacity", |events| {
            frame(&ctx, &mut s, events)
        });
        keys::replace("35.125", |events| frame(&ctx, &mut s, events));
        frame(&ctx, &mut s, keys::key(Key::Enter, Modifiers::NONE));
        invoke(&ctx, &mut s, "Quick Mask color indicates");
        invoke(&ctx, &mut s, "Selected areas");
        invoke(&ctx, &mut s, "Apply");
        assert!(!is_open(&ctx));
        assert_eq!(s.history.len(), history + 2);
        let q = s.doc.quick_mask.as_ref().unwrap();
        assert_eq!(q.options.opacity, 35.125 / 100.);
        assert_eq!(q.options.indicates, Indicates::Selected);
        assert!(q.channel.plane.samples.iter().all(|v| *v == 0.));
        let accepted = crate::project::encode(&s.doc).unwrap();
        s.undo();
        assert_eq!(crate::project::encode(&s.doc).unwrap(), entered);
        s.redo();
        assert_eq!(crate::project::encode(&s.doc).unwrap(), accepted);
        invoke(&ctx, &mut s, "Quick Mask mode");
        assert!(s.doc.quick_mask.is_none());
        assert!(s.pixel_sel.is_none());
        assert_eq!(crate::project::encode(&s.doc).unwrap(), original);
    }
    #[test]
    fn craft_quick_mask_native_invalid_options_stale_owner_and_unopened_defaults_leave_artwork() {
        let ctx = context();
        let mut s = crate::app::quick_mask::tests::fixture();
        let original = crate::project::encode(&s.doc).unwrap();
        let history = s.history.len();
        options(&ctx, &s);
        frame(&ctx, &mut s, vec![]);
        frame(&ctx, &mut s, vec![]);
        keys::tab_to(&ctx, "Quick Mask opacity", |events| {
            frame(&ctx, &mut s, events)
        });
        let output = keys::replace("not a number", |events| frame(&ctx, &mut s, events));
        keys::associated_error(&output, "Quick Mask opacity");
        assert!(keys::named(&output, "Apply").1.is_disabled());
        invoke(&ctx, &mut s, "Cancel");
        assert_eq!(s.history.len(), history);
        assert_eq!(crate::project::encode(&s.doc).unwrap(), original);
        options(&ctx, &s);
        s.canvas_gen += 1;
        frame(&ctx, &mut s, vec![]);
        assert!(!is_open(&ctx));
        assert_eq!(s.history.len(), history);
        assert_eq!(crate::project::encode(&s.doc).unwrap(), original);
    }
}
