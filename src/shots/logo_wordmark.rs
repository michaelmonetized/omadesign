//! A native, editable reconstruction of the user's open maze wordmark.
//! The supplied Omarchy mark is embedded so the scene remains portable.
use crate::app::{Op, Studio};
use crate::color::Rgba;
use crate::document::{Artboard, Cap, Cmd, Document, Fill, Join, Layer, Shape, Stroke, Style};
use crate::filter::{FilterStack, Fx};
use crate::geom::{Anchor, Bounds, Geom, Pt};
use crate::motion_presets::{Options, Preset, Target};
use crate::tools::{Persona, Tool};

pub const DURATION: f32 = 30.0;
const WHITE: Rgba = Rgba {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};
const GREEN: Rgba = Rgba {
    r: 158,
    g: 206,
    b: 106,
    a: 255,
};
const WEIGHT: f32 = 48.0;
const RAW_OFFSET: Pt = Pt { x: -36.0, y: -28.0 };
const CENTRE_D_OFFSET: Pt = Pt {
    x: 660.0,
    y: -192.0,
};
const MAZE_SVG: &str = r##"<svg fill="none" height="1200" viewBox="0 0 1200 1200" width="1200" xmlns="http://www.w3.org/2000/svg"><path clip-rule="evenodd" d="m1200 1200h-480v-80h400v-1040h-479.996v160h-400v720h720v-720h-80v-80h159.996v880h-400v160h-640v-1200h1200zm-1120-80h480v-80h-400l.004-400h-80.004zm0-560h80.004v-400h400v-80h-480.004z" fill="#9ece6a" fill-rule="evenodd"/></svg>"##;
const LETTER_NAMES: [&str; 6] = [
    "d · Draw stroke",
    "e · Pop in",
    "s · Slam",
    "i · Fade in",
    "g · Slide up",
    "n · Fly",
];
const PRESETS: [Preset; 6] = [
    Preset::DrawStroke,
    Preset::PopIn,
    Preset::Slam,
    Preset::Fade,
    Preset::SlideUp,
    Preset::Fly,
];
const DRAW_TIMES: [(f32, f32); 6] = [
    (1.8, 5.4),
    (7.2, 9.8),
    (10.2, 12.8),
    (13.2, 15.1),
    (15.5, 18.1),
    (18.5, 21.1),
];

pub struct Cue {
    pub cursor: Option<Pt>,
    pub shift: bool,
    pub caption: &'static str,
}

pub struct LogoDemo {
    maze: Shape,
    guide: Shape,
    added_maze: bool,
    added_guide: bool,
    letters: [Vec<u64>; 6],
    drawing: Option<usize>,
    anchors_placed: usize,
    presets_applied: usize,
    aligned: bool,
    time: f32,
}

fn white_stroke() -> Style {
    Style {
        fill: Fill::None,
        stroke: Some(Stroke {
            color: WHITE,
            width: WEIGHT,
            cap: Cap::Square,
            join: Join::Miter,
            dash: None,
        }),
    }
}

/// Every centerline follows a 24 px half-module; the finished bar is 48 px.
/// Visible wordmark bounds are x132..1788 / bottom948: 132 px on all three sides.
fn letter_points(index: usize) -> Vec<Pt> {
    let points: &[(f32, f32)] = match index {
        0 => &[
            (444., 492.),
            (348., 492.),
            (348., 924.),
            (156., 924.),
            (156., 732.),
            (276., 732.),
        ],
        1 => &[
            (540., 828.),
            (660., 828.),
            (660., 732.),
            (468., 732.),
            (468., 924.),
            (660., 924.),
        ],
        2 => &[
            (972., 732.),
            (780., 732.),
            (780., 828.),
            (972., 828.),
            (972., 924.),
            (780., 924.),
        ],
        3 => &[(1164., 732.), (1116., 732.), (1116., 924.), (1068., 924.)],
        4 => &[
            (1452., 780.),
            (1452., 732.),
            (1260., 732.),
            (1260., 924.),
            (1452., 924.),
            (1452., 852.),
            (1356., 852.),
        ],
        _ => &[
            (1572., 924.),
            (1572., 732.),
            (1668., 732.),
            (1668., 924.),
            (1764., 924.),
            (1764., 732.),
        ],
    };
    points.iter().map(|&(x, y)| Pt::new(x, y)).collect()
}

fn letter_shape(index: usize) -> Shape {
    let mut shape = Shape::new(
        Geom::Path {
            anchors: letter_points(index)
                .into_iter()
                .map(Anchor::corner)
                .collect(),
            closed: false,
        },
        white_stroke(),
    );
    shape.name = LETTER_NAMES[index].into();
    shape.filters = word_shadow(1.0);
    shape
}
fn dot_shape() -> Shape {
    let mut dot = Shape::new(
        Geom::Rect {
            origin: Pt::new(1092., 636.),
            size: Pt::splat(48.),
            radius: 0.,
        },
        Style {
            fill: Fill::Solid(WHITE),
            stroke: None,
        },
    );
    dot.name = "i · square dot".into();
    dot.filters = word_shadow(1.0);
    dot
}
fn word_shadow(amount: f32) -> FilterStack {
    FilterStack {
        enabled: true,
        items: vec![Fx::Shadow {
            dx: 0.,
            dy: 10. * amount,
            blur: 12. * amount,
            color: Rgba::new(0, 0, 0, 230),
        }],
    }
}
fn maze_shape() -> Shape {
    let mut geom =
        crate::shape_browser::svg_to_geom(MAZE_SVG, 1200.).expect("embedded Omarchy SVG is valid");
    if let Geom::Poly { winding, .. } = &mut geom {
        *winding = false;
    }
    geom.map_into(
        Bounds::from_min_size(Pt::ZERO, Pt::splat(1200.)),
        Bounds::from_min_size(Pt::new(630., 138.), Pt::splat(660.)),
    );
    let mut shape = Shape::new(
        geom,
        Style {
            fill: Fill::Solid(GREEN),
            stroke: None,
        },
    );
    shape.name = "Omarchy maze · soft green light".into();
    shape.opacity = 0.82;
    shape.filters = FilterStack {
        enabled: true,
        items: vec![Fx::Blur { std: 22. }],
    };
    shape
}
fn guide_shape() -> Shape {
    let mut shape = Shape::new(
        Geom::Rect {
            origin: Pt::new(156., 732.),
            size: Pt::splat(192.),
            radius: 0.,
        },
        Style {
            fill: Fill::None,
            stroke: Some(Stroke {
                color: GREEN,
                width: 1.,
                cap: Cap::Butt,
                join: Join::Miter,
                dash: None,
            }),
        },
    );
    shape.name = "192 px letter square · 48 px maze module".into();
    shape.guide = true;
    shape
}
fn blank_document() -> Document {
    let mut doc = Document::new("omadesign · maze wordmark", 1., 1., 72.);
    doc.width = 1920.;
    doc.height = 1080.;
    doc.artboards = vec![Artboard::new(0, Pt::ZERO, Pt::new(1920., 1080.))];
    doc.artboards[0].name = "1920 × 1080 · web".into();
    doc.layers = vec![
        Layer::vector("Black"),
        Layer::vector("Omarchy maze"),
        Layer::vector("Wordmark · editable pen paths"),
        Layer::vector("Construction guide"),
    ];
    let mut black = Shape::new(
        Geom::Rect {
            origin: Pt::ZERO,
            size: Pt::new(1920., 1080.),
            radius: 0.,
        },
        Style {
            fill: Fill::Solid(Rgba::BLACK),
            stroke: None,
        },
    );
    black.name = "Black background".into();
    doc.layers[0].kind.shapes_mut().unwrap().push(black);
    doc.layers[0].locked = true;
    doc.motion.duration = 3.0;
    doc.motion.looped = false;
    doc.ruler.guides_visible = false;
    doc
}
fn options(index: usize) -> Options {
    Options {
        duration: [1.20, 1.0, 0.95, 0.85, 1.05, 1.30][index],
        delay: [0., 0.22, 0.42, 0.62, 0.82, 1.04][index],
        stagger: 0.,
        intensity: 0.65,
        start_at_playhead: false,
    }
}

/// The final reusable project, including real editable preset channels settled by 2.34 s.
pub fn document() -> Document {
    let mut doc = blank_document();
    doc.layers[1].kind.shapes_mut().unwrap().push(maze_shape());
    for (index, preset) in PRESETS.into_iter().enumerate() {
        let shape = letter_shape(index);
        let mut targets = vec![Target {
            id: shape.id,
            bounds: shape.world_bbox(),
            opacity: shape.opacity,
        }];
        doc.layers[2].kind.shapes_mut().unwrap().push(shape);
        if index == 3 {
            let dot = dot_shape();
            targets.push(Target {
                id: dot.id,
                bounds: dot.world_bbox(),
                opacity: dot.opacity,
            });
            doc.layers[2].kind.shapes_mut().unwrap().push(dot);
        }
        doc.motion =
            crate::motion_presets::apply(&doc.motion, preset, &targets, 0., options(index))
                .expect("fixed logo preset settings are valid");
    }
    doc.layers[3].kind.shapes_mut().unwrap().push(guide_shape());
    doc
}

pub fn seed(studio: &mut Studio) -> LogoDemo {
    studio.doc = blank_document();
    studio.path = None;
    studio.history.clear();
    studio.selection.clear();
    studio.artboard_sel.clear();
    studio.active_layer = Some(2);
    studio.persona = Persona::Design;
    studio.tool = Tool::Pen;
    studio.style = white_stroke();
    studio.op = None;
    studio.cursor = None;
    studio.show_welcome = false;
    studio.show_shortcuts = false;
    studio.show_templates = false;
    studio.show_rulers = true;
    studio.show_grid = false;
    studio.show_key_hud = true;
    studio.libraries.sidebar = crate::app::libraries::Sidebar::Inspector;
    studio.playing = false;
    studio.playhead = 0.;
    studio.need_fit = true;
    studio.mark();
    LogoDemo {
        maze: maze_shape(),
        guide: guide_shape(),
        added_maze: false,
        added_guide: false,
        letters: std::array::from_fn(|_| Vec::new()),
        drawing: None,
        anchors_placed: 0,
        presets_applied: 0,
        aligned: false,
        time: 0.,
    }
}

fn eased(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn fraction(t: f32, start: f32, end: f32) -> f32 {
    ((t - start) / (end - start)).clamp(0., 1.)
}
fn path_cursor(points: &[Pt], progress: f32) -> (usize, Pt) {
    let lengths: Vec<f32> = points.windows(2).map(|p| (p[1] - p[0]).length()).collect();
    let mut distance = lengths.iter().sum::<f32>() * progress.clamp(0., 1.);
    for (i, length) in lengths.iter().copied().enumerate() {
        if distance < length {
            return (i + 1, points[i].lerp(points[i + 1], distance / length));
        }
        distance -= length;
    }
    (points.len(), *points.last().unwrap())
}

impl LogoDemo {
    fn set_geom(studio: &mut Studio, layer: usize, id: u64, geom: Geom) {
        if let Some(shape) = studio.doc.find_shape(layer, id).cloned() {
            if shape.geom == geom {
                return;
            }
            studio.commit(Cmd::SetGeom {
                layer,
                id,
                before: shape.geom,
                after: geom,
                rot_before: shape.rotation,
                rot_after: shape.rotation,
            });
        }
    }
    fn finish_letter(&mut self, studio: &mut Studio, index: usize) {
        if let Some(Op::Pen {
            anchors, source, ..
        }) = studio.op.take()
        {
            studio.finish_pen(anchors, false, source);
            if let Some(&(layer, id)) = studio.selection.last() {
                if let Some(shape) = studio.doc.find_shape_mut(layer, id) {
                    shape.name = LETTER_NAMES[index].into();
                }
                self.letters[index].push(id);
            }
        }
        if index == 3 {
            let mut dot = dot_shape();
            dot.filters = FilterStack::default();
            dot.geom.translate(RAW_OFFSET);
            self.letters[index].push(dot.id);
            studio.commit(Cmd::AddShape {
                layer: 2,
                shape: dot,
            });
        }
        self.drawing = None;
        self.anchors_placed = 0;
        studio.cursor = None;
    }
    fn draw_letter(&mut self, studio: &mut Studio, index: usize, progress: f32) -> Option<Pt> {
        if !self.letters[index].is_empty() {
            return None;
        }
        if self.drawing != Some(index) {
            studio.op = None;
            studio.selection.clear();
            studio.tool = Tool::Pen;
            studio.style = white_stroke();
            if index == 0 {
                studio.style.stroke.as_mut().unwrap().width = 64.;
            }
            studio.active_layer = Some(2);
            self.drawing = Some(index);
            self.anchors_placed = 0;
        }
        let offset = if index == 0 {
            CENTRE_D_OFFSET
        } else {
            RAW_OFFSET
        };
        let points: Vec<_> = letter_points(index)
            .into_iter()
            .map(|p| p + offset)
            .collect();
        let (count, cursor) = path_cursor(&points, progress);
        while self.anchors_placed < count {
            studio.pen_click(points[self.anchors_placed]);
            self.anchors_placed += 1;
        }
        studio.cursor = Some(cursor);
        if progress >= 1. {
            self.finish_letter(studio, index);
        }
        Some(cursor)
    }

    /// Advance native construction and animation. May jump directly to DURATION.
    /// The recorder must set play_clock to egui's frame time before drawing the UI.
    pub fn step(&mut self, studio: &mut Studio, seconds: f32) -> Cue {
        let t = seconds.clamp(0., DURATION);
        if t < self.time {
            *self = seed(studio);
        }
        self.time = t;
        let mut cue = Cue {
            cursor: None,
            shift: false,
            caption: "1920 × 1080. Start with black.",
        };
        if t >= 0.8 {
            if !self.added_maze {
                let mut maze = self.maze.clone();
                maze.geom.map_into(
                    maze.geom.bbox(),
                    Bounds::from_min_size(Pt::new(480., 36.), Pt::splat(960.)),
                );
                maze.filters = FilterStack::default();
                maze.opacity = 1.;
                studio.commit(Cmd::AddShape {
                    layer: 1,
                    shape: maze,
                });
                self.added_maze = true;
            }
            cue.caption = "Place the Omarchy maze. A 64 px module.";
        }
        for (index, &(start, end)) in DRAW_TIMES.iter().enumerate() {
            if t >= start {
                let cursor = self.draw_letter(studio, index, fraction(t, start, end));
                if t < end {
                    cue.cursor = cursor;
                    cue.shift = true;
                    cue.caption = [
                        "Draw d inside the maze. Same weight, square corners.",
                        "Trace e. One square, one consistent rhythm.",
                        "Move the square. Draw the angular s.",
                        "A stepped i. A square dot.",
                        "Trace g, keeping the counter open.",
                        "Finish n with the same square module.",
                    ][index];
                }
            }
        }
        if t >= 5.4 && !self.letters[0].is_empty() {
            let amount = eased(fraction(t, 5.4, 6.5));
            let offset = CENTRE_D_OFFSET.lerp(RAW_OFFSET, amount);
            if let Some(shape) = studio.doc.find_shape_mut(2, self.letters[0][0])
                && let Some(stroke) = &mut shape.style.stroke
            {
                stroke.width = 64. - 16. * amount;
            }
            let mut geom = letter_shape(0).geom;
            geom.translate(offset);
            if t <= 6.5 || !self.aligned {
                Self::set_geom(studio, 2, self.letters[0][0], geom);
            }
            if t < 6.5 {
                studio.tool = Tool::Select;
                studio.selection = vec![(2, self.letters[0][0])];
                cue.cursor = Some(Pt::new(300., 708.) + offset);
                cue.caption = "Move d to the lower left. Build from its base.";
            }
        }
        if t >= 6.5 && !self.added_guide {
            studio.commit(Cmd::AddShape {
                layer: 3,
                shape: self.guide.clone(),
            });
            self.added_guide = true;
        }
        if (6.5..21.1).contains(&t) {
            studio.doc.ruler.guides_visible = true;
            let locations = [156., 468., 780., 1020., 1260., 1572.];
            let moves = [
                (6.5, 7.2),
                (9.8, 10.2),
                (12.8, 13.2),
                (15.1, 15.5),
                (18.1, 18.5),
            ];
            let mut x = locations[0];
            for (i, &(start, end)) in moves.iter().enumerate() {
                if t >= start {
                    x = locations[i]
                        + (locations[i + 1] - locations[i]) * eased(fraction(t, start, end));
                }
                if (start..end).contains(&t) {
                    cue.caption = "Move the 192 px construction square to the next letter.";
                    cue.cursor = Some(Pt::new(x + 96., 828.) + RAW_OFFSET);
                    studio.tool = Tool::Select;
                }
            }
            Self::set_geom(
                studio,
                3,
                self.guide.id,
                Geom::Rect {
                    origin: Pt::new(x, 732.) + RAW_OFFSET,
                    size: Pt::splat(192.),
                    radius: 0.,
                },
            );
        }
        if t >= 21.1 {
            studio.op = None;
            studio.doc.ruler.guides_visible = false;
            let offset = RAW_OFFSET * (1. - eased(fraction(t, 21.1, 22.1)));
            for index in 0..6 {
                for (part, &id) in self.letters[index].iter().enumerate() {
                    let mut geom = if part == 0 {
                        letter_shape(index).geom
                    } else {
                        dot_shape().geom
                    };
                    geom.translate(offset);
                    Self::set_geom(studio, 2, id, geom);
                }
            }
            self.aligned = true;
            if t < 22.1 {
                studio.tool = Tool::Select;
                studio.selection = self
                    .letters
                    .iter()
                    .flat_map(|ids| ids.iter().map(|&id| (2, id)))
                    .collect();
                cue.caption = "Align the baseline. Balance the gaps. Centre the wordmark.";
                cue.cursor = Some(Pt::new(960., 900.) + offset);
            }
        }
        if t >= 22.1 {
            let amount = eased(fraction(t, 22.1, 23.4));
            let mut geom = self.maze.geom.clone();
            geom.map_into(
                geom.bbox(),
                Bounds::from_min_size(
                    Pt::new(480., 36.).lerp(Pt::new(630., 138.), amount),
                    Pt::splat(960. - 300. * amount),
                ),
            );
            Self::set_geom(studio, 1, self.maze.id, geom);
            if let Some(shape) = studio.doc.find_shape_mut(1, self.maze.id) {
                shape.filters = FilterStack {
                    enabled: true,
                    items: vec![Fx::Blur { std: 22. * amount }],
                };
                shape.opacity = 1. - 0.18 * amount;
            }
            if t < 23.4 {
                studio.selection = vec![(1, self.maze.id)];
                studio.tool = Tool::Select;
                cue.caption = "Scale the maze down. Soften it with Gaussian blur.";
                cue.cursor = Some(Pt::new(1440., 996.).lerp(Pt::new(1290., 798.), amount));
            }
        }
        if t >= 23.4 {
            let amount = eased(fraction(t, 23.4, 24.4));
            for ids in &self.letters {
                for &id in ids {
                    if let Some(shape) = studio.doc.find_shape_mut(2, id) {
                        shape.filters = word_shadow(amount);
                    }
                }
            }
            if t < 24.4 {
                studio.selection = self
                    .letters
                    .iter()
                    .flat_map(|ids| ids.iter().map(|&id| (2, id)))
                    .collect();
                cue.caption = "A soft shadow gives the pen lettering a little depth.";
            }
        }
        if t >= 24.4 {
            let count = (((t - 24.4) / 1.6 * 6.).floor() as usize + 1).min(6);
            while self.presets_applied < count {
                let index = self.presets_applied;
                studio.selection = self.letters[index].iter().map(|&id| (2, id)).collect();
                studio.motion_preset_options = options(index);
                studio.playhead = 3.;
                studio.apply_motion_preset(PRESETS[index]);
                if self.presets_applied == 0 {
                    studio.need_fit = true;
                }
                self.presets_applied += 1;
            }
            cue.caption = "Give each letter a different motion. Every key stays editable.";
        }
        if t >= 26. {
            studio.selection.clear();
            studio.persona = Persona::Motion;
            studio.playhead = (t - 26.).min(3.);
            studio.playing = t < 29.;
            cue.caption = "Six letters. Six motions. One wordmark.";
        }
        studio.cursor = cue.cursor;
        studio.status = cue.caption.into();
        studio.mark();
        cue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn final_wordmark_has_matching_bars_clear_margins_and_settled_animation() {
        let doc = document();
        assert_eq!((doc.width, doc.height), (1920., 1080.));
        let letters = doc.layers[2].kind.shapes().unwrap();
        assert_eq!(letters.len(), 7);
        let mut visible: Option<Bounds> = None;
        for shape in letters {
            let bounds = shape.world_bbox();
            let width = shape.style.stroke.as_ref().map_or(0., |s| s.width);
            if let Some(stroke) = &shape.style.stroke {
                assert_eq!(stroke.width, WEIGHT);
                assert_eq!(stroke.cap, Cap::Square);
                assert_eq!(stroke.join, Join::Miter);
            }
            let bounds = Bounds {
                min: bounds.min - Pt::splat(width * 0.5),
                max: bounds.max + Pt::splat(width * 0.5),
            };
            visible = Some(visible.map_or(bounds, |old| old.union(bounds)));
            let pose = doc.motion.pose(shape.id, 3.);
            assert_eq!(
                (pose.dx, pose.dy, pose.rotation, pose.scale),
                (0., 0., 0., 1.)
            );
            assert_eq!(pose.opacity.unwrap_or(1.), 1.);
            assert_eq!(pose.stroke_reveal.unwrap_or(1.), 1.);
            if let Geom::Path { anchors, .. } = &shape.geom {
                assert!(
                    anchors
                        .windows(2)
                        .all(|p| p[0].pt.x == p[1].pt.x || p[0].pt.y == p[1].pt.y)
                );
                assert!(anchors.windows(2).all(|p| p[0].pt != p[1].pt));
            }
        }
        let visible = visible.unwrap();
        assert_eq!(
            (visible.min.x, 1920. - visible.max.x, 1080. - visible.max.y),
            (132., 132., 132.)
        );
        assert_eq!(visible.min.y, 468.);
        assert!(
            doc.motion
                .tracks
                .iter()
                .flat_map(|track| &track.keys)
                .all(|key| key.t <= 3.)
        );
    }

    #[test]
    fn native_pen_replay_and_direct_ending_produce_the_same_finished_letters() {
        let mut studio = Studio::new();
        let mut replay = seed(&mut studio);
        let cue = replay.step(&mut studio, 3.0);
        assert!(cue.shift && cue.cursor.is_some());
        assert!(
            matches!(&studio.op, Some(Op::Pen { anchors, source: None, .. }) if anchors.len() >= 2 && anchors.len() < 6)
        );
        assert!(studio.doc.layers[2].kind.shapes().unwrap().is_empty());
        for frame in 91..=900 {
            replay.step(&mut studio, frame as f32 / 30.);
        }
        let mut ending = Studio::new();
        seed(&mut ending).step(&mut ending, DURATION);
        let expected = document();
        for result in [&studio, &ending] {
            assert!(result.op.is_none());
            assert!(!result.doc.ruler.guides_visible);
            let shapes = result.doc.layers[2].kind.shapes().unwrap();
            assert_eq!(shapes.len(), 7);
            for shape in expected.layers[2].kind.shapes().unwrap() {
                let actual = shapes.iter().find(|s| s.name == shape.name).unwrap();
                assert_eq!(actual.geom, shape.geom, "{}", shape.name);
                assert_eq!(actual.style, shape.style);
                assert_eq!(actual.filters, shape.filters);
                assert_eq!(
                    result.doc.motion.pose(actual.id, 3.),
                    expected.motion.pose(shape.id, 3.)
                );
            }
            assert_eq!(result.doc.motion.tracks.len(), expected.motion.tracks.len());
        }
    }
}
