use super::theme::{fg, fg_weak};
use crate::app::Studio;
use crate::color::Rgba;
use crate::tools::Tool;
use eframe::egui::{Button, RichText, Ui};

#[derive(Clone, Copy)]
pub enum Action {
    Reveal,
    Hide,
    Selection,
    Invert,
    Remove,
    Apply,
    Edit(bool),
}

impl Action {
    pub fn run(self, studio: &mut Studio, index: usize) {
        match self {
            Self::Reveal => studio.add_layer_mask(index, true),
            Self::Hide => studio.add_layer_mask(index, false),
            Self::Selection => studio.mask_from_selection(index),
            Self::Invert => studio.invert_layer_mask(index),
            Self::Remove => studio.remove_layer_mask(index),
            Self::Apply => studio.apply_layer_mask(index),
            Self::Edit(enabled) => studio.set_mask_edit(index, enabled),
        }
    }
}

pub fn menu(ui: &mut Ui, studio: &Studio, index: usize) -> Option<Action> {
    let layer = studio.doc.layers.get(index)?;
    let editable = !layer.locked && layer.visible;
    let masked = layer.mask.is_some();
    let dimensions = layer.kind.pixels().map_or(
        (
            studio.doc.width.ceil().max(1.0) as usize,
            studio.doc.height.ceil().max(1.0) as usize,
        ),
        |pixels| (pixels.w as usize, pixels.h as usize),
    );
    let selected = studio.active_layer == Some(index)
        && studio
            .pixel_sel
            .as_ref()
            .is_some_and(|mask| mask.len() == dimensions.0 * dimensions.1);
    let mut action = None;
    let mut item = |ui: &mut Ui, enabled, label, value| {
        if ui.add_enabled(enabled, Button::new(label)).clicked() {
            action = Some(value);
            ui.close();
        }
    };
    item(
        ui,
        editable,
        if masked {
            "Reset to reveal all"
        } else {
            "Reveal all"
        },
        Action::Reveal,
    );
    item(
        ui,
        editable,
        if masked {
            "Reset to hide all"
        } else {
            "Hide all"
        },
        Action::Hide,
    );
    item(
        ui,
        editable && selected,
        if masked {
            "Replace from selection"
        } else {
            "From selection"
        },
        Action::Selection,
    );
    if masked {
        ui.separator();
        item(ui, editable, "Paint mask", Action::Edit(true));
        item(ui, editable, "Invert mask", Action::Invert);
        item(
            ui,
            editable && layer.kind.pixels().is_some(),
            "Apply to pixels",
            Action::Apply,
        );
        item(ui, editable, "Remove mask", Action::Remove);
    }
    action
}

pub fn inspector(ui: &mut Ui, studio: &mut Studio) {
    let Some(index) = studio
        .active_layer
        .filter(|&index| index < studio.doc.layers.len())
    else {
        ui.label(
            RichText::new("Choose a layer to paint")
                .small()
                .color(fg_weak()),
        );
        return;
    };
    let layer = &studio.doc.layers[index];
    let editable = !layer.locked && layer.visible;
    let pixels = layer.kind.pixels().is_some();
    let masked = layer.mask.is_some();
    let editing = studio.paint_mask && masked;
    let mut action = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Paint on").strong().size(12.0).color(fg()));
        ui.add_enabled_ui(editable, |ui| {
            if ui
                .selectable_label(!editing, if pixels { "Pixels" } else { "Artwork" })
                .clicked()
            {
                action = Some(Action::Edit(false));
            }
            if ui
                .add_enabled(masked, Button::new("Mask").selected(editing))
                .clicked()
            {
                action = Some(Action::Edit(true));
            }
        });
        ui.menu_button("···", |ui| {
            if let Some(chosen) = menu(ui, studio, index) {
                action = Some(chosen);
            }
        })
        .response
        .on_hover_text("Layer mask actions");
    });
    if !masked {
        if ui
            .add_enabled(editable, Button::new("Add layer mask"))
            .clicked()
        {
            action = Some(Action::Reveal);
        }
    } else if editing {
        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal(|ui| {
                let black = studio.brush.color.r < 128;
                if ui
                    .selectable_label(black, "Hide")
                    .on_hover_text("Paint black to hide")
                    .clicked()
                {
                    studio.brush.color = Rgba::BLACK;
                    studio.tool = Tool::Brush;
                }
                if ui
                    .selectable_label(!black, "Reveal")
                    .on_hover_text("Paint white to reveal")
                    .clicked()
                {
                    studio.brush.color = Rgba::WHITE;
                    studio.tool = Tool::Brush;
                }
            });
        });
        ui.label(
            RichText::new("Original pixels stay untouched.")
                .small()
                .color(fg_weak()),
        );
    }
    if !editable {
        ui.label(
            RichText::new(if studio.doc.layers[index].locked {
                "Unlock this layer to paint."
            } else {
                "Show this layer to paint."
            })
            .small()
            .color(fg_weak()),
        );
    }
    if let Some(action) = action {
        action.run(studio, index);
    }
}

pub fn retouch_hint(ui: &mut Ui, studio: &mut Studio) {
    if !matches!(studio.tool, Tool::Clone | Tool::Heal) {
        return;
    }
    if studio.paint_mask {
        ui.label(
            RichText::new("Choose Pixels to retouch the image.")
                .small()
                .color(fg_weak()),
        );
        return;
    }
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(if studio.clone_source.is_some() {
                "Source ready"
            } else {
                "Alt-click to choose a source"
            })
            .small()
            .color(fg_weak()),
        );
        if studio.clone_source.is_some() && ui.small_button("Reset").clicked() {
            studio.clone_source = None;
        }
    });
    if studio.tool == Tool::Heal {
        ui.label(
            RichText::new("Clean texture, blended into local colour.")
                .small()
                .color(fg_weak()),
        );
    }
}
