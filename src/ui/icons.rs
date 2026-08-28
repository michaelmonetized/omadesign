//! Tiny tool glyphs painted with egui so the well reads like a studio, not a letter rack.

use crate::tools::Tool;
use crate::ui::theme::{ACCENT, FG_TEXT, FG_WEAK};
use eframe::egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};

pub fn tool_button(ui: &mut Ui, tool: Tool, selected: bool) -> bool {
    let size = vec2(36.0, 32.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hover = resp.hovered();
    let bg = if selected {
        Color32::from_rgb(0x2A, 0x22, 0x1C)
    } else if hover {
        Color32::from_rgb(0x24, 0x28, 0x30)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter()
        .rect_filled(rect.shrink(1.0), 5.0, bg);
    if selected {
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            5.0,
            Stroke::new(1.0, ACCENT),
            eframe::egui::StrokeKind::Inside,
        );
    }
    let color = if selected { ACCENT } else if hover { FG_TEXT } else { FG_WEAK };
    paint_tool(ui, rect.shrink(7.0), tool, color);
    resp.on_hover_text(format!("{}  {}", tool.label(), tool.key()))
        .clicked()
}

fn paint_tool(ui: &mut Ui, r: Rect, tool: Tool, c: Color32) {
    let p = ui.painter();
    let s = Stroke::new(1.4, c);
    let c0 = r.center();
    match tool {
        Tool::Select => {
            let pts = [
                pos2(r.left() + 2.0, r.top() + 1.0),
                pos2(r.left() + 2.0, r.bottom() - 2.0),
                pos2(r.left() + 8.0, r.bottom() - 8.0),
                pos2(r.right() - 2.0, r.bottom() - 2.0),
            ];
            p.line_segment([pts[0], pts[1]], s);
            p.line_segment([pts[1], pts[2]], s);
            p.line_segment([pts[0], pos2(r.right() - 1.0, r.center().y + 2.0)], s);
        }
        Tool::Node => {
            p.rect_stroke(
                Rect::from_center_size(c0, vec2(12.0, 10.0)),
                0.0,
                s,
                eframe::egui::StrokeKind::Middle,
            );
            for h in [
                r.left_top() + vec2(3.0, 3.0),
                r.right_top() + vec2(-3.0, 3.0),
                r.right_bottom() + vec2(-3.0, -3.0),
                r.left_bottom() + vec2(3.0, -3.0),
            ] {
                p.circle_filled(h, 1.8, c);
            }
        }
        Tool::Pen => {
            p.line_segment(
                [pos2(r.left() + 4.0, r.bottom() - 2.0), pos2(r.right() - 3.0, r.top() + 3.0)],
                s,
            );
            p.circle_filled(pos2(r.right() - 3.0, r.top() + 3.0), 2.0, c);
        }
        Tool::Pencil => {
            p.line_segment(
                [pos2(r.left() + 3.0, r.bottom() - 3.0), pos2(r.right() - 4.0, r.top() + 4.0)],
                s,
            );
            p.line_segment(
                [pos2(r.right() - 7.0, r.top() + 2.0), pos2(r.right() - 2.0, r.top() + 7.0)],
                s,
            );
        }
        Tool::Rect => {
            p.rect_stroke(
                r.shrink(2.0),
                2.0,
                s,
                eframe::egui::StrokeKind::Middle,
            );
        }
        Tool::Ellipse => {
            p.circle_stroke(c0, r.width() * 0.38, s);
        }
        Tool::Polygon => {
            let n = 6;
            let rad = r.width() * 0.38;
            let pts: Vec<Pos2> = (0..n)
                .map(|i| {
                    let a = -std::f32::consts::FRAC_PI_2
                        + std::f32::consts::TAU * i as f32 / n as f32;
                    pos2(c0.x + rad * a.cos(), c0.y + rad * a.sin())
                })
                .collect();
            for i in 0..n {
                p.line_segment([pts[i], pts[(i + 1) % n]], s);
            }
        }
        Tool::Star => {
            let n = 5;
            let mut pts = Vec::new();
            for i in 0..n * 2 {
                let a = -std::f32::consts::FRAC_PI_2
                    + std::f32::consts::TAU * i as f32 / (n * 2) as f32;
                let rad = if i % 2 == 0 {
                    r.width() * 0.42
                } else {
                    r.width() * 0.18
                };
                pts.push(pos2(c0.x + rad * a.cos(), c0.y + rad * a.sin()));
            }
            for i in 0..pts.len() {
                p.line_segment([pts[i], pts[(i + 1) % pts.len()]], s);
            }
        }
        Tool::Line => {
            p.line_segment([r.left_bottom() + vec2(2.0, -3.0), r.right_top() + vec2(-2.0, 3.0)], s);
        }
        Tool::Text => {
            p.text(
                c0,
                eframe::egui::Align2::CENTER_CENTER,
                "T",
                eframe::egui::FontId::proportional(16.0),
                c,
            );
        }
        Tool::Gradient => {
            let n = 6;
            for i in 0..n {
                let t = i as f32 / (n - 1) as f32;
                let col = Color32::from_rgba_unmultiplied(
                    (c.r() as f32 * (1.0 - t) + 255.0 * t) as u8,
                    (c.g() as f32 * (1.0 - t)) as u8,
                    (c.b() as f32 * (1.0 - t)) as u8,
                    255,
                );
                let x = r.left() + r.width() * t;
                p.rect_filled(
                    Rect::from_min_size(pos2(x, r.top() + 2.0), vec2(r.width() / n as f32 + 0.5, r.height() - 4.0)),
                    0.0,
                    col,
                );
            }
        }
        Tool::Eyedropper => {
            p.line_segment(
                [pos2(r.left() + 3.0, r.bottom() - 3.0), pos2(r.right() - 4.0, r.top() + 5.0)],
                s,
            );
            p.circle_stroke(pos2(r.right() - 5.0, r.top() + 5.0), 2.4, s);
        }
        Tool::Brush => {
            p.circle_filled(c0, 4.5, c);
            p.line_segment([c0, pos2(r.right() - 2.0, r.top() + 2.0)], s);
        }
        Tool::Eraser => {
            p.rect_stroke(
                Rect::from_center_size(c0, vec2(12.0, 7.0)),
                1.0,
                s,
                eframe::egui::StrokeKind::Middle,
            );
        }
        Tool::Fill => {
            p.rect_filled(
                Rect::from_min_size(pos2(r.left() + 3.0, r.center().y), vec2(r.width() - 6.0, r.height() * 0.35)),
                1.0,
                c,
            );
            p.circle_stroke(pos2(r.center().x, r.top() + 5.0), 3.0, s);
        }
        Tool::Clone => {
            p.circle_stroke(c0, 5.0, s);
            p.circle_filled(c0, 1.5, c);
        }
        Tool::Smudge => {
            p.line_segment([pos2(r.left() + 3.0, c0.y), pos2(r.right() - 3.0, c0.y - 3.0)], s);
            p.line_segment([pos2(r.left() + 3.0, c0.y + 3.0), pos2(r.right() - 3.0, c0.y)], s);
        }
        Tool::Crop => {
            p.line_segment([pos2(r.left() + 4.0, r.top() + 2.0), pos2(r.left() + 4.0, r.bottom() - 2.0)], s);
            p.line_segment([pos2(r.left() + 2.0, r.top() + 6.0), pos2(r.right() - 2.0, r.top() + 6.0)], s);
            p.line_segment([pos2(r.right() - 4.0, r.top() + 2.0), pos2(r.right() - 4.0, r.bottom() - 2.0)], s);
            p.line_segment([pos2(r.left() + 2.0, r.bottom() - 6.0), pos2(r.right() - 2.0, r.bottom() - 6.0)], s);
        }
        Tool::Marquee => {
            dashed_rect(p, r.shrink(2.0), c);
        }
        Tool::EllipseMarquee => {
            p.circle_stroke(c0, r.width() * 0.36, Stroke::new(1.1, c));
        }
        Tool::Lasso => {
            p.circle_stroke(pos2(c0.x, c0.y + 1.0), r.width() * 0.28, s);
            p.line_segment([pos2(c0.x + 5.0, c0.y - 2.0), pos2(r.right() - 2.0, r.top() + 2.0)], s);
        }
        Tool::Wand => {
            p.line_segment([pos2(r.left() + 3.0, r.bottom() - 3.0), pos2(r.right() - 5.0, r.top() + 6.0)], s);
            spark(p, pos2(r.right() - 4.0, r.top() + 4.0), c);
        }
        Tool::Hand => {
            p.rect_stroke(
                Rect::from_center_size(c0 + vec2(0.0, 2.0), vec2(10.0, 8.0)),
                2.0,
                s,
                eframe::egui::StrokeKind::Middle,
            );
            p.line_segment([pos2(c0.x - 2.0, r.top() + 3.0), pos2(c0.x - 2.0, c0.y)], s);
        }
        Tool::Zoom => {
            p.circle_stroke(c0 - vec2(1.5, 1.5), 5.0, s);
            p.line_segment([c0 + vec2(2.0, 2.0), r.right_bottom() - vec2(1.0, 1.0)], s);
        }
    }
}

fn dashed_rect(p: &eframe::egui::Painter, r: Rect, c: Color32) {
    let s = Stroke::new(1.2, c);
    dash_line(p, r.left_top(), r.right_top(), s);
    dash_line(p, r.right_top(), r.right_bottom(), s);
    dash_line(p, r.right_bottom(), r.left_bottom(), s);
    dash_line(p, r.left_bottom(), r.left_top(), s);
}

fn dash_line(p: &eframe::egui::Painter, a: Pos2, b: Pos2, s: Stroke) {
    let d = b - a;
    let len = d.length().max(1.0);
    let n = (len / 4.0) as i32;
    let dir = d / len;
    for i in 0..n {
        if i % 2 == 1 {
            continue;
        }
        let t0 = i as f32 * 4.0;
        let t1 = ((i + 1) as f32 * 4.0).min(len);
        p.line_segment([a + dir * t0, a + dir * t1], s);
    }
}

fn spark(p: &eframe::egui::Painter, at: Pos2, c: Color32) {
    let s = Stroke::new(1.2, c);
    p.line_segment([at - vec2(3.0, 0.0), at + vec2(3.0, 0.0)], s);
    p.line_segment([at - vec2(0.0, 3.0), at + vec2(0.0, 3.0)], s);
}

pub fn well_separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(36.0, 8.0), Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1.0, Color32::from_rgb(0x2C, 0x31, 0x3A)),
    );
}
