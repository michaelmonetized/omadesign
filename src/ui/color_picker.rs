//! One alpha-capable picker for artwork, paint, gradients and effect colors.
use crate::color::Rgba;
use crate::ui::icons::{self, ph};
use eframe::egui::{self, Color32, Id, Popup, PopupCloseBehavior, Sense, Ui, vec2};
use std::hash::Hash;

const SAMPLE_REQUEST: &str = "color-picker-sample";
const SAMPLE_RESULT: &str = "color-picker-sample-result";

/// The open color picker that asked for the next screen pixel, if any.
pub(crate) fn sample_request(ctx: &egui::Context) -> Option<Id> {
    ctx.data(|data| data.get_temp::<Id>(Id::new(SAMPLE_REQUEST)))
        .filter(|id| *id != Id::NULL)
}

/// Hand a screen pixel back to the picker that asked for it.
pub(crate) fn finish_sample(ctx: &egui::Context, color: Rgba) {
    let Some(id) = sample_request(ctx) else {
        return;
    };
    ctx.data_mut(|data| {
        data.insert_temp(Id::new(SAMPLE_REQUEST), Id::NULL);
        data.insert_temp(Id::new(SAMPLE_RESULT), (id, color));
    });
}

/// Drop a cancelled screen sample.
pub(crate) fn clear_sample(ctx: &egui::Context) {
    ctx.data_mut(|data| {
        data.insert_temp(Id::new(SAMPLE_REQUEST), Id::NULL);
        data.insert_temp(Id::new(SAMPLE_RESULT), (Id::NULL, Rgba::TRANSPARENT));
    });
}

fn take_sample(ctx: &egui::Context, id: Id) -> Option<Rgba> {
    let hit = ctx.data(|data| data.get_temp::<(Id, Rgba)>(Id::new(SAMPLE_RESULT)));
    let Some((who, color)) = hit else {
        return None;
    };
    if who != id || who == Id::NULL {
        return None;
    }
    ctx.data_mut(|data| data.insert_temp(Id::new(SAMPLE_RESULT), (Id::NULL, Rgba::TRANSPARENT)));
    Some(color)
}

fn request_sample(ctx: &egui::Context, id: Id) {
    ctx.data_mut(|data| data.insert_temp(Id::new(SAMPLE_REQUEST), id));
    crate::screen_pick::rearm();
}

#[derive(Clone)]
struct PickerState {
    previous: Rgba,
    last: Rgba,
    hsv: egui::ecolor::Hsva,
}

fn chip(ui: &mut Ui, color: Rgba, size: egui::Vec2) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let cell = 6.0;
    for row in 0..(size.y / cell).ceil() as usize {
        for col in 0..(size.x / cell).ceil() as usize {
            let tile = egui::Rect::from_min_size(
                rect.min + vec2(col as f32 * cell, row as f32 * cell),
                vec2(cell, cell),
            )
            .intersect(rect);
            ui.painter().rect_filled(
                tile,
                0.0,
                Color32::from_gray(if (row + col) % 2 == 0 { 170 } else { 110 }),
            );
        }
    }
    ui.painter().rect_filled(rect, 0.0, color.to_egui());
    ui.painter().rect_stroke(
        rect,
        2.0,
        ui.visuals().widgets.inactive.bg_stroke,
        egui::StrokeKind::Inside,
    );
    response
}

pub(crate) fn color_edit(ui: &mut Ui, salt: impl Hash + std::fmt::Debug, color: &mut Rgba) -> bool {
    let id = ui.make_persistent_id(("rgba-picker", salt));
    let popup = id.with("popup");
    let mut state = ui
        .data(|data| data.get_temp::<PickerState>(id))
        .unwrap_or_else(|| PickerState {
            previous: *color,
            last: *color,
            hsv: egui::ecolor::Hsva::from_srgba_unmultiplied(color.to_array()),
        });
    if state.last != *color {
        state.previous = state.last;
        state.hsv = egui::ecolor::Hsva::from_srgba_unmultiplied(color.to_array());
    }
    let before = *color;
    if let Some(sampled) = take_sample(ui.ctx(), id) {
        *color = sampled;
        state.hsv = egui::ecolor::Hsva::from_srgba_unmultiplied(color.to_array());
        state.last = sampled;
    }
    let response = chip(ui, *color, vec2(26.0, 22.0))
        .on_hover_text(format!("{} · Edit color and alpha", color.hex()));
    if response.clicked() && !Popup::is_id_open(ui.ctx(), popup) {
        state.previous = *color;
    }
    Popup::menu(&response)
        .id(popup)
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            picker_contents(ui, id, color, &mut state);
        });
    state.last = *color;
    ui.data_mut(|data| data.insert_temp(id, state));
    *color != before
}

fn picker_contents(ui: &mut Ui, id: Id, color: &mut Rgba, state: &mut PickerState) {
    ui.spacing_mut().slider_width = 240.0;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label("Current");
            chip(ui, *color, vec2(114.0, 36.0));
        });
        ui.vertical(|ui| {
            ui.label("Previous");
            if chip(ui, state.previous, vec2(114.0, 36.0))
                .on_hover_text("Restore the previous color")
                .clicked()
            {
                *color = state.previous;
                state.hsv = egui::ecolor::Hsva::from_srgba_unmultiplied(color.to_array());
            }
        });
    });
    ui.add_space(4.0);
    if egui::color_picker::color_picker_hsva_2d(
        ui,
        &mut state.hsv,
        egui::color_picker::Alpha::OnlyBlend,
    ) {
        *color = Rgba::from_array(state.hsv.to_srgba_unmultiplied());
    }
    ui.horizontal(|ui| {
        ui.label("Hex");
        if hex_field(ui, id.with("hex"), color, 0.0, true) {
            state.hsv = egui::ecolor::Hsva::from_srgba_unmultiplied(color.to_array());
        }
        if icons::tiny_icon(ui, ph::EYEDROPPER, "Sample a screen pixel", false) {
            request_sample(ui.ctx(), id);
        }
    });
}

/// Hex text with copy, paste, and keyboard clipboard while the field is focused.
pub(crate) fn hex_field(
    ui: &mut Ui,
    id: Id,
    color: &mut Rgba,
    reserve: f32,
    buttons: bool,
) -> bool {
    let mut text = ui
        .data(|data| data.get_temp::<(Rgba, String)>(id))
        .filter(|(source, _)| source == color)
        .map(|(_, text)| text)
        .unwrap_or_else(|| color.hex());
    let mut changed = false;
    let button_reserve = if buttons { 52.0 } else { 0.0 };
    let width = (ui.available_width() - reserve - button_reserve).clamp(28.0, 120.0);
    let response = ui.add(
        egui::TextEdit::singleline(&mut text)
            .id(id.with("edit"))
            .desired_width(width)
            .hint_text("#RRGGBB"),
    );
    if response.changed()
        && let Some(parsed) = Rgba::parse_hex(&text)
    {
        *color = parsed;
        text = color.hex();
        changed = true;
    }
    if response.has_focus() {
        let paste = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            })
        });
        let copy = ui.input(|input| {
            input.events.iter().any(|event| matches!(event, egui::Event::Copy))
                || input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Key { key: egui::Key::C, pressed: true, modifiers, .. } if modifiers.command || modifiers.ctrl))
        });
        if copy {
            ui.ctx().copy_text(if text.is_empty() {
                color.hex()
            } else {
                text.clone()
            });
        }
        if let Some(raw) = paste
            && let Some(parsed) = Rgba::parse_hex(raw.trim())
        {
            *color = parsed;
            text = color.hex();
            changed = true;
        }
    }
    if buttons {
        if icons::tiny_icon(ui, ph::COPY, "Copy hex", false) {
            ui.ctx().copy_text(color.hex());
        }
        if ui
            .add_sized([22.0, 22.0], egui::Button::new("↓").small())
            .on_hover_text("Paste hex")
            .clicked()
            && let Ok(crate::clipboard::ClipboardContent::Text(raw)) = crate::clipboard::read()
            && let Some(parsed) = Rgba::parse_hex(raw.trim())
        {
            *color = parsed;
            text = color.hex();
            changed = true;
        }
    }
    ui.data_mut(|data| data.insert_temp(id, (*color, text)));
    changed
}
