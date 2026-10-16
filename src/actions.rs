//! Actions retain explicit editor commands, never guesses from artwork differences.
use crate::{geom::Pt, graphic_styles::Appearance};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) enum Step {
    Duplicate { delta: Pt },
    Group,
    Ungroup,
    Flip { horizontal: bool },
    Outline { selected: Option<usize> },
    Front,
    Back,
    Appearance(Box<Appearance>),
}
impl Step {
    /// List commands that can also be added without recording.
    /// Args: none. Returns the original eight built-in action choices.
    pub fn defaults() -> [Self; 8] {
        [
            Self::Duplicate { delta: Pt::ZERO },
            Self::Group,
            Self::Ungroup,
            Self::Flip { horizontal: true },
            Self::Flip { horizontal: false },
            Self::Outline { selected: None },
            Self::Front,
            Self::Back,
        ]
    }
    /// Describe the actual recorded parameters.
    /// Args: none. Returns a compact step label.
    pub fn name(&self) -> String {
        match self {
            Self::Duplicate { delta } if *delta != Pt::ZERO => {
                format!("Duplicate ({}, {})", delta.x, delta.y)
            }
            Self::Duplicate { .. } => "Duplicate".into(),
            Self::Group => "Group".into(),
            Self::Ungroup => "Ungroup".into(),
            Self::Flip { horizontal: true } => "Flip horizontal".into(),
            Self::Flip { horizontal: false } => "Flip vertical".into(),
            Self::Outline {
                selected: Some(index),
            } => format!("Create outlines (object {})", index.saturating_add(1)),
            Self::Outline { selected: None } => "Create outlines".into(),
            Self::Front => "Bring to front".into(),
            Self::Back => "Send to back".into(),
            Self::Appearance(_) => "Apply appearance".into(),
        }
    }
    /// Validate portable command parameters before any edit.
    /// Args: none. Returns an error for non-finite positions or damaged appearance resources.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Duplicate { delta } if !delta.x.is_finite() || !delta.y.is_finite() => {
                Err("Action offsets must be finite".into())
            }
            Self::Appearance(value) => crate::graphic_styles::validate_appearance(value),
            Self::Outline {
                selected: Some(index),
            } if *index == usize::MAX => {
                Err("The action's selected object index is invalid".into())
            }
            _ => Ok(()),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Action {
    pub id: u64,
    pub name: String,
    pub steps: Vec<Step>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Library {
    pub entries: Vec<Action>,
    pub next: u64,
    #[serde(skip)]
    pub stamp: Option<u64>,
}
#[derive(Serialize, Deserialize)]
struct File {
    version: u8,
    library: Library,
}
impl Library {
    /// Read one bounded action library without replacing unreadable bytes.
    /// Args: file path. Returns the actual file version and fingerprint, or a visible error.
    pub fn load(path: &Path) -> Result<Self, String> {
        let Some((bytes, stamp)) = crate::app::libraries::palette_bytes(path)? else {
            return Ok(Self::default());
        };
        let file: File =
            serde_json::from_slice(&bytes).map_err(|e| format!("Cannot read actions: {e}"))?;
        if file.version != 1 {
            return Err("Unsupported action library version; original file preserved".into());
        }
        file.library.validate()?;
        Ok(Self {
            stamp: Some(stamp),
            ..file.library
        })
    }
    /// Atomically save only the loaded file version.
    /// Args: destination. Returns success after the guarded write and updates its fingerprint.
    pub fn save(&mut self, path: &Path) -> Result<(), String> {
        self.validate()?;
        self.stamp = crate::preset_file::save(
            path,
            self.stamp,
            &File {
                version: 1,
                library: self.clone(),
            },
        )?;
        Ok(())
    }
    /// Append a unique named action with a stable library-local ID.
    /// Args: name and explicit steps. Returns the new ID without writing a file.
    pub fn add(&mut self, name: &str, steps: Vec<Step>) -> Result<u64, String> {
        let mut name = name.trim().to_owned();
        let base = name.clone();
        for index in 2..=258 {
            if !self
                .entries
                .iter()
                .any(|e| e.name.to_lowercase() == name.to_lowercase())
            {
                break;
            }
            let suffix = format!(" ({index})");
            name = format!(
                "{}{}",
                base.chars().take(64 - suffix.len()).collect::<String>(),
                suffix
            );
        }
        let id = self.next.checked_add(1).ok_or("Action library is full")?;
        let mut after = self.clone();
        after.next = id;
        after.entries.push(Action { id, name, steps });
        after.validate()?;
        *self = after;
        Ok(id)
    }
    fn validate(&self) -> Result<(), String> {
        if self.entries.len() > 256 || self.next == u64::MAX {
            return Err("Action library is full".into());
        }
        let mut ids = std::collections::HashSet::new();
        let mut names = std::collections::HashSet::new();
        for entry in &self.entries {
            if entry.id == 0
                || entry.id > self.next
                || !ids.insert(entry.id)
                || !names.insert(entry.name.to_lowercase())
                || entry.name.trim().is_empty()
                || entry.name.chars().count() > 64
                || entry.name.chars().any(char::is_control)
                || entry.steps.len() > 32
            {
                return Err("Actions need unique names and IDs, with at most 32 steps".into());
            }
            for step in &entry.steps {
                step.validate()?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Selection {
    pub objects: Vec<(u64, u64)>,
    pub layer: Option<u64>,
    pub active: Option<u64>,
    pub individual: Option<(u64, u64)>,
}
#[derive(Clone)]
pub(crate) struct Recording {
    pub document: String,
    pub steps: Vec<Step>,
    pub paused: Option<String>,
    selection: Selection,
    limit: usize,
    budget: Library,
    target: u64,
}
#[derive(Default)]
pub(crate) struct Recorder {
    pub recording: Option<Recording>,
    pub depth: usize,
    pub serial: u64,
    pub playing: bool,
}
impl Recorder {
    /// Begin a document-bound recording without touching artwork.
    /// Args: document identity and remaining step capacity. Returns an error if another recording owns the editor.
    pub fn start(
        &mut self,
        document: String,
        selection: Selection,
        limit: usize,
    ) -> Result<(), String> {
        if self.recording.is_some() || self.playing || limit == 0 || limit > 32 {
            return Err("Stop the current action or choose one with room for more steps".into());
        }
        self.recording = Some(Recording {
            document,
            steps: Vec::new(),
            paused: None,
            selection,
            limit,
            budget: {
                let mut book = Library::default();
                book.add("Recording", Vec::new())?;
                book
            },
            target: 1,
        });
        Ok(())
    }
    /// Account for the actual destination library before capturing more data.
    /// Args: loaded library and concrete action ID. Returns an error if that action is no longer available.
    pub fn bind_library(&mut self, book: Library, target: u64) -> Result<(), String> {
        let action = book
            .entries
            .iter()
            .find(|a| a.id == target)
            .ok_or("The recorded action is missing")?;
        let limit = 32usize
            .checked_sub(action.steps.len())
            .filter(|n| *n > 0)
            .ok_or("The action is full")?;
        let recording = self.recording.as_mut().ok_or("Start recording first")?;
        recording.limit = limit;
        recording.budget = book;
        recording.target = target;
        Ok(())
    }
    /// Preserve already recorded commands when an unsupported edit is encountered.
    /// Args: explanation. Returns nothing; saving or discarding remains explicit.
    pub fn pause(&mut self, reason: &str) {
        if !self.playing
            && let Some(recording) = &mut self.recording
            && recording.paused.is_none()
        {
            recording.paused = Some(reason.into());
        }
    }
    /// Check that a recording still belongs to the same open document.
    /// Args: current document identity. Returns nothing; old recordings pause rather than crossing documents.
    pub fn check_document(&mut self, document: &str) {
        if self
            .recording
            .as_ref()
            .is_some_and(|r| r.document != document)
        {
            self.pause("Recording paused because the document changed");
        }
    }
    /// Stop a recording when its target selection changes outside a recorded command.
    /// Args: current concrete target identity. Returns nothing; already captured steps remain available.
    pub fn check_selection(&mut self, selection: &Selection) {
        if self
            .recording
            .as_ref()
            .is_some_and(|r| &r.selection != selection)
        {
            self.pause(
                "Selection changed outside a recorded command. Save or discard the recorded steps",
            );
        }
    }
    /// Observe a real accepted command without interpreting its resulting artwork.
    /// Args: current document. Returns nothing; unsupported commands pause recording.
    pub fn committed(&mut self, document: &str) {
        if self.recording.is_none() && !self.playing {
            return;
        }
        self.serial = self.serial.wrapping_add(1);
        self.check_document(document);
        if self.depth == 0 {
            self.pause("This edit cannot be recorded yet. Save or discard the recorded steps");
        }
    }
    /// Append one top-level semantic command after its actual edit was accepted.
    /// Args: current document and explicit command. Returns nothing; full recordings pause.
    pub fn append(&mut self, document: &str, step: Step, selection: Selection) {
        self.check_document(document);
        if let Some(recording) = &mut self.recording
            && recording.paused.is_none()
        {
            let result = (|| -> Result<Library, String> {
                step.validate()?;
                let mut book = recording.budget.clone();
                book.entries
                    .iter_mut()
                    .find(|a| a.id == recording.target)
                    .ok_or("The recorded action is missing")?
                    .steps
                    .push(step.clone());
                let prepared = File {
                    version: 1,
                    library: book,
                };
                crate::preset_file::encoded(&prepared)?;
                Ok(prepared.library)
            })();
            match result {
                Ok(book) => recording.budget = book,
                Err(error) => {
                    recording.paused = Some(error);
                    return;
                }
            }
            recording.steps.push(step);
            recording.selection = selection;
            if recording.steps.len() >= recording.limit {
                recording.paused =
                    Some("The action reached its step limit. Save the recording".into());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn craft_actions_library_cold_roundtrip_and_stale_corrupt_future_refusals_preserve_bytes() {
        let folder =
            std::env::temp_dir().join(format!("action-library-{}", crate::document::next_id()));
        let path = folder.join("actions.json");
        let mut book = Library::load(&path).unwrap();
        let steps = vec![
            Step::Duplicate {
                delta: Pt::new(20., 30.),
            },
            Step::Flip { horizontal: false },
        ];
        let id = book.add("Offset and flip", steps.clone()).unwrap();
        book.save(&path).unwrap();
        let mut cold = Library::load(&path).unwrap();
        assert_eq!(
            cold.entries[0],
            Action {
                id,
                name: "Offset and flip".into(),
                steps
            }
        );
        cold.entries[0].name = "Another app".into();
        cold.save(&path).unwrap();
        let original = std::fs::read(&path).unwrap();
        assert!(book.save(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        for bytes in [
            b"{broken".as_slice(),
            b"{\"version\":2,\"library\":{\"entries\":[],\"next\":0}}".as_slice(),
        ] {
            std::fs::write(&path, bytes).unwrap();
            assert!(Library::load(&path).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
        std::fs::write(&path, &original).unwrap();
        cold.entries[0].steps[0] = Step::Duplicate {
            delta: Pt::new(f32::NAN, 0.),
        };
        assert!(cold.save(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        std::fs::remove_dir_all(folder).unwrap();
    }
}
