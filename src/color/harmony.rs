//! Hue relationships and editable color-guide variations retain straight alpha.
use super::Rgba;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Harmony {
    Complementary,
    ComplementarySoft,
    SplitComplementary,
    LeftComplement,
    RightComplement,
    Analogous,
    #[default]
    AnalogousFive,
    Monochromatic,
    Shades,
    Triad,
    TriadSoft,
    TriadDeep,
    Tetrad,
    Square,
    NarrowTetrad,
    Compound,
    CompoundLeft,
    HighContrast,
    HighContrastFour,
    HighContrastTriad,
    Pentagram,
}
impl Harmony {
    pub const ALL: [Self; 21] = [
        Self::Complementary,
        Self::ComplementarySoft,
        Self::SplitComplementary,
        Self::LeftComplement,
        Self::RightComplement,
        Self::Analogous,
        Self::AnalogousFive,
        Self::Monochromatic,
        Self::Shades,
        Self::Triad,
        Self::TriadSoft,
        Self::TriadDeep,
        Self::Tetrad,
        Self::Square,
        Self::NarrowTetrad,
        Self::Compound,
        Self::CompoundLeft,
        Self::HighContrast,
        Self::HighContrastFour,
        Self::HighContrastTriad,
        Self::Pentagram,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Complementary => "Complementary",
            Self::ComplementarySoft => "Complementary 2",
            Self::SplitComplementary => "Split complementary",
            Self::LeftComplement => "Left complement",
            Self::RightComplement => "Right complement",
            Self::Analogous => "Analogous",
            Self::AnalogousFive => "Analogous 2",
            Self::Monochromatic => "Monochromatic",
            Self::Shades => "Shades",
            Self::Triad => "Triad",
            Self::TriadSoft => "Triad 2",
            Self::TriadDeep => "Triad 3",
            Self::Tetrad => "Tetrad",
            Self::Square => "Tetrad 2",
            Self::NarrowTetrad => "Tetrad 3",
            Self::Compound => "Compound",
            Self::CompoundLeft => "Compound 2",
            Self::HighContrast => "High contrast",
            Self::HighContrastFour => "High contrast 2",
            Self::HighContrastTriad => "High contrast 3",
            Self::Pentagram => "Pentagram",
        }
    }

    /// Build the harmony from a straight RGBA base.
    /// Args: base color. Returns distinct rule positions with the exact base first.
    pub fn colors(self, base: Rgba) -> Vec<Rgba> {
        let turns: &[f32] = match self {
            Self::Complementary
            | Self::ComplementarySoft
            | Self::HighContrast
            | Self::HighContrastFour => &[0., 180.],
            Self::SplitComplementary => &[0., 150., 210.],
            Self::LeftComplement => &[0., -30., 150.],
            Self::RightComplement => &[0., 30., 210.],
            Self::Analogous => &[0., 30., -30.],
            Self::AnalogousFive => &[0., 30., 60., -30., -60.],
            Self::Triad | Self::TriadSoft | Self::TriadDeep | Self::HighContrastTriad => {
                &[0., 120., 240.]
            }
            Self::Tetrad => &[0., 60., 180., 240.],
            Self::Square => &[0., 90., 180., 270.],
            Self::NarrowTetrad => &[0., 30., 180., 210.],
            Self::Compound => &[0., 40., 160., 200.],
            Self::CompoundLeft => &[0., -40., 160., 200.],
            Self::Pentagram => &[0., 72., 144., 216., 288.],
            Self::Monochromatic | Self::Shades => &[0.],
        };
        let [h, s, v, a] = base.to_hsva();
        let mut colors = vec![base];
        if self == Self::Monochromatic {
            for (sat, value) in [(0.5, 1.), (1., 0.6), (0.25, 1.), (1., 0.35)] {
                colors.push(Rgba::from_hsva(h, s * sat, v * value, a));
            }
        } else if self == Self::Shades {
            for value in [0.8, 0.6, 0.4, 0.2] {
                colors.push(Rgba::from_hsva(h, s, v * value, a));
            }
        } else {
            for (index, turn) in turns.iter().enumerate() {
                let hue = h + turn / 360.;
                if index != 0 {
                    let value = if matches!(self, Self::HighContrast | Self::HighContrastTriad) {
                        if v >= 0.5 { v * 0.3 } else { 0.7 + v * 0.3 }
                    } else {
                        v
                    };
                    colors.push(Rgba::from_hsva(hue, s, value, a));
                }
                match self {
                    Self::ComplementarySoft => colors.push(Rgba::from_hsva(hue, s * 0.5, v, a)),
                    Self::TriadSoft if index != 0 => {
                        colors.push(Rgba::from_hsva(hue, s * 0.5, v, a))
                    }
                    Self::TriadDeep if index != 0 => {
                        colors.push(Rgba::from_hsva(hue, s, v * 0.5, a))
                    }
                    Self::HighContrastFour => colors.push(Rgba::from_hsva(
                        hue,
                        s,
                        if v >= 0.5 { v * 0.3 } else { 0.7 + v * 0.3 },
                        a,
                    )),
                    _ => {}
                }
            }
        }
        colors
    }

    /// Create five linked theme swatches from this rule.
    /// Args: base color. Returns five colors, preserving the base and alpha.
    pub fn theme(self, base: Rgba) -> Vec<Rgba> {
        let mut colors = self.colors(base);
        let count = colors.len();
        for i in count..5 {
            colors.push(Variation::TintsShades.color(
                colors[(i - count) % count],
                if i % 2 == 0 { 0.4 } else { -0.4 },
            ));
        }
        colors.truncate(5);
        colors
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Variation {
    #[default]
    TintsShades,
    WarmCool,
    VividMuted,
}
impl Variation {
    pub const ALL: [Self; 3] = [Self::TintsShades, Self::WarmCool, Self::VividMuted];
    pub fn name(self) -> &'static str {
        match self {
            Self::TintsShades => "Tints / shades",
            Self::WarmCool => "Warm / cool",
            Self::VividMuted => "Vivid / muted",
        }
    }

    /// Vary a guide swatch without changing its alpha.
    /// Args: base and signed strength from minus one to one. Returns a straight RGBA color.
    pub fn color(self, base: Rgba, amount: f32) -> Rgba {
        let strength = amount.clamp(-1., 1.);
        if strength == 0. {
            return base;
        }
        if self == Self::VividMuted {
            let [h, s, v, a] = base.to_hsva();
            return Rgba::from_hsva(
                h,
                if strength < 0. {
                    s * (1. + strength)
                } else {
                    s + (1. - s) * strength
                },
                v,
                a,
            );
        }
        let (target, fraction) = match self {
            Self::WarmCool => (
                if strength < 0. {
                    Rgba::rgb(51, 115, 255)
                } else {
                    Rgba::rgb(255, 140, 26)
                },
                strength.abs() * 0.6,
            ),
            _ => (
                if strength < 0. {
                    Rgba::BLACK
                } else {
                    Rgba::WHITE
                },
                strength.abs(),
            ),
        };
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * fraction).round() as u8;
        Rgba::new(
            mix(base.r, target.r),
            mix(base.g, target.g),
            mix(base.b, target.b),
            base.a,
        )
    }
}

/// Move one theme color on the hue wheel, optionally retaining every color's relation.
/// Args: colors, edited index, replacement and link state. Returns nothing; alpha stays per color.
pub fn move_color(colors: &mut [Rgba], index: usize, replacement: Rgba, linked: bool) {
    let Some(before) = colors.get(index).copied() else {
        return;
    };
    let [h, s, v, _] = before.to_hsva();
    let [next_h, next_s, next_v, _] = replacement.to_hsva();
    for (i, color) in colors.iter_mut().enumerate() {
        if i == index {
            *color = replacement;
        } else if linked {
            let [ch, cs, cv, ca] = color.to_hsva();
            *color = Rgba::from_hsva(
                ch + next_h - h,
                if s > 0.01 {
                    cs * next_s / s
                } else {
                    cs + next_s - s
                },
                cv + next_v - v,
                ca,
            );
        }
    }
}

/// Choose a swatch by weighted distance in linear sRGB, leaving alpha intact.
/// Args: color and available library colors. Returns the input if the library is empty.
pub fn nearest(color: Rgba, palette: &[Rgba]) -> Rgba {
    let linear = |c: u8| {
        let v = c as f32 / 255.;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    palette
        .iter()
        .min_by(|a, b| {
            let distance = |p: &&Rgba| {
                [0.2126, 0.7152, 0.0722]
                    .into_iter()
                    .zip([(color.r, p.r), (color.g, p.g), (color.b, p.b)])
                    .map(|(w, (x, y))| w * (linear(x) - linear(y)).powi(2))
                    .sum::<f32>()
            };
            distance(a).total_cmp(&distance(b))
        })
        .map_or(color, |p| Rgba::new(p.r, p.g, p.b, color.a))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn craft_color_harmonies_preserve_base_alpha_and_known_wheel_relationships() {
        let base = Rgba::new(255, 0, 0, 117);
        assert_eq!(
            Harmony::Complementary.colors(base),
            vec![base, Rgba::new(0, 255, 255, 117)]
        );
        assert_eq!(
            Harmony::Triad.colors(base),
            vec![base, Rgba::new(0, 255, 0, 117), Rgba::new(0, 0, 255, 117)]
        );
        for rule in Harmony::ALL {
            for base in [base, Rgba::new(0, 0, 0, 0), Rgba::new(127, 127, 127, 215)] {
                let colors = rule.colors(base);
                assert_eq!(colors[0], base);
                assert!(colors.iter().all(|c| c.a == base.a));
                let theme = rule.theme(base);
                assert_eq!(theme.len(), 5);
                assert_eq!(theme[0], base);
                assert!(theme.iter().all(|c| c.a == base.a));
            }
        }
        assert_eq!(
            Variation::TintsShades.color(base, -1.),
            Rgba::new(0, 0, 0, 117)
        );
        assert_eq!(
            Variation::TintsShades.color(base, 1.),
            Rgba::new(255, 255, 255, 117)
        );
        for variation in Variation::ALL {
            assert_eq!(variation.color(base, 0.), base);
        }
    }

    #[test]
    fn craft_color_theme_linked_and_custom_edits_keep_alpha_and_library_limits() {
        let mut colors = vec![
            Rgba::new(255, 0, 0, 117),
            Rgba::new(0, 255, 0, 91),
            Rgba::new(0, 0, 255, 211),
        ];
        let old = colors.clone();
        move_color(&mut colors, 0, Rgba::new(255, 255, 0, 117), true);
        assert_eq!(
            colors,
            vec![
                Rgba::new(255, 255, 0, 117),
                Rgba::new(0, 255, 255, 91),
                Rgba::new(255, 0, 255, 211)
            ]
        );
        colors = old.clone();
        move_color(&mut colors, 1, Rgba::WHITE, false);
        assert_eq!(colors, vec![old[0], Rgba::WHITE, old[2]]);
        let library = [Rgba::rgb(255, 0, 0), Rgba::rgb(0, 255, 0)];
        assert_eq!(
            nearest(Rgba::new(240, 10, 0, 23), &library),
            Rgba::new(255, 0, 0, 23)
        );
        assert_eq!(nearest(old[0], &[]), old[0]);
    }
}
