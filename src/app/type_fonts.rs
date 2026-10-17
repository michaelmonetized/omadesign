//! Find Font targets existing text stories and the ordinary typed Type edit route.
use super::*;
use crate::text::font_inventory::{self, Info, Request, Status};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub(crate) struct Replacement {
    pub from: Vec<Request>,
    pub to: String,
}
#[derive(Clone)]
pub(crate) struct Target {
    owner: TypeCommandTarget,
    from: Vec<Request>,
    find: bool,
}
#[derive(Clone, Debug)]
pub(crate) struct Usage {
    pub key: String,
    pub info: Info,
    pub from: Vec<Request>,
    pub runs: usize,
    pub stories: Vec<(usize, u64)>,
}
fn matching(run: &TypeRun, from: &[Request]) -> Result<bool, String> {
    Ok(font_inventory::uses(run)?
        .iter()
        .any(|u| from.contains(&u.request)))
}
impl Studio {
    /// Capture a read-only dialog owner without requiring a selected font or writable story.
    /// Args: none. Returns the current document, selection and draft owner, or a busy error.
    pub(crate) fn capture_font_dialog(&self) -> Result<Target, String> {
        if !self.type_commands_available() {
            return Err("Finish the current operation before finding fonts".into());
        }
        Ok(Target {
            owner: self.type_command_snapshot(TextScope::Document, vec![]),
            from: vec![],
            find: true,
        })
    }
    /// Collect installed and document font bytes for the native family/style picker.
    /// Args: none. Returns a read-only catalog without changing artwork or history.
    pub(crate) fn font_catalog(&self) -> crate::text::font_catalog::Catalog {
        let references = self
            .doc
            .layers
            .iter()
            .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
            .filter_map(|s| {
                if let Geom::Text(run) = &s.geom {
                    Some(
                        std::iter::once(run.font.clone())
                            .chain(run.spans.iter().filter_map(|s| s.font.clone())),
                    )
                } else {
                    None
                }
            })
            .flatten();
        crate::text::font_catalog::Catalog::new(references)
    }
    /// Rebuild derived text geometry after installed faces change, without adding history.
    /// Args: none. Returns no value; captured font command owners retire with the render revision.
    pub(crate) fn refresh_font_rendering(&mut self) {
        for layer in &mut self.doc.layers {
            for shape in layer.kind.shapes_mut().into_iter().flatten() {
                if let Geom::Text(run) = &mut shape.geom {
                    run.layout = None;
                }
                crate::text::fill_contours(&mut shape.geom);
            }
        }
        if let Some(edit) = &mut self.type_edit {
            if let Geom::Text(run) = &mut edit.before {
                run.layout = None;
            }
            crate::text::fill_contours(&mut edit.before);
        }
        crate::text_geometry::reflow(&mut self.doc);
        self.canvas_key = None;
        self.interaction_render.clear();
        self.mark_interaction();
    }
    /// List authored font usage, including hidden and locked stories, without editing or substituting names.
    /// Args: none. Returns rows grouped by available family/style/version or exact unavailable request.
    pub(crate) fn font_usage(&self) -> Result<Vec<Usage>, String> {
        let mut found: BTreeMap<Request, (String, usize, BTreeSet<(usize, u64)>, BTreeSet<char>)> =
            BTreeMap::new();
        let mut seen = BTreeSet::new();
        for (layer, l) in self.doc.layers.iter().enumerate() {
            for shape in l.kind.shapes().unwrap_or(&[]) {
                if !matches!(shape.geom, Geom::Text(_)) {
                    continue;
                }
                let hit = self.story_head((layer, shape.id));
                if !seen.insert(hit) {
                    continue;
                }
                let Some(Geom::Text(run)) = self.doc.find_shape(hit.0, hit.1).map(|s| &s.geom)
                else {
                    continue;
                };
                let chars: Vec<_> = run.content.chars().collect();
                for usage in font_inventory::uses(run)? {
                    let row = found.entry(usage.request).or_default();
                    row.0 = run.font.clone();
                    row.1 += 1;
                    row.2.insert(hit);
                    row.3.extend(chars[usage.range].iter().copied());
                }
            }
        }
        let catalog = self.font_catalog();
        let mut rows: BTreeMap<String, Usage> = BTreeMap::new();
        let mut characters: BTreeMap<String, (String, BTreeSet<char>)> = BTreeMap::new();
        for (request, (fallback, runs, stories, chars)) in found {
            let info = font_inventory::describe_with(&catalog, &request, &fallback, &chars);
            let key = if info.status == Status::Exact {
                info.identity.clone().unwrap()
            } else {
                format!("{request:?}")
            };
            let all = characters.entry(key.clone()).or_default();
            all.0 = fallback;
            all.1.extend(chars);
            if let Some(row) = rows.get_mut(&key) {
                row.from.push(request);
                row.runs += runs;
                row.stories.extend(stories);
                row.stories.sort();
                row.stories.dedup();
            } else {
                rows.insert(
                    key.clone(),
                    Usage {
                        key,
                        info,
                        from: vec![request],
                        runs,
                        stories: stories.into_iter().collect(),
                    },
                );
            }
        }
        for row in rows.values_mut() {
            let (fallback, chars) = characters.get(&row.key).unwrap();
            row.info.missing_glyphs =
                font_inventory::describe_with(&catalog, &row.from[0], fallback, chars)
                    .missing_glyphs;
        }
        let mut rows: Vec<_> = rows.into_values().collect();
        rows.sort_by_cached_key(|r| {
            (
                r.info.family.to_lowercase(),
                r.info.style.to_lowercase(),
                r.info.version.clone(),
            )
        });
        Ok(rows)
    }
    /// Capture only stories using the chosen source font; unrelated locked text is not a destination.
    /// Args: selection/document scope and the row's actual authored requests. Returns an ordinary stale-checked Type target.
    pub(crate) fn capture_font_change(
        &self,
        scope: TextScope,
        from: &[Request],
    ) -> Result<Target, String> {
        let owner =
            self.capture_type_target_filtered(scope, true, false, |run| matching(run, from))?;
        Ok(Target {
            owner,
            from: from.to_vec(),
            find: false,
        })
    }
    fn font_find_hits(&self, from: &[Request]) -> Result<Vec<(usize, u64)>, String> {
        let mut hits = Vec::new();
        let mut seen = BTreeSet::new();
        for (layer, l) in self.doc.layers.iter().enumerate() {
            for shape in l.kind.shapes().unwrap_or(&[]) {
                if !matches!(shape.geom, Geom::Text(_)) {
                    continue;
                }
                let hit = self.story_head((layer, shape.id));
                if !seen.insert(hit) {
                    continue;
                }
                if let Some(Geom::Text(run)) = self.doc.find_shape(hit.0, hit.1).map(|s| &s.geom)
                    && matching(run, from)?
                {
                    hits.push(hit);
                }
            }
        }
        Ok(hits)
    }
    /// Capture all matching stories for the reference Find selection action.
    /// Args: row requests. Returns a read-only owner, including locked and hidden matches without edit permission.
    pub(crate) fn capture_font_find(&self, from: &[Request]) -> Result<Target, String> {
        if !self.type_commands_available() {
            return Err("Finish the current operation before finding text".into());
        }
        if from.is_empty() {
            return Err("Choose a font to find".into());
        }
        let owner = self.type_command_snapshot(TextScope::Document, self.font_find_hits(from)?);
        Ok(Target {
            owner,
            from: from.to_vec(),
            find: true,
        })
    }
    /// Check a font command's captured artwork, selection, live draft and story permissions.
    /// Args: captured target. Returns false after a stale owner or eligibility change.
    pub(crate) fn font_target_current(&self, target: &Target) -> bool {
        if target.find {
            self.type_command_owner_current(&target.owner)
                && (target.from.is_empty()
                    || self
                        .font_find_hits(&target.from)
                        .is_ok_and(|hits| hits == target.owner.hits))
        } else {
            self.type_command_current(&target.owner)
        }
    }
    /// Replace a reported font through the same Type command used for ordinary text/history edits.
    /// Args: captured replacement owner and an available native font reference. Returns changed story count.
    pub(crate) fn replace_font_usage(
        &mut self,
        target: &Target,
        to: &str,
    ) -> Result<usize, String> {
        if target.find || !self.font_target_current(target) {
            return Err("The font destination changed. Reopen Find Font".into());
        }
        self.apply_type_command(
            &target.owner,
            TypeCommand::Font(Replacement {
                from: target.from.clone(),
                to: to.into(),
            }),
        )
    }
    /// Select actual matching text stories using a font, without a search history command.
    /// Args: captured Find owner. Returns selected story count; explicit navigation finishes an existing Type draft.
    pub(crate) fn select_font_usage(&mut self, target: &Target) -> Result<usize, String> {
        if !target.find || !self.font_target_current(target) {
            return Err("The font destination changed. Reopen Find Font".into());
        }
        let hits = target.owner.hits.clone();
        if self.type_edit.is_some() {
            self.commit_type_edit();
        }
        self.selection = hits;
        self.selected_layer = None;
        if let Some(&(layer, _)) = self
            .selection
            .iter()
            .rev()
            .find(|&&(layer, _)| self.layer_unlocked(layer) && self.doc.layer_visible(layer))
        {
            self.active_layer = Some(layer);
        }
        self.individual_object = (self.selection.len() == 1).then(|| self.selection[0]);
        self.node_sel.clear();
        self.tool = Tool::Select;
        self.status = format!("Selected {} text stories", self.selection.len());
        Ok(self.selection.len())
    }
}

#[cfg(test)]
#[path = "type_fonts_tests.rs"]
mod tests;
