//! Native channel controls call the same admitted plane and ordinary history routes.
use super::{
    fields,
    icons::{self, ph},
    jobs, welcome,
};
use crate::{
    app::{
        Studio,
        channels::{self as actions, Combine},
    },
    channels::{Plane, Target},
    resources::CancellationCheck,
};
use eframe::egui::{self, Id, Ui};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
const JOB: &str = "channel-operation";
const PREVIEW: &str = "channel-scalar-preview";
fn owner(studio: &Studio) -> Id {
    Id::new(("channels", &studio.swap_id))
}
fn run_id() -> Id {
    Id::new("channel-operation-owner")
}
#[derive(Clone)]
struct State {
    selected: Target,
    combine: Combine,
    name: String,
    original: String,
    error: String,
    name_error: String,
    textures: HashMap<u64, (usize, egui::TextureHandle)>,
    preview: Option<(u64, Target, egui::TextureHandle)>,
    failed: Option<(u64, Target)>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            selected: Target::Composite,
            combine: Combine::Replace,
            name: String::new(),
            original: String::new(),
            error: String::new(),
            name_error: String::new(),
            textures: HashMap::new(),
            preview: None,
            failed: None,
        }
    }
}
#[derive(Clone)]
struct Run {
    document: String,
    generation: u64,
    selection: u64,
    valid: Arc<AtomicBool>,
}
enum Change {
    Save(Plane),
    Invert(u64, Plane),
    Selection(
        Arc<crate::resources::Allocation<u8>>,
        crate::app::masking::SelectionSpace,
    ),
}
struct RenderedChannel {
    document: String,
    generation: u64,
    target: Target,
    result: Result<Plane, String>,
}
struct Finished {
    target: actions::Target,
    change: Change,
}
enum Work {
    Save,
    Invert(u64),
    Load(Target, Combine),
}
fn start(ctx: &egui::Context, studio: &Studio, work: Work) -> Result<(), String> {
    if ctx.data(|d| d.get_temp::<Run>(run_id())).is_some() {
        return Err("Wait for the current channel operation".into());
    }
    let target = studio.capture_channels()?;
    let document = studio.doc.clone();
    let selection = studio.pixel_sel.clone().zip(studio.pixel_selection_space());
    if matches!(work, Work::Save) && selection.is_none() {
        return Err("Make a pixel selection to save an alpha channel".into());
    }
    let valid = Arc::new(AtomicBool::new(true));
    ctx.data_mut(|d| {
        d.insert_temp(
            run_id(),
            Run {
                document: studio.swap_id.clone(),
                generation: studio.canvas_gen,
                selection: studio.pixel_sel_gen,
                valid: valid.clone(),
            },
        )
    });
    jobs::start_with_progress(ctx, JOB, 1, move |progress| {
        let check = || {
            if !valid.load(Ordering::Relaxed) || progress.cancelled() {
                Err("Channel operation cancelled".into())
            } else {
                Ok(())
            }
        };
        check.check()?;
        let change = match work {
            Work::Save => {
                let (values, space) = selection.unwrap();
                Change::Save(Plane::from_mask(
                    &values,
                    space.w,
                    space.h,
                    space.transform,
                    &check,
                )?)
            }
            Work::Invert(id) => {
                let channel = document
                    .channels
                    .iter()
                    .find(|c| c.id == id)
                    .ok_or("That channel no longer exists")?;
                Change::Invert(id, channel.plane.inverted(&check)?)
            }
            Work::Load(channel, operation) => {
                let plane = crate::channels::render(&document, channel, &check)?;
                let (values, space) = actions::selection(&plane, selection, operation, &check)?;
                Change::Selection(values, space)
            }
        };
        check.check()?;
        Ok(Finished { target, change })
    });
    Ok(())
}
/// Retire stale owners and collect the existing worker even while Channels is hidden.
/// Args: context and displayed editor. Returns nothing; late results cannot modify another document.
pub(super) fn poll(ctx: &egui::Context, studio: &mut Studio) {
    if let Some(Ok(preview)) = jobs::poll::<RenderedChannel>(ctx, PREVIEW) {
        if preview.document == studio.swap_id && preview.generation == studio.canvas_gen {
            let key = owner(studio);
            let mut state = ctx.data(|d| d.get_temp::<State>(key)).unwrap_or_default();
            if state.selected == preview.target {
                match preview.result {
                    Ok(plane) => {
                        let texture = plane_texture(ctx, &plane, 128, "selected-channel");
                        state.preview = Some((preview.generation, preview.target, texture));
                        state.failed = None;
                    }
                    Err(error) => {
                        state.failed = Some((preview.generation, preview.target));
                        state.error = error;
                    }
                }
                ctx.data_mut(|d| d.insert_temp(key, state));
            }
        }
    }
    let Some(run) = ctx.data(|d| d.get_temp::<Run>(run_id())) else {
        return;
    };
    if run.document != studio.swap_id
        || run.generation != studio.canvas_gen
        || run.selection != studio.pixel_sel_gen
    {
        run.valid.store(false, Ordering::Relaxed);
    }
    let Some(result) = jobs::poll::<Finished>(ctx, JOB) else {
        ctx.request_repaint_after(std::time::Duration::from_millis(40));
        return;
    };
    ctx.data_mut(|d| d.remove::<Run>(run_id()));
    if run.document != studio.swap_id {
        return;
    }
    let key = owner(studio);
    let mut state = ctx.data(|d| d.get_temp::<State>(key)).unwrap_or_default();
    let result = result.and_then(|finished| match finished.change {
        Change::Save(plane) => studio
            .save_channel(&finished.target, plane, "")
            .and_then(|id| {
                studio.select_channel_target(Some(id))?;
                state.selected = Target::Saved(id);
                Ok(())
            }),
        Change::Invert(id, plane) => {
            let mut after = studio.doc.channels.clone();
            let channel = after
                .iter_mut()
                .find(|c| c.id == id)
                .ok_or("That channel no longer exists")?;
            channel.plane = plane;
            studio
                .set_channels(&finished.target, after, "Channel inverted")
                .map(|_| ())
        }
        Change::Selection(values, space) => {
            studio.load_channel_selection(&finished.target, values, space)
        }
    });
    if let Err(error) = result {
        studio.status = format!("Channels: {error}");
        state.error = error;
    } else {
        state.error.clear();
    }
    ctx.data_mut(|d| d.insert_temp(key, state));
}
fn icon(ui: &mut Ui, id: Id, glyph: &str, name: &str, enabled: bool) -> bool {
    let response = fields::native_scope(ui, id, |ui| {
        ui.add_enabled(
            enabled,
            egui::Button::new(egui::RichText::new(glyph).font(icons::font(16.))).frame(false),
        )
    })
    .inner;
    welcome::name_control(ui, &response, id.with("label"), name);
    response.on_hover_text(name).clicked()
}
fn thumbnail(
    ui: &Ui,
    state: &mut State,
    channel: &crate::channels::Channel,
) -> egui::TextureHandle {
    let pointer = Arc::as_ptr(&channel.plane.samples) as usize;
    if let Some((old, texture)) = state.textures.get(&channel.id)
        && *old == pointer
    {
        return texture.clone();
    }
    let texture = plane_texture(
        ui.ctx(),
        &channel.plane,
        40,
        &format!("channel-{}", channel.id),
    );
    state
        .textures
        .insert(channel.id, (pointer, texture.clone()));
    texture
}
fn plane_texture(ctx: &egui::Context, plane: &Plane, edge: u32, name: &str) -> egui::TextureHandle {
    let scale = f64::from(edge.max(1)) / f64::from(plane.width.max(plane.height).max(1));
    let scale = scale.min(1.);
    let w = (f64::from(plane.width) * scale).round().max(1.) as usize;
    let h = (f64::from(plane.height) * scale).round().max(1.) as usize;
    let mut pixels = vec![0; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let sx = x * plane.width as usize / w;
            let sy = y * plane.height as usize / h;
            let v = (plane
                .samples
                .get(sy * plane.width as usize + sx)
                .copied()
                .unwrap_or(0.)
                * 255.)
                .round() as u8;
            pixels[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&[v, v, v, 255]);
        }
    }
    ctx.load_texture(
        name,
        egui::ColorImage::from_rgba_unmultiplied([w, h], &pixels),
        egui::TextureOptions::LINEAR,
    )
}

/// Draw concrete channel rows and real selection/history actions.
/// Args: native pane and current editor. Returns nothing; previews are never used as selection data.
pub(super) fn show(ui: &mut Ui, studio: &mut Studio) {
    let key = owner(studio);
    let mut state = ui.data(|d| d.get_temp::<State>(key)).unwrap_or_default();
    if let Some(channel) = studio.channel_paint_target() {
        state.selected = Target::Saved(channel.id);
    } else if matches!(state.selected, Target::Saved(_)) {
        state.selected = Target::Composite;
    }
    let mut select_target = None;
    if let Target::Saved(id) = state.selected
        && !studio.doc.channels.iter().any(|c| c.id == id)
    {
        state.selected = Target::Composite;
        state.name.clear();
        state.original.clear();
    }
    state
        .textures
        .retain(|id, _| studio.doc.channels.iter().any(|c| c.id == *id));
    let composite = super::pane_previews::image(ui, studio, None, None);
    let cmyk = studio.doc.color.model == crate::deep_color::cms::Model::Cmyk;
    if let Target::Color(index) = state.selected
        && index >= if cmyk { 4 } else { 3 }
    {
        state.selected = Target::Composite;
    }
    for target in std::iter::once(Target::Composite)
        .chain((0..if cmyk { 4 } else { 3 }).map(Target::Color))
        .chain(std::iter::once(Target::Transparency))
    {
        let name = target.name(&studio.doc);
        let response = fields::native_scope(ui, key.with(target), |ui| {
            ui.add(
                egui::Button::new(&name)
                    .selected(state.selected == target)
                    .min_size(egui::vec2(ui.available_width(), 26.)),
            )
        })
        .inner;
        welcome::name_control(
            ui,
            &response,
            key.with((target, "label")),
            &format!("Channel {name}"),
        );
        if response.clicked() {
            state.selected = target;
            select_target = Some(None);
            state.error.clear();
        }
    }
    for channel in &studio.doc.channels {
        let texture = thumbnail(ui, &mut state, channel);
        ui.horizontal(|ui| {
            ui.add(egui::Image::new(&texture).fit_to_exact_size(egui::vec2(40., 32.)));
            let target = Target::Saved(channel.id);
            let response = fields::native_scope(ui, key.with(target), |ui| {
                ui.add(
                    egui::Button::new(&channel.name)
                        .selected(state.selected == target)
                        .min_size(egui::vec2(ui.available_width(), 32.)),
                )
            })
            .inner;
            welcome::name_control(
                ui,
                &response,
                key.with((target, "label")),
                &format!("Channel {}", channel.name),
            );
            if response.clicked() {
                state.selected = target;
                select_target = Some(Some(channel.id));
                state.original = channel.name.clone();
                state.name = channel.name.clone();
                state.error.clear();
            }
        });
    }
    if let Some(target) = select_target {
        if let Err(error) = studio.select_channel_target(target) {
            state.error = error;
        }
    }
    let running = ui.data(|d| d.get_temp::<Run>(run_id())).is_some();
    let editable =
        !running && studio.doc.collaboration.is_none() && studio.capture_channels().is_ok();
    let saved = if let Target::Saved(id) = state.selected {
        Some(id)
    } else {
        None
    };
    if let Some(id) = saved
        && let Some(channel) = studio.doc.channels.iter().find(|c| c.id == id)
    {
        if state.original != channel.name {
            state.original = channel.name.clone();
            state.name = channel.name.clone();
            state.name_error.clear();
        }
        let response = ui.add_enabled(
            editable,
            egui::TextEdit::singleline(&mut state.name)
                .id(key.with((id, "name")))
                .desired_width(ui.available_width()),
        );
        welcome::name_control(
            ui,
            &response,
            key.with((id, "name-label")),
            "Saved channel name",
        );
        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let result = studio.capture_channels().and_then(|target| {
                studio.rename_channel(&target, id, &state.original, &state.name)
            });
            match result {
                Ok(_) => state.name_error.clear(),
                Err(e) => state.name_error = e,
            }
        }
        if !state.name_error.is_empty() {
            welcome::control_error(ui, &[&response], &state.name_error);
        }
    }
    let response = fields::native_scope(ui, key.with("combine"), |ui| {
        egui::ComboBox::from_id_salt("channel-combine")
            .selected_text(state.combine.name())
            .show_ui(ui, |ui| {
                for operation in [
                    Combine::Replace,
                    Combine::Add,
                    Combine::Subtract,
                    Combine::Intersect,
                ] {
                    ui.selectable_value(&mut state.combine, operation, operation.name());
                }
            })
    })
    .inner
    .response;
    welcome::name_control(
        ui,
        &response,
        key.with("combine-label"),
        "Channel selection operation",
    );
    let mut work = None;
    ui.horizontal_wrapped(|ui| {
        if icon(
            ui,
            key.with("load"),
            ph::SELECTION,
            "Load channel as pixel selection",
            !running && studio.capture_channels().is_ok(),
        ) {
            work = Some(Work::Load(state.selected, state.combine));
        }
        if icon(
            ui,
            key.with("save"),
            ph::PLUS,
            "Save selection as alpha channel",
            editable && studio.pixel_selection_space().is_some(),
        ) {
            work = Some(Work::Save);
        }
        if icon(
            ui,
            key.with("duplicate"),
            ph::COPY,
            "Duplicate saved channel",
            editable && saved.is_some(),
        ) {
            let result = studio
                .capture_channels()
                .and_then(|target| studio.duplicate_channel(&target, saved.unwrap()));
            match result {
                Ok(id) => {
                    if let Err(error) = studio.select_channel_target(Some(id)) {
                        state.error = error;
                    }
                    state.selected = Target::Saved(id);
                    state.error.clear();
                }
                Err(e) => state.error = e,
            }
        }
        if icon(
            ui,
            key.with("invert"),
            ph::ARROW_CLOCKWISE,
            "Invert saved channel",
            editable && saved.is_some(),
        ) {
            work = Some(Work::Invert(saved.unwrap()));
        }
        if icon(
            ui,
            key.with("remove"),
            ph::TRASH,
            "Delete saved channel",
            editable && saved.is_some(),
        ) {
            let result = studio
                .capture_channels()
                .and_then(|target| studio.remove_channel(&target, saved.unwrap()));
            match result {
                Ok(_) => {
                    state.selected = Target::Composite;
                    state.error.clear();
                }
                Err(e) => state.error = e,
            }
        }
        if icon(ui,key.with("curves"),ph::BEZIER_CURVE,"Adjust channel curves",editable&&saved.is_some()) {super::channel_adjustment::open(ui.ctx(),studio);}
        if running
            && icon(
                ui,
                key.with("cancel"),
                ph::X,
                "Cancel channel operation",
                true,
            )
        {
            if let Some(run) = ui.data(|d| d.get_temp::<Run>(run_id())) {
                run.valid.store(false, Ordering::Relaxed);
            }
            jobs::cancel::<Finished>(ui.ctx(), JOB);
            ui.data_mut(|d| d.remove::<Run>(run_id()));
        }
    });
    if let Some(work) = work {
        if let Err(error) = start(ui.ctx(), studio, work) {
            state.error = error;
        }
    }
    if !state.error.is_empty() {
        ui.colored_label(ui.visuals().error_fg_color, &state.error);
    }
    let texture = if state.selected == Target::Composite {
        composite
    } else {
        let cached = state
            .preview
            .as_ref()
            .filter(|(generation, target, _)| {
                *generation == studio.canvas_gen && *target == state.selected
            })
            .map(|(_, _, texture)| texture.clone());
        if cached.is_none()
            && state.failed != Some((studio.canvas_gen, state.selected))
            && !jobs::is_running::<RenderedChannel>(ui.ctx(), PREVIEW)
        {
            let doc = studio.doc.clone();
            let document = studio.swap_id.clone();
            let generation = studio.canvas_gen;
            let target = state.selected;
            jobs::start(ui.ctx(), PREVIEW, move || {
                Ok(RenderedChannel {
                    document,
                    generation,
                    target,
                    result: crate::channels::preview(
                        &doc,
                        target,
                        128,
                        &crate::deep_color::storage::check,
                    ),
                })
            });
        }
        cached
    };
    if let Some(texture) = texture {
        let size = texture.size_vec2();
        let factor = (ui.available_width().min(256.) / size.x).min(2.);
        ui.add(egui::Image::new(&texture).fit_to_exact_size(size * factor));
    }
    ui.data_mut(|d| d.insert_temp(key, state));
}

#[derive(Clone)]
struct CanvasChannel {
    document: String,
    id: u64,
    generation: u64,
    edge: u32,
    preview: Option<Id>,
    texture: egui::TextureHandle,
}
/// Show the actual targeted scalar plane at its stored document placement.
/// Args: canvas UI, current editor and real canvas clip. Returns nothing; display bytes never feed edits.
pub(super) fn paint_target(ui: &Ui, studio: &Studio, rect: egui::Rect) {
    let key = Id::new("canvas-saved-channel");
    let Some(channel) = studio.channel_paint_target() else {
        ui.ctx().data_mut(|d| d.remove::<CanvasChannel>(key));
        return;
    };
    let adjusted=super::channel_adjustment::preview(ui.ctx(),studio);
    let plane=adjusted.as_ref().map_or(&channel.plane,|(plane,_)|plane);
    let preview=adjusted.as_ref().map(|(_,key)|*key);
    let edge = (rect.width().max(rect.height()) * ui.ctx().pixels_per_point())
        .ceil()
        .clamp(1., 4096.) as u32;
    let cached = ui.ctx().data(|d| d.get_temp::<CanvasChannel>(key));
    let cached = cached
        .filter(|c| {
            c.document == studio.swap_id
                && c.id == channel.id
                && c.generation == studio.canvas_gen
                && c.edge == edge && c.preview == preview
        })
        .unwrap_or_else(|| CanvasChannel {
            document: studio.swap_id.clone(),
            id: channel.id,
            generation: studio.canvas_gen,
            edge,
            preview,
            texture: plane_texture(ui.ctx(), plane, edge, "canvas-saved-channel"),
        });
    let painter = ui.painter_at(rect);
    let screen = |p: crate::geom::Pt| {
        let p = studio.view.to_screen(p);
        rect.min + egui::vec2(p.x, p.y)
    };
    painter.rect_filled(
        egui::Rect::from_two_pos(screen(crate::geom::Pt::ZERO), screen(studio.doc.size())),
        0.,
        egui::Color32::BLACK,
    );
    let mut mesh = egui::Mesh::with_texture(cached.texture.id());
    for (x, y, u, v) in [
        (0., 0., 0., 0.),
        (plane.width as f32, 0., 1., 0.),
        (
            plane.width as f32,
            plane.height as f32,
            1.,
            1.,
        ),
        (0., plane.height as f32, 0., 1.),
    ] {
        let mut point = tiny_skia::Point::from_xy(x, y);
        plane.placement().map_point(&mut point);
        mesh.vertices.push(egui::epaint::Vertex {
            pos: screen(crate::geom::Pt::new(point.x, point.y)),
            uv: egui::pos2(u, v),
            color: egui::Color32::WHITE,
        });
    }
    if mesh.vertices.iter().all(|v| v.pos.is_finite()) {
        mesh.indices.extend([0, 1, 2, 0, 2, 3]);
        painter.add(mesh);
    }
    ui.ctx().data_mut(|d| d.insert_temp(key, cached));
}

/// Keep the brush outline in the targeted alpha plane's actual pixel coordinates.
/// Args: canvas painter, clip, current editor and document cursor. Returns whether a saved channel owns the outline.
pub(super) fn paint_brush_outline(
    painter: &egui::Painter,
    rect: egui::Rect,
    studio: &Studio,
    world: crate::geom::Pt,
) -> bool {
    use crate::geom::Pt;
    let Some(channel) = studio.channel_paint_target() else {
        return false;
    };
    let placement = channel.plane.placement();
    let Some(inverse) = placement.invert() else {
        return true;
    };
    let mut center = tiny_skia::Point::from_xy(world.x, world.y);
    inverse.map_point(&mut center);
    let size = studio.brush.size
        * if studio.brush.pressure_size {
            studio.brush.pressure.clamp(0.01, 1.)
        } else {
            1.
        };
    let angle = studio.brush.angle
        + if studio.brush.follow_direction {
            studio.brush_heading
        } else {
            0.
        };
    let map = |offset: Pt| {
        let mut point = tiny_skia::Point::from_xy(center.x + offset.x, center.y + offset.y);
        placement.map_point(&mut point);
        let p = studio.view.to_screen(Pt::new(point.x, point.y));
        rect.min + egui::vec2(p.x, p.y)
    };
    let edge = |a: Pt, b: Pt| {
        let points = [map(a), map(b)];
        if points.iter().all(|p| p.is_finite()) {
            painter.line_segment(points, egui::Stroke::new(3., egui::Color32::BLACK));
            painter.line_segment(points, egui::Stroke::new(1., egui::Color32::WHITE));
        }
    };
    if let Some(outline) = studio.brushes.tip_outline() {
        for &(a, b) in outline.iter() {
            edge(a.rotate(angle) * size, b.rotate(angle) * size);
        }
    } else {
        for i in 0..64 {
            let point = |i| {
                let angle = i as f32 / 64. * std::f32::consts::TAU;
                Pt::new(angle.cos(), angle.sin()) * (size * 0.5)
            };
            edge(point(i), point(i + 1));
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::super::welcome::keyboard_tests as keys;
    use super::*;
    use egui::{Event, Key, Modifiers};
    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        super::super::theme::apply(&ctx);
        ctx
    }
    #[test]
    fn craft_channels_native_keys_load_save_rename_invert_duplicate_delete_and_reopen() {
        let ctx = context();
        let mut studio = Studio::new();
        studio.show_welcome = false;
        studio.doc = crate::channels::tests::fixture();
        studio.active_layer = Some(0);
        let mut frame = |events| {
            let output = keys::render(&ctx, events, |ui| {
                ui.set_max_width(288.);
                show(ui, &mut studio);
            });
            poll(&ctx, &mut studio);
            output
        };
        let invoke = |name: &str, frame: &mut dyn FnMut(Vec<Event>) -> egui::FullOutput| {
            keys::tab_to(&ctx, name, &mut *frame);
            frame(keys::key(Key::Space, Modifiers::NONE));
            frame(vec![]);
        };
        let wait = |frame: &mut dyn FnMut(Vec<Event>) -> egui::FullOutput| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            while ctx.data(|d| d.get_temp::<Run>(run_id())).is_some() {
                frame(vec![]);
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            frame(vec![]);
        };
        frame(vec![]);
        frame(vec![]);
        invoke("Channel Transparency", &mut frame);
        invoke("Load channel as pixel selection", &mut frame);
        wait(&mut frame);
        invoke("Save selection as alpha channel", &mut frame);
        wait(&mut frame);
        keys::tab_to(&ctx, "Saved channel name", &mut frame);
        keys::replace("Native alpha Ω", &mut frame);
        frame(keys::key(Key::Enter, Modifiers::NONE));
        frame(vec![]);
        drop(frame);
        assert_eq!(studio.doc.channels.len(), 1);
        assert_eq!(studio.doc.channels[0].name, "Native alpha Ω");
        assert_eq!(studio.doc.channels[0].plane.samples.len(), 32);
        assert!(
            studio.doc.channels[0]
                .plane
                .samples
                .iter()
                .all(|v| (*v - 128. / 255.).abs() <= 1. / 255.)
        );
        let original = crate::project::encode(&studio.doc).unwrap();
        let history = studio.history.len();
        let mut frame = |events| {
            let output = keys::render(&ctx, events, |ui| {
                ui.set_max_width(288.);
                show(ui, &mut studio);
            });
            poll(&ctx, &mut studio);
            output
        };
        invoke("Invert saved channel", &mut frame);
        wait(&mut frame);
        invoke("Duplicate saved channel", &mut frame);
        invoke("Delete saved channel", &mut frame);
        drop(frame);
        assert_eq!(studio.doc.channels.len(), 1);
        assert_eq!(studio.history.len(), history + 3);
        assert!(
            studio.doc.channels[0]
                .plane
                .samples
                .iter()
                .all(|v| (*v - 127. / 255.).abs() <= 1. / 255.)
        );
        let saved = crate::project::encode(&studio.doc).unwrap();
        assert_eq!(
            crate::project::encode(&crate::project::decode(&saved).unwrap()).unwrap(),
            saved
        );
        studio.undo();
        studio.undo();
        studio.undo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), original);
        studio.redo();
        studio.redo();
        studio.redo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), saved);
        jobs::cancel::<Finished>(&ctx, JOB);
        jobs::cancel::<RenderedChannel>(&ctx, PREVIEW);
    }
    #[test]
    fn craft_channels_native_worker_retires_changed_document_and_selection_owner() {
        let ctx = context();
        let mut studio = Studio::new();
        studio.doc = crate::channels::tests::fixture();
        let before = crate::project::encode(&studio.doc).unwrap();
        start(
            &ctx,
            &studio,
            Work::Load(Target::Transparency, Combine::Replace),
        )
        .unwrap();
        studio.pixel_sel_gen += 1;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while ctx.data(|d| d.get_temp::<Run>(run_id())).is_some() {
            poll(&ctx, &mut studio);
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(studio.pixel_sel.is_none());
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), before);
    }
}
