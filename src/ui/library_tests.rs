//! Real egui frames and background jobs guard late-result ownership.
use super::*;
use crate::document::{Artboard, Document, Layer, Shape, Style};
use crate::geom::Geom;
use eframe::egui::{Event, Id, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, pos2};
use std::sync::mpsc;

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "omadesign-library-jobs-{}-{}",
            std::process::id(),
            crate::document::next_id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixture(temp: &Temp) -> (egui::Context, Studio) {
    let ctx = egui::Context::default();
    theme::apply(&ctx);
    let mut studio = Studio::new();
    studio.doc = Document::new("Job destination", 1.0, 1.0, 72.0);
    studio.doc.width = 320.0;
    studio.doc.height = 240.0;
    studio.doc.artboards = vec![Artboard::new(0, Pt::ZERO, Pt::new(320.0, 240.0))];
    studio.doc.layers = vec![Layer::vector("Artwork")];
    studio.doc.layers[0]
        .kind
        .shapes_mut()
        .unwrap()
        .push(Shape::new(
            Geom::Rect {
                origin: Pt::new(10.0, 10.0),
                size: Pt::new(20.0, 20.0),
                radius: 0.0,
            },
            Style::default(),
        ));
    studio.path = Some(temp.0.join("artwork.oma"));
    studio.show_welcome = false;
    studio.need_fit = false;
    studio.active_layer = Some(0);
    studio.libraries.project_scope = true;
    studio.libraries.root = Some(temp.0.clone());
    quiet_source(&mut studio);
    (ctx, studio)
}

fn quiet_source(studio: &mut Studio) {
    // No environment changes and no writes to personal libraries. Unrelated
    // periodic scans stay asleep; explicit completion-triggered scans are real.
    studio.libraries.source = Some(Source {
        document: studio.swap_id.clone(),
        path: studio.path.clone(),
        folder: studio.libraries.folders.get(&studio.swap_id).cloned(),
    });
    studio.libraries.next_sync = Instant::now() + Duration::from_secs(3600);
}

fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) {
    let mut output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(960.0, 640.0))),
            events,
            ..Default::default()
        },
        |ui| tick(ui.ctx(), studio),
    );
    output.textures_delta.clear();
}

fn until(ctx: &egui::Context, studio: &mut Studio, done: impl Fn(&Studio) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done(studio) {
        assert!(
            Instant::now() < deadline,
            "background result did not arrive"
        );
        frame(ctx, studio, vec![]);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn blocked<T: Send + 'static>(
    ctx: &egui::Context,
    id: &'static str,
    result: T,
) -> (mpsc::Sender<()>, mpsc::Receiver<()>) {
    let (release, wait) = mpsc::channel();
    let (returned, done) = mpsc::channel();
    jobs::start(ctx, id, move || {
        wait.recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        let _ = returned.send(());
        Ok(result)
    });
    (release, done)
}

fn late_frames(ctx: &egui::Context, studio: &mut Studio) {
    for _ in 0..8 {
        std::thread::sleep(Duration::from_millis(1));
        frame(ctx, studio, vec![]);
    }
}

#[test]
fn successful_save_retires_an_older_snapshot_instead_of_reverting_colours() {
    let temp = Temp::new();
    let (ctx, mut studio) = fixture(&temp);
    let path = temp.0.join(".omacolors");
    let old = vec![Palette::new("Yesterday", vec![Rgba::BLACK])];
    let saved = vec![Palette::new("Today", vec![Rgba::from_hex(0xD97C5B)])];
    crate::palette::save_file(&path, &old).unwrap();
    let old_stamp = file_stamp(&path).unwrap();
    studio.libraries.draft().receive(old.clone(), old_stamp);
    studio.libraries.draft().palettes = saved.clone();
    studio.libraries.draft().dirty = true;
    let stale = Snapshot {
        source: studio.libraries.source.clone().unwrap(),
        root: Some(temp.0.clone()),
        personal: Ok((vec![], None)),
        project: Some(Ok((old, old_stamp))),
        catalog: Ok(None),
        typography: Ok(None),
    };
    let (release_stale, stale_done) = blocked(&ctx, SYNC, stale);
    let work_path = path.clone();
    let work_saved = saved.clone();
    start_palette(&ctx, Some(temp.0.clone()), move || {
        crate::palette::save_file(&work_path, &work_saved)?;
        Ok(PaletteChange::Saved(work_saved, file_stamp(&work_path)?))
    });
    until(&ctx, &mut studio, |s| {
        s.libraries.projects.get(&temp.0).is_some_and(|d| !d.dirty)
    });
    release_stale.send(()).unwrap();
    stale_done.recv_timeout(Duration::from_secs(5)).unwrap();
    late_frames(&ctx, &mut studio);
    let draft = studio.libraries.projects.get(&temp.0).unwrap();
    assert_eq!(draft.palettes, saved);
    assert_eq!(draft.stamp, file_stamp(&path).unwrap());
    assert!(!draft.dirty && !draft.conflict);
    assert_eq!(crate::palette::load_file(&path).unwrap(), saved);
    assert_eq!(
        studio.history.len(),
        0,
        "sidecar save entered artwork undo history"
    );
    jobs::cancel::<Snapshot>(&ctx, SYNC);
}

#[test]
fn completing_a_bank_job_cannot_link_the_new_unsaved_tab() {
    let temp = Temp::new();
    let (ctx, mut studio) = fixture(&temp);
    studio.ensure_tabs();
    let original_document = studio.swap_id.clone();
    let (release, done) = blocked(
        &ctx,
        BRAND_ACTION,
        BrandResult {
            root: temp.0.clone(),
            message: "Bank created for the original document".into(),
        },
    );
    studio.new_tab();
    assert_ne!(studio.swap_id, original_document);
    assert!(studio.path.is_none());
    studio.libraries.root = None;
    studio.libraries.catalog = None;
    quiet_source(&mut studio);
    let new_document = studio.swap_id.clone();
    release.send(()).unwrap();
    done.recv_timeout(Duration::from_secs(5)).unwrap();
    until(&ctx, &mut studio, |_| {
        !jobs::is_running::<BrandResult>(&ctx, BRAND_ACTION)
    });
    late_frames(&ctx, &mut studio);
    assert!(!studio.libraries.folders.contains_key(&new_document));
    assert!(studio.libraries.root.is_none());
    assert!(studio.path.is_none());
    assert_eq!(studio.tab_count(), 2);
    assert!(!studio.libraries.brand_message.contains("Bank created"));
    jobs::cancel::<Snapshot>(&ctx, SYNC);
}

#[test]
fn escape_cancels_loading_placement_even_when_the_worker_finishes_later() {
    let temp = Temp::new();
    let (ctx, mut studio) = fixture(&temp);
    let before = serde_json::to_value(&studio.doc).unwrap();
    let (release, done) = blocked(
        &ctx,
        PLACE,
        Placed {
            document: studio.swap_id.clone(),
            at: Pt::new(90.0, 80.0),
            imported: crate::import::Imported::Raster {
                name: "Late image".into(),
                image: crate::photo::RgbaImage::new(1, 1, vec![255, 80, 40, 255]).unwrap(),
            },
        },
    );
    let mut output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(960.0, 640.0))),
            events: vec![Event::Key {
                key: Key::Escape,
                physical_key: Some(Key::Escape),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            ..Default::default()
        },
        |ui| crate::ui::run(ui, &mut studio),
    );
    output.textures_delta.clear();
    assert!(!jobs::is_running::<Placed>(&ctx, PLACE));
    release.send(()).unwrap();
    done.recv_timeout(Duration::from_secs(5)).unwrap();
    late_frames(&ctx, &mut studio);
    assert_eq!(serde_json::to_value(&studio.doc).unwrap(), before);
    assert_eq!(studio.history.len(), 0);
    assert!(studio.status.to_lowercase().contains("cancel"));
}

#[test]
fn native_tile_drag_places_at_the_view_transformed_pointer_and_undo_removes_it() {
    let temp = Temp::new();
    let (ctx, mut studio) = fixture(&temp);
    let file = temp.0.join("brand-mark.png");
    image::RgbaImage::from_pixel(4, 2, image::Rgba([220, 105, 70, 255]))
        .save(&file)
        .unwrap();
    studio.view.scale = 2.0;
    studio.view.offset = Pt::new(30.0, 40.0);
    // The cached canvas cursor is deliberately wrong: an external widget's drag
    // suppresses hover_pos, so release must use its actual pointer position.
    studio.cursor = Some(Pt::new(1.0, 1.0));
    let before = serde_json::to_value(&studio.doc).unwrap();
    let canvas = Rect::from_min_size(pos2(100.0, 20.0), vec2(420.0, 300.0));
    let start = pos2(35.0, 40.0);
    let drop = pos2(320.0, 160.0);
    let mouse = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    for events in [
        vec![],
        vec![],
        vec![Event::PointerMoved(start)],
        vec![mouse(start, true)],
        vec![Event::PointerMoved(drop)],
        vec![mouse(drop, false)],
    ] {
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(960.0, 640.0))),
                events,
                ..Default::default()
            },
            |ui| {
                tick(ui.ctx(), &mut studio);
                let tile = ui.interact(
                    Rect::from_min_size(pos2(10.0, 10.0), vec2(60.0, 70.0)),
                    Id::new("brand-drag-test-tile"),
                    Sense::click_and_drag(),
                );
                tile.dnd_set_drag_payload(BrandDrag { path: file.clone() });
                let response = ui.interact(
                    canvas,
                    Id::new("brand-drag-test-canvas"),
                    Sense::click_and_drag(),
                );
                canvas_drop(ui, &mut studio, &response);
            },
        );
        output.textures_delta.clear();
    }
    until(&ctx, &mut studio, |s| s.doc.layers.len() == 2);
    let center = studio.doc.layers[1].kind.raster_bounds().unwrap().center();
    assert_eq!(center, Pt::new(95.0, 50.0));
    assert_eq!(studio.history.len(), 1);
    assert_eq!(studio.tab_count(), 1);
    studio.undo();
    assert_eq!(serde_json::to_value(&studio.doc).unwrap(), before);
}

struct PaintedItem {
    label: String,
    rect: Rect,
    clip: Rect,
    fill: Option<Color32>,
}

fn full_frame(
    ctx: &egui::Context,
    studio: &mut Studio,
    events: Vec<Event>,
    height: f32,
) -> Vec<PaintedItem> {
    // Exercise the real app without allowing its idle recovery timer to write.
    studio.last_input = Instant::now();
    let mut output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(960.0, height))),
            events,
            ..Default::default()
        },
        |ui| crate::ui::run(ui, studio),
    );
    fn collect(shape: &egui::epaint::Shape, clip: Rect, labels: &mut Vec<PaintedItem>) {
        match shape {
            egui::epaint::Shape::Text(text) => labels.push(PaintedItem {
                label: text.galley.text().to_owned(),
                rect: text.galley.rect.translate(text.pos.to_vec2()),
                clip,
                fill: None,
            }),
            egui::epaint::Shape::Rect(rect) => labels.push(PaintedItem {
                label: String::new(),
                rect: rect.rect,
                clip,
                fill: Some(rect.fill),
            }),
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, clip, labels);
                }
            }
            _ => {}
        }
    }
    let mut labels = Vec::new();
    for shape in &output.shapes {
        collect(&shape.shape, shape.clip_rect, &mut labels);
    }
    output.textures_delta.clear();
    labels
}

fn label_rect(labels: &[PaintedItem], label: &str) -> Rect {
    let item = labels
        .iter()
        .find(|text| text.label == label && text.rect.intersects(text.clip))
        .unwrap_or_else(|| {
            panic!(
                "missing visible label {label:?}: {:?}",
                labels.iter().map(|t| &t.label).collect::<Vec<_>>()
            )
        });
    item.rect.intersect(item.clip)
}

fn swatch_rect(items: &[PaintedItem], color: Rgba) -> Rect {
    items
        .iter()
        .find(|item| item.fill == Some(color.to_egui()) && item.clip.contains(item.rect.center()))
        .expect("the palette swatch must be visible")
        .rect
}

fn click_at(
    ctx: &egui::Context,
    studio: &mut Studio,
    at: Pos2,
    button: PointerButton,
    height: f32,
) -> Vec<PaintedItem> {
    full_frame(ctx, studio, vec![Event::PointerMoved(at)], height);
    full_frame(
        ctx,
        studio,
        vec![Event::PointerButton {
            pos: at,
            button,
            pressed: true,
            modifiers: Modifiers::NONE,
        }],
        height,
    );
    full_frame(
        ctx,
        studio,
        vec![Event::PointerButton {
            pos: at,
            button,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
        height,
    )
}

fn click_label(ctx: &egui::Context, studio: &mut Studio, label: &str) -> Vec<PaintedItem> {
    let labels = full_frame(ctx, studio, vec![], 900.0);
    click_at(
        ctx,
        studio,
        label_rect(&labels, label).center(),
        PointerButton::Primary,
        900.0,
    )
}

fn replace_field(ctx: &egui::Context, studio: &mut Studio, value: &str) -> Vec<PaintedItem> {
    let control = Modifiers::CTRL | Modifiers::COMMAND;
    full_frame(
        ctx,
        studio,
        vec![
            Event::ModifiersChanged(control),
            Event::Key {
                key: Key::A,
                physical_key: Some(Key::A),
                pressed: true,
                repeat: false,
                modifiers: control,
            },
        ],
        900.0,
    );
    let mut events = vec![
        Event::ModifiersChanged(Modifiers::NONE),
        Event::Key {
            key: Key::A,
            physical_key: Some(Key::A),
            pressed: false,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
    ];
    if value.is_empty() {
        events.push(Event::Key {
            key: Key::Backspace,
            physical_key: Some(Key::Backspace),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
    } else {
        events.push(Event::Text(value.into()));
    }
    full_frame(ctx, studio, events, 900.0);
    full_frame(ctx, studio, vec![], 900.0)
}

#[test]
fn palette_sidebar_creates_edits_applies_and_saves_transparent_colours() {
    let temp = Temp::new();
    let (ctx, mut studio) = fixture(&temp);
    studio.libraries.sidebar = Sidebar::Palettes;
    studio.libraries.draft().receive(vec![], None);
    let selected = studio.doc.layers[0].kind.shapes().unwrap()[0].id;
    studio.selection = vec![(0, selected)];
    for _ in 0..3 {
        full_frame(&ctx, &mut studio, vec![], 900.0);
    }
    click_label(&ctx, &mut studio, "+ Palette");
    click_label(&ctx, &mut studio, "Palette 1");
    replace_field(&ctx, &mut studio, "Studio North");
    click_label(&ctx, &mut studio, "Rename");
    click_label(&ctx, &mut studio, "#D97C5B or #D97C5B80");
    let labels = replace_field(&ctx, &mut studio, "#D97C5B80");
    let hex = label_rect(&labels, "#D97C5B80");
    let plus = labels
        .iter()
        .filter(|text| text.label == "+" && text.rect.intersects(text.clip))
        .min_by(|a, b| {
            (a.rect.center().y - hex.center().y)
                .abs()
                .total_cmp(&(b.rect.center().y - hex.center().y).abs())
        })
        .unwrap()
        .rect
        .center();
    click_at(&ctx, &mut studio, plus, PointerButton::Primary, 900.0);
    let color = Rgba::new(0xD9, 0x7C, 0x5B, 0x80);
    let labels = click_label(&ctx, &mut studio, "Fill");
    let swatch = swatch_rect(&labels, color).center();
    click_at(&ctx, &mut studio, swatch, PointerButton::Primary, 900.0);
    assert_eq!(
        studio.doc.find_shape(0, selected).unwrap().style.fill,
        Fill::Solid(color)
    );
    let labels = click_label(&ctx, &mut studio, "Stroke");
    let swatch = swatch_rect(&labels, color).center();
    click_at(&ctx, &mut studio, swatch, PointerButton::Primary, 900.0);
    assert_eq!(
        studio
            .doc
            .find_shape(0, selected)
            .unwrap()
            .style
            .stroke
            .as_ref()
            .unwrap()
            .color,
        color
    );
    assert_eq!(
        studio.selection,
        vec![(0, selected)],
        "typing in the palette changed canvas selection"
    );
    click_label(&ctx, &mut studio, "Filter names or hex colors…");
    let labels = replace_field(&ctx, &mut studio, "does-not-match");
    assert!(
        labels
            .iter()
            .any(|text| text.label.starts_with("No matching palettes."))
    );
    replace_field(&ctx, &mut studio, "");
    let history_before_save = studio.history.len();
    click_label(&ctx, &mut studio, "Save •");
    until(&ctx, &mut studio, |s| {
        s.libraries.projects.get(&temp.0).is_some_and(|d| !d.dirty)
    });
    let json = std::fs::read_to_string(temp.0.join(".omacolors")).unwrap();
    let reloaded = crate::palette::decode(&json).unwrap();
    assert_eq!(reloaded, vec![Palette::new("Studio North", vec![color])]);
    assert_eq!(
        studio.history.len(),
        history_before_save,
        "library save changed artwork undo history"
    );
    jobs::cancel::<Snapshot>(&ctx, SYNC);
}
