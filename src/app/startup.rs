//! Persist launch choices separately from document state.
use super::{Studio, WelcomePage};
use crate::tools::{Persona, Tool};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartTab {
    #[default]
    Welcome,
    Mode(Persona),
    RememberLast,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartPage {
    Page(WelcomePage),
    #[default]
    RememberLast,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub start_tab: StartTab,
    pub start_page: StartPage,
    pub last_mode: Persona,
    pub last_page: WelcomePage,
    pub ui_font: String,
    pub ui_font_size: u16,
    pub anonymous_usage: bool,
    pub check_updates: bool,
    pub guides_locked_by_default: bool,
    pub show_rulers: bool,
    pub show_key_hud: bool,
    #[serde(flatten)]
    pub extra: std::collections::BTreeMap<String, serde_json::Value>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            start_tab: StartTab::Welcome,
            start_page: StartPage::RememberLast,
            last_mode: Persona::Design,
            last_page: WelcomePage::New,
            ui_font: String::new(),
            ui_font_size: 13,
            anonymous_usage: false,
            check_updates: true,
            guides_locked_by_default: true,
            show_rulers: true,
            show_key_hud: true,
            extra: Default::default(),
        }
    }
}

impl Preferences {
    pub fn welcome_page(&self) -> WelcomePage {
        match self.start_page {
            StartPage::Page(page) => page,
            StartPage::RememberLast => self.last_page,
        }
    }

    pub(crate) fn read(path: &Path) -> Result<Self, String> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.to_string()),
        }
    }

    fn write(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        crate::formats::write_atomic(path, &bytes)
    }
}

pub(crate) fn preferences_path() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into())).join(".config")
        })
        .join("omadesign/preferences.json")
}

impl Studio {
    /// Native startup opts into persistence; tests and screenshot scenes remain isolated.
    pub fn load_startup_preferences(&mut self) {
        self.load_startup_preferences_from(preferences_path());
    }

    fn load_startup_preferences_from(&mut self, path: PathBuf) {
        self.startup_preferences_path = Some(path.clone());
        match Preferences::read(&path) {
            Ok(preferences) => {
                self.startup_preferences = preferences;
                crate::telemetry::set_consent(self.startup_preferences.anonymous_usage);
                self.startup_preferences.ui_font_size =
                    self.startup_preferences.ui_font_size.clamp(10, 24);
                self.show_rulers = self.startup_preferences.show_rulers;
                self.show_key_hud = self.startup_preferences.show_key_hud;
                self.doc.ruler.guides_locked = self.startup_preferences.guides_locked_by_default;
                self.welcome_page = self.startup_preferences.welcome_page();
                let mode = match self.startup_preferences.start_tab {
                    StartTab::Welcome => None,
                    StartTab::Mode(mode) => Some(mode),
                    StartTab::RememberLast => Some(self.startup_preferences.last_mode),
                };
                if let Some(mode) = mode {
                    self.switch_persona(mode);
                }
            }
            Err(error) => self.status = format!("Could not load startup preferences: {error}"),
        }
    }

    pub fn save_startup_preferences(&mut self) {
        if let Some(path) = &self.startup_preferences_path
            && let Err(error) = self.startup_preferences.write(path)
        {
            self.status = format!("Could not save startup preferences: {error}");
        }
    }

    pub fn set_welcome_page(&mut self, page: WelcomePage) {
        self.welcome_page = page;
        if self.startup_preferences.last_page != page {
            self.startup_preferences.last_page = page;
            self.save_startup_preferences();
        }
    }

    pub fn remember_current_mode(&mut self) {
        if !self.show_welcome && self.startup_preferences.last_mode != self.persona {
            self.startup_preferences.last_mode = self.persona;
            self.save_startup_preferences();
        }
    }

    pub fn switch_persona(&mut self, persona: Persona) {
        if self.persona == persona && !self.show_welcome {
            return;
        }
        self.end_deform(true);
        self.end_pixel_stroke(true);
        self.commit_type_edit();
        self.reset_snap_gesture();
        // A welcome placeholder is not a user-created document. Photo and
        // Layout can start working without first choosing an artboard preset.
        if self.show_welcome
            && self.current_is_blank()
            && matches!(persona, Persona::Photo | Persona::Layout)
        {
            self.doc.artboards.clear();
            self.doc.artboardless = true;
            self.doc.layers = vec![crate::document::Layer::vector("Layer 1")];
            self.doc.transparent = true;
            self.active_layer = Some(0);
            self.canvas_gen = self.canvas_gen.wrapping_add(1);
            self.canvas_key = None;
            self.need_fit = true;
        }
        self.persona = persona;
        self.op = None;
        self.playing = false;
        self.tool = match persona {
            Persona::Design | Persona::Motion | Persona::Layout => Tool::Select,
            Persona::Pixel => Tool::Brush,
            Persona::Photo => Tool::Hand,
        };
        self.show_welcome = false;
        if self.startup_preferences.last_mode != persona {
            self.startup_preferences.last_mode = persona;
            self.save_startup_preferences();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn photo_and_layout_open_from_welcome_without_an_artboard() {
        for persona in [Persona::Photo, Persona::Layout] {
            let mut studio = Studio::new();
            studio.switch_persona(persona);
            assert!(!studio.show_welcome);
            assert_eq!(studio.persona, persona);
            assert!(studio.doc.artboards.is_empty());
            assert!(!studio.dirty);
            if persona == Persona::Layout {
                studio.insert_layout_frame("Phone", 390.0, 844.0);
                assert!(studio.dirty);
                assert!(studio.primary().is_some());
            }
        }
    }

    #[test]
    fn direct_layout_workspace_round_trips_without_creating_an_artboard() {
        let mut studio = Studio::new();
        studio.switch_persona(Persona::Layout);
        studio.insert_layout_frame("Phone", 390.0, 844.0);
        let frame_id = studio.primary().unwrap().1;
        let saved = crate::project::encode(&studio.doc).unwrap();
        let reopened = crate::project::decode(&saved).unwrap();
        assert!(reopened.artboardless);
        assert!(reopened.artboards.is_empty());
        assert!(reopened.layers[0].find(frame_id).unwrap().layout.frame);
        assert!(reopened.layout_snapshot().artboardless);
        studio.open_document(reopened, None);
        assert_eq!(studio.persona, Persona::Layout);
        assert!(studio.doc.artboards.is_empty());
        studio.insert_layout_frame("Tablet", 768.0, 1024.0);
        assert_eq!(studio.doc.layers[0].kind.shapes().unwrap().len(), 2);
        assert!(studio.doc.artboards.is_empty());
    }

    #[test]
    fn saved_workspace_reopens_with_its_mode_and_survives_tab_switches() {
        let mut studio = Studio::new();
        for persona in [
            Persona::Design,
            Persona::Pixel,
            Persona::Layout,
            Persona::Photo,
            Persona::Motion,
        ] {
            let mut document = crate::document::Document::new("Workspace", 48., 32., 72.);
            document.workspace = Some(persona);
            let saved = crate::project::encode(&document).unwrap();
            let reopened = crate::project::decode(&saved).unwrap();
            assert_eq!(reopened.workspace, Some(persona));
            studio.open_document(reopened, None);
            assert_eq!(studio.persona, persona);
            assert_eq!(
                studio.tool,
                match persona {
                    Persona::Pixel => Tool::Brush,
                    Persona::Photo => Tool::Hand,
                    _ => Tool::Select,
                }
            );
            studio.dirty = true;
        }
        studio.switch_tab(1);
        assert_eq!(studio.persona, Persona::Pixel);
        studio.remember_current_mode();
        assert_eq!(studio.doc.workspace, Some(Persona::Pixel));
        studio.switch_tab(2);
        assert_eq!(studio.persona, Persona::Layout);
    }

    #[test]
    fn legacy_empty_artboards_still_reopen_with_their_default_page() {
        let mut document = crate::document::Document::new("Legacy", 320.0, 240.0, 72.0);
        document.artboards.clear();
        let mut saved: serde_json::Value =
            serde_json::from_str(&crate::project::encode(&document).unwrap()).unwrap();
        saved["doc"].as_object_mut().unwrap().remove("artboardless");
        let reopened = crate::project::decode(&serde_json::to_string(&saved).unwrap()).unwrap();
        assert!(!reopened.artboardless);
        assert_eq!(reopened.artboards.len(), 1);
        assert_eq!(
            reopened.artboards[0].size,
            crate::geom::Pt::new(320.0, 240.0)
        );
    }

    #[test]
    fn same_mode_can_leave_welcome_and_existing_artwork_is_preserved() {
        let mut studio = Studio::new();
        studio.switch_persona(Persona::Design);
        assert!(!studio.show_welcome);
        studio.seed_demo();
        let before = crate::project::encode(&studio.doc).unwrap();
        studio.switch_persona(Persona::Layout);
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    }

    #[test]
    fn startup_preferences_persist_fixed_and_remembered_choices() {
        let root = std::env::temp_dir().join(format!(
            "omadesign-startup-{}-{}",
            std::process::id(),
            crate::document::next_id()
        ));
        let path = root.join("preferences.json");
        let mut first = Studio::new();
        first.load_startup_preferences_from(path.clone());
        first.startup_preferences.start_tab = StartTab::RememberLast;
        first.switch_persona(Persona::Photo);
        first.set_welcome_page(WelcomePage::Recents);
        let mut second = Studio::new();
        second.load_startup_preferences_from(path.clone());
        assert_eq!(second.persona, Persona::Photo);
        assert!(!second.show_welcome);
        assert_eq!(second.welcome_page, WelcomePage::Recents);
        second.startup_preferences.start_tab = StartTab::Mode(Persona::Layout);
        second.startup_preferences.start_page = StartPage::Page(WelcomePage::Templates);
        second.save_startup_preferences();
        let mut third = Studio::new();
        third.load_startup_preferences_from(path.clone());
        assert_eq!(third.persona, Persona::Layout);
        assert_eq!(third.welcome_page, WelcomePage::Templates);
        third.startup_preferences.start_tab = StartTab::Welcome;
        third.save_startup_preferences();
        let mut fourth = Studio::new();
        fourth.load_startup_preferences_from(path);
        assert!(fourth.show_welcome);
        assert_eq!(fourth.welcome_page, WelcomePage::Templates);
        std::fs::remove_dir_all(root).unwrap();
    }
}
