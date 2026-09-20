//! One alpha-capable picker for artwork, paint, gradients and effect colors.
use crate::color::Rgba;
use eframe::egui::{self, Color32, Id, Popup, PopupCloseBehavior, Sense, Ui, vec2};
use std::hash::Hash;

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
    let text_id = id.with("hex");
    let mut hex = ui
        .data(|data| data.get_temp::<(Rgba, String)>(text_id))
        .filter(|(source, _)| source == color)
        .map(|(_, text)| text)
        .unwrap_or_else(|| color.hex());
    ui.horizontal(|ui| {
        ui.label("Hex / RGBA");
        let response = ui.add(egui::TextEdit::singleline(&mut hex).desired_width(120.0));
        if response.changed()
            && let Some(parsed) = Rgba::parse_hex(&hex)
        {
            *color = parsed;
            state.hsv = egui::ecolor::Hsva::from_srgba_unmultiplied(color.to_array());
        }
    });
    ui.data_mut(|data| data.insert_temp(text_id, (*color, hex)));
}
