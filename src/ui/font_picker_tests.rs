use super::character_studio;
use crate::app::Studio;
use crate::document::{Document, Layer, Shape, Style};
use crate::geom::{Geom, Pt, TypeRun};
use crate::typography::{LoadedFont, LoadedKit, TypeKit};
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, Rect, vec2};
use std::sync::Arc;

fn fixture() -> (egui::Context, Studio, String, String) {
    let ctx = egui::Context::default();
    crate::ui::theme::apply(&ctx);
    let mut studio = Studio::new();
    studio.doc = Document::new("Font picker input", 640., 480., 72.);
    let mut geometry = Geom::Text(TypeRun {
        content: "Search must not edit this artwork".into(),
        origin: Pt::new(20., 60.),
        ..Default::default()
    });
    crate::text::fill_contours(&mut geometry);
    let shape = Shape::new(geometry, Style::default());
    studio.selection = vec![(0, shape.id)];
    let mut layer = Layer::vector("Type");
    layer.kind.shapes_mut().unwrap().push(shape);
    studio.doc.layers = vec![layer];
    studio.active_layer = Some(0);
    studio.font_recents.clear();
    studio.font_query.clear();

    // A real in-memory project font exercises applying and undoing a font
    // without writing the desktop's installed-font recents or downloading.
    let bytes = Arc::new(include_bytes!("../../tests/assets/fonts/EBGaramond.ttf").to_vec());
    let family = crate::text::font_name(&bytes).unwrap();
    let id = format!("omatype:{:032x}", crate::typography::fingerprint(&bytes));
    crate::text::register_memory_font(&id, &family, bytes.clone()).unwrap();
    studio.libraries.typography = Some(Arc::new(LoadedKit {
        kit: TypeKit::default(),
        fonts: ["Search target", "Other face"]
            .into_iter()
            .map(|role| LoadedFont {
                role: role.into(),
                file: "fonts/EBGaramond.ttf".into(),
                family: family.clone(),
                id: id.clone(),
                bytes: bytes.clone(),
            })
            .collect(),
        stamp: 0,
    }));
    (ctx, studio, id, format!("Search target · {family}"))
}

fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) -> Vec<(String, Rect)> {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(960., 900.))),
            events,
            ..Default::default()
        },
        |ui| {
            ui.set_max_width(300.);
            character_studio(ui, studio);
        },
    );
    fn collect(shape: &egui::Shape, labels: &mut Vec<(String, Rect)>) {
        match shape {
            egui::Shape::Text(text) => labels.push((
                text.galley.text().to_owned(),
                text.galley.rect.translate(text.pos.to_vec2()),
            )),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, labels)),
            _ => {}
        }
    }
    let mut labels = vec![];
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut labels);
    }
    output.textures_delta.clear();
    labels
}

fn click_at(ctx: &egui::Context, studio: &mut Studio, position: Pos2) {
    frame(ctx, studio, vec![Event::PointerMoved(position)]);
    for pressed in [true, false] {
        frame(
            ctx,
            studio,
            vec![Event::PointerButton {
                pos: position,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            }],
        );
    }
    frame(ctx, studio, vec![]);
}

fn click_label(ctx: &egui::Context, studio: &mut Studio, label: &str) {
    let labels = frame(ctx, studio, vec![]);
    let position = labels
        .iter()
        .find(|(text, _)| text == label)
        .unwrap_or_else(|| panic!("Missing picker label {label:?}"))
        .1
        .center();
    click_at(ctx, studio, position);
}

fn open(ctx: &egui::Context, studio: &mut Studio) {
    let label = crate::text::label_for(&studio.selected_type().unwrap().font);
    click_label(ctx, studio, &label);
    assert!(
        egui::Popup::is_any_open(ctx),
        "font button must open the picker"
    );
}

#[test]
fn font_search_click_keeps_focus_filters_and_selects_with_undo() {
    let (ctx, mut studio, target, target_label) = fixture();
    let before = crate::project::encode(&studio.doc).unwrap();
    let history = studio.history.len();
    open(&ctx, &mut studio);
    click_label(&ctx, &mut studio, "Search fonts");
    assert!(
        egui::Popup::is_any_open(&ctx),
        "clicking the search field must not dismiss the font picker"
    );
    assert!(
        ctx.egui_wants_keyboard_input(),
        "font search must own keyboard focus"
    );
    let labels = frame(&ctx, &mut studio, vec![Event::Text("Search target".into())]);
    assert_eq!(studio.font_query, "Search target");
    assert!(labels.iter().any(|(text, _)| text == &target_label));
    assert!(
        !labels
            .iter()
            .any(|(text, _)| text.starts_with("Other face ·"))
    );
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    assert_eq!(studio.history.len(), history);

    click_label(&ctx, &mut studio, &target_label);
    assert!(
        !egui::Popup::is_any_open(&ctx),
        "choosing a font must close the picker"
    );
    assert_eq!(studio.selected_type().unwrap().font, target);
    assert_eq!(studio.text_font, target);
    assert_eq!(studio.history.len(), history + 1);
    let applied = crate::project::encode(&studio.doc).unwrap();
    studio.undo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    studio.redo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), applied);

    open(&ctx, &mut studio);
    click_label(&ctx, &mut studio, &target_label);
    assert!(
        !egui::Popup::is_any_open(&ctx),
        "the current font also dismisses the picker"
    );
    assert_eq!(
        studio.history.len(),
        history + 1,
        "choosing the current font is not an edit"
    );
}

#[test]
fn font_search_escape_and_outside_click_dismiss_without_changing_type() {
    let (ctx, mut studio, _, _) = fixture();
    let before = crate::project::encode(&studio.doc).unwrap();
    open(&ctx, &mut studio);
    click_at(&ctx, &mut studio, egui::pos2(850., 800.));
    assert!(!egui::Popup::is_any_open(&ctx));
    open(&ctx, &mut studio);
    frame(
        &ctx,
        &mut studio,
        vec![Event::Key {
            key: Key::Escape,
            physical_key: Some(Key::Escape),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    assert!(!egui::Popup::is_any_open(&ctx));
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
}
