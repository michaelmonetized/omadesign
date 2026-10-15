use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArtworkKind {
    Rectangle,
    Ellipse,
    Polygon,
    Star,
    Line,
    Frame,
    Text,
    Path,
    Artboard,
}

impl ArtworkKind {
    pub(crate) const ALL: [Self; 9] = [
        Self::Rectangle,
        Self::Ellipse,
        Self::Polygon,
        Self::Star,
        Self::Line,
        Self::Frame,
        Self::Text,
        Self::Path,
        Self::Artboard,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
            Self::Polygon => "Polygon",
            Self::Star => "Star",
            Self::Line => "Line",
            Self::Frame => "Frame",
            Self::Text => "Text",
            Self::Path => "Path",
            Self::Artboard => "Artboard",
        }
    }

    pub(crate) fn for_tool(tool: Tool) -> Option<Self> {
        Some(match tool {
            Tool::Rect => Self::Rectangle,
            Tool::Ellipse => Self::Ellipse,
            Tool::Polygon => Self::Polygon,
            Tool::Star => Self::Star,
            Tool::Line => Self::Line,
            Tool::Frame => Self::Frame,
            Tool::Text => Self::Text,
            Tool::Pen | Tool::Pencil => Self::Path,
            Tool::Artboard => Self::Artboard,
            _ => return None,
        })
    }
}

#[derive(Clone)]
pub(crate) struct ArtworkDraft {
    owner: String,
    generation: u64,
    selection: Vec<(usize, u64)>,
    persona: Persona,
    layer: Option<u64>,
    pub kind: ArtworkKind,
    pub coordinates: [String; 4],
    pub text: String,
    pub points: String,
    pub closed: bool,
}

impl ArtworkDraft {
    /// Capture a keyboard creation target.
    /// Args: editor and artwork kind. Returns a draft tied to this document and selection.
    pub(crate) fn new(studio: &mut Studio, kind: ArtworkKind) -> Self {
        let layer = studio.vector_target().map(|i| studio.doc.layers[i].id);
        Self {
            owner: studio.swap_id.clone(),
            generation: studio.canvas_gen,
            selection: studio.selection.clone(),
            persona: studio.persona,
            layer,
            kind,
            coordinates: [
                (studio.doc.width * 0.5 - 50.).to_string(),
                (studio.doc.height * 0.5 - 50.).to_string(),
                "100".into(),
                "100".into(),
            ],
            text: "Text".into(),
            points: "0,0; 100,0; 100,100".into(),
            closed: false,
        }
    }

    pub(crate) fn current(&self, studio: &Studio) -> bool {
        self.owner == studio.swap_id
            && self.generation == studio.canvas_gen
            && self.selection == studio.selection
            && self.persona == studio.persona
    }

    pub(crate) fn number(&self, index: usize) -> Result<f32, String> {
        let name = ["Position X", "Position Y", "Width", "Height"][index];
        let value = self.coordinates[index]
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && v.abs() <= 1_000_000.);
        let Some(value) = value else {
            return Err(format!(
                "{name}: enter a finite number from -1000000 to 1000000"
            ));
        };
        if index >= 2 && (value < 0. || (value < 2. && self.kind != ArtworkKind::Line)) {
            return Err(format!("{name}: enter at least 2 px"));
        }
        Ok(value)
    }

    fn anchors(&self, origin: Pt) -> Result<Vec<Anchor>, String> {
        let mut anchors = Vec::new();
        for pair in self.points.split(';') {
            let Some((x, y)) = pair.trim().split_once(',') else {
                return Err("Path points: use x,y; x,y for each point".into());
            };
            let coordinate = |s: &str| {
                s.trim()
                    .parse::<f32>()
                    .ok()
                    .filter(|n| n.is_finite() && n.abs() <= 1_000_000.)
            };
            let (Some(x), Some(y)) = (coordinate(x), coordinate(y)) else {
                return Err(
                    "Path points: each coordinate must be a finite number within 1000000 px".into(),
                );
            };
            anchors.push(Anchor::corner(origin + Pt::new(x, y)));
            if anchors.len() > 4096 {
                return Err("Path points: use at most 4096 points".into());
            }
        }
        if anchors.len() < if self.closed { 3 } else { 2 } {
            return Err(
                "Path points: an open path needs two points; a closed path needs three".into(),
            );
        }
        Ok(anchors)
    }

    pub(crate) fn validate(&self) -> Result<(Pt, Pt), String> {
        let start = Pt::new(self.number(0)?, self.number(1)?);
        let size = Pt::new(self.number(2)?, self.number(3)?);
        if self.kind == ArtworkKind::Line && size.length() < 2. {
            return Err("Line: enter a width or height of at least 2 px".into());
        }
        if self.kind == ArtworkKind::Path {
            self.anchors(start)?;
        }
        if self.kind == ArtworkKind::Text && self.text.trim().is_empty() {
            return Err("Text: enter some text".into());
        }
        Ok((start, start + size))
    }
}

impl Studio {
    /// Move only the artwork that the normal selection tools allow editing.
    /// Args: canvas delta. Returns without a command if any selected owner became hidden or locked.
    pub(crate) fn nudge_keyboard_artwork(&mut self, dx: f32, dy: f32) {
        let eligible: std::collections::HashSet<_> =
            self.selectable_objects().into_iter().collect();
        if self
            .selection
            .iter()
            .any(|target| !eligible.contains(target))
        {
            self.status = "Select visible, unlocked artwork to move".into();
            return;
        }
        self.nudge(dx, dy);
    }

    /// Create artwork through the existing document tools.
    /// Args: validated keyboard draft. Returns an error without editing for stale, busy or locked targets.
    pub(crate) fn create_keyboard_artwork(&mut self, draft: &ArtworkDraft) -> Result<(), String> {
        if !draft.current(self) {
            return Err("The document or selection changed. Open Create artwork again.".into());
        }
        if self.show_welcome
            || self.file_dialog_pending()
            || self.updates.freezing
            || !matches!(
                self.persona,
                Persona::Design | Persona::Layout | Persona::Motion
            )
            || self.op.is_some()
            || self.type_edit.is_some()
            || self.free_transform.is_some()
        {
            return Err("Finish the current edit before creating artwork.".into());
        }
        let (start, end) = draft.validate()?;
        if draft.kind != ArtworkKind::Artboard {
            let Some(index) = self
                .doc
                .layers
                .iter()
                .position(|l| Some(l.id) == draft.layer)
            else {
                return Err("The target vector layer is no longer available.".into());
            };
            if !self.doc.layer_editable(index) || self.doc.layers[index].kind.shapes().is_none() {
                return Err("The target vector layer is hidden or locked.".into());
            }
            self.active_layer = Some(index);
        }
        match draft.kind {
            ArtworkKind::Text => {
                self.place_text(start);
                self.type_insert(&draft.text);
                self.tool = Tool::Text;
            }
            ArtworkKind::Path => self.finish_pen(draft.anchors(start)?, draft.closed, None),
            ArtworkKind::Artboard => {
                let mut board = Artboard::new(self.doc.artboards.len(), start, end - start);
                board.name = self.doc.unique_artboard_name(&board.name);
                let id = board.id;
                let mut boards = self.doc.artboards.clone();
                boards.push(board);
                self.commit_artboards(boards);
                self.selected_objects(vec![]);
                self.artboard_sel = vec![id];
            }
            kind => self.finish_create(
                match kind {
                    ArtworkKind::Rectangle => CreateKind::Rect,
                    ArtworkKind::Ellipse => CreateKind::Ellipse,
                    ArtworkKind::Polygon => CreateKind::Polygon,
                    ArtworkKind::Star => CreateKind::Star,
                    ArtworkKind::Line => CreateKind::Line,
                    ArtworkKind::Frame => CreateKind::Frame,
                    _ => unreachable!(),
                },
                start,
                end,
            ),
        }
        Ok(())
    }

    /// Select the next or previous editable artwork item.
    /// Args: direction and whether to extend. Returns after updating the app's normal selection state.
    pub(crate) fn cycle_keyboard_artwork(&mut self, backward: bool, extend: bool) {
        if self.op.is_some() || self.type_edit.is_some() {
            return;
        }
        let objects = self.selectable_objects();
        if objects.is_empty() {
            self.status = "No visible, unlocked artwork".into();
            return;
        }
        let current = self
            .selection
            .last()
            .and_then(|id| objects.iter().position(|o| o == id));
        let next = match (current, backward) {
            (Some(0), true) | (None, true) => objects.len() - 1,
            (Some(i), true) => i - 1,
            (Some(i), false) => (i + 1) % objects.len(),
            (None, false) => 0,
        };
        let target = objects[next];
        let mut selection = if extend {
            self.selection.clone()
        } else {
            vec![]
        };
        if !selection.contains(&target) {
            selection.push(target);
        }
        self.commit_type_edit();
        self.selected_objects(selection);
        self.active_layer = Some(target.0);
        self.status = self
            .doc
            .find_shape(target.0, target.1)
            .map(|s| format!("Selected {} · {} of {}", s.name, next + 1, objects.len()))
            .unwrap_or_else(|| format!("Selected {}", self.doc.layers[target.0].name));
    }

    pub(crate) fn edit_keyboard_text(&mut self) -> bool {
        let Some(hit) = self.primary() else {
            return false;
        };
        let Some(shape) = self.doc.find_shape(hit.0, hit.1) else {
            return false;
        };
        if !self.doc.layer_editable(hit.0) || shape.locked || !shape.visible {
            return false;
        }
        let Geom::Text(run) = &shape.geom else {
            return false;
        };
        let at = run.origin;
        self.set_tool(Tool::Text);
        self.begin_type_edit(hit, at);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_authoring_all_creation_tools_produce_editable_undoable_commands() {
        for kind in ArtworkKind::ALL {
            let mut studio = Studio::new();
            studio.doc = Document::new("Keyboard artwork", 128., 96., 72.);
            studio.show_welcome = false;
            let mut draft = ArtworkDraft::new(&mut studio, kind);
            draft.text = "Editable text".into();
            let before = crate::project::encode(&studio.doc).unwrap();
            studio.create_keyboard_artwork(&draft).unwrap();
            studio.commit_type_edit();
            let after = crate::project::encode(&studio.doc).unwrap();
            assert_ne!(before, after, "{kind:?}");
            if kind == ArtworkKind::Artboard {
                assert_eq!(studio.artboard_sel.len(), 1);
            } else {
                let (layer, id) = studio.primary().expect("new artwork selected");
                assert!(studio.doc.find_shape(layer, id).is_some());
                assert!(studio.doc.layer_editable(layer));
            }
            while studio.history.can_undo() {
                studio.undo();
            }
            assert_eq!(
                crate::project::encode(&studio.doc).unwrap(),
                before,
                "{kind:?}"
            );
        }
    }

    #[test]
    fn keyboard_authoring_selection_skips_hidden_locked_and_stale_items() {
        let mut studio = Studio::new();
        studio.doc = Document::new("Keyboard selection", 128., 96., 72.);
        studio.show_welcome = false;
        let mut ids = Vec::new();
        for _ in 0..3 {
            let draft = ArtworkDraft::new(&mut studio, ArtworkKind::Rectangle);
            studio.create_keyboard_artwork(&draft).unwrap();
            ids.push(studio.primary().unwrap());
        }
        studio
            .doc
            .find_shape_mut(ids[1].0, ids[1].1)
            .unwrap()
            .locked = true;
        studio
            .doc
            .find_shape_mut(ids[2].0, ids[2].1)
            .unwrap()
            .visible = false;
        studio.selected_objects(vec![(0, u64::MAX)]);
        studio.cycle_keyboard_artwork(false, false);
        assert_eq!(studio.selection, vec![ids[0]]);
        studio.cycle_keyboard_artwork(true, true);
        assert_eq!(studio.selection, vec![ids[0]]);
        studio.doc.layers[ids[0].0].locked = true;
        let before = studio.selection.clone();
        studio.cycle_keyboard_artwork(false, false);
        assert_eq!(studio.selection, before);
        assert_eq!(studio.status, "No visible, unlocked artwork");
    }
}
