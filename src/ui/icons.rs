//! Phosphor Light glyphs in the tool well and chrome.

use crate::tools::{Persona, Tool};
use crate::ui::theme::{accent, accent_soft, bg_widget_hover, border, fg, fg_weak};
use eframe::egui::{
    FontFamily, FontId, Response, Sense, Stroke, Ui, Vec2, WidgetInfo, WidgetType, vec2,
};

mod gradient;
pub use gradient::sparkle_texture;

pub mod ph {
    pub const INFO: &str = "\u{E2CE}";
    pub const TEXT_AA: &str = "\u{E6EE}";
    pub const PALETTE: &str = "\u{E6C8}";
    pub const TOOLBOX: &str = "\u{ECA0}";
    pub const CURSOR: &str = "\u{E1DC}";
    pub const PATH: &str = "\u{E39C}";
    // Official Phosphor Light code points; the bundled font includes these glyphs.
    pub const BEZIER_CURVE: &str = "\u{EB00}";
    pub const LAYOUT: &str = "\u{E6D6}";
    pub const PERSON_SIMPLE_RUN: &str = "\u{E730}";
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
    pub const FLIP_HORIZONTAL: &str = "\u{ED6A}";
    pub const FLIP_VERTICAL: &str = "\u{ED6C}";
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

pub fn persona_glyph(persona: Persona) -> &'static str {
    match persona {
        Persona::Design => ph::BEZIER_CURVE,
        Persona::Pixel => ph::PAINT_BRUSH,
        Persona::Layout => ph::LAYOUT,
        Persona::Photo => ph::IMAGES,
        Persona::Motion => ph::PERSON_SIMPLE_RUN,
    }
}

pub fn mode_color(persona: Persona) -> eframe::egui::Color32 {
    let (r, g, b) = match persona {
        Persona::Design => (243, 92, 106),
        Persona::Pixel => (244, 156, 72),
        Persona::Layout => (225, 190, 58),
        Persona::Photo => (71, 158, 242),
        Persona::Motion => (125, 119, 242),
    };
    eframe::egui::Color32::from_rgb(r, g, b)
}

pub fn colored_button(ui: &mut Ui, icon: &str, tip: &str, selected: bool, color: eframe::egui::Color32) -> Response {
    let (rect, allocated) = ui.allocate_exact_size(vec2(36., 30.), Sense::hover());
    let id = match tip {
        "Agent" => eframe::egui::Id::new("studio-agent-toggle"),
        "Welcome" => eframe::egui::Id::new("studio-welcome-toggle"),
        _ => allocated.id,
    };
    let response = ui.interact(rect, id, Sense::click());
    response.widget_info(|| WidgetInfo::selected(WidgetType::Button, ui.is_enabled(), selected, tip));
    if selected || response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect.shrink(1.), 6., color.gamma_multiply(0.18));
    }
    if response.has_focus() {
        ui.painter().rect_stroke(rect.shrink(1.), 6., Stroke::new(1., color), eframe::egui::StrokeKind::Inside);
    }
    ui.painter().text(rect.center(), eframe::egui::Align2::CENTER_CENTER, icon, font(22.), color);
    response.on_hover_text(tip)
}

pub fn persona_button(ui: &mut Ui, persona: Persona, selected: bool) -> Response {
    colored_button(ui, persona_glyph(persona), &format!("{} — {}", persona.name(), persona.hint()), selected, mode_color(persona))
}

pub fn welcome_button(ui: &mut Ui, selected: bool) -> Response {
    let response = colored_button(ui, "", "Welcome", selected, eframe::egui::Color32::from_rgb(90, 195, 114));
    if let Some(texture) = svg_texture(ui, "welcome-omadesign-mark", include_bytes!("../../assets/omadesign-mark.svg")) {
        ui.painter().image(texture.id(), eframe::egui::Rect::from_center_size(response.rect.center(), vec2(24.,24.)), eframe::egui::Rect::from_min_max(eframe::egui::Pos2::ZERO, eframe::egui::pos2(1.,1.)), eframe::egui::Color32::from_rgb(90, 195, 114));
    }
    response
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
        Tool::BezierLasso => ph::BEZIER_CURVE,
        Tool::Wand => ph::MAGIC_WAND,
        Tool::Hand => ph::HAND,
        Tool::Zoom => ph::MAGNIFYING_GLASS_PLUS,
        Tool::Artboard => ph::FRAME_CORNERS,
        Tool::Frame => ph::STACK,
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

#[cfg(test)]
mod tests {
    use super::*;
    use ab_glyph::Font;

    #[test]
    fn every_mode_icon_is_present_in_the_bundled_phosphor_font() {
        let font = ab_glyph::FontRef::try_from_slice(include_bytes!(
            "../../assets/phosphor/Phosphor-Light.ttf"
        ))
        .unwrap();
        for persona in [
            Persona::Design,
            Persona::Pixel,
            Persona::Layout,
            Persona::Photo,
            Persona::Motion,
        ] {
            let character = persona_glyph(persona).chars().next().unwrap();
            assert_ne!(
                font.glyph_id(character).0,
                0,
                "{} must not render a missing glyph",
                persona.name()
            );
        }
    }
}

/// Cache a transparent SVG at native UI resolution. The same source artwork is
/// used in chrome and welcome so resizing never changes its aspect or padding.
pub fn svg_texture(
    ui: &Ui,
    key: &'static str,
    source: &[u8],
) -> Option<eframe::egui::TextureHandle> {
    svg_texture_id(ui, key, eframe::egui::Id::new(("ui-svg", key)), source)
}

fn svg_texture_id(ui: &Ui, key: &'static str, id: eframe::egui::Id, source: &[u8]) -> Option<eframe::egui::TextureHandle> {
    if let Some(texture) = ui
        .ctx()
        .data(|d| d.get_temp::<eframe::egui::TextureHandle>(id))
    {
        return Some(texture);
    }
    let tree = usvg::Tree::from_data(source, &usvg::Options::default()).ok()?;
    let scale = 768. / tree.size().width().max(tree.size().height());
    let mut pixels = tiny_skia::Pixmap::new(
        (tree.size().width() * scale).ceil().max(1.) as u32,
        (tree.size().height() * scale).ceil().max(1.) as u32,
    )?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixels.as_mut(),
    );
    let (w, h) = (pixels.width() as usize, pixels.height() as usize);
    let mut bounds = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if pixels.data()[(y * w + x) * 4 + 3] != 0 {
                bounds = (
                    bounds.0.min(x),
                    bounds.1.min(y),
                    bounds.2.max(x + 1),
                    bounds.3.max(y + 1),
                );
            }
        }
    }
    if bounds.0 >= bounds.2 {
        return None;
    }
    let mut cropped = Vec::new();
    for y in bounds.1..bounds.3 {
        cropped.extend_from_slice(&pixels.data()[(y * w + bounds.0) * 4..(y * w + bounds.2) * 4]);
    }
    let image = eframe::egui::ColorImage::from_rgba_premultiplied(
        [bounds.2 - bounds.0, bounds.3 - bounds.1],
        &cropped,
    );
    let texture = ui
        .ctx()
        .load_texture(key, image, eframe::egui::TextureOptions::LINEAR);
    ui.ctx().data_mut(|d| d.insert_temp(id, texture.clone()));
    Some(texture)
}
