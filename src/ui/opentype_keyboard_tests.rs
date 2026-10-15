use super::*;
use crate::ui::welcome::keyboard_tests as keys;
use egui::{Event, Key, Modifiers};

fn frame(
    ctx: &egui::Context,
    studio: &mut Studio,
    font: &str,
    preceding: bool,
    events: Vec<Event>,
) -> egui::FullOutput {
    keys::render(ctx, events, |ui| {
        if preceding {
            ui.label("Conditional inspector status");
            let _ = ui.button("Conditional inspector action");
        }
        show(ui, studio, font);
    })
}

fn press(ctx: &egui::Context, studio: &mut Studio, font: &str, label: &str) -> egui::FullOutput {
    keys::tab_to(ctx, label, |events| frame(ctx, studio, font, false, events));
    frame(
        ctx,
        studio,
        font,
        false,
        keys::key(Key::Enter, Modifiers::NONE),
    );
    frame(ctx, studio, font, false, vec![])
}

#[test]
fn keyboard_opentype_uses_real_loaded_font_support_native_choices_and_stable_property_ids() {
    let fonts = egui::FontDefinitions::default();
    let mut fixture = None;
    for (name, data) in fonts.font_data {
        let bytes = std::sync::Arc::new(data.font.to_vec());
        let id = format!("omatype:{:032x}", crate::typography::fingerprint(&bytes));
        if crate::text::register_memory_font(&id, &name, bytes).is_err() {
            continue;
        }
        let features = crate::text::font_features(&id);
        let has = |tag: [u8; 4]| {
            features
                .iter()
                .any(|(feature, _)| feature.to_bytes() == tag)
        };
        if has(*b"liga") && has(*b"pnum") && has(*b"subs") {
            fixture = Some(id);
            break;
        }
    }
    let font =
        fixture.expect("A bundled egui font must expose actual liga, pnum and subs features");
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    crate::ui::theme::apply(&ctx);
    let mut studio = Studio::new();
    studio.text_font = font.clone();
    assert!(studio.selection.is_empty());
    let before = crate::project::encode(&studio.doc).unwrap();
    let history = studio.history.len();
    frame(&ctx, &mut studio, &font, false, vec![]);
    let output = press(&ctx, &mut studio, &font, "OpenType features");
    let figures = keys::named(&output, "Figures").0;
    let liga = keys::named(&output, "Ligatures").0;
    let original = studio.selection_feature(*b"liga").unwrap();
    keys::tab_to(&ctx, "Ligatures", |events| {
        frame(&ctx, &mut studio, &font, false, events)
    });
    let output = frame(&ctx, &mut studio, &font, true, vec![]);
    assert_eq!(keys::named(&output, "Ligatures").0, liga);
    assert_eq!(keys::named(&output, "Figures").0, figures);
    keys::focused(&ctx, &output, "Ligatures");
    frame(
        &ctx,
        &mut studio,
        &font,
        true,
        keys::key(Key::Space, Modifiers::NONE),
    );
    assert_eq!(
        studio.selection_feature(*b"liga"),
        Some(u32::from(original == 0))
    );
    assert_eq!(
        studio.selection_feature(*b"clig"),
        Some(u32::from(original == 0))
    );
    press(&ctx, &mut studio, &font, "Figures");
    press(&ctx, &mut studio, &font, "Default");
    for (tag, value) in [*b"onum", *b"lnum", *b"pnum", *b"tnum"]
        .into_iter()
        .zip([0, 0, 0, 0])
    {
        assert_eq!(studio.selection_feature(tag), Some(value));
    }
    press(&ctx, &mut studio, &font, "Position");
    press(&ctx, &mut studio, &font, "Subscript");
    for (tag, value) in [*b"sups", *b"subs", *b"ordn"].into_iter().zip([0, 1, 0]) {
        assert_eq!(studio.selection_feature(tag), Some(value));
    }
    let output = frame(&ctx, &mut studio, &font, true, vec![]);
    assert_eq!(keys::named(&output, "Figures").0, figures);
    assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    assert_eq!(
        studio.history.len(),
        history,
        "Next-text feature defaults do not edit existing artwork"
    );
}
