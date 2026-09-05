//! A fixed-height, read-only shortcut strip. Modifier changes never move the canvas.
use crate::app::{KeyHint, Studio};
use crate::ui::theme::{accent, accent_soft, bg_panel, bg_widget, border, fg, fg_weak};
use eframe::egui::{
    self, Align2, FontId, Frame, Margin, Modifiers, Panel, Rect, RichText, Sense, Stroke, Ui, pos2,
    vec2,
};

pub(super) const HEIGHT: f32 = 58.0;
const ROW_HEIGHT: f32 = 22.0;
const LABEL_WIDTH: f32 = 116.0;
const GAP: f32 = 14.0;

pub(super) fn show(ui: &mut Ui, studio: &Studio) {
    if !studio.show_key_hud || studio.show_welcome {
        return;
    }
    let hints = studio.key_hints(ui.ctx());
    let idle_hint = match hints.context {
        "Window inactive" => "Focus the window to see its keys",
        "Dialog open" => "Finish or dismiss the dialog to return to the canvas",
        "Menu open" => "Choose an item, or Esc to return",
        "Control focused" => "Typing stays in the focused control",
        _ => "Hold a modifier to explore",
    };
    let modifiers = ui.input(|input| {
        if !input.focused
            || input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::WindowFocused(false)))
        {
            Modifiers::NONE
        } else {
            input.modifiers
        }
    });
    let mut held = Vec::new();
    if modifiers.ctrl || modifiers.command {
        held.push("Ctrl");
    }
    if modifiers.alt {
        held.push("Alt");
    }
    if modifiers.shift {
        held.push("Shift");
    }
    let key_heading = if held.is_empty() {
        "Keys".into()
    } else {
        held.join(" + ")
    };
    Panel::bottom("shortcut-hud")
        .exact_size(HEIGHT)
        .resizable(false)
        .frame(
            Frame::new()
                .fill(bg_panel())
                .inner_margin(Margin::symmetric(12, 4)),
        )
        .show(ui, |ui| {
            let rect = ui.max_rect();
            ui.painter()
                .hline(rect.x_range(), rect.top() - 4.0, Stroke::new(1.0, border()));
            ui.spacing_mut().item_spacing = vec2(0.0, 4.0);
            hint_row(
                ui,
                hints.context,
                &hints.gestures,
                idle_hint,
                Modifiers::NONE,
            );
            hint_row(
                ui,
                &key_heading,
                &hints.keys,
                "F1 opens the full key list",
                modifiers,
            );
        });
}

fn hint_row(ui: &mut Ui, heading: &str, hints: &[KeyHint], empty: &str, held: Modifiers) {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::hover());
    let painter = ui.painter_at(row);
    let label = egui::WidgetText::from(RichText::new(heading).size(11.0).strong().color(fg_weak()))
        .into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            LABEL_WIDTH - 12.0,
            egui::TextStyle::Body,
        );
    painter.galley(
        pos2(row.left(), row.center().y - label.size().y * 0.5),
        label,
        fg_weak(),
    );
    let content_left = row.left() + LABEL_WIDTH;
    if hints.is_empty() {
        painter.text(
            pos2(content_left, row.center().y),
            Align2::LEFT_CENTER,
            empty,
            FontId::proportional(11.0),
            fg_weak(),
        );
        return;
    }
    let measured: Vec<_> = hints
        .iter()
        .map(|hint| {
            let key = painter.layout_no_wrap(
                compact_key(hint.keys, held).into(),
                FontId::monospace(10.0),
                if hint.active { accent() } else { fg() },
            );
            let label = painter.layout_no_wrap(
                hint.label.into(),
                FontId::proportional(11.0),
                if hint.active { fg() } else { fg_weak() },
            );
            let width = key.size().x + 12.0 + 6.0 + label.size().x;
            (hint, key, label, width)
        })
        .collect();
    let total_width =
        measured.iter().map(|m| m.3).sum::<f32>() + GAP * measured.len().saturating_sub(1) as f32;
    let overflow = total_width > row.right() - content_left;
    let right = row.right() - if overflow { 84.0 } else { 0.0 };
    let mut x = content_left;
    let mut shown = 0;
    for (hint, key, label, width) in &measured {
        if x + width > right {
            break;
        }
        let cap = Rect::from_center_size(
            pos2(x + (key.size().x + 12.0) * 0.5, row.center().y),
            vec2(key.size().x + 12.0, 19.0),
        );
        painter.rect_filled(
            cap,
            4.0,
            if hint.active {
                accent_soft()
            } else {
                bg_widget()
            },
        );
        painter.rect_stroke(
            cap,
            4.0,
            Stroke::new(0.5, if hint.active { accent() } else { border() }),
            egui::StrokeKind::Inside,
        );
        painter.galley(cap.center() - key.size() * 0.5, key.clone(), fg());
        painter.galley(
            pos2(cap.right() + 6.0, row.center().y - label.size().y * 0.5),
            label.clone(),
            fg_weak(),
        );
        x += width + GAP;
        shown += 1;
    }
    if shown < hints.len() {
        let more = Rect::from_min_max(pos2(row.right() - 78.0, row.top()), row.max);
        painter.text(
            more.center(),
            Align2::CENTER_CENTER,
            format!("+{} more", hints.len() - shown),
            FontId::proportional(11.0),
            accent(),
        );
        // Hover-only: inspecting hints must not take keyboard focus from the canvas.
        ui.interact(
            more,
            ui.id().with(("hint-overflow", heading)),
            Sense::hover(),
        )
        .on_hover_ui(|ui| {
            ui.label(RichText::new(heading).strong());
            for hint in hints {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(hint.keys).monospace().color(if hint.active {
                        accent()
                    } else {
                        fg()
                    }));
                    ui.label(hint.label);
                });
            }
            ui.add_space(4.0);
            ui.label(
                RichText::new("F1 · All shortcuts    Ctrl+/ · Hide hints")
                    .small()
                    .color(fg_weak()),
            );
        });
    }
}

fn compact_key(mut keys: &'static str, held: Modifiers) -> &'static str {
    loop {
        let next = [
            ("Ctrl+", held.ctrl || held.command),
            ("Alt+", held.alt),
            ("Shift+", held.shift),
        ]
        .into_iter()
        .find_map(|(prefix, down)| down.then(|| keys.strip_prefix(prefix)).flatten());
        match next {
            Some(rest) => keys = rest,
            None => return keys,
        }
    }
}
