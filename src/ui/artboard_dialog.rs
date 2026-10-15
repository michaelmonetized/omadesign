use crate::{app::Studio, document::Artboard, geom::Pt};
use eframe::egui::{self, Id};
use std::cell::RefCell;
const ID: &str = "new-artboard-size";
#[derive(Clone)]
struct Dialog {
    owner: String,
    generation: u64,
    selection: Vec<(usize, u64)>,
    artboards: Vec<u64>,
    origin: Pt,
    width: f32,
    height: f32,
}
pub(super) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<Dialog>(Id::new(ID)).is_some())
}
pub(super) fn open(ctx: &egui::Context, studio: &Studio, origin: Pt) {
    let owner = Id::new(("artboard-size-modal", &studio.swap_id, origin.x.to_bits(), origin.y.to_bits()));
    ctx.data_mut(|d| {
        for property in ["width", "height"] {
            d.remove::<String>(owner.with(property).with("number-validation"));
        }
        d.insert_temp(
            Id::new(ID),
            Dialog {
                owner: studio.swap_id.clone(),
                generation: studio.canvas_gen,
                selection: studio.selection.clone(),
                artboards: studio.artboard_sel.clone(),
                origin,
                width: studio.custom_w,
                height: studio.custom_h,
            },
        )
    });
}
pub(super) fn show(ctx: &egui::Context, studio: &mut Studio) {
    let Some(mut dialog) = ctx.data(|d| d.get_temp::<Dialog>(Id::new(ID))) else {
        return;
    };
    if dialog.owner != studio.swap_id || dialog.generation != studio.canvas_gen
        || dialog.selection != studio.selection || dialog.artboards != studio.artboard_sel
    {
        ctx.data_mut(|d| d.remove::<Dialog>(Id::new(ID)));
        return;
    }
    let mut create = false;
    let mut cancel = false;
    let available = !studio.show_welcome && !studio.file_dialog_pending() && !studio.updates.freezing
        && studio.op.is_none() && studio.type_edit.is_none() && studio.free_transform.is_none()
        && studio.deformation.is_none() && studio.pending_place.is_none();
    let owner = Id::new(("artboard-size-modal", &dialog.owner, dialog.origin.x.to_bits(), dialog.origin.y.to_bits()));
    let response = egui::Modal::new(owner).show(ctx, |ui| {
        ui.heading("New artboard");
        let mut fields = Vec::new();
        let mut invalid = Vec::new();
        ui.add_enabled_ui(available, |ui| ui.horizontal(|ui| {
            let before = dialog.width;
            let property = owner.with("width");
            let attempt = RefCell::new(None);
            let (response, label) = ui.scope_builder(egui::UiBuilder::new().id(property), |ui| {
                let label = ui.label("Width (px)").id;
                (ui.add(
                egui::DragValue::new(&mut dialog.width)
                    .range(1.0..=65535.)
                    .custom_parser(|text| super::welcome::parse_number(&attempt, text))
                    .prefix("Width  ")
                    .suffix(" px"),
                ), label)
            }).inner;
            super::welcome::name_control(ui, &response, label, "Artboard width (px)");
            if !super::welcome::native_number_valid(ui, &response, property, attempt.into_inner(), f64::from(dialog.width), 1.0..=65535.) {
                dialog.width = before;
                if response.enabled() { invalid.push(response.id); }
            }
            fields.push(response);
            let before = dialog.height;
            let property = owner.with("height");
            let attempt = RefCell::new(None);
            let (response, label) = ui.scope_builder(egui::UiBuilder::new().id(property), |ui| {
                let label = ui.label("Height (px)").id;
                (ui.add(
                egui::DragValue::new(&mut dialog.height)
                    .range(1.0..=65535.)
                    .custom_parser(|text| super::welcome::parse_number(&attempt, text))
                    .prefix("Height  ")
                    .suffix(" px"),
                ), label)
            }).inner;
            super::welcome::name_control(ui, &response, label, "Artboard height (px)");
            if !super::welcome::native_number_valid(ui, &response, property, attempt.into_inner(), f64::from(dialog.height), 1.0..=65535.) {
                dialog.height = before;
                if response.enabled() { invalid.push(response.id); }
            }
            fields.push(response);
        }));
        let valid = invalid.is_empty() && dialog.width.is_finite() && dialog.height.is_finite()
            && (1.0..=65535.).contains(&dialog.width) && (1.0..=65535.).contains(&dialog.height)
            && dialog.origin.x.is_finite() && dialog.origin.y.is_finite();
        ui.horizontal(|ui| {
            let response = ui.scope_builder(egui::UiBuilder::new().id(owner.with("create")), |ui| {
                ui.add_enabled(available && valid, egui::Button::new("Create artboard"))
            }).inner;
            super::welcome::focus_control(ui, &response);
            create = available && valid && response.clicked();
            let response = ui.scope_builder(egui::UiBuilder::new().id(owner.with("cancel")), |ui| ui.button("Cancel")).inner;
            super::welcome::focus_control(ui, &response);
            cancel = response.clicked();
        });
        if !valid {
            let affected: Vec<_> = fields.iter().filter(|field| invalid.is_empty() || invalid.contains(&field.id)).collect();
            super::welcome::control_error(ui, &affected, "Enter finite artboard dimensions from 1 to 65535 px and a finite origin.");
        }
    });
    if create {
        let mut board = Artboard::new(
            studio.doc.artboards.len(),
            dialog.origin,
            Pt::new(dialog.width, dialog.height),
        );
        board.name = studio.doc.unique_artboard_name(&board.name);
        let id = board.id;
        let mut after = studio.doc.artboards.clone();
        after.push(board);
        studio.commit_artboards(after);
        studio.artboard_sel = vec![id];
    }
    let close = create || cancel || response.should_close();
    ctx.data_mut(|d| {
        if close {
            d.remove::<Dialog>(Id::new(ID));
        } else {
            d.insert_temp(Id::new(ID), dialog);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::welcome::keyboard_tests as keys;
    use egui::{Event, Key, Modifiers, accesskit};

    fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
        keys::render(ctx, events, |ui| {
            studio.handle_shortcuts(ui.ctx());
            show(ui.ctx(), studio);
        })
    }

    #[test]
    fn artboard_modal_can_render_and_close_without_nested_context_locks() {
        let ctx = egui::Context::default();
        let mut studio = Studio::new();
        open(&ctx, &studio, Pt::ZERO);
        let mut output = ctx.run_ui(Default::default(), |ui| show(ui.ctx(), &mut studio));
        output.textures_delta.clear();
        assert!(is_open(&ctx));
        let input = egui::RawInput { events: keys::key(Key::Escape, Modifiers::NONE), ..Default::default() };
        let mut output = ctx.run_ui(input, |ui| show(ui.ctx(), &mut studio));
        output.textures_delta.clear();
        assert!(!is_open(&ctx));
    }

    #[test]
    fn artboard_dialog_keyboard_edits_dimensions_and_creates_native_selection() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut studio = Studio::new();
        studio.doc = crate::document::Document::new("Keyboard artboard", 80., 120., 72.);
        studio.show_welcome = false;
        let origin = Pt::new(40., 60.);
        let before = studio.doc.artboards.clone();
        assert!(studio.selection.is_empty());
        frame(&ctx, &mut studio, keys::key(Key::O, Modifiers::SHIFT));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        assert!(is_open(&ctx));
        for (name, text) in [("Artboard width (px)", "320"), ("Artboard height (px)", "240")] {
            let output = keys::tab_to(&ctx, name, |events| frame(&ctx, &mut studio, events));
            keys::focused(&ctx, &output, name);
            assert!(!keys::named(&output, name).1.labelled_by().is_empty());
            keys::replace(text, |events| frame(&ctx, &mut studio, events));
        }
        let output = keys::tab(true, |events| frame(&ctx, &mut studio, events));
        keys::focused(&ctx, &output, "Artboard width (px)");
        keys::tab_to(&ctx, "Create artboard", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        assert!(!is_open(&ctx));
        assert_eq!(&studio.doc.artboards[..before.len()], before.as_slice());
        let board = studio.doc.artboards.last().unwrap();
        assert_eq!((board.origin, board.size), (origin, Pt::new(320., 240.)));
        assert_ne!(board.id, 0);
        assert_eq!(studio.artboard_sel, vec![board.id]);
    }

    #[test]
    fn artboard_dialog_keyboard_invalid_and_busy_actions_preserve_document() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut studio = Studio::new();
        studio.show_welcome = false;
        let before = studio.doc.artboards.clone();
        open(&ctx, &studio, Pt::ZERO);
        keys::tab_to(&ctx, "Artboard width (px)", |events| frame(&ctx, &mut studio, events));
        let output = keys::replace("NaN", |events| frame(&ctx, &mut studio, events));
        keys::associated_error(&output, "Artboard width (px)");
        let output = keys::tab(false, |events| frame(&ctx, &mut studio, events));
        keys::associated_error(&output, "Artboard width (px)");
        keys::focused(&ctx, &output, "Artboard height (px)");
        keys::tab(true, |events| frame(&ctx, &mut studio, events));
        let create = keys::named(&output, "Create artboard");
        assert!(create.1.is_disabled());
        frame(&ctx, &mut studio, keys::action(create.0, accesskit::Action::Click, None));
        assert_eq!(studio.doc.artboards, before);
        keys::replace("128", |events| frame(&ctx, &mut studio, events));
        let output = frame(&ctx, &mut studio, vec![]);
        assert_eq!(keys::named(&output, "Artboard width (px)").1.invalid(), None);
        studio.updates.freezing = true;
        let output = frame(&ctx, &mut studio, vec![]);
        let width = keys::named(&output, "Artboard width (px)").0;
        let prior = ctx.data(|data| data.get_temp::<Dialog>(Id::new(ID))).unwrap().width;
        frame(&ctx, &mut studio, keys::action(width, accesskit::Action::SetValue, Some(accesskit::ActionData::NumericValue(512.))));
        assert_eq!(ctx.data(|data| data.get_temp::<Dialog>(Id::new(ID))).unwrap().width, prior);
        frame(&ctx, &mut studio, keys::action(create.0, accesskit::Action::Click, None));
        assert_eq!(studio.doc.artboards, before);
        assert!(is_open(&ctx));
        studio.updates.freezing = false;
        keys::tab_to(&ctx, "Cancel", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        assert!(!is_open(&ctx));
        assert_eq!(studio.doc.artboards, before);
    }

    #[test]
    fn artboard_dialog_rejects_stale_owners_and_invalid_origin() {
        for change in 0..4 {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let mut studio = Studio::new();
            studio.show_welcome = false;
            let before = studio.doc.artboards.clone();
            open(&ctx, &studio, Pt::ZERO);
            let output = frame(&ctx, &mut studio, vec![]);
            let create = keys::named(&output, "Create artboard").0;
            match change {
                0 => studio.swap_id.push_str("-new-document"),
                1 => studio.canvas_gen += 1,
                2 => studio.selection.push((0, 999)),
                _ => studio.artboard_sel.push(999),
            }
            frame(&ctx, &mut studio, keys::action(create, accesskit::Action::Click, None));
            assert!(!is_open(&ctx));
            assert_eq!(studio.doc.artboards, before);
        }
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut studio = Studio::new();
        studio.show_welcome = false;
        let before = studio.doc.artboards.clone();
        open(&ctx, &studio, Pt::new(f32::NAN, 0.));
        frame(&ctx, &mut studio, vec![]);
        let output = frame(&ctx, &mut studio, vec![]);
        keys::associated_error(&output, "Artboard width (px)");
        let create = keys::named(&output, "Create artboard");
        assert!(create.1.is_disabled());
        frame(&ctx, &mut studio, keys::action(create.0, accesskit::Action::Click, None));
        assert_eq!(studio.doc.artboards, before);
    }
}
