//! Literal text search shares the existing captured Type command and story history.
use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Query {
    pub find: String,
    pub match_case: bool,
    pub whole_word: bool,
}
impl Query {
    /// Validate a literal search before scanning a story.
    /// Args: this query. Returns an error for an empty pattern.
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.find.is_empty() {
            Err("Enter text to find".into())
        } else {
            Ok(())
        }
    }
}
#[derive(Clone)]
pub(crate) struct Target {
    owner: TypeCommandTarget,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextMatch {
    pub hit: (usize, u64),
    pub range: Range<usize>,
}
#[derive(Clone)]
pub(crate) struct Replacement {
    pub query: Query,
    pub text: String,
    pub only: Option<Range<usize>>,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Replaced {
    pub matches: usize,
    pub stories: usize,
}

fn equal(a: char, b: char, case: bool) -> bool {
    a == b || (!case && a.to_lowercase().eq(b.to_lowercase()))
}
fn word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Find nonoverlapping literals inside the actual character-style and retained SVG leaves.
/// Args: source run, scalar range and query. Returns original scalar ranges, with reference word/case rules.
fn ranges(run: &TypeRun, range: Range<usize>, query: &Query) -> Vec<Range<usize>> {
    let needle: Vec<_> = query.find.chars().collect();
    if needle.is_empty() {
        return vec![];
    }
    let chars: Vec<_> = run.content.chars().collect();
    if range.start > range.end || range.end > chars.len() {
        return vec![];
    }
    let mut prefix = vec![0; needle.len()];
    for i in 1..needle.len() {
        let mut k = prefix[i - 1];
        while k > 0 && !equal(needle[i], needle[k], query.match_case) {
            k = prefix[k - 1];
        }
        if equal(needle[i], needle[k], query.match_case) {
            k += 1;
        }
        prefix[i] = k;
    }
    let mut edges: std::collections::BTreeSet<_> = run
        .character_style_runs(0, chars.len())
        .into_iter()
        .flat_map(|s| [s.start, s.end])
        .collect();
    fn leaves(
        node: &crate::text::svg_type::TextNode,
        at: &mut usize,
        edges: &mut std::collections::BTreeSet<usize>,
    ) {
        match node {
            crate::text::svg_type::TextNode::Characters { count } => {
                *at += count;
                edges.insert(*at);
            }
            crate::text::svg_type::TextNode::Element { children, .. } => {
                for child in children {
                    leaves(child, at, edges);
                }
            }
        }
    }
    if let Some(svg) = &run.svg_text {
        leaves(&svg.root, &mut 0, &mut edges);
    }
    let boundaries = edges.clone();
    edges.extend([range.start, range.end]);
    let edges: Vec<_> = edges
        .into_iter()
        .filter(|&i| range.start <= i && i <= range.end)
        .collect();
    let mut found = Vec::new();
    for edge in edges.windows(2) {
        let (start, end) = (edge[0], edge[1]);
        let mut k = 0;
        for i in start..end {
            while k > 0 && !equal(chars[i], needle[k], query.match_case) {
                k = prefix[k - 1];
            }
            if equal(chars[i], needle[k], query.match_case) {
                k += 1;
            }
            if k == needle.len() {
                let a = i + 1 - k;
                let b = i + 1;
                let whole = !query.whole_word
                    || ((boundaries.contains(&a) || !word(chars[a - 1]))
                        && (boundaries.contains(&b) || !word(chars[b])));
                if whole {
                    found.push(a..b);
                    k = 0;
                } else {
                    k = prefix[k - 1];
                }
            }
        }
    }
    found
}

pub(super) fn rewrites(
    run: &TypeRun,
    range: Range<usize>,
    replacement: &Replacement,
) -> Vec<CharacterRewrite> {
    ranges(run, range, &replacement.query)
        .into_iter()
        .filter(|r| replacement.only.as_ref().is_none_or(|only| only == r))
        .map(|r| CharacterRewrite {
            start: r.start,
            end: r.end,
            text: replacement.text.clone(),
        })
        .collect()
}

impl Studio {
    /// Capture visible editable stories for native Find and Replace.
    /// Args: selection or document scope. Returns an owner checked against later edits, locks and documents.
    pub(crate) fn capture_text_search(&self, scope: TextScope) -> Result<Target, String> {
        self.capture_type_target(scope, false, scope == TextScope::Document)
            .map(|owner| Target { owner })
    }
    /// Check that a search still owns its original writable stories and live selection.
    /// Args: captured target. Returns false after unrelated selection, document, draft or permission changes.
    pub(crate) fn text_search_current(&self, target: &Target) -> bool {
        self.type_command_current(&target.owner)
            && target
                .owner
                .hits
                .iter()
                .all(|&hit| self.type_command_writable(hit, false))
    }
    /// Read literal matches without changing selection, content or history.
    /// Args: captured target and query. Returns stable story/scalar matches or a stale-target error.
    pub(crate) fn text_search_matches(
        &self,
        target: &Target,
        query: &Query,
    ) -> Result<Vec<TextMatch>, String> {
        query.validate()?;
        if !self.text_search_current(target) {
            return Err("The text destination changed. Close and reopen Find and Replace".into());
        }
        let mut result = Vec::new();
        for &(layer, id) in &target.owner.hits {
            let Geom::Text(run) = &self.doc.find_shape(layer, id).unwrap().geom else {
                return Err("The text destination changed".into());
            };
            let range = if target.owner.scope == TextScope::Selection {
                target
                    .owner
                    .edit
                    .as_ref()
                    .filter(|e| e.hit == (layer, id))
                    .map(|e| e.caret.min(e.anchor)..e.caret.max(e.anchor))
                    .filter(|r| !r.is_empty())
            } else {
                None
            }
            .unwrap_or(0..run.content.chars().count());
            result.extend(
                ranges(run, range, query)
                    .into_iter()
                    .map(|range| TextMatch {
                        hit: (layer, id),
                        range,
                    }),
            );
        }
        Ok(result)
    }
    /// Replace accepted search results through the sole Type content mutation route.
    /// Args: captured owner, query, literal replacement and optional one match. Returns match and changed-story counts; no-ops keep redo.
    pub(crate) fn replace_text_search(
        &mut self,
        target: &Target,
        query: &Query,
        text: &str,
        one: Option<&TextMatch>,
    ) -> Result<Replaced, String> {
        let matches = self.text_search_matches(target, query)?;
        if let Some(one) = one
            && !matches.contains(one)
        {
            return Err("The found text changed. Find it again".into());
        }
        let count = if one.is_some() { 1 } else { matches.len() };
        if count == 0 {
            return Ok(Replaced {
                matches: 0,
                stories: 0,
            });
        }
        let mut owner = target.owner.clone();
        if let Some(one) = one {
            owner.hits = vec![one.hit];
        }
        let stories = self.apply_type_command(
            &owner,
            TypeCommand::Replace(Replacement {
                query: query.clone(),
                text: text.into(),
                only: one.map(|m| m.range.clone()),
            }),
        )?;
        Ok(Replaced {
            matches: count,
            stories,
        })
    }
    /// Select an accepted match using the ordinary explicit text-object navigation path.
    /// Args: current captured owner, query and result. Returns an error before changing selection if the result is stale.
    pub(crate) fn select_text_search_match(
        &mut self,
        target: &Target,
        query: &Query,
        found: &TextMatch,
    ) -> Result<(), String> {
        if !self.text_search_matches(target, query)?.contains(found) {
            return Err("The found text changed. Find it again".into());
        }
        if self
            .type_edit
            .as_ref()
            .is_some_and(|e| (e.layer, e.id) != found.hit)
        {
            self.commit_type_edit();
        }
        let frame = self
            .doc
            .layers
            .iter()
            .enumerate()
            .flat_map(|(li, l)| l.kind.shapes().unwrap_or(&[]).iter().map(move |s| (li, s)))
            .find_map(|(li, s)| {
                if self.story_head((li, s.id)) != found.hit {
                    return None;
                }
                let Geom::Text(run) = &s.geom else {
                    return None;
                };
                run.layout
                    .as_ref()
                    .is_some_and(|l| {
                        l.visible_start <= found.range.start && found.range.start < l.visible_end
                    })
                    .then_some((li, s.id))
            })
            .unwrap_or(found.hit);
        self.selection = vec![frame];
        if let Some(edit) = self
            .type_edit
            .as_mut()
            .filter(|e| (e.layer, e.id) == found.hit)
        {
            edit.anchor = found.range.start;
            edit.caret = found.range.end;
            edit.frame = frame;
        }
        self.selected_layer = None;
        self.active_layer = Some(frame.0);
        self.individual_object = Some(frame);
        self.node_sel.clear();
        self.status = "Found text".into();
        Ok(())
    }
    /// Select the next visible editable text story, wrapping after the current one.
    /// Args: last accepted query. Returns the first match in the next story, without a search history command.
    pub(crate) fn find_next_text_story(
        &mut self,
        query: &Query,
    ) -> Result<Option<TextMatch>, String> {
        let target = self.capture_text_search(TextScope::Document)?;
        let matches = self.text_search_matches(&target, query)?;
        let hits = &target.owner.hits;
        let start = self
            .primary()
            .and_then(|hit| hits.iter().position(|h| *h == self.story_head(hit)))
            .map_or(0, |i| i + 1);
        let found = (0..hits.len()).find_map(|step| {
            let hit = hits[(start + step) % hits.len()];
            matches.iter().find(|m| m.hit == hit).cloned()
        });
        if let Some(found) = &found {
            self.select_text_search_match(&target, query, found)?;
        } else {
            self.status = "No matching text".into();
        }
        Ok(found)
    }
}

#[cfg(test)]
#[path = "type_search_tests.rs"]
mod tests;
