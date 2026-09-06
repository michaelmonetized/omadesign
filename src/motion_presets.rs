//! Small, editable motion recipes. Presets write ordinary animation channels;
//! there is no hidden preset runtime or dependency on the inspector staying open.
use crate::document::Shape;
use crate::geom::Bounds;
use crate::motion::{Ease, Key, Motion, Prop, Track};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    DrawStroke,
    PopIn,
    Slam,
    Shake,
    FillUp,
    SlideUp,
    SlideDown,
    SlideLeft,
    SlideRight,
    Fly,
    Zoom,
    Buzz,
    Fade,
}
impl Preset {
    pub const ALL: [Self; 13] = [
        Self::DrawStroke,
        Self::PopIn,
        Self::Slam,
        Self::Shake,
        Self::FillUp,
        Self::SlideUp,
        Self::SlideDown,
        Self::SlideLeft,
        Self::SlideRight,
        Self::Fly,
        Self::Zoom,
        Self::Buzz,
        Self::Fade,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::DrawStroke => "Draw stroke",
            Self::PopIn => "Pop in",
            Self::Slam => "Slam",
            Self::Shake => "Shake",
            Self::FillUp => "Fill up",
            Self::SlideUp => "Slide up",
            Self::SlideDown => "Slide down",
            Self::SlideLeft => "Slide left",
            Self::SlideRight => "Slide right",
            Self::Fly => "Fly",
            Self::Zoom => "Zoom",
            Self::Buzz => "Buzz",
            Self::Fade => "Fade in",
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            Self::DrawStroke => "Trace the existing outline along its path.",
            Self::PopIn => "Spring into place with a small overshoot.",
            Self::Slam => "Drop in hard, then settle.",
            Self::Shake => "A short, broad side-to-side shake.",
            Self::FillUp => "Reveal the fill from the bottom upward.",
            Self::SlideUp => "Enter from below.",
            Self::SlideDown => "Enter from above.",
            Self::SlideLeft => "Enter from the right.",
            Self::SlideRight => "Enter from the left.",
            Self::Fly => "Sweep in diagonally with a turn.",
            Self::Zoom => "Zoom down into place.",
            Self::Buzz => "A quick, tight vibration.",
            Self::Fade => "Fade gently into view.",
        }
    }
    pub fn supports(self, shape: &Shape) -> bool {
        match self {
            Self::DrawStroke => shape
                .style
                .stroke
                .as_ref()
                .is_some_and(|s| s.width > 0.0 && s.color.a > 0),
            Self::FillUp => shape.geom.is_closed() && !shape.style.fill.is_none(),
            _ => true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub duration: f32,
    pub delay: f32,
    pub stagger: f32,
    pub intensity: f32,
    pub start_at_playhead: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            duration: 0.8,
            delay: 0.0,
            stagger: 0.08,
            intensity: 1.0,
            start_at_playhead: true,
        }
    }
}
impl Options {
    fn checked(self) -> Result<Self, String> {
        if [self.duration, self.delay, self.stagger, self.intensity]
            .iter()
            .any(|v| !v.is_finite())
        {
            return Err("Animation settings must be finite numbers".into());
        }
        Ok(Self {
            duration: self.duration.clamp(0.08, 30.0),
            delay: self.delay.clamp(0.0, 60.0),
            stagger: self.stagger.clamp(0.0, 5.0),
            intensity: self.intensity.clamp(0.1, 4.0),
            ..self
        })
    }
}

pub struct Target {
    pub id: u64,
    pub bounds: Bounds,
    pub opacity: f32,
}

pub fn apply(
    before: &Motion,
    preset: Preset,
    targets: &[Target],
    playhead: f32,
    options: Options,
) -> Result<Motion, String> {
    let options = options.checked()?;
    if targets.is_empty() {
        return Err("Select compatible vector objects first".into());
    }
    if !playhead.is_finite() {
        return Err("Choose a valid animation start time".into());
    }
    let start = if options.start_at_playhead {
        playhead.max(0.0)
    } else {
        0.0
    } + options.delay;
    let mut after = before.clone();
    after.duration = after
        .duration
        .max(start + (targets.len() - 1) as f32 * options.stagger + options.duration);
    for (index, target) in targets.iter().enumerate() {
        let start = start + index as f32 * options.stagger;
        let pose = before.pose(target.id, start);
        let scale = pose.scale;
        let opacity = pose.opacity.unwrap_or(target.opacity);
        let amount = options.intensity;
        let travel = target
            .bounds
            .width()
            .max(target.bounds.height())
            .clamp(40.0, 400.0)
            * amount;
        let mut channels: Vec<(Prop, Vec<(f32, f32, Ease)>)> = vec![];
        let mut add = |prop, points: &[(f32, f32, Ease)]| channels.push((prop, points.to_vec()));
        use Ease::{EaseIn, EaseInOut, EaseOut, Linear};
        match preset {
            Preset::DrawStroke => add(
                Prop::StrokeReveal,
                &[(0.0, 0.0, EaseInOut), (1.0, 1.0, Linear)],
            ),
            Preset::FillUp => add(
                Prop::FillReveal,
                &[(0.0, 0.0, EaseInOut), (1.0, 1.0, Linear)],
            ),
            Preset::PopIn => {
                add(
                    Prop::Scale,
                    &[
                        (0.0, scale * 0.01, EaseOut),
                        (0.7, scale * (1.0 + 0.16 * amount), EaseInOut),
                        (1.0, scale, Linear),
                    ],
                );
                add(
                    Prop::Opacity,
                    &[
                        (0.0, 0.0, EaseOut),
                        (0.2, opacity, Linear),
                        (1.0, opacity, Linear),
                    ],
                );
            }
            Preset::Slam => {
                add(
                    Prop::Y,
                    &[
                        (0.0, pose.dy - travel * 1.5, EaseIn),
                        (0.65, pose.dy + travel * 0.04, EaseOut),
                        (1.0, pose.dy, Linear),
                    ],
                );
                add(
                    Prop::Scale,
                    &[
                        (0.0, scale * (1.0 + 0.8 * amount), EaseIn),
                        (0.65, scale * 0.94, EaseOut),
                        (1.0, scale, Linear),
                    ],
                );
                add(
                    Prop::Opacity,
                    &[
                        (0.0, 0.0, Linear),
                        (0.1, opacity, Linear),
                        (1.0, opacity, Linear),
                    ],
                );
            }
            Preset::Shake | Preset::Buzz => {
                let cycles = if preset == Preset::Buzz { 12 } else { 4 };
                let distance = if preset == Preset::Buzz {
                    2.5 * amount
                } else {
                    travel * 0.07
                };
                let rotation = if preset == Preset::Buzz {
                    0.012 * amount
                } else {
                    0.04 * amount
                };
                for (prop, base, amplitude) in [
                    (Prop::X, pose.dx, distance),
                    (Prop::Rotation, pose.rotation, rotation),
                ] {
                    let mut points = vec![(0.0, base, Linear)];
                    for n in 1..cycles * 2 {
                        let t = n as f32 / (cycles * 2) as f32;
                        let sign = if n % 2 == 0 { -1.0 } else { 1.0 };
                        points.push((t, base + sign * amplitude * (1.0 - t * 0.65), EaseInOut));
                    }
                    points.push((1.0, base, Linear));
                    channels.push((prop, points));
                }
            }
            Preset::SlideUp | Preset::SlideDown | Preset::SlideLeft | Preset::SlideRight => {
                let (prop, base, offset) = match preset {
                    Preset::SlideUp => (Prop::Y, pose.dy, travel),
                    Preset::SlideDown => (Prop::Y, pose.dy, -travel),
                    Preset::SlideLeft => (Prop::X, pose.dx, travel),
                    _ => (Prop::X, pose.dx, -travel),
                };
                add(prop, &[(0.0, base + offset, EaseOut), (1.0, base, Linear)]);
                add(
                    Prop::Opacity,
                    &[
                        (0.0, 0.0, EaseOut),
                        (0.35, opacity, Linear),
                        (1.0, opacity, Linear),
                    ],
                );
            }
            Preset::Fly => {
                add(
                    Prop::X,
                    &[
                        (0.0, pose.dx - travel * 1.6, EaseOut),
                        (1.0, pose.dx, Linear),
                    ],
                );
                add(
                    Prop::Y,
                    &[
                        (0.0, pose.dy + travel * 0.8, EaseOut),
                        (1.0, pose.dy, Linear),
                    ],
                );
                add(
                    Prop::Rotation,
                    &[
                        (0.0, pose.rotation - 0.5 * amount, EaseOut),
                        (1.0, pose.rotation, Linear),
                    ],
                );
                add(
                    Prop::Scale,
                    &[(0.0, scale * 0.65, EaseOut), (1.0, scale, Linear)],
                );
                add(
                    Prop::Opacity,
                    &[
                        (0.0, 0.0, EaseOut),
                        (0.3, opacity, Linear),
                        (1.0, opacity, Linear),
                    ],
                );
            }
            Preset::Zoom => {
                add(
                    Prop::Scale,
                    &[
                        (0.0, scale * (1.0 + 2.0 * amount), EaseOut),
                        (1.0, scale, Linear),
                    ],
                );
                add(
                    Prop::Opacity,
                    &[
                        (0.0, 0.0, EaseOut),
                        (0.25, opacity, Linear),
                        (1.0, opacity, Linear),
                    ],
                );
            }
            Preset::Fade => add(
                Prop::Opacity,
                &[(0.0, 0.0, EaseInOut), (1.0, opacity, Linear)],
            ),
        }
        for (prop, points) in channels {
            let end = start + options.duration;
            let mut keys = before
                .tracks
                .iter()
                .find(|track| track.shape == target.id && track.prop == prop)
                .map(|track| track.keys.clone())
                .unwrap_or_default();
            let was_empty = keys.is_empty();
            // Preserve surrounding animation. One short bridge before the window
            // prevents a distant earlier key from drifting towards an entrance.
            let guard = (start - 1.0 / before.fps.clamp(1.0, 120.0)).max(0.0);
            let guard_value = before.value(target.id, prop, guard);
            keys.retain(|key| key.t < start || key.t > end);
            if was_empty && start > 0.0 {
                keys.push(Key {
                    t: 0.0,
                    value: points[0].1,
                    ease: Linear,
                });
            } else if start > 0.0
                && let Some(value) = guard_value
            {
                keys.retain(|key| (key.t - guard).abs() > 1e-5);
                keys.push(Key {
                    t: guard,
                    value,
                    ease: Linear,
                });
            }
            keys.extend(points.into_iter().map(|(fraction, value, ease)| Key {
                t: start + fraction * options.duration,
                value,
                ease,
            }));
            keys.sort_by(|a, b| a.t.total_cmp(&b.t));
            let replacement = Track {
                shape: target.id,
                prop,
                keys,
            };
            if let Some(track) = after
                .tracks
                .iter_mut()
                .find(|track| track.shape == target.id && track.prop == prop)
            {
                *track = replacement;
            } else {
                after.tracks.push(replacement);
            }
        }
    }
    Ok(after)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Pt;

    fn target(id: u64) -> Target {
        Target {
            id,
            bounds: Bounds::from_min_size(Pt::ZERO, Pt::new(80.0, 60.0)),
            opacity: 0.4,
        }
    }

    #[test]
    fn every_recipe_moves_then_lands_on_the_existing_pose() {
        let mut before = Motion::default();
        for (prop, value) in [
            (Prop::X, 19.0),
            (Prop::Y, -8.0),
            (Prop::Rotation, 0.3),
            (Prop::Scale, 1.25),
        ] {
            before.set_key(1, prop, 0.0, value, Ease::Linear);
        }
        for preset in Preset::ALL {
            let after = apply(
                &before,
                preset,
                &[target(1)],
                0.0,
                Options {
                    duration: 1.0,
                    ..Options::default()
                },
            )
            .unwrap();
            let end = after.pose(1, 1.0);
            assert!((end.dx - 19.0).abs() < 1e-5, "{preset:?}");
            assert!((end.dy + 8.0).abs() < 1e-5, "{preset:?}");
            assert!((end.rotation - 0.3).abs() < 1e-5, "{preset:?}");
            assert!((end.scale - 1.25).abs() < 1e-5, "{preset:?}");
            assert!(
                (end.opacity.unwrap_or(0.4) - 0.4).abs() < 1e-5,
                "{preset:?}"
            );
            assert_eq!(end.stroke_reveal.unwrap_or(1.0), 1.0);
            assert_eq!(end.fill_reveal.unwrap_or(1.0), 1.0);
            assert!(
                (0..25).any(|i| after.pose(1, i as f32 / 25.0) != end),
                "{preset:?} must visibly animate"
            );
        }
    }

    #[test]
    fn stagger_and_delay_hold_entrances_then_extend_the_clip() {
        let after = apply(
            &Motion::default(),
            Preset::FillUp,
            &[target(1), target(2)],
            1.5,
            Options {
                duration: 1.0,
                delay: 0.5,
                stagger: 0.25,
                ..Options::default()
            },
        )
        .unwrap();
        assert_eq!(after.duration, 3.25);
        assert_eq!(after.pose(1, 1.0).fill_reveal, Some(0.0));
        assert_eq!(after.pose(2, 2.25).fill_reveal, Some(0.0));
        assert!(after.pose(1, 2.25).fill_reveal.unwrap() > 0.0);
        assert_eq!(after.pose(2, 3.25).fill_reveal, Some(1.0));
        let from_start = apply(
            &Motion::default(),
            Preset::DrawStroke,
            &[target(1)],
            1.5,
            Options {
                start_at_playhead: false,
                delay: 0.0,
                stagger: 0.0,
                ..Options::default()
            },
        )
        .unwrap();
        assert_eq!(from_start.pose(1, 0.0).stroke_reveal, Some(0.0));
        assert_eq!(from_start.pose(1, 0.8).stroke_reveal, Some(1.0));
    }

    #[test]
    fn replacing_a_window_preserves_other_objects_channels_and_outside_keys() {
        let mut before = Motion {
            duration: 6.0,
            ..Motion::default()
        };
        for (id, prop) in [(1, Prop::X), (1, Prop::FillReveal), (2, Prop::Y)] {
            for t in [0.0, 1.0, 3.0, 5.0] {
                before.set_key(id, prop, t, t * 0.1, Ease::Linear);
            }
        }
        let after = apply(
            &before,
            Preset::SlideRight,
            &[target(1)],
            2.0,
            Options {
                duration: 1.0,
                ..Options::default()
            },
        )
        .unwrap();
        for track in before.tracks.iter().filter(|track| track.prop != Prop::X) {
            assert!(after.tracks.contains(track));
        }
        let x = after
            .tracks
            .iter()
            .find(|track| track.shape == 1 && track.prop == Prop::X)
            .unwrap();
        for old in before.tracks[0]
            .keys
            .iter()
            .filter(|key| key.t < 2.0 || key.t > 3.0)
        {
            assert!(x.keys.contains(old));
        }
        assert!(
            x.keys
                .iter()
                .all(|key| key.t.is_finite() && key.value.is_finite())
        );
    }

    #[test]
    fn short_buzz_keeps_distinct_editable_keys_and_rejects_invalid_settings() {
        let after = apply(
            &Motion::default(),
            Preset::Buzz,
            &[target(1)],
            0.0,
            Options {
                duration: 0.08,
                ..Options::default()
            },
        )
        .unwrap();
        let x = after
            .tracks
            .iter()
            .find(|track| track.prop == Prop::X)
            .unwrap();
        assert_eq!(x.keys.len(), 25);
        assert!(x.keys.windows(2).all(|keys| keys[0].t < keys[1].t));
        assert!(
            apply(
                &after,
                Preset::Buzz,
                &[target(1)],
                0.0,
                Options {
                    duration: f32::NAN,
                    ..Options::default()
                }
            )
            .is_err()
        );
    }
}
