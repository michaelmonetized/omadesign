//! Small live previews share the editor renderer and retain no full-resolution texture.
use crate::app::Studio;
use eframe::egui::{self, TextureHandle};
use std::collections::{HashMap, HashSet};

const JOB: &str = "pane-thumbnail-render";
type Target = (u64, u64);
type Rendered = (String, u64, Target, egui::ColorImage);
#[derive(Clone, Default)]
struct Cache {
    document: String,
    images: HashMap<Target, (u64, TextureHandle, std::sync::Arc<egui::ColorImage>)>,
    failed: HashSet<(u64, Target)>,
}

/// Fetch a bounded preview of the document, a layer, or a concrete object.
/// Args: UI, editor and optional layer/object. Returns the last rendered texture while a replacement is pending.
pub(super) fn image(
    ui: &egui::Ui,
    studio: &Studio,
    layer: Option<usize>,
    object: Option<u64>,
) -> Option<TextureHandle> {
    let id = egui::Id::new("pane-thumbnails");
    let mut cache = ui
        .ctx()
        .data_mut(|d| d.remove_temp::<Cache>(id))
        .unwrap_or_default();
    if cache.document != studio.swap_id {
        cache = Cache {
            document: studio.swap_id.clone(),
            ..Default::default()
        };
    }
    if let Some(result) = super::jobs::poll::<Rendered>(ui.ctx(), JOB) {
        if let Ok((document, generation, target, pixels)) = result {
            if document == studio.swap_id {
                let retained = std::sync::Arc::new(pixels.clone());
                let texture = if let Some((_, texture, _)) = cache.images.get_mut(&target) {
                    texture.set(pixels, egui::TextureOptions::LINEAR);
                    texture.clone()
                } else {
                    ui.ctx().load_texture(
                        format!("pane-{document}-{}-{}", target.0, target.1),
                        pixels,
                        egui::TextureOptions::LINEAR,
                    )
                };
                cache.images.insert(target, (generation, texture, retained));
            }
        }
    }
    let target = (
        layer
            .and_then(|i| studio.doc.layers.get(i))
            .map_or(0, |l| l.id),
        object.unwrap_or(0),
    );
    let stale = cache
        .images
        .get(&target)
        .is_none_or(|(generation, _, _)| *generation != studio.canvas_gen);
    if stale
        && !cache.failed.contains(&(studio.canvas_gen, target))
        && !super::jobs::is_running::<Rendered>(ui.ctx(), JOB)
        && !ui.input(|i| i.pointer.any_down())
        && !studio.playing
    {
        let mut document = studio.doc.clone();
        document.collaboration = None;
        if let Some(index) = layer {
            let keep: HashSet<_> = std::iter::once(index)
                .chain(studio.doc.layer_ancestors(index))
                .chain(
                    (0..studio.doc.layers.len())
                        .filter(|&i| studio.doc.layer_ancestors(i).contains(&index)),
                )
                .collect();
            for (i, item) in document.layers.iter_mut().enumerate() {
                item.visible = keep.contains(&i);
            }
            if let Some(object) = object {
                if let Some(shapes) = document
                    .layers
                    .get_mut(index)
                    .and_then(|l| l.kind.shapes_mut())
                {
                    let mut parents = HashSet::new();
                    let mut parent = shapes
                        .iter()
                        .find(|s| s.id == object)
                        .and_then(|s| s.layout.parent);
                    while let Some(id) = parent {
                        if !parents.insert(id) {
                            break;
                        }
                        parent = shapes
                            .iter()
                            .find(|s| s.id == id)
                            .and_then(|s| s.layout.parent);
                    }
                    for shape in shapes {
                        shape.visible = shape.id == object || parents.contains(&shape.id);
                    }
                }
            }
            document.transparent = true;
        }
        let document_id = studio.swap_id.clone();
        let generation = studio.canvas_gen;
        let playhead = studio.is_motion().then_some(studio.playhead);
        cache.failed.retain(|(revision, _)| *revision == generation);
        cache.failed.insert((generation, target));
        super::jobs::start(ui.ctx(), JOB, move || {
            let proxy = super::chrome::document_previews::snapshot(&document)?;
            let pixels = super::chrome::document_previews::render(&proxy, playhead)?;
            Ok((document_id, generation, target, pixels))
        });
    }
    let texture = cache
        .images
        .get(&target)
        .map(|(_, texture, _)| texture.clone());
    if cache.images.len() > 160 {
        cache.images.retain(|key, _| *key == target);
    }
    ui.ctx().data_mut(|d| d.insert_temp(id, cache));
    texture
}

/// Draw a real layer thumbnail over a theme-derived transparency checker.
/// Args: UI, editor, layer and optional object. Returns the actual selectable thumbnail response.
pub(super) fn thumbnail(
    ui: &mut egui::Ui,
    studio: &Studio,
    layer: usize,
    object: Option<u64>,
) -> egui::Response {
    let size = egui::vec2(36.0, 30.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    let palette = super::theme::p();
    ui.painter().rect_filled(rect, 2.0, palette.bg_widget);
    for y in 0..5 {
        for x in 0..6 {
            if (x + y) % 2 == 0 {
                let tile = egui::Rect::from_min_size(
                    rect.min + egui::vec2(x as f32 * 6.0, y as f32 * 6.0),
                    egui::vec2(6.0, 6.0),
                );
                ui.painter().rect_filled(tile, 0.0, palette.bg_panel);
            }
        }
    }
    if let Some(texture) = image(ui, studio, Some(layer), object) {
        ui.painter().image(
            texture.id(),
            rect.shrink(1.0),
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
    ui.painter().rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(1.0, palette.border),
        egui::StrokeKind::Inside,
    );
    response.on_hover_text("Select artwork")
}

/// Read the bounded display pixels already rendered for a pane.
/// Args: context and current editor. Returns the current document preview without decoding source pixels.
pub(super) fn pixels(
    ctx: &egui::Context,
    studio: &Studio,
) -> Option<std::sync::Arc<egui::ColorImage>> {
    ctx.data(|d| d.get_temp::<Cache>(egui::Id::new("pane-thumbnails")))
        .filter(|cache| cache.document == studio.swap_id)
        .and_then(|cache| {
            cache
                .images
                .get(&(0, 0))
                .map(|(_, _, pixels)| pixels.clone())
        })
}
