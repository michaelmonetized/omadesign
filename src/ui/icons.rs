//! Phosphor Light glyphs in the tool well.

use crate::tools::Tool;
use crate::ui::theme::{accent, bg_widget_hover, border, fg, fg_weak};
use eframe::egui::{FontId, RichText, Sense, Stroke, Ui, vec2};

mod ph {
    pub const CURSOR: &str = "\u{E1DC}";
    pub const PATH: &str = "\u{E39C}";
    pub const PEN_NIB: &str = "\u{E3AC}";
    pub const PENCIL: &str = "\u{E3AE}";
    pub const RECTANGLE: &str = "\u{E3F0}";
    pub const CIRCLE: &str = "\u{E18A}";
    pub const HEXAGON: &str = "\u{E2AE}";
    pub const STAR: &str = "\u{E46A}";
    pub const LINE_SEGMENT: &str = "\u{E6D2}";
    pub const TEXT_T: &str = "\u{E48A}";
    pub const GRADIENT: &str = "\u{EB42}";
    pub const EYEDROPPER: &str = "\u{E568}";
    pub const PAINT_BRUSH: &str = "\u{E6F0}";
    pub const ERASER: &str = "\u{E21E}";
    pub const PAINT_BUCKET: &str = "\u{E392}";
    pub const COPY: &str = "\u{E1CA}";
    pub const DROP: &str = "\u{E210}";
    pub const CROP: &str = "\u{E1D4}";
    pub const SELECTION: &str = "\u{E69A}";
    pub const CIRCLE_DASHED: &str = "\u{E602}";
    pub const POLYGON: &str = "\u{E6D0}";
    pub const MAGIC_WAND: &str = "\u{E6B6}";
    pub const HAND: &str = "\u{E298}";
    pub const MAGNIFYING_GLASS_PLUS: &str = "\u{E310}";
}

pub fn tool_button(ui: &mut Ui, tool: Tool, selected: bool) -> bool {
    let size = vec2(36.0, 32.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hover = resp.hovered();
    let bg = if selected {
        accent().linear_multiply(0.22)
    } else if hover {
        bg_widget_hover()
    } else {
        eframe::egui::Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect.shrink(1.0), 5.0, bg);
    if selected {
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            5.0,
            Stroke::new(1.0, accent()),
            eframe::egui::StrokeKind::Inside,
        );
    }
    let color = if selected {
        accent()
    } else if hover {
        fg()
    } else {
        fg_weak()
    };
    ui.painter().text(
        rect.center(),
        eframe::egui::Align2::CENTER_CENTER,
        glyph(tool),
        FontId::proportional(18.0),
        color,
    );
    resp.on_hover_text(format!("{}  {}", tool.label(), tool.key()))
        .clicked()
}

fn glyph(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => ph::CURSOR,
        Tool::Node => ph::PATH,
        Tool::Pen => ph::PEN_NIB,
        Tool::Pencil => ph::PENCIL,
        Tool::Rect => ph::RECTANGLE,
        Tool::Ellipse => ph::CIRCLE,
        Tool::Polygon => ph::HEXAGON,
        Tool::Star => ph::STAR,
        Tool::Line => ph::LINE_SEGMENT,
        Tool::Text => ph::TEXT_T,
        Tool::Gradient => ph::GRADIENT,
        Tool::Eyedropper => ph::EYEDROPPER,
        Tool::Brush => ph::PAINT_BRUSH,
        Tool::Eraser => ph::ERASER,
        Tool::Fill => ph::PAINT_BUCKET,
        Tool::Clone => ph::COPY,
        Tool::Smudge => ph::DROP,
        Tool::Crop => ph::CROP,
        Tool::Marquee => ph::SELECTION,
        Tool::EllipseMarquee => ph::CIRCLE_DASHED,
        Tool::Lasso => ph::POLYGON,
        Tool::Wand => ph::MAGIC_WAND,
        Tool::Hand => ph::HAND,
        Tool::Zoom => ph::MAGNIFYING_GLASS_PLUS,
    }
}

pub fn well_separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(36.0, 8.0), Sense::hover());
    ui.painter()
        .hline(rect.x_range(), rect.center().y, Stroke::new(1.0, border()));
}

#[allow(dead_code)]
pub fn rich(icon: &str, size: f32) -> RichText {
    RichText::new(icon).font(FontId::proportional(size))
}
