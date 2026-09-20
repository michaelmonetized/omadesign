//! Pixel-mode filter/effect banks. Preview never mutates the document.
use crate::app::Studio;
use crate::color::Rgba;
use crate::document::{Cmd, apply as apply_command};
use crate::raster::{Bank, Kind, Settings};
use eframe::egui::{
    self, Color32, ColorImage, Context, Id, RichText, Sense, Slider, TextureHandle, TextureOptions,
    Ui, vec2,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Job = Arc<Mutex<Option<Result<Vec<u8>, String>>>>;

#[derive(Clone)]
struct Session {
    document: String,
    layer_id: u64,
    name: String,
    mask: bool,
    w: u32,
    h: u32,
    source: Arc<Vec<u8>>,
    selection: Option<Arc<Vec<u8>>>,
    settings: Settings,
    preview_w: u32,
    preview_h: u32,
    preview_source: Arc<Vec<u8>>,
    preview_selection: Option<Arc<Vec<u8>>>,
    original_texture: TextureHandle,
    texture: TextureHandle,
    preview_job: Option<(Settings, Job)>,
    rendered: Option<Settings>,
    apply_job: Option<Job>,
    show_original: bool,
    sample: bool,
    error: Option<String>,
}

fn state_id() -> Id {
    Id::new("pixel-raster-filter-session")
}
pub(super) fn is_open(ctx: &Context) -> bool {
    ctx.data(|d| d.get_temp::<Session>(state_id()).is_some())
}

pub(super) fn ready(ctx: &Context) -> bool {
    ctx.data(|d| {
        d.get_temp::<Session>(state_id())
            .is_none_or(|s| s.preview_job.is_none() && s.rendered.as_ref() == Some(&s.settings))
    })
}

pub(super) fn inspector(ui: &mut Ui, studio: &mut Studio) {
    let editable = studio.active_layer.is_some_and(|i| {
        studio.doc.layer_editable(i)
            && studio.doc.layers.get(i).is_some_and(|l| {
                if studio.paint_mask {
                    l.mask.is_some()
                } else {
                    l.kind.pixels().is_some()
                }
            })
    });
    ui.label(RichText::new("Raster studio").strong().size(12.0));
    let mut chosen = None;
    ui.add_enabled_ui(editable, |ui| {
        ui.horizontal(|ui| {
            for (label, bank) in [("Filters", Bank::Filters), ("Effects", Bank::Effects)] {
                ui.menu_button(label, |ui| {
                    for category in [
                        "Key & transparency",
                        "Color & tone",
                        "Blur",
                        "Detail",
                        "Stylize",
                        "Distort",
                    ] {
                        if !Kind::ALL
                            .iter()
                            .any(|kind| kind.bank() == bank && kind.category() == category)
                        {
                            continue;
                        }
                        ui.menu_button(category, |ui| {
                            for kind in Kind::ALL
                                .into_iter()
                                .filter(|kind| kind.bank() == bank && kind.category() == category)
                            {
                                if ui
                                    .button(kind.name())
                                    .on_hover_text(kind.description())
                                    .clicked()
                                {
                                    chosen = Some(kind);
                                    ui.close();
                                }
                            }
                        });
                    }
                });
            }
        });
        if ui
            .button("Chroma key…")
            .on_hover_text("Remove green, blue, or any sampled background color")
            .clicked()
        {
            chosen = Some(Kind::ChromaKey);
        }
    });
    if !editable {
        ui.small("Select an unlocked pixel layer or editable layer mask.");
    }
    if let Some(kind) = chosen {
        open(ui.ctx(), studio, kind);
    }
}

pub(super) fn open(ctx: &Context, studio: &mut Studio, kind: Kind) {
    open_with_settings(ctx, studio, Settings::new(kind));
}

pub(super) fn open_with_settings(ctx: &Context, studio: &mut Studio, settings: Settings) {
    studio.end_pixel_stroke(false);
    let Some(index) = studio.active_layer else {
        return;
    };
    if !studio.doc.layer_editable(index) {
        return;
    }
    let layer = &studio.doc.layers[index];
    let Some(pixels) = (if studio.paint_mask {
        layer.mask.as_ref()
    } else {
        layer.kind.pixels()
    }) else {
        return;
    };
    if pixels.w == 0
        || pixels.h == 0
        || pixels.data.len() != pixels.w as usize * pixels.h as usize * 4
    {
        return;
    }
    let selection = studio.pixel_sel.clone();
    if selection
        .as_ref()
        .is_some_and(|m| m.len() != pixels.w as usize * pixels.h as usize)
    {
        studio.status = "Clear the old pixel selection before filtering this layer".into();
        return;
    }
    let scale = (600.0 / pixels.w.max(pixels.h) as f32).min(1.0);
    let pw = (pixels.w as f32 * scale).round().max(1.0) as u32;
    let ph = (pixels.h as f32 * scale).round().max(1.0) as u32;
    let source = Arc::new(pixels.data.clone());
    let preview_source = Arc::new(thumbnail(&source, pixels.w, pixels.h, pw, ph, 4));
    let preview_selection = selection
        .as_ref()
        .map(|s| Arc::new(thumbnail(s, pixels.w, pixels.h, pw, ph, 1)));
    let original_texture = ctx.load_texture(
        "raster-original",
        ColorImage::from_rgba_unmultiplied([pw as usize, ph as usize], &preview_source),
        TextureOptions::LINEAR,
    );
    let texture = ctx.load_texture(
        "raster-preview",
        ColorImage::from_rgba_unmultiplied([pw as usize, ph as usize], &preview_source),
        TextureOptions::LINEAR,
    );
    ctx.data_mut(|d| {
        d.insert_temp(
            state_id(),
            Session {
                document: studio.swap_id.clone(),
                layer_id: layer.id,
                name: layer.name.clone(),
                mask: studio.paint_mask,
                w: pixels.w,
                h: pixels.h,
                source,
                selection: selection.map(Arc::new),
                settings,
                preview_w: pw,
                preview_h: ph,
                preview_source,
                preview_selection,
                original_texture,
                texture,
                preview_job: None,
                rendered: None,
                apply_job: None,
                show_original: false,
                sample: false,
                error: None,
            },
        )
    });
}

fn thumbnail(data: &[u8], w: u32, h: u32, dw: u32, dh: u32, channels: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(dw as usize * dh as usize * channels);
    for y in 0..dh {
        for x in 0..dw {
            let sx = ((x as f64 + 0.5) * w as f64 / dw as f64)
                .floor()
                .min((w - 1) as f64) as usize;
            let sy = ((y as f64 + 0.5) * h as f64 / dh as f64)
                .floor()
                .min((h - 1) as f64) as usize;
            let start = (sy * w as usize + sx) * channels;
            out.extend_from_slice(&data[start..start + channels]);
        }
    }
    out
}

fn render_job(
    ctx: &Context,
    source: Arc<Vec<u8>>,
    selection: Option<Arc<Vec<u8>>>,
    w: u32,
    h: u32,
    settings: Settings,
) -> Job {
    let result = Arc::new(Mutex::new(None));
    let destination = result.clone();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let rendered = crate::raster::apply(
            &source,
            w,
            h,
            &settings,
            selection.as_deref().map(Vec::as_slice),
        );
        if let Ok(mut slot) = destination.lock() {
            *slot = Some(rendered);
        }
        ctx.request_repaint();
    });
    result
}

pub(super) fn show(ui: &mut Ui, studio: &mut Studio) {
    let ctx = ui.ctx().clone();
    let Some(mut session) = ctx.data(|d| d.get_temp::<Session>(state_id())) else {
        return;
    };
    if session.document != studio.swap_id {
        ctx.data_mut(|d| d.remove::<Session>(state_id()));
        return;
    }
    if let Some(job) = &session.apply_job {
        let result = job.lock().ok().and_then(|mut j| j.take());
        if let Some(result) = result {
            session.apply_job = None;
            match result.and_then(|after| commit(studio, &session, after)) {
                Ok(()) => {
                    ctx.data_mut(|d| d.remove::<Session>(state_id()));
                    return;
                }
                Err(error) => session.error = Some(error),
            }
        }
    }
    if let Some((settings, job)) = &session.preview_job {
        let result = job.lock().ok().and_then(|mut j| j.take());
        if let Some(result) = result {
            if settings == &session.settings {
                match result {
                    Ok(data) => {
                        session.texture.set(
                            ColorImage::from_rgba_unmultiplied(
                                [session.preview_w as usize, session.preview_h as usize],
                                &data,
                            ),
                            TextureOptions::LINEAR,
                        );
                        session.rendered = Some(settings.clone());
                    }
                    Err(error) => session.error = Some(error),
                }
            }
            session.preview_job = None;
        }
    }
    let mut close = false;
    let busy = session.apply_job.is_some();
    let dialog = egui::Modal::new(Id::new("raster-filter-modal")).show(&ctx, |ui| {
        let width = (ctx.viewport_rect().width() - 70.0).clamp(560.0, 950.0);
        ui.set_width(width);
        ui.horizontal(|ui| {
            ui.heading(if session.settings.kind.bank() == Bank::Filters { "Raster filters" } else { "Raster effects" });
            ui.label(format!("{}{} · {} × {}", session.name, if session.mask { " / mask" } else { "" }, session.w, session.h));
        });
        ui.separator();
        egui::ScrollArea::vertical().max_height((ctx.viewport_rect().height() - 180.0).max(300.0)).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width((width - 310.0).max(220.0));
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut session.show_original, false, "Preview");
                        ui.selectable_value(&mut session.show_original, true, "Original");
                        if session.preview_job.is_some() { ui.spinner(); }
                    });
                    preview(ui, &mut session, busy);
                    if session.sample { ui.small("Click the original image to sample the background color."); }
                    else if session.selection.is_some() { ui.small("Only the current pixel selection is affected."); }
                    else { ui.small(if session.mask { "Applies to the whole mask." } else { "Applies to the whole image." }); }
                    if session.settings.kind == Kind::ChromaKey {
                        ui.add_space(6.0);
                        ui.small("Falloff softens the key boundary. Hardness tightens that transition. After applying, use a layer mask and brush Edge / Flow for local cleanup.");
                    }
                });
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_width(275.0);
                    ui.add_enabled_ui(!busy, |ui| {
                        let before_kind = session.settings.kind;
                        egui::ComboBox::from_id_salt("raster-kind").selected_text(session.settings.kind.name()).width(255.0).show_ui(ui, |ui| {
                            for bank in [Bank::Filters, Bank::Effects] {
                                ui.strong(if bank == Bank::Filters { "Filters" } else { "Effects" });
                                for kind in Kind::ALL.into_iter().filter(|k| k.bank() == bank) { ui.selectable_value(&mut session.settings.kind, kind, kind.name()); }
                                ui.separator();
                            }
                        });
                        if before_kind != session.settings.kind { session.settings = Settings::new(session.settings.kind); session.sample = false; session.error = None; }
                        ui.small(session.settings.kind.description());
                        ui.add_space(8.0);
                        controls(ui, &mut session);
                        ui.separator();
                        let keying = session.settings.kind == Kind::ChromaKey;
                        ui.add(Slider::new(&mut session.settings.strength, 0.0..=1.0).text(if keying { "Flow" } else { "Strength" }).custom_formatter(|v,_| format!("{:.0}%",v*100.0)))
                            .on_hover_text(if keying { "How much of the key to apply in this pass" } else { "Blend the processed image with the original" });
                        if ui.button("Reset settings").clicked() { session.settings = Settings::new(session.settings.kind); }
                    });
                });
            });
        });
        ui.separator();
        if let Some(error) = &session.error { ui.colored_label(Color32::LIGHT_RED, error); }
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() { close = true; }
            if busy { ui.spinner(); ui.label("Applying at full resolution…"); }
            else if ui.button(RichText::new("Apply").strong()).clicked() {
                session.error = None;
                session.apply_job = Some(render_job(&ctx, session.source.clone(), session.selection.clone(), session.w, session.h, session.settings.clone()));
            }
            ui.small("Apply commits one edit · Undo restores the original");
        });
    });
    if close || dialog.should_close() {
        ctx.data_mut(|d| d.remove::<Session>(state_id()));
        return;
    }
    if session.preview_job.is_none()
        && session.apply_job.is_none()
        && session.rendered.as_ref() != Some(&session.settings)
    {
        let settings = session
            .settings
            .scaled(session.preview_w as f32 / session.w as f32);
        session.preview_job = Some((
            session.settings.clone(),
            render_job(
                &ctx,
                session.preview_source.clone(),
                session.preview_selection.clone(),
                session.preview_w,
                session.preview_h,
                settings,
            ),
        ));
    }
    if session.preview_job.is_some() || session.apply_job.is_some() {
        ctx.request_repaint_after(Duration::from_millis(40));
    }
    ctx.data_mut(|d| d.insert_temp(state_id(), session));
}

fn preview(ui: &mut Ui, session: &mut Session, busy: bool) {
    let available = vec2(
        ui.available_width(),
        450.0_f32.min((ui.ctx().viewport_rect().height() - 300.0).max(200.0)),
    );
    let scale =
        (available.x / session.preview_w as f32).min(available.y / session.preview_h as f32);
    let size = vec2(
        session.preview_w as f32 * scale,
        session.preview_h as f32 * scale,
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let square = 12.0;
    let painter = ui.painter_at(rect);
    for y in 0..(rect.height() / square).ceil() as usize {
        for x in 0..(rect.width() / square).ceil() as usize {
            painter.rect_filled(
                egui::Rect::from_min_size(
                    rect.min + vec2(x as f32 * square, y as f32 * square),
                    vec2(square, square),
                )
                .intersect(rect),
                0.0,
                Color32::from_gray(if (x + y) % 2 == 0 { 80 } else { 112 }),
            );
        }
    }
    let original = session.show_original || session.sample;
    painter.image(
        if original {
            session.original_texture.id()
        } else {
            session.texture.id()
        },
        rect,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    if session.sample
        && !busy
        && response.clicked()
        && let Some(p) = response.interact_pointer_pos()
    {
        let x = (((p.x - rect.left()) / rect.width()) * session.w as f32)
            .floor()
            .clamp(0.0, (session.w - 1) as f32) as usize;
        let y = (((p.y - rect.top()) / rect.height()) * session.h as f32)
            .floor()
            .clamp(0.0, (session.h - 1) as f32) as usize;
        let i = (y * session.w as usize + x) * 4;
        session.settings.color = Rgba::rgb(
            session.source[i],
            session.source[i + 1],
            session.source[i + 2],
        );
        session.sample = false;
        session.show_original = false;
    }
    if session.sample {
        response.on_hover_cursor(egui::CursorIcon::Crosshair);
    }
}

fn controls(ui: &mut Ui, session: &mut Session) {
    let s = &mut session.settings;
    if s.kind == Kind::ChromaKey {
        ui.horizontal(|ui| {
            ui.label("Key color");
            super::color_picker::color_edit(ui, "raster-key-color", &mut s.color);
        });
        ui.horizontal(|ui| {
            if ui.button("Green").clicked() {
                s.color = Rgba::rgb(0, 255, 0);
            }
            if ui.button("Blue").clicked() {
                s.color = Rgba::rgb(0, 0, 255);
            }
            if ui
                .selectable_label(session.sample, "Sample image")
                .clicked()
            {
                session.sample = !session.sample;
            }
        });
    }
    let mut slider = |index: usize, label: &str, min: f32, max: f32| {
        ui.add(Slider::new(&mut s.values[index], min..=max).text(label));
    };
    match s.kind {
        Kind::ChromaKey => {
            slider(0, "Similarity", 0.0, 0.8);
            slider(1, "Falloff", 0.0, 0.8);
            slider(2, "Hardness", 0.0, 1.0);
            slider(3, "Spill cleanup", 0.0, 1.0);
        }
        Kind::BrightnessContrast => {
            slider(0, "Brightness", -1.0, 1.0);
            slider(1, "Contrast", -1.0, 1.0);
        }
        Kind::Exposure => {
            slider(0, "Exposure (EV)", -5.0, 5.0);
            slider(1, "Gamma", 0.1, 4.0);
        }
        Kind::Levels => {
            slider(0, "Black point", 0.0, 0.99);
            slider(1, "White point", 0.01, 1.0);
            slider(2, "Gamma", 0.1, 4.0);
            s.values[1] = s.values[1].max(s.values[0] + 0.001);
        }
        Kind::HueSaturation => {
            slider(0, "Hue (°)", -180.0, 180.0);
            slider(1, "Saturation", -1.0, 1.0);
            slider(2, "Lightness", -1.0, 1.0);
        }
        Kind::Vibrance => slider(0, "Vibrance", -1.0, 2.0),
        Kind::ColorBalance => {
            slider(0, "Cyan / red", -1.0, 1.0);
            slider(1, "Magenta / green", -1.0, 1.0);
            slider(2, "Yellow / blue", -1.0, 1.0);
        }
        Kind::Temperature => {
            slider(0, "Cool / warm", -1.0, 1.0);
            slider(1, "Green / magenta", -1.0, 1.0);
        }
        Kind::Threshold | Kind::AlphaThreshold | Kind::Solarize => slider(0, "Threshold", 0.0, 1.0),
        Kind::Posterize => slider(0, "Levels", 2.0, 32.0),
        Kind::GaussianBlur | Kind::FeatherAlpha => slider(0, "Radius (px)", 0.0, 100.0),
        Kind::GrowAlpha | Kind::ShrinkAlpha => slider(0, "Radius (px)", 1.0, 20.0),
        Kind::MotionBlur => {
            slider(0, "Distance (px)", 1.0, 100.0);
            slider(1, "Angle (°)", -180.0, 180.0);
        }
        Kind::Sharpen => slider(0, "Amount", 0.0, 2.0),
        Kind::UnsharpMask => {
            slider(0, "Radius (px)", 0.1, 50.0);
            slider(1, "Amount", 0.0, 5.0);
            slider(2, "Threshold", 0.0, 0.3);
        }
        Kind::EdgeDetect | Kind::Emboss => slider(0, "Amount", 0.1, 5.0),
        Kind::Pixelate | Kind::Halftone => slider(0, "Cell size (px)", 2.0, 100.0),
        Kind::Noise => slider(0, "Grain amount", 0.0, 1.0),
        Kind::Vignette => {
            slider(0, "Darkness", 0.0, 1.0);
            slider(1, "Inner radius", 0.0, 1.2);
        }
        Kind::Swirl => {
            slider(0, "Twist (°)", -720.0, 720.0);
            slider(1, "Radius", 0.05, 1.5);
        }
        Kind::Ripple => {
            slider(0, "Amplitude (px)", 0.0, 50.0);
            slider(1, "Wavelength (px)", 4.0, 200.0);
        }
        Kind::Grayscale | Kind::Sepia | Kind::Invert => {
            ui.small("Use Strength to blend with the original image.");
        }
    }
}

fn commit(studio: &mut Studio, session: &Session, after: Vec<u8>) -> Result<(), String> {
    if studio.swap_id != session.document {
        return Err("The document changed. Reopen the filter.".into());
    }
    let index = studio
        .doc
        .layers
        .iter()
        .position(|layer| layer.id == session.layer_id)
        .ok_or("The target layer was removed")?;
    if !studio.doc.layer_editable(index) {
        return Err("The target layer is locked or hidden".into());
    }
    let layer = &studio.doc.layers[index];
    let pixels = (if session.mask {
        layer.mask.as_ref()
    } else {
        layer.kind.pixels()
    })
    .ok_or("The target image or mask changed")?;
    if pixels.w != session.w || pixels.h != session.h || pixels.data != *session.source {
        return Err("The pixels changed while previewing. Cancel and reopen the filter.".into());
    }
    if after.len() != session.source.len() {
        return Err("The filter returned invalid pixel data".into());
    }
    if after == *session.source {
        studio.status = format!("{} · no pixel changes", session.settings.kind.name());
        return Ok(());
    }
    let cmd = Cmd::Pixels {
        layer: index,
        mask: session.mask,
        before: session.source.as_ref().clone(),
        after,
    };
    apply_command(&mut studio.doc, &cmd);
    studio.history.push(cmd);
    studio.dirty = true;
    studio.mark();
    studio.status = format!(
        "{} applied to {}{}",
        session.settings.kind.name(),
        session.name,
        if session.mask { " mask" } else { "" }
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Layer, Pixels};
    #[test]
    fn filter_preview_cancel_is_non_destructive_and_apply_undo_round_trips() {
        let ctx = Context::default();
        let mut studio = Studio::new();
        studio.doc.layers = vec![Layer::raster("Key", 2, 1)];
        studio.doc.layers[0].kind.pixels_mut().unwrap().data = vec![0, 255, 0, 255, 255, 0, 0, 128];
        studio.active_layer = Some(0);
        let before = studio.doc.layers[0].kind.pixels().unwrap().data.clone();
        open(&ctx, &mut studio, Kind::ChromaKey);
        let session = ctx.data(|d| d.get_temp::<Session>(state_id())).unwrap();
        let after = crate::raster::apply(&session.source, 2, 1, &session.settings, None).unwrap();
        assert_eq!(studio.doc.layers[0].kind.pixels().unwrap().data, before);
        assert_eq!(studio.history.len(), 0);
        commit(&mut studio, &session, after.clone()).unwrap();
        assert_eq!(studio.doc.layers[0].kind.pixels().unwrap().data, after);
        let undo = studio.history.undo().unwrap();
        apply_command(&mut studio.doc, &undo);
        assert_eq!(studio.doc.layers[0].kind.pixels().unwrap().data, before);
        let redo = studio.history.redo().unwrap();
        apply_command(&mut studio.doc, &redo);
        assert_eq!(studio.doc.layers[0].kind.pixels().unwrap().data, after);
        let encoded = serde_json::to_string(&studio.doc).unwrap();
        let saved: crate::document::Document = serde_json::from_str(&encoded).unwrap();
        assert_eq!(saved.layers[0].kind.pixels().unwrap().data, after);
    }
    #[test]
    fn filter_on_mask_preserves_image_and_rejects_stale_source() {
        let ctx = Context::default();
        let mut studio = Studio::new();
        studio.doc.layers = vec![Layer::raster("Image", 1, 1)];
        studio.doc.layers[0].mask = Pixels::from_rgba(1, 1, vec![30, 30, 30, 255]);
        studio.active_layer = Some(0);
        studio.paint_mask = true;
        let source_image = studio.doc.layers[0].kind.pixels().unwrap().data.clone();
        open(&ctx, &mut studio, Kind::Invert);
        let session = ctx.data(|d| d.get_temp::<Session>(state_id())).unwrap();
        let after = crate::raster::apply(&session.source, 1, 1, &session.settings, None).unwrap();
        commit(&mut studio, &session, after.clone()).unwrap();
        assert_eq!(
            studio.doc.layers[0].mask.as_ref().unwrap().data,
            vec![225, 225, 225, 255]
        );
        assert_eq!(
            studio.doc.layers[0].kind.pixels().unwrap().data,
            source_image
        );
        assert!(commit(&mut studio, &session, after).is_err());
    }

    fn frame(
        ctx: &Context,
        studio: &mut Studio,
        events: Vec<egui::Event>,
    ) -> std::collections::HashMap<String, egui::Rect> {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(960.0, 640.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                inspector(ui, studio);
                show(ui, studio);
            },
        );
        fn visit(shape: &egui::Shape, labels: &mut std::collections::HashMap<String, egui::Rect>) {
            match shape {
                egui::Shape::Text(t) => {
                    labels.insert(
                        t.galley.job.text.clone(),
                        t.galley.rect.translate(t.pos.to_vec2()),
                    );
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        visit(shape, labels);
                    }
                }
                _ => {}
            }
        }
        let mut labels = std::collections::HashMap::new();
        for shape in &output.shapes {
            visit(&shape.shape, &mut labels);
        }
        output.textures_delta.clear();
        labels
    }
    fn click(ctx: &Context, studio: &mut Studio, position: egui::Pos2) {
        frame(
            ctx,
            studio,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(
            ctx,
            studio,
            vec![egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }
    #[test]
    fn dialog_opens_from_inspector_cancels_and_applies_at_minimum_window_size() {
        let ctx = Context::default();
        let mut studio = Studio::new();
        studio.doc.layers = vec![Layer::raster("Key", 2, 1)];
        studio.doc.layers[0].kind.pixels_mut().unwrap().data =
            vec![0, 255, 0, 255, 220, 10, 10, 255];
        studio.active_layer = Some(0);
        let original = studio.doc.layers[0].kind.pixels().unwrap().data.clone();
        frame(&ctx, &mut studio, vec![]);
        let labels = frame(&ctx, &mut studio, vec![]);
        click(&ctx, &mut studio, labels["Chroma key…"].center());
        assert!(is_open(&ctx));
        frame(&ctx, &mut studio, vec![]);
        let labels = frame(&ctx, &mut studio, vec![]);
        assert!(labels["Cancel"].bottom() < 640.0);
        assert!(labels["Apply"].right() < 960.0);
        click(&ctx, &mut studio, labels["Cancel"].center());
        assert!(!is_open(&ctx));
        assert_eq!(studio.history.len(), 0);
        assert_eq!(studio.doc.layers[0].kind.pixels().unwrap().data, original);
        open(&ctx, &mut studio, Kind::ChromaKey);
        frame(&ctx, &mut studio, vec![]);
        let labels = frame(&ctx, &mut studio, vec![]);
        click(&ctx, &mut studio, labels["Apply"].center());
        for _ in 0..100 {
            frame(&ctx, &mut studio, vec![]);
            if !is_open(&ctx) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!is_open(&ctx));
        assert_eq!(studio.history.len(), 1);
        assert_eq!(studio.doc.layers[0].kind.pixels().unwrap().data[3], 0);
    }
}
