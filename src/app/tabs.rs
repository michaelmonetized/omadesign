use super::*;
use std::mem::swap;

/// Inactive documents own their state here. The active slot is an empty staging
/// area: exchanging fields moves even large rasters and undo histories in O(1).
pub(super) struct TabState {
    doc: Document,
    path: Option<PathBuf>,
    dirty: bool,
    view: View,
    selection: Vec<(usize, u64)>,
    active_layer: Option<usize>,
    history: History,
    clone_source: Option<Pt>,
    pixel_sel: Option<Vec<u8>>,
    type_edit: Option<TypeEdit>,
    need_fit: bool,
    layer_rename: Option<(usize, String)>,
    paste_nudge: u32,
    node_sel: BTreeSet<usize>,
    playhead: f32,
    playing: bool,
    play_clock: f64,
    pose_drag: HashMap<u64, Pose>,
    selected_key: Option<(u64, Prop, usize)>,
    pending_place: Option<PendingPlace>,
    show_welcome: bool,
    artboard_sel: Vec<u64>,
    layer_expanded: HashSet<u64>,
    swap_id: String,
    last_input: Instant,
    last_swap: Option<Instant>,
    shape_rename: Option<(usize, u64, String)>,
    artboard_rename: Option<(u64, String)>,
}

impl TabState {
    pub(super) fn new(doc: Document, path: Option<PathBuf>) -> Self {
        let active_layer = doc.layers.len().checked_sub(1);
        Self {
            doc,
            path,
            dirty: false,
            view: View::default(),
            selection: vec![],
            active_layer,
            history: History::default(),
            clone_source: None,
            pixel_sel: None,
            type_edit: None,
            need_fit: true,
            layer_rename: None,
            paste_nudge: 0,
            node_sel: BTreeSet::new(),
            playhead: 0.0,
            playing: false,
            play_clock: 0.0,
            pose_drag: HashMap::new(),
            selected_key: None,
            pending_place: None,
            show_welcome: false,
            artboard_sel: vec![],
            layer_expanded: HashSet::new(),
            swap_id: crate::project::new_swap_id(),
            last_input: Instant::now(),
            last_swap: None,
            shape_rename: None,
            artboard_rename: None,
        }
    }
}

impl Studio {
    fn exchange_tab(&mut self, i: usize) {
        self.end_pixel_stroke(true);
        self.end_deform(true);
        self.reset_snap_gesture();
        self.paint_mask = false;
        let t = &mut self.tabs[i];
        swap(&mut self.doc, &mut t.doc);
        swap(&mut self.path, &mut t.path);
        swap(&mut self.dirty, &mut t.dirty);
        swap(&mut self.view, &mut t.view);
        swap(&mut self.selection, &mut t.selection);
        swap(&mut self.active_layer, &mut t.active_layer);
        swap(&mut self.history, &mut t.history);
        swap(&mut self.clone_source, &mut t.clone_source);
        swap(&mut self.pixel_sel, &mut t.pixel_sel);
        swap(&mut self.type_edit, &mut t.type_edit);
        swap(&mut self.need_fit, &mut t.need_fit);
        swap(&mut self.layer_rename, &mut t.layer_rename);
        swap(&mut self.paste_nudge, &mut t.paste_nudge);
        swap(&mut self.node_sel, &mut t.node_sel);
        swap(&mut self.playhead, &mut t.playhead);
        swap(&mut self.playing, &mut t.playing);
        swap(&mut self.play_clock, &mut t.play_clock);
        swap(&mut self.pose_drag, &mut t.pose_drag);
        swap(&mut self.selected_key, &mut t.selected_key);
        swap(&mut self.pending_place, &mut t.pending_place);
        swap(&mut self.show_welcome, &mut t.show_welcome);
        swap(&mut self.artboard_sel, &mut t.artboard_sel);
        swap(&mut self.layer_expanded, &mut t.layer_expanded);
        swap(&mut self.swap_id, &mut t.swap_id);
        swap(&mut self.last_input, &mut t.last_input);
        swap(&mut self.last_swap, &mut t.last_swap);
        swap(&mut self.shape_rename, &mut t.shape_rename);
        swap(&mut self.artboard_rename, &mut t.artboard_rename);
    }

    fn activate_tab(&mut self) {
        self.op = None;
        self.key_drag = None;
        self.cursor = None;
        self.canvas_key = None;
        self.canvas_gen = self.canvas_gen.wrapping_add(1);
    }

    pub fn ensure_tabs(&mut self) {
        if self.tabs.is_empty() {
            let mut staging = TabState::new(Document::new("", 1.0, 1.0, 72.0), None);
            staging.doc.layers.clear();
            self.tabs.push(staging);
            self.active_tab = 0;
        }
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len().max(1)
    }

    pub fn tab_title(&self, i: usize) -> (&str, bool) {
        let (name, dirty) = if i == self.active_tab {
            (self.doc.name.as_str(), self.dirty)
        } else if let Some(tab) = self.tabs.get(i) {
            (tab.doc.name.as_str(), tab.dirty)
        } else {
            ("", false)
        };
        (if name.is_empty() { "Untitled" } else { name }, dirty)
    }

    pub fn has_unsaved_changes(&self) -> bool {
        self.dirty
            || self
                .tabs
                .iter()
                .enumerate()
                .any(|(i, t)| i != self.active_tab && t.dirty)
    }

    pub fn switch_tab(&mut self, i: usize) {
        self.ensure_tabs();
        if i == self.active_tab || i >= self.tabs.len() {
            return;
        }
        self.commit_type_edit();
        self.playing = false;
        self.exchange_tab(self.active_tab);
        self.active_tab = i;
        self.exchange_tab(i);
        self.activate_tab();
        self.status = format!("tab {}", i + 1);
    }

    pub(super) fn push_tab(&mut self, tab: TabState) {
        self.ensure_tabs();
        self.commit_type_edit();
        self.playing = false;
        self.exchange_tab(self.active_tab);
        self.active_tab = self.tabs.len();
        self.tabs.push(tab);
        self.exchange_tab(self.active_tab);
        self.activate_tab();
    }

    pub(super) fn replace_active_tab(&mut self, tab: TabState) {
        self.tabs[self.active_tab] = tab;
        self.exchange_tab(self.active_tab);
        // The old document occupied the staging slot after the exchange.
        // Drop it instead of retaining a second document in memory.
        let mut staging = TabState::new(Document::new("", 1.0, 1.0, 72.0), None);
        staging.doc.layers.clear();
        self.tabs[self.active_tab] = staging;
        self.activate_tab();
    }

    pub fn new_tab(&mut self) {
        if self.show_welcome && self.current_is_blank() && self.tab_count() <= 1 {
            self.show_welcome = false;
            self.status = "new document".into();
            return;
        }
        self.push_tab(Self::blank_tab_state());
        self.status = "new tab".into();
    }

    pub fn new_tab_welcome(&mut self) {
        self.new_tab();
        self.show_welcome = true;
        self.welcome_page = WelcomePage::New;
    }

    fn blank_tab_state() -> TabState {
        let mut doc = Document::new("Untitled", 1280.0, 800.0, 72.0);
        doc.grid.visible = false;
        TabState::new(doc, None)
    }

    pub fn request_close_tab(&mut self, i: usize) {
        self.ensure_tabs();
        if i >= self.tabs.len() {
            return;
        }
        if i == self.active_tab {
            self.commit_type_edit();
        }
        if self.tab_title(i).1 {
            self.pending_nav = Some(PendingNav::CloseTab(i));
        } else {
            self.close_tab(i);
        }
    }

    pub fn close_tab(&mut self, i: usize) {
        self.ensure_tabs();
        if i >= self.tabs.len() {
            return;
        }
        if i == self.active_tab {
            crate::project::delete_swap(&self.swap_id);
            if self.tabs.len() == 1 {
                self.replace_active_tab(Self::blank_tab_state());
                self.show_welcome = true;
                return;
            }
            self.exchange_tab(i);
            self.tabs.remove(i);
            self.active_tab = i.min(self.tabs.len() - 1);
            self.exchange_tab(self.active_tab);
            self.activate_tab();
        } else {
            crate::project::delete_swap(&self.tabs[i].swap_id);
            self.tabs.remove(i);
            if i < self.active_tab {
                self.active_tab -= 1;
            }
        }
    }

    pub fn recover_swap(&mut self, path: PathBuf) {
        match crate::project::load_swap(&path) {
            Ok(meta) => {
                let mut tab = TabState::new(meta.doc, meta.original);
                tab.dirty = true;
                tab.swap_id = meta.id;
                self.push_tab(tab);
                self.status = format!("recovered {}", meta.name);
            }
            Err(e) => self.status = format!("recover failed: {e}"),
        }
    }

    pub fn delete_swap_file(&mut self, path: &std::path::Path) {
        let _ = std::fs::remove_file(path);
        self.status = "discarded recovery".into();
    }

    pub fn execute_nav(&mut self, ctx: &egui::Context, save: bool) {
        match self.pending_nav.take() {
            Some(PendingNav::CloseTab(i)) => {
                if save {
                    self.switch_tab(i);
                    self.save();
                    if self.dirty {
                        self.pending_nav = Some(PendingNav::CloseTab(self.active_tab));
                        return;
                    }
                    self.close_tab(self.active_tab);
                } else {
                    self.close_tab(i);
                }
            }
            Some(PendingNav::Quit) => {
                if save {
                    for i in 0..self.tabs.len() {
                        self.switch_tab(i);
                        if self.dirty {
                            self.save();
                            if self.dirty {
                                self.pending_nav = Some(PendingNav::Quit);
                                return;
                            }
                        }
                    }
                } else {
                    for (i, tab) in self.tabs.iter_mut().enumerate() {
                        if i != self.active_tab {
                            tab.dirty = false;
                            crate::project::delete_swap(&tab.swap_id);
                        }
                    }
                    self.dirty = false;
                    crate::project::delete_swap(&self.swap_id);
                }
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            None => {}
        }
    }

    pub(super) fn finish_recovery(
        &mut self,
        id: &str,
        revision: Instant,
        saved: crate::project::PreparedSwap,
    ) -> Result<(), String> {
        let saved_revision = if self.swap_id == id && self.dirty {
            Some(&mut self.last_swap)
        } else {
            self.tabs
                .iter_mut()
                .enumerate()
                .find(|(i, tab)| *i != self.active_tab && tab.swap_id == id && tab.dirty)
                .map(|(_, tab)| &mut tab.last_swap)
        };
        if let Some(last_swap) = saved_revision
            && last_swap.is_none_or(|saved_at| saved_at < revision)
        {
            saved.commit()?;
            *last_swap = Some(revision);
        }
        Ok(())
    }

    pub(super) fn pending_recovery(&self) -> Option<recovery::RecoverySource<'_>> {
        let active = (self.dirty && self.last_swap.is_none_or(|saved| saved < self.last_input))
            .then_some(recovery::RecoverySource {
                doc: &self.doc,
                path: &self.path,
                id: &self.swap_id,
                revision: self.last_input,
            });
        active
            .into_iter()
            .chain(
                self.tabs
                    .iter()
                    .enumerate()
                    .filter(|(i, tab)| {
                        *i != self.active_tab
                            && tab.dirty
                            && tab.last_swap.is_none_or(|saved| saved < tab.last_input)
                    })
                    .map(|(_, tab)| recovery::RecoverySource {
                        doc: &tab.doc,
                        path: &tab.path,
                        id: &tab.swap_id,
                        revision: tab.last_input,
                    }),
            )
            .min_by_key(|source| source.revision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_move_rasters_and_restore_independent_history_and_selection() {
        let mut studio = Studio::new();
        studio.show_welcome = false;
        studio.finish_create(CreateKind::Rect, Pt::ZERO, Pt::new(20.0, 20.0));
        let selection = studio.selection.clone();
        let pixels = studio.doc.layers[0].kind.pixels().unwrap().data.as_ptr();
        studio.pixel_sel = Some(vec![255; 64]);
        let mask = studio.pixel_sel.as_ref().unwrap().as_ptr();
        studio.view.scale = 2.5;

        studio.new_tab();
        assert!(studio.tabs[studio.active_tab].doc.layers.is_empty());
        assert_eq!(
            studio.tabs[0].doc.layers[0]
                .kind
                .pixels()
                .unwrap()
                .data
                .as_ptr(),
            pixels
        );
        assert!(!studio.history.can_undo());
        assert!(studio.selection.is_empty());

        studio.switch_tab(0);
        assert_eq!(
            studio.doc.layers[0].kind.pixels().unwrap().data.as_ptr(),
            pixels
        );
        assert_eq!(studio.pixel_sel.as_ref().unwrap().as_ptr(), mask);
        assert_eq!(studio.selection, selection);
        assert_eq!(studio.view.scale, 2.5);
        studio.undo();
        assert!(
            studio
                .doc
                .find_shape(selection[0].0, selection[0].1)
                .is_none()
        );
        studio.switch_tab(1);
        assert!(!studio.history.can_redo());
    }

    #[test]
    fn closing_an_inactive_tab_preserves_the_live_document_and_operation() {
        let mut studio = Studio::new();
        studio.show_welcome = false;
        studio.new_tab();
        studio.new_tab();
        studio.doc.name = "Keep editing".into();
        studio.op = Some(Op::Pencil {
            pts: vec![Pt::ZERO],
        });
        let revision = studio.canvas_gen;
        studio.close_tab(0);

        assert_eq!(studio.active_tab, 1);
        assert_eq!(studio.doc.name, "Keep editing");
        assert!(matches!(studio.op, Some(Op::Pencil { .. })));
        assert_eq!(studio.canvas_gen, revision);
        studio.close_tab(99);
        assert_eq!(studio.tab_count(), 2);
        studio.close_tab(1);
        assert_eq!(studio.active_tab, 0);
        assert_eq!(studio.doc.name, "Untitled");
        studio.close_tab(0);
        assert!(studio.show_welcome);
        assert_eq!(studio.tab_count(), 1);
        assert!(!studio.has_unsaved_changes());
    }

    #[test]
    fn undo_and_redo_after_saving_require_another_save() {
        let mut studio = Studio::new();
        studio.finish_create(CreateKind::Rect, Pt::ZERO, Pt::new(20.0, 20.0));
        studio.dirty = false;
        studio.undo();
        assert!(studio.has_unsaved_changes());
        studio.dirty = false;
        studio.redo();
        assert!(studio.has_unsaved_changes());
    }
}
