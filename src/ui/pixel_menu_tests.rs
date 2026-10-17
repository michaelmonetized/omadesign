use super::*;
use crate::document::{Layer, Pixels};
use crate::ui::{channel_adjustment, raster, welcome::keyboard_tests as keys};
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2};
use std::time::{Duration, Instant};

struct Seat {
    ctx: egui::Context,
    time: f64,
    output: egui::FullOutput,
}
impl Seat {
    fn new() -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::ui::theme::apply(&ctx);
        Self {
            ctx,
            time: 0.,
            output: Default::default(),
        }
    }
    fn frame(&mut self, studio: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
        self.time += 0.2;
        channel_adjustment::tick(&self.ctx, studio);
        let mut output = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1600., 1000.))),
                time: Some(self.time),
                events,
                ..Default::default()
            },
            |ui| {
                top_bar(ui, studio);
                raster::show(ui, studio);
                channel_adjustment::show(ui.ctx(), studio);
            },
        );
        output.textures_delta.clear();
        self.output = output.clone();
        output
    }
    fn click(&mut self, studio: &mut Studio, name: &str) {
        for _ in 0..3 {
            self.frame(studio, vec![]);
        }
        assert!(
            self.output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .any(|(_, node)| node.label() == Some(name)),
            "missing menu {name}; actual labels: {:?}",
            self.output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .filter_map(|(_, node)| node.label().map(|label| (
                    label,
                    node.bounds(),
                    node.is_disabled()
                )))
                .collect::<Vec<_>>()
        );
        let bounds = keys::named(&self.output, name).1.bounds().unwrap();
        let pos = egui::pos2(
            ((bounds.x0 + bounds.x1) / 2.) as f32,
            ((bounds.y0 + bounds.y1) / 2.) as f32,
        );
        self.frame(studio, vec![Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            self.frame(
                studio,
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
        self.frame(studio, vec![]);
    }
    fn open_kind(&mut self, studio: &mut Studio, kind: crate::raster::Kind) {
        self.frame(studio, vec![]);
        if kind.category() == "Color & tone" {
            self.click(studio, "Image");
            self.click(studio, "Adjustments ⏵");
        } else {
            self.click(studio, "Filter");
        }
        self.click(studio, &format!("{} ⏵", kind.category()));
        let ctx = self.ctx.clone();
        keys::tab_to(&ctx, kind.name(), |events| self.frame(studio, events));
        self.frame(studio, keys::key(Key::Enter, Modifiers::NONE));
        self.frame(studio, vec![]);
    }
    fn invoke(&mut self, studio: &mut Studio, name: &str) {
        let ctx = self.ctx.clone();
        keys::tab_to(&ctx, name, |events| self.frame(studio, events));
        self.frame(studio, keys::key(Key::Space, Modifiers::NONE));
        self.frame(studio, vec![]);
    }
    fn wait(&mut self, studio: &mut Studio, condition: impl Fn(&egui::Context, &Studio) -> bool) {
        let until = Instant::now() + Duration::from_secs(15);
        loop {
            self.frame(studio, vec![]);
            if condition(&self.ctx, studio) {
                break;
            }
            assert!(
                Instant::now() < until,
                "real menu dialog worker did not finish"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn heading(&self, text: &str) -> bool {
        fn contains(shape: &egui::Shape, text: &str) -> bool {
            match shape {
                egui::Shape::Text(t) => t.galley.text().starts_with(text),
                egui::Shape::Vec(v) => v.iter().any(|s| contains(s, text)),
                _ => false,
            }
        }
        self.output.shapes.iter().any(|s| contains(&s.shape, text))
    }
}
fn alpha() -> Studio {
    let mut studio = crate::app::channel_adjustment::tests::fixture();
    studio.persona = Persona::Pixel;
    studio
}
fn image() -> Studio {
    let mut studio = Studio::new();
    studio.show_welcome = false;
    studio.persona = Persona::Pixel;
    studio.doc = crate::channels::tests::fixture();
    studio.active_layer = Some(0);
    studio
}

#[test]
fn craft_pixel_command_menus_every_existing_kind_opens_by_native_menu_and_cancels() {
    let mut seat = Seat::new();
    let mut studio = alpha();
    let before = crate::project::encode(&studio.doc).unwrap();
    let history = studio.history.len();
    for kind in crate::raster::Kind::ALL {
        seat.open_kind(&mut studio, kind);
        seat.wait(&mut studio, |ctx, studio| {
            channel_adjustment::preview(ctx, studio).is_some()
        });
        assert!(channel_adjustment::is_open(&seat.ctx), "{}", kind.name());
        assert!(
            seat.heading(&format!("{} · Curve alpha", kind.name())),
            "wrong actual dialog for {}",
            kind.name()
        );
        assert!(!raster::is_open(&seat.ctx));
        seat.invoke(&mut studio, "Cancel");
        assert!(!channel_adjustment::is_open(&seat.ctx));
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
        assert_eq!(studio.history.len(), history);
    }
}

#[test]
fn craft_pixel_command_menus_image_adjustment_commits_saved_alpha_and_quick_mask_once() {
    for quick in [false, true] {
        let mut studio = alpha();
        let original = studio.doc.channels.last().unwrap().plane.clone();
        if quick {
            let target = studio.capture_channels().unwrap();
            studio
                .load_channel_selection(
                    &target,
                    original.samples.clone(),
                    crate::app::masking::SelectionSpace {
                        w: 8,
                        h: 4,
                        transform: tiny_skia::Transform::identity(),
                    },
                )
                .unwrap();
            studio.set_quick_mask(true).unwrap();
        }
        let before = crate::project::encode(&studio.doc).unwrap();
        let history = studio.history.len();
        let mut seat = Seat::new();
        seat.open_kind(&mut studio, crate::raster::Kind::Invert);
        seat.wait(&mut studio, |ctx, studio| {
            channel_adjustment::preview(ctx, studio).is_some()
        });
        seat.invoke(&mut studio, "Apply");
        seat.wait(&mut studio, |ctx, _| !channel_adjustment::is_open(ctx));
        assert_eq!(studio.history.len(), history + 1);
        let actual = if quick {
            &studio.doc.quick_mask.as_ref().unwrap().channel.plane
        } else {
            &studio.doc.channels.last().unwrap().plane
        };
        assert!(
            actual
                .samples
                .iter()
                .zip(original.samples.iter())
                .all(|(a, b)| a.to_bits() == (1. - b).to_bits())
        );
        let after = crate::project::encode(&studio.doc).unwrap();
        assert_eq!(
            crate::project::encode(&crate::project::decode(&after).unwrap()).unwrap(),
            after
        );
        studio.undo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
        studio.redo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), after);
        if quick {
            assert_eq!(studio.doc.channels.last().unwrap().plane, original);
        }
    }
}

#[test]
fn craft_pixel_command_menus_image_adjustment_keeps_layer_mask_and_artwork_targets_separate() {
    for mask in [false, true] {
        let mut studio = image();
        studio.doc.layers[0].mask =
            Some(Pixels::from_rgba(8, 4, [32, 64, 96, 255].repeat(32)).unwrap());
        studio.paint_mask = mask;
        let untouched = serde_json::to_value(if mask {
            studio.doc.layers[0].kind.pixels().unwrap()
        } else {
            studio.doc.layers[0].mask.as_ref().unwrap()
        })
        .unwrap();
        let before = crate::project::encode(&studio.doc).unwrap();
        let mut seat = Seat::new();
        seat.open_kind(&mut studio, crate::raster::Kind::Invert);
        seat.wait(&mut studio, |ctx, _| raster::ready(ctx));
        assert!(raster::is_open(&seat.ctx));
        assert!(!channel_adjustment::is_open(&seat.ctx));
        seat.invoke(&mut studio, "Apply");
        seat.wait(&mut studio, |ctx, _| !raster::is_open(ctx));
        let layer = &studio.doc.layers[0];
        let actual = if mask {
            layer.mask.as_ref().unwrap()
        } else {
            layer.kind.pixels().unwrap()
        };
        let expected = if mask {
            [223, 191, 159, 255]
        } else {
            [191, 127, 64, 128]
        };
        assert_eq!(actual.data.as_slice(), expected.repeat(32));
        assert_eq!(
            serde_json::to_value(if mask {
                layer.kind.pixels().unwrap()
            } else {
                layer.mask.as_ref().unwrap()
            })
            .unwrap(),
            untouched
        );
        assert_eq!(studio.history.len(), 1);
        let after = crate::project::encode(&studio.doc).unwrap();
        assert_eq!(
            crate::project::encode(&crate::project::decode(&after).unwrap()).unwrap(),
            after
        );
        studio.undo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
        studio.redo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), after);
    }
}

#[test]
fn craft_pixel_command_menus_invalid_targets_disable_commands_without_opening_or_editing() {
    for case in 0..4 {
        let mut studio = image();
        match case {
            0 => studio.active_layer = None,
            1 => studio.doc.layers[0].locked = true,
            2 => studio.doc.layers[0] = Layer::vector("Vector"),
            _ => studio.updates.freezing = true,
        }
        let before = crate::project::encode(&studio.doc).unwrap();
        let mut seat = Seat::new();
        seat.frame(&mut studio, vec![]);
        seat.click(&mut studio, "Image");
        seat.click(&mut studio, "Adjustments ⏵");
        assert!(keys::named(&seat.output, "Color & tone ⏵").1.is_disabled());
        assert!(keys::named(&seat.output, "Curves…").1.is_disabled());
        assert!(!channel_adjustment::is_open(&seat.ctx) && !raster::is_open(&seat.ctx));
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
        assert_eq!(studio.history.len(), 0);
    }
}
