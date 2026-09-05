//! Bottom timeline. Playhead, object rows, key diamonds.

use crate::app::Studio;
use crate::motion::{Ease, Prop};
use crate::tools::Persona;
use crate::ui::icons::{self, ph};
use crate::ui::theme::{accent, bg_panel, bg_widget, border, fg, fg_weak, select};
use eframe::egui::{
    pos2, vec2, Align2, Color32, FontId, PointerButton, Pos2, Rect, Sense, Stroke, Ui,
};

const ROW: f32 = 22.0;
const LABEL: f32 = 108.0;
const PAD: f32 = 8.0;

pub fn show(ui: &mut Ui, studio: &mut Studio) {
    if studio.persona != Persona::Motion {
        return;
    }
    eframe::egui::Panel::bottom("timeline")
        .exact_size(168.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                transport(ui, studio);
            });
            ui.add_space(2.0);
            let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
            paint_timeline(ui, studio, rect, &resp);
        });
}

fn transport(ui: &mut Ui, studio: &mut Studio) {
    if icons::tiny_icon(ui, ph::SKIP_BACK, "To start  Home", false) {
        studio.playhead = 0.0;
        studio.playing = false;
    }
    let play = if studio.playing { ph::PAUSE } else { ph::PLAY };
    if icons::tiny_icon(
        ui,
        play,
        if studio.playing {
            "Pause  Space"
        } else {
            "Play  Space"
        },
        studio.playing,
    ) {
        studio.playing = !studio.playing;
    }
    if icons::tiny_icon(ui, ph::SKIP_FORWARD, "To end  End", false) {
        studio.playhead = studio.doc.motion.duration;
        studio.playing = false;
    }
    if icons::tiny_icon(ui, ph::REPEAT, "Loop", studio.doc.motion.looped) {
        let mut after = studio.doc.motion.clone();
        after.looped = !after.looped;
        studio.commit_motion(after);
    }
    ui.separator();
    ui.label(
        eframe::egui::RichText::new(format!(
            "{:.2}s  /  {:.2}s",
            studio.playhead, studio.doc.motion.duration
        ))
        .monospace()
        .small()
        .color(fg()),
    );
    let frame = (studio.playhead * studio.doc.motion.fps).round() as i32;
    ui.label(
        eframe::egui::RichText::new(format!("f{frame}"))
            .monospace()
            .small()
            .color(fg_weak()),
    );
    ui.separator();
    ui.label(eframe::egui::RichText::new("Duration").small().color(fg_weak()));
    let mut dur = studio.doc.motion.duration;
    if ui
        .add(eframe::egui::DragValue::new(&mut dur).speed(0.05).range(0.2..=60.0).suffix("s"))
        .changed()
    {
        let mut after = studio.doc.motion.clone();
        after.duration = dur;
        studio.playhead = studio.playhead.min(dur);
        studio.commit_motion(after);
    }
    ui.label(eframe::egui::RichText::new("fps").small().color(fg_weak()));
    let mut fps = studio.doc.motion.fps;
    if ui
        .add(eframe::egui::DragValue::new(&mut fps).speed(1.0).range(8.0..=60.0))
        .changed()
    {
        let mut after = studio.doc.motion.clone();
        after.fps = fps;
        studio.commit_motion(after);
    }
    ui.separator();
    if ui
        .add_enabled(
            !studio.selection.is_empty(),
            eframe::egui::Button::new("Key  K").small(),
        )
        .on_hover_text("Key X/Y/Rotate/Scale at the playhead")
        .clicked()
    {
        studio.key_selection(Ease::EaseInOut);
    }
    if let Some((_, _, _)) = studio.selected_key {
        if ui.small_button("Cycle ease").clicked() {
            if let Some((id, prop, i)) = studio.selected_key {
                let mut after = studio.doc.motion.clone();
                if let Some(tr) = after
                    .tracks
                    .iter_mut()
                    .find(|tr| tr.shape == id && tr.prop == prop)
                    && let Some(k) = tr.keys.get_mut(i)
                {
                    k.ease = k.ease.cycle();
                    studio.status = format!("ease {}", k.ease.name());
                }
                studio.commit_motion(after);
            }
        }
    }
}

fn paint_timeline(ui: &mut Ui, studio: &mut Studio, rect: Rect, resp: &eframe::egui::Response) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, bg_panel());
    painter.hline(
        rect.x_range(),
        rect.min.y,
        Stroke::new(1.0, border()),
    );

    let dur = studio.doc.motion.duration.max(0.05);
    let lane = Rect::from_min_max(
        pos2(rect.min.x + LABEL, rect.min.y + 18.0),
        pos2(rect.max.x - PAD, rect.max.y - 4.0),
    );
    let ruler = Rect::from_min_max(
        pos2(lane.min.x, rect.min.y),
        pos2(lane.max.x, rect.min.y + 18.0),
    );

    let t_to_x = |t: f32| lane.min.x + (t / dur).clamp(0.0, 1.0) * lane.width();
    let x_to_t = |x: f32| ((x - lane.min.x) / lane.width().max(1.0)).clamp(0.0, 1.0) * dur;

    painter.rect_filled(ruler, 0.0, bg_widget());
    let step = nice_time(dur);
    let mut t = 0.0;
    while t <= dur + 1e-4 {
        let x = t_to_x(t);
        painter.line_segment(
            [pos2(x, ruler.min.y + 10.0), pos2(x, ruler.max.y)],
            Stroke::new(1.0, border()),
        );
        painter.text(
            pos2(x + 3.0, ruler.min.y + 2.0),
            Align2::LEFT_TOP,
            format!("{t:.2}s"),
            FontId::monospace(9.0),
            fg_weak(),
        );
        t += step;
    }

    let mut rows: Vec<(u64, String)> = Vec::new();
    for id in studio.doc.motion.shapes() {
        let name = shape_name(studio, id);
        rows.push((id, name));
    }
    for (_, id) in &studio.selection {
        if !rows.iter().any(|(x, _)| *x == *id) {
            rows.push((*id, shape_name(studio, *id)));
        }
    }
    rows.truncate(((lane.height() / ROW).floor() as usize).max(1));

    let mut clicked_key: Option<(u64, Prop, usize)> = None;
    let mut dragged_key: Option<(u64, Prop, usize, f32)> = None;
    let pointer = resp.interact_pointer_pos();
    let press = resp.ctx.input(|i| i.pointer.primary_pressed());
    let down = resp.ctx.input(|i| i.pointer.button_down(PointerButton::Primary));
    if !down {
        studio.key_drag = None;
    }

    for (i, (id, name)) in rows.iter().enumerate() {
        let y = lane.min.y + i as f32 * ROW;
        let row = Rect::from_min_max(pos2(rect.min.x, y), pos2(rect.max.x, y + ROW));
        let selected = studio.selection.iter().any(|(_, sid)| sid == id);
        if selected {
            painter.rect_filled(row, 0.0, accent().linear_multiply(0.10));
        } else if i % 2 == 1 {
            painter.rect_filled(row, 0.0, bg_widget().linear_multiply(0.4));
        }
        painter.text(
            pos2(rect.min.x + 8.0, y + 4.0),
            Align2::LEFT_TOP,
            name,
            FontId::proportional(11.0),
            if selected { accent() } else { fg() },
        );
        painter.line_segment(
            [pos2(lane.min.x, y + ROW), pos2(lane.max.x, y + ROW)],
            Stroke::new(1.0, border().linear_multiply(0.5)),
        );
        for tr in studio.doc.motion.tracks.iter().filter(|tr| tr.shape == *id) {
            let col = prop_color(tr.prop);
            for (ki, k) in tr.keys.iter().enumerate() {
                let x = t_to_x(k.t);
                let c = pos2(x, y + ROW * 0.5);
                let on = studio.selected_key == Some((*id, tr.prop, ki));
                diamond(&painter, c, if on { 6.0 } else { 5.0 }, if on { select() } else { col });
                if let Some(p) = pointer {
                    if (p - c).length() <= 8.0 {
                        if press {
                            studio.key_drag = Some((*id, tr.prop, ki));
                            clicked_key = Some((*id, tr.prop, ki));
                        } else if resp.clicked() {
                            clicked_key = Some((*id, tr.prop, ki));
                        }
                    }
                }
            }
        }
        if let Some(p) = pointer
            && resp.double_clicked()
            && row.contains(p)
            && p.x >= lane.min.x
        {
            let t = x_to_t(p.x);
            studio.playhead = t;
            studio.key_selection(Ease::EaseInOut);
        }
    }

    let px = t_to_x(studio.playhead);
    painter.line_segment(
        [pos2(px, rect.min.y), pos2(px, rect.max.y)],
        Stroke::new(1.5, accent()),
    );
    let head = Rect::from_center_size(pos2(px, ruler.center().y), vec2(8.0, 14.0));
    painter.rect_filled(head, 2.0, accent());

    if let (Some((id, prop, i)), Some(p)) = (studio.key_drag, pointer) {
        if resp.dragged_by(PointerButton::Primary) {
            dragged_key = Some((id, prop, i, x_to_t(p.x)));
        }
    }

    if resp.clicked() || resp.dragged_by(PointerButton::Primary) {
        if let Some(p) = pointer {
            if dragged_key.is_none()
                && studio.key_drag.is_none()
                && (ruler.contains(p) || (p.x >= lane.min.x && p.y <= lane.max.y))
                && clicked_key.is_none()
            {
                studio.playhead = x_to_t(p.x);
                studio.playing = false;
            }
        }
    }
    if let Some((id, prop, i)) = clicked_key {
        studio.selected_key = Some((id, prop, i));
        if let Some(k) = studio
            .doc
            .motion
            .tracks
            .iter()
            .find(|tr| tr.shape == id && tr.prop == prop)
            .and_then(|tr| tr.keys.get(i))
        {
            studio.playhead = k.t;
            studio.status = format!("{} · {} · {:.2}s", prop.name(), k.ease.name(), k.t);
        }
        if let Some(hit) = find_shape_layer(studio, id) {
            studio.selection = vec![hit];
        }
    }
    if let Some((id, prop, i, t)) = dragged_key {
        let mut after = studio.doc.motion.clone();
        if let Some(tr) = after
            .tracks
            .iter_mut()
            .find(|tr| tr.shape == id && tr.prop == prop)
            && let Some(k) = tr.keys.get_mut(i)
        {
            k.t = t.clamp(0.0, dur);
        }
        if let Some(tr) = after
            .tracks
            .iter_mut()
            .find(|tr| tr.shape == id && tr.prop == prop)
        {
            tr.keys
                .sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
        }
        studio.commit_motion(after);
        studio.selected_key = Some((id, prop, i));
        studio.playhead = t;
        studio.playing = false;
    }
}

fn shape_name(studio: &Studio, id: u64) -> String {
    for layer in &studio.doc.layers {
        if let Some(s) = layer.find(id) {
            return s.name.clone();
        }
    }
    format!("#{id}")
}

fn find_shape_layer(studio: &Studio, id: u64) -> Option<(usize, u64)> {
    for (li, layer) in studio.doc.layers.iter().enumerate() {
        if layer.find(id).is_some() {
            return Some((li, id));
        }
    }
    None
}

fn prop_color(p: Prop) -> Color32 {
    match p {
        Prop::X => Color32::from_rgb(0x89, 0xB4, 0xFA),
        Prop::Y => Color32::from_rgb(0xA6, 0xE3, 0xA1),
        Prop::Rotation => Color32::from_rgb(0xFA, 0xB3, 0x87),
        Prop::Scale => Color32::from_rgb(0xCB, 0xA6, 0xF7),
        Prop::Opacity => Color32::from_rgb(0xF5, 0xC2, 0xE7),
    }
}

fn diamond(p: &eframe::egui::Painter, c: Pos2, r: f32, col: Color32) {
    let pts = vec![
        pos2(c.x, c.y - r),
        pos2(c.x + r, c.y),
        pos2(c.x, c.y + r),
        pos2(c.x - r, c.y),
    ];
    p.add(eframe::egui::epaint::PathShape {
        points: pts,
        closed: true,
        fill: col,
        stroke: Stroke::new(1.0, Color32::from_black_alpha(80)).into(),
    });
}

fn nice_time(dur: f32) -> f32 {
    let raw = dur / 4.0;
    let p = 10f32.powf(raw.max(0.05).log10().floor());
    let n = raw / p;
    let m = if n < 2.0 {
        1.0
    } else if n < 5.0 {
        2.0
    } else {
        5.0
    };
    (m * p).max(0.05)
}
