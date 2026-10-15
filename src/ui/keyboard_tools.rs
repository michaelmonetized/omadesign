use crate::{
    app::Studio,
    geom::Pt,
    tools::{Persona, Tool},
};
use eframe::egui::{self, Id, Key, Modifiers};

const ID: &str = "keyboard-tool-points";

#[derive(Clone, PartialEq)]
enum GeometryTarget {
    Pixel(
        crate::app::pixel_edit::Geometry,
        crate::app::pixel_edit::ModeKind,
    ),
    Vector(crate::deform::Cage),
    Transform(Vec<Pt>),
}

impl GeometryTarget {
    fn handles(&self) -> Vec<Pt> {
        match self {
            Self::Pixel(crate::app::pixel_edit::Geometry::Cage(cage), _) | Self::Vector(cage) => {
                cage.handles()
            }
            Self::Transform(handles) => handles.clone(),
            _ => vec![],
        }
    }

    fn path(&self) -> bool {
        matches!(
            self,
            Self::Pixel(crate::app::pixel_edit::Geometry::Path { .. }, _)
        )
    }
}

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
    geometry: Option<GeometryTarget>,
    handle: usize,
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
            && self.geometry == geometry_target(studio)
    }

    fn anchors(&self) -> Result<Vec<crate::geom::Anchor>, String> {
        let mut anchors = Vec::new();
        for row in self.points.split(';') {
            let values = row
                .split(',')
                .map(|part| part.trim().parse::<f32>())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| {
                    "Use x,y or x,y,in dx,in dy,out dx,out dy for each anchor.".to_owned()
                })?;
            if !matches!(values.len(), 2 | 6)
                || values
                    .iter()
                    .any(|value| !value.is_finite() || value.abs() > 1_000_000.)
            {
                return Err(
                    "Each anchor needs 2 or 6 finite coordinates within 1000000 px.".into(),
                );
            }
            let mut anchor = crate::geom::Anchor::corner(Pt::new(values[0], values[1]));
            if values.len() == 6 {
                anchor.h_in = Pt::new(values[2], values[3]);
                anchor.h_out = Pt::new(values[4], values[5]);
            }
            anchors.push(anchor);
            if anchors.len() > 4096 {
                return Err("Use at most 4096 anchors.".into());
            }
        }
        if anchors.len() < 3 {
            return Err("A closed selection path needs at least three anchors.".into());
        }
        Ok(anchors)
    }

    fn validation(&self) -> Option<String> {
        if self.geometry.as_ref().is_some_and(GeometryTarget::path) {
            self.anchors().err()
        } else {
            self.parse().err()
        }
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
        if studio.show_welcome
            || studio.file_dialog_pending()
            || studio.updates.freezing
            || studio.op.is_some()
            || studio.type_edit.is_some()
        {
            return Err("Finish the current edit first.".into());
        }
        if let Some(target) = &self.geometry {
            return self.apply_geometry(studio, target);
        }
        super::canvas::keyboard_gesture(
            studio,
            &self.parse()?,
            self.add,
            self.subtract,
            self.source,
        )
    }

    fn apply_geometry(&self, studio: &mut Studio, target: &GeometryTarget) -> Result<(), String> {
        use crate::app::pixel_edit::Geometry;
        match target {
            GeometryTarget::Pixel(before, mode) => {
                if studio.raster_target().is_none() {
                    return Err("Choose a visible, unlocked pixel layer.".into());
                }
                let after = match before {
                    Geometry::Path { .. } => Geometry::Path {
                        anchors: self.anchors()?,
                        closed: true,
                    },
                    Geometry::Cage(cage) => {
                        let point = self.parse()?[0];
                        let start = if self.handle == usize::MAX {
                            cage.bounds.min
                        } else {
                            *cage
                                .handles()
                                .get(self.handle)
                                .ok_or("The handle no longer exists.")?
                        };
                        let after = if self.handle == usize::MAX {
                            cage.translated(point - start)
                        } else if *mode == crate::app::pixel_edit::ModeKind::Transform {
                            cage.resized(self.handle, point - start, self.add)
                        } else {
                            cage.dragged(self.handle, point - start)
                        };
                        Geometry::Cage(after.ok_or(
                            "Keep the envelope open; crossed or collapsed corners are invalid.",
                        )?)
                    }
                };
                let editor = studio
                    .pixel_edit
                    .as_mut()
                    .ok_or("The selection editor closed.")?;
                if editor.drag.is_some() || editor.finish {
                    return Err("Finish the current selection edit first.".into());
                }
                editor.geometry = after;
                editor.record(before.clone());
                studio.status = "Selection geometry applied · Enter finishes · Esc cancels".into();
            }
            GeometryTarget::Vector(cage) => {
                if !studio.can_deform()
                    || studio
                        .deformation
                        .as_ref()
                        .is_none_or(|session| session.dragging())
                {
                    return Err(
                        "Choose visible, unlocked vector objects and finish the current drag."
                            .into(),
                    );
                }
                let start = *cage
                    .handles()
                    .get(self.handle)
                    .ok_or("The handle no longer exists.")?;
                let point = self.parse()?[0];
                cage.dragged(self.handle, point - start)
                    .ok_or("Keep the envelope open; crossed or collapsed corners are invalid.")?;
                studio.deformation_drag_start(self.handle, start);
                if !studio
                    .deformation
                    .as_ref()
                    .is_some_and(|session| session.dragging())
                {
                    return Err(studio.status.clone());
                }
                studio.deformation_drag_to(point, self.add);
                studio.deformation_drag_finish();
            }
            GeometryTarget::Transform(handles) => {
                if studio.selection.is_empty()
                    || studio.selection.iter().any(|&(layer, id)| {
                        !studio.doc.layer_editable(layer)
                            || (id != crate::document::RASTER_ID
                                && !studio
                                    .doc
                                    .find_shape(layer, id)
                                    .is_some_and(|shape| shape.visible && !shape.locked))
                    })
                {
                    return Err("Choose visible, unlocked artwork to transform.".into());
                }
                let start = *handles
                    .get(self.handle)
                    .ok_or("The handle no longer exists.")?;
                super::canvas::keyboard_gesture(
                    studio,
                    &[start, self.parse()?[0]],
                    self.add,
                    self.subtract,
                    false,
                )?;
            }
        }
        Ok(())
    }
}

fn geometry_target(studio: &Studio) -> Option<GeometryTarget> {
    studio
        .pixel_edit
        .as_ref()
        .map(|editor| GeometryTarget::Pixel(editor.geometry.clone(), editor.mode))
        .or_else(|| {
            studio
                .deformation
                .as_ref()
                .map(|session| GeometryTarget::Vector(session.cage.clone()))
        })
        .or_else(|| {
            let handles = super::canvas::keyboard_transform_handles(studio);
            (!handles.is_empty()).then_some(GeometryTarget::Transform(handles))
        })
}

pub(super) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<Dialog>(Id::new(ID)).is_some())
}

/// Open explicit coordinates for the active tool.
/// Args: context and editor. Returns after capturing the current document, tool and selection.
pub(crate) fn open(ctx: &egui::Context, studio: &mut Studio) {
    if studio.show_welcome
        || studio.file_dialog_pending()
        || studio.updates.freezing
        || studio.type_edit.is_some()
        || studio.op.is_some()
    {
        return;
    }
    if studio.persona == Persona::Pixel
        && studio.tool == Tool::BezierLasso
        && studio.pixel_edit.is_none()
    {
        studio.begin_pixel_edit(crate::app::pixel_edit::ModeKind::Bezier);
    }
    let geometry = geometry_target(studio);
    let first = studio.cursor.unwrap_or(Pt::new(8., 8.));
    let last = first + Pt::new(24., 24.);
    let points = match &geometry {
        Some(GeometryTarget::Pixel(crate::app::pixel_edit::Geometry::Path { anchors, .. }, _))
            if !anchors.is_empty() =>
        {
            anchors
                .iter()
                .map(|a| {
                    format!(
                        "{},{},{},{},{},{}",
                        a.pt.x, a.pt.y, a.h_in.x, a.h_in.y, a.h_out.x, a.h_out.y
                    )
                })
                .collect::<Vec<_>>()
                .join("; ")
        }
        Some(target) if target.path() => "8,8; 32,8; 32,32; 8,32".into(),
        Some(target) => {
            let p = target.handles()[0];
            format!("{},{}", p.x, p.y)
        }
        None => format!("{},{}; {},{}", first.x, first.y, last.x, last.y),
    };
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
        geometry,
        handle: 0,
        points,
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
        dialog.geometry.as_ref().map(|target| match target {
            GeometryTarget::Pixel(_, mode) => format!("pixel-{mode:?}"),
            GeometryTarget::Vector(cage) => format!("vector-{:?}", cage.mode),
            GeometryTarget::Transform(_) => "transform".into(),
        }),
    ));
    let modal = egui::Modal::new(Id::new(ID)).show(ctx, |ui| {
        ui.set_width(380.);
        ui.heading(if dialog.geometry.is_some() { "Edit geometry with keyboard".into() } else { format!("{} with keyboard", dialog.tool.label()) });
        if dialog.persona == Persona::Photo && let Some(image) = studio.photo.selected() {
            let (width, height) = image.develop.output_dim(image.preview.w, image.preview.h);
            ui.label(format!("Photo coordinates: {width} × {height} px"));
        }
        let path = dialog.geometry.as_ref().is_some_and(GeometryTarget::path);
        ui.label(if path { "Enter image-pixel anchors. Optional handle offsets preserve curves: x,y,in dx,in dy,out dx,out dy; next anchor. Apply closes the path; Enter on the canvas finishes." }
            else if dialog.geometry.is_some() { "Choose a handle and enter its destination in pixels. Apply uses the active geometry editor; Enter on the canvas finishes." }
            else { "Enter canvas coordinates in pixels. Use one point to click or sample, two to draw a line or region, and more to follow a path." });
        ui.push_id(scope, |ui| {
            if let Some(target) = &dialog.geometry && !path {
                let label = ui.label("Geometry handle");
                let selected = if dialog.handle == usize::MAX { "Move entire selection".into() } else { format!("Handle {}", dialog.handle + 1) };
                let mut changed = false;
                let picker = egui::ComboBox::from_id_salt("geometry-handle").selected_text(selected).show_ui(ui, |ui| {
                    for (index, point) in target.handles().into_iter().enumerate() {
                        if ui.selectable_value(&mut dialog.handle, index, format!("Handle {} ({}, {})", index + 1, point.x, point.y)).clicked() {
                            dialog.points = format!("{},{}", point.x, point.y);
                            changed = true;
                        }
                    }
                    if let GeometryTarget::Pixel(crate::app::pixel_edit::Geometry::Cage(cage), _) = target
                        && ui.selectable_value(&mut dialog.handle, usize::MAX, "Move entire selection").clicked()
                    { dialog.points = format!("{},{}", cage.bounds.min.x, cage.bounds.min.y); changed = true; }
                }).response.labelled_by(label.id);
                if changed { picker.request_focus(); }
            }
            let error = dialog.validation();
            let response = ui.push_id(dialog.handle, |ui| super::keyboard_authoring::input(ui, "tool-points", if path { "Selection anchors and handle offsets (px)" } else { "Tool points (x,y; x,y)" }, &mut dialog.points, error.as_deref(), true)).inner;
            if dialog.focus { response.request_focus(); }
            if dialog.geometry.is_none() {
                ui.checkbox(&mut dialog.add, "Extend selection / constrain shape");
                ui.checkbox(&mut dialog.subtract, "Subtract selection / alternate tool action");
            } else if !path { ui.checkbox(&mut dialog.add, "Constrain geometry"); }
            if matches!(dialog.tool, Tool::Clone | Tool::Heal) { ui.checkbox(&mut dialog.source, "Set clone or healing source"); }
            if !current { ui.colored_label(ui.visuals().error_fg_color, "The target changed. Cancel and open Tool points again."); }
            if let Some(error) = &dialog.error {
                let warning = ui.colored_label(ui.visuals().error_fg_color, error);
                ui.ctx().accesskit_node_builder(warning.id, |node| node.set_live(egui::accesskit::Live::Polite));
            }
            let enabled = current && dialog.validation().is_none() && !studio.file_dialog_pending() && !studio.updates.freezing;
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
                if studio.persona == Persona::Design {
                    super::super::deform::inspector(ui, studio);
                }
                ui.interact(
                    ui.max_rect(),
                    Id::new(if studio.persona == Persona::Photo {
                        "studio-photo-canvas"
                    } else {
                        "studio-canvas"
                    }),
                    egui::Sense::click(),
                );
                studio.poll_pixel_edit(ui.ctx());
                super::super::keyboard_authoring::show(ui.ctx(), studio);
                show(ui.ctx(), studio);
            },
        );
        output.textures_delta.clear();
        output
    }

    fn settle_selection(ctx: &egui::Context, studio: &mut Studio) {
        let start = std::time::Instant::now();
        loop {
            frame(ctx, studio, vec![]);
            if studio
                .pixel_edit
                .as_ref()
                .is_none_or(|edit| !edit.pending() && edit.job.is_none())
            {
                return;
            }
            assert!(
                start.elapsed() < std::time::Duration::from_secs(3),
                "selection render did not complete"
            );
            std::thread::yield_now();
        }
    }

    fn tab_to(ctx: &egui::Context, studio: &mut Studio, name: &str) {
        for _ in 0..64 {
            let output = frame(ctx, studio, vec![]);
            let tree = output.platform_output.accesskit_update.unwrap();
            let id = tree
                .nodes
                .iter()
                .find(|(_, node)| node.label() == Some(name))
                .unwrap()
                .0;
            if ctx
                .memory(|memory| memory.focused())
                .is_some_and(|focus| focus.accesskit_id() == id)
            {
                return;
            }
            frame(ctx, studio, key(Key::Tab, Modifiers::NONE));
        }
        panic!("Tab did not reach {name}");
    }

    #[test]
    fn keyboard_geometry_authors_curved_selection_then_resizes_and_undoes_without_pointer() {
        use crate::app::pixel_edit::Geometry;
        let ctx = egui::Context::default();
        let mut studio = Studio::new();
        studio.doc = crate::document::Document::new("Keyboard selection geometry", 32., 32., 72.);
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
        let pixels = studio.doc.layers[layer].kind.pixels().unwrap().data.clone();
        let history = studio.history.len();
        frame(&ctx, &mut studio, key(Key::Q, Modifiers::SHIFT));
        apply_points(
            &ctx,
            &mut studio,
            "8,8,0,0,4,-2; 24,8,-4,-2,0,0; 24,24; 8,24",
        );
        settle_selection(&ctx, &mut studio);
        let path = studio.pixel_selection_path().unwrap().to_vec();
        assert_eq!(path[0].h_out, Pt::new(4., -2.));
        let coverage = studio.pixel_sel.clone().unwrap();
        assert!(coverage.iter().any(|value| *value != 0));
        frame(
            &ctx,
            &mut studio,
            key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND),
        );
        assert!(
            matches!(&studio.pixel_edit.as_ref().unwrap().geometry, Geometry::Path { anchors, closed: false } if anchors.is_empty())
        );
        frame(
            &ctx,
            &mut studio,
            key(
                Key::Z,
                Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
            ),
        );
        settle_selection(&ctx, &mut studio);
        frame(&ctx, &mut studio, key(Key::Enter, Modifiers::NONE));
        settle_selection(&ctx, &mut studio);
        assert!(studio.pixel_edit.is_none());
        frame(
            &ctx,
            &mut studio,
            key(Key::T, Modifiers::CTRL | Modifiers::COMMAND),
        );
        assert!(studio.pixel_edit.is_some());
        let before = studio.pixel_edit.as_ref().unwrap().geometry.clone();
        apply_points(&ctx, &mut studio, "4,4");
        settle_selection(&ctx, &mut studio);
        assert_ne!(studio.pixel_edit.as_ref().unwrap().geometry, before);
        assert_ne!(studio.pixel_sel.as_ref().unwrap(), &coverage);
        frame(
            &ctx,
            &mut studio,
            key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND),
        );
        settle_selection(&ctx, &mut studio);
        assert_eq!(studio.pixel_edit.as_ref().unwrap().geometry, before);
        assert_eq!(studio.doc.layers[layer].kind.pixels().unwrap().data, pixels);
        assert_eq!(studio.history.len(), history);
    }

    #[test]
    fn keyboard_geometry_reshapes_created_artwork_with_native_mode_and_one_command() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut studio = Studio::new();
        studio.doc = crate::document::Document::new("Keyboard envelope", 128., 96., 72.);
        studio.show_welcome = false;
        frame(&ctx, &mut studio, key(Key::R, Modifiers::NONE));
        frame(&ctx, &mut studio, key(Key::Enter, Modifiers::NONE));
        frame(&ctx, &mut studio, vec![]);
        frame(
            &ctx,
            &mut studio,
            key(Key::Enter, Modifiers::CTRL | Modifiers::COMMAND),
        );
        frame(&ctx, &mut studio, vec![]);
        let (layer, id) = studio.primary().unwrap();
        let original = studio.doc.find_shape(layer, id).unwrap().geom.clone();
        tab_to(&ctx, &mut studio, "Distort");
        frame(&ctx, &mut studio, key(Key::Enter, Modifiers::NONE));
        frame(&ctx, &mut studio, vec![]);
        assert!(studio.deformation.is_some());
        frame(&ctx, &mut studio, key(Key::F8, Modifiers::NONE));
        let history = studio.history.len();
        apply_points(&ctx, &mut studio, "0,-10");
        assert_ne!(studio.doc.find_shape(layer, id).unwrap().geom, original);
        assert_eq!(studio.history.len(), history + 1);
        frame(&ctx, &mut studio, key(Key::Enter, Modifiers::NONE));
        assert!(studio.deformation.is_none());
        frame(
            &ctx,
            &mut studio,
            key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND),
        );
        assert_eq!(studio.doc.find_shape(layer, id).unwrap().geom, original);
        frame(
            &ctx,
            &mut studio,
            key(Key::T, Modifiers::CTRL | Modifiers::COMMAND),
        );
        assert!(studio.free_transform.is_some());
        let history = studio.history.len();
        apply_points(&ctx, &mut studio, "4,4");
        assert_ne!(studio.doc.find_shape(layer, id).unwrap().geom, original);
        assert_eq!(studio.history.len(), history);
        frame(&ctx, &mut studio, key(Key::Enter, Modifiers::NONE));
        assert!(studio.free_transform.is_none());
        assert_eq!(studio.history.len(), history + 1);
        frame(
            &ctx,
            &mut studio,
            key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND),
        );
        assert_eq!(studio.doc.find_shape(layer, id).unwrap().geom, original);
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
        open(&ctx, &mut studio);
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
