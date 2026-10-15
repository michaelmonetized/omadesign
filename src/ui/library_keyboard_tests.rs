use super::*;
use crate::ui::welcome::keyboard_tests as keys;
use egui::{Event, Key, Modifiers};

struct Fixture {
    root: PathBuf,
    ctx: egui::Context,
    studio: Studio,
}

impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".artifacts")
            .join(format!(
                "keyboard-library-{}-{}",
                std::process::id(),
                crate::document::next_id()
            ));
        std::fs::create_dir_all(&root).unwrap();
        crate::palette::save_file(root.join(".omacolors"), &[]).unwrap();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        theme::apply(&ctx);
        let mut studio = Studio::new();
        studio.doc = crate::document::Document::new("Keyboard library authoring", 500., 400., 72.);
        studio.path = Some(root.join("artwork.oma"));
        studio.show_welcome = false;
        studio.persona = crate::tools::Persona::Design;
        studio
            .libraries
            .choose_folder(&studio.swap_id, root.clone());
        assert!(studio.selection.is_empty());
        let mut fixture = Self { root, ctx, studio };
        fixture.until(|fixture| {
            fixture
                .studio
                .libraries
                .projects
                .get(&fixture.root)
                .is_some_and(|draft| draft.loaded)
        });
        fixture
    }

    fn frame(&mut self, events: Vec<Event>) -> egui::FullOutput {
        keys::render(&self.ctx, events, |ui| {
            tick(ui.ctx(), &mut self.studio);
            self.studio.handle_shortcuts(ui.ctx());
            egui::Panel::right("keyboard-library-native-panel")
                .exact_size(380.)
                .show(ui, |ui| {
                    tabs(ui, &mut self.studio);
                    show(ui, &mut self.studio);
                });
            crate::ui::canvas::show(ui, &mut self.studio);
            crate::ui::keyboard_authoring::show(ui.ctx(), &mut self.studio);
        })
    }

    fn press(&mut self, name: &str) -> egui::FullOutput {
        let ctx = self.ctx.clone();
        keys::tab_to(&ctx, name, |events| self.frame(events));
        self.frame(keys::key(Key::Enter, Modifiers::NONE));
        self.frame(vec![])
    }

    fn text(&mut self, name: &str, value: &str) -> egui::FullOutput {
        let ctx = self.ctx.clone();
        keys::tab_to(&ctx, name, |events| self.frame(events));
        keys::replace(value, |events| self.frame(events));
        self.frame(vec![])
    }

    fn until(&mut self, ready: impl Fn(&Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready(self) {
            assert!(
                Instant::now() < deadline,
                "Library worker did not finish: {}",
                self.studio.status
            );
            self.frame(vec![]);
            std::thread::yield_now();
        }
        self.frame(vec![]);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        jobs::cancel::<Snapshot>(&self.ctx, SYNC);
        jobs::cancel::<PreviewBatch>(&self.ctx, PREVIEW);
        jobs::cancel::<Placed>(&self.ctx, PLACE);
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn keyboard_library_palette_validates_native_drafts_retains_owner_and_saves_reopens() {
    let mut f = Fixture::new();
    let artwork = crate::project::encode(&f.studio.doc).unwrap();
    let history = f.studio.history.len();
    f.press("Palettes");
    f.press("Project palette collection");
    f.press("Create palette");
    let output = f.text("Palette name", "");
    keys::associated_error(&output, "Palette name");
    assert!(keys::named(&output, "Rename palette").1.is_disabled());
    let id = keys::named(&output, "Palette name").0;
    let output = f.text("Palette name", "Warm earth");
    assert_eq!(keys::named(&output, "Palette name").0, id);
    keys::focused(&f.ctx, &output, "Palette name");
    f.press("Rename palette");
    let output = f.text("Hex color to add to palette", "#zz");
    keys::associated_error(&output, "Hex color to add to palette");
    assert!(keys::named(&output, "Add hex color").1.is_disabled());
    f.text("Hex color to add to palette", "#D97C5B");
    f.press("Add hex color");
    f.text("Hex color to add to palette", "#3377CC");
    f.press("Add hex color");
    let output = f.frame(vec![]);
    let name = keys::named(&output, "Palette name").0;
    let survivor = keys::named(&output, "Apply #3377CC to fill").0;
    let removed = keys::named(&output, "Apply #D97C5B to fill").0;
    f.press("Actions for #D97C5B swatch");
    f.press("Remove swatch");
    let output = f.frame(vec![]);
    assert_eq!(keys::named(&output, "Palette name").0, name);
    assert_eq!(
        keys::named(&output, "Apply #3377CC to fill").0,
        survivor,
        "The surviving native swatch must retain its identity after the preceding swatch is removed"
    );
    assert_ne!(survivor, removed);
    assert_eq!(
        f.studio.libraries.draft().palettes[0],
        Palette::new("Warm earth", vec![Rgba::from_hex(0x3377CC)])
    );
    f.press("Save palette collection");
    f.until(|f| !f.studio.libraries.projects[&f.root].dirty);
    let saved = crate::palette::load_file(f.root.join(".omacolors")).unwrap();
    assert_eq!(saved, f.studio.libraries.projects[&f.root].palettes);
    assert_eq!(crate::project::encode(&f.studio.doc).unwrap(), artwork);
    assert_eq!(
        f.studio.history.len(),
        history,
        "Library sidecars must not enter artwork Undo"
    );
    let mut reopened = Libraries::default();
    reopened.root = Some(f.root.clone());
    reopened.project_scope = true;
    let (palettes, stamp) = read_colours(&f.root.join(".omacolors"), false).unwrap();
    reopened.draft().receive(palettes, stamp);
    assert_eq!(reopened.draft().palettes, saved);
}

#[test]
fn keyboard_library_brand_places_from_empty_selection_colors_undoes_and_native_reopens() {
    let mut f = Fixture::new();
    let bank = crate::brand::create(&f.root, "Keyboard brand").unwrap();
    std::fs::write(bank.root.join("mark.svg"), r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="30"><path d="M0 0H40V30H0Z" fill="#112233"/></svg>"##).unwrap();
    f.studio.libraries.next_sync = Instant::now();
    f.until(|f| {
        f.studio
            .libraries
            .catalog
            .as_ref()
            .is_some_and(|catalog| catalog.assets.len() == 1)
    });
    let before = crate::project::encode(&f.studio.doc).unwrap();
    let history = f.studio.history.len();
    assert!(f.studio.selection.is_empty());
    f.press("Brand assets");
    let output = f.press("Place brand asset mark");
    f.until(|f| !f.studio.selection.is_empty());
    let target = f.studio.primary().unwrap();
    assert!(f.studio.doc.find_shape(target.0, target.1).is_some());
    assert_eq!(f.studio.history.len(), history + 1);
    let placed = crate::project::encode(&f.studio.doc).unwrap();
    assert_ne!(placed, before);
    assert!(
        keys::named(&output, "Place brand asset mark")
            .1
            .supports_action(egui::accesskit::Action::Click)
    );
    f.press("Palettes");
    f.press("Project palette collection");
    f.press("Create palette");
    f.text("Hex color to add to palette", "#D97C5B");
    f.press("Add hex color");
    f.press("Apply #D97C5B to fill");
    assert_eq!(f.studio.primary(), Some(target));
    assert_eq!(
        f.studio
            .doc
            .find_shape(target.0, target.1)
            .unwrap()
            .style
            .fill,
        Fill::Solid(Rgba::from_hex(0xD97C5B))
    );
    let colored = crate::project::encode(&f.studio.doc).unwrap();
    f.frame(keys::key(Key::F8, Modifiers::NONE));
    f.frame(keys::key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND));
    assert_eq!(crate::project::encode(&f.studio.doc).unwrap(), placed);
    f.frame(keys::key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND));
    assert_eq!(crate::project::encode(&f.studio.doc).unwrap(), before);
    for _ in 0..2 {
        f.frame(keys::key(
            Key::Z,
            Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
        ));
    }
    assert_eq!(crate::project::encode(&f.studio.doc).unwrap(), colored);
    let path = f.root.join("authored.oma");
    crate::project::save_to(&f.studio.doc, &path).unwrap();
    assert_eq!(
        crate::project::encode(&crate::project::load_from(&path).unwrap()).unwrap(),
        colored
    );
    f.frame(keys::key(Key::F8, Modifiers::NONE));
    f.frame(keys::key(Key::A, Modifiers::CTRL | Modifiers::COMMAND));
    assert!(f.studio.selection.contains(&target));
    f.studio.doc.layers[target.0].locked = true;
    let locked = crate::project::encode(&f.studio.doc).unwrap();
    let output = f.frame(vec![]);
    assert!(
        keys::named(&output, "Apply #D97C5B to fill")
            .1
            .is_disabled()
    );
    f.frame(keys::action(
        keys::named(&output, "Apply #D97C5B to fill").0,
        egui::accesskit::Action::Click,
        None,
    ));
    assert_eq!(crate::project::encode(&f.studio.doc).unwrap(), locked);
}

#[test]
fn keyboard_library_typography_uses_native_names_validation_and_real_project_save() {
    let mut f = Fixture::new();
    let definitions = egui::FontDefinitions::default();
    let font = definitions
        .font_data
        .values()
        .find(|font| font.font.starts_with(&[0, 1, 0, 0]))
        .expect("Bundled egui TrueType fixture");
    let source = f.root.join("fixture.ttf");
    std::fs::write(&source, font.font.as_ref()).unwrap();
    let loaded = crate::typography::add_fonts(&f.root, &[source], None).unwrap();
    let native_font = loaded.fonts[0].clone();
    f.studio.libraries.next_sync = Instant::now();
    f.until(|f| f.studio.libraries.typography.is_some());
    let before = crate::project::encode(&f.studio.doc).unwrap();
    let history = f.studio.history.len();
    f.press("Brand assets");
    let output = f.text("Typography kit name", "");
    keys::associated_error(&output, "Typography kit name");
    assert!(
        keys::named(&output, "Save typography kit name")
            .1
            .is_disabled()
    );
    f.text("Typography kit name", "Editorial type");
    f.press("Save typography kit name");
    f.until(|f| {
        !typography::busy(&f.ctx)
            && f.studio
                .libraries
                .typography
                .as_ref()
                .is_some_and(|loaded| loaded.kit.name == "Editorial type")
    });
    f.text("Typography role name", "Reading");
    f.press("Save typography role name");
    f.until(|f| {
        !typography::busy(&f.ctx)
            && f.studio
                .libraries
                .typography
                .as_ref()
                .is_some_and(|loaded| loaded.kit.roles[0].name == "Reading")
    });
    f.press(&format!(
        "Apply typography role Reading: {}",
        native_font.family
    ));
    assert_eq!(f.studio.text_font, native_font.id);
    assert!(f.studio.selection.is_empty());
    let reopened = crate::typography::load(&f.root).unwrap().unwrap();
    assert_eq!(reopened.kit.name, "Editorial type");
    assert_eq!(reopened.kit.roles[0].name, "Reading");
    assert_eq!(reopened.fonts[0].bytes, native_font.bytes);
    assert_eq!(crate::project::encode(&f.studio.doc).unwrap(), before);
    assert_eq!(f.studio.history.len(), history);
}
