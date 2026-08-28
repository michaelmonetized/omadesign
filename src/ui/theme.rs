//! UI chrome follows the desktop: Omarchy theme + fontconfig. No baked-in brand hex.

use eframe::egui::{
    style::{Selection, WidgetVisuals, Widgets},
    Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Shadow, Stroke,
    TextStyle, Theme, Vec2, Visuals,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Clone, Debug)]
pub struct Palette {
    pub accent: Color32,
    pub accent_dim: Color32,
    pub accent_soft: Color32,
    pub bg_window: Color32,
    pub bg_panel: Color32,
    pub bg_widget: Color32,
    pub bg_widget_hover: Color32,
    pub bg_widget_active: Color32,
    pub bg_extreme: Color32,
    pub bg_canvas: Color32,
    pub fg: Color32,
    pub fg_weak: Color32,
    pub fg_strong: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub select: Color32,
    pub select_fill: Color32,
    pub warn: Color32,
    pub error: Color32,
    pub dark: bool,
}

static PALETTE: OnceLock<Palette> = OnceLock::new();

pub fn p() -> &'static Palette {
    PALETTE.get_or_init(Palette::load)
}

pub fn accent() -> Color32 {
    p().accent
}
pub fn accent_dim() -> Color32 {
    p().accent_dim
}
pub fn accent_soft() -> Color32 {
    p().accent_soft
}
pub fn bg_window() -> Color32 {
    p().bg_window
}
pub fn bg_panel() -> Color32 {
    p().bg_panel
}
pub fn bg_widget() -> Color32 {
    p().bg_widget
}
pub fn bg_widget_hover() -> Color32 {
    p().bg_widget_hover
}
pub fn bg_widget_active() -> Color32 {
    p().bg_widget_active
}
pub fn bg_extreme() -> Color32 {
    p().bg_extreme
}
pub fn bg_canvas() -> Color32 {
    p().bg_canvas
}
pub fn fg() -> Color32 {
    p().fg
}
pub fn fg_weak() -> Color32 {
    p().fg_weak
}
pub fn fg_strong() -> Color32 {
    p().fg_strong
}
pub fn border() -> Color32 {
    p().border
}
pub fn border_strong() -> Color32 {
    p().border_strong
}
pub fn select() -> Color32 {
    p().select
}
pub fn select_fill() -> Color32 {
    p().select_fill
}

fn hex(s: &str) -> Option<Color32> {
    let s = s.trim().trim_matches('"').trim_matches('\'');
    let s = s.strip_prefix('#').unwrap_or(s);
    let n = u32::from_str_radix(s.get(..6)?, 16).ok()?;
    Some(Color32::from_rgb(
        ((n >> 16) & 0xFF) as u8,
        ((n >> 8) & 0xFF) as u8,
        (n & 0xFF) as u8,
    ))
}

fn lum(c: Color32) -> f32 {
    0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgba_unmultiplied(
        (a.r() as f32 * (1.0 - t) + b.r() as f32 * t) as u8,
        (a.g() as f32 * (1.0 - t) + b.g() as f32 * t) as u8,
        (a.b() as f32 * (1.0 - t) + b.b() as f32 * t) as u8,
        255,
    )
}

fn dim(c: Color32, t: f32) -> Color32 {
    mix(c, Color32::BLACK, t)
}

fn lift(c: Color32, t: f32) -> Color32 {
    mix(c, Color32::WHITE, t)
}

fn alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

fn parse_kv(text: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        m.insert(k.trim().to_string(), v.trim().trim_matches('"').to_string());
    }
    m
}

fn get(map: &HashMap<String, String>, keys: &[&str]) -> Option<Color32> {
    for k in keys {
        if let Some(v) = map.get(*k)
            && let Some(c) = hex(v)
        {
            return Some(c);
        }
    }
    None
}

fn read_to_string(p: &Path) -> Option<String> {
    std::fs::read_to_string(p).ok()
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

fn omarchy_theme_name() -> Option<String> {
    let home = home();
    read_to_string(&home.join(".local/state/omarchy/current/theme.name"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn omarchy_colors_toml() -> Option<String> {
    let home = home();
    let name = omarchy_theme_name();
    let candidates = [
        home.join(".local/state/omarchy/current/theme/colors.toml"),
        name.as_ref()
            .map(|n| home.join(format!(".config/omarchy/themes/{n}/colors.toml")))
            .unwrap_or_default(),
        name.as_ref()
            .map(|n| PathBuf::from(format!("/usr/share/omarchy/themes/{n}/colors.toml")))
            .unwrap_or_default(),
        home.join(".config/omarchy/themes/catppuccin/colors.toml"),
        PathBuf::from("/usr/share/omarchy/themes/catppuccin/colors.toml"),
    ];
    for p in candidates {
        if p.as_os_str().is_empty() {
            continue;
        }
        if let Some(s) = read_to_string(&p) {
            return Some(s);
        }
    }
    None
}

impl Palette {
    pub fn load() -> Self {
        if let Some(toml) = omarchy_colors_toml() {
            return Self::from_map(&parse_kv(&toml));
        }
        Self::fallback()
    }

    fn from_map(m: &HashMap<String, String>) -> Self {
        let bg = get(m, &["background", "base", "bg"]).unwrap_or(Color32::from_rgb(0x1E, 0x1E, 0x2E));
        let fg = get(m, &["foreground", "text", "fg"]).unwrap_or(Color32::from_rgb(0xCD, 0xD6, 0xF4));
        let accent = get(m, &["accent", "blue", "color4", "primary"])
            .unwrap_or(Color32::from_rgb(0x89, 0xB4, 0xFA));
        let muted = get(m, &["muted", "dark_foreground", "color8", "overlay0"])
            .unwrap_or(mix(fg, bg, 0.45));
        let sel = get(m, &["selection", "selection_background", "surface1"])
            .unwrap_or(mix(accent, bg, 0.55));
        let dark = lum(bg) < 140.0;
        let panel = get(m, &["dark_background", "mantle", "surface0"]).unwrap_or(if dark {
            lift(bg, 0.04)
        } else {
            dim(bg, 0.04)
        });
        let widget = get(m, &["lighter_background", "surface1"]).unwrap_or(if dark {
            lift(bg, 0.10)
        } else {
            dim(bg, 0.08)
        });
        let extreme = get(m, &["darker_background", "crust"]).unwrap_or(if dark {
            dim(bg, 0.25)
        } else {
            lift(bg, 0.12)
        });
        let error = get(m, &["red", "color1"]).unwrap_or(Color32::from_rgb(0xF3, 0x8B, 0xA8));
        let warn = get(m, &["yellow", "orange", "color3"]).unwrap_or(Color32::from_rgb(0xF9, 0xE2, 0xAF));
        let border = mix(widget, fg, 0.12);
        Self {
            accent,
            accent_dim: dim(accent, 0.28),
            accent_soft: alpha(accent, 40),
            bg_window: bg,
            bg_panel: panel,
            bg_widget: widget,
            bg_widget_hover: if dark {
                lift(widget, 0.08)
            } else {
                dim(widget, 0.08)
            },
            bg_widget_active: if dark {
                lift(widget, 0.16)
            } else {
                dim(widget, 0.14)
            },
            bg_extreme: extreme,
            bg_canvas: widget,
            fg,
            fg_weak: muted,
            fg_strong: if dark { Color32::WHITE } else { Color32::BLACK },
            border,
            border_strong: mix(border, fg, 0.18),
            select: get(m, &["cursor", "sapphire", "sky"]).unwrap_or(accent),
            select_fill: alpha(sel, 40),
            warn,
            error,
            dark,
        }
    }

    /// Catppuccin Mocha, only used when no desktop theme is on disk.
    fn fallback() -> Self {
        let mut m = HashMap::new();
        m.insert("mode".into(), "dark".into());
        m.insert("accent".into(), "#89b4fa".into());
        m.insert("selection".into(), "#45475a".into());
        m.insert("muted".into(), "#585b70".into());
        m.insert("background".into(), "#1e1e2e".into());
        m.insert("dark_background".into(), "#181825".into());
        m.insert("darker_background".into(), "#11111b".into());
        m.insert("lighter_background".into(), "#313244".into());
        m.insert("foreground".into(), "#cdd6f4".into());
        m.insert("dark_foreground".into(), "#6c7086".into());
        m.insert("red".into(), "#f38ba8".into());
        m.insert("yellow".into(), "#f9e2af".into());
        m.insert("blue".into(), "#89b4fa".into());
        Self::from_map(&m)
    }
}

fn fc_file_for(pattern: &str) -> Option<PathBuf> {
    let out = std::process::Command::new("fc-match")
        .args(["-f", "%{file}", pattern])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if p.is_empty() {
        return None;
    }
    let path = PathBuf::from(p);
    path.exists().then_some(path)
}

fn omarchy_font_name() -> Option<String> {
    let out = std::process::Command::new("omarchy")
        .args(["font", "current"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn load_ui_font_bytes() -> Option<(String, Vec<u8>)> {
    if let Ok(p) = std::env::var("OMADESIGN_FONT") {
        let path = PathBuf::from(p);
        if let Ok(b) = std::fs::read(&path) {
            return Some((
                path.file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "ui".into()),
                b,
            ));
        }
    }
    let named = omarchy_font_name();
    let patterns: Vec<String> = [
        named.clone(),
        named.map(|n| format!("{n}:style=Regular")),
        Some("sans-serif".into()),
        Some("sans".into()),
        Some("Noto Sans".into()),
        Some("Inter".into()),
        Some("JetBrainsMono Nerd Font".into()),
    ]
    .into_iter()
    .flatten()
    .collect();
    for pat in patterns {
        if let Some(path) = fc_file_for(&pat)
            && let Ok(b) = std::fs::read(&path)
            && path
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| matches!(s.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc"))
                .unwrap_or(false)
        {
            return Some(("ui".into(), b));
        }
    }
    None
}

pub fn apply(ctx: &Context) {
    let pal = Palette::load();
    let _ = PALETTE.set(pal.clone());

    let mut fonts = FontDefinitions::default();
    if let Some((name, bytes)) = load_ui_font_bytes() {
        fonts
            .font_data
            .insert(name.clone(), std::sync::Arc::new(FontData::from_owned(bytes)));
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, name.clone());
        fonts
            .families
            .entry(FontFamily::Monospace)
            .or_default()
            .insert(0, name);
    }
    fonts.font_data.insert(
        "phosphor".into(),
        std::sync::Arc::new(FontData::from_static(include_bytes!(
            "../../assets/phosphor/Phosphor-Light.ttf"
        ))),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(1, "phosphor".into());
    ctx.set_fonts(fonts);

    let r4 = CornerRadius::same(4);
    let widget = |bg: Color32| WidgetVisuals {
        bg_fill: bg,
        weak_bg_fill: bg,
        bg_stroke: Stroke::new(1.0, pal.border),
        corner_radius: r4,
        fg_stroke: Stroke::new(1.0, pal.fg),
        expansion: 0.0,
    };
    let widgets = Widgets {
        noninteractive: widget(pal.bg_widget),
        inactive: widget(pal.bg_widget),
        hovered: widget(pal.bg_widget_hover),
        active: widget(pal.bg_widget_active),
        open: widget(pal.bg_widget),
        ..Default::default()
    };
    let visuals = Visuals {
        dark_mode: pal.dark,
        override_text_color: Some(pal.fg),
        weak_text_alpha: 0.6,
        weak_text_color: Some(pal.fg_weak),
        widgets,
        selection: Selection {
            bg_fill: pal.accent_dim,
            stroke: Stroke::new(1.0, pal.accent),
        },
        hyperlink_color: pal.accent,
        faint_bg_color: pal.bg_widget,
        extreme_bg_color: pal.bg_extreme,
        text_edit_bg_color: Some(pal.bg_extreme),
        code_bg_color: pal.bg_widget,
        warn_fg_color: pal.warn,
        error_fg_color: pal.error,
        window_corner_radius: CornerRadius::same(8),
        window_shadow: Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: Color32::from_black_alpha(140),
        },
        window_fill: pal.bg_window,
        window_stroke: Stroke::new(1.0, pal.border_strong),
        window_highlight_topmost: false,
        menu_corner_radius: r4,
        panel_fill: pal.bg_panel,
        popup_shadow: Shadow {
            offset: [0, 4],
            blur: 12,
            spread: 0,
            color: Color32::from_black_alpha(120),
        },
        resize_corner_size: 12.0,
        ..Default::default()
    };
    if pal.dark {
        ctx.set_theme(eframe::egui::ThemePreference::Dark);
    } else {
        ctx.set_theme(eframe::egui::ThemePreference::Light);
    }
    ctx.set_visuals(visuals);

    let theme = if pal.dark { Theme::Dark } else { Theme::Light };
    ctx.style_mut_of(theme, |style| {
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
            FontId::new(16.0, FontFamily::Proportional),
        );
        style
            .text_styles
            .insert(TextStyle::Body, FontId::new(13.0, FontFamily::Proportional));
        style.text_styles.insert(
            TextStyle::Button,
            FontId::new(12.5, FontFamily::Proportional),
        );
        style
            .text_styles
            .insert(TextStyle::Small, FontId::new(11.0, FontFamily::Proportional));
        style.text_styles.insert(
            TextStyle::Monospace,
            FontId::new(12.0, FontFamily::Monospace),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mocha_fallback_is_dark() {
        let p = Palette::fallback();
        assert!(p.dark);
        assert!(lum(p.bg_window) < lum(p.fg));
    }

    #[test]
    fn parses_omarchy_toml() {
        let mut m = HashMap::new();
        m.insert("background".into(), "#010101".into());
        m.insert("foreground".into(), "#cdd6f4".into());
        m.insert("accent".into(), "#89b4fa".into());
        let p = Palette::from_map(&m);
        assert_eq!(p.bg_window, Color32::from_rgb(0x01, 0x01, 0x01));
        assert_eq!(p.accent, Color32::from_rgb(0x89, 0xB4, 0xFA));
        assert!(p.dark);
    }
}
