//! Phosphor Light glyphs in the tool well and chrome.

use crate::tools::Tool;
use crate::ui::theme::{accent, accent_soft, bg_widget_hover, border, fg, fg_weak};
use eframe::egui::{
    FontFamily, FontId, Response, Sense, Stroke, Ui, Vec2, WidgetInfo, WidgetType, vec2,
};

pub mod ph {
    pub const CURSOR: &str = "\u{E1DC}";
    pub const PATH: &str = "\u{E39C}";
    pub const PEN: &str = "\u{E3AA}";
    pub const PENCIL: &str = "\u{E3AE}";
    pub const RECTANGLE: &str = "\u{E3F0}";
    pub const CIRCLE: &str = "\u{E18A}";
    pub const HEXAGON: &str = "\u{E2AE}";
    pub const STAR: &str = "\u{E46A}";
    pub const LINE_SEGMENT: &str = "\u{E6D2}";
    pub const TEXT_T: &str = "\u{E48A}";
    pub const GRADIENT: &str = "\u{EB42}";
    pub const EYEDROPPER: &str = "\u{E568}";
    pub const VECTOR_TWO: &str = "\u{EE64}";
    pub const PAINT_BRUSH: &str = "\u{E6F0}";
    pub const ERASER: &str = "\u{E21E}";
    pub const PAINT_BUCKET: &str = "\u{E392}";
    pub const BANDAIDS: &str = "\u{E0B2}";
    pub const COPY: &str = "\u{E1CA}";
    pub const DROP: &str = "\u{E210}";
    pub const CROP: &str = "\u{E1D4}";
    pub const SELECTION: &str = "\u{E69A}";
    pub const CIRCLE_DASHED: &str = "\u{E602}";
    pub const POLYGON: &str = "\u{E6D0}";
    pub const MAGIC_WAND: &str = "\u{E6B6}";
    pub const HAND: &str = "\u{E298}";
    pub const MAGNIFYING_GLASS_PLUS: &str = "\u{E310}";
    pub const EYE: &str = "\u{E220}";
    pub const EYE_SLASH: &str = "\u{E224}";
    pub const LOCK: &str = "\u{E2FA}";
    pub const LOCK_OPEN: &str = "\u{E306}";
    pub const PLUS: &str = "\u{E3D4}";
    pub const X: &str = "\u{E4F6}";
    pub const FOLDER_OPEN: &str = "\u{E256}";
    pub const MINUS: &str = "\u{E32A}";
    pub const IMAGES: &str = "\u{E836}";
    pub const SHAPES: &str = "\u{EC5E}";
    pub const ALIGN_LEFT: &str = "\u{E50E}";
    pub const ALIGN_RIGHT: &str = "\u{E510}";
    pub const ALIGN_TOP: &str = "\u{E512}";
    pub const ALIGN_BOTTOM: &str = "\u{E506}";
    pub const ALIGN_CENTER_H: &str = "\u{E50A}";
    pub const ALIGN_CENTER_V: &str = "\u{E50C}";
    pub const STACK: &str = "\u{E466}";
    pub const CARET_DOWN: &str = "\u{E136}";
    pub const CARET_RIGHT: &str = "\u{E13A}";
    pub const FRAME_CORNERS: &str = "\u{E626}";
    pub const PLAY: &str = "\u{E3D0}";
    pub const PAUSE: &str = "\u{E39E}";
    pub const REPEAT: &str = "\u{E3F6}";
    pub const SKIP_BACK: &str = "\u{E5A4}";
    pub const SKIP_FORWARD: &str = "\u{E5A6}";
}

pub fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("phosphor".into()))
}

fn glyph_button(
    ui: &mut Ui,
    icon: &str,
    tip: &str,
    selected: bool,
    size: Vec2,
    glyph_size: f32,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response
        .widget_info(|| WidgetInfo::selected(WidgetType::Button, ui.is_enabled(), selected, tip));
    if ui.is_rect_visible(rect) {
        let active = response.hovered() || response.has_focus();
        if selected || active {
            ui.painter().rect_filled(
                rect.shrink(1.0),
                6.0,
                if selected {
                    accent_soft()
                } else {
                    bg_widget_hover()
                },
            );
        }
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect.shrink(1.0),
                6.0,
                Stroke::new(1.0, accent()),
                eframe::egui::StrokeKind::Inside,
            );
        }
        ui.painter().text(
            rect.center(),
            eframe::egui::Align2::CENTER_CENTER,
            icon,
            font(glyph_size),
            if selected {
                accent()
            } else if active {
                fg()
            } else {
                fg_weak()
            },
        );
    }
    response.on_hover_text(tip)
}

pub fn icon_button(ui: &mut Ui, icon: &str, tip: &str, selected: bool) -> bool {
    glyph_button(ui, icon, tip, selected, vec2(30.0, 28.0), 18.0).clicked()
}

pub fn tiny_icon(ui: &mut Ui, icon: &str, tip: &str, selected: bool) -> bool {
    glyph_button(ui, icon, tip, selected, vec2(22.0, 22.0), 15.0).clicked()
}

pub fn tool_button(ui: &mut Ui, tool: Tool, selected: bool) -> bool {
    glyph_button(
        ui,
        tool_glyph(tool),
        &format!("{}  {}", tool.label(), tool.key()),
        selected,
        vec2(36.0, 32.0),
        20.0,
    )
    .clicked()
}

fn tool_glyph(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => ph::CURSOR,
        Tool::Node => ph::PATH,
        Tool::Pen => ph::PEN,
        Tool::Pencil => ph::PENCIL,
        Tool::Rect => ph::RECTANGLE,
        Tool::Ellipse => ph::CIRCLE,
        Tool::Polygon => ph::HEXAGON,
        Tool::Star => ph::STAR,
        Tool::Line => ph::LINE_SEGMENT,
        Tool::Text => ph::TEXT_T,
        Tool::Gradient => ph::GRADIENT,
        Tool::Eyedropper => ph::EYEDROPPER,
        Tool::Trace => ph::VECTOR_TWO,
        Tool::Brush => ph::PAINT_BRUSH,
        Tool::Eraser => ph::ERASER,
        Tool::Fill => ph::PAINT_BUCKET,
        Tool::Clone => ph::COPY,
        Tool::Heal => ph::BANDAIDS,
        Tool::Smudge => ph::DROP,
        Tool::Crop => ph::CROP,
        Tool::Marquee => ph::SELECTION,
        Tool::EllipseMarquee => ph::CIRCLE_DASHED,
        Tool::Lasso => ph::POLYGON,
        Tool::Wand => ph::MAGIC_WAND,
        Tool::Hand => ph::HAND,
        Tool::Zoom => ph::MAGNIFYING_GLASS_PLUS,
        Tool::Artboard => ph::FRAME_CORNERS,
    }
}

pub fn well_separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(36.0, 10.0), Sense::hover());
    ui.painter().hline(
        rect.shrink(9.0).x_range(),
        rect.center().y,
        Stroke::new(1.0, border()),
    );
}
