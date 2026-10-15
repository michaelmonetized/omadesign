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

#[cfg(test)]
#[path = "browser_keyboard_tests.rs"]
mod keyboard_tests;

/// Focus the actual visible authoring surface after accepting an asset.
/// Args: context and studio. Returns no value; hidden surfaces never receive a focus request.
fn focus_document(ctx: &egui::Context, studio: &mut Studio) {
    let id = egui::Id::new(if studio.persona == crate::tools::Persona::Photo {
        "studio-photo-canvas"
    } else { "studio-canvas" });
    if let Some(response) = ctx.read_response(id).filter(|response| response.enabled()
        && ctx.memory(|memory| memory.allows_interaction(response.layer_id)))
    {
        response.request_focus();
    } else {
        super::panel_workspace::return_focus(ctx, studio);
    }
}

#[derive(Clone)]
struct BrowserOwner {
    id: egui::Id,
    document: String,
    focus: bool,
    focused: Option<(egui::Id, egui::LayerId)>,
}

/// Capture a native browser draft without changing its floating window identity.
/// Args: context, browser and document owner. Returns a stable open-instance scope.
fn owner(ctx: &egui::Context, browser: &str, document: &str) -> BrowserOwner {
    ctx.data(|data| data.get_temp::<BrowserOwner>(owner_key(browser)))
        .filter(|owner| owner.document == document)
        .unwrap_or_else(|| BrowserOwner {
            id: super::keyboard_controls::draft_id(ctx, browser),
            document: document.into(),
            focus: true,
            focused: None,
        })
}

/// Namespace app browser draft state separately from its existing worker keys.
/// Args: browser identity. Returns its transient UI-state key.
fn owner_key(browser: &str) -> egui::Id { egui::Id::new(("keyboard-browser-owner", browser)) }

/// Name an existing native search field and focus it when its window becomes interactive.
/// Args: UI, open owner, purpose and query. Returns its actual response.
fn query(ui: &mut egui::Ui, owner: &mut BrowserOwner, purpose: &str, value: &mut String) -> egui::Response {
    let label = ui.label(purpose);
    let response = ui.add(egui::TextEdit::singleline(value).id(owner.id.with("query"))
        .desired_width(190.)).labelled_by(label.id);
    super::welcome::name_control(ui, &response, label.id, purpose);
    if owner.focus && !ui.is_sizing_pass() && response.enabled()
        && ui.memory(|memory| memory.allows_interaction(response.layer_id))
    {
        response.request_focus();
        owner.focus = false;
    }
    response
}

/// Associate ordinary browser feedback with its real search control.
/// Args: UI, optional native search response and status. Returns no value.
fn status(ui: &mut egui::Ui, query: Option<&egui::Response>, text: &str) {
    let status = ui.label(RichText::new(text).small().color(theme::fg_weak()));
    ui.ctx().accesskit_node_builder(status.id, |node| {
        node.set_live(egui::accesskit::Live::Polite);
        node.set_live_atomic();
    });
    if let Some(query) = query {
        ui.ctx().accesskit_node_builder(query.id, |node| node.push_described_by(status.id.accesskit_id()));
    }
}

/// Retain the browser's actual native keyboard owner for the next frame.
/// Args: context, open draft and rendered window response. Returns no value.
fn observe_focus(ctx: &egui::Context, owner: &mut BrowserOwner, shown: Option<&egui::Response>) {
    owner.focused = ctx.memory(|memory| memory.focused())
        .and_then(|id| ctx.read_response(id))
        .filter(|response| response.enabled()
            && shown.is_some_and(|shown| shown.layer_id == response.layer_id)
            && ctx.memory(|memory| memory.allows_interaction(response.layer_id)))
        .map(|response| (response.id, response.layer_id));
}

/// Check the real previous-frame owner after native Escape has surrendered focus.
/// Args: context, browser and document identity. Returns whether that browser owns the key.
fn owns_escape(ctx: &egui::Context, browser: &str, document: &str) -> bool {
    let focused = ctx.data(|data| data.get_temp::<BrowserOwner>(owner_key(browser)))
        .filter(|owner| owner.document == document).and_then(|owner| owner.focused);
    focused.is_some_and(|(id, layer)| ctx.read_response(id).is_some_and(|response|
        response.enabled() && response.layer_id == layer)
        && ctx.memory(|memory| memory.had_focus_last_frame(id) && memory.allows_interaction(layer)))
}

/// Cancel only a browser-owned Escape before either canvas shortcut handler runs.
/// Args: context and studio. Returns whether both canvas handlers must skip this close frame.
pub(super) fn before_shortcuts(ctx: &egui::Context, studio: &mut Studio) -> bool {
    if studio.file_dialog_pending() || studio.updates.freezing || egui::Popup::is_any_open(ctx)
        || !ctx.input(|input| input.key_pressed(egui::Key::Escape))
    { return false; }
    let shapes = studio.show_shape_browser && owns_escape(ctx, SHAPE, &studio.swap_id);
    let photos = studio.show_asset_browser && owns_escape(ctx, PHOTO, &studio.swap_id);
    if !(shapes || photos)
        || !ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    { return false; }
    if shapes { close_shapes(ctx, studio); }
    if photos { close_photos(ctx, studio); }
    true
}

/// Close the shape browser through its existing worker cancellation path.
/// Args: context and studio. Returns no value; artwork and history stay unchanged.
fn close_shapes(ctx: &egui::Context, studio: &mut Studio) {
    studio.show_shape_browser = false;
    jobs::cancel::<ShapeResult>(ctx, SHAPE);
    studio.shape_status.clear();
    ctx.data_mut(|data| data.remove::<BrowserOwner>(owner_key(SHAPE)));
    super::panel_workspace::return_focus(ctx, studio);
}

/// Close the photo browser through its existing worker cancellation paths.
/// Args: context and studio. Returns no value; artwork and history stay unchanged.
fn close_photos(ctx: &egui::Context, studio: &mut Studio) {
    studio.show_asset_browser = false;
    jobs::cancel::<Vec<crate::asset_browser::AssetHit>>(ctx, SEARCH);
    jobs::cancel::<PhotoResult>(ctx, PHOTO);
    studio.asset_status.clear();
    ctx.data_mut(|data| data.remove::<BrowserOwner>(owner_key(PHOTO)));
    super::panel_workspace::return_focus(ctx, studio);
}

pub fn show_shape_browser(ui: &mut egui::Ui, studio: &mut Studio) {
    if let Some(result) = jobs::poll::<ShapeResult>(ui.ctx(), SHAPE) {
        match result {
            Ok((document, name, geom, target)) if document == studio.swap_id => {
                studio.place_palette_shape(geom, name, target);
                studio.show_shape_browser = false;
                studio.shape_status.clear();
                focus_document(ui.ctx(), studio);
            }
            Ok(_) => {
                studio.shape_status = "Document changed. Select the shape again to place it.".into()
            }
            Err(error) => studio.shape_status = error,
        }
    }
    if !studio.show_shape_browser {
        ui.ctx().data_mut(|data| data.remove::<BrowserOwner>(owner_key(SHAPE)));
        return;
    }
    let mut owner = owner(ui.ctx(), SHAPE, &studio.swap_id);
    let downloading = jobs::is_running::<ShapeResult>(ui.ctx(), SHAPE);
    let available = !studio.file_dialog_pending() && !studio.updates.freezing;
    let mut open = true;
    let shown = egui::Window::new("Shapes")
        .open(&mut open)
        .collapsible(false)
        .default_size([560.0, 480.0])
        .show(ui.ctx(), |ui| {
            super::fields::native_scope(ui, owner.id, |ui| ui.add_enabled_ui(available, |ui| {
            let mut search = None;
            ui.horizontal_wrapped(|ui| {
                let libraries = std::iter::once("All").chain(crate::shape_browser::libs().iter().copied())
                    .map(|library| (library.to_string(), library.to_string())).collect::<Vec<_>>();
                super::keyboard_controls::choice(ui, "shape-lib", "Shape collection", &mut studio.shape_lib, &libraries);
                search = Some(query(ui, &mut owner, "Search shapes", &mut studio.shape_query));
                let clear = super::keyboard_controls::button(ui, "clear", "Clear");
                if clear.clicked() {
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
                                let response = super::fields::native_scope(ui, owner.id.with(("shape", icon.lib, icon.name)), |ui| ui
                                    .add_enabled(
                                        !downloading,
                                        egui::Button::new(icon.name)
                                            .min_size(egui::vec2(120.0, 28.0)),
                                    )).inner.on_hover_text(format!("{} · {}", icon.lib, icon.name));
                                response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button,
                                    response.enabled(), format!("Add {} shape {}", icon.lib, icon.name)));
                                super::welcome::focus_control(ui, &response);
                                if response.clicked() {
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
                status(ui, search.as_ref(), &studio.shape_status);
            });
            }));
        });
    observe_focus(ui.ctx(), &mut owner, shown.as_ref().map(|shown| &shown.response));
    ui.ctx().data_mut(|data| data.insert_temp(owner_key(SHAPE), owner));
    if !open {
        close_shapes(ui.ctx(), studio);
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
                focus_document(ui.ctx(), studio);
            }
            Ok(_) => {
                studio.asset_status = "Document changed. Select the photo again to place it.".into()
            }
            Err(error) => studio.asset_status = error,
        }
    }
    if !studio.show_asset_browser {
        ui.ctx().data_mut(|data| data.remove::<BrowserOwner>(owner_key(PHOTO)));
        return;
    }
    let mut owner = owner(ui.ctx(), PHOTO, &studio.swap_id);
    let available = !studio.file_dialog_pending() && !studio.updates.freezing;
    let searching = jobs::is_running::<Vec<crate::asset_browser::AssetHit>>(ui.ctx(), SEARCH);
    let downloading = jobs::is_running::<PhotoResult>(ui.ctx(), PHOTO);
    let mut open = true;
    let shown = egui::Window::new("Photos")
        .open(&mut open)
        .collapsible(false)
        .default_size([640.0, 480.0])
        .show(ui.ctx(), |ui| {
            super::fields::native_scope(ui, owner.id, |ui| ui.add_enabled_ui(available, |ui| {
            let mut search = None;
            ui.horizontal_wrapped(|ui| {
                let providers = crate::asset_browser::providers().iter()
                    .map(|provider| (provider.to_string(), provider.to_string())).collect::<Vec<_>>();
                super::keyboard_controls::choice(ui, "asset-provider", "Photo provider", &mut studio.asset_provider, &providers);
                let query = query(ui, &mut owner, "Search photos", &mut studio.asset_query);
                let submit = query.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                search = Some(query);
                let find = ui.add_enabled(!searching, egui::Button::new("Search"));
                find.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, find.enabled(), "Find photos"));
                super::welcome::focus_control(ui, &find);
                if find.clicked() || submit && !searching {
                    studio.asset_results.clear();
                    studio.asset_status = "Searching…".into();
                    let query = studio.asset_query.clone();
                    let provider = studio.asset_provider.clone();
                    jobs::start(ui.ctx(), SEARCH, move || crate::asset_browser::search(&query, &provider, 12));
                }
                if super::keyboard_controls::button(ui, "clear", "Clear").clicked() {
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
                status(ui, search.as_ref(), &studio.asset_status);
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
                            let response = super::fields::native_scope(ui, owner.id.with(("photo", &hit.provider, &hit.id, &hit.full_url)),
                                |ui| ui.add_enabled(!downloading, egui::Button::new("Add to document"))).inner;
                            response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, response.enabled(),
                                format!("Add {} photo {}", hit.provider, hit.tags)));
                            super::welcome::focus_control(ui, &response);
                            if response.clicked() {
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
            }));
        });
    observe_focus(ui.ctx(), &mut owner, shown.as_ref().map(|shown| &shown.response));
    ui.ctx().data_mut(|data| data.insert_temp(owner_key(PHOTO), owner));
    if !open {
        close_photos(ui.ctx(), studio);
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
