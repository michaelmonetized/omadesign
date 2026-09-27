//! Selection dialogs preview coverage on the canvas without changing artwork.
use crate::app::{
    Studio,
    masking::SelectionSpace,
    pixel_selection::{self, Edit},
};
use eframe::egui::{self, Context, Id, Ui};
use std::sync::{Arc, Mutex};

type Preview = (Arc<Vec<u8>>, SelectionSpace);
type Job = Arc<Mutex<Option<Option<Preview>>>>;

#[derive(Clone)]
struct Session {
    document: String,
    generation: u64,
    source: Arc<Vec<u8>>,
    space: SelectionSpace,
    settings: Edit,
    initial: Edit,
    aspect: f32,
    lock_aspect: bool,
    preview: Option<Preview>,
    rendered: Option<Edit>,
    revision: u64,
    job: Option<(Edit, Job)>,
    error: bool,
}

fn id() -> Id {
    Id::new("pixel-selection-dialog")
}

pub(super) fn is_open(ctx: &Context) -> bool {
    ctx.data(|d| d.get_temp::<Session>(id()).is_some())
}

pub(super) fn ready(ctx: &Context) -> bool {
    ctx.data(|d| {
        d.get_temp::<Session>(id())
            .is_none_or(|s| s.rendered == Some(s.settings))
    })
}

pub(super) fn canvas_preview(
    ctx: &Context,
    studio: &Studio,
) -> Option<(Arc<Vec<u8>>, SelectionSpace, u64)> {
    let session = ctx.data(|d| d.get_temp::<Session>(id()))?;
    if session.document != studio.swap_id || session.generation != studio.pixel_sel_gen {
        return None;
    }
    let (values, space) = session.preview?;
    Some((values, space, session.revision))
}

fn open(ctx: &Context, studio: &mut Studio, settings: Edit) {
    studio.end_pixel_stroke(false);
    let Some(space) = studio.pixel_selection_space() else {
        return;
    };
    let source = studio.pixel_sel.as_ref().unwrap();
    let Some((x0, y0, x1, y1)) = crate::paint::selection_bounds(source, space.w, space.h) else {
        return;
    };
    ctx.data_mut(|d| {
        d.insert_temp(
            id(),
            Session {
                document: studio.swap_id.clone(),
                generation: studio.pixel_sel_gen,
                source: Arc::new(source.clone()),
                space,
                settings,
                initial: settings,
                aspect: (x1 - x0) as f32 / (y1 - y0) as f32,
                lock_aspect: true,
                preview: None,
                rendered: None,
                revision: 0,
                job: None,
                error: false,
            },
        )
    });
}

pub(super) fn menu(ui: &mut Ui, studio: &mut Studio) {
    if ui.button("All pixels  Ctrl+A").clicked() {
        studio.select_all_pixels();
        ui.close();
    }
    if ui
        .add_enabled(
            studio.pixel_sel.is_some(),
            egui::Button::new("Deselect  Ctrl+D"),
        )
        .clicked()
    {
        studio.set_pixel_sel(None);
        ui.close();
    }
    if ui.button("Invert pixel selection").clicked() {
        studio.invert_pixel_selection();
        ui.close();
    }
    ui.separator();
    let bounds = studio
        .pixel_selection_space()
        .and_then(|s| crate::paint::selection_bounds(studio.pixel_sel.as_ref()?, s.w, s.h));
    let (width, height) = bounds.map_or((1.0, 1.0), |(x0, y0, x1, y1)| {
        ((x1 - x0) as f32, (y1 - y0) as f32)
    });
    for (label, settings) in [
        ("Move…", Edit::Move { x: 0.0, y: 0.0 }),
        ("Resize…", Edit::Resize { width, height }),
        ("Grow…", Edit::Grow { radius: 1 }),
        ("Shrink…", Edit::Shrink { radius: 1 }),
        ("Feather…", Edit::Feather { radius: 2 }),
        (
            "Reshape…",
            Edit::Reshape {
                angle: 0.0,
                skew_x: 0.0,
                skew_y: 0.0,
            },
        ),
    ] {
        if ui
            .add_enabled(bounds.is_some(), egui::Button::new(label))
            .on_disabled_hover_text("Make a marquee, ellipse, or lasso selection first.")
            .clicked()
        {
            open(ui.ctx(), studio, settings);
            ui.close();
        }
    }
}

fn controls(ui: &mut Ui, session: &mut Session) {
    match &mut session.settings {
        Edit::Move { x, y } => {
            ui.horizontal(|ui| {
                ui.label("Horizontal");
                ui.add(
                    egui::DragValue::new(x)
                        .speed(1.0)
                        .range(-100_000.0..=100_000.0)
                        .suffix(" px"),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Vertical");
                ui.add(
                    egui::DragValue::new(y)
                        .speed(1.0)
                        .range(-100_000.0..=100_000.0)
                        .suffix(" px"),
                );
            });
        }
        Edit::Resize { width, height } => {
            ui.horizontal(|ui| {
                ui.label("Width");
                if ui
                    .add(
                        egui::DragValue::new(width)
                            .speed(1.0)
                            .range(1.0..=100_000.0)
                            .suffix(" px"),
                    )
                    .changed()
                    && session.lock_aspect
                {
                    *height = *width / session.aspect;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Height");
                if ui
                    .add(
                        egui::DragValue::new(height)
                            .speed(1.0)
                            .range(1.0..=100_000.0)
                            .suffix(" px"),
                    )
                    .changed()
                    && session.lock_aspect
                {
                    *width = *height * session.aspect;
                }
            });
            if ui
                .checkbox(&mut session.lock_aspect, "Keep proportions")
                .changed()
                && session.lock_aspect
            {
                session.aspect = *width / *height;
            }
            ui.small("Anchored at the selection’s top-left corner.");
        }
        Edit::Grow { radius } | Edit::Shrink { radius } | Edit::Feather { radius } => {
            ui.horizontal(|ui| {
                ui.label("Radius");
                ui.add(egui::DragValue::new(radius).range(0..=1024).suffix(" px"));
            });
        }
        Edit::Reshape {
            angle,
            skew_x,
            skew_y,
        } => {
            ui.add(
                egui::Slider::new(angle, -180.0..=180.0)
                    .text("Rotation")
                    .suffix("°"),
            );
            ui.add(
                egui::Slider::new(skew_x, -80.0..=80.0)
                    .text("Horizontal skew")
                    .suffix("°"),
            );
            ui.add(
                egui::Slider::new(skew_y, -80.0..=80.0)
                    .text("Vertical skew")
                    .suffix("°"),
            );
            ui.small("Reshape around the center of the selection.");
        }
    }
}

pub(super) fn show(ctx: &Context, studio: &mut Studio) {
    let Some(mut session) = ctx.data_mut(|d| {
        let session = d.get_temp::<Session>(id());
        d.remove::<Session>(id());
        session
    }) else {
        return;
    };
    if session.document != studio.swap_id
        || session.generation != studio.pixel_sel_gen
        || studio.persona != crate::tools::Persona::Pixel
    {
        return;
    }
    if let Some((settings, job)) = &session.job {
        let result = job.lock().unwrap().take();
        if let Some(result) = result {
            if *settings == session.settings {
                session.error = result.is_none();
                session.preview = result;
                session.rendered = Some(*settings);
                session.revision = crate::document::next_id();
                ctx.request_repaint();
            }
            session.job = None;
        }
    }
    let mut apply = false;
    let mut cancel = false;
    let mut opened = true;
    egui::Window::new(session.settings.title())
        .id(id())
        .open(&mut opened)
        .collapsible(false)
        .resizable(false)
        .default_pos(egui::pos2(80.0, 110.0))
        .show(ctx, |ui| {
            ui.set_min_width(280.0);
            ui.label("Adjust the selection boundary.");
            ui.small("Distances use the selected image’s pixels. Edges clip to its bounds.");
            ui.separator();
            controls(ui, &mut session);
            ui.separator();
            let ready = session.rendered == Some(session.settings) && !session.error;
            if !ready && !session.error {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Updating preview…");
                });
            }
            if session.error {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    "Unable to preview this selection.",
                );
            }
            if ready
                && session
                    .preview
                    .as_ref()
                    .is_some_and(|(v, _)| !v.iter().any(|v| *v > 0))
            {
                ui.label("This will leave an empty selection.");
            }
            ui.horizontal(|ui| {
                if ui.button("Reset").clicked() {
                    session.settings = session.initial;
                }
                cancel = ui.button("Cancel").clicked();
                apply = ui.add_enabled(ready, egui::Button::new("Apply")).clicked();
            });
        });
    if !opened || cancel || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        ctx.request_repaint();
        return;
    }
    if apply {
        if let Some((values, space)) = session.preview {
            studio.replace_pixel_selection((*values).clone(), space);
        }
        ctx.request_repaint();
        return;
    }
    if session.rendered != Some(session.settings) && session.job.is_none() {
        let source = session.source.clone();
        let space = session.space;
        let settings = session.settings;
        let job: Job = Arc::new(Mutex::new(None));
        session.job = Some((settings, job.clone()));
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result =
                pixel_selection::edit(&source, space, settings).map(|(v, s)| (Arc::new(v), s));
            *job.lock().unwrap() = Some(result);
            ctx.request_repaint();
        });
    }
    ctx.data_mut(|d| d.insert_temp(id(), session));
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Modifiers, PointerButton, Pos2, Rect, Shape};

    fn studio() -> Studio {
        let mut s = Studio::new();
        s.doc = crate::document::Document::new("Selection UI", 32.0, 24.0, 96.0);
        s.doc.layers = vec![crate::document::Layer::raster("Pixels", 32, 24)];
        s.active_layer = Some(0);
        s.persona = crate::tools::Persona::Pixel;
        s.set_pixel_sel(Some(crate::paint::fill_rect_mask(
            32, 24, 8.0, 8.0, 16.0, 16.0,
        )));
        s
    }

    fn frame(ctx: &Context, studio: &mut Studio, events: Vec<Event>) -> Vec<(String, Rect)> {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 650.0))),
                events,
                ..Default::default()
            },
            |ui| {
                ui.add_enabled_ui(!is_open(ctx), |ui| {
                    ui.menu_button("Select", |ui| super::super::selection::menu(ui, studio));
                });
                show(ctx, studio);
            },
        );
        fn collect(shape: &Shape, out: &mut Vec<(String, Rect)>) {
            match shape {
                Shape::Text(t) => out.push((
                    t.galley.text().into(),
                    t.galley.rect.translate(t.pos.to_vec2()),
                )),
                Shape::Vec(v) => {
                    for s in v {
                        collect(s, out);
                    }
                }
                _ => {}
            }
        }
        let mut labels = vec![];
        for shape in output.shapes {
            collect(&shape.shape, &mut labels);
        }
        output.textures_delta.clear();
        labels
    }

    fn click(ctx: &Context, studio: &mut Studio, label: &str) {
        let labels = frame(ctx, studio, vec![]);
        let pos = labels
            .iter()
            .find(|(text, _)| text == label)
            .unwrap_or_else(|| panic!("Missing {label}: {labels:?}"))
            .1
            .center();
        frame(
            ctx,
            studio,
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        frame(
            ctx,
            studio,
            vec![Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
        );
    }

    fn settle(ctx: &Context, studio: &mut Studio) {
        for _ in 0..100 {
            frame(ctx, studio, vec![]);
            if ctx.data(|d| {
                d.get_temp::<Session>(id())
                    .is_some_and(|s| s.rendered == Some(s.settings))
            }) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("Selection preview timed out");
    }

    #[test]
    fn select_menu_opens_all_dialogs_and_cancel_preserves_selection_and_pixels() {
        let ctx = Context::default();
        let mut studio = studio();
        let original = studio.pixel_sel.clone();
        let pixels = studio.doc.layers[0].kind.pixels().unwrap().data.clone();
        for label in [
            "Move…",
            "Resize…",
            "Grow…",
            "Shrink…",
            "Feather…",
            "Reshape…",
        ] {
            frame(&ctx, &mut studio, vec![]);
            click(&ctx, &mut studio, "Select");
            click(&ctx, &mut studio, label);
            assert!(is_open(&ctx), "{label}");
            settle(&ctx, &mut studio);
            assert!(canvas_preview(&ctx, &studio).is_some());
            assert_eq!(studio.pixel_sel, original);
            click(&ctx, &mut studio, "Cancel");
            assert!(!is_open(&ctx));
            assert_eq!(studio.pixel_sel, original);
            assert_eq!(studio.doc.layers[0].kind.pixels().unwrap().data, pixels);
        }
    }

    #[test]
    fn apply_commits_preview_and_stale_sessions_are_discarded() {
        let ctx = Context::default();
        let mut studio = studio();
        open(&ctx, &mut studio, Edit::Grow { radius: 2 });
        settle(&ctx, &mut studio);
        let preview = canvas_preview(&ctx, &studio).unwrap();
        click(&ctx, &mut studio, "Apply");
        assert!(!is_open(&ctx));
        assert_eq!(studio.pixel_sel.as_deref(), Some(preview.0.as_slice()));
        assert_eq!(
            crate::paint::selection_bounds(studio.pixel_sel.as_ref().unwrap(), 32, 24),
            Some((6, 6, 18, 18))
        );
        open(&ctx, &mut studio, Edit::Feather { radius: 2 });
        studio.set_pixel_sel(None);
        frame(&ctx, &mut studio, vec![]);
        assert!(!is_open(&ctx));
        assert!(studio.pixel_sel.is_none());
        studio.select_all_pixels();
        open(&ctx, &mut studio, Edit::Shrink { radius: 2 });
        studio.swap_id = "another-document".into();
        frame(&ctx, &mut studio, vec![]);
        assert!(!is_open(&ctx));
        assert!(studio.pixel_sel.as_ref().unwrap().iter().all(|v| *v == 255));
    }
}
