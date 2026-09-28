//! Story-level area layout and geometric wrap. Source content is never truncated.
use super::*;
use crate::geom::{BreakMode, ParagraphStyle};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VAlign {
    #[default]
    Top,
    Center,
    Bottom,
    Justify,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Overflow {
    Visible,
    #[default]
    Clip,
    Ellipsis,
    Flow,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextFrame {
    pub size: Pt,
    pub inset: [f32; 4],
    pub valign: VAlign,
    pub auto_height: bool,
    pub overflow: Overflow,
    pub max_lines: Option<u32>,
    pub terminal_overflow: Overflow,
    pub contour: Option<Vec<Vec<Pt>>>,
}
impl Default for TextFrame {
    fn default() -> Self {
        Self {
            size: Pt::new(240., 160.),
            inset: [0.; 4],
            valign: VAlign::Top,
            auto_height: false,
            overflow: Overflow::Clip,
            max_lines: None,
            terminal_overflow: Overflow::Clip,
            contour: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextThread {
    pub story: u64,
    pub prev: Option<u64>,
    pub next: Option<u64>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WrapMode {
    #[default]
    None,
    BoundingBox,
    ObjectShape,
    JumpObject,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextWrap {
    pub mode: WrapMode,
    pub offset: [f32; 4],
    pub invert: bool,
    pub ignore_locked: bool,
}
impl TextWrap {
    pub fn is_none(&self) -> bool {
        self.mode == WrapMode::None
    }
}

pub fn frame_bounds(run: &TypeRun) -> Option<Bounds> {
    let frame = run.frame.as_ref()?;
    Some(Bounds::from_min_size(
        Pt::new(run.origin.x, run.origin.y - run.px * 0.85),
        frame.size,
    ))
}

#[derive(Clone)]
pub struct Obstacle {
    pub id: u64,
    pub order: (usize, usize),
    pub bounds: Bounds,
    pub contours: Vec<Vec<Pt>>,
    pub wrap: TextWrap,
    pub locked: bool,
}
fn polygon_intervals(poly: &[Pt], y: f32) -> Vec<(f32, f32)> {
    let bottom=poly.iter().map(|p|p.y).fold(f32::NEG_INFINITY,f32::max);
    let y=if (y-bottom).abs()<0.0001 {bottom-0.0001}else{y};
    let mut xs = vec![];
    if poly.len() < 3 {
        return vec![];
    }
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        if (a.y <= y && b.y > y) || (b.y <= y && a.y > y) {
            xs.push(a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y));
        }
    }
    xs.sort_by(f32::total_cmp);
    xs.chunks_exact(2).map(|p| (p[0], p[1])).collect()
}
fn union_spans(mut spans: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut result: Vec<(f32, f32)> = vec![];
    for span in spans {
        if let Some(last) = result.last_mut()
            && span.0 <= last.1
        {
            last.1 = last.1.max(span.1);
        } else {
            result.push(span);
        }
    }
    result
}
fn subtract(spans: Vec<(f32, f32)>, blocked: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut result = spans;
    for &(a, b) in blocked {
        result = result
            .into_iter()
            .flat_map(|(l, r)| {
                let mut next = vec![];
                if b <= l || a >= r {
                    next.push((l, r));
                } else {
                    if a > l {
                        next.push((l, a));
                    }
                    if b < r {
                        next.push((b, r));
                    }
                }
                next
            })
            .collect();
    }
    result
}
fn intersect(spans: Vec<(f32, f32)>, inside: &[(f32, f32)]) -> Vec<(f32, f32)> {
    spans
        .into_iter()
        .flat_map(|(a, b)| {
            inside.iter().filter_map(move |&(l, r)| {
                let lo = a.max(l);
                let hi = b.min(r);
                (hi > lo).then_some((lo, hi))
            })
        })
        .collect()
}

pub fn line_spans(
    bounds: Bounds,
    top: f32,
    bottom: f32,
    obstacles: &[Obstacle],
) -> Vec<(f32, f32)> {
    let mut spans = vec![(bounds.min.x, bounds.max.x)];
    for object in obstacles {
        if object.wrap.ignore_locked && object.locked {
            continue;
        }
        let [t, r, b, l] = object.wrap.offset;
        let active = top < object.bounds.max.y + b && bottom > object.bounds.min.y - t;
        if !active {
            if object.wrap.invert {
                spans.clear();
            }
            continue;
        }
        let occupied = match object.wrap.mode {
            WrapMode::None => continue,
            WrapMode::BoundingBox | WrapMode::JumpObject => {
                vec![(object.bounds.min.x - l, object.bounds.max.x + r)]
            }
            WrapMode::ObjectShape => {
                let offset = t.max(r).max(b).max(l).max(0.);
                let mut occupied = vec![];
                // Union the occupied spans through the full line band, so tall glyphs
                // do not collide with a sloped/curved edge between sample baselines.
                let steps = ((bottom - top + 2. * offset).ceil() as usize).clamp(2, 128);
                for step in 0..=steps {
                    let y =
                        top - offset + (bottom - top + 2. * offset) * step as f32 / steps as f32;
                    let at_y = union_spans(
                        object
                            .contours
                            .iter()
                            .flat_map(|contour| {
                                polygon_intervals(contour, y)
                                    .into_iter()
                                    .map(|(a, b)| (a - offset, b + offset))
                            })
                            .collect(),
                    );
                    if object.wrap.invert {
                        occupied = if step == 0 {
                            at_y
                        } else {
                            intersect(occupied, &at_y)
                        };
                    } else {
                        occupied.extend(at_y);
                    }
                }
                union_spans(occupied)
            }
        };
        if object.wrap.mode == WrapMode::JumpObject && !object.wrap.invert {
            return vec![];
        }
        spans = if object.wrap.invert {
            intersect(spans, &occupied)
        } else {
            subtract(spans, &occupied)
        };
    }
    spans.into_iter().filter(|(a, b)| b - a >= 1.).collect()
}

fn sliced_run(story: &TypeRun, start: usize) -> TypeRun {
    let mut run = story.clone();
    run.origin = Pt::ZERO;
    run.frame = None;
    run.thread = None;
    run.on_path = None;
    run.layout = None;
    run.contours.clear();
    run.content = story.content.chars().skip(start).collect();
    run.manual_kern = story
        .manual_kern
        .range(start.saturating_add(1)..)
        .map(|(&index, &value)| (index - start, value))
        .collect();
    let mut active = crate::text::paragraph_style(story, start);
    active.start = 0;
    run.paragraphs = vec![active];
    run.paragraphs.extend(
        story
            .paragraphs
            .iter()
            .filter(|p| p.start > start)
            .cloned()
            .map(|mut p| {
                p.start -= start;
                p
            }),
    );
    run.spans = story
        .spans
        .iter()
        .filter(|s| s.end > start)
        .cloned()
        .map(|mut s| {
            s.start = s.start.saturating_sub(start);
            s.end -= start;
            s
        })
        .collect();
    run
}

/// Ellipsis candidates follow paragraph boundaries and extended grapheme clusters.
fn ellipsized(run: &TypeRun, text: &str, width: f32, style: &ParagraphStyle) -> String {
    let mut opportunities = match style.break_mode {
        BreakMode::Word => text
            .unicode_word_indices()
            .map(|(i, _)| i)
            .collect::<Vec<_>>(),
        BreakMode::Normal => unicode_linebreak::linebreaks(text)
            .map(|(i, _)| i)
            .collect(),
        _ => text.grapheme_indices(true).map(|(i, _)| i).collect(),
    };
    opportunities.push(0);
    opportunities.push(text.len());
    opportunities.sort_unstable();
    opportunities.dedup();
    let graphemes: std::collections::HashSet<_> = text
        .grapheme_indices(true)
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .collect();
    let mut candidate = run.clone();
    candidate.wrap_width = None;
    candidate.paragraphs.clear();
    candidate.align = TextAlign::Start;
    candidate.content = "…".into();
    let mark = if crate::text::compose(&candidate)
        .first()
        .is_some_and(|line| line.glyphs.iter().all(|glyph| glyph.id != 0))
    {
        "…"
    } else {
        "..."
    };
    for end in opportunities
        .into_iter()
        .rev()
        .filter(|i| graphemes.contains(i))
    {
        let prefix = text[..end].trim_end_matches(|c: char| {
            c.is_whitespace() || matches!(c, ',' | '.' | ';' | ':' | '!' | '?' | '–' | '—' | '-')
        });
        candidate.content = format!("{prefix}{mark}");
        if crate::text::compose(&candidate)
            .first()
            .is_some_and(|line| line.width <= width + 0.01)
        {
            return candidate.content;
        }
    }
    mark.into()
}

struct PlacedLine {
    run: TypeRun,
    line: crate::text::LayoutLine,
    start: usize,
    end: usize,
    x: f32,
    y: f32,
    width: f32,
    row: usize,
    descent: f32,
}
fn line_metrics(run: &TypeRun, line: &crate::text::LayoutLine) -> (f32, f32) {
    let mut top = line.baseline - run.px * 0.85;
    let mut bottom = line.baseline + run.px * 0.2;
    for glyph in &line.glyphs {
        for point in crate::text::glyph_contours(run, glyph)
            .into_iter()
            .flatten()
        {
            top = top.min(point.y);
            bottom = bottom.max(point.y);
        }
    }
    (line.baseline - top, bottom - line.baseline)
}

pub fn layout_frame(
    frame_run: &TypeRun,
    story: &TypeRun,
    start: usize,
    obstacles: &[Obstacle],
    has_next: bool,
) -> TextGeometryLayout {
    layout_frame_continued(frame_run, story, start, obstacles, has_next, 0)
}

#[derive(Default)]
struct FlowMetrics { top: f32, bottom: f32, rows: usize }

fn layout_frame_continued(
    frame_run: &TypeRun,
    story: &TypeRun,
    start: usize,
    obstacles: &[Obstacle],
    has_next: bool,
    initial_hyphens: usize,
) -> TextGeometryLayout {
    let mut metrics=FlowMetrics::default();
    let Some(frame)=frame_run.frame.as_ref() else { return TextGeometryLayout::default(); };
    if frame.valign==VAlign::Top || (obstacles.is_empty() && frame.contour.is_none()) {
        return layout_frame_pass(frame_run,story,start,obstacles,has_next,initial_hyphens,None,&mut metrics);
    }
    // Obstacle intervals depend on the baseline. Reflow at the aligned position
    // instead of translating glyphs after their collision-free spans were chosen.
    let height=(frame.size.y-frame.inset[0]-frame.inset[2]).max(0.);
    let mut best=layout_frame_pass(frame_run,story,start,obstacles,has_next,initial_hyphens,Some((0.,0.)),&mut metrics);
    if metrics.rows>0 && metrics.bottom>=height-0.01 {return best;}
    let initially_empty=metrics.rows==0;
    if initially_empty {
        let mut probe=frame_run.clone();probe.frame.as_mut().unwrap().contour=None;
        layout_frame_pass(&probe,story,start,&[],has_next,initial_hyphens,Some((0.,0.)),&mut metrics);
        if metrics.rows==0 {return best;}
    }
    let required_end=best.visible_end;
    let score=|m:&FlowMetrics|match frame.valign {VAlign::Center=>(height-m.bottom-m.top).abs(),VAlign::Bottom|VAlign::Justify=>(height-m.bottom).abs(),VAlign::Top=>0.};
    let mut best_score=if initially_empty {f32::INFINITY}else{score(&metrics)};
    let mut offset=0.;let mut gap=0.;let mut previous:Vec<(f32,f32)>=Vec::new();
    for _ in 0..8 {
        let (mut next_offset,mut next_gap)=match frame.valign {
            VAlign::Center=>((offset+(height-metrics.bottom-metrics.top)*0.5).clamp(-story.line_height(),height),0.),
            VAlign::Bottom=>((offset+height-metrics.bottom).clamp(-story.line_height(),height),0.),
            VAlign::Justify if metrics.rows>1=>(0.,(gap+(height-metrics.bottom)/(metrics.rows-1) as f32).clamp(0.,story.line_height()*3.)),
            _=>break,
        };
        if previous.iter().any(|&(x,y)|(x-next_offset).abs()<0.1&&(y-next_gap).abs()<0.1) {next_offset=(next_offset+offset)*0.5;next_gap=(next_gap+gap)*0.5;}
        if (next_offset-offset).abs()<0.05 && (next_gap-gap).abs()<0.05 {break;}
        previous.push((next_offset,next_gap));
        let (old_offset,old_gap)=(offset,gap);
        offset=next_offset;gap=next_gap;
        let mut trial_metrics=FlowMetrics::default();
        let mut trial=layout_frame_pass(frame_run,story,start,obstacles,has_next,initial_hyphens,Some((offset,gap)),&mut trial_metrics);
        // Tight obstacle bands can add lines. Back off until alignment retains
        // every character that the top-aligned frame could display.
        for _ in 0..6 {
            if trial_metrics.rows>0 && trial.visible_end>=required_end && trial_metrics.bottom<=height+0.01 {break;}
            offset=(offset+old_offset)*0.5;gap=(gap+old_gap)*0.5;
            trial=layout_frame_pass(frame_run,story,start,obstacles,has_next,initial_hyphens,Some((offset,gap)),&mut trial_metrics);
        }
        if trial_metrics.rows>0 && trial.visible_end>=required_end && trial_metrics.bottom<=height+0.01 {
            let trial_score=score(&trial_metrics);
            if trial.visible_end>best.visible_end || (trial.visible_end==best.visible_end && trial_score<best_score) {best_score=trial_score;best=trial;}
            metrics=trial_metrics;
            if best_score<0.1 {break;}
        } else {break;}
    }
    best
}

fn layout_frame_pass(
    frame_run: &TypeRun,
    story: &TypeRun,
    start: usize,
    obstacles: &[Obstacle],
    has_next: bool,
    initial_hyphens: usize,
    vertical: Option<(f32,f32)>,
    metrics: &mut FlowMetrics,
) -> TextGeometryLayout {
    let Some(frame) = frame_run.frame.as_ref() else {
        return TextGeometryLayout::default();
    };
    let bounds = frame_bounds(frame_run).unwrap();
    let inner = Bounds {
        min: bounds.min + Pt::new(frame.inset[3], frame.inset[0]),
        max: bounds.max - Pt::new(frame.inset[1], frame.inset[2]),
    };
    let mut frame_obstacles = obstacles.to_vec();
    if let Some(contour) = &frame.contour {
        let contours = contour
            .iter()
            .map(|poly| {
                poly.iter()
                    .map(|p| bounds.min + Pt::new(p.x * frame.size.x, p.y * frame.size.y))
                    .collect()
            })
            .collect();
        frame_obstacles.insert(
            0,
            Obstacle {
                id: 0,
                order: (0, 0),
                bounds,
                contours,
                wrap: TextWrap {
                    mode: WrapMode::ObjectShape,
                    invert: true,
                    ..Default::default()
                },
                locked: false,
            },
        );
    }
    let obstacles = &frame_obstacles;
    let total = story.content.chars().count();
    let mut index = start.min(total);
    let mut row = 0usize;
    let mut previous_baseline: Option<f32> = None;
    let mut previous_had_text = false;
    let mut placed = vec![];
    let mut consecutive = initial_hyphens;
    let mut visible_rows = 0usize;
    let mode = if has_next {
        Overflow::Flow
    } else if frame.overflow == Overflow::Flow {
        if frame.terminal_overflow == Overflow::Ellipsis {
            Overflow::Ellipsis
        } else {
            Overflow::Clip
        }
    } else {
        frame.overflow
    };
    let height_limit = frame.contour.is_some() || (!frame.auto_height && mode != Overflow::Visible);
    let flow_bottom = obstacles
        .iter()
        .filter(|obstacle| obstacle.wrap.invert)
        .map(|obstacle| obstacle.bounds.max.y + obstacle.wrap.offset[2])
        .fold(f32::INFINITY, f32::min);
    if inner.width() <= 0. || (height_limit && inner.height() <= 0.) {
        return TextGeometryLayout {
            overflow: start < total,
            visible_start: start,
            visible_end: start,
            carets: vec![(start, frame_run.origin, Pt::new(1., 0.))],
            ..Default::default()
        };
    }
    let mut horizontal_overflow = false;
    // Empty stories still expose an insertion caret.
    'rows: while index < total && row < 100000 {
        if frame
            .max_lines
            .is_some_and(|max| visible_rows >= max as usize)
        {
            break;
        }
        let mut probe = sliced_run(story, index);
        probe.wrap_width = Some(inner.width().max(1.));
        let composed = crate::text::compose(&probe);
        let Some(first) = composed.first() else { break };
        let (ascent, descent) = line_metrics(&probe, first);
        let line_height = first.height.max(ascent + descent);
        let (offset,gap)=vertical.unwrap_or((0.,0.));
        let y = previous_baseline.map_or(inner.min.y + ascent + offset, |baseline| baseline + line_height + if previous_had_text { gap } else { 0. });
        let row_top = y - ascent;
        if row_top < inner.min.y - 0.001 {
            previous_had_text=false;previous_baseline=Some(y);row+=1;continue;
        }
        if row_top > flow_bottom {
            break;
        }
        if height_limit && y + descent > inner.max.y + 0.001 {
            break;
        }
        let spans = line_spans(inner, row_top, y + descent, obstacles);
        if spans.is_empty() {
            previous_had_text = false;
            previous_baseline = Some(y);
            row += 1;
            continue;
        }
        let placed_before_row = placed.len();
        for (left, right) in spans {
            if index >= total {
                break;
            }
            let mut local = sliced_run(story, index);
            local.wrap_width = Some((right - left).max(1.));
            if let Some(style) = local.paragraphs.first_mut()
                && let Some(hyphen) = style.hyphen.as_ref()
                && hyphen.max_consecutive != 0
                && consecutive >= hyphen.max_consecutive
            {
                style.hyphenate = false;
            }
            let composed = crate::text::compose(&local);
            let Some(line) = composed.first().cloned() else {
                break;
            };
            // An unbreakable word may overflow the composer measure. A narrow
            // interval beside an obstacle is optional: defer the word until a
            // wider interval instead of consuming and clipping its characters.
            if line.width > right - left + 0.01 && right - left < inner.width() - 0.01 {
                continue;
            }
            let (_ascent, descent) = line_metrics(&local, &line);
            let baseline = y;
            if height_limit && baseline + descent > inner.max.y + 0.001 {
                break 'rows;
            }
            let count = line.end.min(total - index);
            let newline = story.content.chars().nth(index + count) == Some('\n');
            let consumed = count + usize::from(newline);
            if consumed == 0 {
                break 'rows;
            }
            horizontal_overflow |= line.width > right - left + 0.01;
            let end = (index + consumed).min(total);
            consecutive = if line.hyphenated { consecutive + 1 } else { 0 };
            placed.push(PlacedLine {
                run: local,
                line,
                start: index,
                end,
                x: left,
                y: baseline,
                width: right - left,
                row,
                descent,
            });
            index = end;
            if newline {
                break;
            }
        }
        previous_had_text = placed.len() > placed_before_row;
        previous_baseline = Some(y);
        row += 1;
        visible_rows += usize::from(previous_had_text);
    }
    // Respect paragraph boundaries when advancing to another frame. A paragraph
    // taller than an empty frame still advances; it must never create a flow loop.
    if has_next && index < total && !placed.is_empty() {
        let style = crate::text::paragraph_style(story, index);
        let first = placed
            .iter()
            .position(|line| line.start >= style.start)
            .unwrap_or(placed.len());
        let here = placed
            .get(first)
            .map_or(0, |first| placed.last().unwrap().row - first.row + 1);
        let mut tail = sliced_run(story, index);
        tail.wrap_width = Some(inner.width());
        let after = crate::text::compose(&tail)
            .iter()
            .take_while(|line| line.paragraph == 0)
            .count();
        let (edge_start, edge_end) = match style.keep_together {
            crate::geom::KeepTogether::Edges { first, last } => (first as usize, last as usize),
            _ => (0, 0),
        };
        let minimum_start = edge_start.max(if style.allow_orphans {
            0
        } else {
            style.min_start_lines as usize
        });
        let minimum_end = edge_end.max(if style.allow_orphans {
            0
        } else {
            style.min_end_lines as usize
        });
        let mut keep = placed.len();
        if here > 0 {
            let entire = matches!(style.keep_together, crate::geom::KeepTogether::All);
            if first > 0 && (entire || here < minimum_start) {
                keep = first;
            } else if after < minimum_end {
                let move_rows = minimum_end - after;
                if here > move_rows && here - move_rows >= minimum_start {
                    let row = placed.last().unwrap().row + 1 - move_rows;
                    keep = placed.iter().position(|l| l.row >= row).unwrap_or(keep);
                } else if first > 0 {
                    keep = first;
                }
            }
        }
        // Numeric keep-with-next means N following lines, not just a nonempty tail.
        if first > 0
            && let Some(previous_line) = placed.get(first - 1)
        {
            let previous = crate::text::paragraph_style(story, previous_line.start);
            if previous.keep_with_next as usize > here {
                let previous_first = placed
                    .iter()
                    .position(|l| l.start >= previous.start)
                    .unwrap_or(0);
                if previous_first > 0 {
                    keep = keep.min(previous_first);
                }
            }
        }
        if keep < placed.len() {
            index = placed[keep].start;
            placed.truncate(keep);
        }
    }
    let vertical_overflow = !frame.auto_height
        && placed
            .last()
            .is_some_and(|last| last.y + last.descent > inner.max.y + 0.001);
    let overflow = index < total || horizontal_overflow || vertical_overflow;
    if overflow
        && mode == Overflow::Ellipsis
        && let Some(last) = placed.last_mut()
    {
        let source = story
            .content
            .chars()
            .skip(last.start)
            .take_while(|c| *c != '\n')
            .collect::<String>();
        let style = crate::text::paragraph_style(story, last.start);
        last.run.content = ellipsized(&last.run, &source, last.width, &style);
        last.run.wrap_width = None;
        last.run.paragraphs.clear();
        last.run.align = style.align;
        if let Some(line) = crate::text::compose(&last.run).first() {
            last.line = line.clone();
        }
    }
    let occupied = placed
        .last()
        .map(|l| l.y + l.descent - inner.min.y)
        .unwrap_or(0.);
    let free = (inner.height() - occupied).max(0.);
    metrics.top=placed.first().map(|line|line.y-line_metrics(&line.run,&line.line).0-inner.min.y).unwrap_or(0.);
    metrics.bottom=occupied;
    metrics.rows=placed.iter().enumerate().filter(|(i,line)|*i==0||placed[*i-1].row!=line.row).count();
    let align = if vertical.is_some() || (mode == Overflow::Visible && occupied > inner.height()) {
        VAlign::Top
    } else {
        frame.valign
    };
    let rows = placed.last().map(|l| l.row + 1).unwrap_or(1);
    let tail_hyphens = placed
        .iter()
        .rev()
        .take_while(|p| p.line.hyphenated)
        .count();
    let hyphen_tail = tail_hyphens
        + if tail_hyphens == placed.len() {
            initial_hyphens
        } else {
            0
        };
    let mut layout = TextGeometryLayout {
        overflow,
        visible_start: start,
        visible_end: index,
        hyphen_tail,
        ..Default::default()
    };
    let mut emitted_line = false;
    for placed in placed {
        let dy = match align {
            VAlign::Top => 0.,
            VAlign::Center => free * 0.5,
            VAlign::Bottom => free,
            VAlign::Justify => {
                if rows > 1 {
                    free.min(story.line_height() * 3. * (rows - 1) as f32) * placed.row as f32
                        / (rows - 1) as f32
                } else {
                    0.
                }
            }
        };
        let base = Pt::new(placed.x, placed.y + dy - placed.line.baseline);
        for glyph in &placed.line.glyphs {
            for contour in crate::text::glyph_contours(&placed.run, glyph) {
                layout
                    .contours
                    .push(contour.into_iter().map(|p| p + base).collect());
            }
        }
        for &(i, x) in &placed.line.carets {
            let metrics = crate::text::character_metrics(
                story,
                (placed.start + i).min(total.saturating_sub(1)),
            );
            layout
                .caret_heights
                .push((placed.start + i, story.px * metrics.vscale.unwrap() / 100.));
            layout.carets.push((
                (placed.start + i).min(placed.end),
                Pt::new(
                    placed.x + x,
                    placed.y + dy - metrics.baseline_shift.unwrap(),
                ),
                Pt::new(1., 0.),
            ));
        }
        for pair in placed.line.carets.windows(2) {
            let (a, x) = pair[0];
            let (b, z) = pair[1];
            let metrics = crate::text::character_metrics(story, placed.start + a);
            let height = story.px * metrics.vscale.unwrap() / 100.;
            let baseline = placed.y + dy - metrics.baseline_shift.unwrap();
            let top = baseline - height * 0.9;
            let bottom = baseline + height * 0.2;
            layout.selections.push((
                placed.start + a,
                placed.start + b,
                [
                    Pt::new(placed.x + x, top),
                    Pt::new(placed.x + z, top),
                    Pt::new(placed.x + z, bottom),
                    Pt::new(placed.x + x, bottom),
                ],
            ));
        }
        if emitted_line {
            layout.visible_text.push('\n');
        }
        emitted_line = true;
        layout
            .visible_text
            .push_str(&placed.line.text.replace('\u{ad}', ""));
        if placed.line.hyphenated {
            layout.visible_text.push('-');
        }
    }
    if mode != Overflow::Visible {
        layout.contours = layout
            .contours
            .into_iter()
            .map(|contour| clip_contour(contour, bounds))
            .filter(|c| c.len() >= 3)
            .collect();
    }
    if layout.carets.is_empty() {
        layout.carets.push((
            start,
            Pt::new(inner.min.x, inner.min.y + story.px * 0.85),
            Pt::new(1., 0.),
        ));
    }
    let mut visible = sliced_run(story, start);
    visible.content = story
        .content
        .chars()
        .skip(start)
        .take(index - start)
        .collect();
    if overflow && mode == Overflow::Ellipsis {
        visible.content = layout.visible_text.clone();
    }
    let count = visible.content.chars().count();
    visible.spans.retain(|span| span.start < count);
    for span in &mut visible.spans {
        span.end = span.end.min(count);
    }
    visible
        .paragraphs
        .retain(|paragraph| paragraph.start < count);
    visible.manual_kern.retain(|&index, _| index < count);
    layout.visible_run = Some(Box::new(visible));
    layout
}

pub fn shape_frame(run: &TypeRun) -> Vec<Vec<Pt>> {
    layout_frame(run, run, 0, &[], false).contours
}

fn layer_wrap_geometry(kind: &crate::document::LayerKind) -> (Option<Bounds>, Vec<Vec<Pt>>) {
    let mut contours = vec![];
    let mut bounds: Option<Bounds> = None;
    if let Some(b) = kind.raster_bounds() {
        bounds = Some(b);
        if let crate::document::LayerKind::Raster {
            pixels,
            origin,
            size,
            rotation,
            ..
        } = kind
        {
            let size = if size.x == 0. || size.y == 0. {
                Pt::new(pixels.w as f32, pixels.h as f32)
            } else {
                *size
            };
            let center = *origin + size * 0.5;
            // Keep every alpha row: thin opaque details must still repel text.
            for y in 0..pixels.h {
                let mut x = 0;
                while x < pixels.w {
                    while x < pixels.w && pixels.data[((y * pixels.w + x) * 4 + 3) as usize] < 16 {
                        x += 1;
                    }
                    let start = x;
                    while x < pixels.w && pixels.data[((y * pixels.w + x) * 4 + 3) as usize] >= 16 {
                        x += 1;
                    }
                    if x > start {
                        let a = *origin
                            + Pt::new(
                                start as f32 / pixels.w as f32 * size.x,
                                y as f32 / pixels.h as f32 * size.y,
                            );
                        let b = *origin
                            + Pt::new(
                                x as f32 / pixels.w as f32 * size.x,
                                (y + 1) as f32 / pixels.h as f32 * size.y,
                            );
                        contours.push(
                            vec![a, Pt::new(b.x, a.y), b, Pt::new(a.x, b.y)]
                                .into_iter()
                                .map(|p| p.rotate_about(center, *rotation))
                                .collect(),
                        );
                    }
                }
            }
        }
    } else {
        for shape in kind
            .shapes()
            .unwrap_or(&[])
            .iter()
            .filter(|shape| shape.visible)
        {
            let b = shape.world_bbox();
            if let Some(old) = &mut bounds {
                old.union_pt(b.min);
                old.union_pt(b.max);
            } else {
                bounds = Some(b);
            }
            contours.extend(shape.world_contours(96));
        }
    }
    (bounds, contours)
}

pub fn collect_obstacles(doc: &Document) -> Vec<Obstacle> {
    let mut result = vec![];
    for (li, layer) in doc
        .layers
        .iter()
        .enumerate()
        .filter(|(li, _)| doc.layer_visible(*li))
    {
        let locked = layer.locked
            || doc
                .layer_ancestors(li)
                .iter()
                .any(|&index| doc.layers[index].locked);
        for (si, shape) in layer
            .kind
            .shapes()
            .unwrap_or(&[])
            .iter()
            .enumerate()
            .filter(|(_, shape)| shape.visible && !shape.text_wrap.is_none())
        {
            result.push(Obstacle {
                id: shape.id,
                order: (li, si),
                bounds: shape.world_bbox(),
                contours: shape.world_contours(96),
                wrap: shape.text_wrap.clone(),
                locked: locked || shape.locked,
            });
        }
        if layer.text_wrap.is_none() {
            continue;
        }
        let mut bounds: Option<Bounds> = None;
        let mut contours = vec![];
        for (child_index, child) in doc.layers.iter().enumerate().filter(|(index, child)| {
            doc.layer_visible(*index)
                && (child.id == layer.id
                    || doc
                        .layer_ancestors(*index)
                        .iter()
                        .any(|&i| doc.layers[i].id == layer.id))
        }) {
            let (child_bounds, child_contours) = layer_wrap_geometry(&child.kind);
            if let Some(b) = child_bounds {
                if let Some(old) = &mut bounds {
                    old.union_pt(b.min);
                    old.union_pt(b.max);
                } else {
                    bounds = Some(b);
                }
            }
            contours.extend(child_contours);
            let _ = child_index;
        }
        if let Some(bounds) = bounds {
            result.push(Obstacle {
                id: layer.id,
                order: (li, usize::MAX),
                bounds,
                contours,
                wrap: layer.text_wrap.clone(),
                locked,
            });
        }
    }
    result
}

pub fn reflow_areas(doc: &mut Document) {
    let obstacles = collect_obstacles(doc);
    let frames: HashMap<_, _> = doc
        .layers
        .iter()
        .enumerate()
        .flat_map(|(li, l)| {
            l.kind
                .shapes()
                .unwrap_or(&[])
                .iter()
                .enumerate()
                .filter_map(move |(si, s)| {
                    if let Geom::Text(t) = &s.geom
                        && t.frame.is_some()
                    {
                        Some((s.id, (li, si, t.clone())))
                    } else {
                        None
                    }
                })
        })
        .collect();
    let mut results = vec![];
    for (&id, (li, _, head)) in &frames {
        if head
            .thread
            .as_ref()
            .and_then(|t| t.prev)
            .is_some_and(|id| frames.contains_key(&id))
        {
            continue;
        }
        let mut seen = std::collections::HashSet::new();
        let mut next = Some(id);
        let mut start = 0;
        let mut hyphens = 0;
        while let Some(current) = next {
            if !seen.insert(current) {
                break;
            }
            let Some((layer, order, run)) = frames.get(&current) else {
                break;
            };
            let parents: std::collections::HashSet<_> = doc
                .layer_ancestors(*layer)
                .into_iter()
                .map(|index| doc.layers[index].id)
                .chain(std::iter::once(doc.layers[*layer].id))
                .collect();
            let relevant: Vec<_> = obstacles
                .iter()
                .filter(|o| {
                    o.id != current
                        && !parents.contains(&o.id)
                        && if doc.text_wrap_above_only {
                            o.order < (*layer, *order)
                        } else {
                            o.order > (*layer, *order)
                        }
                })
                .cloned()
                .collect();
            next = run
                .thread
                .as_ref()
                .and_then(|t| t.next)
                .filter(|id| frames.contains_key(id));
            let mut source =
                serde_json::to_string(&(run, head, start, next, hyphens)).unwrap_or_default();
            for obstacle in &relevant {
                source.push_str(&format!(
                    "{:?}{:?}{:?}",
                    obstacle.bounds, obstacle.contours, obstacle.wrap
                ));
            }
            let layout = if let Some(layout) =
                run.layout.as_ref().filter(|layout| layout.source == source)
            {
                layout.clone()
            } else {
                let mut layout =
                    layout_frame_continued(run, head, start, &relevant, next.is_some(), hyphens);
                layout.source = source;
                layout
            };
            start = layout.visible_end;
            hyphens = layout.hyphen_tail;
            results.push((*layer, current, layout));
        }
        let _ = li;
    }
    for (layer, id, layout) in results {
        if let Some(s) = doc.find_shape_mut(layer, id)
            && let Geom::Text(t) = &mut s.geom
        {
            if let Some(frame) = &mut t.frame {
                t.wrap_width = Some((frame.size.x - frame.inset[1] - frame.inset[3]).max(1.));
                if frame.auto_height && frame.contour.is_none() {
                    let bottom = layout
                        .contours
                        .iter()
                        .flatten()
                        .map(|p| p.y)
                        .fold(t.origin.y, f32::max);
                    frame.size.y = (bottom - (t.origin.y - t.px * 0.85) + frame.inset[2]).max(t.px);
                }
            }
            t.contours = layout.contours.clone();
            t.layout = Some(layout);
        }
    }
}

pub fn story_slice(story: &TypeRun, start: usize) -> TypeRun {
    sliced_run(story, start)
}
pub fn frame_snapshot(doc: &Document) -> Vec<(usize, Shape)> {
    doc.layers
        .iter()
        .enumerate()
        .flat_map(|(l, layer)| {
            layer
                .kind
                .shapes()
                .unwrap_or(&[])
                .iter()
                .filter(|s| matches!(&s.geom,Geom::Text(t) if t.frame.is_some()))
                .cloned()
                .map(move |s| (l, s))
        })
        .collect()
}

/// Reconnect surviving frames after deletion, including promotion of a new head.
pub fn reconcile_threads(before: &[(usize, Shape)], doc: &Document) -> Vec<Cmd> {
    let surviving: std::collections::HashSet<_> = doc
        .layers
        .iter()
        .filter_map(|l| l.kind.shapes())
        .flatten()
        .map(|s| s.id)
        .collect();
    let removed: std::collections::HashSet<_> = before
        .iter()
        .map(|(_, s)| s.id)
        .filter(|id| !surviving.contains(id))
        .collect();
    if removed.is_empty() {
        return vec![];
    }
    let old: HashMap<_, _> = before
        .iter()
        .filter_map(|(_, s)| {
            if let Geom::Text(t) = &s.geom {
                Some((s.id, t))
            } else {
                None
            }
        })
        .collect();
    let mut commands = vec![];
    for (layer, l) in doc.layers.iter().enumerate() {
        for shape in l.kind.shapes().unwrap_or(&[]) {
            let Geom::Text(run) = &shape.geom else {
                continue;
            };
            let Some(thread) = &run.thread else { continue };
            let mut after = run.clone();
            let mut link = thread.clone();
            for previous in [true, false] {
                let mut cursor = if previous { link.prev } else { link.next };
                let mut seen = std::collections::HashSet::new();
                while let Some(id) = cursor {
                    if !removed.contains(&id) {
                        break;
                    }
                    if !seen.insert(id) {
                        cursor = None;
                        break;
                    }
                    cursor = old
                        .get(&id)
                        .and_then(|t| t.thread.as_ref())
                        .and_then(|t| if previous { t.prev } else { t.next });
                }
                if previous {
                    link.prev = cursor;
                } else {
                    link.next = cursor;
                }
            }
            if removed.contains(&thread.story) {
                let mut head = shape.id;
                let mut cursor = link.prev;
                let mut seen = std::collections::HashSet::new();
                while let Some(id) = cursor {
                    if !seen.insert(id) {
                        break;
                    }
                    if surviving.contains(&id) {
                        head = id;
                    }
                    cursor = old
                        .get(&id)
                        .and_then(|t| t.thread.as_ref())
                        .and_then(|t| t.prev);
                }
                link.story = head;
                if shape.id == head
                    && let Some(original) = old.get(&thread.story)
                {
                    let top = after.origin.y - after.px * 0.85;
                    let origin = after.origin;
                    let frame = after.frame.clone();
                    after = (*original).clone();
                    after.origin = Pt::new(origin.x, top + after.px * 0.85);
                    after.frame = frame;
                }
            }
            if link != *thread {
                if link.next.is_none()
                    && let Some(frame) = &mut after.frame
                {
                    frame.overflow = frame.terminal_overflow;
                }
                after.thread = Some(link);
                after.layout = None;
                commands.push(Cmd::SetGeom {
                    layer,
                    id: shape.id,
                    before: shape.geom.clone(),
                    after: Geom::Text(after),
                    rot_before: shape.rotation,
                    rot_after: shape.rotation,
                });
            }
        }
    }
    commands
}

fn clip_contour(mut polygon: Vec<Pt>, bounds: Bounds) -> Vec<Pt> {
    for edge in 0..4 {
        let input = std::mem::take(&mut polygon);
        if input.is_empty() {
            break;
        }
        let inside = |p: Pt| match edge {
            0 => p.x >= bounds.min.x,
            1 => p.x <= bounds.max.x,
            2 => p.y >= bounds.min.y,
            _ => p.y <= bounds.max.y,
        };
        let intersection = |a: Pt, b: Pt| {
            let value = match edge {
                0 => bounds.min.x,
                1 => bounds.max.x,
                2 => bounds.min.y,
                _ => bounds.max.y,
            };
            let (x, y) = if edge < 2 { (a.x, b.x) } else { (a.y, b.y) };
            a.lerp(b, ((value - x) / (y - x)).clamp(0., 1.))
        };
        let mut a = *input.last().unwrap();
        for b in input {
            let (ai, bi) = (inside(a), inside(b));
            if ai != bi {
                polygon.push(intersection(a, b));
            }
            if bi {
                polygon.push(b);
            }
            a = b;
        }
    }
    polygon
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run() -> TypeRun {
        TypeRun{origin:Pt::new(20.,37.),content:"The editor keeps all of this original story while its frames change width and height. It flows through every frame without losing the final sentence.".into(),px:20.,frame:Some(TextFrame{size:Pt::new(180.,62.),..Default::default()}),..Default::default()}
    }
    #[test]
    fn visible_clip_ellipsis_and_max_lines_never_truncate_source() {
        let mut text = run();
        let source = text.content.clone();
        let clip = layout_frame(&text, &text, 0, &[], false);
        assert!(clip.overflow);
        assert!(clip.visible_end < source.chars().count());
        assert!(clip.contours.iter().flatten().all(|p| p.y <= 82.001));
        text.frame.as_mut().unwrap().overflow = Overflow::Visible;
        let visible = layout_frame(&text, &text, 0, &[], false);
        assert!(visible.overflow);
        assert_eq!(visible.visible_end, source.chars().count());
        assert!(visible.contours.iter().flatten().any(|p| p.y > 82.));
        text.frame.as_mut().unwrap().overflow = Overflow::Ellipsis;
        text.frame.as_mut().unwrap().max_lines = Some(1);
        let ellipsis = layout_frame(&text, &text, 0, &[], false);
        assert!(ellipsis.visible_text.ends_with('…'));
        assert!(ellipsis.overflow);
        assert_eq!(text.content, source);
    }
    #[test]
    fn ellipsis_respects_graphemes_and_word_boundary() {
        let text = run();
        let style = ParagraphStyle {
            break_mode: BreakMode::Word,
            ..Default::default()
        };
        let result = ellipsized(&text, "one enormous encyclopedia", 90., &style);
        assert!(result == "one…" || result == "…");
        let style = ParagraphStyle {
            break_mode: BreakMode::BreakAll,
            ..Default::default()
        };
        let result = ellipsized(&text, "e\u{301}e\u{301}e\u{301}e\u{301}", 35., &style);
        let prefix = result.trim_end_matches('…');
        assert!(prefix.is_empty() || prefix.ends_with('\u{301}'));
    }
    #[test]
    fn vertical_alignment_uses_visible_lines() {
        let mut text = run();
        text.content = "short".into();
        text.frame.as_mut().unwrap().size.y = 200.;
        let top = layout_frame(&text, &text, 0, &[], false).carets[0].1.y;
        text.frame.as_mut().unwrap().valign = VAlign::Center;
        let center = layout_frame(&text, &text, 0, &[], false).carets[0].1.y;
        text.frame.as_mut().unwrap().valign = VAlign::Bottom;
        let bottom = layout_frame(&text, &text, 0, &[], false).carets[0].1.y;
        assert!(center > top + 50. && bottom > center + 50.);
    }
    fn obstacle(geom: Geom) -> Obstacle {
        let shape = Shape::new(geom, Default::default());
        Obstacle {
            id: shape.id,
            order: (1, 1),
            bounds: shape.world_bbox(),
            contours: shape.world_contours(96),
            wrap: TextWrap {
                mode: WrapMode::ObjectShape,
                ..Default::default()
            },
            locked: false,
        }
    }
    #[test]
    fn vertical_alignment_reflows_around_obstacles_at_final_baselines() {
        for align in [VAlign::Center,VAlign::Bottom,VAlign::Justify] {
            let mut text=run();text.font=concat!(env!("CARGO_MANIFEST_DIR"),"/tests/assets/fonts/EBGaramond.ttf").into();
            text.content="One two three four\nFive six seven eight".into();
            text.frame.as_mut().unwrap().size=Pt::new(300.,300.);
            let bounds=frame_bounds(&text).unwrap();
            let blocker=obstacle(Geom::Rect{origin:bounds.min+Pt::new(80.,if align==VAlign::Bottom{230.}else{80.}),size:Pt::new(140.,90.),radius:0.});
            let top=layout_frame(&text,&text,0,&[blocker.clone()],false);
            text.frame.as_mut().unwrap().valign=align;
            let legacy=layout_frame_pass(&text,&text,0,&[blocker.clone()],false,0,None,&mut FlowMetrics::default());
            assert!(legacy.contours.iter().flatten().any(|p|blocker.bounds.contains(*p)),"fixture must expose the old {align:?} post-layout translation collision");
            let timed=std::time::Instant::now();
            let layout=layout_frame(&text,&text,0,&[blocker.clone()],false);
            eprintln!("wrapped {align:?} alignment: {:?}",timed.elapsed());
            assert_eq!(layout.visible_end,text.content.chars().count(),"{align:?} must preserve the whole short story");
            assert!(layout.contours.iter().flatten().all(|p|!blocker.bounds.contains(*p)),"{align:?} shifted glyphs into the wrap object");
            let extent=|layout:&TextGeometryLayout|layout.contours.iter().flatten().map(|p|p.y).fold(f32::NEG_INFINITY,f32::max);
            assert!(extent(&layout)>extent(&top)+10.,"{align:?} must still move the text vertically");
        }
    }
    #[test]
    fn aligned_closed_frames_and_line_limits_preserve_vertical_semantics() {
        let mut text=run();text.font=concat!(env!("CARGO_MANIFEST_DIR"),"/tests/assets/fonts/EBGaramond.ttf").into();text.content="Short text\nMore words".into();
        text.frame.as_mut().unwrap().size=Pt::new(300.,300.);
        text.frame.as_mut().unwrap().contour=Some(vec![vec![Pt::ZERO,Pt::new(1.,0.),Pt::splat(1.),Pt::new(0.,1.)]]);
        let bounds=frame_bounds(&text).unwrap();
        for align in [VAlign::Center,VAlign::Bottom] {
            text.frame.as_mut().unwrap().valign=align;
            let layout=layout_frame(&text,&text,0,&[],false);
            assert_eq!(layout.visible_end,text.content.chars().count());
            let top=layout.carets.iter().map(|(_,p,_)|p.y-17.).fold(f32::INFINITY,f32::min)-bounds.min.y;
            let bottom=bounds.max.y-layout.carets.iter().map(|(_,p,_)|p.y+4.).fold(f32::NEG_INFINITY,f32::max);
            assert!(if align==VAlign::Center {(top-bottom).abs()<0.5}else{bottom.abs()<0.5},"{align:?}: top={top}, bottom={bottom}");
        }
        text.frame.as_mut().unwrap().contour=None;text.frame.as_mut().unwrap().max_lines=Some(1);
        let blocker=obstacle(Geom::Rect{origin:bounds.min+Pt::new(110.,100.),size:Pt::new(60.,80.),radius:0.});
        text.frame.as_mut().unwrap().valign=VAlign::Center;
        let limited=layout_frame(&text,&text,0,&[blocker.clone()],false);
        assert!(limited.overflow);assert!(limited.visible_end<text.content.chars().count());
        assert!(limited.carets.iter().all(|(_,p,_)|(p.y-limited.carets[0].1.y).abs()<0.01));
        text.content="Short words remain visible beyond the frame. ".repeat(20);text.frame.as_mut().unwrap().max_lines=None;text.frame.as_mut().unwrap().overflow=Overflow::Visible;
        let visible=layout_frame(&text,&text,0,&[blocker.clone()],false);text.frame.as_mut().unwrap().valign=VAlign::Top;
        let top=layout_frame(&text,&text,0,&[blocker],false);
        assert_eq!(visible.contours,top.contours);assert_eq!(visible.visible_end,text.content.chars().count());
    }
    #[test]
    fn tapered_frame_alignment_finds_baselines_between_top_scan_rows() {
        for content in ["communication","words"] {
            let mut text=run();text.font=concat!(env!("CARGO_MANIFEST_DIR"),"/tests/assets/fonts/EBGaramond.ttf").into();text.content=content.into();
            let width=crate::text::measure(&TypeRun{frame:None,wrap_width:None,..text.clone()}).0;
            text.frame.as_mut().unwrap().size=Pt::new(if content=="words" {width*2.}else{width*1.25},50.);
            text.frame.as_mut().unwrap().contour=Some(Geom::Ellipse{center:Pt::splat(0.5),radii:Pt::splat(0.5)}.contours(128));
            text.frame.as_mut().unwrap().valign=VAlign::Center;
            let layout=layout_frame(&text,&text,0,&[],false);let bounds=frame_bounds(&text).unwrap();
            assert_eq!(layout.visible_end,content.chars().count(),"{content} must fit after baseline phase adjustment");
            let baseline=layout.carets[0].1.y;assert!((baseline-17.-bounds.min.y-14.5).abs()<0.5,"{content} centered baseline={baseline}");
        }
    }
    #[test]
    fn words_skip_narrow_obstacle_intervals_without_losing_characters() {
        let mut text=run();
        text.font=concat!(env!("CARGO_MANIFEST_DIR"),"/tests/assets/fonts/EBGaramond.ttf").into();
        text.content="Two remaining words stay intact".into();
        text.frame.as_mut().unwrap().size=Pt::new(220.,240.);
        let bounds=frame_bounds(&text).unwrap();
        let blocker=obstacle(Geom::Rect{origin:bounds.min+Pt::new(90.,0.),size:Pt::new(110.,100.),radius:0.});
        let layout=layout_frame(&text,&text,0,&[blocker],false);
        assert_eq!(layout.visible_end,text.content.chars().count());
        assert!(!layout.overflow);
        assert!(layout.carets.iter().all(|(_,point,_)|point.x<=bounds.max.x+0.01),"No consumed word may cross the frame edge from a narrow interval");
        let first=layout.carets.iter().find(|(index,_,_)|*index==0).unwrap().1;
        let next=layout.carets.iter().filter(|(index,_,_)|*index==4).last().unwrap().1;
        assert!(next.y>first.y,"The word must move below the first row instead of clipping in its narrow right interval");
    }
    #[test]
    fn rectangle_circle_offset_and_invert_compute_distinct_free_spans() {
        let bounds = Bounds::from_min_size(Pt::ZERO, Pt::new(200., 200.));
        let mut rectangle = obstacle(Geom::Rect {
            origin: Pt::new(70., 30.),
            size: Pt::new(60., 80.),
            radius: 0.,
        });
        assert_eq!(
            line_spans(bounds, 50., 65., &[rectangle.clone()]),
            vec![(0., 70.), (130., 200.)]
        );
        rectangle.wrap.offset = [10.; 4];
        assert_eq!(
            line_spans(bounds, 50., 65., &[rectangle.clone()]),
            vec![(0., 60.), (140., 200.)]
        );
        rectangle.wrap.invert = true;
        assert_eq!(
            line_spans(bounds, 50., 65., &[rectangle]),
            vec![(60., 140.)]
        );
        let circle = obstacle(Geom::Ellipse {
            center: Pt::new(100., 100.),
            radii: Pt::splat(50.),
        });
        let top = line_spans(bounds, 52., 60., &[circle.clone()]);
        let middle = line_spans(bounds, 95., 105., &[circle]);
        assert!(top[0].1 > middle[0].1 + 15.);
    }
    #[test]
    fn frame_resize_rewraps_and_keeps_font_size() {
        let mut geom = Geom::Text(run());
        let before = geom.bbox();
        geom.map_into(
            before,
            Bounds::from_min_size(before.min, Pt::new(90., 120.)),
        );
        let Geom::Text(text) = geom else { panic!() };
        assert_eq!(text.px, 20.);
        assert_eq!(text.frame.as_ref().unwrap().size, Pt::new(90., 120.));
        assert_eq!(text.wrap_width, Some(90.));
        assert_eq!(frame_bounds(&text).unwrap().min, before.min);
    }
    #[test]
    fn threads_reflow_delete_middle_and_undo_round_trip() {
        let mut studio = crate::app::Studio::new();
        studio.active_layer = Some(1);
        studio.text_px = 20.;
        let mut ids = vec![];
        for x in [20., 230., 440.] {
            studio.place_area_text(Bounds::from_min_size(Pt::new(x, 20.), Pt::new(180., 80.)));
            if ids.is_empty() {
                studio.type_insert(&run().content);
            }
            studio.commit_type_edit();
            ids.push(studio.selection[0]);
        }
        studio.thread_text_frames(ids[0], ids[1]).unwrap();
        studio.thread_text_frames(ids[1], ids[2]).unwrap();
        let text = |s: &crate::app::Studio, id: (usize, u64)| {
            let Geom::Text(t) = &s.doc.find_shape(id.0, id.1).unwrap().geom else {
                panic!()
            };
            t.clone()
        };
        assert!(text(&studio, ids[1]).content.is_empty());
        assert!(text(&studio, ids[1]).layout.as_ref().unwrap().visible_start > 0);
        let source = text(&studio, ids[0]).content;
        studio.selection = vec![ids[1]];
        studio.delete_selection();
        assert_eq!(text(&studio, ids[0]).thread.unwrap().next, Some(ids[2].1));
        assert_eq!(text(&studio, ids[2]).thread.unwrap().prev, Some(ids[0].1));
        assert_eq!(text(&studio, ids[0]).content, source);
        studio.undo();
        assert_eq!(text(&studio, ids[0]).thread.unwrap().next, Some(ids[1].1));
        let encoded = crate::project::encode(&studio.doc).unwrap();
        let restored = crate::project::decode(&encoded).unwrap();
        assert!(encoded.contains("\"version\":10"));
        assert!(
            matches!(&restored.find_shape(ids[0].0,ids[0].1).unwrap().geom,Geom::Text(t) if t.content==source)
        );
    }
    #[test]
    fn moving_obstacle_reflows_live_area_text() {
        let mut doc = Document::new("wrap", 500., 500., 96.);
        let mut text = run();
        text.frame.as_mut().unwrap().size = Pt::new(300., 300.);
        let shape = Shape::new(Geom::Text(text), Default::default());
        let id = shape.id;
        let mut circle = Shape::new(
            Geom::Ellipse {
                center: Pt::new(140., 100.),
                radii: Pt::splat(50.),
            },
            Default::default(),
        );
        circle.text_wrap = TextWrap {
            mode: WrapMode::ObjectShape,
            offset: [8.; 4],
            ..Default::default()
        };
        let circle_id = circle.id;
        doc.layers[1]
            .kind
            .shapes_mut()
            .unwrap()
            .extend([shape, circle]);
        reflow(&mut doc);
        let before = doc.find_shape(1, id).unwrap().geom.clone();
        doc.find_shape_mut(1, circle_id)
            .unwrap()
            .geom
            .translate(Pt::new(250., 0.));
        reflow(&mut doc);
        assert_ne!(before, doc.find_shape(1, id).unwrap().geom);
    }
    #[test]
    fn typed_story_from_second_frame_edits_head() {
        let mut studio = crate::app::Studio::new();
        studio.active_layer = Some(1);
        studio.text_px = 20.;
        studio.place_area_text(Bounds::from_min_size(Pt::ZERO, Pt::new(120., 50.)));
        studio.type_insert(&run().content);
        studio.commit_type_edit();
        let head = studio.selection[0];
        studio.place_area_text(Bounds::from_min_size(
            Pt::new(200., 0.),
            Pt::new(120., 150.),
        ));
        studio.commit_type_edit();
        let next = studio.selection[0];
        studio.thread_text_frames(head, next).unwrap();
        let point = if let Geom::Text(t) = &studio.doc.find_shape(next.0, next.1).unwrap().geom {
            t.layout.as_ref().unwrap().carets[0].1
        } else {
            panic!()
        };
        studio.begin_type_edit(next, point);
        assert_eq!(studio.type_edit.as_ref().unwrap().id, head.1);
        studio.type_insert("INSERTED ");
        assert!(
            matches!(&studio.doc.find_shape(head.0,head.1).unwrap().geom,Geom::Text(t) if t.content.contains("INSERTED"))
        );
    }
}

#[cfg(test)]
mod regression_tests {
    use super::*;
    fn frame(content: &str) -> TypeRun {
        TypeRun {
            content: content.into(),
            origin: Pt::new(0., 17.),
            px: 20.,
            frame: Some(TextFrame {
                size: Pt::new(150., 1000.),
                ..Default::default()
            }),
            ..Default::default()
        }
    }
    #[test]
    fn widow_minimum_moves_lines_even_when_paragraph_starts_frame() {
        let mut run = frame(
            "One two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty.",
        );
        let mut plain = run.clone();
        plain.frame = None;
        plain.wrap_width = Some(150.);
        let count = crate::text::compose(&plain).len();
        assert!(count > 5);
        run.frame.as_mut().unwrap().max_lines = Some(count as u32 - 1);
        run.paragraphs = vec![ParagraphStyle {
            min_start_lines: 2,
            min_end_lines: 3,
            allow_orphans: false,
            ..Default::default()
        }];
        let layout = layout_frame(&run, &run, 0, &[], true);
        let lines = layout.visible_text.lines().count();
        assert_eq!(lines, count - 3, "{layout:?}");
    }
    #[test]
    fn numeric_keep_with_next_requires_requested_following_lines() {
        let prefix = "Intro.\n";
        let heading = "A heading with length\n";
        let mut run = frame(&format!(
            "{prefix}{heading}One two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen."
        ));
        let body_start = (prefix.to_owned() + heading).chars().count();
        let mut heading_run = frame(heading.trim_end());
        heading_run.frame = None;
        heading_run.wrap_width = Some(150.);
        let heading_lines = crate::text::compose(&heading_run).len();
        run.frame.as_mut().unwrap().max_lines = Some(1 + heading_lines as u32 + 1);
        run.paragraphs = vec![
            ParagraphStyle::default(),
            ParagraphStyle {
                start: prefix.chars().count(),
                keep_with_next: 3,
                ..Default::default()
            },
            ParagraphStyle {
                start: body_start,
                ..Default::default()
            },
        ];
        let layout = layout_frame(&run, &run, 0, &[], true);
        assert_eq!(layout.visible_end, prefix.chars().count(), "{layout:?}");
    }
    fn threaded() -> (crate::app::Studio, Vec<(usize, u64)>, String) {
        let mut studio = crate::app::Studio::new();
        studio.active_layer = Some(1);
        studio.text_px = 22.;
        let source="First frame story starts here and flows into the next frame with several paragraphs and every original word intact. The final frame should retain all remaining text through any deletion or conversion.".to_owned();
        let mut ids = vec![];
        for x in [0., 200., 400.] {
            studio.place_area_text(Bounds::from_min_size(Pt::new(x, 0.), Pt::new(160., 65.)));
            if ids.is_empty() {
                studio.type_insert(&source);
            } else {
                studio.type_insert("");
            }
            studio.commit_type_edit();
            ids.push(studio.selection[0]);
        }
        studio.thread_text_frames(ids[0], ids[1]).unwrap();
        studio.thread_text_frames(ids[1], ids[2]).unwrap();
        let source = text(&studio, ids[0]).content;
        (studio, ids, source)
    }
    fn text(studio: &crate::app::Studio, id: (usize, u64)) -> TypeRun {
        let Geom::Text(run) = &studio.doc.find_shape(id.0, id.1).unwrap().geom else {
            panic!()
        };
        run.clone()
    }
    #[test]
    fn removing_story_head_promotes_style_and_updates_every_follower() {
        let (mut studio, ids, source) = threaded();
        studio.selection = vec![ids[0]];
        studio.patch_type(|run| {
            run.px = 29.;
            run.tracking = 1.5;
            run.liga = false;
        });
        studio.delete_selection();
        let head = text(&studio, ids[1]);
        assert_eq!(head.content, source);
        assert_eq!(head.px, 29.);
        assert_eq!(head.tracking, 1.5);
        assert!(!head.liga);
        assert_eq!(head.thread.as_ref().unwrap().story, ids[1].1);
        assert_eq!(text(&studio, ids[2]).thread.unwrap().story, ids[1].1);
        studio.undo();
        assert_eq!(text(&studio, ids[0]).content, source);
    }
    #[test]
    fn converting_middle_frame_preserves_all_three_stories() {
        let (mut studio, ids, source) = threaded();
        studio.selection = vec![ids[1]];
        studio.convert_text_frame(false);
        let parts: Vec<_> = ids.iter().map(|&id| text(&studio, id)).collect();
        assert!(parts[1].frame.is_none());
        assert!(parts[1].thread.is_none());
        assert_eq!(
            parts
                .iter()
                .map(|run| run.content.as_str())
                .collect::<String>(),
            source
        );
        assert_eq!(parts[0].thread.as_ref().unwrap().next, None);
        assert_eq!(parts[2].thread.as_ref().unwrap().prev, None);
        assert_eq!(parts[2].thread.as_ref().unwrap().story, ids[2].1);
    }
    #[test]
    fn consecutive_hyphen_limit_crosses_frame_boundary() {
        let mut run = frame(
            "extraordinary typography hyphenation internationalization characterization extraordinary typography",
        );
        run.frame.as_mut().unwrap().size.x = 80.;
        run.frame.as_mut().unwrap().max_lines = Some(1);
        run.paragraphs = vec![ParagraphStyle {
            hyphenate: true,
            hyphen: Some(crate::geom::HyphenSettings {
                max_consecutive: 1,
                min_before: 2,
                min_after: 2,
                ..Default::default()
            }),
            ..Default::default()
        }];
        let first = layout_frame_continued(&run, &run, 0, &[], true, 0);
        assert!(first.hyphen_tail > 0, "{first:?}");
        let second =
            layout_frame_continued(&run, &run, first.visible_end, &[], true, first.hyphen_tail);
        assert_eq!(second.hyphen_tail, 0, "{second:?}");
    }
}

#[cfg(test)]
mod shape_frame_tests {
    use super::*;
    #[test]
    fn arbitrary_closed_outline_constrains_every_glyph_and_roundtrips() {
        let mut studio = crate::app::Studio::new();
        studio.active_layer = Some(1);
        studio.text_px = 16.;
        let shape = Shape::new(
            Geom::Ellipse {
                center: Pt::new(160., 160.),
                radii: Pt::splat(100.),
            },
            Default::default(),
        );
        let id = shape.id;
        studio.doc.layers[1].kind.shapes_mut().unwrap().push(shape);
        studio.selection = vec![(1, id)];
        assert!(studio.can_text_inside_shape());
        studio.text_inside_shape();
        studio.type_insert(&"Typography flows inside a closed shape. ".repeat(12));
        studio.commit_type_edit();
        let (layer, id) = studio.selection[0];
        let Geom::Text(run) = &studio.doc.find_shape(layer, id).unwrap().geom else {
            panic!()
        };
        assert!(run.frame.as_ref().unwrap().contour.is_some());
        assert!(!run.contours.is_empty());
        assert!(
            run.contours
                .iter()
                .flatten()
                .all(|p| (*p - Pt::new(160., 160.)).length() <= 100.2)
        );
        let encoded = crate::project::encode(&studio.doc).unwrap();
        let restored = crate::project::decode(&encoded).unwrap();
        let Geom::Text(loaded) = &restored.find_shape(layer, id).unwrap().geom else {
            panic!()
        };
        assert_eq!(loaded.contours, run.contours);
    }
    #[test]
    fn nested_group_uses_visible_alpha_and_respects_locked_ancestors() {
        use crate::document::{Layer, Pixels};
        let mut doc = Document::new("group wrap", 400., 400., 96.);
        let mut group = Layer::group("Wrap group");
        group.text_wrap = TextWrap {
            mode: WrapMode::ObjectShape,
            ..Default::default()
        };
        let group_id = group.id;
        let mut nested = Layer::group("Nested");
        nested.parent = Some(group_id);
        let nested_id = nested.id;
        let mut pixels = Pixels::new(10, 10);
        for y in 0..10 {
            for x in 4..6 {
                pixels.data[((y * 10 + x) * 4 + 3) as usize] = 255;
            }
        }
        let mut raster =
            Layer::placed_raster("Alpha", pixels, Pt::new(100., 100.), Pt::new(100., 100.));
        raster.parent = Some(nested_id);
        doc.layers.extend([group, nested, raster]);
        let obstacles = collect_obstacles(&doc);
        let group = obstacles.iter().find(|o| o.id == group_id).unwrap();
        let bounds = Bounds::from_min_size(Pt::ZERO, Pt::new(400., 400.));
        assert_eq!(
            line_spans(bounds, 140., 155., &[group.clone()]),
            vec![(0., 140.), (160., 400.)]
        );
        doc.layers.last_mut().unwrap().visible = false;
        assert!(!collect_obstacles(&doc).iter().any(|o| o.id == group_id));
    }
}

#[cfg(test)]
mod range_metrics_tests {
    use super::*;
    #[test]
    fn area_uses_current_line_leading_and_stops_at_last_full_styled_line() {
        let mut run = TypeRun {
            content: "first\nsecond".into(),
            px: 20.,
            origin: Pt::new(0., 17.),
            frame: Some(TextFrame {
                size: Pt::new(250., 150.),
                ..Default::default()
            }),
            ..Default::default()
        };
        run.set_character_style(6, 12, |s| {
            s.leading = Some(crate::geom::Leading::Fixed(76.))
        });
        let layout = layout_frame(&run, &run, 0, &[], false);
        let first = layout.carets.iter().find(|(i, _, _)| *i == 0).unwrap().1.y;
        let second = layout.carets.iter().find(|(i, _, _)| *i == 6).unwrap().1.y;
        assert!((second - first - 76.).abs() < 0.01, "{first} {second}");
        run.frame.as_mut().unwrap().size.y = 80.;
        let clip = layout_frame(&run, &run, 0, &[], false);
        assert_eq!(clip.visible_end, 6);
        assert!(clip.overflow);
        run.frame.as_mut().unwrap().size.y = 150.;
        run.set_character_style(6, 12, |s| {
            s.vscale = Some(350.);
            s.baseline_shift = Some(-65.);
        });
        let clip = layout_frame(&run, &run, 0, &[], false);
        assert_eq!(clip.visible_end, 6, "tall shifted line must fit completely");
    }
    #[test]
    fn story_slice_preserves_range_metrics_features_and_reindexes_pair_boundaries() {
        let mut run = TypeRun {
            content: "abcdefghi".into(),
            ..Default::default()
        };
        run.set_character_style(2, 8, |s| {
            s.leading = Some(crate::geom::Leading::Fixed(44.));
            s.tracking = Some(25.);
            s.features = vec![(*b"smcp", 1)];
        });
        run.manual_kern.extend([(2, 10.), (4, -35.), (7, 45.)]);
        let sliced = story_slice(&run, 3);
        assert_eq!(sliced.content, "defghi");
        assert_eq!(
            sliced.manual_kern,
            std::collections::BTreeMap::from([(1, -35.), (4, 45.)])
        );
        assert_eq!(
            sliced.character_style(0).leading,
            Some(crate::geom::Leading::Fixed(44.))
        );
        assert_eq!(sliced.character_style(0).features, vec![(*b"smcp", 1)]);
        assert_eq!(sliced.character_style(4).tracking, Some(25.));
    }
}

#[cfg(test)]
mod frame_anchor_tests {
    use super::*;
    #[test]
    fn changing_font_size_keeps_frame_edges_fixed() {
        let mut studio = crate::app::Studio::new();
        studio.active_layer = Some(1);
        studio.place_area_text(Bounds::from_min_size(
            Pt::new(20., 30.),
            Pt::new(200., 100.),
        ));
        studio.type_insert("A stable frame");
        studio.commit_type_edit();
        let id = studio.selection[0];
        let before = studio.doc.find_shape(id.0, id.1).unwrap().geom.bbox();
        studio.patch_type(|run| run.px = 38.);
        let after = studio.doc.find_shape(id.0, id.1).unwrap().geom.bbox();
        assert_eq!(before, after);
    }
}

#[cfg(test)]
mod explicit_transform_tests {
    use super::*;
    #[test]
    fn explicit_transform_scales_glyph_metrics_while_frame_resize_reflows() {
        let run=TypeRun{content:"Area text".into(),px:20.,frame:Some(TextFrame{size:Pt::new(160.,80.),inset:[2.;4],..Default::default()}),..Default::default()};
        let mut normal=Geom::Text(run.clone());let before=normal.bbox();let after=Bounds{min:before.min,max:before.min+before.size()*2.};
        normal.map_into(before,after);let Geom::Text(resized)=normal else {unreachable!()};assert_eq!(resized.px,20.);
        let mut scaled=Geom::Text(run);scaled.map_into_with_text_scale(before,after);let bounds=scaled.bbox();let Geom::Text(scaled)=scaled else {unreachable!()};
        assert_eq!(scaled.px,40.);assert_eq!(scaled.frame.as_ref().unwrap().inset,[4.;4]);assert!((bounds.min-before.min).length()<0.001);assert!((bounds.size()-after.size()).length()<0.001);
    }
}

/// Scale source typography without changing any frame's bounds or thread links.
pub fn scale_typography(run: &mut TypeRun, sx: f32, sy: f32) {
    let top = run.origin.y - run.px * 0.85;
    run.px *= sy;
    run.origin.y = top + run.px * 0.85;
    run.tracking *= sx;
    run.leading *= sy;
    run.scale_character_metrics(sy);
    if (sx - sy).abs() > 0.001 && sy.abs() > 0.001 {
        run.scale_character_widths(sx / sy);
    }
    run.layout = None;
}

#[cfg(test)]
mod threaded_transform_tests {
    use super::*;
    #[test]
    fn follower_nonuniform_transform_preserves_rendered_tracking_and_pair_spacing() {
        let mut studio = crate::app::Studio::new();
        studio.active_layer = Some(1);
        studio.text_px = 24.;
        studio.place_area_text(Bounds::from_min_size(Pt::new(10., 10.), Pt::new(140., 90.)));
        studio.type_insert(&"AVATAR AV continues through frames. ".repeat(8));
        studio.commit_type_edit();
        let head = studio.selection[0];
        if let Geom::Text(run) = &mut studio.doc.find_shape_mut(head.0, head.1).unwrap().geom {
            run.tracking = 2.;
            run.set_character_style(0, 3, |span| span.tracking = Some(100.));
            run.set_character_style(4, 8, |span| span.tracking = Some(-40.));
            run.set_character_style(2, 5, |span| span.hscale = Some(125.));
            run.manual_kern.insert(1, -80.);
            run.manual_kern.insert(7, 120.);
        }
        studio.place_area_text(Bounds::from_min_size(Pt::new(200., 10.), Pt::new(140., 90.)));
        studio.commit_type_edit();
        let tail = studio.selection[0];
        studio.thread_text_frames(head, tail).unwrap();
        let Geom::Text(original) = &studio.doc.find_shape(head.0, head.1).unwrap().geom else { panic!() };
        let mut original = original.clone();
        original.frame = None;
        original.layout = None;
        original.wrap_width = None;
        let before = crate::text::compose(&original);
        let bounds = studio.doc.find_shape(tail.0, tail.1).unwrap().geom.bbox();
        studio.transform_shape_with_text_scale(tail.0, tail.1, Bounds::from_min_size(bounds.min, Pt::new(bounds.width() * 2., bounds.height() * 1.5)));
        let Geom::Text(transformed) = &studio.doc.find_shape(head.0, head.1).unwrap().geom else { panic!() };
        let mut transformed = transformed.clone();
        transformed.frame = None;
        transformed.layout = None;
        transformed.wrap_width = None;
        let after = crate::text::compose(&transformed);
        assert_eq!(before.len(), after.len());
        for (before, after) in before.iter().zip(after.iter()) {
            assert_eq!(before.glyphs.len(), after.glyphs.len());
            for (a, b) in before.glyphs.iter().zip(&after.glyphs) {
                assert_eq!(a.id, b.id);
                assert!((b.x - a.x * 2.).abs() < 0.01, "glyph {} spacing drift: {}", a.cluster, b.x - a.x * 2.);
                assert!((b.advance - a.advance * 2.).abs() < 0.01);
            }
        }
        let reopened = crate::project::decode(&crate::project::encode(&studio.doc).unwrap()).unwrap();
        for target in [head, tail] {
            let Geom::Text(saved) = &studio.doc.find_shape(target.0, target.1).unwrap().geom else { panic!() };
            let Geom::Text(loaded) = &reopened.find_shape(target.0, target.1).unwrap().geom else { panic!() };
            assert_eq!(saved.spans, loaded.spans);
            assert_eq!(saved.manual_kern, loaded.manual_kern);
            assert_eq!(saved.contours.len(), loaded.contours.len());
            for (a, b) in saved.contours.iter().zip(&loaded.contours) {
                assert_eq!(a.len(), b.len());
                for (a, b) in a.iter().zip(b) { assert!((*a - *b).length() < 0.001); }
            }
        }
    }
    #[test]
    fn follower_transform_scales_story_once_and_undo_restores_bounds_and_type() {
        let mut studio=crate::app::Studio::new();studio.active_layer=Some(1);studio.text_px=20.;
        studio.place_area_text(Bounds::from_min_size(Pt::new(10.,10.),Pt::new(140.,90.)));studio.type_insert(&"A long story continues across a frame. ".repeat(12));studio.commit_type_edit();let head=studio.selection[0];
        studio.place_area_text(Bounds::from_min_size(Pt::new(200.,10.),Pt::new(140.,90.)));studio.type_insert("");studio.commit_type_edit();let tail=studio.selection[0];studio.thread_text_frames(head,tail).unwrap();
        let before=studio.doc.find_shape(tail.0,tail.1).unwrap().geom.bbox();let source_before=studio.doc.find_shape(head.0,head.1).unwrap().geom.bbox();
        studio.transform_shape_with_text_scale(tail.0,tail.1,Bounds::from_min_size(before.min,before.size()*2.));
        let Geom::Text(head_run)=&studio.doc.find_shape(head.0,head.1).unwrap().geom else {panic!()};assert_eq!(head_run.px,40.);assert_eq!(head_run.frame.as_ref().unwrap().size,source_before.size());
        let Geom::Text(tail_run)=&studio.doc.find_shape(tail.0,tail.1).unwrap().geom else {panic!()};assert_eq!(tail_run.layout.as_ref().unwrap().visible_run.as_ref().unwrap().px,40.);assert!(tail_run.content.is_empty());
        studio.undo();let Geom::Text(head_run)=&studio.doc.find_shape(head.0,head.1).unwrap().geom else {panic!()};assert_eq!(head_run.px,20.);assert_eq!(studio.doc.find_shape(tail.0,tail.1).unwrap().geom.bbox(),before);
    }
}
