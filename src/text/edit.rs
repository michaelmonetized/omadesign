//! Character-indexed edits preserve paragraph styles and normalized range overrides.
use crate::geom::{CharSpan, ParagraphStyle, TypeRun};
impl TypeRun {
    pub fn character_style(&self, index: usize) -> CharSpan {
        let mut style = self
            .spans
            .iter()
            .find(|s| s.start <= index && index < s.end)
            .cloned()
            .unwrap_or_default();
        style.start = 0;
        style.end = 0;
        style
    }
    /// Normalized, complete style intervals, including inherited gaps. The number
    /// of intervals follows style changes rather than the number of characters.
    pub fn character_style_runs(&self, start: usize, end: usize) -> Vec<CharSpan> {
        let mut result = Vec::new();
        let mut cursor = start;
        for span in &self.spans {
            let a = span.start.max(cursor);
            let b = span.end.min(end);
            if a >= b {
                continue;
            }
            if cursor < a {
                result.push(CharSpan {
                    start: cursor,
                    end: a,
                    ..Default::default()
                });
            }
            result.push(CharSpan {
                start: a,
                end: b,
                ..span.clone()
            });
            cursor = b;
            if cursor == end {
                break;
            }
        }
        if cursor < end {
            result.push(CharSpan {
                start: cursor,
                end,
                ..Default::default()
            });
        }
        result
    }
    pub fn set_character_style(
        &mut self,
        start: usize,
        end: usize,
        mut update: impl FnMut(&mut CharSpan),
    ) {
        let count = self.content.chars().count();
        let start = start.min(count);
        let end = end.max(start).min(count);
        let mut changed = self.character_style_runs(start, end);
        for span in &mut changed {
            let (a, b) = (span.start, span.end);
            span.start = 0;
            span.end = 0;
            update(span);
            span.start = a;
            span.end = b;
        }
        self.replace_character_styles(start, end, changed);
    }
    /// Replace all overrides in one interval in a single pass. Incoming span
    /// coordinates are absolute, ordered, and may leave inherited gaps.
    pub fn replace_character_styles(&mut self, start: usize, end: usize, spans: Vec<CharSpan>) {
        let mut result = Vec::with_capacity(self.spans.len() + spans.len() + 2);
        for span in &self.spans {
            if span.start < start {
                let mut before = span.clone();
                before.end = before.end.min(start);
                result.push(before);
            }
        }
        result.extend(spans);
        for span in &self.spans {
            if span.end > end {
                let mut after = span.clone();
                after.start = after.start.max(end);
                result.push(after);
            }
        }
        self.assign_character_spans(result);
    }
    fn assign_character_spans(&mut self, spans: Vec<CharSpan>) {
        self.spans.clear();
        for mut span in spans {
            let (start, end) = (span.start, span.end);
            if start >= end {
                continue;
            }
            span.start = 0;
            span.end = 0;
            if span == CharSpan::default() {
                continue;
            }
            if let Some(last) = self.spans.last_mut() {
                let mut previous = last.clone();
                previous.start = 0;
                previous.end = 0;
                if last.end == start && previous == span {
                    last.end = end;
                    continue;
                }
            }
            self.spans.push(CharSpan { start, end, ..span });
        }
    }
    pub fn replace_text(
        &mut self,
        start: usize,
        end: usize,
        text: &str,
        pending: Option<CharSpan>,
    ) {
        let count = self.content.chars().count();
        let start = start.min(count);
        let end = end.max(start).min(count);
        let mut chars = self.character_style_runs(0, start);
        // Resolve paragraph styles at paragraph boundaries once. Calling the
        // prefix-scanning paragraph_style for every character is quadratic for
        // long clipboard text, even when it has only one paragraph.
        let old_starts: Vec<_> = std::iter::once(0)
            .chain(
                self.content
                    .chars()
                    .enumerate()
                    .filter_map(|(i, c)| (c == '\n').then_some(i + 1)),
            )
            .collect();
        let explicit: std::collections::BTreeMap<_, _> = self
            .paragraphs
            .iter()
            .map(|p| (p.start, p.clone()))
            .collect();
        let old_style = |index: usize| {
            let at = old_starts[old_starts
                .partition_point(|at| *at <= index)
                .saturating_sub(1)];
            explicit.get(&at).cloned().unwrap_or(ParagraphStyle {
                start: at,
                align: self.align,
                ..Default::default()
            })
        };
        let inserted_paragraph = old_style(start);
        let paragraph_styles: Vec<_> = old_starts.iter().map(|&at| old_style(at)).collect();
        let inherited = pending.unwrap_or_else(|| self.character_style(start.saturating_sub(1)));
        let n = text.chars().count();
        if n > 0 {
            chars.push(CharSpan {
                start,
                end: start + n,
                ..inherited
            });
        }
        chars.extend(
            self.character_style_runs(end, count)
                .into_iter()
                .map(|mut span| {
                    span.start = span.start - (end - start) + n;
                    span.end = span.end - (end - start) + n;
                    span
                }),
        );
        let a = super::char_to_byte(&self.content, start);
        let b = super::char_to_byte(&self.content, end);
        self.content.replace_range(a..b, text);
        self.assign_character_spans(chars);
        self.paragraphs.clear();
        let starts = std::iter::once(0).chain(
            self.content
                .chars()
                .enumerate()
                .filter_map(|(i, c)| (c == '\n').then_some(i + 1)),
        );
        for at in starts {
            let mut p = if at >= start && at < start + n {
                inserted_paragraph.clone()
            } else {
                let old_at = if at < start {
                    at
                } else {
                    at - n + (end - start)
                };
                paragraph_styles[old_starts
                    .partition_point(|&at| at <= old_at)
                    .saturating_sub(1)]
                .clone()
            };
            p.start = at;
            if p != (ParagraphStyle {
                start: at,
                align: self.align,
                ..Default::default()
            }) {
                self.paragraphs.push(p);
            }
        }
    }
    pub fn update_paragraphs(
        &mut self,
        start: usize,
        end: usize,
        mut update: impl FnMut(&mut ParagraphStyle),
    ) {
        let starts: Vec<_> = std::iter::once(0)
            .chain(
                self.content
                    .chars()
                    .enumerate()
                    .filter_map(|(i, c)| (c == '\n').then_some(i + 1)),
            )
            .collect();
        let first = super::paragraph_style(self, start).start;
        for at in starts
            .into_iter()
            .filter(|at| *at >= first && (*at < end || *at == first))
        {
            let mut p = super::paragraph_style(self, at);
            update(&mut p);
            if let Some(old) = self.paragraphs.iter_mut().find(|p| p.start == at) {
                *old = p;
            } else {
                self.paragraphs.push(p);
            }
        }
        self.paragraphs.sort_by_key(|p| p.start);
    }
}

#[cfg(test)]
mod interval_tests {
    use super::*;
    #[test]
    fn batch_edits_preserve_paragraph_styles_across_unicode_and_newlines() {
        for (start, end, insert) in [
            (0, 0, "é\n"),
            (2, 7, "🦊\nx"),
            (3, 9, ""),
            (0, 11, ""),
            (11, 11, "\nZ"),
        ] {
            let mut run = TypeRun {
                content: "ab\ncédé\nxyz".into(),
                ..Default::default()
            };
            run.update_paragraphs(3, 8, |p| p.align = crate::geom::TextAlign::Center);
            run.update_paragraphs(8, 11, |p| p.overflow_wrap = true);
            let count = run.content.chars().count();
            let start = start.min(count);
            let end = end.min(count);
            let mut expected: Vec<_> = (0..=count)
                .map(|i| crate::text::paragraph_style(&run, i))
                .collect();
            let inherited = crate::text::paragraph_style(&run, start);
            expected.splice(
                start..end,
                std::iter::repeat_n(inherited, insert.chars().count()),
            );
            run.replace_text(start, end, insert, None);
            for at in std::iter::once(0).chain(
                run.content
                    .chars()
                    .enumerate()
                    .filter_map(|(i, c)| (c == '\n').then_some(i + 1)),
            ) {
                let mut want = expected[at].clone();
                want.start = at;
                assert_eq!(
                    crate::text::paragraph_style(&run, at),
                    want,
                    "{start}..{end} {insert:?} paragraph {at}"
                );
            }
        }
    }
}
