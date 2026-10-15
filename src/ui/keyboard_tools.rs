use crate::{
    app::Studio,
    geom::Pt,
    tools::{Persona, Tool},
};
use eframe::egui::{self, Id, Key, Modifiers};

const ID: &str = "keyboard-tool-points";

#[derive(Clone)]
struct Dialog {
    document: String,
    generation: u64,
    selection: Vec<(usize, u64)>,
    layer: Option<u64>,
    pixel_generation: u64,
    photo: Option<(usize, crate::photo::DevelopParams)>,
    photo_generation: u64,
    tool: Tool,
    persona: Persona,
    points: String,
    add: bool,
    subtract: bool,
    source: bool,
    focus: bool,
    error: Option<String>,
}

impl Dialog {
    fn current(&self, studio: &Studio) -> bool {
        self.document == studio.swap_id
            && self.photo_generation == studio.photo.sel_version
            && self.generation == studio.canvas_gen
            && self.selection == studio.selection
            && self.pixel_generation == studio.pixel_sel_gen
            && self.tool == studio.tool
            && self.persona == studio.persona
            && self.layer
                == studio
                    .active_layer
                    .and_then(|i| studio.doc.layers.get(i).map(|l| l.id))
            && self.photo
                == studio
                    .photo
                    .selected
                    .and_then(|i| studio.photo.images.get(i).map(|p| (i, p.develop.clone())))
    }

    fn parse(&self) -> Result<Vec<Pt>, String> {
        let mut result = Vec::new();
        for pair in self.points.split(';') {
            let Some((x, y)) = pair.trim().split_once(',') else {
                return Err("Enter points as x,y; x,y.".into());
            };
            let number = |text: &str| {
                text.trim()
                    .parse::<f32>()
                    .ok()
                    .filter(|v| v.is_finite() && v.abs() <= 1_000_000.)
            };
            let (Some(x), Some(y)) = (number(x), number(y)) else {
                return Err("Each coordinate must be a finite number within 1000000 px.".into());
            };
            result.push(Pt::new(x, y));
            if result.len() > 4096 {
                return Err("Use at most 4096 points.".into());
            }
        }
        Ok(result)
    }

    fn apply(&self, studio: &mut Studio) -> Result<(), String> {
        if !self.current(studio) {
            return Err(
                "The document, target or selection changed. Open Tool points again.".into(),
            );
        }
        if studio.show_welcome || studio.free_transform.is_some() || studio.deformation.is_some() {
            return Err("Finish the current edit first.".into());
        }
        super::canvas::keyboard_gesture(
            studio,
            &self.parse()?,
            self.add,
            self.subtract,
            self.source,
        )
    }
}

pub(super) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<Dialog>(Id::new(ID)).is_some())
}

/// Open explicit coordinates for the active tool.
/// Args: context and editor. Returns after capturing the current document, tool and selection.
pub(crate) fn open(ctx: &egui::Context, studio: &Studio) {
    if studio.show_welcome
        || studio.file_dialog_pending()
        || studio.updates.freezing
        || studio.type_edit.is_some()
        || studio.op.is_some()
    {
        return;
    }
    let first = studio.cursor.unwrap_or(Pt::new(8., 8.));
    let last = first + Pt::new(24., 24.);
    let dialog = Dialog {
        document: studio.swap_id.clone(),
        generation: studio.canvas_gen,
        selection: studio.selection.clone(),
        layer: studio
            .active_layer
            .and_then(|i| studio.doc.layers.get(i).map(|l| l.id)),
        pixel_generation: studio.pixel_sel_gen,
        photo: studio
            .photo
            .selected
            .and_then(|i| studio.photo.images.get(i).map(|p| (i, p.develop.clone()))),
        photo_generation: studio.photo.sel_version,
        tool: studio.tool,
        persona: studio.persona,
        points: format!("{},{}; {},{}", first.x, first.y, last.x, last.y),
        add: false,
        subtract: false,
        source: false,
        focus: true,
        error: None,
    };
    ctx.data_mut(|data| data.insert_temp(Id::new(ID), dialog));
}

pub(super) fn show(ctx: &egui::Context, studio: &mut Studio) {
    let Some(mut dialog) = ctx.data(|data| data.get_temp::<Dialog>(Id::new(ID))) else {
        return;
    };
    let current = dialog.current(studio);
    let mut apply = false;
    let mut cancel = false;
    let scope = Id::new((
        ID,
        &dialog.document,
        dialog.layer,
        &dialog.selection,
        dialog.tool,
        dialog.pixel_generation,
        dialog.photo.as_ref().map(|(index, _)| *index),
        dialog.photo_generation,
    ));
    let modal = egui::Modal::new(Id::new(ID)).show(ctx, |ui| {
        ui.set_width(380.);
        ui.heading(format!("{} with keyboard", dialog.tool.label()));
        if dialog.persona == Persona::Photo && let Some(image) = studio.photo.selected() {
            let (width, height) = image.develop.output_dim(image.preview.w, image.preview.h);
            ui.label(format!("Photo coordinates: {width} × {height} px"));
        }
        ui.label("Enter canvas coordinates in pixels. Use one point to click or sample, two to draw a line or region, and more to follow a path.");
        ui.push_id(scope, |ui| {
            let error = dialog.parse().err();
            let response = super::keyboard_authoring::input(ui, "tool-points", "Tool points (x,y; x,y)", &mut dialog.points, error.as_deref(), true);
            if dialog.focus { response.request_focus(); }
            ui.checkbox(&mut dialog.add, "Extend selection / constrain shape");
            ui.checkbox(&mut dialog.subtract, "Subtract selection / alternate tool action");
            if matches!(dialog.tool, Tool::Clone | Tool::Heal) { ui.checkbox(&mut dialog.source, "Set clone or healing source"); }
            if !current { ui.colored_label(ui.visuals().error_fg_color, "The target changed. Cancel and open Tool points again."); }
            if let Some(error) = &dialog.error {
                let warning = ui.colored_label(ui.visuals().error_fg_color, error);
                ui.ctx().accesskit_node_builder(warning.id, |node| node.set_live(egui::accesskit::Live::Polite));
            }
            let enabled = current && dialog.parse().is_ok() && !studio.file_dialog_pending() && !studio.updates.freezing;
            ui.horizontal(|ui| {
                apply = ui.add_enabled(enabled, egui::Button::new("Apply tool").shortcut_text("Ctrl+Enter")).clicked();
                cancel = ui.button("Cancel").clicked();
            });
            if enabled && ui.input_mut(|input| input.consume_key(Modifiers::CTRL, Key::Enter) || input.consume_key(Modifiers::COMMAND, Key::Enter)) { apply = true; }
        });
    });
    dialog.focus = false;
    let applied = if apply {
        match dialog.apply(studio) {
            Ok(()) => true,
            Err(error) => {
                dialog.error = Some(error);
                false
            }
        }
    } else {
        false
    };
    let close = applied || cancel || modal.should_close();
    ctx.data_mut(|data| {
        if close {
            data.remove::<Dialog>(Id::new(ID));
        } else {
            data.insert_temp(Id::new(ID), dialog);
        }
    });
    if close {
        ctx.memory_mut(|memory| {
            memory.request_focus(Id::new(if studio.persona == Persona::Photo {
                "studio-photo-canvas"
            } else {
                "studio-canvas"
            }))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: Key, modifiers: Modifiers) -> Vec<egui::Event> {
        [true, false]
            .into_iter()
            .map(|pressed| egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers,
            })
            .collect()
    }

    fn frame(
        ctx: &egui::Context,
        studio: &mut Studio,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                events,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 720.),
                )),
                ..Default::default()
            },
            |ui| {
                studio.handle_shortcuts(ui.ctx());
                ui.interact(
                    ui.max_rect(),
                    Id::new(if studio.persona == Persona::Photo {
                        "studio-photo-canvas"
                    } else {
                        "studio-canvas"
                    }),
                    egui::Sense::click(),
                );
                show(ui.ctx(), studio);
            },
        );
        output.textures_delta.clear();
        output
    }

    fn apply_points(ctx: &egui::Context, studio: &mut Studio, points: &str) {
        frame(ctx, studio, key(Key::F9, Modifiers::NONE));
        frame(ctx, studio, vec![]);
        assert!(is_open(ctx));
        frame(
            ctx,
            studio,
            key(Key::A, Modifiers::CTRL | Modifiers::COMMAND),
        );
        frame(ctx, studio, vec![egui::Event::Text(points.into())]);
        frame(
            ctx,
            studio,
            key(Key::Enter, Modifiers::CTRL | Modifiers::COMMAND),
        );
        frame(ctx, studio, vec![]);
        assert!(!is_open(ctx), "{}: {}", studio.tool.label(), studio.status);
    }

    #[test]
    fn keyboard_tools_create_layer_select_paint_undo_and_reopen_without_pointer() {
        let ctx = egui::Context::default();
        let mut studio = Studio::new();
        studio.doc = crate::document::Document::new("Keyboard pixels", 32., 32., 72.);
        studio.show_welcome = false;
        studio.persona = Persona::Pixel;
        assert!(studio.selection.is_empty());
        frame(
            &ctx,
            &mut studio,
            key(
                Key::N,
                Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
            ),
        );
        let layer = studio.raster_target().unwrap();
        let original = studio.doc.layers[layer].kind.pixels().unwrap().data.clone();
        frame(&ctx, &mut studio, key(Key::M, Modifiers::SHIFT));
        apply_points(&ctx, &mut studio, "4,4; 20,20");
        assert!(studio.pixel_sel.as_ref().unwrap().iter().any(|n| *n > 0));
        assert!(studio.pixel_sel.as_ref().unwrap().iter().any(|n| *n == 0));
        frame(&ctx, &mut studio, key(Key::B, Modifiers::NONE));
        let history = studio.history.len();
        apply_points(&ctx, &mut studio, "8,8; 12,8; 12,12");
        assert_eq!(studio.history.len(), history + 1);
        let painted = studio.doc.layers[layer].kind.pixels().unwrap().data.clone();
        assert_ne!(painted, original);
        frame(
            &ctx,
            &mut studio,
            key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND),
        );
        assert_eq!(
            studio.doc.layers[layer].kind.pixels().unwrap().data,
            original
        );
        frame(
            &ctx,
            &mut studio,
            key(
                Key::Z,
                Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
            ),
        );
        assert_eq!(
            studio.doc.layers[layer].kind.pixels().unwrap().data,
            painted
        );
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".artifacts")
            .join(format!(
                "keyboard-pixels-test-{}-{}",
                std::process::id(),
                crate::document::next_id()
            ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("pixels.oma");
        crate::project::save_to(&studio.doc, &path).unwrap();
        let reopened = crate::project::load_from(&path).unwrap();
        assert_eq!(reopened.layers[layer].kind.pixels().unwrap().data, painted);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn keyboard_tools_photo_crop_uses_real_photo_history_and_rejects_stale_target() {
        let ctx = egui::Context::default();
        let mut studio = Studio::new();
        studio.doc = crate::document::Document::new("Keyboard photo", 32., 32., 72.);
        studio.show_welcome = false;
        studio.persona = Persona::Photo;
        studio
            .photo
            .images
            .push(crate::photo::PhotoImage::from_full(
                "Keyboard photo".into(),
                crate::photo::RgbaImage::new(32, 32, vec![128; 32 * 32 * 4]).unwrap(),
            ));
        assert!(studio.photo.selected.is_none());
        frame(&ctx, &mut studio, key(Key::PageDown, Modifiers::NONE));
        assert_eq!(studio.photo.selected, Some(0));
        frame(&ctx, &mut studio, key(Key::C, Modifiers::NONE));
        apply_points(&ctx, &mut studio, "4,4; 28,28");
        assert_eq!(
            studio.photo.selected().unwrap().develop.crop,
            Some([0.125, 0.125, 0.875, 0.875])
        );
        frame(
            &ctx,
            &mut studio,
            key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND),
        );
        assert_eq!(studio.photo.selected().unwrap().develop.crop, None);
        frame(&ctx, &mut studio, key(Key::F9, Modifiers::NONE));
        let draft = ctx.data(|d| d.get_temp::<Dialog>(Id::new(ID))).unwrap();
        studio.photo.images[0].develop.exposure = 1.;
        assert!(draft.apply(&mut studio).unwrap_err().contains("changed"));
    }

    #[test]
    fn keyboard_tools_reject_nonfinite_locked_busy_and_selection_change() {
        let ctx = egui::Context::default();
        let mut studio = Studio::new();
        studio.doc = crate::document::Document::new("Keyboard guards", 32., 32., 72.);
        studio.show_welcome = false;
        studio.persona = Persona::Pixel;
        studio.add_layer(true);
        studio.set_tool(Tool::Brush);
        open(&ctx, &studio);
        let draft = ctx.data(|d| d.get_temp::<Dialog>(Id::new(ID))).unwrap();
        let mut invalid = draft.clone();
        invalid.points = "NaN,1; 2,3".into();
        assert!(invalid.apply(&mut studio).is_err());
        let mut too_long = draft.clone();
        too_long.points = "0,0; 1000000,1000000".into();
        assert!(too_long.apply(&mut studio).unwrap_err().contains("shorter"));
        let layer = studio.raster_target().unwrap();
        studio.doc.layers[layer].locked = true;
        assert!(draft.apply(&mut studio).unwrap_err().contains("unlocked"));
        studio.doc.layers[layer].locked = false;
        studio.updates.freezing = true;
        assert!(draft.apply(&mut studio).is_err());
        studio.updates.freezing = false;
        studio.pixel_sel_gen += 1;
        assert!(draft.apply(&mut studio).unwrap_err().contains("changed"));
    }
}
