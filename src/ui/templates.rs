//! Editable starts, with previews rendered once off the drawing thread.
use crate::app::Studio;
use crate::compositor::{Draft, View};
use crate::geom::Pt;
use crate::templates;
use crate::tools::Persona;
use crate::ui::theme::{accent, accent_soft, bg_panel, border, fg, fg_weak};
use eframe::egui::{self, Align2, Color32, FontId, Id, Rect, RichText, Sense, Stroke, Ui, vec2};
use std::collections::HashMap;
use std::sync::Arc;

const JOB: &str = "template-preview-batch";
const LAYOUT_JOB: &str = "layout-template-preview-batch";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Size {
    width: u32,
    height: u32,
    dpi: u32,
}

#[derive(Clone)]
struct Browser {
    mode: Option<Persona>,
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
            mode: None,
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

fn render_previews(size: Size, layout: bool) -> Result<PreviewBatch, String> {
    let ids: Vec<_> = if layout {
        crate::layout_templates::CATALOG
            .iter()
            .map(|t| t.id)
            .collect()
    } else {
        templates::CATALOG.iter().map(|t| t.id).collect()
    };
    let images = ids
        .into_iter()
        .map(|id| {
            let document = if layout {
                crate::layout_templates::build(
                    id,
                    size.width as f32,
                    size.height as f32,
                    size.dpi as f32,
                )?
            } else {
                templates::build(id, size.width as f32, size.height as f32, size.dpi as f32)?
            };
            // Some Layout starters have their own minimum dimensions or several pages.
            // Fit their actual first canvas instead of cropping to the requested size.
            let scale = (320.0 / document.width).min(240.0 / document.height);
            let width = (document.width * scale).round().max(1.0) as u32;
            let height = (document.height * scale).round().max(1.0) as u32;
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
                id,
                egui::ColorImage::from_rgba_premultiplied(
                    [width as usize, height as usize],
                    pixels.data(),
                ),
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok((size, images))
}

fn previews(ctx: &egui::Context, size: Size, layout: bool) -> PreviewCache {
    let cache_id = Id::new(if layout {
        "layout-template-preview-cache"
    } else {
        "template-preview-cache"
    });
    let job = if layout { LAYOUT_JOB } else { JOB };
    let mut cache = ctx
        .data(|data| data.get_temp::<PreviewCache>(cache_id))
        .unwrap_or_default();
    if let Some(result) = super::jobs::poll::<PreviewBatch>(ctx, job) {
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
    if cache.size != Some(size) && !super::jobs::is_running::<PreviewBatch>(ctx, job) {
        super::jobs::start(ctx, job, move || render_previews(size, layout));
    }
    cache
}

pub(super) fn previews_ready(ctx: &egui::Context) -> bool {
    let state = ctx
        .data(|data| data.get_temp::<Browser>(Id::new("template-browser-state")))
        .unwrap_or_default();
    let cache = if state.mode == Some(Persona::Layout) {
        "layout-template-preview-cache"
    } else {
        "template-preview-cache"
    };
    ctx.data(|data| data.get_temp::<PreviewCache>(Id::new(cache)))
        .is_some_and(|cache| cache.size == Some(state.size()))
}

/// Open only the templates meaningful for this creation action. Other entrypoints
/// may still set show_templates directly to use the general library.
pub fn open(ctx: &egui::Context, studio: &mut Studio, persona: Persona) {
    if !matches!(persona, Persona::Design | Persona::Layout) {
        return;
    }
    let id = Id::new("template-browser-state");
    let mut state = ctx
        .data(|data| data.get_temp::<Browser>(id))
        .unwrap_or_default();
    if state.mode != Some(persona) {
        state = Browser {
            mode: Some(persona),
            ..Browser::default()
        };
        if persona == Persona::Layout {
            state.selected = crate::layout_templates::CATALOG[0].id;
            state.width = 1280.0;
            state.height = 900.0;
            state.dpi = 96.0;
        }
    }
    ctx.data_mut(|data| data.insert_temp(id, state));
    studio.show_templates = true;
}

pub fn window(ui: &mut Ui, studio: &mut Studio) {
    if !studio.show_templates {
        reset_scope(ui.ctx());
        return;
    }
    let ctx = ui.ctx().clone();
    let mut open = true;
    let screen = ctx.viewport_rect().size();
    egui::Window::new("Template library")
        .id(Id::new("template-library-window"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .fixed_size(vec2(
            (screen.x - 80.0).clamp(280.0, 1040.0),
            (screen.y - 100.0).max(220.0),
        ))
        .show(&ctx, |ui| {
            // A fixed content width breaks the resize feedback loop between the
            // window's measured width, the responsive grid and the scroll area.
            ui.set_width((screen.x - 80.0).clamp(280.0, 1040.0));
            library(ui, studio);
        });
    studio.show_templates &= open;
    if !studio.show_templates {
        reset_scope(&ctx);
    }
}

fn reset_scope(ctx: &egui::Context) {
    ctx.data_mut(|data| {
        let id = Id::new("template-browser-state");
        if data
            .get_temp::<Browser>(id)
            .is_some_and(|state| state.mode.is_some())
        {
            data.insert_temp(id, Browser::default());
        }
    });
}

pub fn library(ui: &mut Ui, studio: &mut Studio) {
    let state_id = Id::new("template-browser-state");
    let mut state = ui
        .ctx()
        .data(|data| data.get_temp::<Browser>(state_id))
        .unwrap_or_default();
    if state.mode == Some(Persona::Layout) {
        layout_library(ui, studio, &mut state);
        ui.ctx().data_mut(|data| data.insert_temp(state_id, state));
        return;
    }
    ui.label(
        RichText::new(if state.mode == Some(Persona::Design) {
            "Vector templates"
        } else {
            "52 good starts."
        })
        .size(24.0)
        .strong()
        .color(fg()),
    );
    ui.label(
        RichText::new("Original designs for every size. Every word and shape is yours to change.")
            .color(fg_weak())
            .size(12.0),
    );
    ui.add_space(10.0);
    if state.mode.is_none() {
        ui.label(RichText::new("Layout starters").strong().size(12.0));
        ui.horizontal_wrapped(|ui| {
            for template in crate::layout_templates::CATALOG {
                if ui
                    .button(template.name)
                    .on_hover_text(template.description)
                    .clicked()
                {
                    studio.use_layout_template(template.id, state.width, state.height, state.dpi);
                }
            }
        });
    }
    ui.add_space(14.0);
    size_controls(ui, studio, &mut state);
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
    let cache = previews(ui.ctx(), size, false);
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
                            let response = card(
                                ui,
                                width,
                                state.selected == template.id,
                                texture,
                                template.name,
                                &format!("{:02} / 52  ·  {}", template.week, template.category),
                                template.description,
                                template.palette,
                            );
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

fn layout_library(ui: &mut Ui, studio: &mut Studio, state: &mut Browser) {
    ui.heading("Layout templates");
    ui.label(
        RichText::new("Editable frames, responsive stacks and working prototypes.")
            .color(fg_weak()),
    );
    ui.add_space(14.0);
    ui.add_enabled_ui(state.selected != "layout-fieldwork", |ui| {
        size_controls(ui, studio, state)
    });
    if state.selected == "layout-fieldwork" {
        ui.label(
            RichText::new("Fieldwork includes its own responsive pages and sizes.")
                .small()
                .color(fg_weak()),
        );
    }
    ui.add_space(10.0);
    ui.add(
        egui::TextEdit::singleline(&mut state.query)
            .hint_text("Find a Layout starter…")
            .desired_width(240.0),
    );
    let query = state.query.trim().to_lowercase();
    let visible: Vec<_> = crate::layout_templates::CATALOG
        .iter()
        .filter(|template| {
            query.is_empty()
                || format!("{} {}", template.name, template.description)
                    .to_lowercase()
                    .contains(&query)
        })
        .collect();
    if !visible.iter().any(|template| template.id == state.selected)
        && let Some(first) = visible.first()
    {
        state.selected = first.id;
    }
    let size = state.size();
    let cache = previews(ui.ctx(), size, true);
    let mut use_selected = false;
    ui.add_space(12.0);
    if let Some(template) = crate::layout_templates::find(state.selected) {
        ui.horizontal_wrapped(|ui| {
            use_selected = ui
                .add_enabled(
                    !visible.is_empty(),
                    egui::Button::new("Use this template")
                        .fill(accent_soft())
                        .min_size(vec2(160.0, 32.0)),
                )
                .clicked();
            ui.label(RichText::new(template.name).strong());
        });
        ui.label(RichText::new(template.description).small().color(fg_weak()));
    }
    ui.add_space(12.0);
    if let Some(error) = &cache.error {
        ui.label(RichText::new(error).color(accent()));
    }
    if visible.is_empty() {
        ui.weak("No matching Layout starters. Try another word.");
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
    let max_height = (ui.ctx().viewport_rect().height() - 330.0).clamp(225.0, 560.0);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = vec2(gap, gap);
        egui::ScrollArea::vertical()
            .id_salt("layout-template-cards")
            .max_height(max_height)
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
                            let response = card(
                                ui,
                                width,
                                state.selected == template.id,
                                texture,
                                template.name,
                                "Layout · editable frames",
                                template.description,
                                [0xf4eee4, 0x121822, 0x2f6bff, 0xffffff],
                            );
                            if response.clicked() {
                                state.selected = template.id;
                            }
                            if response.double_clicked() {
                                state.selected = template.id;
                                use_selected = true;
                            }
                        }
                    });
                }
            });
    });
    if use_selected {
        studio.use_layout_template(
            state.selected,
            size.width as f32,
            size.height as f32,
            size.dpi as f32,
        );
    }
}

fn size_controls(ui: &mut Ui, studio: &Studio, state: &mut Browser) {
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
}

fn card(
    ui: &mut Ui,
    width: f32,
    selected: bool,
    texture: Option<&egui::TextureHandle>,
    title: &str,
    subtitle: &str,
    description: &str,
    palette: [u32; 4],
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
        let paper = palette[0];
        painter.rect_filled(
            preview,
            4.0,
            Color32::from_rgb((paper >> 16) as u8, (paper >> 8) as u8, paper as u8),
        );
        let ink = palette[1];
        painter.text(
            preview.center(),
            Align2::CENTER_CENTER,
            "Preview loading…",
            FontId::proportional(11.0),
            Color32::from_rgb((ink >> 16) as u8, (ink >> 8) as u8, ink as u8),
        );
    }
    let name = egui::WidgetText::from(RichText::new(title).strong().size(12.0).color(fg()))
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
        subtitle,
        FontId::proportional(10.0),
        fg_weak(),
    );
    response.on_hover_text(description)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_cards_keep_a_full_visible_row_in_all_modes_and_compact_windows() {
        for screen in [vec2(1600.0, 1000.0), vec2(960.0, 640.0)] {
            for mode in [None, Some(Persona::Design), Some(Persona::Layout)] {
                let ctx = egui::Context::default();
                crate::ui::theme::apply(&ctx);
                let mut studio = Studio::new();
                if let Some(mode) = mode {
                    open(&ctx, &mut studio, mode);
                }
                let state = ctx
                    .data(|data| data.get_temp::<Browser>(Id::new("template-browser-state")))
                    .unwrap_or_default();
                // Preview pixels do not affect layout. Avoid spawning a render job.
                ctx.data_mut(|data| {
                    data.insert_temp(
                        Id::new(if mode == Some(Persona::Layout) {
                            "layout-template-preview-cache"
                        } else {
                            "template-preview-cache"
                        }),
                        PreviewCache {
                            size: Some(state.size()),
                            ..Default::default()
                        },
                    );
                });
                studio.show_templates = true;
                let mut output = egui::FullOutput::default();
                for _ in 0..3 {
                    output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, screen)),
                            ..Default::default()
                        },
                        |ui| {
                            window(ui, &mut studio);
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
                    "a full first row must remain visible at {screen:?}, mode={mode:?}; got {full_cards} cards"
                );
                for label in &["Use this template", "Template library"] {
                    assert!(output.shapes.iter().any(|clipped| {
                        matches!(&clipped.shape, egui::Shape::Text(text)
                            if text.galley.job.text == *label
                                && viewport.contains_rect(text.visual_bounding_rect())
                                && clipped.clip_rect.expand(pixel_tolerance).contains_rect(text.visual_bounding_rect()))
                    }), "{label} must remain visible at {screen:?}, mode={mode:?}");
                }
            }
        }
    }

    #[test]
    fn scoped_chooser_creates_the_requested_editing_mode() {
        for mode in [Persona::Design, Persona::Layout] {
            let ctx = egui::Context::default();
            crate::ui::theme::apply(&ctx);
            let mut studio = Studio::new();
            open(&ctx, &mut studio, mode);
            let state = ctx
                .data(|data| data.get_temp::<Browser>(Id::new("template-browser-state")))
                .unwrap();
            assert_eq!(
                crate::layout_templates::is_layout_template(state.selected),
                mode == Persona::Layout
            );
            ctx.data_mut(|data| {
                data.insert_temp(
                    Id::new(if mode == Persona::Layout {
                        "layout-template-preview-cache"
                    } else {
                        "template-preview-cache"
                    }),
                    PreviewCache {
                        size: Some(state.size()),
                        ..Default::default()
                    },
                )
            });
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(960., 640.))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input.clone(), |ui| library(ui, &mut studio));
            output.textures_delta.clear();
            output = ctx.run_ui(input.clone(), |ui| library(ui, &mut studio));
            output.textures_delta.clear();
            let labels: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|clipped| match &clipped.shape {
                    egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                    _ => None,
                })
                .collect();
            assert!(labels.contains(&if mode == Persona::Layout {
                "Layout templates"
            } else {
                "Vector templates"
            }));
            assert!(
                !labels.contains(&"Layout starters"),
                "scoped Vector chooser must not offer Layout starters"
            );
            let target = output
                .shapes
                .iter()
                .find_map(|clipped| match &clipped.shape {
                    egui::Shape::Text(text) if text.galley.job.text == "Use this template" => {
                        Some(text.visual_bounding_rect().center())
                    }
                    _ => None,
                })
                .unwrap();
            for pressed in [true, false] {
                let mut click = input.clone();
                click.events = vec![
                    egui::Event::PointerMoved(target),
                    egui::Event::PointerButton {
                        pos: target,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ];
                let mut clicked = ctx.run_ui(click, |ui| library(ui, &mut studio));
                clicked.textures_delta.clear();
            }
            assert_eq!(studio.persona, mode);
            assert!(!studio.show_templates);
            assert!(studio.dirty);
            assert!(!studio.doc.layers.is_empty());
        }
    }
    #[test]
    fn chooser_width_is_stable_before_and_after_pointer_entry() {
        for mode in [Persona::Design, Persona::Layout] {
            let ctx = egui::Context::default();
            crate::ui::theme::apply(&ctx);
            let mut studio = Studio::new();
            open(&ctx, &mut studio, mode);
            let state = ctx
                .data(|d| d.get_temp::<Browser>(Id::new("template-browser-state")))
                .unwrap();
            ctx.data_mut(|d| {
                d.insert_temp(
                    Id::new(if mode == Persona::Layout {
                        "layout-template-preview-cache"
                    } else {
                        "template-preview-cache"
                    }),
                    PreviewCache {
                        size: Some(state.size()),
                        ..Default::default()
                    },
                )
            });
            let mut widths = vec![];
            for n in 0..16 {
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1600., 900.))),
                    events: vec![egui::Event::PointerMoved(egui::pos2(
                        if n < 5 { 2. } else { 800. },
                        450.,
                    ))],
                    ..Default::default()
                };
                let mut output = ctx.run_ui(input, |ui| window(ui, &mut studio));
                output.textures_delta.clear();
                if n > 0 {
                    widths.push(
                        ctx.memory(|m| m.area_rect(Id::new("template-library-window")))
                            .unwrap()
                            .width(),
                    );
                }
            }
            assert!(widths[0] >= 1040., "must open at final width: {widths:?}");
            assert!(
                widths.iter().all(|w| (w - widths[0]).abs() < 1.),
                "pointer entry must not resize window: {widths:?}"
            );
        }
    }
}
