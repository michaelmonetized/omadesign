//! Reuse the exact, already-composited backdrop during a direct manipulation.
//!
//! Only a prefix is cached: compositing is not associative with 8-bit rounding,
//! and later blend modes must continue to see the actual backdrop. The caller
//! invalidates this cache on every edit outside the current gesture.
use super::*;

#[derive(Default)]
pub(crate) struct Cache {
    entry: Option<Entry>,
}

#[derive(PartialEq)]
struct Key {
    viewport: [u32; 5],
    background: Rgba,
    changed: Vec<(usize, u64)>,
}

struct Entry {
    key: Key,
    layer: usize,
    shape: Option<usize>,
    backdrop: Pixmap,
}

impl Cache {
    pub(crate) fn clear(&mut self) {
        self.entry = None;
    }

    pub(crate) fn render(
        &mut self,
        doc: &Document,
        view: View,
        width: u32,
        height: u32,
        draft: Draft<'_>,
        changed: &[(usize, u64)],
    ) -> Option<Pixmap> {
        let viewport = [
            width,
            height,
            view.scale.to_bits(),
            view.offset.x.to_bits(),
            view.offset.y.to_bits(),
        ];
        let background = canvas_bg();
        if self.entry.as_ref().is_none_or(|entry| {
            entry.key.viewport != viewport
                || entry.key.background != background
                || entry.key.changed != changed
        }) {
            self.clear();
            let Some((layer, shape)) = split(doc, changed) else {
                return render_view(doc, view, width, height, draft);
            };
            // Bound the extra live canvas allocation; oversized viewports retain
            // the ordinary renderer. Inactive tabs never retain these pixels.
            if u64::from(width) * u64::from(height) * 4 > 64 * 1024 * 1024 {
                return render_view(doc, view, width, height, draft);
            }
            let mut backdrop = Pixmap::new(width, height)?;
            fill_solid(
                &mut backdrop,
                0.,
                0.,
                width as f32,
                height as f32,
                background,
            );
            draw_plates(&mut backdrop, doc, view);
            for below in &doc.layers[..layer] {
                draw_complete_layer(&mut backdrop, below, view.transform(), None, doc);
            }
            if let Some(end) = shape {
                let target = &doc.layers[layer];
                draw_shapes(
                    &mut backdrop,
                    &target.kind.shapes()?[..end],
                    target,
                    view.transform(),
                );
            }
            self.entry = Some(Entry {
                key: Key {
                    viewport,
                    background,
                    changed: changed.to_vec(),
                },
                layer,
                shape,
                backdrop,
            });
        }
        let entry = self.entry.as_ref()?;
        let mut pixels = entry.backdrop.clone();
        for (index, layer) in doc.layers.iter().enumerate().skip(entry.layer) {
            if index == entry.layer
                && let Some(start) = entry.shape
            {
                draw_shapes(
                    &mut pixels,
                    &layer.kind.shapes()?[start..],
                    layer,
                    view.transform(),
                );
            } else {
                let brush = draft
                    .brush
                    .and_then(|(i, pixels, opacity)| (i == index).then_some((pixels, opacity)));
                draw_complete_layer(&mut pixels, layer, view.transform(), brush, doc);
            }
        }
        Some(pixels)
    }
}

fn draw_complete_layer(
    pm: &mut Pixmap,
    layer: &Layer,
    transform: Transform,
    brush: Option<(&Pixmap, f32)>,
    doc: &Document,
) {
    if layer.visible && layer.opacity > 0. && !is_paper_raster(layer) {
        draw_layer(pm, layer, transform, brush, None, None, doc, None);
    }
}

fn draw_shapes(pm: &mut Pixmap, shapes: &[Shape], layer: &Layer, transform: Transform) {
    if layer.visible && layer.opacity > 0. {
        for shape in shapes.iter().filter(|shape| shape.visible && !shape.guide) {
            draw_shape(
                pm,
                shape,
                transform,
                layer.opacity,
                layer.blend.to_skia(),
                Pose::identity(),
            );
        }
    }
}

fn split(doc: &Document, changed: &[(usize, u64)]) -> Option<(usize, Option<usize>)> {
    // A moving wrap obstacle or path guide can reflow text in an earlier layer.
    // Group isolation also requires a hierarchical split, so keep those scenes
    // on the complete renderer until that dependency can be represented here.
    if doc.layers.iter().any(|layer| layer.is_group || layer.parent.is_some())
        || doc.layers.iter().filter_map(|layer| layer.kind.shapes()).flatten().any(|shape| {
            matches!(&shape.geom, Geom::Text(run) if run.on_path.is_some() || run.frame.is_some())
        })
    {
        return None;
    }
    let first = changed.iter().map(|(layer, _)| *layer).min()?;
    let layer = doc.layers.get(first)?;
    let shape = layer
        .kind
        .shapes()
        .filter(|shapes| {
            layer.opacity >= 1.
                && layer.fill_opacity >= 1.
                && layer.mask.is_none()
                && layer.blend == crate::color::Blend::Normal
                && !layer.filters.active()
                && shapes.iter().all(|shape| {
                    !shape.layout.frame
                        && shape.layout.parent.is_none()
                        && shape.blend == crate::color::Blend::Normal
                        && !shape.filters.blends_backdrop()
                })
        })
        .and_then(|shapes| {
            shapes
                .iter()
                .position(|shape| changed.contains(&(first, shape.id)))
        });
    Some((first, shape))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Pixels, Style};

    fn fixture() -> Document {
        let mut doc = Document::new("interaction", 128., 96., 72.);
        doc.layers.clear();
        for layer_index in 0..3 {
            let mut layer = Layer::vector(format!("Layer {layer_index}"));
            for index in 0..8 {
                let mut shape = Shape::new(
                    Geom::Rect {
                        origin: Pt::new(7.3 + index as f32 * 10., 8.8 + layer_index as f32 * 18.),
                        size: Pt::new(23.7, 21.6),
                        radius: 4.,
                    },
                    Style {
                        fill: Fill::Solid(Rgba::rgb(35 + index * 20, 81, 170)),
                        stroke: None,
                    },
                );
                shape.opacity = 0.73;
                layer.kind.shapes_mut().unwrap().push(shape);
            }
            doc.layers.push(layer);
        }
        doc
    }

    #[test]
    fn cached_drag_matches_full_renderer_at_every_position_and_view() {
        let mut doc = fixture();
        let id = doc.layers[1].kind.shapes().unwrap()[3].id;
        let mut cache = Cache::default();
        for scale in [0.7, 1., 2.25] {
            let view = View {
                scale,
                offset: Pt::new(4.3, -2.7),
            };
            for step in 0..8 {
                doc.find_shape_mut(1, id)
                    .unwrap()
                    .geom
                    .translate(Pt::new(step as f32 * 0.7, 0.3));
                let full = render_view(&doc, view, 180, 140, Draft::none()).unwrap();
                let cached = cache
                    .render(&doc, view, 180, 140, Draft::none(), &[(1, id)])
                    .unwrap();
                assert_eq!(cached.data(), full.data());
            }
        }
        assert!(cache.entry.as_ref().unwrap().shape.is_some());
    }

    #[test]
    fn brush_and_isolated_layer_blends_keep_their_exact_backdrop() {
        let mut doc = fixture();
        doc.layers[2].opacity = 0.68;
        doc.layers[2].blend = crate::color::Blend::Multiply;
        let mut raster = Layer::raster("paint", 128, 96);
        raster.mask = Some(Pixels::new(128, 96));
        raster.mask.as_mut().unwrap().data.fill(190);
        raster.mask.as_mut().unwrap().touch();
        doc.layers.insert(2, raster);
        let view = View {
            scale: 1.1,
            offset: Pt::new(2.3, 3.6),
        };
        let mut buf = Pixmap::new(128, 96).unwrap();
        let mut cache = Cache::default();
        for step in 0..6 {
            fill_solid(
                &mut buf,
                10. + step as f32 * 5.,
                20.,
                18.,
                15.,
                Rgba::rgb(250, 20, 60),
            );
            let draft = || Draft {
                preview: None,
                brush: Some((2, &buf, 0.63)),
            };
            let full = render_view(&doc, view, 180, 140, draft()).unwrap();
            let cached = cache
                .render(&doc, view, 180, 140, draft(), &[(2, 0)])
                .unwrap();
            assert_eq!(cached.data(), full.data());
        }
        assert_eq!(cache.entry.as_ref().unwrap().shape, None);
    }

    #[test]
    fn unrelated_edit_invalidates_the_backdrop() {
        let mut studio = crate::app::Studio::new();
        studio.doc = fixture();
        let id = studio.doc.layers[2].kind.shapes().unwrap()[2].id;
        studio
            .interaction_render
            .render(
                &studio.doc,
                View::default(),
                128,
                96,
                Draft::none(),
                &[(2, id)],
            )
            .unwrap();
        studio.mark_interaction();
        assert!(studio.interaction_render.entry.is_some());
        studio.doc.layers[0].visible = false;
        studio.mark();
        assert!(studio.interaction_render.entry.is_none());
        let actual = studio
            .interaction_render
            .render(
                &studio.doc,
                View::default(),
                128,
                96,
                Draft::none(),
                &[(2, id)],
            )
            .unwrap();
        assert_eq!(
            actual.data(),
            render_view(&studio.doc, View::default(), 128, 96, Draft::none())
                .unwrap()
                .data()
        );
    }

    #[test]
    fn dense_same_layer_drag_preserves_single_and_multiple_selection_compositing() {
        let mut doc = Document::new("Dense same-layer interaction", 180.0, 140.0, 72.0);
        doc.layers = vec![Layer::vector("Artwork")];
        for index in 0..144 {
            let mut shape = Shape::new(
                Geom::Rect {
                    origin: Pt::new(
                        3.25 + (index % 16) as f32 * 9.25,
                        4.75 + (index / 16) as f32 * 11.5,
                    ),
                    size: Pt::new(21.75, 19.25),
                    radius: 2.0,
                },
                Style {
                    fill: Fill::Solid(Rgba::new(
                        (index * 17 % 256) as u8,
                        (index * 43 % 256) as u8,
                        177,
                        193,
                    )),
                    stroke: Some(crate::document::Stroke {
                        color: Rgba::new(12, 88, 144, 209),
                        width: 1.25,
                        ..Default::default()
                    }),
                },
            );
            shape.opacity = 0.73;
            shape.rotation = (index % 7) as f32 * 0.03;
            if index % 19 == 0 {
                shape.mask = Some(
                    Pixels::from_rgba(
                        2,
                        2,
                        vec![
                            255, 255, 255, 255, 100, 100, 100, 255, 180, 180, 180, 255, 0, 0, 0,
                            255,
                        ],
                    )
                    .unwrap(),
                );
            }
            if index % 31 == 0 {
                shape.filters.items = vec![crate::filter::Fx::Blur { std: 0.7 }];
            }
            doc.layers[0].kind.shapes_mut().unwrap().push(shape);
        }
        for indices in [vec![117], vec![101, 51], vec![120, 13, 44]] {
            let mut scene = doc.clone();
            let changed: Vec<_> = indices
                .iter()
                .map(|&index| (0, scene.layers[0].kind.shapes().unwrap()[index].id))
                .collect();
            let original: Vec<_> = changed
                .iter()
                .map(|&(layer, id)| scene.find_shape(layer, id).unwrap().geom.clone())
                .collect();
            let mut cache = Cache::default();
            for step in 0..7 {
                for (&(layer, id), geometry) in changed.iter().zip(&original) {
                    let shape = scene.find_shape_mut(layer, id).unwrap();
                    shape.geom = geometry.clone();
                    shape
                        .geom
                        .translate(Pt::new(step as f32 * 1.35, -(step as f32) * 0.65));
                }
                let full = render_view(&scene, View::default(), 180, 140, Draft::none()).unwrap();
                let cached = cache
                    .render(&scene, View::default(), 180, 140, Draft::none(), &changed)
                    .unwrap();
                assert_eq!(
                    cached.data(),
                    full.data(),
                    "dense scene selection {indices:?}, sample {step}"
                );
                assert_eq!(
                    cache.entry.as_ref().unwrap().shape,
                    indices.iter().min().copied()
                );
            }
        }
    }

    #[test]
    fn moving_later_guides_and_wrap_obstacles_reflows_earlier_text_without_stale_pixels() {
        use crate::geom::TypeRun;
        use crate::text_geometry::{TextFrame, TextOnPath, WrapMode};
        for area in [false, true] {
            let mut doc = Document::new("Linked text dependencies", 200.0, 120.0, 72.0);
            doc.layers = vec![
                Layer::vector("Text below guide"),
                Layer::vector("Moving geometry"),
            ];
            let mut guide = Shape::new(
                if area {
                    Geom::Rect {
                        origin: Pt::new(25.0, 20.0),
                        size: Pt::new(55.0, 38.0),
                        radius: 0.0,
                    }
                } else {
                    Geom::Line {
                        a: Pt::new(12.0, 32.0),
                        b: Pt::new(186.0, 52.0),
                    }
                },
                Style {
                    fill: Fill::None,
                    stroke: None,
                },
            );
            let id = guide.id;
            if area {
                guide.text_wrap.mode = WrapMode::BoundingBox;
            }
            let mut run = TypeRun {
                origin: Pt::new(10.0, 24.0),
                content: "Moving artwork must keep linked text current. Every pointer sample must update this earlier layer.".into(),
                px: 12.0,
                ..Default::default()
            };
            if area {
                run.frame = Some(TextFrame {
                    size: Pt::new(178.0, 90.0),
                    ..Default::default()
                });
            } else {
                run.on_path = Some(TextOnPath {
                    path_id: id,
                    ..Default::default()
                });
            }
            let text = Shape::new(
                Geom::Text(run),
                Style {
                    fill: Fill::Solid(Rgba::BLACK),
                    stroke: None,
                },
            );
            doc.layers[0].kind.shapes_mut().unwrap().push(text);
            doc.layers[1].kind.shapes_mut().unwrap().push(guide);
            crate::text_geometry::reflow(&mut doc);
            let before = render_view(&doc, View::default(), 200, 120, Draft::none()).unwrap();
            assert!(
                before.pixels().iter().any(|pixel| pixel.red() < 250),
                "linked text must actually render"
            );
            let mut cache = Cache::default();
            for step in 0..4 {
                if step > 0 {
                    doc.find_shape_mut(1, id).unwrap().geom.translate(if area {
                        Pt::new(36.0, 0.0)
                    } else {
                        Pt::new(3.0, 8.0)
                    });
                    crate::text_geometry::reflow(&mut doc);
                }
                let full = render_view(&doc, View::default(), 200, 120, Draft::none()).unwrap();
                let cached = cache
                    .render(&doc, View::default(), 200, 120, Draft::none(), &[(1, id)])
                    .unwrap();
                assert_eq!(
                    cached.data(),
                    full.data(),
                    "linked text area={area}, sample={step}"
                );
                assert!(
                    cache.entry.is_none(),
                    "dependent earlier text must not become a cached prefix"
                );
                if step > 0 {
                    assert_ne!(
                        full.data(),
                        before.data(),
                        "moving the unpainted guide must change its dependent text"
                    );
                }
            }
        }
    }
}
