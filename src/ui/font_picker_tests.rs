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

fn native_frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
    crate::ui::welcome::keyboard_tests::render(ctx, events, |ui| {
        ui.set_max_width(300.);
        character_studio(ui, studio);
    })
}

#[test]
fn keyboard_font_native_local_search_choice_escape_and_typed_history_keep_owner() {
    use crate::ui::welcome::keyboard_tests as keys;
    let (ctx, mut studio, target, label) = fixture();
    ctx.enable_accesskit();
    let before = crate::project::encode(&studio.doc).unwrap();
    keys::tab_to(&ctx, "Font family", |events| {
        native_frame(&ctx, &mut studio, events)
    });
    native_frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
    let output = keys::tab_to(&ctx, "Search local fonts", |events| {
        native_frame(&ctx, &mut studio, events)
    });
    keys::focused(&ctx, &output, "Search local fonts");
    let search = keys::named(&output, "Search local fonts").0;
    keys::replace("Search target", |events| {
        native_frame(&ctx, &mut studio, events)
    });
    let output = native_frame(&ctx, &mut studio, vec![]);
    assert_eq!(keys::named(&output, "Search local fonts").0, search);
    keys::focused(&ctx, &output, "Search local fonts");
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    let choice = format!("Project font {label}");
    keys::tab_to(&ctx, &choice, |events| {
        native_frame(&ctx, &mut studio, events)
    });
    native_frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
    assert!(!egui::Popup::is_any_open(&ctx));
    assert_eq!(studio.selected_type().unwrap().font, target);
    let after = crate::project::encode(&studio.doc).unwrap();
    studio.undo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    studio.redo();
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), after);
    keys::tab_to(&ctx, "Font family", |events| {
        native_frame(&ctx, &mut studio, events)
    });
    native_frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
    keys::tab_to(&ctx, "Search local fonts", |events| {
        native_frame(&ctx, &mut studio, events)
    });
    native_frame(&ctx, &mut studio, keys::key(Key::Escape, Modifiers::NONE));
    assert!(!egui::Popup::is_any_open(&ctx));
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), after);
}

#[test]
fn keyboard_font_native_offline_google_query_and_exact_variant_keys_do_not_start_jobs() {
    use crate::ui::welcome::keyboard_tests as keys;
    let (ctx, mut studio, _, _) = fixture();
    ctx.enable_accesskit();
    studio.google_catalog_loaded = true;
    studio.google_catalog = vec![crate::google_fonts::GoogleFont {
        id: "ui05-offline-fixture".into(),
        family: "UI05 Offline Family".into(),
        variants: vec![
            "regular".into(),
            "italic".into(),
            "700".into(),
            "700italic".into(),
        ],
        subsets: vec![],
        category: "sans-serif".into(),
        files: Default::default(),
    }];
    studio.google_status = "Offline catalog fixture".into();
    let before = crate::project::encode(&studio.doc).unwrap();
    keys::tab_to(&ctx, "Google Fonts", |events| {
        native_frame(&ctx, &mut studio, events)
    });
    native_frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
    let output = keys::tab_to(&ctx, "Search Google fonts", |events| {
        native_frame(&ctx, &mut studio, events)
    });
    let search = keys::named(&output, "Search Google fonts").0;
    keys::replace("Offline", |events| native_frame(&ctx, &mut studio, events));
    let output = native_frame(&ctx, &mut studio, vec![]);
    keys::focused(&ctx, &output, "Search Google fonts");
    assert_eq!(studio.google_query, "Offline");
    assert!(studio.font_query.is_empty());
    assert!(
        !keys::named(&output, "Search Google fonts")
            .1
            .described_by()
            .is_empty()
    );
    keys::tab_to(&ctx, "Google font variant", |events| {
        native_frame(&ctx, &mut studio, events)
    });
    native_frame(
        &ctx,
        &mut studio,
        keys::key(Key::ArrowDown, Modifiers::NONE),
    );
    assert_eq!(studio.google_variant, "italic");
    let output = native_frame(&ctx, &mut studio, vec![]);
    keys::focused(&ctx, &output, "Google font variant");
    native_frame(&ctx, &mut studio, keys::key(Key::End, Modifiers::NONE));
    assert_eq!(studio.google_variant, "700italic");
    let output = native_frame(&ctx, &mut studio, vec![]);
    assert_eq!(keys::named(&output, "Search Google fonts").0, search);
    assert!(
        keys::named(
            &output,
            "Download Google font UI05 Offline Family 700italic"
        )
        .1
        .is_disabled()
            == false
    );
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    assert!(!crate::ui::jobs::any_running(&ctx));
}

#[test]
fn keyboard_font_native_effect_blend_retirement_and_layer_owner_guards() {
    use crate::color::Blend;
    use crate::ui::welcome::keyboard_tests as keys;
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    crate::ui::theme::apply(&ctx);
    let make = crate::filter::Fx::catalog()
        .iter()
        .find(|(_, make)| make().appearance_mut().is_some())
        .unwrap()
        .1;
    let mut stack = crate::filter::FilterStack {
        items: vec![make(), make()],
        ..Default::default()
    };
    let name = format!("{} effect 1 blend", stack.items[0].name());
    let mut frame = |events| {
        keys::render(&ctx, events, |ui| {
            ui.set_max_width(300.);
            super::fx_stack_editor(ui, &mut stack, "native-blend");
        })
    };
    let output = keys::tab_to(&ctx, &name, &mut frame);
    let stale = keys::named(&output, &name).0;
    frame(keys::key(Key::ArrowDown, Modifiers::NONE));
    let output = frame(vec![]);
    keys::focused(&ctx, &output, &name);
    drop(frame);
    assert_eq!(*stack.items[0].appearance_mut().unwrap().0, Blend::ALL[1]);
    stack.items.remove(0);
    let mut frame = |events| {
        keys::render(&ctx, events, |ui| {
            ui.set_max_width(300.);
            super::fx_stack_editor(ui, &mut stack, "native-blend");
        })
    };
    let output = frame(vec![]);
    assert_ne!(keys::named(&output, &name).0, stale);
    assert_ne!(
        ctx.memory(|memory| memory.focused())
            .map(|id| id.accesskit_id()),
        Some(stale)
    );
    drop(frame);
    let mut studio = Studio::new();
    studio.show_welcome = false;
    let layer = studio.active_layer.unwrap();
    let expected = studio.doc.layers[layer].blend;
    let mut frame = |events| {
        keys::render(&ctx, events, |ui| {
            ui.set_max_width(300.);
            super::layers_studio(ui, &mut studio);
        })
    };
    let output = keys::tab_to(&ctx, "Layer blend", &mut frame);
    let first = keys::named(&output, "Layer blend").0;
    frame(keys::key(Key::End, Modifiers::NONE));
    drop(frame);
    assert_eq!(studio.doc.layers[layer].blend, *Blend::ALL.last().unwrap());
    studio.add_layer_group();
    let output = keys::render(&ctx, vec![], |ui| super::layers_studio(ui, &mut studio));
    assert_ne!(keys::named(&output, "Layer blend").0, first);
    assert!(keys::named(&output, "Layer blend").1.is_disabled());
    assert_ne!(studio.doc.layers[layer].blend, expected);
}
