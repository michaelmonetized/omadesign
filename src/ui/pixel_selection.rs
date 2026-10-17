//! Selection dialogs preview coverage on the canvas without changing artwork.
use crate::app::{
    Studio,
    masking::SelectionSpace,
    pixel_selection::{self, Edit},
};
use eframe::egui::{self, Context, Id, Ui};
use std::sync::{Arc, Mutex};

type Preview = (Arc<crate::resources::Allocation<f32>>, SelectionSpace);
type Job = Arc<Mutex<Option<Option<Preview>>>>;

#[derive(Clone)]
struct Session {
    instance: Id,
    document: String,
    generation: u64,
    source: Arc<crate::resources::Allocation<f32>>,
    space: SelectionSpace,
    settings: Edit,
    initial: Edit,
    preview: Option<Preview>,
    rendered: Option<Edit>,
    revision: u64,
    job: Option<(Edit, Job)>,
    error: bool,
    draft_valid: bool,
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
) -> Option<(Arc<crate::resources::Allocation<f32>>, SelectionSpace, u64)> {
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
    let Some(_) = crate::paint::selection_bounds(source, space.w, space.h) else {
        return;
    };
    let instance = super::keyboard_controls::draft_id(ctx, "pixel-selection-radius");
    ctx.data_mut(|d| {
        d.insert_temp(
            id(),
            Session {
                instance,
                document: studio.swap_id.clone(),
                generation: studio.pixel_sel_gen,
                source: source.clone(),
                space,
                settings,
                initial: settings,
                preview: None,
                rendered: None,
                revision: 0,
                job: None,
                error: false,
                draft_valid: true,
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
    let enabled = bounds.is_some();
    if ui
        .add_enabled(enabled, egui::Button::new("Move / resize selection"))
        .clicked()
    {
        studio.begin_pixel_edit(crate::app::pixel_edit::ModeKind::Transform);
        ui.close();
    }
    ui.add_enabled_ui(enabled, |ui| {
        ui.menu_button("Reshape", |ui| {
            for mode in crate::deform::Mode::ALL {
                if ui.button(mode.label()).clicked() {
                    studio.begin_pixel_edit(crate::app::pixel_edit::ModeKind::Reshape(mode));
                    ui.close();
                }
            }
        })
    });
    for (label, settings) in [
        ("Grow…", Edit::Grow { radius: 1 }),
        ("Shrink…", Edit::Shrink { radius: 1 }),
        ("Feather…", Edit::Feather { radius: 2 }),
    ] {
        if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
            studio.pixel_edit = None;
            open(ui.ctx(), studio, settings);
            ui.close();
        }
    }
    if ui.button("Edit Bézier selection").clicked() {
        studio.set_tool(crate::tools::Tool::BezierLasso);
        studio.begin_pixel_edit(crate::app::pixel_edit::ModeKind::Bezier);
        ui.close();
    }

    if ui.button("New Bézier selection").clicked() {
        studio.set_tool(crate::tools::Tool::BezierLasso);
        studio.begin_new_bezier();
        ui.close();
    }
}

fn controls(ui: &mut Ui, session: &mut Session) -> super::fields::FieldResponse {
    let owner = Id::new((
        "selection-radius",
        session.instance,
        &session.document,
        session.generation,
        session.initial.title(),
    ));
    let initial = match session.initial {
        Edit::Grow { radius } | Edit::Shrink { radius } | Edit::Feather { radius } => radius,
    };
    match &mut session.settings {
        Edit::Grow { radius } | Edit::Shrink { radius } | Edit::Feather { radius } => {
            let response = super::fields::number(
                ui,
                owner,
                "Selection radius (px)",
                radius,
                0..=1024,
                " px",
                Some(initial),
                Default::default(),
            );
            super::welcome::focus_control(ui, &response.response);
            response
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
    let available = !studio.file_dialog_pending() && !studio.updates.freezing;
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
            let field = ui
                .add_enabled_ui(available, |ui| controls(ui, &mut session))
                .inner;
            session.draft_valid = super::fields::number_is_valid(&field);
            ui.separator();
            let ready = session.rendered == Some(session.settings) && !session.error;
            if !ready && !session.error {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Updating preview…");
                });
            }
            if session.error {
                super::welcome::control_error(
                    ui,
                    &[&field.response],
                    "Unable to preview this selection.",
                );
            }
            if ready
                && session
                    .preview
                    .as_ref()
                    .is_some_and(|(v, _)| !v.iter().any(|v| *v > 0.))
            {
                ui.label("This will leave an empty selection.");
            }
            ui.horizontal(|ui| {
                let response = ui.button("Cancel");
                super::welcome::focus_control(ui, &response);
                cancel = response.clicked();
                let response = ui.add_enabled(
                    available && ready && session.draft_valid,
                    egui::Button::new("Apply"),
                );
                super::welcome::focus_control(ui, &response);
                apply = available && session.draft_valid && response.clicked();
            });
        });
    if !opened || cancel || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        ctx.request_repaint();
        return;
    }
    if available && apply {
        if let Some((values, space)) = session.preview {
            studio.replace_pixel_selection_admitted(values, space);
        }
        ctx.request_repaint();
        return;
    }
    if available && session.rendered != Some(session.settings) && session.job.is_none() {
        let source = session.source.clone();
        let space = session.space;
        let settings = session.settings;
        let job: Job = Arc::new(Mutex::new(None));
        session.job = Some((settings, job.clone()));
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result =
                pixel_selection::edit_admitted(&source, space, settings).ok().flatten();
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
        s.doc.layers = vec![crate::document::Layer::raster("Pixels", 32, 24).unwrap()];
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
        let pixels = studio.doc.layers[0].kind.pixels().unwrap().data.to_vec();
        for label in ["Grow…", "Shrink…", "Feather…"] {
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
            assert_eq!(studio.doc.layers[0].kind.pixels().unwrap().data.as_slice(), pixels);
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
        assert_eq!(studio.pixel_sel.as_ref().map(|values| values.as_slice()), Some(preview.0.as_slice()));
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
        assert!(studio.pixel_sel.as_ref().unwrap().iter().all(|v| *v == 1.));
    }
}

#[cfg(test)]
mod keyboard_tests {
    use super::*;
    use crate::ui::welcome::keyboard_tests as keys;
    use egui::{Event, Key, Modifiers};

    fn frame(ctx: &Context, studio: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
        keys::render(ctx, events, |ui| {
            if !is_open(ctx) && !studio.file_dialog_pending() && !studio.updates.freezing {
                studio.handle_shortcuts(ui.ctx());
            }
            ui.add_enabled_ui(!is_open(ctx), |ui| {
                ui.menu_button("Select pixels", |ui| menu(ui, studio));
            });
            ui.interact(
                ui.available_rect_before_wrap(),
                Id::new("studio-canvas"),
                egui::Sense::click(),
            );
            super::super::keyboard_tools::show(ui.ctx(), studio);
            show(ui.ctx(), studio);
        })
    }

    fn settle(ctx: &Context, studio: &mut Studio) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            frame(ctx, studio, vec![]);
            if ready(ctx) {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "selection preview did not finish"
            );
            std::thread::yield_now();
        }
    }

    #[test]
    fn keyboard_selection_dialog_grows_keyboard_created_coverage_without_changing_pixels() {
        let ctx = Context::default();
        ctx.enable_accesskit();
        let mut studio = Studio::new();
        studio.doc = crate::document::Document::new("Keyboard selection radius", 32., 32., 72.);
        studio.show_welcome = false;
        studio.persona = crate::tools::Persona::Pixel;
        assert!(studio.selection.is_empty());
        let chord = Modifiers::CTRL | Modifiers::COMMAND;
        frame(
            &ctx,
            &mut studio,
            keys::key(Key::N, chord | Modifiers::SHIFT),
        );
        let layer = studio.raster_target().unwrap();
        let pixels = studio.doc.layers[layer].kind.pixels().unwrap().data.clone();
        frame(&ctx, &mut studio, keys::key(Key::M, Modifiers::SHIFT));
        frame(&ctx, &mut studio, keys::key(Key::F9, Modifiers::NONE));
        frame(&ctx, &mut studio, vec![]);
        keys::replace("8,8; 24,24", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, keys::key(Key::Enter, chord));
        frame(&ctx, &mut studio, vec![]);
        assert!(studio.pixel_sel.is_some());
        let before = studio.pixel_sel.clone().unwrap();
        keys::tab_to(&ctx, "Select pixels", |events| {
            frame(&ctx, &mut studio, events)
        });
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        keys::tab_to(&ctx, "Grow…", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        settle(&ctx, &mut studio);
        assert!(is_open(&ctx));
        let output = keys::tab_to(&ctx, "Selection radius (px)", |events| {
            frame(&ctx, &mut studio, events)
        });
        keys::focused(&ctx, &output, "Selection radius (px)");
        let radius_id = keys::named(&output, "Selection radius (px)").0;
        let output = keys::replace("letters", |events| frame(&ctx, &mut studio, events));
        keys::associated_error(&output, "Selection radius (px)");
        assert!(keys::named(&output, "Apply").1.is_disabled());
        assert_eq!(studio.pixel_sel.as_ref().unwrap(), &before);
        keys::replace("2", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        settle(&ctx, &mut studio);
        studio.updates.freezing = true;
        let output = frame(&ctx, &mut studio, vec![]);
        assert!(
            keys::named(&output, "Selection radius (px)")
                .1
                .is_disabled()
        );
        assert!(keys::named(&output, "Apply").1.is_disabled());
        assert_eq!(studio.pixel_sel.as_ref().unwrap(), &before);
        studio.updates.freezing = false;
        let output = frame(&ctx, &mut studio, vec![]);
        assert_eq!(keys::named(&output, "Selection radius (px)").0, radius_id);
        keys::tab_to(&ctx, "Apply", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        assert!(!is_open(&ctx));
        let coverage = studio.pixel_sel.as_ref().unwrap();
        assert!(
            coverage.iter().filter(|value| **value > 0.).count()
                > before.iter().filter(|value| **value > 0.).count()
        );
        assert_eq!(studio.doc.layers[layer].kind.pixels().unwrap().data, pixels);
    }
}
