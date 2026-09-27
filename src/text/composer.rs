//! Shared positioned composition. Rendering, editing and export consume this layout.
use super::*;
use crate::geom::{BreakMode, LastLine, ParagraphStyle, TextAlign};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug)]
pub struct LayoutGlyph {
    pub id: u16,
    pub cluster: usize,
    pub x: f32,
    pub y: f32,
    pub advance: f32,
    pub is_space: bool,
    pub hscale: f32,
    pub vscale: f32,
}
#[derive(Clone, Debug)]
pub struct LayoutLine {
    pub text: String,
    pub start: usize,
    pub end: usize,
    pub paragraph: usize,
    pub last: bool,
    pub hyphenated: bool,
    pub baseline: f32,
    pub height: f32,
    pub offset: f32,
    pub width: f32,
    pub natural_width: f32,
    pub glyphs: Vec<LayoutGlyph>,
    pub carets: Vec<(usize, f32)>,
}
impl LayoutLine {
    pub fn caret_x(&self, index: usize) -> f32 {
        self.carets
            .iter()
            .min_by_key(|(i, _)| i.abs_diff(index))
            .map_or(self.offset, |(_, x)| *x)
    }
}

pub fn paragraph_style(run: &TypeRun, index: usize) -> ParagraphStyle {
    let start = run
        .content
        .chars()
        .take(index)
        .enumerate()
        .filter_map(|(i, c)| (c == '\n').then_some(i + 1))
        .last()
        .unwrap_or(0);
    run.paragraphs
        .iter()
        .find(|p| p.start == start)
        .cloned()
        .unwrap_or(ParagraphStyle {
            start,
            align: run.align,
            ..Default::default()
        })
}

/// Pure layout; geometry flows may clone a run, clear their layout cache and call this.
pub fn compose_plain(run: &TypeRun) -> Arc<Vec<LayoutLine>> {
    compose(run)
}
pub fn compose(run: &TypeRun) -> Arc<Vec<LayoutLine>> {
    type Cache = HashMap<(String, u64), Arc<Vec<LayoutLine>>>;
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let key = (
        serde_json::to_string(run).unwrap_or_default(),
        FONT_REVISION.load(std::sync::atomic::Ordering::Relaxed),
    );
    let cache = CACHE.get_or_init(Default::default);
    if let Some(lines) = cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key)
        .cloned()
    {
        return lines;
    }
    let bytes = resolve_path(run).and_then(|p| font_bytes(&p));
    let face = bytes
        .as_ref()
        .and_then(|b| rustybuzz::Face::from_slice(b, 0));
    let mut lines = Vec::new();
    let mut base = 0;
    let mut baseline = 0.;
    for paragraph in run.content.split('\n') {
        let style = paragraph_style(run, base);
        let measure = run
            .wrap_width
            .filter(|w| w.is_finite() && *w > 0. && style.break_mode != BreakMode::KeepAll);
        let mut boundaries = Vec::new();
        let mut start = 0;
        let mut consecutive_hyphens = 0;
        while start < paragraph.len() {
            let rest = &paragraph[start..];
            let start_char = base + paragraph[..start].chars().count();
            let Some(width) = measure else {
                boundaries.push((start, paragraph.len(), false));
                break;
            };
            let mut candidates: Vec<(usize, bool)> = if style.break_mode == BreakMode::BreakAll {
                rest.grapheme_indices(true)
                    .map(|(i, g)| (i + g.len(), false))
                    .collect()
            } else {
                unicode_linebreak::linebreaks(rest)
                    .map(|(i, _)| (i, rest[..i].ends_with('\u{ad}')))
                    .collect()
            };
            // A soft hyphen is available even when automatic hyphenation is disabled.
            for (i, c) in rest.char_indices() {
                if c == '\u{ad}' {
                    candidates.push((i + c.len_utf8(), true));
                }
            }
            if style.hyphenate {
                let settings = style.hyphen.clone().unwrap_or_default();
                if settings.max_consecutive == 0 || consecutive_hyphens < settings.max_consecutive {
                    for (word_byte, word) in rest.unicode_word_indices() {
                        if !settings.last_word && word_byte + word.len() == rest.len() {
                            continue;
                        }
                        for offset in hyphen_points(word, &settings) {
                            let at = word_byte + char_to_byte(word, offset);
                            candidates.push((at, true));
                        }
                    }
                }
            }
            candidates.push((rest.len(), false));
            for (at,hyphen) in &mut candidates {if *at==rest.len(){*hyphen=false;}}
            candidates.sort_unstable();
            candidates.dedup();
            let words: Vec<_> = rest.unicode_word_indices().map(|(at, word)| (at, at + word.len())).collect();
            candidates.retain(|(byte, hyphen)| {
                if style.break_mode == BreakMode::Word && !hyphen && words.iter().any(|(start, end)| start < byte && byte < end) {
                    return false;
                }
                if *byte == rest.len() {
                    return true;
                }
                let at = start_char + rest[..*byte].chars().count();
                !run.spans
                    .iter()
                    .any(|s| s.no_break && s.start < at && at < s.end)
                    && !rest[..*byte].ends_with(['\u{a0}', '\u{2011}', '\u{2060}'])
                    && !rest[*byte..].starts_with(['\u{a0}', '\u{2011}', '\u{2060}'])
            });
            let mut best = None;
            for &(byte, hyphen) in &candidates {
                let glyphs = shape_line(run, face.as_ref(), &rest[..byte], start_char, hyphen);
                if trim_width(&glyphs) <= width + 0.01 {
                    best = Some((byte, hyphen));
                } else if best.is_some() && !hyphen {
                    break;
                }
            }
            let mut chosen =
                best.unwrap_or_else(|| candidates.first().copied().unwrap_or((rest.len(), false)));
            if chosen.1 && !matches!(style.align, TextAlign::Justify { .. }) {
                let zone = style.hyphen.as_ref().map_or(0., |h| h.zone);
                if zone > 0. {
                    for &(byte, hyphen) in candidates.iter().rev() {
                        if !hyphen && byte < chosen.0 {
                            let used = trim_width(&shape_line(
                                run,
                                face.as_ref(),
                                &rest[..byte],
                                start_char,
                                false,
                            ));
                            if used <= width && width - used <= zone {
                                chosen = (byte, false);
                                break;
                            }
                        }
                    }
                }
            }
            if best.is_none() && style.overflow_wrap && style.break_mode != BreakMode::BreakAll {
                for (i, g) in rest.grapheme_indices(true) {
                    let byte = i + g.len();
                    let at = start_char + rest[..byte].chars().count();
                    if run
                        .spans
                        .iter()
                        .any(|s| s.no_break && s.start < at && at < s.end)
                    {
                        continue;
                    }
                    if rest[..byte].ends_with(['\u{a0}', '\u{2011}', '\u{2060}'])
                        || rest[byte..].starts_with(['\u{a0}', '\u{2011}', '\u{2060}'])
                    {
                        continue;
                    }
                    if trim_width(&shape_line(
                        run,
                        face.as_ref(),
                        &rest[..byte],
                        start_char,
                        false,
                    )) <= width
                        || i == 0
                    {
                        chosen = (byte, false);
                    } else {
                        break;
                    }
                }
            }
            boundaries.push((start, start + chosen.0, chosen.1));
            start += chosen.0;
            consecutive_hyphens = if chosen.1 { consecutive_hyphens + 1 } else { 0 };
        }
        if boundaries.is_empty() {
            boundaries.push((0, 0, false));
        }
        if let (Some(rule), Some(width)) = (&style.runt, measure) {
            let n = boundaries.len();
            if n >= 2 {
                let (a, b, _) = boundaries[n - 1];
                let tail = paragraph[a..b].trim();
                if tail.unicode_words().count() <= rule.words
                    || tail.chars().count() < rule.characters
                {
                    let (pa, _, _) = boundaries[n - 2];
                    let opportunities: Vec<_> = unicode_linebreak::linebreaks(&paragraph[pa..a])
                        .map(|(i, _)| pa + i)
                        .filter(|i| *i > pa && *i < a)
                        .filter(|i| !paragraph.unicode_word_indices().any(|(start, word)| start < *i && *i < start + word.len()))
                        .collect();
                    for split in opportunities.into_iter().rev() {
                        let at=base+paragraph[..split].chars().count();
                        if run.spans.iter().any(|s|s.no_break&&s.start<at&&at<s.end)||paragraph[..split].ends_with(['\u{a0}','\u{2011}','\u{2060}'])||paragraph[split..].starts_with(['\u{a0}','\u{2011}','\u{2060}']) {continue;}

                        if trim_width(&shape_line(
                            run,
                            face.as_ref(),
                            &paragraph[split..b],
                            base + paragraph[..split].chars().count(),
                            false,
                        )) <= width
                        {
                            boundaries[n - 2].1 = split;
                            boundaries[n - 2].2 = false;
                            boundaries[n - 1].0 = split;
                            break;
                        }
                    }
                }
            }
        }
        for (i, &(a, b, hyphen)) in boundaries.iter().enumerate() {
            let source = &paragraph[a..b];
            let start = base + paragraph[..a].chars().count();
            let end = start + source.chars().count();
            let mut glyphs = shape_line(run, face.as_ref(), source, start, hyphen);
            let natural_width = trim_width(&glyphs);
            let last = i + 1 == boundaries.len();
            let mut width = natural_width;
            let mut offset = 0.;
            let effective = if measure.is_none() && matches!(style.align, TextAlign::Justify { .. })
            {
                TextAlign::Start
            } else {
                style.align
            };
            let justify = matches!(
                effective,
                TextAlign::Justify {
                    last: LastLine::Justify
                }
            ) || (!last && matches!(effective, TextAlign::Justify { .. }));
            if justify {
                if let Some(target) = measure {
                    width = justify_glyphs(&mut glyphs, target, &style, run.px);
                }
            } else {
                let align = match effective {
                    TextAlign::Justify {
                        last: LastLine::Center,
                    } => TextAlign::Center,
                    TextAlign::Justify {
                        last: LastLine::End,
                    } => TextAlign::End,
                    TextAlign::Justify { .. } => TextAlign::Start,
                    other => other,
                };
                let remaining = measure.map_or(-width, |m| (m - width).max(0.));
                offset = match align {
                    TextAlign::Center => remaining * 0.5,
                    TextAlign::End => remaining,
                    _ => 0.,
                };
            }
            let height = run.line_height();
            for glyph in &mut glyphs {
                glyph.x += offset;
                glyph.y += baseline;
            }
            let mut carets = Vec::new();
            for (byte, _) in source
                .grapheme_indices(true)
                .chain(std::iter::once((source.len(), "")))
            {
                let ch = start + source[..byte].chars().count();
                let x = cluster_x(&glyphs, ch, end, offset, width);
                carets.push((ch, x));
            }
            lines.push(LayoutLine {
                text: source.to_owned(),
                start,
                end,
                paragraph: base,
                last,
                hyphenated: hyphen,
                baseline,
                height,
                offset,
                width,
                natural_width,
                glyphs,
                carets,
            });
            baseline += height;
        }
        base += paragraph.chars().count() + 1;
    }
    let result = Arc::new(lines);
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    if cache.len() >= 256 {
        cache.clear();
    }
    cache.insert(key, result.clone());
    result
}

// Pattern and discretionary breaks share source-preserving candidate handling.
fn hyphen_points(word: &str, settings: &crate::geom::HyphenSettings) -> Vec<usize> {
    super::hyphenation::hyphen_points(word, settings)
}

fn cluster_x(glyphs: &[LayoutGlyph], ch: usize, end: usize, offset: f32, width: f32) -> f32 {
    if ch >= end {
        return glyphs
            .last()
            .map_or(offset, |g| g.x + g.advance)
            .max(offset + width);
    }
    if let Some(g) = glyphs.iter().find(|g| g.cluster == ch) {
        return g.x;
    }
    let prev = glyphs.iter().rev().find(|g| g.cluster < ch);
    let next = glyphs.iter().find(|g| g.cluster > ch);
    match (prev, next) {
        (Some(a), Some(b)) => {
            a.x + (b.x - a.x) * (ch - a.cluster) as f32 / (b.cluster - a.cluster) as f32
        }
        (Some(a), None) => {
            a.x + a.advance * (ch - a.cluster) as f32 / (end - a.cluster).max(1) as f32
        }
        _ => offset,
    }
}

fn shape_line(
    run: &TypeRun,
    face: Option<&rustybuzz::Face<'_>>,
    text: &str,
    start: usize,
    hyphen: bool,
) -> Vec<LayoutGlyph> {
    let mut visible = String::new();
    let mut mapping = Vec::new();
    for (i, c) in text.chars().enumerate() {
        if c != '\u{ad}' {
            mapping.push((visible.len(), start + i, c));
            visible.push(c);
        }
    }
    if hyphen {
        mapping.push((
            visible.len(),
            start + text.chars().count().saturating_sub(1),
            '-',
        ));
        visible.push('-');
    }
    let Some(face) = face else {
        return mapping
            .iter()
            .enumerate()
            .map(|(i, (_, ch, c))| LayoutGlyph {
                id: 0,
                cluster: *ch,
                x: i as f32 * run.px * 0.55,
                y: 0.,
                advance: run.px * 0.55,
                is_space: *c == ' ',
                hscale: 1.,
                vscale: 1.,
            })
            .collect();
    };
    let scale = run.px.max(1.) / face.units_per_em() as f32;
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(&visible);
    buffer.set_direction(rustybuzz::Direction::LeftToRight);
    let shaped = rustybuzz::shape(face, &ot_features(run), buffer);
    let mut pen = 0.;
    let mut result: Vec<_> = shaped
        .glyph_infos()
        .iter()
        .zip(shaped.glyph_positions())
        .map(|(info, pos)| {
            let (_, cluster, c) = mapping
                .iter()
                .rev()
                .find(|(byte, _, _)| *byte <= info.cluster as usize)
                .copied()
                .unwrap_or((0, start, ' '));
            let advance = pos.x_advance as f32 * scale + run.tracking;
            let glyph = LayoutGlyph {
                id: info.glyph_id as u16,
                cluster,
                x: pen + pos.x_offset as f32 * scale,
                y: -pos.y_offset as f32 * scale,
                advance,
                is_space: c == ' ' || c == '\t',
                hscale: 1.,
                vscale: 1.,
            };
            pen += advance;
            glyph
        })
        .collect();
    let style = paragraph_style(run, start);
    let n = result.len();
    let mut added = 0.;
    for (i, g) in result.iter_mut().enumerate() {
        g.x += added;
        let extra = (if g.is_space {
            g.advance * (style.word_spacing[1] / 100. - 1.)
        } else {
            0.
        }) + if i + 1 < n {
            run.px * style.letter_spacing[1] / 100.
        } else {
            0.
        };
        g.advance += extra;
        added += extra;
    }
    result
}
fn trim_width(glyphs: &[LayoutGlyph]) -> f32 {
    glyphs
        .iter()
        .rfind(|g| !g.is_space)
        .map_or(0., |g| g.x + g.advance)
}
fn justify_glyphs(glyphs: &mut [LayoutGlyph], target: f32, style: &ParagraphStyle, px: f32) -> f32 {
    let count = glyphs
        .iter()
        .rposition(|g| !g.is_space)
        .map_or(0, |i| i + 1);
    if count == 0 {
        return 0.;
    }
    let natural = trim_width(glyphs);
    let mut old_pen = 0.;
    let offsets: Vec<_> = glyphs
        .iter()
        .map(|g| {
            let offset = g.x - old_pen;
            old_pen += g.advance;
            offset
        })
        .collect();
    let spaces: Vec<_> = glyphs[..count]
        .iter()
        .enumerate()
        .filter(|(_, g)| g.is_space)
        .map(|(i, g)| (i, g.advance / (style.word_spacing[1] / 100.).max(0.01)))
        .collect();
    let total_space: f32 = spaces.iter().map(|(_, v)| v).sum();
    let mut remaining = target - natural;
    let word_delta = remaining.clamp(
        total_space * ((style.word_spacing[0] - style.word_spacing[1]) / 100.),
        total_space * ((style.word_spacing[2] - style.word_spacing[1]) / 100.),
    );
    if total_space > 0. {
        for (i, width) in spaces {
            glyphs[i].advance += word_delta * width / total_space;
        }
        remaining -= word_delta;
    }
    let gaps = count.saturating_sub(1);
    if gaps > 0 {
        let letter = (remaining / gaps as f32).clamp(
            px * (style.letter_spacing[0] - style.letter_spacing[1]) / 100.,
            px * (style.letter_spacing[2] - style.letter_spacing[1]) / 100.,
        );
        for g in &mut glyphs[..gaps] {
            g.advance += letter;
        }
    }
    let mut pen = 0.;
    for (glyph, offset) in glyphs.iter_mut().zip(offsets) {
        glyph.x = pen + offset;
        pen += glyph.advance;
    }
    glyphs[..count].iter().map(|g| g.advance).sum()
}

pub fn glyph_contours(run: &TypeRun, glyph: &LayoutGlyph) -> Vec<Vec<Pt>> {
    let Some(bytes) = resolve_path(run).and_then(|p| font_bytes(&p)) else {
        return Vec::new();
    };
    let Ok(font) = ab_glyph::FontRef::try_from_slice(&bytes) else {
        return Vec::new();
    };
    let Some(outline) = font.outline(GlyphId(glyph.id)) else {
        return Vec::new();
    };
    let scale = run.px.max(1.) / font.units_per_em().unwrap_or(1000.);
    let curves = map_curves(&outline.curves, 0., 0., scale);
    flatten_outline(&curves)
        .into_iter()
        .map(|contour| {
            contour
                .into_iter()
                .map(|p| {
                    Pt::new(
                        run.origin.x + glyph.x + p.x * glyph.hscale,
                        run.origin.y + glyph.y + p.y * glyph.vscale,
                    )
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(content: &str) -> TypeRun {
        TypeRun {
            content: content.into(),
            px: 24.,
            font: "/usr/share/fonts/liberation/LiberationSans-Regular.ttf".into(),
            ..Default::default()
        }
    }
    #[test]
    fn justification_limits_and_last_lines() {
        let mut r = run("alpha beta gamma delta epsilon zeta eta theta");
        r.wrap_width = Some(200.);
        r.update_paragraphs(0, r.content.chars().count(), |p| {
            p.align = TextAlign::Justify {
                last: LastLine::Start,
            };
            p.word_spacing = [80., 100., 300.];
            p.letter_spacing = [0., 0., 20.];
        });
        let lines = compose(&r);
        assert!(lines.len() > 1);
        for line in lines.iter().filter(|l| !l.last) {
            assert!((line.width - 200.).abs() < 0.5);
        }
        assert!(lines.last().unwrap().width < 200.);
        r.paragraphs[0].align = TextAlign::Justify {
            last: LastLine::Justify,
        };
        r.paragraphs[0].letter_spacing[2] = 100.;
        assert!((compose(&r).last().unwrap().width - 200.).abs() < 0.5);
        r.paragraphs[0].word_spacing = [100., 100., 100.];
        r.paragraphs[0].letter_spacing = [0., 0., 0.];
        assert!(compose(&r).iter().any(|l| l.width < 199. && l.offset == 0.));
    }
    #[test]
    fn point_alignment_and_per_paragraph_carets() {
        let mut r = run("one two three\nfour five six");
        r.align = TextAlign::Center;
        assert!(compose(&r)[0].offset < 0.);
        r.wrap_width = Some(210.);
        r.update_paragraphs(0, 4, |p| p.align = TextAlign::End);
        r.update_paragraphs(14, 20, |p| p.align = TextAlign::Start);
        assert!(compose(&r)[0].offset > 0.);
        assert_eq!(compose(&r)[1].offset, 0.);
        for line in compose(&r).iter() {
            for (i, x) in &line.carets {
                assert_eq!(
                    hit_char(&r, Pt::new(r.origin.x + x, r.origin.y + line.baseline)),
                    *i
                );
            }
        }
    }
    #[test]
    fn unicode_modes_graphemes_and_no_break() {
        let mut r = run("abcde\u{301}fghijkl");
        r.wrap_width = Some(30.);
        assert_eq!(compose(&r).len(), 1);
        r.update_paragraphs(0, 0, |p| p.overflow_wrap = true);
        for l in compose(&r).iter() {
            assert!(!l.text.starts_with('\u{301}'));
        }
        assert!(compose(&r).len() > 1);
        r.set_character_style(0, r.content.chars().count(), |s| s.no_break = true);
        assert_eq!(compose(&r).len(), 1);
        r.spans.clear();
        r.paragraphs[0].break_mode = BreakMode::KeepAll;
        assert_eq!(compose(&r).len(), 1);
    }
    #[test]
    fn soft_hyphens_only_render_at_break() {
        let mut r = run("discre\u{ad}tionary");
        let plain = run("discretionary");
        assert!((measure(&r).0 - measure(&plain).0).abs() < 0.01);
        r.wrap_width = Some(measure(&run("discre-")).0 + 0.1);
        assert!(compose(&r).len() > 1);
        assert_eq!(compose(&r)[0].glyphs.len(), 7);
    }
    #[test]
    fn paragraph_edit_roundtrip() {
        let mut r = run("first\nsecond");
        r.update_paragraphs(6, 12, |p| p.align = TextAlign::End);
        r.set_character_style(6, 12, |s| s.no_break = true);
        r.replace_text(0, 0, "new\n", None);
        assert_eq!(paragraph_style(&r, 10).align, TextAlign::End);
        assert!(r.character_style(11).no_break);
        r.replace_text(0, 4, "", None);
        assert_eq!(paragraph_style(&r, 6).align, TextAlign::End);
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(serde_json::from_str::<TypeRun>(&json).unwrap(), r);
    }
}

#[cfg(test)] mod stretch_tests {
    use super::*;
    #[test] fn dictionary_hyphenation_reduces_word_space_stretch() {
        let text="Typography and composition establish the rhythm of communication. Beautiful paragraphs balance the distribution of information with comfortable reading. Hyphenation distributes complicated terminology across available lines and improves consistency between neighboring words. Designers refine proportions and relationships while preserving the integrity of individual characters.";
        let mut sample=TypeRun{font:concat!(env!("CARGO_MANIFEST_DIR"),"/tests/assets/fonts/EBGaramond.ttf").into(),content:text.into(),px:24.,wrap_width:Some(320.),..Default::default()};
        sample.update_paragraphs(0,text.chars().count(),|p|{p.align=TextAlign::Justify{last:LastLine::Start};p.word_spacing=[80.,100.,1000.];p.hyphen=Some(crate::geom::HyphenSettings{capitalized:true,last_word:true,..Default::default()});});
        fn metric(run:&TypeRun)->(f32,f32) {
            let base=TypeRun{content:"x x".into(),wrap_width:None,paragraphs:vec![],..run.clone()};let space=compose(&base)[0].glyphs.iter().find(|g|g.is_space).unwrap().advance;
            let lines=compose(run);let values:Vec<_>=lines.iter().filter(|l|!l.last).flat_map(|l|l.glyphs.iter().take_while(|g|g.x<l.width).filter(|g|g.is_space).map(|g|g.advance/space)).collect();
            (values.iter().sum::<f32>()/values.len().max(1) as f32,values.into_iter().fold(0.,f32::max))
        }
        let off=metric(&sample);sample.paragraphs[0].hyphenate=true;let on=metric(&sample);
        eprintln!("justification stretch: off={off:?}, on={on:?}");assert!(on.0<off.0&&on.1<off.1);
    }
}

#[cfg(test)] mod migration_regressions {
    use super::*;
    use crate::document::{Document,Shape,Style};
    use crate::geom::{Geom,RuntRule};
    #[test] fn new_wrap_semantics_roundtrip_and_legacy_point_stays_plain() {
        for width in [None,Some(20.)] {
            let mut document=Document::new("Roundtrip",400.,400.,96.);
            let run=TypeRun{content:"unbreakableword".into(),px:24.,wrap_width:width,..Default::default()};
            document.layers[1].kind.shapes_mut().unwrap().push(Shape::new(Geom::Text(run.clone()),Style::default()));
            let json=crate::project::encode(&document).unwrap();
            assert!(json.contains(if width.is_some(){"\"version\":8"}else{"\"version\":5"}));
            let loaded=crate::project::decode(&json).unwrap();let Geom::Text(restored)=&loaded.layers[1].kind.shapes().unwrap()[0].geom else{panic!()};
            assert!(restored.paragraphs.is_empty());assert_eq!(compose(&run).len(),compose(restored).len());
            let legacy=json.replace("\"version\":8","\"version\":7").replace("\"version\":5","\"version\":7");
            let loaded=crate::project::decode(&legacy).unwrap();let Geom::Text(restored)=&loaded.layers[1].kind.shapes().unwrap()[0].geom else{panic!()};
            assert_eq!(!restored.paragraphs.is_empty(),width.is_some());
            if width.is_some(){assert!(restored.paragraphs[0].overflow_wrap);}
        }
    }
    #[test] fn trailing_discretionary_hyphen_is_invisible_without_break() {
        let run=TypeRun{content:"word\u{ad}".into(),wrap_width:Some(500.),..Default::default()};
        assert_eq!(compose(&run).len(),1);assert!(!compose(&run)[0].hyphenated);
        let plain=TypeRun{content:"word".into(),..run.clone()};assert!((measure(&run).0-measure(&plain).0).abs()<0.01);assert_eq!(glyph_count(&run),glyph_count(&plain));
    }
    #[test] fn runt_rebalancing_preserves_no_break_ranges() {
        let mut run=TypeRun{content:"one two three four".into(),px:24.,..Default::default()};
        run.wrap_width=Some(measure(&TypeRun{content:"one two three".into(),..run.clone()}).0);
        run.set_character_style(4,13,|s|s.no_break=true);
        run.update_paragraphs(0,0,|p|p.runt=Some(RuntRule::default()));
        assert!(compose(&run).iter().all(|line|line.start<=4||line.start>=13));
    }
}

#[cfg(test)] mod word_mode_tests {
    use super::*;
    #[test] fn word_mode_keeps_katakana_words_but_anywhere_is_explicit() {
        let mut run=TypeRun{content:"カタカナ".into(),font:concat!(env!("CARGO_MANIFEST_DIR"),"/tests/assets/fonts/EBGaramond.ttf").into(),px:30.,wrap_width:Some(25.),..Default::default()};
        assert!(compose(&run).len()>1,"Normal honors UAX14 opportunities inside Katakana");
        run.update_paragraphs(0,4,|p|p.break_mode=BreakMode::Word);
        assert_eq!(compose(&run).len(),1,"Word mode retains the Unicode word");
        run.update_paragraphs(0,4,|p|p.overflow_wrap=true);
        assert!(compose(&run).len()>1,"Anywhere explicitly permits long-word breaks");
        run.content="inter\u{ad}national".into();run.wrap_width=Some(80.);run.update_paragraphs(0,run.content.chars().count(),|p|p.overflow_wrap=false);
        assert!(compose(&run).iter().any(|l|l.hyphenated),"Explicit discretionary hyphens remain honored");
        run.content="カタカナ".into();run.wrap_width=None;let word_width=compose(&run)[0].width;
        run.content.push_str(" a");run.wrap_width=Some(word_width+0.5);run.update_paragraphs(0,6,|p|p.runt=Some(Default::default()));
        let lines=compose(&run);assert_eq!(lines.len(),2);assert!(lines[0].text.starts_with("カタカナ"),"Runt adjustment must move whole Unicode words");
    }
}
