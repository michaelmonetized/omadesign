use super::jobs;
use crate::app::Studio;
use crate::document::{Cmd, Layer, Shape};
use crate::geom::{Geom, Pt};
use crate::ui::theme;
use eframe::egui::{self, RichText, ScrollArea};

const SHAPE: &str = "shape-download";
const SEARCH: &str = "photo-search";
const PHOTO: &str = "photo-download";
type ShapeResult = (String, String, Geom);
type PhotoResult = (String, Layer);

pub fn show_shape_browser(ui: &mut egui::Ui, studio: &mut Studio) {
    if let Some(result) = jobs::poll::<ShapeResult>(ui.ctx(), SHAPE) {
        match result {
            Ok((document, name, mut geom)) if document == studio.swap_id => {
                let center = Pt::new(studio.doc.width * 0.5, studio.doc.height * 0.5);
                geom.translate(center - geom.bbox().center());
                let mut style = studio.style.clone();
                style.stroke = None;
                if style.fill.is_none() {
                    style.fill = crate::document::Fill::Solid(studio.brush.color);
                }
                let shape = Shape::new(geom, style);
                let id = shape.id;
                if let Some(layer) = studio.vector_target() {
                    studio.commit(Cmd::AddShape { layer, shape });
                    studio.selection = vec![(layer, id)];
                    studio.show_shape_browser = false;
                    studio.status = format!("Added {name}");
                    studio.shape_status.clear();
                } else {
                    studio.shape_status = "Add a vector layer to place this shape.".into();
                }
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
                                if ui
                                    .add_enabled(
                                        !downloading,
                                        egui::Button::new(icon.name)
                                            .min_size(egui::vec2(120.0, 34.0)),
                                    )
                                    .on_hover_text(format!("{} · {}", icon.lib, icon.name))
                                    .clicked()
                                {
                                    studio.shape_status = format!("Loading {}…", icon.name);
                                    let icon = icon.clone();
                                    let document = studio.swap_id.clone();
                                    jobs::start(ui.ctx(), SHAPE, move || {
                                        let geom =
                                            crate::shape_browser::icon_to_geom(&icon, 256.0)?;
                                        Ok((document, icon.name.to_string(), geom))
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
            Ok((document, layer)) if document == studio.swap_id => {
                let index = studio.doc.layers.len();
                let name = layer.name.clone();
                studio.commit(Cmd::AddLayer { index, layer });
                studio.active_layer = Some(index);
                studio.status = format!("Added {name}");
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
            let mut chosen = None;
            ScrollArea::vertical().max_height(330.0).show(ui, |ui| {
                if studio.asset_results.is_empty() && !searching {
                    ui.weak("Find a photo to add to your document.");
                }
                egui::Grid::new("asset-grid").num_columns(3).spacing([16.0, 16.0]).show(ui, |ui| {
                    for (i, hit) in studio.asset_results.iter().enumerate() {
                        ui.vertical(|ui| {
                            ui.set_width(175.0);
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
                jobs::start(ui.ctx(), PHOTO, move || {
                    let bytes = crate::asset_browser::download(&hit)?;
                    let full = crate::photo::decode_bytes(&bytes).ok_or("Could not decode this photo.")?;
                    let name = format!("{} {}", hit.provider, hit.tags.chars().take(32).collect::<String>());
                    let mut layer = Layer::raster(name, full.w, full.h);
                    if let crate::document::LayerKind::Raster { pixels, .. } = &mut layer.kind {
                        *pixels = crate::document::Pixels::from_rgba(full.w, full.h, full.data)
                            .ok_or("Invalid photo dimensions.")?;
                    }
                    Ok((document, layer))
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
