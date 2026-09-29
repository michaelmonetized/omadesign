use super::jobs;
use crate::{
    app::Studio,
    background_removal::{self as removal, RawMask, Settings, Source},
    document::Pixels,
    ml::{Progress, models::Model},
    tools::Persona,
};
use eframe::egui::{self, Color32, ColorImage, Context, Id, TextureHandle, TextureOptions};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

const JOB: &str = "pixel-remove-background";
fn id() -> Id {
    Id::new("remove-background-dialog")
}
fn cache_id() -> Id {
    Id::new("remove-background-cache")
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Owner {
    document: String,
    layer: u64,
    version: u64,
    w: u32,
    h: u32,
    selection_generation: u64,
}
#[derive(Clone)]
struct Cache {
    owner: Owner,
    model: Model,
    source: Source,
    raw: Arc<RawMask>,
}
#[derive(Clone)]
struct Session {
    owner: Owner,
    name: String,
    source: Source,
    model: Model,
    settings: Settings,
    raw: Option<(Model, Arc<RawMask>)>,
    rendered: Option<(Model, Settings)>,
    mask: Option<Arc<Vec<u8>>>,
    original: TextureHandle,
    preview: TextureHandle,
    matte: TextureHandle,
    view: u8,
    error: Option<String>,
    busy: bool,
    downloading: bool,
    changed_at: Instant,
}
enum Output {
    Downloaded,
    Rendered {
        model: Model,
        settings: Settings,
        raw: Arc<RawMask>,
        mask: Arc<Vec<u8>>,
        preview: Vec<u8>,
        matte: Vec<u8>,
    },
}

pub(super) fn available(studio: &Studio) -> bool {
    studio.persona == Persona::Pixel
        && studio.active_layer.is_some_and(|i| {
            studio.doc.layer_editable(i)
                && studio
                    .doc
                    .layers
                    .get(i)
                    .is_some_and(|l| l.kind.pixels().is_some())
        })
}
pub(super) fn is_open(ctx: &Context) -> bool {
    ctx.data(|d| d.get_temp::<Session>(id()).is_some())
}
pub(super) fn ready(ctx: &Context) -> bool {
    ctx.data(|d| {
        d.get_temp::<Session>(id())
            .is_none_or(|s| !s.busy && s.rendered == Some((s.model, s.settings)))
    })
}

fn dimensions(source: &Source) -> (u32, u32) {
    let scale = (640. / source.w.max(source.h) as f32).min(1.);
    (
        (source.w as f32 * scale).round().max(1.) as u32,
        (source.h as f32 * scale).round().max(1.) as u32,
    )
}
fn thumbnail(source: &Source, mask: Option<&[u8]>, matte: bool) -> Vec<u8> {
    let (w, h) = dimensions(source);
    let mut data = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h {
        for x in 0..w {
            let sx = ((x as f64 + 0.5) * source.w as f64 / w as f64)
                .floor()
                .min((source.w - 1) as f64) as usize;
            let sy = ((y as f64 + 0.5) * source.h as f64 / h as f64)
                .floor()
                .min((source.h - 1) as f64) as usize;
            let i = (sy * source.w as usize + sx) * 4;
            let amount = mask.map_or(255, |m| {
                ((m[i] as f32 * 0.2126 + m[i + 1] as f32 * 0.7152 + m[i + 2] as f32 * 0.0722)
                    * (m[i + 3] as f32 / 255.))
                    .round() as u8
            });
            if matte {
                data.extend_from_slice(&[amount, amount, amount, 255]);
            } else {
                let p = &source.rgba[i..i + 4];
                data.extend_from_slice(&[
                    p[0],
                    p[1],
                    p[2],
                    ((p[3] as u32 * amount as u32 + 127) / 255) as u8,
                ]);
            }
        }
    }
    data
}
fn image(source: &Source, data: &[u8]) -> ColorImage {
    let (w, h) = dimensions(source);
    ColorImage::from_rgba_unmultiplied([w as usize, h as usize], data)
}

pub(super) fn open(ctx: &Context, studio: &mut Studio) {
    if !available(studio) {
        return;
    }
    studio.end_pixel_stroke(false);
    let index = studio.active_layer.unwrap();
    let layer = &studio.doc.layers[index];
    let pixels = layer.kind.pixels().unwrap();
    if layer
        .mask
        .as_ref()
        .is_some_and(|m| m.w != pixels.w || m.h != pixels.h)
    {
        studio.status = "The existing mask has different dimensions. Align or remove it before removing the background.".into();
        return;
    }
    let selection = studio
        .pixel_sel_mask(index)
        .map(|s| Arc::new(s.into_owned()));
    if studio.pixel_sel.is_some() && selection.is_none() {
        studio.status = "Clear the old selection before removing the background".into();
        return;
    }
    let source = Source {
        w: pixels.w,
        h: pixels.h,
        rgba: Arc::new(pixels.data.clone()),
        selection,
        existing_mask: layer.mask.as_ref().map(|m| Arc::new(m.data.clone())),
    };
    if let Err(e) = source.region() {
        studio.status = e;
        return;
    }
    let owner = Owner {
        document: studio.swap_id.clone(),
        layer: layer.id,
        version: pixels.version,
        w: pixels.w,
        h: pixels.h,
        selection_generation: studio.pixel_sel_gen,
    };
    // Bound cache retention to one layer. Comparing source data also covers
    // operations that replace a Pixels value and restart its version counter.
    let cached = ctx.data(|d| d.get_temp::<Cache>(cache_id())).filter(|c| {
        c.owner == owner && c.source.rgba == source.rgba && c.source.selection == source.selection
    });
    let model = cached.as_ref().map_or(Model::U2NetP, |c| c.model);
    let original = ctx.load_texture(
        "remove-background-original",
        image(
            &source,
            &thumbnail(
                &source,
                source.existing_mask.as_deref().map(Vec::as_slice),
                false,
            ),
        ),
        TextureOptions::LINEAR,
    );
    let preview = ctx.load_texture(
        "remove-background-preview",
        image(&source, &thumbnail(&source, None, false)),
        TextureOptions::LINEAR,
    );
    let matte = ctx.load_texture(
        "remove-background-matte",
        image(&source, &thumbnail(&source, None, true)),
        TextureOptions::LINEAR,
    );
    jobs::cancel::<Output>(ctx, JOB);
    ctx.data_mut(|d| {
        d.insert_temp(
            id(),
            Session {
                owner,
                name: layer.name.clone(),
                source,
                model,
                settings: Settings::default(),
                raw: cached.map(|c| (c.model, c.raw)),
                rendered: None,
                mask: None,
                original,
                preview,
                matte,
                view: 0,
                error: None,
                busy: false,
                downloading: false,
                changed_at: Instant::now() - Duration::from_secs(1),
            },
        )
    });
}

fn owner_matches(studio: &Studio, s: &Session) -> bool {
    available(studio)
        && studio.swap_id == s.owner.document
        && studio.pixel_sel_gen == s.owner.selection_generation
        && studio
            .active_layer
            .and_then(|i| studio.doc.layers.get(i))
            .is_some_and(|l| {
                l.id == s.owner.layer
                    && l.kind.pixels().is_some_and(|p| {
                        p.version == s.owner.version && p.w == s.owner.w && p.h == s.owner.h
                    })
            })
}
fn commit(studio: &mut Studio, s: &Session) -> Result<(), String> {
    if !owner_matches(studio, s) {
        return Err("The active layer or selection changed. Open Remove Background again.".into());
    }
    let i = studio.active_layer.unwrap();
    if studio.pixel_sel_mask(i).as_deref() != s.source.selection.as_deref().map(Vec::as_slice) {
        return Err(
            "The layer moved relative to the selection. Open Remove Background again.".into(),
        );
    }
    let layer = &studio.doc.layers[i];
    if layer.kind.pixels().unwrap().data != *s.source.rgba
        || layer.mask.as_ref().map(|m| &m.data) != s.source.existing_mask.as_deref()
    {
        return Err("The source pixels or mask changed. Open Remove Background again.".into());
    }
    if s.rendered != Some((s.model, s.settings)) {
        return Err("Wait for the current preview to finish".into());
    }
    let mask = s.mask.as_ref().ok_or("No mask is ready")?;
    let pixels = Pixels::from_rgba(s.source.w, s.source.h, mask.as_ref().clone())
        .ok_or("Invalid mask dimensions")?;
    studio.replace_layer_mask(i, Some(pixels));
    studio.paint_mask = true;
    studio.status =
        "Background removed · editable layer mask · Undo restores the previous mask".into();
    Ok(())
}

fn start(ctx: &Context, s: &mut Session) {
    let source = s.source.clone();
    let model = s.model;
    let settings = s.settings;
    let raw = s
        .raw
        .as_ref()
        .filter(|(m, _)| *m == model)
        .map(|(_, r)| r.clone());
    s.busy = true;
    s.error = None;
    jobs::start_with_progress(ctx, JOB, 1, move |progress| {
        let raw = if let Some(raw) = raw {
            raw
        } else {
            Arc::new(removal::infer(&source, model, progress.as_ref())?)
        };
        let mask = Arc::new(removal::refine(&source, &raw, settings, progress.as_ref())?);
        progress.check()?;
        let preview = thumbnail(&source, Some(&mask), false);
        let matte = thumbnail(&source, Some(&mask), true);
        Ok(Output::Rendered {
            model,
            settings,
            raw,
            mask,
            preview,
            matte,
        })
    });
}
fn close(ctx: &Context) {
    jobs::cancel::<Output>(ctx, JOB);
    ctx.data_mut(|d| d.remove::<Session>(id()));
}

pub(super) fn show(ctx: &Context, studio: &mut Studio) {
    let Some(mut s) = ctx.data(|d| d.get_temp::<Session>(id())) else {
        return;
    };
    if !owner_matches(studio, &s) {
        close(ctx);
        studio.status =
            "Background removal cancelled because the layer or selection changed".into();
        return;
    }
    if let Some(result) = jobs::poll::<Output>(ctx, JOB) {
        s.busy = false;
        s.downloading = false;
        match result {
            Err(e) => s.error = Some(e),
            Ok(Output::Downloaded) => s.error = None,
            Ok(Output::Rendered {
                model,
                settings,
                raw,
                mask,
                preview,
                matte,
            }) => {
                // Preserve inference even if sliders moved while it ran. Only
                // a matching result becomes the preview that Apply can commit.
                ctx.data_mut(|d| {
                    d.insert_temp(
                        cache_id(),
                        Cache {
                            owner: s.owner.clone(),
                            model,
                            source: s.source.clone(),
                            raw: raw.clone(),
                        },
                    )
                });
                s.raw = Some((model, raw));
                if model == s.model && settings == s.settings {
                    s.preview
                        .set(image(&s.source, &preview), TextureOptions::LINEAR);
                    s.matte
                        .set(image(&s.source, &matte), TextureOptions::LINEAR);
                    s.mask = Some(mask);
                    s.rendered = Some((model, settings));
                }
            }
        }
    }
    let mut cancel = false;
    let mut apply = false;
    let before = (s.model, s.settings);
    let response = egui::Modal::new(Id::new("remove-background-modal")).show(ctx, |ui| {
        let width = (ctx.content_rect().width() - 70.).clamp(560., 960.);
        ui.set_width(width);
        ui.heading("Remove Background");
        ui.label(format!("{} · {} × {}", s.name, s.source.w, s.source.h));
        ui.separator();
        egui::ScrollArea::vertical()
            .max_height((ctx.content_rect().height() - 230.).max(200.))
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(width - 330.);
                        ui.horizontal(|ui| {
                            for (value, label) in [(0, "Preview"), (1, "Original"), (2, "Mask")] {
                                ui.selectable_value(&mut s.view, value, label);
                            }
                        });
                        preview(ui, &s);
                        ui.small(if s.source.selection.is_some() {
                            "Only the pixel selection is affected; feathering is preserved."
                        } else {
                            "Original pixels are preserved. Paint the mask after applying."
                        });
                    });
                    ui.add_space(12.);
                    ui.vertical(|ui| {
                        ui.set_width(292.);
                        egui::ComboBox::from_id_salt("removal-model")
                            .selected_text(s.model.label())
                            .show_ui(ui, |ui| {
                                for model in Model::ALL {
                                    ui.selectable_value(&mut s.model, model, model.label());
                                }
                            });
                        ui.small("Runs locally on your machine.");
                        if !s.model.present() {
                            ui.label(format!(
                                "Optional model · {} MB download",
                                s.model.size() / 1_000_000
                            ));
                            if ui
                                .add_enabled(!s.busy, egui::Button::new("Download model"))
                                .clicked()
                            {
                                let model = s.model;
                                s.busy = true;
                                s.downloading = true;
                                s.error = None;
                                jobs::start_with_progress(ctx, JOB, 1, move |progress| {
                                    model.download(progress.as_ref())?;
                                    Ok(Output::Downloaded)
                                });
                            }
                        }
                        ui.separator();
                        ui.strong("Refine matte");
                        ui.add(
                            egui::Slider::new(&mut s.settings.radius, 0..=64)
                                .text("Radius")
                                .suffix(" px"),
                        );
                        ui.add(
                            egui::Slider::new(&mut s.settings.epsilon, 1e-6..=0.1)
                                .logarithmic(true)
                                .text("Epsilon"),
                        );
                        ui.add(
                            egui::Slider::new(&mut s.settings.shift, -32..=32)
                                .text("Shift edge")
                                .suffix(" px"),
                        );
                        ui.add(
                            egui::Slider::new(&mut s.settings.contrast, 0.1..=4.)
                                .text("Matte contrast"),
                        );
                        if s.source.existing_mask.is_some() {
                            ui.checkbox(&mut s.settings.intersect, "Intersect with existing mask");
                            ui.small("Unchecked replaces the mask inside the selection.");
                        }
                        if ui.button("Reset matte").clicked() {
                            s.settings = Settings::default();
                        }
                        if let Some((model, raw)) = &s.raw
                            && *model == s.model
                        {
                            ui.small(format!(
                                "{} edge tiles · {} tiles skipped",
                                raw.stats.edge_tiles,
                                raw.stats.tiles - raw.stats.edge_tiles
                            ));
                            ui.small("Matte controls reuse the cached subject mask.");
                        }
                    });
                });
            });
        ui.separator();
        if s.busy {
            let (done, total) = jobs::frame_progress::<Output>(ctx, JOB).unwrap_or((0, 1));
            ui.add(
                egui::ProgressBar::new(done as f32 / total.max(1) as f32)
                    .animate(true)
                    .text(format!(
                        "{} · {done} / {total}",
                        jobs::stage::<Output>(ctx, JOB)
                    )),
            );
        }
        if let Some(error) = &s.error {
            ui.colored_label(Color32::LIGHT_RED, error);
            if ui.button("Try again").clicked() {
                s.error = None;
            }
            if s.model != Model::U2NetP
                && ui
                    .add_enabled(!s.busy, egui::Button::new("Download verified model again"))
                    .clicked()
            {
                let model = s.model;
                s.error = None;
                s.busy = true;
                s.downloading = true;
                s.raw = None;
                jobs::start_with_progress(ctx, JOB, 1, move |progress| {
                    model.download(progress.as_ref())?;
                    Ok(Output::Downloaded)
                });
            }
        }
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
            if ui
                .add_enabled(
                    !s.busy && s.rendered == Some((s.model, s.settings)),
                    egui::Button::new("Apply mask"),
                )
                .clicked()
            {
                apply = true;
            }
            ui.small("One undo step · editable mask");
        });
    });
    if cancel || response.should_close() {
        close(ctx);
        return;
    }
    if before != (s.model, s.settings) {
        s.changed_at = Instant::now();
        s.error = None;
        if before.0 != s.model && s.busy {
            jobs::cancel::<Output>(ctx, JOB);
            s.busy = false;
            s.downloading = false;
        }
    }
    if apply {
        match commit(studio, &s) {
            Ok(()) => {
                close(ctx);
                return;
            }
            Err(e) => s.error = Some(e),
        }
    }
    if !s.busy
        && s.error.is_none()
        && s.model.present()
        && s.rendered != Some((s.model, s.settings))
        && s.changed_at.elapsed() >= Duration::from_millis(120)
    {
        start(ctx, &mut s);
    }
    if s.busy
        || (s.error.is_none() && s.model.present() && s.rendered != Some((s.model, s.settings)))
    {
        ctx.request_repaint_after(Duration::from_millis(33));
    }
    ctx.data_mut(|d| d.insert_temp(id(), s));
}

fn preview(ui: &mut egui::Ui, s: &Session) {
    let tex = match s.view {
        1 => &s.original,
        2 => &s.matte,
        _ => &s.preview,
    };
    let scale = (ui.available_width() / tex.size_vec2().x)
        .min(((ui.ctx().content_rect().height() - 350.).clamp(150., 480.)) / tex.size_vec2().y);
    let (rect, _) = ui.allocate_exact_size(tex.size_vec2() * scale, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    for y in 0..(rect.height() / 12.).ceil() as usize {
        for x in 0..(rect.width() / 12.).ceil() as usize {
            painter.rect_filled(
                egui::Rect::from_min_size(
                    rect.min + egui::vec2(x as f32 * 12., y as f32 * 12.),
                    egui::vec2(12., 12.),
                )
                .intersect(rect),
                0.,
                Color32::from_gray(if (x + y) % 2 == 0 { 80 } else { 112 }),
            );
        }
    }
    painter.image(
        tex.id(),
        rect,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
        Color32::WHITE,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Document, Layer};
    fn studio() -> Studio {
        let mut s = Studio::new();
        s.doc = Document::new("Removal QA", 4., 1., 96.);
        s.doc.layers = vec![Layer::raster("Pixels", 4, 1)];
        s.doc.layers[0].kind.pixels_mut().unwrap().data = vec![255; 16];
        s.active_layer = Some(0);
        s.persona = Persona::Pixel;
        s.show_welcome = false;
        s
    }
    fn completed(ctx: &Context, studio: &mut Studio) -> Session {
        open(ctx, studio);
        let mut s = ctx.data(|d| d.get_temp::<Session>(id())).unwrap();
        s.mask = Some(Arc::new(vec![20; 16]));
        s.rendered = Some((s.model, s.settings));
        s
    }
    #[test]
    fn apply_mask_is_one_undo_step_preserves_pixels_and_survives_save() {
        let ctx = Context::default();
        let mut studio = studio();
        studio.doc.layers[0].mask = Pixels::from_rgba(4, 1, vec![128; 16]);
        let s = completed(&ctx, &mut studio);
        commit(&mut studio, &s).unwrap();
        assert_eq!(
            studio.doc.layers[0].kind.pixels().unwrap().data,
            vec![255; 16]
        );
        assert_eq!(
            studio.doc.layers[0].mask.as_ref().unwrap().data,
            vec![20; 16]
        );
        studio.undo();
        assert_eq!(
            studio.doc.layers[0].mask.as_ref().unwrap().data,
            vec![128; 16]
        );
        studio.redo();
        assert_eq!(
            studio.doc.layers[0].mask.as_ref().unwrap().data,
            vec![20; 16]
        );
        let doc: Document =
            serde_json::from_str(&serde_json::to_string(&studio.doc).unwrap()).unwrap();
        assert_eq!(doc.layers[0].mask.as_ref().unwrap().data, vec![20; 16]);
    }
    #[test]
    fn stale_context_pixels_mask_selection_and_locked_layers_reject_commit() {
        for change in 0..7 {
            let ctx = Context::default();
            let mut studio = studio();
            let s = completed(&ctx, &mut studio);
            match change {
                0 => studio.swap_id = "other document".into(),
                1 => studio.doc.layers[0].kind.pixels_mut().unwrap().data[0] = 3,
                2 => studio.doc.layers[0].mask = Some(Pixels::new(4, 1)),
                3 => studio.set_pixel_sel(Some(vec![255; 4])),
                4 => studio.doc.layers[0].locked = true,
                5 => studio.active_layer = None,
                _ => studio.persona = Persona::Photo,
            }
            assert!(commit(&mut studio, &s).is_err(), "change {change}");
        }
    }
    #[test]
    fn cancel_discards_preview_and_only_pixel_layers_enable_command() {
        let ctx = Context::default();
        let mut studio = studio();
        let _ = completed(&ctx, &mut studio);
        close(&ctx);
        assert!(!is_open(&ctx));
        assert!(studio.doc.layers[0].mask.is_none());
        studio.persona = Persona::Design;
        assert!(!available(&studio));
        studio.persona = Persona::Pixel;
        studio.doc.layers[0] = Layer::vector("Vector");
        assert!(!available(&studio));
    }

    #[test]
    fn moving_a_layer_under_a_selection_rejects_the_old_result() {
        let ctx = Context::default();
        let mut studio = studio();
        studio.set_pixel_sel(Some(vec![255, 255, 0, 0]));
        let s = completed(&ctx, &mut studio);
        if let crate::document::LayerKind::Raster { origin, .. } = &mut studio.doc.layers[0].kind {
            origin.x = 1.;
        }
        assert!(commit(&mut studio, &s).unwrap_err().contains("moved"));
        assert!(studio.doc.layers[0].mask.is_none());
    }

    #[test]
    fn matte_changes_reuse_the_same_raw_mask_without_loading_a_model() {
        let ctx = Context::default();
        let mut studio = studio();
        let mut s = completed(&ctx, &mut studio);
        let raw = Arc::new(RawMask {
            region: s.source.region().unwrap(),
            rough: vec![0., 0.3, 0.7, 1.],
            values: vec![0., 0.3, 0.7, 1.],
            stats: Default::default(),
        });
        s.model = Model::IsNet;
        s.raw = Some((s.model, raw.clone()));
        s.settings.contrast = 1.8;
        start(&ctx, &mut s);
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(result) = jobs::poll::<Output>(&ctx, JOB) {
                match result.unwrap() {
                    Output::Rendered {
                        raw: reused,
                        settings,
                        ..
                    } => {
                        assert!(Arc::ptr_eq(&raw, &reused));
                        assert_eq!(settings, s.settings);
                    }
                    _ => panic!("Unexpected download"),
                }
                break;
            }
            assert!(Instant::now() < deadline, "refinement timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
