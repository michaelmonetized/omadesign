//! Sidebar rows borrow no geometry and never copy object masks or text layout.
use crate::document::Shape;
use crate::geom::Geom;
use eframe::egui;

pub(super) const HEIGHT: f32 = 30.0;

pub(super) struct ObjectRow {
    pub index: usize,
    pub visual_index: usize,
    pub id: u64,
    pub name: String,
    pub icon: &'static str,
    pub visible: bool,
    pub locked: bool,
    pub guide: bool,
    pub text: bool,
    pub overflow: bool,
    pub effects: bool,
    pub masked: bool,
}

pub(super) fn snapshot(
    shapes: &[Shape],
    top: f32,
    clip: egui::Rect,
    spacing: f32,
    rename: Option<u64>,
) -> Vec<ObjectRow> {
    let pitch = HEIGHT + spacing;
    let count = shapes.len();
    let first = (((clip.top() - top) / pitch).floor().max(0.0) as usize).min(count);
    let end = (((clip.bottom() - top) / pitch).ceil().max(0.0) as usize).min(count);
    let mut visible: Vec<_> = (first..end).collect();
    // Keep a live text field in egui's focus lifecycle even when scrolled out.
    if let Some(id) = rename
        && let Some(index) = shapes.iter().position(|shape| shape.id == id)
    {
        let visual = count - 1 - index;
        if !visible.contains(&visual) {
            visible.push(visual);
            visible.sort_unstable();
        }
    }
    visible.into_iter().map(|visual_index| {
        let index = count - 1 - visual_index;
        let shape = &shapes[index];
        ObjectRow {
            index,
            visual_index,
            id: shape.id,
            name: shape.name.clone(),
            icon: super::geometry_icon(&shape.geom),
            visible: shape.visible,
            locked: shape.locked,
            guide: shape.guide,
            text: matches!(shape.geom, Geom::Text(_)),
            overflow: matches!(&shape.geom, Geom::Text(run) if run.layout.as_ref().is_some_and(|layout| layout.overflow)),
            effects: shape.filters.active(),
            masked: shape.mask.is_some(),
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Studio;
    use crate::{
        document::{Pixels, Style},
        geom::Pt,
    };

    #[test]
    fn large_expanded_layers_only_snapshot_visible_rows_and_a_live_rename() {
        let mut shapes: Vec<_> = (0..2000)
            .map(|index| {
                let mut shape = Shape::new(
                    Geom::Rect {
                        origin: Pt::ZERO,
                        size: Pt::splat(10.0),
                        radius: 0.0,
                    },
                    Style::default(),
                );
                shape.name = format!("Object {index}");
                shape
            })
            .collect();
        shapes[1994].mask = Some(Pixels::new(2048, 2048));
        let storage = shapes[1994].mask.as_ref().unwrap().data.as_ptr();
        let clip = egui::Rect::from_min_max(egui::pos2(0.0, 180.0), egui::pos2(300.0, 288.0));
        let rows = snapshot(&shapes, 0.0, clip, 6.0, Some(shapes[0].id));
        assert_eq!(
            rows.iter().map(|row| row.visual_index).collect::<Vec<_>>(),
            [5, 6, 7, 1999]
        );
        assert_eq!(rows[0].name, "Object 1994");
        assert!(rows[0].masked);
        assert_eq!(rows[3].name, "Object 0");
        assert_eq!(shapes[1994].mask.as_ref().unwrap().data.as_ptr(), storage);
        assert!(snapshot(&shapes, 400.0, clip, 6.0, None).is_empty());
    }

    fn studio() -> Studio {
        let mut studio = Studio::new();
        studio.doc = crate::document::Document::new("Rows", 8.0, 8.0, 72.0);
        studio.show_welcome = false;
        studio.active_layer = Some(1);
        studio.layer_expanded.insert(studio.doc.layers[1].id);
        studio.doc.layers[1]
            .kind
            .shapes_mut()
            .unwrap()
            .extend((0..200).map(|i| {
                let mut shape = Shape::new(
                    Geom::Rect {
                        origin: Pt::ZERO,
                        size: Pt::splat(10.0),
                        radius: 0.0,
                    },
                    Style::default(),
                );
                shape.name = format!("Object {i}");
                shape
            }));
        studio
    }

    fn frame(
        ctx: &egui::Context,
        studio: &mut Studio,
        offset: Option<f32>,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Rect)> {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(360.0, 420.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let mut scroll = egui::ScrollArea::vertical()
                    .id_salt("row-qa")
                    .auto_shrink([false, false]);
                if let Some(offset) = offset {
                    scroll = scroll.vertical_scroll_offset(offset);
                }
                scroll.show(ui, |ui| super::super::layers_studio(ui, studio));
            },
        );
        let mut labels: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| {
                let egui::Shape::Text(text) = &shape.shape else {
                    return None;
                };
                let rect = text.galley.rect.translate(text.pos.to_vec2());
                (text.galley.text().starts_with("Object ") && shape.clip_rect.contains_rect(rect))
                    .then(|| (text.galley.text().to_owned(), rect))
            })
            .collect();
        labels.sort_by(|a, b| a.1.top().total_cmp(&b.1.top()));
        output.textures_delta.clear();
        labels
    }

    fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn virtual_rows_scroll_select_drag_and_undo_the_visible_objects() {
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut studio = studio();
        for _ in 0..3 {
            frame(&ctx, &mut studio, None, vec![]);
        }
        frame(&ctx, &mut studio, Some(3600.0), vec![]);
        let rows = frame(&ctx, &mut studio, Some(3600.0), vec![]);
        assert!(
            rows.len() >= 4 && rows.len() < 15,
            "only visible rows should be drawn"
        );
        let name = &rows[0].0;
        let index: usize = name.trim_start_matches("Object ").parse().unwrap();
        assert!(index < 120, "scroll must expose middle objects");
        let id = studio.doc.layers[1].kind.shapes().unwrap()[index].id;
        let from = rows[0].1.center();
        frame(
            &ctx,
            &mut studio,
            None,
            vec![egui::Event::PointerMoved(from)],
        );
        for pressed in [true, false] {
            frame(&ctx, &mut studio, None, vec![button(from, pressed)]);
        }
        assert_eq!(studio.selection, vec![(1, id)]);
        let order = |studio: &Studio| {
            studio.doc.layers[1]
                .kind
                .shapes()
                .unwrap()
                .iter()
                .map(|shape| shape.id)
                .collect::<Vec<_>>()
        };
        let before = order(&studio);
        let to = rows[3].1.center();
        frame(&ctx, &mut studio, None, vec![button(from, true)]);
        frame(
            &ctx,
            &mut studio,
            None,
            vec![egui::Event::PointerMoved(from + egui::vec2(0.0, 10.0))],
        );
        frame(&ctx, &mut studio, None, vec![egui::Event::PointerMoved(to)]);
        frame(&ctx, &mut studio, None, vec![button(to, false)]);
        assert_ne!(
            order(&studio),
            before,
            "dragging visible rows must reorder the actual objects"
        );
        studio.undo();
        assert_eq!(order(&studio), before);
    }

    #[test]
    fn an_offscreen_rename_keeps_focus_and_commits_on_enter() {
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut studio = studio();
        let id = studio.doc.layers[1].kind.shapes().unwrap()[199].id;
        studio.shape_rename = Some((1, id, "Object 199".into()));
        let mut rows = vec![];
        for _ in 0..3 {
            rows = frame(&ctx, &mut studio, None, vec![]);
        }
        let point = rows
            .iter()
            .find(|(name, _)| name == "Object 199")
            .unwrap()
            .1
            .center();
        frame(
            &ctx,
            &mut studio,
            None,
            vec![egui::Event::PointerMoved(point)],
        );
        for pressed in [true, false] {
            frame(&ctx, &mut studio, None, vec![button(point, pressed)]);
        }
        let focus = ctx
            .memory(|memory| memory.focused())
            .expect("rename text field focus");
        frame(&ctx, &mut studio, Some(3600.0), vec![]);
        frame(&ctx, &mut studio, Some(3600.0), vec![]);
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(focus));
        let command = egui::Modifiers {
            ctrl: true,
            command: true,
            ..egui::Modifiers::NONE
        };
        frame(
            &ctx,
            &mut studio,
            None,
            vec![egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: command,
            }],
        );
        frame(
            &ctx,
            &mut studio,
            None,
            vec![egui::Event::Text("Renamed while offscreen".into())],
        );
        frame(
            &ctx,
            &mut studio,
            None,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(studio.shape_rename.is_none());
        assert_eq!(
            studio.doc.find_shape(1, id).unwrap().name,
            "Renamed while offscreen"
        );
    }
}
