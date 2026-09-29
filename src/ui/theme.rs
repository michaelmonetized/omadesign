//! UI chrome follows the desktop: Omarchy theme + fontconfig. No baked-in brand hex.

use eframe::egui::{
    Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Shadow, Stroke,
    TextStyle, Theme, Vec2, Visuals,
    style::{Selection, WidgetVisuals, Widgets},
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant, SystemTime};

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub accent: Color32,
    pub blue: Color32,
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

/// Shared AI affordance colors, with enough contrast in either appearance.
pub fn agent_gradient(dark: bool) -> [Color32; 2] {
    if dark {
        [
            Color32::from_rgb(245, 194, 231),
            Color32::from_rgb(203, 166, 247),
        ]
    } else {
        [
            Color32::from_rgb(234, 118, 203),
            Color32::from_rgb(136, 57, 239),
        ]
    }
}

struct LiveTheme {
    palette: Palette,
    font: String,
    colors_key: Option<u128>,
    name_key: Option<u128>,
    warned: bool,
    poll: ThemePoll,
}

type UiFontBytes = (String, Vec<u8>);

#[derive(Clone, Debug, PartialEq, Eq)]
struct ThemeSnapshot {
    colors_key: Option<u128>,
    name_key: Option<u128>,
    font: String,
}

#[derive(Debug)]
struct ThemeUpdate {
    snapshot: ThemeSnapshot,
    files_changed: bool,
    changed: bool,
    palette: Option<Palette>,
    font_bytes: Option<UiFontBytes>,
}

#[derive(Default)]
struct ThemePoll {
    pending: Option<mpsc::Receiver<ThemeUpdate>>,
}

impl ThemePoll {
    fn start(
        &mut self,
        ctx: Context,
        lookup: impl FnOnce() -> ThemeUpdate + Send + 'static,
    ) -> bool {
        if self.pending.is_some() {
            return false;
        }
        let (sender, receiver) = mpsc::channel();
        self.pending = Some(receiver);
        let started = std::thread::Builder::new()
            .name("omadesign-theme".into())
            .spawn(move || {
                let update = lookup();
                let changed = update.changed;
                if sender.send(update).is_ok() && changed {
                    ctx.request_repaint();
                }
            });
        if started.is_err() {
            self.pending = None;
            return false;
        }
        true
    }

    fn completed(&mut self) -> Option<ThemeUpdate> {
        match self.pending.as_ref()?.try_recv() {
            Ok(update) => {
                self.pending = None;
                Some(update)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.pending = None;
                None
            }
        }
    }
}

impl LiveTheme {
    fn snapshot(&self) -> ThemeSnapshot {
        ThemeSnapshot {
            colors_key: self.colors_key,
            name_key: self.name_key,
            font: self.font.clone(),
        }
    }

    fn accept(&mut self, update: ThemeUpdate) -> Option<Option<UiFontBytes>> {
        if update.files_changed {
            if let Some(palette) = update.palette {
                self.palette = palette;
                self.warned = false;
            } else if !self.warned {
                eprintln!(
                    "omadesign: Omarchy theme colors could not be read; keeping the current palette"
                );
                self.warned = true;
            }
        }
        self.colors_key = update.snapshot.colors_key;
        self.name_key = update.snapshot.name_key;
        self.font = update.snapshot.font;
        update.changed.then_some(update.font_bytes)
    }
}

fn read_theme(previous: ThemeSnapshot) -> ThemeUpdate {
    let snapshot = ThemeSnapshot {
        colors_key: file_key(&omarchy_colors_path()),
        name_key: file_key(&omarchy_name_path()),
        font: omarchy_font_name().unwrap_or_default(),
    };
    prepare_theme_update(
        previous,
        snapshot,
        || palette_from_file(&omarchy_colors_path()),
        |font| load_ui_font_bytes_named(|| (!font.is_empty()).then(|| font.to_owned())),
    )
}

fn prepare_theme_update(
    previous: ThemeSnapshot,
    snapshot: ThemeSnapshot,
    palette: impl FnOnce() -> Option<Palette>,
    font: impl FnOnce(&str) -> Option<UiFontBytes>,
) -> ThemeUpdate {
    let files_changed =
        snapshot.colors_key != previous.colors_key || snapshot.name_key != previous.name_key;
    let changed = snapshot != previous;
    let palette = files_changed.then(palette).flatten();
    let font_bytes = changed.then(|| font(&snapshot.font)).flatten();
    ThemeUpdate {
        snapshot,
        files_changed,
        changed,
        palette,
        font_bytes,
    }
}

static LIVE: OnceLock<Mutex<LiveTheme>> = OnceLock::new();

fn live() -> &'static Mutex<LiveTheme> {
    LIVE.get_or_init(|| {
        Mutex::new(LiveTheme {
            palette: Palette::load(),
            font: omarchy_font_name().unwrap_or_default(),
            colors_key: file_key(&omarchy_colors_path()),
            name_key: file_key(&omarchy_name_path()),
            warned: false,
            poll: ThemePoll::default(),
        })
    })
}

pub fn p() -> Palette {
    live()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .palette
}

/// Re-read the Omarchy theme after it changes on disk. A bad file keeps the last good palette.
pub fn poll(ctx: &Context) {
    // Consume a worker's result even when its repaint arrives before the next
    // 400ms check. No command or font-file read runs on this UI path.
    let ready = {
        let mut state = live().lock().unwrap_or_else(|poison| poison.into_inner());
        poll_theme(&mut state, ctx, Instant::now(), read_theme)
    };
    if let Some(font_bytes) = ready {
        apply_prepared(ctx, font_bytes);
    }
}

fn poll_theme(
    state: &mut LiveTheme,
    ctx: &Context,
    now: Instant,
    lookup: impl FnOnce(ThemeSnapshot) -> ThemeUpdate + Send + 'static,
) -> Option<Option<UiFontBytes>> {
    let ready = state
        .poll
        .completed()
        .and_then(|update| state.accept(update));
    let id = eframe::egui::Id::new("omarchy-theme-poll");
    if let Some(then) = ctx.data(|data| data.get_temp::<Instant>(id)) {
        let remaining =
            Duration::from_millis(400).saturating_sub(now.saturating_duration_since(then));
        if !remaining.is_zero() {
            // A worker's immediate repaint can replace the previous timer.
            // Re-arm the deadline so watching continues while the editor idles.
            ctx.request_repaint_after(remaining);
            return ready;
        }
    }
    ctx.data_mut(|data| data.insert_temp(id, now));
    ctx.request_repaint_after(Duration::from_millis(500));
    let previous = state.snapshot();
    state.poll.start(ctx.clone(), move || lookup(previous));
    ready
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

fn omarchy_colors_path() -> PathBuf {
    home().join(".local/state/omarchy/current/theme/colors.toml")
}

fn omarchy_name_path() -> PathBuf {
    home().join(".local/state/omarchy/current/theme.name")
}

fn file_key(path: &Path) -> Option<u128> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let nanos = modified
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    Some(nanos ^ u128::from(meta.len()))
}

fn palette_from_toml(text: &str) -> Option<Palette> {
    let map = parse_kv(text);
    let known = get(
        &map,
        &[
            "background",
            "base",
            "bg",
            "foreground",
            "text",
            "fg",
            "accent",
        ],
    )
    .is_some();
    known.then(|| Palette::from_map(&map))
}

fn palette_from_file(path: &Path) -> Option<Palette> {
    let text = std::fs::read_to_string(path).ok()?;
    palette_from_toml(&text)
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
        let bg =
            get(m, &["background", "base", "bg"]).unwrap_or(Color32::from_rgb(0x1E, 0x1E, 0x2E));
        let fg =
            get(m, &["foreground", "text", "fg"]).unwrap_or(Color32::from_rgb(0xCD, 0xD6, 0xF4));
        let accent = get(m, &["accent", "blue", "color4", "primary"])
            .unwrap_or(Color32::from_rgb(0x89, 0xB4, 0xFA));
        let muted = mix(fg, bg, 0.38);
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
        let warn =
            get(m, &["yellow", "orange", "color3"]).unwrap_or(Color32::from_rgb(0xF9, 0xE2, 0xAF));
        // Keep surfaces close to the desktop background; contrast belongs to the artwork.
        let panel = mix(bg, panel, 0.45);
        let widget = mix(bg, widget, 0.55);
        let border = mix(panel, fg, 0.10);
        Self {
            accent,
            blue: get(m, &["blue", "color4"]).unwrap_or(Color32::from_rgb(0x89, 0xB4, 0xFA)),
            accent_dim: dim(accent, 0.28),
            accent_soft: alpha(accent, 25),
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
            bg_canvas: mix(bg, extreme, 0.6),
            fg,
            fg_weak: muted,
            fg_strong: mix(fg, if dark { Color32::WHITE } else { Color32::BLACK }, 0.25),
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

fn load_ui_font_bytes() -> Option<UiFontBytes> {
    load_ui_font_bytes_named(omarchy_font_name)
}

fn load_ui_font_bytes_named(named: impl FnOnce() -> Option<String>) -> Option<UiFontBytes> {
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
    let named = named();
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
    apply_preferences(ctx, "");
}

pub fn apply_preferences(ctx: &Context, preferred: &str) {
    let selected = if preferred.is_empty() {
        None
    } else {
        let path = Path::new(preferred);
        let path = if path.is_file() {
            Some(path.to_path_buf())
        } else {
            fc_file_for(preferred)
        };
        path.filter(|p| std::fs::metadata(p).is_ok_and(|m| m.len() <= 64 * 1024 * 1024))
            .and_then(|p| std::fs::read(p).ok())
            .filter(|bytes| ab_glyph::FontArc::try_from_vec(bytes.clone()).is_ok())
            .map(|bytes| ("preferred-ui".to_string(), bytes))
    };
    apply_prepared(ctx, selected.or_else(load_ui_font_bytes));
}

fn apply_prepared(ctx: &Context, font_bytes: Option<UiFontBytes>) {
    let pal = p();
    let mut fonts = FontDefinitions::default();
    if let Some((name, bytes)) = font_bytes {
        fonts.font_data.insert(
            name.clone(),
            std::sync::Arc::new(FontData::from_owned(bytes)),
        );
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
        .insert(FontFamily::Name("phosphor".into()), vec!["phosphor".into()]);
    ctx.set_fonts(fonts);

    let radius = CornerRadius::same(6);
    let widget = |bg: Color32| WidgetVisuals {
        bg_fill: bg,
        weak_bg_fill: bg,
        bg_stroke: Stroke::NONE,
        corner_radius: radius,
        fg_stroke: Stroke::new(1.0, pal.fg),
        expansion: 0.0,
    };
    let widgets = Widgets {
        noninteractive: WidgetVisuals {
            bg_stroke: Stroke::new(1.0, pal.border),
            ..widget(pal.bg_widget)
        },
        inactive: widget(pal.bg_widget),
        hovered: widget(pal.bg_widget_hover),
        active: widget(pal.bg_widget_active),
        open: widget(pal.bg_widget),
    };
    let visuals = Visuals {
        dark_mode: pal.dark,
        override_text_color: Some(pal.fg),
        weak_text_alpha: 0.6,
        weak_text_color: Some(pal.fg_weak),
        widgets,
        selection: Selection {
            bg_fill: pal.accent_soft,
            stroke: Stroke::new(1.0, pal.accent),
        },
        hyperlink_color: pal.accent,
        faint_bg_color: pal.bg_widget,
        extreme_bg_color: pal.bg_extreme,
        text_edit_bg_color: Some(pal.bg_extreme),
        code_bg_color: pal.bg_widget,
        warn_fg_color: pal.warn,
        error_fg_color: pal.error,
        window_corner_radius: CornerRadius::same(10),
        window_shadow: Shadow {
            offset: [0, 6],
            blur: 18,
            spread: 0,
            color: Color32::from_black_alpha(80),
        },
        window_fill: pal.bg_window,
        window_stroke: Stroke::new(1.0, pal.border),
        window_highlight_topmost: false,
        menu_corner_radius: radius,
        panel_fill: pal.bg_panel,
        popup_shadow: Shadow {
            offset: [0, 4],
            blur: 12,
            spread: 0,
            color: Color32::from_black_alpha(64),
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
    ctx.options_mut(|o| {
        o.zoom_with_keyboard = false;
        o.input_options.zoom_modifier = eframe::egui::Modifiers::COMMAND
            | eframe::egui::Modifiers::CTRL
            | eframe::egui::Modifiers::ALT;
    });
    crate::compositor::set_canvas_bg(pal.bg_canvas.r(), pal.bg_canvas.g(), pal.bg_canvas.b());

    let theme = if pal.dark { Theme::Dark } else { Theme::Light };
    ctx.style_mut_of(theme, |style| {
        style.spacing.item_spacing = Vec2::new(6.0, 6.0);
        style.animation_time = 0.10;
        style.spacing.button_padding = Vec2::new(9.0, 5.0);
        style.spacing.icon_width = 14.0;
        style.spacing.slider_width = 132.0;
        style.spacing.slider_rail_height = 3.0;
        style.spacing.interact_size = Vec2::new(26.0, 26.0);
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
        style.text_styles.insert(
            TextStyle::Small,
            FontId::new(11.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Monospace,
            FontId::new(12.0, FontFamily::Monospace),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme_state() -> LiveTheme {
        LiveTheme {
            palette: Palette::fallback(),
            font: "Original".into(),
            colors_key: Some(1),
            name_key: Some(1),
            warned: false,
            poll: ThemePoll::default(),
        }
    }

    fn settle_repaints(ctx: &Context) {
        for _ in 0..3 {
            ctx.begin_pass(Default::default());
            ctx.end_pass().textures_delta.clear();
        }
    }

    #[test]
    fn theme_lookup_is_bounded_and_completion_rearms_idle_watching() {
        let ctx = Context::default();
        settle_repaints(&ctx);
        let (repaint_tx, repaint_rx) = mpsc::channel();
        ctx.set_request_repaint_callback(move |info| {
            let _ = repaint_tx.send(info.delay);
        });
        let mut state = theme_state();
        let original = state.snapshot();
        let now = Instant::now();
        let caller = std::thread::current().id();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        assert!(
            poll_theme(&mut state, &ctx, now, move |previous| {
                entered_tx.send(std::thread::current().id()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                let mut next = previous.clone();
                next.font = "Updated".into();
                prepare_theme_update(
                    previous,
                    next,
                    || panic!("unchanged palette"),
                    |name| {
                        assert_eq!(name, "Updated");
                        Some(("ui".into(), vec![1, 2, 3]))
                    },
                )
            })
            .is_none()
        );
        assert_ne!(
            entered_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            caller
        );
        assert!(!state.poll.start(ctx.clone(), || panic!("duplicate job")));
        assert!(state.poll.completed().is_none());
        assert_eq!(state.snapshot(), original);
        // Discard the periodic timer, then observe the worker's immediate wake.
        while repaint_rx.try_recv().is_ok() {}
        release_tx.send(()).unwrap();
        assert_eq!(
            repaint_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            Duration::ZERO
        );
        let ready = poll_theme(&mut state, &ctx, now + Duration::from_millis(20), |_| {
            panic!("poll interval has not elapsed")
        });
        assert_eq!(ready, Some(Some(("ui".into(), vec![1, 2, 3]))));
        assert_eq!(state.font, "Updated");
        assert!(state.poll.pending.is_none());
        // Egui settles an immediate wake over two passes. The subsequent idle
        // pass must still schedule a future poll, even though it is not due.
        settle_repaints(&ctx);
        while repaint_rx.try_recv().is_ok() {}
        assert!(
            poll_theme(&mut state, &ctx, now + Duration::from_millis(30), |_| {
                panic!("poll interval has not elapsed")
            })
            .is_none()
        );
        let delay = repaint_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(delay > Duration::ZERO && delay <= Duration::from_millis(370));
    }

    #[test]
    fn unchanged_theme_neither_loads_fonts_nor_replaces_font_definitions() {
        let mut state = theme_state();
        let previous = state.snapshot();
        let update = prepare_theme_update(
            previous.clone(),
            previous.clone(),
            || panic!("unchanged palette must not be loaded"),
            |_| panic!("unchanged font must not be loaded"),
        );
        assert!(!update.changed);
        let (sender, receiver) = mpsc::channel();
        state.poll.pending = Some(receiver);
        sender.send(update).unwrap();
        let ctx = Context::default();
        let now = Instant::now();
        ctx.data_mut(|data| data.insert_temp(eframe::egui::Id::new("omarchy-theme-poll"), now));
        assert!(poll_theme(&mut state, &ctx, now, |_| panic!("not due")).is_none());
        assert_eq!(state.snapshot(), previous);
        assert!(state.poll.pending.is_none());
    }

    #[test]
    fn changed_palette_keeps_last_good_colors_and_existing_font_fallback_semantics() {
        let mut state = theme_state();
        let color = state.palette.bg_window;
        let previous = state.snapshot();
        let mut next = previous.clone();
        next.colors_key = Some(2);
        let update = prepare_theme_update(previous, next, || None, |_| None);
        assert_eq!(state.accept(update), Some(None));
        assert_eq!(state.palette.bg_window, color);
        assert!(state.warned);
        let previous = state.snapshot();
        let mut next = previous.clone();
        next.colors_key = Some(3);
        let mut palette = state.palette;
        palette.bg_window = Color32::RED;
        let update = prepare_theme_update(
            previous,
            next,
            || Some(palette),
            |_| Some(("ui".into(), vec![4, 5])),
        );
        assert_eq!(state.accept(update), Some(Some(("ui".into(), vec![4, 5]))));
        assert_eq!(state.palette.bg_window, Color32::RED);
        assert!(!state.warned);
    }

    #[test]
    fn failed_worker_releases_the_in_flight_slot() {
        let (sender, receiver) = mpsc::channel();
        let mut poll = ThemePoll {
            pending: Some(receiver),
        };
        drop(sender);
        assert!(poll.completed().is_none());
        assert!(poll.pending.is_none());
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

    #[test]
    fn reloads_palette_from_a_changed_colors_file() {
        let dir = std::env::temp_dir().join(format!(
            "omadesign-theme-{}-{}",
            std::process::id(),
            crate::document::next_id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("colors.toml");
        std::fs::write(
            &path,
            "background = \"#010101\"\nforeground = \"#eeeeee\"\naccent = \"#89b4fa\"\n",
        )
        .unwrap();
        let first = palette_from_file(&path).unwrap();
        assert_eq!(first.bg_window, Color32::from_rgb(0x01, 0x01, 0x01));
        std::fs::write(
            &path,
            "background = \"#fafafa\"\nforeground = \"#111111\"\naccent = \"#cc0000\"\n",
        )
        .unwrap();
        let second = palette_from_file(&path).unwrap();
        assert_eq!(second.bg_window, Color32::from_rgb(0xfa, 0xfa, 0xfa));
        assert!(!second.dark);
        std::fs::write(&path, "this is not a theme\n").unwrap();
        assert!(palette_from_file(&path).is_none());
        assert!(palette_from_file(&dir.join("missing.toml")).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
