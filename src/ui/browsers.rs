use crate::app::Studio;
use crate::document::{Cmd, Layer, Shape, Style};
use crate::geom::{Geom, Pt};
use eframe::egui::{self, RichText, ScrollArea};

pub fn show_shape_browser(ui: &mut egui::Ui, studio: &mut Studio) {
    if !studio.show_shape_browser {
        return;
    }
    egui::Window::new("Shape Browser")
        .collapsible(false)
        .resizable(true)
        .default_size([560.0, 520.0])
        .show(ui.ctx(), |ui| {
            ui.horizontal(|ui| {
                ui.label("Library");
                egui::ComboBox::from_id_salt("shape-lib")
                    .selected_text(studio.shape_lib.clone())
                    .width(120.0)
                    .show_ui(ui, |ui| {
                        for lib in ["All", "Phosphor", "LineIcons", "Heroicons", "Feather", "Lucide"] {
                            ui.selectable_value(&mut studio.shape_lib, lib.to_string(), lib);
                        }
                    });
                ui.add(
                    egui::TextEdit::singleline(&mut studio.shape_query)
                        .hint_text("Search heart, arrow…")
                        .desired_width(180.0),
                );
                if ui.small_button("Clear").clicked() {
                    studio.shape_query.clear();
                }
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new("Popular OSS libs: Phosphor (Light), LineIcons, Heroicons, Feather, Lucide. Click Add to pull the SVG onto the artboard.")
                    .small()
                    .color(crate::ui::theme::fg_weak()),
            );
            let icons = crate::shape_browser::search(&studio.shape_query, &studio.shape_lib, 80);
            ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                egui::Grid::new("shape-grid").num_columns(4).spacing([8.0, 8.0]).show(ui, |ui| {
                    for (i, ic) in icons.iter().enumerate() {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(ic.lib).small().color(crate::ui::theme::fg_weak()));
                            if ui
                                .add_sized([120.0, 28.0], egui::Button::new(format!("{}  +", ic.name)))
                                .on_hover_text(format!("{}/{} – click to add", ic.lib, ic.name))
                                .clicked()
                            {
                                studio.shape_status = format!("Fetching {}…", ic.name);
                                match crate::shape_browser::fetch_svg(ic) {
                                    Ok(svg) => match crate::shape_browser::svg_to_geom(&svg, 256.0) {
                                        Ok(mut geom) => {
                                            // Place at document centre
                                            let doc = &studio.doc;
                                            let centre = Pt::new(doc.width * 0.5, doc.height * 0.5);
                                            // Icons are 0..256, centre them
                                            let bbox = geom.bbox();
                                            let off = centre - bbox.center();
                                            geom.translate(off);
                                            // Style uses current fill/stroke
                                            let shape = Shape::new(geom, studio.style.clone());
                                            let id = shape.id;
                                            if let Some(li) = studio.vector_target() {
                                                studio.commit(Cmd::AddShape { layer: li, shape });
                                                studio.selection = vec![(li, id)];
                                                studio.show_shape_browser = false;
                                                studio.status = format!("added shape {}", ic.name);
                                                studio.shape_status = format!("Added {} to artboard", ic.name);
                                            } else {
                                                studio.shape_status = "add a vector layer first".into();
                                            }
                                        }
                                        Err(e) => studio.shape_status = format!("SVG parse failed: {e}"),
                                    },
                                    Err(e) => studio.shape_status = e,
                                }
                            }
                            ui.label(RichText::new(ic.name).small());
                        });
                        if (i + 1) % 4 == 0 {
                            ui.end_row();
                        }
                    }
                });
            });
            if !studio.shape_status.is_empty() {
                ui.label(RichText::new(&studio.shape_status).small().color(crate::ui::theme::accent()));
            }
            ui.horizontal(|ui| {
                if ui.small_button("Close").clicked() {
                    studio.show_shape_browser = false;
                }
                ui.label(
                    RichText::new("Tip: shapes are added at the artboard centre. Scale with handles.")
                        .small()
                        .color(crate::ui::theme::fg_weak()),
                );
            });
        });
}

pub fn show_asset_browser(ui: &mut egui::Ui, studio: &mut Studio) {
    if !studio.show_asset_browser {
        return;
    }
    egui::Window::new("Free Asset Browser")
        .collapsible(false)
        .resizable(true)
        .default_size([640.0, 560.0])
        .show(ui.ctx(), |ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("asset-provider")
                    .selected_text(studio.asset_provider.clone())
                    .width(110.0)
                    .show_ui(ui, |ui| {
                        for p in crate::asset_browser::providers() {
                            ui.selectable_value(&mut studio.asset_provider, p.to_string(), *p);
                        }
                    });
                ui.add(
                    egui::TextEdit::singleline(&mut studio.asset_query)
                        .hint_text("Search nature, city, abstract…")
                        .desired_width(200.0),
                );
                if ui.button("Search").clicked() {
                    studio.asset_status = "Searching…".into();
                    studio.asset_results = crate::asset_browser::search(&studio.asset_query, &studio.asset_provider, 12);
                    studio.asset_status = format!("found {} assets", studio.asset_results.len());
                }
                if ui.small_button("Clear").clicked() {
                    studio.asset_query.clear();
                    studio.asset_results.clear();
                    studio.asset_status.clear();
                }
            });
            ui.label(
                RichText::new("Free libs: Pixabay, Pexels, Vecteezy, Vexels (Picsum fallback). Set PIXABAY_API_KEY / PEXELS_API_KEY or ~/.config/omadesign/assets.toml to enable live search.")
                    .small()
                    .color(crate::ui::theme::fg_weak()),
            );
            if !studio.asset_status.is_empty() {
                ui.label(RichText::new(&studio.asset_status).small().color(crate::ui::theme::accent()));
            }
            ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                if studio.asset_results.is_empty() {
                    ui.label(
                        RichText::new("No results yet – type a query and hit Search. Try “nature” or “technology”.")
                            .small()
                            .color(crate::ui::theme::fg_weak()),
                    );
                    return;
                }
                egui::Grid::new("asset-grid").num_columns(3).spacing([8.0, 8.0]).show(ui, |ui| {
                    let results = studio.asset_results.clone();
                    for (i, hit) in results.iter().enumerate() {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&hit.provider).small().color(crate::ui::theme::fg_weak()));
                            ui.label(RichText::new(&hit.tags).small());
                            ui.label(RichText::new(format!("{}×{}", hit.w, hit.h)).small().color(crate::ui::theme::fg_weak()));
                            if ui.small_button("Add to artboard").clicked() {
                                studio.asset_status = format!("Downloading {}…", hit.id);
                                match crate::asset_browser::download(hit) {
                                    Ok(bytes) => {
                                        // Decode and place as raster layer
                                        match crate::photo::decode_bytes(&bytes) {
                                            Some(full) => {
                                                let name = format!("{} {}", hit.provider, hit.tags.chars().take(20).collect::<String>());
                                                let mut layer = Layer::raster(name.clone(), full.w, full.h);
                                                if let crate::document::LayerKind::Raster { pixels } = &mut layer.kind {
                                                    *pixels = crate::document::Pixels::from_rgba(full.w, full.h, full.data).unwrap_or_else(|| crate::document::Pixels::new(full.w, full.h));
                                                }
                                                let idx = studio.doc.layers.len();
                                                studio.commit(Cmd::AddLayer { index: idx, layer });
                                                studio.active_layer = Some(idx);
                                                studio.asset_status = format!("added {} to layer {}", hit.id, name);
                                                studio.show_asset_browser = false;
                                            }
                                            None => studio.asset_status = "failed to decode image".into(),
                                        }
                                    }
                                    Err(e) => studio.asset_status = e,
                                }
                            }
                        });
                        if (i + 1) % 3 == 0 {
                            ui.end_row();
                        }
                    }
                });
            });
            ui.horizontal(|ui| {
                if ui.small_button("Close").clicked() {
                    studio.show_asset_browser = false;
                }
                ui.label(
                    RichText::new("Assets are placed as new pixel layers at full resolution.")
                        .small()
                        .color(crate::ui::theme::fg_weak()),
                );
            });
        });
}
