use super::jobs;
use crate::app::Studio;
use crate::document::{Cmd, Layer};
use crate::geom::{Geom, Pt};
use crate::ui::{icons, theme};
use eframe::egui::{self, Align2, RichText, ScrollArea};

const SHAPE: &str = "shape-download";
const SEARCH: &str = "photo-search";
const PHOTO: &str = "photo-download";
type FrameTarget = Option<(usize, u64)>;
type ShapeResult = (String, String, Geom, FrameTarget);
type PhotoResult = (String, Layer, FrameTarget);

pub fn show_shape_browser(ui: &mut egui::Ui, studio: &mut Studio) {
    if let Some(result) = jobs::poll::<ShapeResult>(ui.ctx(), SHAPE) {
        match result {
            Ok((document, name, geom, target)) if document == studio.swap_id => {
                studio.place_palette_shape(geom, name, target);
                studio.show_shape_browser = false;
                studio.shape_status.clear();
            }
            Ok(_) => {
                studio.shape_status = "Document changed. Select the shape again to place it.".into()
            }
            Err(error) => studio.shape_status = error,
        }
    }
    if !studio.show_shape_browser {
        return;
    }
    let downloading = jobs::is_running::<ShapeResult>(ui.ctx(), SHAPE);
    let mut open = true;
    egui::Window::new("Shapes")
        .open(&mut open)
        .collapsible(false)
        .default_size([560.0, 480.0])
        .show(ui.ctx(), |ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("shape-lib")
                    .selected_text(&studio.shape_lib)
                    .width(110.0)
                    .show_ui(ui, |ui| {
                        for lib in std::iter::once("All")
                            .chain(crate::shape_browser::libs().iter().copied())
                        {
                            ui.selectable_value(&mut studio.shape_lib, lib.to_string(), lib);
                        }
                    });
                ui.add(
                    egui::TextEdit::singleline(&mut studio.shape_query)
                        .hint_text("Search shapes…")
                        .desired_width(240.0),
                );
                if ui.small_button("Clear").clicked() {
                    studio.shape_query.clear();
                }
            });
            ui.add_space(12.0);
            let icons = crate::shape_browser::search(&studio.shape_query, &studio.shape_lib, 80);
            ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                if icons.is_empty() {
                    ui.weak("No matching shapes. Try another search or library.");
                }
                egui::Grid::new("shape-grid")
                    .num_columns(4)
                    .spacing([8.0, 8.0])
                    .show(ui, |ui| {
                        for (i, icon) in icons.iter().enumerate() {
                            ui.vertical(|ui| {
                                let (thumb, _) =
                                    ui.allocate_exact_size(egui::vec2(120.0, 44.0), egui::Sense::hover());
                                if icon.lib == "Phosphor"
                                    && let Some(glyph) = crate::shape_browser::phosphor_char(icon.name)
                                {
                                    ui.painter().text(
                                        thumb.center(),
                                        Align2::CENTER_CENTER,
                                        glyph.to_string(),
                                        icons::font(28.0),
                                        theme::fg(),
                                    );
                                } else {
                                    ui.painter().rect_stroke(
                                        thumb.shrink(6.0),
                                        4.0,
                                        egui::Stroke::new(1.0, theme::border()),
                                        egui::StrokeKind::Inside,
                                    );
                                }
                                if ui
                                    .add_enabled(
                                        !downloading,
                                        egui::Button::new(icon.name)
                                            .min_size(egui::vec2(120.0, 28.0)),
                                    )
                                    .on_hover_text(format!("{} · {}", icon.lib, icon.name))
                                    .clicked()
                                {
                                    studio.shape_status = format!("Loading {}…", icon.name);
                                    let icon = icon.clone();
                                    let document = studio.swap_id.clone();
                                    let target = studio.asset_frame_target();
                                    jobs::start(ui.ctx(), SHAPE, move || {
                                        let geom =
                                            crate::shape_browser::icon_to_geom(&icon, 256.0)?;
                                        Ok((document, icon.name.to_string(), geom, target))
                                    });
                                }
                                ui.label(RichText::new(icon.lib).small().color(theme::fg_weak()));
                            });
                            if (i + 1) % 4 == 0 {
                                ui.end_row();
                            }
                        }
                    });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if downloading {
                    ui.spinner();
                }
                ui.label(
                    RichText::new(&studio.shape_status)
                        .small()
                        .color(theme::fg_weak()),
                );
            });
        });
    if !open {
        studio.show_shape_browser = false;
        jobs::cancel::<ShapeResult>(ui.ctx(), SHAPE);
        studio.shape_status.clear();
    }
}

pub fn show_asset_browser(ui: &mut egui::Ui, studio: &mut Studio) {
    if let Some(result) = jobs::poll::<Vec<crate::asset_browser::AssetHit>>(ui.ctx(), SEARCH) {
        match result {
            Ok(results) => {
                studio.asset_status = format!("{} photos", results.len());
                studio.asset_results = results;
            }
            Err(error) => studio.asset_status = error,
        }
    }
    if let Some(result) = jobs::poll::<PhotoResult>(ui.ctx(), PHOTO) {
        match result {
            Ok((document, layer, target)) if document == studio.swap_id => {
                let name = layer.name.clone();
                if let Some((li, id)) = target {
                    let at = studio.asset_frame_center((li, id)).unwrap_or(Pt::ZERO);
                    if let crate::document::LayerKind::Raster { pixels, .. } = layer.kind {
                        let imported = crate::import::Imported::Raster {
                            name,
                            image: crate::photo::RgbaImage {
                                w: pixels.w,
                                h: pixels.h,
                                data: pixels.data,
                            },
                        };
                        if let Err(error) =
                            studio.place_brand_imported_in_frame(imported, at, target)
                        {
                            studio.asset_status = error;
                            return;
                        }
                    }
                } else if let crate::document::LayerKind::Raster { pixels, .. } = layer.kind {
                    let board = studio
                        .doc
                        .artboards
                        .iter()
                        .find(|board| board.size.x > 1.0 && board.size.y > 1.0)
                        .map(|board| board.local_bounds())
                        .unwrap_or_else(|| {
                            crate::geom::Bounds::from_min_size(
                                crate::geom::Pt::ZERO,
                                crate::geom::Pt::new(studio.doc.width, studio.doc.height),
                            )
                        });
                    let native = crate::geom::Pt::new(pixels.w.max(1) as f32, pixels.h.max(1) as f32);
                    let scale = (board.width() * 0.8 / native.x)
                        .min(board.height() * 0.8 / native.y)
                        .min(1.0)
                        .max(0.01);
                    let size = native * scale;
                    let origin = board.center() - size * 0.5;
                    let placed = Layer::placed_raster(name.clone(), pixels, origin, size);
                    let index = studio.doc.layers.len();
                    studio.commit(Cmd::AddLayer { index, layer: placed });
                    studio.active_layer = Some(index);
                    studio.selected_layer = None;
                    studio.selection = vec![(index, crate::document::RASTER_ID)];
                    studio.tool = crate::tools::Tool::Select;
                    studio.status = format!("Added {name}");
                }
                studio.asset_status.clear();
                studio.show_asset_browser = false;
            }
            Ok(_) => {
                studio.asset_status = "Document changed. Select the photo again to place it.".into()
            }
            Err(error) => studio.asset_status = error,
        }
    }
    if !studio.show_asset_browser {
        return;
    }
    let searching = jobs::is_running::<Vec<crate::asset_browser::AssetHit>>(ui.ctx(), SEARCH);
    let downloading = jobs::is_running::<PhotoResult>(ui.ctx(), PHOTO);
    let mut open = true;
    egui::Window::new("Photos")
        .open(&mut open)
        .collapsible(false)
        .default_size([640.0, 480.0])
        .show(ui.ctx(), |ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("asset-provider")
                    .selected_text(&studio.asset_provider).width(100.0)
                    .show_ui(ui, |ui| {
                        for provider in crate::asset_browser::providers() {
                            ui.selectable_value(&mut studio.asset_provider, provider.to_string(), *provider);
                        }
                    });
                let query = ui.add(egui::TextEdit::singleline(&mut studio.asset_query)
                    .hint_text("Search photos…").desired_width(220.0));
                let submit = query.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.add_enabled(!searching, egui::Button::new("Search")).clicked() || submit && !searching {
                    studio.asset_results.clear();
                    studio.asset_status = "Searching…".into();
                    let query = studio.asset_query.clone();
                    let provider = studio.asset_provider.clone();
                    jobs::start(ui.ctx(), SEARCH, move || crate::asset_browser::search(&query, &provider, 12));
                }
                if ui.small_button("Clear").clicked() {
                    jobs::cancel::<Vec<crate::asset_browser::AssetHit>>(ui.ctx(), SEARCH);
                    studio.asset_query.clear();
                    studio.asset_results.clear();
                    studio.asset_status.clear();
                }
            });
            ui.add_space(8.0);
            ui.label(RichText::new("Search Pixabay and Pexels. Picsum provides sample photos.")
                .small().color(theme::fg_weak()));
            ui.collapsing("Connect photo providers", |ui| {
                ui.weak("Add PIXABAY_API_KEY or PEXELS_API_KEY to your environment, or add pixabay_api_key / pexels_api_key to ~/.config/omadesign/assets.toml.");
            });
            ui.horizontal(|ui| {
                if searching || downloading { ui.spinner(); }
                ui.label(RichText::new(&studio.asset_status).small().color(theme::fg_weak()));
            });
            ui.add_space(8.0);
            let thumbs = photo_thumbs(ui, &studio.asset_results);
            let mut chosen = None;
            ScrollArea::vertical().max_height(330.0).show(ui, |ui| {
                if studio.asset_results.is_empty() && !searching {
                    ui.weak("Find a photo to add to your document.");
                }
                egui::Grid::new("asset-grid").num_columns(3).spacing([16.0, 16.0]).show(ui, |ui| {
                    for (i, hit) in studio.asset_results.iter().enumerate() {
                        ui.vertical(|ui| {
                            ui.set_width(175.0);
                            if let Some(texture) = thumbs.get(&hit.thumb_url) {
                                ui.image((texture.id(), egui::vec2(160.0, 100.0)));
                            } else {
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(160.0, 100.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(rect, 4.0, theme::bg_widget());
                            }
                            ui.label(RichText::new(&hit.tags).size(13.0));
                            ui.label(RichText::new(format!("{} · {} × {}", hit.provider, hit.w, hit.h))
                                .small().color(theme::fg_weak()));
                            if ui.add_enabled(!downloading, egui::Button::new("Add to document")).clicked() {
                                chosen = Some(hit.clone());
                            }
                        });
                        if (i + 1) % 3 == 0 { ui.end_row(); }
                    }
                });
            });
            if let Some(hit) = chosen {
                studio.asset_status = "Downloading photo…".into();
                let document = studio.swap_id.clone();
                let target = studio.asset_frame_target();
                jobs::start(ui.ctx(), PHOTO, move || {
                    let bytes = crate::asset_browser::download(&hit)?;
                    let full = crate::photo::decode_bytes(&bytes).ok_or("Could not decode this photo.")?;
                    let name = format!("{} {}", hit.provider, hit.tags.chars().take(32).collect::<String>());
                    let mut layer = Layer::raster(name, full.w, full.h);
                    if let crate::document::LayerKind::Raster { pixels, .. } = &mut layer.kind {
                        *pixels = crate::document::Pixels::from_rgba(full.w, full.h, full.data)
                            .ok_or("Invalid photo dimensions.")?;
                    }
                    Ok((document, layer, target))
                });
            }
        });
    if !open {
        studio.show_asset_browser = false;
        jobs::cancel::<Vec<crate::asset_browser::AssetHit>>(ui.ctx(), SEARCH);
        jobs::cancel::<PhotoResult>(ui.ctx(), PHOTO);
        studio.asset_status.clear();
    }
}

const THUMB: &str = "photo-thumb";

fn photo_thumbs(
    ui: &mut egui::Ui,
    hits: &[crate::asset_browser::AssetHit],
) -> std::collections::HashMap<String, egui::TextureHandle> {
    let id = egui::Id::new("photo-thumb-cache");
    if let Some(Ok((url, bytes))) = jobs::poll::<(String, Vec<u8>)>(ui.ctx(), THUMB) {
        if let Some(image) = decode_thumb(&bytes) {
            let texture = ui.ctx().load_texture(&url, image, egui::TextureOptions::LINEAR);
            ui.ctx().data_mut(|data| {
                let mut cache = data
                    .get_temp::<std::collections::HashMap<String, egui::TextureHandle>>(id)
                    .unwrap_or_default();
                cache.insert(url, texture);
                data.insert_temp(id, cache);
            });
        }
    }
    let cache = ui
        .ctx()
        .data(|data| data.get_temp::<std::collections::HashMap<String, egui::TextureHandle>>(id))
        .unwrap_or_default();
    if !jobs::is_running::<(String, Vec<u8>)>(ui.ctx(), THUMB)
        && let Some(hit) = hits
            .iter()
            .find(|hit| !hit.thumb_url.is_empty() && !cache.contains_key(&hit.thumb_url))
    {
        let url = hit.thumb_url.clone();
        jobs::start(ui.ctx(), THUMB, move || {
            crate::asset_browser::fetch_thumb(&url).map(|bytes| (url, bytes))
        });
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(200));
    }
    cache
}

fn decode_thumb(bytes: &[u8]) -> Option<egui::ColorImage> {
    let image = image::load_from_memory(bytes).ok()?.thumbnail(320, 200);
    let rgba = image.to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Document, Shape, Style};
    use crate::tools::Persona;
    use std::time::{Duration, Instant};

    #[test]
    fn palette_completions_keep_the_frame_selected_when_the_asset_was_chosen() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut studio = Studio::new();
        studio.doc = Document::new("Palette destination", 1.0, 1.0, 96.0);
        studio.doc.width = 640.0;
        studio.doc.height = 480.0;
        studio.doc.layers = vec![Layer::vector("First"), Layer::vector("Second")];
        studio.persona = Persona::Layout;
        studio.show_welcome = false;
        let first = crate::layout::make_frame(Pt::new(20.0, 20.0), Pt::new(200.0, 200.0));
        let second = crate::layout::make_frame(Pt::new(280.0, 20.0), Pt::new(200.0, 200.0));
        let target_id = first.id;
        let second_id = second.id;
        let mut child = Shape::new(
            Geom::Rect {
                origin: Pt::new(40.0, 40.0),
                size: Pt::new(20.0, 20.0),
                radius: 0.0,
            },
            Style::default(),
        );
        child.layout.parent = Some(target_id);
        studio.selection = vec![(0, child.id)];
        studio.active_layer = Some(0);
        studio.doc.layers[0]
            .kind
            .shapes_mut()
            .unwrap()
            .extend([first, child]);
        studio.doc.layers[1].kind.shapes_mut().unwrap().push(second);
        let target = studio.asset_frame_target();
        assert_eq!(target, Some((0, target_id)));
        let owner = studio.swap_id.clone();
        jobs::start(&ctx, SHAPE, move || {
            Ok((
                owner,
                "Loaded icon".to_owned(),
                Geom::Rect {
                    origin: Pt::ZERO,
                    size: Pt::new(32.0, 32.0),
                    radius: 0.0,
                },
                target,
            ))
        });
        let owner = studio.swap_id.clone();
        jobs::start(&ctx, PHOTO, move || {
            let mut layer = Layer::raster("Loaded photo", 2, 2);
            if let crate::document::LayerKind::Raster { pixels, .. } = &mut layer.kind {
                pixels.data = [240, 90, 20, 255].repeat(4);
            }
            Ok((owner, layer, target))
        });
        // A user's next selection cannot redirect an asset that is already loading.
        studio.selection = vec![(1, second_id)];
        studio.active_layer = Some(1);
        let deadline = Instant::now() + Duration::from_secs(5);
        while studio.history.len() < 2 {
            assert!(Instant::now() < deadline, "Palette completion timed out");
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                show_shape_browser(ui, &mut studio);
                show_asset_browser(ui, &mut studio);
            });
            output.textures_delta.clear();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(studio.doc.layers.len(), 2);
        let shapes = studio.doc.layers[0].kind.shapes().unwrap();
        for name in ["Loaded icon", "Loaded photo"] {
            let placed = shapes.iter().find(|shape| shape.name == name).unwrap();
            assert_eq!(placed.layout.parent, Some(target_id));
        }
        assert_eq!(studio.doc.layers[1].kind.shapes().unwrap().len(), 1);
        studio.doc.validate_hierarchy().unwrap();
        studio.undo();
        studio.undo();
        assert_eq!(studio.doc.layers[0].kind.shapes().unwrap().len(), 2);
    }
}
