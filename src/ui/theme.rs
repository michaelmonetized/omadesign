use eframe::egui::{
    style::{Selection, WidgetVisuals, Widgets},
    Color32, Context, CornerRadius, Shadow, Stroke, TextStyle, Theme, Vec2, Visuals,
};

pub const ACCENT: Color32 = Color32::from_rgb(0xF4, 0x7C, 0x2E);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(0xB5, 0x5A, 0x1F);
pub const ACCENT_SOFT: Color32 = Color32::from_rgba_premultiplied(0xF4, 0x7C, 0x2E, 40);

pub const BG_WINDOW: Color32 = Color32::from_rgb(0x12, 0x14, 0x18);
pub const BG_PANEL: Color32 = Color32::from_rgb(0x18, 0x1B, 0x21);
pub const BG_WIDGET: Color32 = Color32::from_rgb(0x22, 0x26, 0x2E);
pub const BG_WIDGET_HOVER: Color32 = Color32::from_rgb(0x2C, 0x31, 0x3B);
pub const BG_WIDGET_ACTIVE: Color32 = Color32::from_rgb(0x36, 0x3C, 0x48);
pub const BG_EXTREME: Color32 = Color32::from_rgb(0x0C, 0x0E, 0x12);
pub const BG_CANVAS: Color32 = Color32::from_rgb(0x22, 0x26, 0x2C);

pub const FG_TEXT: Color32 = Color32::from_rgb(0xE8, 0xEA, 0xF0);
pub const FG_WEAK: Color32 = Color32::from_rgb(0x8A, 0x93, 0xA6);
pub const FG_STRONG: Color32 = Color32::WHITE;

pub const BORDER: Color32 = Color32::from_rgb(0x2C, 0x31, 0x3A);
pub const BORDER_STRONG: Color32 = Color32::from_rgb(0x3D, 0x43, 0x50);

pub const SELECT: Color32 = Color32::from_rgb(0x4D, 0xAE, 0xFF);
pub const SELECT_FILL: Color32 = Color32::from_rgba_premultiplied(0x4D, 0xAE, 0xFF, 28);

const R4: CornerRadius = CornerRadius::same(4);

fn widget(bg: Color32) -> WidgetVisuals {
    WidgetVisuals {
        bg_fill: bg,
        weak_bg_fill: bg,
        bg_stroke: Stroke::new(1.0, BORDER),
        corner_radius: R4,
        fg_stroke: Stroke::new(1.0, FG_TEXT),
        expansion: 0.0,
    }
}

pub fn apply(ctx: &Context) {
    ctx.set_theme(eframe::egui::ThemePreference::Dark);

    let widgets = Widgets {
        noninteractive: widget(BG_WIDGET),
        inactive: widget(BG_WIDGET),
        hovered: widget(BG_WIDGET_HOVER),
        active: widget(BG_WIDGET_ACTIVE),
        open: widget(BG_WIDGET),
        ..Default::default()
    };

    let visuals = Visuals {
        dark_mode: true,
        override_text_color: Some(FG_TEXT),
        weak_text_alpha: 0.6,
        weak_text_color: Some(FG_WEAK),
        widgets,
        selection: Selection {
            bg_fill: ACCENT_DIM,
            stroke: Stroke::new(1.0, ACCENT),
        },
        hyperlink_color: ACCENT,
        faint_bg_color: Color32::from_rgb(0x20, 0x23, 0x2B),
        extreme_bg_color: BG_EXTREME,
        text_edit_bg_color: Some(BG_EXTREME),
        code_bg_color: BG_WIDGET,
        warn_fg_color: Color32::from_rgb(0xE8, 0xA0, 0x33),
        error_fg_color: Color32::from_rgb(0xE5, 0x48, 0x4D),
        window_corner_radius: CornerRadius::same(8),
        window_shadow: Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: Color32::from_black_alpha(140),
        },
        window_fill: BG_WINDOW,
        window_stroke: Stroke::new(1.0, BORDER_STRONG),
        window_highlight_topmost: false,
        menu_corner_radius: R4,
        panel_fill: BG_PANEL,
        popup_shadow: Shadow {
            offset: [0, 4],
            blur: 12,
            spread: 0,
            color: Color32::from_black_alpha(120),
        },
        resize_corner_size: 12.0,
        ..Default::default()
    };
    ctx.set_visuals(visuals);

    ctx.style_mut_of(Theme::Dark, |style| {
        style.spacing.item_spacing = Vec2::new(6.0, 6.0);
        style.spacing.button_padding = Vec2::new(8.0, 4.0);
        style.spacing.icon_width = 14.0;
        style.spacing.slider_width = 132.0;
        style.spacing.slider_rail_height = 3.0;
        style.spacing.interact_size = Vec2::new(24.0, 22.0);
        style.spacing.scroll = eframe::egui::style::ScrollStyle::thin();
        style.spacing.indent = 12.0;

        style.text_styles.insert(
            TextStyle::Heading,
            eframe::egui::FontId::new(16.0, eframe::egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Body,
            eframe::egui::FontId::new(13.0, eframe::egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Button,
            eframe::egui::FontId::new(12.5, eframe::egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Small,
            eframe::egui::FontId::new(11.0, eframe::egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Monospace,
            eframe::egui::FontId::new(12.0, eframe::egui::FontFamily::Monospace),
        );
    });
}
