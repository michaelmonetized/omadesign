//! Editable starts, with previews rendered once off the drawing thread.
use crate::app::Studio;
use crate::compositor::{Draft, View};
use crate::geom::Pt;
use crate::templates::{self, Template};
use crate::ui::theme::{accent, accent_soft, bg_panel, border, fg, fg_weak};
use eframe::egui::{self, Align2, Color32, FontId, Id, Rect, RichText, Sense, Stroke, Ui, vec2};
use std::collections::HashMap;
use std::sync::Arc;

const JOB: &str = "template-preview-batch";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Size {
    width: u32,
    height: u32,
    dpi: u32,
}

#[derive(Clone)]
struct Browser {
    query: String,
    category: &'static str,
    selected: &'static str,
    width: f32,
    height: f32,
    dpi: f32,
}

impl Default for Browser {
    fn default() -> Self {
        Self {
            query: String::new(),
            category: "All",
            selected: templates::CATALOG[0].id,
            width: 1080.0,
            height: 1350.0,
            dpi: 72.0,
        }
    }
}

impl Browser {
    fn size(&self) -> Size {
        Size {
            width: self.width.round().clamp(32.0, 16000.0) as u32,
            height: self.height.round().clamp(32.0, 16000.0) as u32,
            dpi: self.dpi.round().clamp(36.0, 600.0) as u32,
        }
    }
}

#[derive(Clone, Default)]
struct PreviewCache {
    size: Option<Size>,
    textures: Arc<HashMap<&'static str, egui::TextureHandle>>,
    error: Option<String>,
}

type PreviewBatch = (Size, Vec<(&'static str, egui::ColorImage)>);

fn render_previews(size: Size) -> Result<PreviewBatch, String> {
    let scale = (320.0 / size.width as f32).min(240.0 / size.height as f32);
    let width = (size.width as f32 * scale).round().max(1.0) as u32;
    let height = (size.height as f32 * scale).round().max(1.0) as u32;
    let images = templates::CATALOG
        .iter()
        .map(|template| {
            let document = templates::build(
                template.id,
                size.width as f32,
                size.height as f32,
                size.dpi as f32,
            )?;
            let pixels = crate::compositor::render_view(
                &document,
                View {
                    scale,
                    offset: Pt::ZERO,
                },
                width,
                height,
                Draft::none(),
            )
            .ok_or("Could not create a template preview")?;
            Ok((
                template.id,
                egui::ColorImage::from_rgba_premultiplied(
                    [width as usize, height as usize],
                    pixels.data(),
                ),
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok((size, images))
}

fn previews(ctx: &egui::Context, size: Size) -> PreviewCache {
    let cache_id = Id::new("template-preview-cache");
    let mut cache = ctx
        .data(|data| data.get_temp::<PreviewCache>(cache_id))
        .unwrap_or_default();
    if let Some(result) = super::jobs::poll::<PreviewBatch>(ctx, JOB) {
        match result {
            Ok((loaded, images)) if loaded == size => {
                let textures = images
                    .into_iter()
                    .map(|(id, image)| {
                        (
                            id,
                            ctx.load_texture(
                                format!("template-{id}"),
                                image,
                                egui::TextureOptions::LINEAR,
                            ),
                        )
                    })
                    .collect();
                cache = PreviewCache {
                    size: Some(loaded),
                    textures: Arc::new(textures),
                    error: None,
                };
            }
            Err(error) => {
                cache.size = Some(size);
                cache.error = Some(error);
            }
            _ => {}
        }
        ctx.data_mut(|data| data.insert_temp(cache_id, cache.clone()));
    }
    // Finish an old size's work before starting another. Scrubbing size controls
    // cannot create an unbounded queue of rendering threads.
    if cache.size != Some(size) && !super::jobs::is_running::<PreviewBatch>(ctx, JOB) {
        super::jobs::start(ctx, JOB, move || render_previews(size));
    }
    cache
}

pub(super) fn previews_ready(ctx: &egui::Context) -> bool {
    let state = ctx
        .data(|data| data.get_temp::<Browser>(Id::new("template-browser-state")))
        .unwrap_or_default();
    ctx.data(|data| data.get_temp::<PreviewCache>(Id::new("template-preview-cache")))
        .is_some_and(|cache| cache.size == Some(state.size()))
}

pub fn window(ui: &mut Ui, studio: &mut Studio) {
    if !studio.show_templates {
        return;
    }
    let ctx = ui.ctx().clone();
    let mut open = true;
    let screen = ctx.viewport_rect().size();
    egui::Window::new("Template library")
        .id(Id::new("template-library-window"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size(vec2(
            (screen.x - 100.0).clamp(400.0, 1040.0),
            (screen.y - 120.0).max(300.0),
        ))
        .show(&ctx, |ui| library(ui, studio));
    studio.show_templates &= open;
}

pub fn library(ui: &mut Ui, studio: &mut Studio) {
    let state_id = Id::new("template-browser-state");
    let mut state = ui
        .ctx()
        .data(|data| data.get_temp::<Browser>(state_id))
        .unwrap_or_default();
    ui.label(
        RichText::new("52 good starts.")
            .size(24.0)
            .strong()
            .color(fg()),
    );
    ui.label(
        RichText::new("Original designs for every size. Every word and shape is yours to change.")
            .color(fg_weak())
            .size(12.0),
    );
    ui.add_space(14.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Make it fit").strong().size(12.0));
        let size = state.size();
        let preset_name = crate::presets::all()
            .iter()
            .find(|preset| {
                preset.w.round() as u32 == size.width
                    && preset.h.round() as u32 == size.height
                    && preset.dpi.round() as u32 == size.dpi
            })
            .map_or("Custom size", |preset| preset.name);
        egui::ComboBox::from_id_salt("template-size")
            .selected_text(preset_name)
            .width(190.0)
            .show_ui(ui, |ui| {
                for preset in crate::presets::all() {
                    if ui
                        .selectable_label(
                            preset.name == preset_name,
                            format!("{} · {}", preset.group, preset.name),
                        )
                        .clicked()
                    {
                        state.width = preset.w;
                        state.height = preset.h;
                        state.dpi = preset.dpi;
                    }
                }
            });
        if ui.small_button("Current canvas").clicked() {
            state.width = studio.doc.width;
            state.height = studio.doc.height;
            state.dpi = studio.doc.dpi;
        }
        ui.add(
            egui::DragValue::new(&mut state.width)
                .prefix("W ")
                .range(32.0..=16000.0)
                .speed(5.0),
        );
        ui.add(
            egui::DragValue::new(&mut state.height)
                .prefix("H ")
                .range(32.0..=16000.0)
                .speed(5.0),
        );
        ui.add(
            egui::DragValue::new(&mut state.dpi)
                .suffix(" dpi")
                .range(36.0..=600.0)
                .speed(1.0),
        );
    });
    ui.add_space(10.0);
    ui.horizontal_wrapped(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut state.query)
                .hint_text("Find your next idea…")
                .desired_width(190.0),
        );
        egui::ComboBox::from_id_salt("template-category")
            .selected_text(if state.category == "All" {
                "All categories"
            } else {
                state.category
            })
            .width(150.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.category, "All", "All categories");
                for category in templates::categories() {
                    ui.selectable_value(&mut state.category, *category, *category);
                }
            });
    });
    ui.add_space(12.0);
    let size = state.size();
    let cache = previews(ui.ctx(), size);
    let query = state.query.trim().to_lowercase();
    let visible: Vec<_> = templates::CATALOG
        .iter()
        .filter(|template| {
            (state.category == "All" || template.category == state.category)
                && (query.is_empty()
                    || format!(
                        "{} {} {} {}",
                        template.name, template.title, template.category, template.description
                    )
                    .to_lowercase()
                    .contains(&query))
        })
        .collect();
    if !visible.iter().any(|template| template.id == state.selected)
        && let Some(first) = visible.first()
    {
        state.selected = first.id;
    }
    let selected = templates::find(state.selected);
    let mut use_selected = false;
    if let Some(selected) = selected {
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !visible.is_empty(),
                    egui::Button::new("Use this template")
                        .fill(accent_soft())
                        .min_size(vec2(160.0, 32.0)),
                )
                .clicked()
            {
                use_selected = true;
            }
            ui.label(RichText::new(selected.name).strong());
            ui.label(
                RichText::new(format!(
                    "Week {:02} · {} × {}",
                    selected.week, size.width, size.height
                ))
                .small()
                .color(fg_weak()),
            );
        });
        ui.label(RichText::new(selected.description).small().color(fg_weak()));
    }
    ui.add_space(12.0);
    if let Some(error) = &cache.error {
        ui.label(RichText::new(error).color(accent()));
    }
    if visible.is_empty() {
        ui.label(RichText::new("No matches. Try another word or category.").color(fg_weak()));
    }
    let columns = if ui.available_width() >= 720.0 {
        3
    } else if ui.available_width() >= 450.0 {
        2
    } else {
        1
    };
    let gap = 12.0;
    let width = (ui.available_width() - gap * (columns - 1) as f32).max(160.0) / columns as f32;
    let max_height = (ui.ctx().viewport_rect().height() - 380.0).clamp(180.0, 560.0);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = vec2(gap, gap);
        egui::ScrollArea::vertical()
            .id_salt("template-cards")
            .max_height(max_height)
            .min_scrolled_height(max_height)
            .auto_shrink([false, false])
            .show_rows(ui, 225.0, visible.len().div_ceil(columns), |ui, rows| {
                for row in rows {
                    ui.horizontal(|ui| {
                        for &template in
                            &visible[row * columns..((row + 1) * columns).min(visible.len())]
                        {
                            let texture = (cache.size == Some(size))
                                .then(|| cache.textures.get(template.id))
                                .flatten();
                            let response =
                                card(ui, template, width, state.selected == template.id, texture);
                            if response.clicked() || response.double_clicked() {
                                state.selected = template.id;
                            }
                            if response.double_clicked() {
                                use_selected = true;
                            }
                        }
                    });
                }
            });
    });
    ui.ctx()
        .data_mut(|data| data.insert_temp(state_id, state.clone()));
    if use_selected {
        studio.use_template(
            state.selected,
            size.width as f32,
            size.height as f32,
            size.dpi as f32,
        );
    }
}

fn card(
    ui: &mut Ui,
    template: &Template,
    width: f32,
    selected: bool,
    texture: Option<&egui::TextureHandle>,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 225.0), Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 8.0, bg_panel());
    painter.rect_stroke(
        rect.shrink(0.5),
        8.0,
        Stroke::new(
            if selected { 1.5 } else { 1.0 },
            if selected || response.hovered() {
                accent()
            } else {
                border()
            },
        ),
        egui::StrokeKind::Inside,
    );
    let preview = Rect::from_min_max(rect.min + vec2(9.0, 9.0), rect.max - vec2(9.0, 53.0));
    if let Some(texture) = texture {
        let image_size = texture.size_vec2();
        let scale = (preview.width() / image_size.x).min(preview.height() / image_size.y);
        let target = Rect::from_center_size(preview.center(), image_size * scale);
        painter.image(
            texture.id(),
            target,
            Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        let paper = template.palette[0];
        painter.rect_filled(
            preview,
            4.0,
            Color32::from_rgb((paper >> 16) as u8, (paper >> 8) as u8, paper as u8),
        );
        let ink = template.palette[1];
        painter.text(
            preview.center(),
            Align2::CENTER_CENTER,
            "Preview loading…",
            FontId::proportional(11.0),
            Color32::from_rgb((ink >> 16) as u8, (ink >> 8) as u8, ink as u8),
        );
    }
    let name = egui::WidgetText::from(RichText::new(template.name).strong().size(12.0).color(fg()))
        .into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            width - 20.0,
            egui::TextStyle::Body,
        );
    painter.galley(
        egui::pos2(rect.left() + 10.0, rect.bottom() - 44.0),
        name,
        fg(),
    );
    painter.text(
        egui::pos2(rect.left() + 10.0, rect.bottom() - 18.0),
        Align2::LEFT_CENTER,
        format!("{:02} / 52  ·  {}", template.week, template.category),
        FontId::proportional(10.0),
        fg_weak(),
    );
    response.on_hover_text(template.description)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_cards_keep_a_full_visible_row_in_welcome_and_compact_windows() {
        for screen in [vec2(1600.0, 1000.0), vec2(960.0, 640.0)] {
            for floating in [false, true] {
                let ctx = egui::Context::default();
                crate::ui::theme::apply(&ctx);
                // Preview pixels do not affect layout. Avoid spawning a render job
                // while inspecting the real nested welcome and window layouts.
                ctx.data_mut(|data| {
                    data.insert_temp(
                        Id::new("template-preview-cache"),
                        PreviewCache {
                            size: Some(Browser::default().size()),
                            ..Default::default()
                        },
                    );
                });
                let mut studio = Studio::new();
                studio.welcome_page = crate::app::WelcomePage::Templates;
                studio.show_templates = floating;
                let mut output = egui::FullOutput::default();
                for _ in 0..3 {
                    output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, screen)),
                            ..Default::default()
                        },
                        |ui| {
                            if floating {
                                window(ui, &mut studio);
                            } else {
                                super::super::welcome::show(ui, &mut studio);
                            }
                        },
                    );
                    output.textures_delta.clear();
                }
                // Widget and clip edges are rounded independently to physical pixels.
                let pixel_tolerance = 0.5 / ctx.pixels_per_point();
                let viewport = Rect::from_min_size(egui::Pos2::ZERO, screen);
                let full_cards = output
                    .shapes
                    .iter()
                    .filter(|clipped| {
                        matches!(&clipped.shape, egui::Shape::Rect(rect)
                        if (rect.rect.height() - 225.0).abs() < 0.01
                            && clipped.clip_rect.expand(pixel_tolerance).contains_rect(rect.rect)
                            && viewport.contains_rect(rect.rect))
                    })
                    .count();
                assert!(
                    full_cards >= 2,
                    "a full first row must remain visible at {screen:?}, floating={floating}; got {full_cards} cards"
                );
                for label in if floating {
                    &["Use this template", "Template library"][..]
                } else {
                    &["Use this template"][..]
                } {
                    assert!(output.shapes.iter().any(|clipped| {
                        matches!(&clipped.shape, egui::Shape::Text(text)
                            if text.galley.job.text == *label
                                && viewport.contains_rect(text.visual_bounding_rect())
                                && clipped.clip_rect.expand(pixel_tolerance).contains_rect(text.visual_bounding_rect()))
                    }), "{label} must remain visible at {screen:?}, floating={floating}");
                }
            }
        }
    }
}
