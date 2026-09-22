//! Authenticated welcome browser. All requests and preview rendering run off the
//! event loop; changing accounts drops pending results and cached private images.
use crate::{
    app::Studio,
    cloud::{
        self,
        client::{Client, Event, File, Project},
    },
    ui::{jobs, theme},
};
use eframe::egui::{self, ColorImage, Pos2, Rect, Sense, Stroke, Vec2};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::{Duration, Instant},
};

const STATE: &str = "welcome-team-state";
const PROJECTS: &str = "welcome-team-projects";
const FILES: &str = "welcome-team-files";
const PREVIEW: &str = "welcome-team-preview";
const REFRESH: Duration = Duration::from_secs(60);
type FileResult = (String, Vec<File>);
type PreviewResult = (String, Result<ColorImage, String>);

#[derive(Clone, Default)]
struct State {
    identity: cloud::Identity,
    projects: Vec<Project>,
    project: Option<Project>,
    files: Vec<File>,
    images: HashMap<String, Result<egui::TextureHandle, String>>,
    image_order: VecDeque<String>,
    selected: HashSet<String>,
    last_refresh: Option<Instant>,
    error: Option<String>,
    preview: Option<File>,
    review_selection: Vec<File>,
    pending_preview: bool,
}

fn state(ctx: &egui::Context) -> State {
    ctx.data(|d| d.get_temp(egui::Id::new(STATE)))
        .unwrap_or_default()
}
fn store(ctx: &egui::Context, state: State) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(STATE), state));
}
fn cancel(ctx: &egui::Context) {
    jobs::cancel::<Vec<Project>>(ctx, PROJECTS);
    jobs::cancel::<FileResult>(ctx, FILES);
    jobs::cancel::<PreviewResult>(ctx, PREVIEW);
}

/// Drop the welcome-owned jobs when an editor takes over; completed results must
/// not keep the shared background-job registry busy while this panel is hidden.
pub(super) fn suspend(ctx: &egui::Context) {
    if ctx.data(|d| d.get_temp::<State>(egui::Id::new(STATE)).is_some()) {
        cancel(ctx);
        ctx.data_mut(|d| d.remove::<State>(egui::Id::new(STATE)));
    }
}

/// Call each frame even while Recent is selected so Team appears after login or
/// an accepted invitation. An owner's solo backup does not expose this tab.
pub(super) fn has_shared(ctx: &egui::Context, studio: &mut Studio) -> bool {
    let mut state = state(ctx);
    // Visible cards reassert this in show(). Switching back to Recent must not
    // keep screenshot/update readiness waiting for thumbnails no longer shown.
    state.pending_preview = false;
    if state.identity != studio.cloud_identity {
        cancel(ctx);
        state = State {
            identity: studio.cloud_identity.clone(),
            ..Default::default()
        };
    }
    if !cloud::signed_in(&state.identity) {
        cancel(ctx);
        store(ctx, State::default());
        return false;
    }
    if let Some(result) = jobs::poll::<Vec<Project>>(ctx, PROJECTS) {
        match result {
            Ok(projects) => {
                state.projects = projects.into_iter().filter(|p| p.shared).collect();
                state.error = None;
                if let Some(current) = state.project.clone()
                    && let Some(updated) =
                        state.projects.iter().find(|p| p.id == current.id).cloned()
                    && updated.updated != current.updated
                    && !jobs::is_running::<FileResult>(ctx, FILES)
                {
                    let client = Client::new(state.identity.clone());
                    let id = updated.id.clone();
                    state.project = Some(updated);
                    jobs::start(ctx, FILES, move || {
                        client.project_files(&id).map(|files| (id, files))
                    });
                }
                if state
                    .project
                    .as_ref()
                    .is_some_and(|p| !state.projects.iter().any(|q| q.id == p.id))
                {
                    state.project = None;
                    state.files.clear();
                    state.selected.clear();
                    state.preview = None;
                    state.review_selection.clear();
                    state.images.clear();
                    state.image_order.clear();
                    jobs::cancel::<FileResult>(ctx, FILES);
                    jobs::cancel::<PreviewResult>(ctx, PREVIEW);
                }
            }
            Err(error) => {
                // A revoked or expired identity must not leave a stale Team tab.
                state.projects.clear();
                state.project = None;
                state.files.clear();
                state.images.clear();
                state.image_order.clear();
                state.selected.clear();
                state.preview = None;
                state.review_selection.clear();
                jobs::cancel::<FileResult>(ctx, FILES);
                jobs::cancel::<PreviewResult>(ctx, PREVIEW);
                studio.status = format!("Team projects unavailable: {error}");
                state.error = Some(error);
            }
        }
    }
    if let Some(result) = jobs::poll::<FileResult>(ctx, FILES) {
        match result {
            Ok((project_id, files))
                if state.project.as_ref().is_some_and(|p| p.id == project_id) =>
            {
                state.files = files;
                state
                    .selected
                    .retain(|id| state.files.iter().any(|f| &f.id == id));
                state.error = None;
            }
            Err(error) => {
                state.files.clear();
                state.images.clear();
                state.image_order.clear();
                state.selected.clear();
                state.preview = None;
                state.review_selection.clear();
                state.error = Some(error);
            }
            _ => {}
        }
    }
    if let Some(Ok((id, result))) = jobs::poll::<PreviewResult>(ctx, PREVIEW) {
        state.image_order.push_back(id.clone());
        state.images.insert(
            id.clone(),
            result.map(|image| {
                ctx.load_texture(format!("team-{id}"), image, egui::TextureOptions::LINEAR)
            }),
        );
        while state.images.len() > 120 {
            if let Some(oldest) = state.image_order.pop_front() {
                state.images.remove(&oldest);
            } else {
                break;
            }
        }
    }
    if state.last_refresh.is_none_or(|at| at.elapsed() >= REFRESH)
        && !jobs::is_running::<Vec<Project>>(ctx, PROJECTS)
    {
        let client = Client::new(state.identity.clone());
        jobs::start(ctx, PROJECTS, move || client.projects());
        state.last_refresh = Some(Instant::now());
    }
    ctx.request_repaint_after(Duration::from_secs(1));
    let visible = !state.projects.is_empty();
    store(ctx, state);
    visible
}

pub(super) fn ready(ctx: &egui::Context) -> bool {
    !jobs::is_running::<Vec<Project>>(ctx, PROJECTS)
        && !jobs::is_running::<FileResult>(ctx, FILES)
        && !jobs::is_running::<PreviewResult>(ctx, PREVIEW)
        && !state(ctx).pending_preview
}

fn fetch_files(ctx: &egui::Context, state: &mut State, project: Project) {
    jobs::cancel::<FileResult>(ctx, FILES);
    state.selected.clear();
    state.files.clear();
    state.error = None;
    state.preview = None;
    state.review_selection.clear();
    let id = project.id.clone();
    state.project = Some(project);
    let client = Client::new(state.identity.clone());
    jobs::start(ctx, FILES, move || {
        client.project_files(&id).map(|files| (id, files))
    });
}

pub(super) fn show(ui: &mut egui::Ui, studio: &mut Studio) {
    let ctx = ui.ctx().clone();
    let mut state = state(&ctx);
    if !cloud::signed_in(&studio.cloud_identity) || state.identity != studio.cloud_identity {
        return;
    }
    state.pending_preview = false;
    ui.horizontal(|ui| {
        if let Some(project) = state.project.clone() {
            if ui.small_button("← Team").clicked() {
                state.project = None;
                state.files.clear();
                state.selected.clear();
                state.preview = None;
                state.review_selection.clear();
                jobs::cancel::<FileResult>(&ctx, FILES);
            }
            ui.label(egui::RichText::new(project.title).color(theme::fg_weak()));
        }
        if ui
            .small_button("↻")
            .on_hover_text("Refresh team projects")
            .clicked()
        {
            state.last_refresh = None;
            state.images.clear();
            state.image_order.clear();
            if let Some(project) = state.project.clone() {
                fetch_files(&ctx, &mut state, project);
            }
        }
    });
    if let Some(error) = state.error.clone() {
        ui.colored_label(theme::p().error, error);
    }
    if jobs::is_running::<FileResult>(&ctx, FILES) {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Loading shared files…");
        });
    }
    if let Some(file) = state.preview.clone() {
        if ui.button("← Files").clicked() {
            state.preview = None;
            state.review_selection.clear();
        }
        ui.label(&file.name);
        if state.review_selection.len() > 1 {
            let index = state
                .review_selection
                .iter()
                .position(|f| f.id == file.id)
                .unwrap_or(0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(index > 0, egui::Button::new("← Previous"))
                    .clicked()
                {
                    state.preview = Some(state.review_selection[index - 1].clone());
                }
                ui.label(format!("{} / {}", index + 1, state.review_selection.len()));
                if ui
                    .add_enabled(
                        index + 1 < state.review_selection.len(),
                        egui::Button::new("Next →"),
                    )
                    .clicked()
                {
                    state.preview = Some(state.review_selection[index + 1].clone());
                }
            });
        }
        match state.images.get(&file.id).cloned() {
            Some(Ok(texture)) => {
                ui.add(
                    egui::Image::new(&texture)
                        .max_size(Vec2::new(
                            ui.available_width(),
                            (ui.available_height() - 35.0).max(1.0),
                        ))
                        .maintain_aspect_ratio(true),
                );
            }
            Some(Err(error)) => {
                ui.colored_label(theme::p().error, error);
                if ui.button("Retry preview").clicked() {
                    state.images.remove(&file.id);
                    request_preview(&ctx, &mut state, &file);
                }
            }
            None => {
                request_preview(&ctx, &mut state, &file);
                ui.spinner();
            }
        }
        if let Some(project) = &state.project {
            ui.hyperlink_to(
                "Open cloud review",
                format!("https://omadesign.app/cloud?project={}", project.id),
            );
        }
        store(&ctx, state);
        return;
    }
    if !state.selected.is_empty() {
        ui.horizontal(|ui| {
            ui.label(format!("{} selected", state.selected.len()));
            if ui
                .add_enabled(!studio.cloud_busy(), egui::Button::new("Open selected"))
                .clicked()
            {
                if let Some(project) = &state.project {
                    let project_id = project.id.clone();
                    let files: Vec<_> = visible_files(&state)
                        .into_iter()
                        .filter(|f| state.selected.contains(&f.id) && f.kind == "source")
                        .collect();
                    if !files.is_empty() {
                        studio.pull_cloud_files(
                            project_id,
                            files.into_iter().map(|f| f.id).collect(),
                        );
                    } else {
                        state.review_selection = visible_files(&state)
                            .into_iter()
                            .filter(|f| state.selected.contains(&f.id))
                            .collect();
                        state.preview = state.review_selection.first().cloned();
                    }
                }
                state.selected.clear();
            }
            if ui.button("Clear").clicked() {
                state.selected.clear();
            }
        });
    }
    egui::ScrollArea::vertical()
        .id_salt("welcome-team-browser")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let width = ui.available_width().max(1.0);
            let columns = ((width / 170.0).floor() as usize).clamp(1, 3);
            let gap = 12.0;
            let tile_width = (width - gap * (columns - 1) as f32) / columns as f32;
            let start = ui.cursor().min;
            let mut bottoms = vec![start.y; columns];
            let projects = state.project.is_none();
            let files = visible_files(&state);
            let count = if projects {
                state.projects.len()
            } else {
                files.len()
            };
            for i in 0..count {
                let col = bottoms
                    .iter()
                    .enumerate()
                    .min_by(|a, b| a.1.total_cmp(b.1))
                    .unwrap()
                    .0;
                let ratio = if projects {
                    1.25
                } else {
                    aspect(&files[i], &state)
                };
                let rect = Rect::from_min_size(
                    Pos2::new(start.x + col as f32 * (tile_width + gap), bottoms[col]),
                    Vec2::new(tile_width, tile_width / ratio),
                );
                bottoms[col] = rect.bottom() + gap;
                if !ui.is_rect_visible(rect) {
                    continue;
                }
                if projects {
                    let project = state.projects[i].clone();
                    let response = ui.interact(rect, ui.id().with(&project.id), Sense::click());
                    for file in &project.previews {
                        request_preview(&ctx, &mut state, file);
                    }
                    folder(ui, rect, &project, &state, response.hovered());
                    if response.hovered() {
                        hover_name(ui, rect, &project.title);
                    }
                    let response =
                        response.on_hover_text(format!("{} · {}", project.title, project.role));
                    if response.clicked() {
                        fetch_files(&ctx, &mut state, project);
                        break;
                    }
                } else {
                    let file = &files[i];
                    let source = preview_source(file, &state.files);
                    request_preview(&ctx, &mut state, &source);
                    let response = ui.interact(rect, ui.id().with(&file.id), Sense::click());
                    draw_file(ui, rect, &source.id, &state);
                    let selected = state.selected.contains(&file.id);
                    if selected || response.hovered() {
                        ui.painter().rect_stroke(
                            rect,
                            6,
                            Stroke::new(if selected { 2.0 } else { 1.0 }, theme::accent()),
                            egui::StrokeKind::Inside,
                        );
                    }
                    if response.hovered() {
                        hover_name(ui, rect, &file.name);
                    }
                    let response = response.on_hover_text(format!(
                        "{} · version {}{}",
                        file.name,
                        file.version,
                        if file.kind == "snapshot" {
                            " · flat review export"
                        } else {
                            ""
                        }
                    ));
                    if response.clicked() {
                        if ui.input(|i| i.modifiers.shift) || !state.selected.is_empty() {
                            if !state.selected.insert(file.id.clone()) {
                                state.selected.remove(&file.id);
                            }
                        } else if file.kind == "source" {
                            if !studio.cloud_busy() {
                                let project = state.project.as_ref().unwrap().id.clone();
                                let id = file.id.clone();
                                studio.cloud_task(move |client| {
                                    client.pull_file(&project, &id).map(Event::Pulled)
                                });
                            }
                        } else {
                            state.preview = Some(file.clone());
                        }
                    }
                }
            }
            let height = bottoms.into_iter().fold(start.y, f32::max) - start.y;
            ui.allocate_space(Vec2::new(width, height));
            if count == 0 && !jobs::is_running::<FileResult>(&ctx, FILES) {
                ui.label(if projects {
                    "No shared team projects."
                } else {
                    "This project has no accessible designs or flat review exports yet."
                });
            }
        });
    store(&ctx, state);
}

fn visible_files(state: &State) -> Vec<File> {
    let has_sources = state.files.iter().any(|f| f.kind == "source");
    let mut seen = HashSet::new();
    let mut files = state.files.clone();
    files.sort_by(|a, b| {
        b.created
            .total_cmp(&a.created)
            .then_with(|| b.version.cmp(&a.version))
    });
    files
        .into_iter()
        .filter(|f| f.kind == if has_sources { "source" } else { "snapshot" })
        .filter(|f| seen.insert(f.name.clone()))
        .collect()
}

fn preview_source(file: &File, files: &[File]) -> File {
    if file.kind != "source" {
        return file.clone();
    }
    let base = file.name.strip_suffix(".oma").unwrap_or(&file.name);
    files
        .iter()
        .find(|f| f.kind == "snapshot" && f.name == format!("{base}-review.png"))
        .cloned()
        .unwrap_or_else(|| file.clone())
}

fn aspect(file: &File, state: &State) -> f32 {
    let source = preview_source(file, &state.files);
    let ratio = match state.images.get(&source.id) {
        Some(Ok(texture)) => texture.size_vec2().x / texture.size_vec2().y,
        _ => source
            .width
            .zip(source.height)
            .map(|(w, h)| w / h)
            .unwrap_or(1.0),
    };
    if ratio.is_finite() && ratio > 0.0 {
        ratio
    } else {
        1.0
    }
}

fn request_preview(ctx: &egui::Context, state: &mut State, file: &File) {
    if state.images.contains_key(&file.id) {
        return;
    }
    state.pending_preview = true;
    if jobs::is_running::<PreviewResult>(ctx, PREVIEW) {
        return;
    }
    let client = Client::new(state.identity.clone());
    let file = file.clone();
    jobs::start(ctx, PREVIEW, move || {
        Ok((file.id.clone(), render_preview(&client, &file)))
    });
}

fn render_preview(client: &Client, file: &File) -> Result<ColorImage, String> {
    let bytes = client.download(&file.id)?;
    if file.kind == "source" {
        let doc = crate::project::decode(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?)?;
        let mut bounds = crate::geom::Bounds::from_min_size(
            crate::geom::Pt::ZERO,
            crate::geom::Pt::new(doc.width, doc.height),
        );
        for artboard in &doc.artboards {
            bounds = bounds.union(artboard.bounds());
        }
        let scale = 512.0 / bounds.width().max(bounds.height()).max(1.0);
        let width = (bounds.width() * scale).round().max(1.0) as u32;
        let height = (bounds.height() * scale).round().max(1.0) as u32;
        let view = crate::compositor::View {
            scale,
            offset: bounds.min * -scale,
        };
        let pixels = crate::compositor::render_view(
            &doc,
            view,
            width,
            height,
            crate::compositor::Draft::none(),
        )
        .ok_or("Could not render preview")?;
        return Ok(ColorImage::from_rgba_premultiplied(
            [width as usize, height as usize],
            pixels.data(),
        ));
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(256 * 1024 * 1024);
    limits.max_image_width = Some(32768);
    limits.max_image_height = Some(32768);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|e| e.to_string())?
        .thumbnail(512, 512)
        .to_rgba8();
    Ok(ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    ))
}

fn draw_file(ui: &egui::Ui, rect: Rect, id: &str, state: &State) {
    ui.painter().rect_filled(rect, 6, theme::bg_widget());
    match state.images.get(id) {
        Some(Ok(texture)) => {
            ui.painter().image(
                texture.id(),
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        Some(Err(_)) => {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Preview unavailable",
                egui::FontId::proportional(12.0),
                theme::fg_weak(),
            );
        }
        None => {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Loading…",
                egui::FontId::proportional(12.0),
                theme::fg_weak(),
            );
        }
    }
}

fn folder(ui: &egui::Ui, rect: Rect, project: &Project, state: &State, hovered: bool) {
    let painter = ui.painter();
    let stroke = Stroke::new(
        1.5,
        if hovered {
            theme::accent()
        } else {
            theme::border_strong()
        },
    );
    let body = rect.shrink(4.0);
    let tab = Rect::from_min_size(
        body.min,
        Vec2::new(body.width() * 0.42, body.height() * 0.18),
    );
    painter.rect_filled(tab, 5, theme::bg_widget());
    painter.rect_stroke(tab, 5, stroke, egui::StrokeKind::Inside);
    let pile = Rect::from_min_max(
        body.min + Vec2::new(10.0, body.height() * 0.12),
        body.max - Vec2::new(10.0, 10.0),
    );
    for (i, file) in project.previews.iter().rev().enumerate() {
        let shift = Vec2::new(i as f32 * 5.0, i as f32 * 5.0);
        let target = Rect::from_min_size(pile.min + shift, pile.size() - Vec2::splat(10.0));
        painter.rect_filled(target, 4, theme::bg_panel());
        painter.rect_stroke(
            target,
            4,
            Stroke::new(1.0, theme::border()),
            egui::StrokeKind::Inside,
        );
        if let Some(Ok(texture)) = state.images.get(&file.id) {
            let size = texture.size_vec2();
            let factor = (target.width() / size.x).min(target.height() / size.y);
            let image = Rect::from_center_size(target.center(), size * factor);
            painter.image(
                texture.id(),
                image,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
    }
    let front = Rect::from_min_max(
        Pos2::new(body.left(), body.top() + body.height() * 0.22),
        body.max,
    );
    painter.rect_stroke(front, 7, stroke, egui::StrokeKind::Inside);
}

fn hover_name(ui: &egui::Ui, rect: Rect, name: &str) {
    let bar = Rect::from_min_max(
        Pos2::new(rect.left(), (rect.bottom() - 28.0).max(rect.top())),
        rect.max,
    );
    ui.painter()
        .rect_filled(bar, 4, theme::bg_panel().gamma_multiply(0.96));
    let galley = ui.painter().layout(
        name.to_string(),
        egui::FontId::proportional(12.0),
        theme::fg(),
        (rect.width() - 12.0).max(1.0),
    );
    ui.painter()
        .with_clip_rect(bar)
        .galley(bar.min + Vec2::new(6.0, 6.0), galley, theme::fg());
}

#[cfg(test)]
mod tests {
    use super::*;
    fn file(id: &str, name: &str, kind: &str, created: f64) -> File {
        File {
            id: id.into(),
            name: name.into(),
            kind: kind.into(),
            version: created as u64,
            created,
            width: None,
            height: None,
        }
    }
    #[test]
    fn shows_latest_versions_in_modified_order_and_reviewers_only_get_snapshots() {
        let state = State {
            files: vec![
                file("a", "logo.oma", "source", 1.0),
                file("b", "poster.oma", "source", 2.0),
                file("c", "logo.oma", "source", 3.0),
                file("s", "logo-review.png", "snapshot", 4.0),
            ],
            ..Default::default()
        };
        assert_eq!(
            visible_files(&state)
                .iter()
                .map(|f| f.id.as_str())
                .collect::<Vec<_>>(),
            ["c", "b"]
        );
        assert_eq!(preview_source(&state.files[2], &state.files).id, "s");
        let state = State {
            files: vec![file("s", "flat.png", "snapshot", 1.0)],
            ..Default::default()
        };
        assert_eq!(visible_files(&state)[0].id, "s");
    }
    #[test]
    fn sign_out_removes_cached_private_projects_and_previews() {
        let ctx = egui::Context::default();
        let mut studio = Studio::new();
        studio.cloud_identity = Default::default();
        store(
            &ctx,
            State {
                projects: vec![Project {
                    id: "shared".into(),
                    title: "Private team".into(),
                    role: "owner".into(),
                    updated: 0.0,
                    shared: true,
                    previews: vec![],
                }],
                ..Default::default()
            },
        );
        assert!(!has_shared(&ctx, &mut studio));
        assert!(state(&ctx).projects.is_empty());
        assert!(ready(&ctx));
    }
}
