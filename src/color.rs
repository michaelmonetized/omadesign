//! Colors, blend modes, and a starter swatch book.

use serde::{Deserialize, Serialize};

/// Convert stored straight RGBA bytes to tiny-skia's premultiplied pixels.
pub(crate) fn rgba_to_pixmap(w: u32, h: u32, data: &[u8]) -> Option<tiny_skia::Pixmap> {
    let len = (w as usize).checked_mul(h as usize)?.checked_mul(4)?;
    if data.len() != len {
        return None;
    }
    let mut pm = tiny_skia::Pixmap::new(w, h)?;
    for (dst, src) in pm.pixels_mut().iter_mut().zip(data.chunks_exact(4)) {
        *dst = tiny_skia::ColorU8::from_rgba(src[0], src[1], src[2], src[3]).premultiply();
    }
    Some(pm)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const TRANSPARENT: Rgba = Rgba {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    pub const BLACK: Rgba = Rgba {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const WHITE: Rgba = Rgba {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub fn from_hex(hex: u32) -> Self {
        Self::rgb(
            ((hex >> 16) & 0xff) as u8,
            ((hex >> 8) & 0xff) as u8,
            (hex & 0xff) as u8,
        )
    }

    pub fn to_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    pub fn from_array(a: [u8; 4]) -> Self {
        Self {
            r: a[0],
            g: a[1],
            b: a[2],
            a: a[3],
        }
    }

    pub fn to_egui(self) -> eframe::egui::Color32 {
        eframe::egui::Color32::from_rgba_unmultiplied(self.r, self.g, self.b, self.a)
    }

    pub fn from_egui(c: eframe::egui::Color32) -> Self {
        Self::from_array(c.to_srgba_unmultiplied())
    }

    pub fn to_skia(self) -> tiny_skia::Color {
        tiny_skia::Color::from_rgba8(self.r, self.g, self.b, self.a)
    }

    pub fn hex(self) -> String {
        if self.a == 255 {
            format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
        } else {
            format!("#{:02X}{:02X}{:02X}{:02X}", self.r, self.g, self.b, self.a)
        }
    }

    pub fn parse_hex(s: &str) -> Option<Self> {
        let t = s.trim().trim_start_matches('#');
        match t.len() {
            6 => {
                let n = u32::from_str_radix(t, 16).ok()?;
                Some(Self::from_hex(n))
            }
            8 => {
                let n = u32::from_str_radix(t, 16).ok()?;
                Some(Self::new(
                    ((n >> 24) & 0xff) as u8,
                    ((n >> 16) & 0xff) as u8,
                    ((n >> 8) & 0xff) as u8,
                    (n & 0xff) as u8,
                ))
            }
            3 => {
                let n = u32::from_str_radix(t, 16).ok()?;
                Some(Self::rgb(
                    (((n >> 8) & 0xf) * 0x11) as u8,
                    (((n >> 4) & 0xf) * 0x11) as u8,
                    ((n & 0xf) * 0x11) as u8,
                ))
            }
            _ => None,
        }
    }

    pub fn to_hsva(self) -> [f32; 4] {
        let r = self.r as f32 / 255.0;
        let g = self.g as f32 / 255.0;
        let b = self.b as f32 / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let d = max - min;
        let h = if d < 1e-6 {
            0.0
        } else if max == r {
            ((g - b) / d).rem_euclid(6.0) / 6.0
        } else if max == g {
            (((b - r) / d) + 2.0) / 6.0
        } else {
            (((r - g) / d) + 4.0) / 6.0
        };
        let s = if max < 1e-6 { 0.0 } else { d / max };
        [h, s, max, self.a as f32 / 255.0]
    }

    pub fn from_hsva(h: f32, s: f32, v: f32, a: f32) -> Self {
        let h = h.rem_euclid(1.0) * 6.0;
        let s = s.clamp(0.0, 1.0);
        let v = v.clamp(0.0, 1.0);
        let c = v * s;
        let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
        let m = v - c;
        let (r, g, b) = match h as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        Self::new(
            ((r + m) * 255.0).round() as u8,
            ((g + m) * 255.0).round() as u8,
            ((b + m) * 255.0).round() as u8,
            (a.clamp(0.0, 1.0) * 255.0).round() as u8,
        )
    }

    pub fn css(self) -> String {
        format!(
            "rgba({},{},{},{:.3})",
            self.r,
            self.g,
            self.b,
            self.a as f32 / 255.0
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Blend {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl Blend {
    pub const ALL: [Blend; 16] = [
        Blend::Normal,
        Blend::Multiply,
        Blend::Screen,
        Blend::Overlay,
        Blend::Darken,
        Blend::Lighten,
        Blend::ColorDodge,
        Blend::ColorBurn,
        Blend::HardLight,
        Blend::SoftLight,
        Blend::Difference,
        Blend::Exclusion,
        Blend::Hue,
        Blend::Saturation,
        Blend::Color,
        Blend::Luminosity,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Blend::Normal => "Normal",
            Blend::Multiply => "Multiply",
            Blend::Screen => "Screen",
            Blend::Overlay => "Overlay",
            Blend::Darken => "Darken",
            Blend::Lighten => "Lighten",
            Blend::ColorDodge => "Color Dodge",
            Blend::ColorBurn => "Color Burn",
            Blend::HardLight => "Hard Light",
            Blend::SoftLight => "Soft Light",
            Blend::Difference => "Difference",
            Blend::Exclusion => "Exclusion",
            Blend::Hue => "Hue",
            Blend::Saturation => "Saturation",
            Blend::Color => "Color",
            Blend::Luminosity => "Luminosity",
        }
    }

    pub fn to_skia(self) -> tiny_skia::BlendMode {
        use tiny_skia::BlendMode as B;
        match self {
            Blend::Normal => B::SourceOver,
            Blend::Multiply => B::Multiply,
            Blend::Screen => B::Screen,
            Blend::Overlay => B::Overlay,
            Blend::Darken => B::Darken,
            Blend::Lighten => B::Lighten,
            Blend::ColorDodge => B::ColorDodge,
            Blend::ColorBurn => B::ColorBurn,
            Blend::HardLight => B::HardLight,
            Blend::SoftLight => B::SoftLight,
            Blend::Difference => B::Difference,
            Blend::Exclusion => B::Exclusion,
            Blend::Hue => B::Hue,
            Blend::Saturation => B::Saturation,
            Blend::Color => B::Color,
            Blend::Luminosity => B::Luminosity,
        }
    }

    pub fn css(self) -> &'static str {
        match self {
            Blend::Normal => "normal",
            Blend::Multiply => "multiply",
            Blend::Screen => "screen",
            Blend::Overlay => "overlay",
            Blend::Darken => "darken",
            Blend::Lighten => "lighten",
            Blend::ColorDodge => "color-dodge",
            Blend::ColorBurn => "color-burn",
            Blend::HardLight => "hard-light",
            Blend::SoftLight => "soft-light",
            Blend::Difference => "difference",
            Blend::Exclusion => "exclusion",
            Blend::Hue => "hue",
            Blend::Saturation => "saturation",
            Blend::Color => "color",
            Blend::Luminosity => "luminosity",
        }
    }

    pub fn parse(s: &str) -> Self {
        Self::ALL
            .iter()
            .copied()
            .find(|b| b.name().eq_ignore_ascii_case(s) || b.css().eq_ignore_ascii_case(s))
            .unwrap_or(Blend::Normal)
    }
}

pub fn default_swatches() -> Vec<Rgba> {
    const HEX: &[u32] = &[
        0x000000, 0x1B1B1B, 0x3D3D3D, 0x6B6B6B, 0x9E9E9E, 0xC8C8C8, 0xEDEDED, 0xFFFFFF, 0x7A1F1F,
        0xC62828, 0xE53935, 0xEF9A9A, 0x7A3A12, 0xEF6C00, 0xFB8C00, 0xFFCC80, 0x7A6A10, 0xF9A825,
        0xFDD835, 0xFFF59D, 0x1B5E20, 0x2E7D32, 0x43A047, 0xA5D6A7, 0x004D40, 0x00897B, 0x26A69A,
        0x80CBC4, 0x0D47A1, 0x1565C0, 0x1E88E5, 0x90CAF9, 0x4A148C, 0x6A1B9A, 0x8E24AA, 0xCE93D8,
        0x880E4F, 0xC2185B, 0xEC407A, 0xF48FB1, 0xF47C2E, 0xE5484D, 0x4F8CFF, 0x2EC4B6, 0xFFD166,
        0x06D6A0, 0x118AB2, 0x073B4C,
    ];
    HEX.iter().copied().map(Rgba::from_hex).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let c = Rgba::from_hex(0xF47C2E);
        assert_eq!(c.hex(), "#F47C2E");
        assert_eq!(Rgba::parse_hex("#f47c2e"), Some(c));
        assert_eq!(Rgba::parse_hex("F47"), Some(Rgba::rgb(0xFF, 0x44, 0x77)));
        let translucent = Rgba::new(255, 0, 0, 128);
        assert_eq!(Rgba::from_egui(translucent.to_egui()), translucent);
    }

    #[test]
    fn hsv_roundtrip_primary() {
        let c = Rgba::rgb(255, 0, 0);
        let [h, s, v, _] = c.to_hsva();
        let back = Rgba::from_hsva(h, s, v, 1.0);
        assert!((back.r as i16 - 255).abs() <= 1);
        assert!(back.g <= 2);
        assert!(back.b <= 2);
    }
}
