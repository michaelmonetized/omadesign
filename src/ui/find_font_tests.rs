use super::*;
use crate::{
    document::{Cmd, Document, Fill, Layer, Shape, Style},
    geom::{Geom, Pt, TypeRun},
    tools::Persona,
    ui::welcome::keyboard_tests as keys,
};
use egui::{Event, Key, Modifiers, PointerButton, Pos2};

fn editor() -> Studio {
    let mut s = Studio::new();
    s.show_welcome = false;
    s.persona = Persona::Layout;
    s.doc = Document::new("Native Find Font", 500., 220., 96.);
    s.doc.workspace = Some(Persona::Layout);
    s.doc.layers = vec![Layer::vector("Editable stories")];
    for (i, text) in ["Cat cat catalog", "the CAT"].into_iter().enumerate() {
        let mut run = TypeRun {
            content: text.into(),
            origin: Pt::new(35., 75. + i as f32 * 70.),
            px: 30.,
            font: concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/assets/fonts/EBGaramond.ttf"
            )
            .into(),
            ..Default::default()
        };
        run.set_character_style(0, 3, |span| {
            span.fill = Some(Fill::Solid(crate::color::Rgba::from_hex(0xd6224a)))
        });
        let mut shape = Shape::new(Geom::Text(run), Style::default());
        shape.name = format!("Story {}", i + 1);
        crate::text::fill_contours(&mut shape.geom);
        s.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
    }
    let id = s.doc.layers[0].kind.shapes().unwrap()[0].id;
    s.selection = vec![(0, id)];
    s.active_layer = Some(0);
    s.mark();
    s.history.clear();
    s
}
fn wire(s: &Studio) -> String {
    crate::project::encode(&s.doc).unwrap()
}
fn texts(s: &Studio) -> Vec<String> {
    s.doc
        .layers
        .iter()
        .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
        .filter_map(|s| {
            if let Geom::Text(r) = &s.geom {
                Some(r.content.clone())
            } else {
                None
            }
        })
        .collect()
}
fn emit(name: &str, s: &Studio) {
    if let Some(root) = std::env::var_os("OMADESIGN_FIND_FONT_QA") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(std::path::Path::new(&root).join(name), wire(s)).unwrap();
    }
}
struct Seat {
    ctx: egui::Context,
    time: f64,
    out: egui::FullOutput,
    size: egui::Vec2,
}
impl Seat {
    fn new() -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::ui::theme::apply(&ctx);
        ctx.all_styles_mut(|style| style.animation_time = 0.);
        Self {
            ctx,
            time: 0.,
            out: Default::default(),
            size: egui::vec2(1600., 1000.),
        }
    }
    fn frame(&mut self, s: &mut Studio, mut events: Vec<Event>) -> egui::FullOutput {
        self.time += 0.2;
        let modifiers = events
            .iter()
            .find_map(|e| {
                if let Event::Key { modifiers, .. } = e {
                    Some(*modifiers)
                } else {
                    None
                }
            })
            .unwrap_or_default();
        events.insert(0, Event::ModifiersChanged(modifiers));
        let mut out = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, self.size)),
                time: Some(self.time),
                events,
                ..Default::default()
            },
            |ui| crate::ui::run(ui, s),
        );
        out.textures_delta.clear();
        self.out = out.clone();
        out
    }
    fn click(&mut self, s: &mut Studio, name: &str) {
        for _ in 0..3 {
            self.frame(s, vec![]);
        }
        let b = keys::named(&self.out, name).1.bounds().unwrap();
        let pos = egui::pos2(((b.x0 + b.x1) * 0.5) as f32, ((b.y0 + b.y1) * 0.5) as f32);
        self.frame(s, vec![Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            self.frame(
                s,
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        self.frame(s, vec![]);
    }
    fn invoke(&mut self, s: &mut Studio, name: &str) {
        let ctx = self.ctx.clone();
        keys::tab_to(&ctx, name, |e| self.frame(s, e));
        self.frame(s, keys::key(Key::Space, Modifiers::NONE));
        self.frame(s, vec![]);
    }
    fn type_field(&mut self, s: &mut Studio, name: &str, text: &str) {
        let ctx = self.ctx.clone();
        keys::tab_to(&ctx, name, |e| self.frame(s, e));
        self.frame(s, keys::key(Key::A, Modifiers::CTRL | Modifiers::COMMAND));
        self.frame(s, vec![Event::Paste(text.into())]);
        self.frame(s, vec![]);
        keys::focused(&self.ctx, &self.out, name);
    }
    fn open(&mut self, s: &mut Studio) {
        self.frame(s, vec![]);
        self.click(s, "Type");
        self.invoke(s, "Find Font…");
        assert!(is_open(&self.ctx));
        self.frame(s, vec![]);
        assert!(self.ctx.memory(|m| m.focused()).is_some());
    }
    fn canvas_focus(&mut self, s: &mut Studio) {
        for _ in 0..3 {
            self.frame(s, vec![]);
        }
        let response = self.ctx.read_response(Id::new("studio-canvas")).unwrap();
        assert_eq!(self.ctx.memory(|m| m.focused()), Some(response.id));
        assert_eq!(
            self.out
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .focus,
            response.id.accesskit_id()
        );
    }
}

#[test]
fn craft_find_font_native_menu_filter_style_selected_change_undo_and_reopen() {
    let mut s = editor();
    let mut seat = Seat::new();
    let before = wire(&s);
    seat.open(&mut s);
    assert_eq!(wire(&s), before);
    assert_eq!(s.history.len(), 0);
    seat.invoke(&mut s, "Refresh");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while super::super::jobs::frame_progress::<usize>(&seat.ctx, "type-find-font-refresh").is_some()
    {
        assert!(
            std::time::Instant::now() < deadline,
            "Installed font refresh did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
        seat.frame(&mut s, vec![]);
    }
    seat.frame(&mut s, vec![]);
    assert_eq!(wire(&s), before);
    assert_eq!(s.history.len(), 0);
    let catalog = s.font_catalog();
    let family = catalog
        .families()
        .into_iter()
        .find(|f| f != "EB Garamond" && !catalog.styles(f).is_empty())
        .unwrap();
    seat.type_field(&mut s, "Filter font families", &family);
    let ctx = seat.ctx.clone();
    keys::tab_to(&ctx, "Font family", |e| seat.frame(&mut s, e));
    let options: Vec<_> = catalog
        .families()
        .into_iter()
        .filter(|f| f == &family || f == "EB Garamond")
        .collect();
    let key = if options.first() == Some(&family) {
        Key::Home
    } else {
        Key::End
    };
    seat.frame(&mut s, keys::key(key, Modifiers::NONE));
    seat.frame(&mut s, vec![]);
    keys::focused(&seat.ctx, &seat.out, "Font family");
    let chosen = seat.ctx.data(|d| d.get_temp::<Draft>(Id::new(ID))).unwrap();
    assert_eq!(chosen.family, family);
    seat.invoke(&mut s, "Change");
    assert_eq!(s.history.len(), 1);
    assert_ne!(wire(&s), before);
    let shapes = s.doc.layers[0].kind.shapes().unwrap();
    let Geom::Text(first) = &shapes[0].geom else {
        panic!()
    };
    let Geom::Text(second) = &shapes[1].geom else {
        panic!()
    };
    assert!(first.font.starts_with("omatype:"));
    assert!(second.font.ends_with("EBGaramond.ttf"));
    let after = wire(&s);
    let doc = crate::project::decode(&after).unwrap();
    assert_eq!(crate::project::encode(&doc).unwrap(), after);
    assert_eq!(
        crate::compositor::export_png(&doc, 1).unwrap(),
        crate::compositor::export_png(&s.doc, 1).unwrap()
    );
    emit("dialog-after.oma", &s);
    seat.invoke(&mut s, "Done");
    assert!(!is_open(&seat.ctx));
    seat.canvas_focus(&mut s);
    s.undo();
    assert_eq!(wire(&s), before);
    s.redo();
    assert_eq!(wire(&s), after);
}
#[test]
fn craft_find_font_native_find_all_cancel_stale_and_narrow_modal_keep_document_unchanged() {
    let mut s = editor();
    let mut seat = Seat::new();
    let before = wire(&s);
    seat.open(&mut s);
    seat.invoke(&mut s, "Find");
    assert_eq!(s.selection.len(), 2);
    assert_eq!(wire(&s), before);
    assert_eq!(s.history.len(), 0);
    seat.invoke(&mut s, "Done");
    seat.canvas_focus(&mut s);
    seat.open(&mut s);
    seat.size = egui::vec2(800., 600.);
    seat.frame(&mut s, vec![]);
    for name in [
        "Font family",
        "Font style",
        "Find",
        "Change",
        "Change All",
        "Done",
    ] {
        let (_, node) = keys::named(&seat.out, name);
        let b = node.bounds().unwrap();
        assert!(
            b.x0 >= 0. && b.y0 >= 0. && b.x1 <= 800. && b.y1 <= 600.,
            "{name}: {b:?}"
        );
    }
    s.canvas_gen = s.canvas_gen.wrapping_add(1);
    seat.frame(&mut s, vec![]);
    assert_eq!(wire(&s), before);
    assert_eq!(s.history.len(), 0);
    seat.invoke(&mut s, "Done");
    assert!(!is_open(&seat.ctx));
    seat.canvas_focus(&mut s);
}

fn recovered_fixture() -> Vec<u8> {
    let mut bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/assets/fonts/EBGaramond.ttf"
    ))
    .unwrap();
    let face = rustybuzz::ttf_parser::Face::parse(&bytes, 0).unwrap();
    let names: Vec<_> = face
        .names()
        .into_iter()
        .filter(|n| matches!(n.name_id, 1 | 16))
        .map(|n| {
            (
                (n.name.as_ptr() as usize) - bytes.as_ptr() as usize,
                n.name.len(),
                n.is_unicode(),
            )
        })
        .collect();
    for (at, len, unicode) in names {
        let text = "Oma Font QA";
        let name = if unicode {
            text.encode_utf16()
                .flat_map(u16::to_be_bytes)
                .collect::<Vec<_>>()
        } else {
            text.as_bytes().to_vec()
        };
        assert_eq!(len, name.len());
        bytes[at..at + len].copy_from_slice(&name);
    }
    let count = u16::from_be_bytes(bytes[4..6].try_into().unwrap()) as usize;
    let sum = |data: &[u8]| {
        data.chunks(4).fold(0u32, |n, b| {
            let mut word = [0; 4];
            word[..b.len()].copy_from_slice(b);
            n.wrapping_add(u32::from_be_bytes(word))
        })
    };
    let mut head = 0;
    for i in 0..count {
        let row = 12 + i * 16;
        let start = u32::from_be_bytes(bytes[row + 8..row + 12].try_into().unwrap()) as usize;
        let len = u32::from_be_bytes(bytes[row + 12..row + 16].try_into().unwrap()) as usize;
        if &bytes[row..row + 4] == b"head" {
            head = start;
            bytes[start + 8..start + 12].fill(0);
        }
        let checksum = sum(&bytes[start..start + len]);
        bytes[row + 4..row + 8].copy_from_slice(&checksum.to_be_bytes());
    }
    let adjustment = 0xb1b0afbau32.wrapping_sub(sum(&bytes));
    bytes[head + 8..head + 12].copy_from_slice(&adjustment.to_be_bytes());
    assert!(ab_glyph::FontRef::try_from_slice(&bytes).is_ok());
    bytes
}
#[test]
fn craft_find_font_native_missing_folder_paste_search_explicit_add_redraw_no_history_and_cold_reopen()
 {
    use sha2::{Digest, Sha256};
    let bytes = recovered_fixture();
    let folder = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".artifacts/font-recovery-tests")
        .join(format!("native-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("OmaFindFontQA.ttf");
    std::fs::write(&path, &bytes).unwrap();
    let destination = crate::project::data_dir().join("fonts/OmaFindFontQA.ttf");
    assert!(!destination.exists());
    let source = r#"<svg xmlns="http://www.w3.org/2000/svg" width="300" height="150"><text x="25" y="70" font-size="36" font-family="Oma Font QA">Font AV fi</text></svg>"#;
    let (doc, _) = crate::formats::svg::read(source, "Missing native font").unwrap();
    let mut s = editor();
    s.doc = doc;
    s.selection = s
        .doc
        .layers
        .iter()
        .enumerate()
        .flat_map(|(l, layer)| {
            layer
                .kind
                .shapes()
                .unwrap_or(&[])
                .iter()
                .filter_map(move |shape| {
                    matches!(shape.geom, Geom::Text(_)).then_some((l, shape.id))
                })
        })
        .collect();
    s.active_layer = Some(s.selection[0].0);
    s.mark();
    s.history.clear();
    let before = wire(&s);
    let before_png = crate::compositor::export_png(&s.doc, 1).unwrap();
    assert!(
        s.font_usage()
            .unwrap()
            .iter()
            .any(|r| r.info.family == "Oma Font QA" && r.info.status == Status::Missing),
        "actual requests: {:?}; document: {}",
        s.font_usage().unwrap(),
        before
    );
    let mut seat = Seat::new();
    seat.open(&mut s);
    seat.type_field(&mut s, "Font search folder", folder.to_str().unwrap());
    seat.invoke(&mut s, "Search font files");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while super::super::jobs::frame_progress::<crate::text::font_recovery::Search>(
        &seat.ctx,
        "type-find-font-search",
    )
    .is_some()
    {
        assert!(
            std::time::Instant::now() < deadline,
            "Actual font search did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
        seat.frame(&mut s, vec![]);
    }
    assert_eq!(wire(&s), before);
    assert_eq!(s.history.len(), 0);
    assert!(!destination.exists());
    seat.invoke(&mut s, "Add selected font files");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while super::super::jobs::frame_progress::<Vec<std::path::PathBuf>>(
        &seat.ctx,
        "type-find-font-add",
    )
    .is_some()
    {
        assert!(
            std::time::Instant::now() < deadline,
            "Actual font addition did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
        seat.frame(&mut s, vec![]);
    }
    seat.frame(&mut s, vec![]);
    assert_eq!(std::fs::read(&destination).unwrap(), bytes);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(wire(&s), before);
    assert_eq!(s.history.len(), 0);
    assert!(
        s.font_usage()
            .unwrap()
            .iter()
            .any(|r| r.info.family == "Oma Font QA" && r.info.status == Status::Exact)
    );
    let after_png = crate::compositor::export_png(&s.doc, 1).unwrap();
    assert_ne!(after_png, before_png);
    let root = std::env::var_os("OMADESIGN_FIND_FONT_QA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| folder.join("qa"));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("recovered.oma"), &before).unwrap();
    std::fs::write(root.join("recovered.png"), &after_png).unwrap();
    std::fs::write(root.join("unresolved.png"), &before_png).unwrap();
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "ui::find_font::tests::craft_find_font_recovery_cold_child",
            "--ignored",
            "--nocapture",
        ])
        .env("OMADESIGN_FIND_FONT_COLD", &root)
        .output()
        .unwrap();
    std::fs::write(
        root.join("cold-font.log"),
        [&child.stdout[..], &child.stderr[..]].concat(),
    )
    .unwrap();
    assert!(
        child.status.success(),
        "Cold font loader refused: {}",
        String::from_utf8_lossy(&child.stderr)
    );
    std::fs::write(root.join("font-input.json"), serde_json::to_vec_pretty(&serde_json::json!({"source":path,"installed":destination,"sha256":format!("{:x}",Sha256::digest(&bytes)),"bytes":bytes.len(),"fixture":"OFL EB Garamond outlines, isolated test family name, native SFNT checksums"})).unwrap()).unwrap();
    seat.invoke(&mut s, "Done");
    seat.canvas_focus(&mut s);
    assert_eq!(wire(&s), before);
    std::fs::remove_file(destination).unwrap();
    std::fs::remove_file(path).unwrap();
}
#[test]
#[ignore = "spawned by actual recovery dialog test with its private installed-font directory"]
fn craft_find_font_recovery_cold_child() {
    let root = std::path::PathBuf::from(
        std::env::var_os("OMADESIGN_FIND_FONT_COLD").expect("Outer recovery test fixture"),
    );
    let doc = crate::project::decode(&std::fs::read_to_string(root.join("recovered.oma")).unwrap())
        .unwrap();
    assert_eq!(
        crate::compositor::export_png(&doc, 1).unwrap(),
        std::fs::read(root.join("recovered.png")).unwrap()
    );
}
