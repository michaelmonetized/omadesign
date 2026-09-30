//! Bottom timeline. Playhead, object rows, key diamonds.

use crate::app::Studio;
use crate::motion::{Ease, Prop};
use crate::tools::Persona;
use crate::ui::icons::{self, ph};
use crate::ui::theme::{accent, bg_panel, bg_widget, border, fg, fg_weak, select};
use eframe::egui::{
    Align2, Color32, FontId, PointerButton, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2,
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
    ui.label(
        eframe::egui::RichText::new("Duration")
            .small()
            .color(fg_weak()),
    );
    let mut dur = studio.doc.motion.duration;
    if ui
        .add(
            eframe::egui::DragValue::new(&mut dur)
                .speed(0.05)
                .range(0.2..=60.0)
                .suffix("s"),
        )
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
        .add(
            eframe::egui::DragValue::new(&mut fps)
                .speed(1.0)
                .range(8.0..=60.0),
        )
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
    if let Some((_, _, _)) = studio.selected_key
        && ui.small_button("Cycle ease").clicked()
        && let Some((id, prop, i)) = studio.selected_key
    {
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

fn paint_timeline(ui: &mut Ui, studio: &mut Studio, rect: Rect, resp: &eframe::egui::Response) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, bg_panel());
    painter.hline(rect.x_range(), rect.min.y, Stroke::new(1.0, border()));

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

    let rows = row_ids(studio);
    let visible = ((lane.height() / ROW).floor() as usize).max(1);
    let max_scroll = rows.len().saturating_sub(visible);
    if resp.hovered() {
        let wheel = resp.ctx.input(|input| input.smooth_scroll_delta.y);
        if wheel > 0.0 {
            studio.timeline_scroll = studio.timeline_scroll.saturating_sub(1);
        } else if wheel < 0.0 {
            studio.timeline_scroll = studio.timeline_scroll.saturating_add(1);
        }
    }
    studio.timeline_scroll = studio.timeline_scroll.min(max_scroll);
    let start = studio.timeline_scroll;
    // Looking up a name walks the document. Resolve only the few rows painted
    // in this viewport, not every animated object on every playback frame.
    let rows: Vec<(u64, String)> = rows
        .into_iter()
        .skip(start)
        .take(visible)
        .map(|id| (id, shape_name(studio, id)))
        .collect();

    studio.forget_stale_key();
    let mut clicked_key: Option<(u64, Prop, usize)> = None;
    let mut clicked_row: Option<(usize, u64)> = None;
    let mut dragged_key: Option<(u64, Prop, usize, f32)> = None;
    let pointer = resp.interact_pointer_pos();
    let press = resp.ctx.input(|i| i.pointer.primary_pressed());
    let down = resp
        .ctx
        .input(|i| i.pointer.button_down(PointerButton::Primary));
    if !down {
        studio.key_drag = None;
    }

    for (i, (id, name)) in rows.iter().enumerate() {
        let y = lane.min.y + i as f32 * ROW;
        let row = Rect::from_min_max(pos2(rect.min.x, y), pos2(rect.max.x, y + ROW));
        let selected = crate::motion::selection(&studio.doc, *id)
            .is_some_and(|hit| studio.selection.contains(&hit));
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
                diamond(
                    &painter,
                    c,
                    if on { 6.0 } else { 5.0 },
                    if on { select() } else { col },
                );
                if let Some(p) = pointer
                    && (p - c).length() <= 8.0
                {
                    if press {
                        studio.key_drag = Some((*id, tr.prop, ki));
                        clicked_key = Some((*id, tr.prop, ki));
                    } else if resp.clicked() {
                        clicked_key = Some((*id, tr.prop, ki));
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
        if let Some(p) = pointer
            && resp.clicked()
            && row.contains(p)
            && p.x < lane.min.x
            && clicked_key.is_none()
            && let Some(hit) = find_shape_layer(studio, *id)
        {
            clicked_row = Some(hit);
        }
    }

    let px = t_to_x(studio.playhead);
    painter.line_segment(
        [pos2(px, rect.min.y), pos2(px, rect.max.y)],
        Stroke::new(1.5, accent()),
    );
    let head = Rect::from_center_size(pos2(px, ruler.center().y), vec2(8.0, 14.0));
    painter.rect_filled(head, 2.0, accent());

    if let (Some((id, prop, i)), Some(p)) = (studio.key_drag, pointer)
        && resp.dragged_by(PointerButton::Primary)
    {
        dragged_key = Some((id, prop, i, x_to_t(p.x)));
    }

    if (resp.clicked() || resp.dragged_by(PointerButton::Primary))
        && let Some(p) = pointer
        && dragged_key.is_none()
        && studio.key_drag.is_none()
        && (ruler.contains(p) || (p.x >= lane.min.x && p.y <= lane.max.y))
        && clicked_key.is_none()
    {
        studio.playhead = x_to_t(p.x);
        studio.playing = false;
    }
    if let Some(hit) = clicked_row {
        studio.selection = vec![hit];
        studio.selected_key = None;
        studio.active_layer = Some(hit.0);
        studio.status = shape_name(studio, hit.1);
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
            studio.active_layer = Some(hit.0);
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

fn row_ids(studio: &Studio) -> Vec<u64> {
    let mut rows = studio.doc.motion.shapes();
    let mut seen: std::collections::HashSet<_> = rows.iter().copied().collect();
    for &(layer, id) in &studio.selection {
        let id = if id == crate::document::RASTER_ID {
            let Some(layer) = studio
                .doc
                .layers
                .get(layer)
                .filter(|layer| layer.kind.pixels().is_some())
            else {
                continue;
            };
            layer.id
        } else {
            id
        };
        if seen.insert(id) {
            rows.push(id);
        }
    }
    rows
}

fn shape_name(studio: &Studio, id: u64) -> String {
    for layer in &studio.doc.layers {
        if layer.id == id && layer.kind.pixels().is_some() {
            return layer.name.clone();
        }
        if let Some(s) = layer.find(id) {
            return s.name.clone();
        }
    }
    format!("#{id}")
}

fn find_shape_layer(studio: &Studio, id: u64) -> Option<(usize, u64)> {
    crate::motion::selection(&studio.doc, id)
}

fn prop_color(p: Prop) -> Color32 {
    match p {
        Prop::X => Color32::from_rgb(0x89, 0xB4, 0xFA),
        Prop::Y => Color32::from_rgb(0xA6, 0xE3, 0xA1),
        Prop::Rotation => Color32::from_rgb(0xFA, 0xB3, 0x87),
        Prop::Scale => Color32::from_rgb(0xCB, 0xA6, 0xF7),
        Prop::Opacity => Color32::from_rgb(0xF5, 0xC2, 0xE7),
        Prop::StrokeReveal => accent(),
        Prop::FillReveal => fg(),
        Prop::GradientAngle => Color32::from_rgb(0x94, 0xE2, 0xD5),
        Prop::Width => Color32::from_rgb(0x74, 0xC7, 0xEC),
        Prop::Height => Color32::from_rgb(0xA6, 0xD1, 0x89),
        Prop::StrokeWidth => Color32::from_rgb(0xE6, 0xC3, 0x84),
        Prop::Fill => Color32::from_rgb(0xF3, 0x8B, 0xA8),
        Prop::Dash => Color32::from_rgb(0xBA, 0xC2, 0xDE),
        Prop::Gap => Color32::from_rgb(0x93, 0x9A, 0xB7),
        Prop::DashLength => Color32::from_rgb(0x7A, 0xA2, 0xF7),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Document, Shape, Style};
    use crate::geom::{Geom, Pt};
    use eframe::egui::{self, Event, Modifiers};

    fn fixture() -> Studio {
        let mut studio = Studio::new();
        studio.doc = Document::new("Timeline", 100.0, 100.0, 72.0);
        studio.persona = Persona::Motion;
        studio.doc.motion.duration = 2.0;
        for id in 101..=112 {
            let mut shape = Shape::new(Geom::Rect {
                origin: Pt::ZERO,
                size: Pt::new(10.0, 10.0),
                radius: 0.0,
            }, Style::default());
            shape.id = id;
            shape.name = format!("Object {id}");
            studio.doc.layers[1].kind.shapes_mut().unwrap().push(shape);
            studio.doc.motion.set_key(id, Prop::X, 1.0, 10.0, Ease::Linear);
        }
        studio
    }

    fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>)
        -> (Rect, Vec<egui::epaint::ClippedShape>)
    {
        let mut rect = Rect::NOTHING;
        let mut output = ctx.run_ui(egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(600.0, 240.0))),
            events,
            ..Default::default()
        }, |ui| {
            let (allocated, response) = ui.allocate_exact_size(vec2(500.0, 128.0), Sense::click_and_drag());
            rect = allocated;
            paint_timeline(ui, studio, rect, &response);
        });
        output.textures_delta.clear();
        (rect, output.shapes)
    }

    fn click(ctx: &egui::Context, studio: &mut Studio, pos: Pos2) {
        frame(ctx, studio, vec![Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            frame(ctx, studio, vec![Event::PointerButton {
                pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE,
            }]);
        }
    }

    #[test]
    fn raster_motion_timeline_rows_and_keys_select_the_right_image() {
        let mut s = Studio::new();
        s.persona = Persona::Motion;
        s.doc.layers = vec![
            crate::document::Layer::raster("First image", 10, 10),
            crate::document::Layer::raster("Second image", 10, 10),
        ];
        s.selection = vec![
            (0, crate::document::RASTER_ID),
            (1, crate::document::RASTER_ID),
        ];
        s.playhead = 1.;
        s.key_selection(Ease::Linear);
        let ids = [s.doc.layers[0].id, s.doc.layers[1].id];
        assert_eq!(row_ids(&s), ids);
        assert_eq!(shape_name(&s, ids[1]), "Second image");
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let (rect, _) = frame(&ctx, &mut s, vec![]);
        click(
            &ctx,
            &mut s,
            pos2(rect.left() + 20., rect.top() + 18. + ROW * 1.5),
        );
        assert_eq!(s.selection, vec![(1, crate::document::RASTER_ID)]);
        let x = rect.left() + LABEL + (rect.width() - LABEL - PAD) * 0.5;
        click(&ctx, &mut s, pos2(x, rect.top() + 18. + ROW * 0.5));
        assert_eq!(s.selection, vec![(0, crate::document::RASTER_ID)]);
        assert_eq!(s.active_layer, Some(0));
        assert_eq!(s.selected_key.unwrap().0, ids[0]);
        s.forget_stale_key();
        assert!(s.selected_key.is_some());
    }

    #[test]
    fn timeline_orders_animated_rows_then_unique_selected_objects() {
        let mut studio = fixture();
        studio
            .doc
            .motion
            .set_key(106, Prop::Y, 1.0, 5.0, Ease::Linear);
        studio.selection = vec![(1, 109), (1, 300), (1, 200), (1, 300)];
        let mut expected: Vec<_> = (101..=112).collect();
        expected.extend([300, 200]);
        assert_eq!(row_ids(&studio), expected);
        assert_eq!(shape_name(&studio, 300), "#300", "missing objects retain their row label");
    }

    #[test]
    fn scrolled_timeline_paints_only_visible_names_and_selects_their_rows_and_keys() {
        let ctx = egui::Context::default();
        let mut studio = fixture();
        studio.timeline_scroll = usize::MAX;
        let (rect, shapes) = frame(&ctx, &mut studio, vec![]);
        assert_eq!(studio.timeline_scroll, 8);
        let labels: Vec<_> = shapes.iter().filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text().starts_with("Object ") => {
                Some(text.galley.text().to_owned())
            }
            _ => None,
        }).collect();
        assert_eq!(labels, ["Object 109", "Object 110", "Object 111", "Object 112"]);

        click(&ctx, &mut studio, pos2(rect.left() + 30.0, rect.top() + 18.0 + ROW * 0.5));
        assert_eq!(studio.selection, [(1, 109)]);
        assert_eq!(studio.active_layer, Some(1));
        assert_eq!(studio.status, "Object 109");

        let lane_left = rect.left() + LABEL;
        let lane_width = rect.width() - LABEL - PAD;
        click(&ctx, &mut studio, pos2(lane_left + lane_width * 0.5, rect.top() + 18.0 + ROW * 1.5));
        assert_eq!(studio.selected_key, Some((110, Prop::X, 1)));
        assert_eq!(studio.selection, [(1, 110)]);
        assert_eq!(studio.playhead, 1.0);

        studio.playing = true;
        click(&ctx, &mut studio, pos2(lane_left + lane_width * 0.25, rect.top() + 9.0));
        assert!(!studio.playing, "ruler scrubbing pauses playback");
        assert!((studio.playhead - 0.5).abs() < 0.001);
    }
}
