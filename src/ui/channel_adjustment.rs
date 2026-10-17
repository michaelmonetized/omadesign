//! Channel Curves previews are independent of the document and commit through one captured owner.
use super::{
    fields,
    icons::{self, ph},
    jobs, keyboard_controls, theme, welcome,
};
use crate::{
    app::{Studio, channel_adjustment::Captured},
    channels::{Plane, curve::Curve, filter::Filter},
};
use eframe::egui::{self, Id, Key, Ui};
const JOB: &str = "channel-curves-job";
fn id() -> Id {
    Id::new("channel-curves-dialog")
}
#[derive(Clone)]
struct Session {
    source: Captured,
    scope: Id,
    curve: Curve,
    filter: Option<Filter>,
    revision: u64,
    rendered: Option<u64>,
    preview: Option<Plane>,
    selected: usize,
    point_ids: Vec<u64>,
    next_point: u64,
    last_draw: Option<[f32; 2]>,
    histogram: [u32; 256],
    original: bool,
    applying: bool,
    error: Option<String>,
    valid: bool,
}
struct Rendered {
    revision: u64,
    plane: Plane,
}
/// Report whether an actual channel adjustment owns editor input.
/// Args: UI context. Returns true until Apply, Cancel or source retirement closes it.
pub(super) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<Session>(id()).is_some())
}
/// Open Curves from the concrete alpha target without modifying its source plane.
/// Args: context and editor. Returns nothing; unavailable owners report their actual error.
pub(super) fn open(ctx: &egui::Context, studio: &mut Studio) {
    open_adjustment(ctx, studio, None);
}
/// Open a scalar filter with the existing captured worker, preview and commit path.
/// Args: context, editor and authored settings. Returns nothing; unavailable targets report their actual error.
pub(super) fn open_filter(ctx: &egui::Context, studio: &mut Studio, filter: Filter) {
    if let Err(error) = filter.validate() {
        studio.status = error;
        return;
    }
    open_adjustment(ctx, studio, Some(filter));
}
fn open_adjustment(ctx: &egui::Context, studio: &mut Studio, filter: Option<Filter>) {
    if is_open(ctx) {
        return;
    }
    match Captured::new(studio) {
        Ok(source) => {
            let mut histogram = [0; 256];
            let step = source.source.samples.len().div_ceil(65536).max(1);
            for value in source.source.samples.iter().step_by(step) {
                histogram[(*value * 255.).round() as usize] += 1;
            }
            let mut session = Session {
                source,
                scope: keyboard_controls::draft_id(ctx, "channel-curves"),
                curve: Curve::default(),
                filter,
                revision: 0,
                rendered: None,
                preview: None,
                selected: 0,
                point_ids: vec![0, 1],
                next_point: 2,
                last_draw: None,
                histogram,
                original: false,
                applying: false,
                error: None,
                valid: true,
            };
            launch(ctx, &mut session, false);
            ctx.data_mut(|d| d.insert_temp(id(), session));
        }
        Err(error) => studio.status = error,
    }
}
fn launch(ctx: &egui::Context, session: &mut Session, apply: bool) {
    jobs::cancel::<Rendered>(ctx, JOB);
    let source = session.source.clone();
    let curve = session.curve.clone();
    let filter = session.filter.clone();
    let revision = session.revision;
    session.applying = apply;
    jobs::start_with_progress(ctx, JOB, 1, move |progress| {
        let cancel = || {
            if progress.cancelled() {
                Err("Channel adjustment cancelled".into())
            } else {
                Ok(())
            }
        };
        let edge = if apply { None } else { Some(512) };
        let result = if let Some(filter) = filter {
            source.render_filter(&filter, edge, &cancel)
        } else {
            source.render(&curve, edge, &cancel)
        };
        result.map(|plane| Rendered { revision, plane })
    });
}
/// Collect the real worker before drawing the canvas and retire stale sources.
/// Args: context and editor. Returns nothing; a late or cancelled result cannot mutate a new owner.
pub(super) fn tick(ctx: &egui::Context, studio: &mut Studio) {
    let Some(mut session) = ctx.data_mut(|d| {
        let session = d.get_temp::<Session>(id());
        d.remove::<Session>(id());
        session
    }) else {
        return;
    };
    if !session.source.current(studio) {
        jobs::cancel::<Rendered>(ctx, JOB);
        studio.status = "Channel adjustment cancelled because its source changed".into();
        return;
    }
    if let Some(result) = jobs::poll::<Rendered>(ctx, JOB) {
        match result {
            Ok(rendered) if rendered.revision == session.revision => {
                if session.applying {
                    match if session.filter.is_some() {
                        session.source.commit_named(
                            studio,
                            rendered.plane,
                            "Channel filter applied",
                        )
                    } else {
                        session.source.commit(studio, rendered.plane)
                    } {
                        Ok(_) => return,
                        Err(error) => {
                            session.error = Some(error);
                            session.applying = false;
                        }
                    }
                } else {
                    session.preview = Some(rendered.plane);
                    session.rendered = Some(rendered.revision);
                    session.error = None;
                }
            }
            Ok(_) => {}
            Err(error) => {
                session.error = Some(error);
                session.applying = false;
            }
        }
    }
    ctx.data_mut(|d| d.insert_temp(id(), session));
    if jobs::is_running::<Rendered>(ctx, JOB) {
        ctx.request_repaint_after(std::time::Duration::from_millis(30));
    }
}
/// Borrow only the preview belonging to the displayed alpha source.
/// Args: context and editor. Returns its immutable proxy and display identity, never a document edit.
pub(super) fn preview(ctx: &egui::Context, studio: &Studio) -> Option<(Plane, Id)> {
    ctx.data(|d| d.get_temp::<Session>(id()))
        .filter(|s| !s.original && s.source.current(studio))
        .and_then(|s| Some((s.preview?, s.scope.with(s.rendered?))))
}
fn button(ui: &mut Ui, scope: Id, glyph: &str, name: &str, enabled: bool) -> bool {
    let response = fields::native_scope(ui, scope, |ui| {
        ui.add_enabled(
            enabled,
            egui::Button::new(egui::RichText::new(glyph).font(icons::font(17.))).frame(false),
        )
    })
    .inner;
    welcome::name_control(ui, &response, scope.with("name"), name);
    response.on_hover_text(name).clicked()
}
fn changed(session: &mut Session, ctx: &egui::Context) {
    session.revision = session.revision.wrapping_add(1);
    session.rendered = None;
    match session
        .filter
        .as_ref()
        .map_or_else(|| session.curve.prepare().map(|_| ()), Filter::validate)
    {
        Ok(_) => {
            session.error = None;
            launch(ctx, session, false);
        }
        Err(error) => {
            jobs::cancel::<Rendered>(ctx, JOB);
            session.error = Some(error);
        }
    }
}
fn insert_point(session: &mut Session, point: [f32; 2]) -> bool {
    let Curve::Points(points) = &mut session.curve else {
        return false;
    };
    if points.len() >= 19 {
        session.error = Some("The curve already has 19 points".into());
        return false;
    }
    let at = points.partition_point(|p| p[0] < point[0]);
    if points.get(at).is_some_and(|p| p[0] == point[0]) {
        session.selected = at;
        return false;
    }
    points.insert(at, point);
    session.point_ids.insert(at, session.next_point);
    session.next_point += 1;
    session.selected = at;
    true
}
fn remove_point(session: &mut Session) -> bool {
    let Curve::Points(points) = &mut session.curve else {
        return false;
    };
    if points.len() <= 2 {
        return false;
    }
    points.remove(session.selected);
    session.point_ids.remove(session.selected);
    session.selected = session.selected.min(points.len() - 1);
    true
}
fn move_point(session: &mut Session, point: [f32; 2]) -> bool {
    let Curve::Points(points) = &mut session.curve else {
        return false;
    };
    let i = session.selected.min(points.len() - 1);
    let min = if i == 0 {
        0.
    } else {
        points[i - 1][0].next_up()
    };
    let max = if i + 1 == points.len() {
        1.
    } else {
        points[i + 1][0].next_down()
    };
    let point = [
        if min <= max {
            point[0].clamp(min, max)
        } else {
            points[i][0]
        },
        point[1].clamp(0., 1.),
    ];
    let different = points[i] != point;
    points[i] = point;
    different
}
fn graph(ui: &mut Ui, session: &mut Session) -> bool {
    let side = ui
        .available_width()
        .min(360.)
        .min((ui.ctx().viewport_rect().height() - 290.).max(120.));
    let response = fields::native_scope(ui, session.scope.with("graph"), |ui| {
        ui.allocate_response(egui::vec2(side, side), egui::Sense::click_and_drag())
    })
    .inner;
    let rect = response.rect;
    let screen = |p: [f32; 2]| {
        egui::pos2(
            rect.left() + p[0] * rect.width(),
            rect.bottom() - p[1] * rect.height(),
        )
    };
    let at = |p: egui::Pos2| {
        [
            ((p.x - rect.left()) / rect.width()).clamp(0., 1.),
            ((rect.bottom() - p.y) / rect.height()).clamp(0., 1.),
        ]
    };
    welcome::name_control(
        ui,
        &response,
        session.scope.with("graph-label"),
        "Channel curve graph",
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Other,
            response.enabled(),
            "Channel curve graph",
        )
    });
    ui.ctx().accesskit_node_builder(response.id,|node|{node.set_description("Tab enters the curve. Arrows move its selected point; Page Up and Page Down select points; Insert adds and Delete removes a point.");});
    let mut edited = false;
    if response.clicked() || response.drag_started() {
        response.request_focus();
    }
    if let Some(pointer) = response.interact_pointer_pos() {
        let point = at(pointer);
        if matches!(session.curve, Curve::Drawn(_)) {
            if response.is_pointer_button_down_on() {
                let from = if ui.input(|i| i.modifiers.shift) || response.dragged() {
                    session.last_draw.unwrap_or(point)
                } else {
                    point
                };
                edited |= session.curve.pencil_line(from, point).unwrap_or(false);
                session.last_draw = Some(point);
            }
        } else {
            if response.drag_started() || response.clicked() {
                let nearest = if let Curve::Points(points) = &session.curve {
                    points
                        .iter()
                        .enumerate()
                        .map(|(i, p)| (i, screen(*p).distance(pointer)))
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                } else {
                    None
                };
                if let Some((i, distance)) = nearest.filter(|(_, distance)| *distance <= 10.) {
                    let _ = distance;
                    session.selected = i;
                } else {
                    edited |= insert_point(session, point);
                }
            }
            if response.dragged() {
                edited |= move_point(session, point);
            }
        }
    }
    if response.has_focus() {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                response.id,
                egui::EventFilter {
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    ..Default::default()
                },
            )
        });
        let events = ui.input(|i| i.events.clone());
        for event in events {
            if let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            {
                if modifiers.ctrl || modifiers.command || modifiers.alt {
                    continue;
                }
                let step = if modifiers.shift { 10. } else { 1. } / 255.;
                let point = match &session.curve {
                    Curve::Points(points) => Some(points[session.selected.min(points.len() - 1)]),
                    _ => None,
                };
                match (key, point) {
                    (Key::PageUp, Some(_)) => session.selected = session.selected.saturating_sub(1),
                    (Key::PageDown, Some(_)) => {
                        if let Curve::Points(points) = &session.curve {
                            session.selected = (session.selected + 1).min(points.len() - 1)
                        }
                    }
                    (Key::Delete, Some(_)) => edited |= remove_point(session),
                    (Key::Insert, Some(point)) => {
                        let x = if let Curve::Points(points) = &session.curve {
                            if session.selected + 1 < points.len() {
                                (point[0] + points[session.selected + 1][0]) * 0.5
                            } else {
                                (points[session.selected - 1][0] + point[0]) * 0.5
                            }
                        } else {
                            0.5
                        };
                        let y = session.curve.prepare().map_or(x, |e| e.sample(x));
                        edited |= insert_point(session, [x, y]);
                    }
                    (Key::ArrowLeft, Some(p)) => edited |= move_point(session, [p[0] - step, p[1]]),
                    (Key::ArrowRight, Some(p)) => {
                        edited |= move_point(session, [p[0] + step, p[1]])
                    }
                    (Key::ArrowUp, Some(p)) => edited |= move_point(session, [p[0], p[1] + step]),
                    (Key::ArrowDown, Some(p)) => edited |= move_point(session, [p[0], p[1] - step]),
                    _ => {}
                }
            }
        }
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0., theme::bg_widget());
    let max = session.histogram.iter().copied().max().unwrap_or(1).max(1) as f32;
    for (i, count) in session.histogram.iter().enumerate() {
        let x = rect.left() + i as f32 / 256. * rect.width();
        let height = (*count as f32).sqrt() / max.sqrt() * rect.height();
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(x, rect.bottom() - height),
                egui::pos2(x + rect.width() / 256. + 1., rect.bottom()),
            ),
            0.,
            theme::fg_weak().gamma_multiply(0.25),
        );
    }
    for i in 1..4 {
        let f = i as f32 / 4.;
        painter.line_segment(
            [screen([f, 0.]), screen([f, 1.])],
            egui::Stroke::new(1., theme::border()),
        );
        painter.line_segment(
            [screen([0., f]), screen([1., f])],
            egui::Stroke::new(1., theme::border()),
        );
    }
    painter.line_segment(
        [screen([0., 0.]), screen([1., 1.])],
        egui::Stroke::new(1., theme::fg_weak()),
    );
    if let Ok(eval) = session.curve.prepare() {
        painter.add(egui::Shape::line(
            (0..=256)
                .map(|i| {
                    let x = i as f32 / 256.;
                    screen([x, eval.sample(x)])
                })
                .collect(),
            egui::Stroke::new(2., theme::accent()),
        ));
    }
    if let Curve::Points(points) = &session.curve {
        for (i, p) in points.iter().enumerate() {
            painter.circle_filled(
                screen(*p),
                if i == session.selected { 5. } else { 3.5 },
                if i == session.selected {
                    theme::accent()
                } else {
                    theme::fg()
                },
            );
        }
    }
    painter.rect_stroke(
        rect,
        0.,
        egui::Stroke::new(
            if response.has_focus() { 2. } else { 1. },
            if response.has_focus() {
                theme::accent()
            } else {
                theme::border()
            },
        ),
        egui::StrokeKind::Inside,
    );
    welcome::focus_control(ui, &response);
    edited
}
fn filter_controls(ui: &mut Ui, session: &mut Session) -> bool {
    let mut strength = session.filter.as_ref().unwrap().strength();
    let mut filter = session.filter.as_ref().unwrap().base().clone();
    let label = ui.label("Channel filter");
    let mut response = fields::native_scope(ui, session.scope.with("filter-choice"), |ui| {
        egui::ComboBox::from_id_salt("channel-filter")
            .selected_text(filter.name())
            .show_ui(ui, |ui| {
                for choice in Filter::choices() {
                    let selected = filter.name() == choice.name();
                    let response = ui.selectable_label(selected, choice.name());
                    welcome::name_control(
                        ui,
                        &response,
                        session.scope.with(choice.name()),
                        choice.name(),
                    );
                    if response.clicked() {
                        if !selected {
                            filter = choice;
                        }
                        ui.close();
                    }
                }
            })
            .response
    })
    .inner
    .labelled_by(label.id);
    welcome::name_control(
        ui,
        &response,
        session.scope.with("filter-label"),
        "Channel filter",
    );
    let choices = Filter::choices();
    let options: Vec<_> = choices
        .iter()
        .enumerate()
        .map(|(i, f)| (i, f.name().to_string()))
        .collect();
    let mut selected = choices
        .iter()
        .position(|f| f.name() == filter.name())
        .unwrap();
    let before = selected;
    keyboard_controls::selection_keys(ui, &mut response, "Channel filter", &mut selected, &options);
    if selected != before {
        filter = choices[selected].clone();
    }
    if let Filter::Raster { settings, .. } = &mut filter {
        if settings.kind == crate::raster::Kind::ChromaKey {
            ui.horizontal(|ui| {
                ui.label("Key color");
                super::color_picker::color_edit(
                    ui,
                    session.scope.with("key-color"),
                    &mut settings.color,
                );
            });
        }
    }
    let mut changed = false;
    let mut valid = true;
    let mut responses = Vec::new();
    let mut field = |ui: &mut Ui,
                     key: &str,
                     name: &str,
                     value: &mut f32,
                     range: std::ops::RangeInclusive<f32>,
                     speed: f64,
                     suffix: &str| {
        let (response, accepted) = keyboard_controls::motion_number(
            ui,
            session.scope.with(key),
            (name, name),
            value,
            range,
            speed,
            suffix,
            Some(8),
        );
        changed |= response.changed;
        valid &= accepted;
        responses.push(response.response);
    };
    match &mut filter {
        Filter::GaussianBlur { radius } => {
            field(ui, "radius", "Blur radius", radius, 0. ..=100., 0.1, " px")
        }
        Filter::Levels {
            black,
            white,
            gamma,
            output_black,
            output_white,
        } => {
            field(ui, "black", "Input black", black, 0. ..=1., 0.01, "");
            field(ui, "white", "Input white", white, 0. ..=1., 0.01, "");
            field(ui, "gamma", "Midtone gamma", gamma, 0.01..=10., 0.01, "");
            field(
                ui,
                "output-black",
                "Output black",
                output_black,
                0. ..=1.,
                0.01,
                "",
            );
            field(
                ui,
                "output-white",
                "Output white",
                output_white,
                0. ..=1.,
                0.01,
                "",
            );
        }
        Filter::Threshold { level } => field(
            ui,
            "threshold",
            "Threshold level",
            level,
            0. ..=1.,
            0.01,
            "",
        ),
        Filter::Raster { settings, .. } => {
            for parameter in settings.kind.parameters() {
                let key = format!("{}-{}", settings.kind.name(), parameter.index);
                field(
                    ui,
                    &key,
                    parameter.label,
                    &mut settings.values[parameter.index],
                    parameter.min..=parameter.max,
                    parameter.step,
                    "",
                );
            }
        }
        Filter::Mix { .. } => unreachable!("Strength wrapper is edited separately"),
    }
    field(
        ui,
        "strength",
        "Filter strength",
        &mut strength,
        0. ..=1.,
        0.01,
        "",
    );
    let filter = filter.with_strength(strength);
    changed |= filter != *session.filter.as_ref().unwrap();
    let validation = filter.validate();
    session.valid = valid && validation.is_ok();
    if let Err(error) = validation {
        welcome::control_error(ui, &responses.iter().collect::<Vec<_>>(), &error);
    } else if let Some(error) = session.error.as_ref() {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
    session.filter = Some(filter);
    changed
}
fn controls(ui: &mut Ui, session: &mut Session) -> bool {
    if session.filter.is_some() {
        return filter_controls(ui, session);
    }
    let mut edited = false;
    ui.horizontal(|ui| {
        let drawn = matches!(session.curve, Curve::Drawn(_));
        if button(
            ui,
            session.scope.with("points"),
            ph::BEZIER_CURVE,
            "Point curve",
            drawn,
        ) {
            if let Ok(curve) = session.curve.fitted() {
                session.curve = curve;
                if let Curve::Points(p) = &session.curve {
                    session.point_ids =
                        (session.next_point..session.next_point + p.len() as u64).collect();
                    session.next_point += p.len() as u64;
                    session.selected = 0;
                }
                edited = true;
            }
        }
        if button(
            ui,
            session.scope.with("draw"),
            ph::PENCIL,
            "Draw curve",
            !drawn,
        ) {
            if let Ok(curve) = session.curve.drawn() {
                session.curve = curve;
                edited = true;
            }
        }
        if button(
            ui,
            session.scope.with("smooth"),
            ph::MAGIC_WAND,
            "Smooth drawn curve",
            drawn,
        ) {
            if let Ok(curve) = session.curve.smooth() {
                session.curve = curve;
                edited = true;
            }
        }
        if button(
            ui,
            session.scope.with("reset"),
            ph::ARROW_CLOCKWISE,
            "Reset channel curve",
            true,
        ) {
            session.curve = Curve::default();
            session.point_ids = vec![session.next_point, session.next_point + 1];
            session.next_point += 2;
            session.selected = 0;
            edited = true;
        }
    });
    edited |= graph(ui, session);
    session.valid = true;
    if let Curve::Points(points) = session.curve.clone() {
        let point = points[session.selected.min(points.len() - 1)];
        let key = session.scope.with(session.point_ids[session.selected]);
        let mut x = point[0] * 255.;
        let mut y = point[1] * 255.;
        let fields = ui.columns(2, |ui| {
            let input = keyboard_controls::motion_number(
                &mut ui[0],
                key.with("input"),
                ("Input", "Curve point input"),
                &mut x,
                0.0..=255.0,
                1.,
                "",
                Some(6),
            );
            let output = keyboard_controls::motion_number(
                &mut ui[1],
                key.with("output"),
                ("Output", "Curve point output"),
                &mut y,
                0.0..=255.0,
                1.,
                "",
                Some(6),
            );
            (input, output)
        });
        session.valid = fields.0.1 && fields.1.1 && session.curve.prepare().is_ok();
        if fields.0.0.changed || fields.1.0.changed {
            let mut candidate = session.curve.clone();
            if let Curve::Points(p) = &mut candidate {
                if fields.0.0.changed {
                    p[session.selected][0] = x / 255.;
                }
                if fields.1.0.changed {
                    p[session.selected][1] = y / 255.;
                }
            }
            session.curve = candidate;
            edited = true;
        }
        ui.horizontal(|ui| {
            if button(
                ui,
                key.with("add"),
                ph::PLUS,
                "Add curve point",
                points.len() < 19,
            ) {
                let x = if session.selected + 1 < points.len() {
                    (point[0] + points[session.selected + 1][0]) * 0.5
                } else {
                    (points[session.selected - 1][0] + point[0]) * 0.5
                };
                let y = session.curve.prepare().map_or(x, |c| c.sample(x));
                edited |= insert_point(session, [x, y]);
            }
            if button(
                ui,
                key.with("remove"),
                ph::MINUS,
                "Remove curve point",
                points.len() > 2,
            ) {
                edited |= remove_point(session);
            }
        });
        if let Err(error) = session.curve.prepare() {
            welcome::control_error(ui, &[&fields.0.0.response, &fields.1.0.response], &error);
        }
        if let Some(error) = session
            .error
            .as_ref()
            .filter(|_| session.curve.prepare().is_ok())
        {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
    } else if let Some(error) = session.error.as_ref() {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
    edited
}
/// Draw native Curves controls and commit only a completed full-resolution Apply.
/// Args: context and current editor. Returns nothing; cancel and preview never add history.
pub(super) fn show(ctx: &egui::Context, studio: &mut Studio) {
    let Some(mut session) = ctx.data_mut(|d| {
        let session = d.get_temp::<Session>(id());
        d.remove::<Session>(id());
        session
    }) else {
        return;
    };
    if !session.source.current(studio) {
        jobs::cancel::<Rendered>(ctx, JOB);
        return;
    }
    let mut apply = false;
    let mut cancel = false;
    let response = egui::Modal::new(session.scope.with("modal"))
        .backdrop_color(theme::bg_window().gamma_multiply(0.65))
        .show(ctx, |ui| {
            ui.set_width(380.0_f32.min((ctx.viewport_rect().width() - 40.).max(180.)));
            ui.label(
                egui::RichText::new(format!(
                    "{} · {}",
                    session.filter.as_ref().map_or("Curves", Filter::name),
                    session.source.name
                ))
                .strong(),
            );
            let edited = ui
                .add_enabled_ui(!session.applying, |ui| controls(ui, &mut session))
                .inner;
            if edited {
                changed(&mut session, ctx);
            }
            let ready = session.rendered == Some(session.revision) && session.error.is_none();
            ui.horizontal(|ui| {
                ui.checkbox(&mut session.original, "Original");
                if !ready && !session.applying && session.error.is_none() && session.valid {
                    ui.spinner();
                }
                apply = ui
                    .add_enabled(
                        ready && session.valid && !session.applying,
                        egui::Button::new("Apply"),
                    )
                    .clicked();
                cancel = ui.button("Cancel").clicked();
            });
            if session.applying {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Applying…");
                });
            }
        });
    if cancel || response.should_close() {
        jobs::cancel::<Rendered>(ctx, JOB);
        return;
    }
    if apply {
        launch(ctx, &mut session, true);
    }
    ctx.data_mut(|d| d.insert_temp(id(), session));
}

#[cfg(test)]
mod tests {
    use super::super::welcome::keyboard_tests as keys;
    use super::*;
    use egui::{Event, Modifiers};
    fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<Event>) -> egui::FullOutput {
        tick(ctx, studio);
        keys::render(ctx, events, |ui| {
            ui.set_max_width(288.);
            super::super::channels::show(ui, studio);
            show(ui.ctx(), studio);
        })
    }
    fn invoke(ctx: &egui::Context, studio: &mut Studio, name: &str) {
        keys::tab_to(ctx, name, |events| frame(ctx, studio, events));
        frame(ctx, studio, keys::key(Key::Space, Modifiers::NONE));
        frame(ctx, studio, vec![]);
    }
    fn wait(ctx: &egui::Context, studio: &mut Studio) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while jobs::is_running::<Rendered>(ctx, JOB) {
            frame(ctx, studio, vec![]);
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        frame(ctx, studio, vec![]);
    }
    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        super::super::theme::apply(&ctx);
        ctx
    }
    #[test]
    fn craft_channel_filter_families_native_catalog_visits_every_kind_and_applies_last() {
        let ctx = context();
        let mut studio = crate::app::channel_adjustment::tests::fixture();
        let before = crate::project::encode(&studio.doc).unwrap();
        let history = studio.history.len();
        open_filter(&ctx, &mut studio, Filter::GaussianBlur { radius: 4. });
        wait(&ctx, &mut studio);
        keys::tab_to(&ctx, "Channel filter", |events| {
            frame(&ctx, &mut studio, events)
        });
        for (index, choice) in Filter::choices().into_iter().enumerate() {
            if index > 0 {
                frame(
                    &ctx,
                    &mut studio,
                    keys::key(Key::ArrowDown, Modifiers::NONE),
                );
                wait(&ctx, &mut studio);
            }
            let session = ctx.data(|d| d.get_temp::<Session>(id())).unwrap();
            assert_eq!(session.filter.as_ref().unwrap().name(), choice.name());
            assert!(session.valid, "{}", choice.name());
            assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
            assert_eq!(studio.history.len(), history);
        }
        keys::tab_to(&ctx, "Amplitude (px)", |events| {
            frame(&ctx, &mut studio, events)
        });
        keys::replace("4.25", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        wait(&ctx, &mut studio);
        invoke(&ctx, &mut studio, "Apply");
        wait(&ctx, &mut studio);
        assert!(!is_open(&ctx));
        assert_eq!(studio.history.len(), history + 1);
        let after = crate::project::encode(&studio.doc).unwrap();
        assert_ne!(after, before);
        assert_eq!(
            crate::project::encode(&crate::project::decode(&after).unwrap()).unwrap(),
            after
        );
        studio.undo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
        studio.redo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), after);
    }
    #[test]
    fn craft_channel_filter_families_native_strength_is_stable_and_blends_once() {
        let ctx = context();
        let mut studio = crate::app::channel_adjustment::tests::fixture();
        let before = crate::project::encode(&studio.doc).unwrap();
        let original = studio.doc.channels.last().unwrap().plane.clone();
        let history = studio.history.len();
        super::super::raster::open(&ctx, &mut studio, crate::raster::Kind::Invert);
        wait(&ctx, &mut studio);
        keys::tab_to(&ctx, "Filter strength", |events| {
            frame(&ctx, &mut studio, events)
        });
        keys::replace("0.375", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        wait(&ctx, &mut studio);
        let session = ctx.data(|d| d.get_temp::<Session>(id())).unwrap();
        assert_eq!(session.filter.as_ref().unwrap().strength(), 0.375);
        let revision = session.revision;
        for _ in 0..3 {
            frame(&ctx, &mut studio, vec![]);
        }
        assert_eq!(
            ctx.data(|d| d.get_temp::<Session>(id())).unwrap().revision,
            revision
        );
        assert!(!jobs::is_running::<Rendered>(&ctx, JOB));
        let output = preview(&ctx, &studio).unwrap().0;
        for (input, actual) in original.samples.iter().zip(output.samples.iter()) {
            assert_eq!(
                actual.to_bits(),
                (input + ((1. - input) - input) * 0.375).to_bits()
            );
        }
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
        invoke(&ctx, &mut studio, "Apply");
        wait(&ctx, &mut studio);
        assert_eq!(studio.history.len(), history + 1);
        studio.undo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    }
    #[test]
    fn craft_channel_filters_native_numeric_preview_apply_undo_and_reopen() {
        let ctx = context();
        let mut studio = crate::app::channel_adjustment::tests::fixture();
        let before = crate::project::encode(&studio.doc).unwrap();
        let history = studio.history.len();
        frame(&ctx, &mut studio, vec![]);
        frame(&ctx, &mut studio, vec![]);
        invoke(&ctx, &mut studio, "Adjust channel filter");
        wait(&ctx, &mut studio);
        keys::tab_to(&ctx, "Blur radius", |events| {
            frame(&ctx, &mut studio, events)
        });
        keys::replace("2.125", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        wait(&ctx, &mut studio);
        let session = ctx.data(|d| d.get_temp::<Session>(id())).unwrap();
        assert_eq!(session.filter, Some(Filter::GaussianBlur { radius: 2.125 }));
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
        assert_eq!(studio.history.len(), history);
        let expected = preview(&ctx, &studio).unwrap().0;
        invoke(&ctx, &mut studio, "Original");
        assert!(preview(&ctx, &studio).is_none());
        invoke(&ctx, &mut studio, "Original");
        invoke(&ctx, &mut studio, "Apply");
        wait(&ctx, &mut studio);
        assert!(!is_open(&ctx));
        assert_eq!(studio.history.len(), history + 1);
        assert_eq!(
            studio.doc.channels.last().unwrap().plane.samples,
            expected.samples
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
    }
    #[test]
    fn craft_channel_filters_native_kind_invalid_cancel_and_stale_worker() {
        let ctx = context();
        let mut studio = crate::app::channel_adjustment::tests::fixture();
        let before = crate::project::encode(&studio.doc).unwrap();
        open_filter(&ctx, &mut studio, Filter::GaussianBlur { radius: 4. });
        wait(&ctx, &mut studio);
        keys::tab_to(&ctx, "Channel filter", |events| {
            frame(&ctx, &mut studio, events)
        });
        frame(
            &ctx,
            &mut studio,
            keys::key(Key::ArrowDown, Modifiers::NONE),
        );
        wait(&ctx, &mut studio);
        assert!(matches!(
            ctx.data(|d| d.get_temp::<Session>(id())).unwrap().filter,
            Some(Filter::Levels { .. })
        ));
        keys::tab_to(&ctx, "Input black", |events| {
            frame(&ctx, &mut studio, events)
        });
        keys::replace("1", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        let session = ctx.data(|d| d.get_temp::<Session>(id())).unwrap();
        assert!(!session.valid);
        assert!(session.filter.unwrap().validate().is_err());
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
        invoke(&ctx, &mut studio, "Cancel");
        assert!(!is_open(&ctx));
        super::super::raster::open(&ctx, &mut studio, crate::raster::Kind::Threshold);
        wait(&ctx, &mut studio);
        assert!(matches!(
            ctx.data(|d| d.get_temp::<Session>(id())).unwrap().filter,
            Some(Filter::Threshold { .. })
        ));
        studio.pixel_sel_gen += 1;
        frame(&ctx, &mut studio, vec![]);
        assert!(!is_open(&ctx));
        assert!(preview(&ctx, &studio).is_none());
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    }
    #[test]
    fn craft_channel_curves_native_graph_numeric_preview_apply_undo_and_reopen() {
        let ctx = context();
        let mut studio = crate::app::channel_adjustment::tests::fixture();
        let before = crate::project::encode(&studio.doc).unwrap();
        let history = studio.history.len();
        frame(&ctx, &mut studio, vec![]);
        frame(&ctx, &mut studio, vec![]);
        invoke(&ctx, &mut studio, "Adjust channel curves");
        wait(&ctx, &mut studio);
        assert!(is_open(&ctx));
        keys::tab_to(&ctx, "Channel curve graph", |events| {
            frame(&ctx, &mut studio, events)
        });
        frame(&ctx, &mut studio, keys::key(Key::Insert, Modifiers::NONE));
        frame(&ctx, &mut studio, keys::key(Key::ArrowUp, Modifiers::SHIFT));
        keys::tab_to(&ctx, "Curve point output", |events| {
            frame(&ctx, &mut studio, events)
        });
        keys::replace("210.125", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        wait(&ctx, &mut studio);
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
        assert_eq!(studio.history.len(), history);
        let live = preview(&ctx, &studio).unwrap().0;
        assert!((live.samples[16] - 210.125 / 255.).abs() < 1e-7);
        invoke(&ctx, &mut studio, "Original");
        assert!(preview(&ctx, &studio).is_none());
        invoke(&ctx, &mut studio, "Original");
        invoke(&ctx, &mut studio, "Apply");
        wait(&ctx, &mut studio);
        assert!(!is_open(&ctx));
        assert_eq!(studio.history.len(), history + 1);
        let saved = crate::project::encode(&studio.doc).unwrap();
        assert_eq!(
            crate::project::encode(&crate::project::decode(&saved).unwrap()).unwrap(),
            saved
        );
        studio.undo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
        studio.redo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), saved);
    }
    #[test]
    fn craft_channel_curves_native_invalid_point_cancel_and_stale_worker_keep_document() {
        let ctx = context();
        let mut studio = crate::app::channel_adjustment::tests::fixture();
        let before = crate::project::encode(&studio.doc).unwrap();
        open(&ctx, &mut studio);
        wait(&ctx, &mut studio);
        keys::tab_to(&ctx, "Curve point input", |events| {
            frame(&ctx, &mut studio, events)
        });
        keys::replace("255", |events| frame(&ctx, &mut studio, events));
        frame(&ctx, &mut studio, keys::key(Key::Enter, Modifiers::NONE));
        let session = ctx.data(|d| d.get_temp::<Session>(id())).unwrap();
        assert!(session.error.as_ref().unwrap().contains("increasing order"));
        assert!(session.curve.prepare().is_err());
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
        invoke(&ctx, &mut studio, "Cancel");
        assert!(!is_open(&ctx));
        open(&ctx, &mut studio);
        studio.pixel_sel_gen += 1;
        frame(&ctx, &mut studio, vec![]);
        assert!(!is_open(&ctx));
        assert!(preview(&ctx, &studio).is_none());
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
        jobs::cancel::<Rendered>(&ctx, JOB);
    }
}
