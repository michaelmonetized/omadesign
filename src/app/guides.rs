use super::*;
use crate::document::{RulerSettings, RulerUnit};

impl Studio {
    pub fn can_convert_to_guides(&self) -> bool {
        self.selection.iter().any(|&(li, id)| {
            self.doc.layers.get(li).is_some_and(|layer| {
                layer.visible
                    && !layer.locked
                    && if id == RASTER_ID {
                        layer.kind.is_placed_raster()
                    } else {
                        layer
                            .find(id)
                            .is_some_and(|shape| shape.visible && !shape.locked && !shape.guide)
                    }
            })
        })
    }

    pub fn can_release_guides(&self) -> bool {
        self.selection.iter().any(|&(li, id)| {
            self.doc.layers.get(li).is_some_and(|layer| {
                layer.visible
                    && !layer.locked
                    && layer
                        .find(id)
                        .is_some_and(|shape| shape.visible && !shape.locked && shape.guide)
            })
        })
    }

    pub fn convert_selection_to_guides(&mut self) {
        self.commit_type_edit();
        self.end_deform(true);
        self.reset_snap_gesture();
        let mut commands = vec![];
        let mut selected = vec![];
        let mut raster_count = 0;
        let mut bounds_layer = self
            .doc
            .layers
            .iter()
            .position(|layer| layer.visible && !layer.locked && layer.kind.shapes().is_some());
        for (layer, id) in self.selection.iter().copied() {
            let Some(source) = self
                .doc
                .layers
                .get(layer)
                .filter(|layer| layer.visible && !layer.locked)
            else {
                continue;
            };
            if id == RASTER_ID && source.kind.is_placed_raster() {
                let Some((origin, size, rotation)) = source.kind.raster_xform() else {
                    continue;
                };
                let mut guide = Shape::new(
                    Geom::Rect {
                        origin: origin.min(origin + size),
                        size: Pt::new(size.x.abs(), size.y.abs()),
                        radius: 0.0,
                    },
                    Style {
                        fill: Fill::None,
                        stroke: Some(Stroke::default()),
                    },
                );
                guide.name = format!("{} · bounds guide", source.name);
                guide.rotation = rotation;
                guide.guide = true;
                let target = *bounds_layer.get_or_insert_with(|| {
                    let target = self.doc.layers.len();
                    commands.push(Cmd::AddLayer {
                        index: target,
                        layer: Layer::vector("Guides"),
                    });
                    target
                });
                selected.push((target, guide.id));
                commands.push(Cmd::AddShape {
                    layer: target,
                    shape: guide,
                });
                raster_count += 1;
            } else if let Some(shape) = source
                .find(id)
                .filter(|shape| shape.visible && !shape.locked)
            {
                if !shape.guide {
                    commands.push(Cmd::SetShapeGuide {
                        layer,
                        id,
                        before: false,
                        after: true,
                    });
                }
                selected.push((layer, id));
            }
        }
        if commands.is_empty() {
            self.status = "Select artwork to turn into guides".into();
            return;
        }
        if !self.doc.ruler.guides_visible {
            commands.push(Cmd::SetRuler {
                before: self.doc.ruler,
                after: RulerSettings {
                    guides_visible: true,
                    ..self.doc.ruler
                },
            });
        }
        self.commit(Cmd::Batch(commands));
        self.selection = selected;
        if let Some(&(layer, _)) = self.selection.first() {
            self.active_layer = Some(layer);
        }
        self.node_sel.clear();
        self.artboard_sel.clear();
        self.status = if raster_count > 0 {
            format!("{} guides · image artwork kept", self.selection.len())
        } else {
            format!(
                "{} editable guides · original artwork preserved",
                self.selection.len()
            )
        };
    }

    pub fn release_selected_guides(&mut self) {
        self.commit_type_edit();
        self.end_deform(true);
        self.reset_snap_gesture();
        let commands: Vec<_> = self
            .selection
            .iter()
            .filter_map(|&(layer, id)| {
                let source = self
                    .doc
                    .layers
                    .get(layer)
                    .filter(|layer| layer.visible && !layer.locked)?;
                let shape = source
                    .find(id)
                    .filter(|shape| shape.visible && !shape.locked && shape.guide)?;
                Some(Cmd::SetShapeGuide {
                    layer,
                    id: shape.id,
                    before: true,
                    after: false,
                })
            })
            .collect();
        if commands.is_empty() {
            self.status = "Select object guides to release".into();
            return;
        }
        self.commit(Cmd::Batch(commands));
        self.status = "Guides released · artwork restored".into();
    }
    pub fn move_guide(&mut self, index: usize, pos: f32) {
        if !pos.is_finite() || self.doc.guides.get(index).is_none_or(|g| g.pos == pos) {
            return;
        }
        let mut after = self.doc.guides.clone();
        after[index].pos = pos;
        self.commit(Cmd::SetGuides {
            before: self.doc.guides.clone(),
            after,
        });
        self.status = "Guide moved".into();
    }

    pub fn remove_guide(&mut self, index: usize) {
        if let Some(guide) = self.doc.guides.get(index).copied() {
            self.commit(Cmd::RemoveGuide { index, guide });
            self.status = "Guide removed".into();
        }
    }

    pub fn clear_guides(&mut self) {
        if !self.doc.guides.is_empty() {
            self.commit(Cmd::SetGuides {
                before: self.doc.guides.clone(),
                after: vec![],
            });
            self.status = "Ruler guides cleared · undo to bring them back".into();
        }
    }

    fn set_ruler(&mut self, after: RulerSettings) {
        if self.doc.ruler != after {
            self.commit(Cmd::SetRuler {
                before: self.doc.ruler,
                after,
            });
        }
    }

    pub fn toggle_guides(&mut self) {
        if self.doc.ruler.guides_visible
            && self.selection.iter().any(|&(layer, id)| {
                self.doc
                    .find_shape(layer, id)
                    .is_some_and(|shape| shape.guide)
            })
        {
            self.commit_type_edit();
            self.end_deform(true);
            self.reset_snap_gesture();
            self.selection.retain(|&(layer, id)| {
                self.doc
                    .find_shape(layer, id)
                    .is_none_or(|shape| !shape.guide)
            });
            self.node_sel.clear();
        }
        self.set_ruler(RulerSettings {
            guides_visible: !self.doc.ruler.guides_visible,
            ..self.doc.ruler
        });
        self.status = if self.doc.ruler.guides_visible {
            "Guides shown"
        } else {
            "Guides hidden"
        }
        .into();
    }

    pub fn set_ruler_origin(&mut self, origin: Pt) {
        if origin.x.is_finite() && origin.y.is_finite() {
            self.set_ruler(RulerSettings {
                origin,
                ..self.doc.ruler
            });
            self.status = "Ruler origin set".into();
        }
    }

    pub fn set_ruler_unit(&mut self, unit: RulerUnit) {
        self.set_ruler(RulerSettings {
            unit,
            ..self.doc.ruler
        });
        self.status = format!("Ruler units · {}", unit.label());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Guide;

    #[test]
    fn guide_edits_restore_exact_order_and_duplicates_through_undo_redo() {
        let mut s = Studio::new();
        s.doc.guides = vec![
            Guide {
                vertical: true,
                pos: 10.0,
            },
            Guide {
                vertical: false,
                pos: 20.0,
            },
            Guide {
                vertical: true,
                pos: 30.0,
            },
        ];
        let original = s.doc.guides.clone();
        s.remove_guide(1);
        s.undo();
        assert_eq!(s.doc.guides, original);
        s.redo();
        assert_eq!(s.doc.guides, vec![original[0], original[2]]);
        s.undo();
        s.add_guide(true, 10.0);
        s.undo();
        assert_eq!(s.doc.guides, original);
        s.move_guide(1, 42.0);
        s.undo();
        assert_eq!(s.doc.guides, original);
        s.redo();
        assert_eq!(s.doc.guides[1].pos, 42.0);
        let moved = s.doc.guides.clone();
        s.clear_guides();
        assert!(s.doc.guides.is_empty());
        s.undo();
        assert_eq!(s.doc.guides, moved);
    }

    #[test]
    fn rulers_roundtrip_and_old_documents_default_to_visible_pixel_guides() {
        let old = r#"{"name":"legacy","width":200,"height":100,"dpi":300,"layers":[],"guides":[{"vertical":true,"pos":20}],"grid":{"visible":false,"snap":true,"size":8,"subdivisions":1}}"#;
        let mut doc: Document = serde_json::from_str(old).unwrap();
        assert_eq!(doc.ruler, RulerSettings::default());
        doc.ruler = RulerSettings {
            origin: Pt::new(15.0, -20.0),
            unit: RulerUnit::Inches,
            guides_visible: false,
        };
        let roundtrip: Document =
            serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
        assert_eq!(roundtrip.ruler, doc.ruler);
        assert_eq!(roundtrip.guides, doc.guides);
        for (unit, expected) in [
            (RulerUnit::Pixels, 300.0),
            (RulerUnit::Millimeters, 25.4),
            (RulerUnit::Centimeters, 2.54),
            (RulerUnit::Inches, 1.0),
            (RulerUnit::Points, 72.0),
        ] {
            assert!((300.0 / unit.pixels_per_unit(300.0) - expected).abs() < 0.001);
        }
    }
}

#[cfg(test)]
mod object_guide_tests {
    use super::*;

    #[test]
    fn curves_text_and_parametric_guides_preserve_artwork_through_save_release_and_undo() {
        let mut studio = Studio::new();
        studio.doc.transparent = true;
        studio.doc.layers = vec![Layer::vector("Artwork")];
        let mut start = Anchor::corner(Pt::new(10.0, 10.0));
        start.h_out = Pt::new(0.0, 50.0);
        let mut end = Anchor::corner(Pt::new(90.0, 10.0));
        end.h_in = Pt::new(0.0, 50.0);
        let geoms = [
            Geom::Path {
                anchors: vec![start, end],
                closed: false,
            },
            Geom::Ellipse {
                center: Pt::new(150.0, 70.0),
                radii: Pt::new(30.0, 20.0),
            },
            Geom::Text(TypeRun {
                content: "Keep me editable".into(),
                tracking: 2.0,
                ..Default::default()
            }),
        ];
        let original: Vec<_> = geoms
            .into_iter()
            .map(|geom| Shape::new(geom, Style::default()))
            .collect();
        *studio.doc.layers[0].kind.shapes_mut().unwrap() = original.clone();
        studio.selection = original.iter().map(|shape| (0, shape.id)).collect();
        assert!(studio.can_convert_to_guides());
        studio.convert_selection_to_guides();
        assert_eq!(studio.history.len(), 1);
        assert!(studio.can_release_guides());
        for (guide, source) in studio.doc.layers[0]
            .kind
            .shapes()
            .unwrap()
            .iter()
            .zip(&original)
        {
            assert!(guide.guide);
            assert_eq!(guide.geom, source.geom);
            assert_eq!(guide.style, source.style);
            assert_eq!(guide.id, source.id);
        }
        let saved = crate::project::encode(&studio.doc).unwrap();
        let reopened = crate::project::decode(&saved).unwrap();
        assert!(
            reopened.layers[0]
                .kind
                .shapes()
                .unwrap()
                .iter()
                .all(|shape| shape.guide)
        );
        let Geom::Text(text) = &reopened.layers[0].kind.shapes().unwrap()[2].geom else {
            panic!("guide conversion must retain live text");
        };
        assert_eq!(text.content, "Keep me editable");
        assert_eq!(text.tracking, 2.0);
        let png = crate::compositor::export_png(&studio.doc, 1).unwrap();
        assert!(
            image::load_from_memory(&png)
                .unwrap()
                .to_rgba8()
                .pixels()
                .all(|pixel| pixel[3] == 0),
            "guides must never print into raster exports"
        );
        let svg = crate::svg::export(&studio.doc).unwrap();
        for shape in &original {
            assert!(!svg.contains(&format!("id=\"oma-{}\"", shape.id)));
        }
        studio.release_selected_guides();
        assert_eq!(studio.doc.layers[0].kind.shapes().unwrap(), original);
        studio.undo();
        assert!(
            studio.doc.layers[0]
                .kind
                .shapes()
                .unwrap()
                .iter()
                .all(|shape| shape.guide)
        );
        studio.undo();
        assert_eq!(studio.doc.layers[0].kind.shapes().unwrap(), original);
        let mut legacy = serde_json::to_value(&original[0]).unwrap();
        legacy.as_object_mut().unwrap().remove("guide");
        assert!(!serde_json::from_value::<Shape>(legacy).unwrap().guide);
    }

    #[test]
    fn image_bounds_guides_keep_pixels_and_hidden_guides_do_not_capture_artwork() {
        let mut studio = Studio::new();
        studio.doc.layers = vec![Layer::raster("Image", 8, 8)];
        studio.doc.layers[0].kind.pixels_mut().unwrap().data = [24, 96, 180, 255].repeat(64);
        studio.doc.layers[0]
            .kind
            .set_raster_xform(Pt::new(20.0, 20.0), Pt::new(80.0, 80.0), 0.0);
        let pixels = studio.doc.layers[0].kind.pixels().unwrap().data.clone();
        studio.selection = vec![(0, RASTER_ID)];
        studio.convert_selection_to_guides();
        assert_eq!(studio.doc.layers.len(), 2);
        assert_eq!(studio.doc.layers[0].kind.pixels().unwrap().data, pixels);
        let guide = studio.selection[0];
        assert!(studio.doc.find_shape(guide.0, guide.1).unwrap().guide);
        assert_eq!(studio.doc.hit_test(Pt::new(20.0, 50.0), 3.0), Some(guide));
        assert_eq!(
            studio.doc.hit_test(Pt::new(50.0, 50.0), 3.0),
            Some((0, RASTER_ID))
        );
        studio.toggle_guides();
        assert!(
            studio.selection.is_empty(),
            "hidden guides must not retain invisible editable handles"
        );
        assert_eq!(
            studio.doc.hit_test(Pt::new(20.0, 50.0), 3.0),
            Some((0, RASTER_ID))
        );
        studio.undo();
        studio.undo();
        assert_eq!(studio.doc.layers.len(), 1);
        assert_eq!(studio.doc.layers[0].kind.pixels().unwrap().data, pixels);
    }
}

#[cfg(test)]
mod compound_guide_tests {
    use super::*;

    #[test]
    fn releasing_rotated_compound_guides_preserves_contours_style_export_and_one_undo() {
        let mut studio = Studio::new();
        studio.doc = Document::new("Compound guide", 128.0, 128.0, 72.0);
        studio.doc.transparent = true;
        studio.doc.layers = vec![Layer::vector("Guides")];
        let mut compound = Shape::new(
            Geom::Poly {
                contours: vec![
                    vec![
                        Pt::new(20.0, 20.0),
                        Pt::new(45.0, 20.0),
                        Pt::new(45.0, 40.0),
                        Pt::new(20.0, 40.0),
                    ],
                    vec![
                        Pt::new(65.0, 50.0),
                        Pt::new(90.0, 50.0),
                        Pt::new(75.0, 85.0),
                    ],
                ],
                winding: false,
            },
            Style {
                fill: Fill::Linear {
                    from: [0.1, 0.2],
                    to: [0.9, 0.85],
                    c0: Rgba::rgb(150, 50, 190),
                    c1: Rgba::rgb(230, 180, 40),
                },
                stroke: Some(Stroke {
                    color: Rgba::rgb(40, 160, 120),
                    width: 3.0,
                    ..Default::default()
                }),
            },
        );
        compound.rotation = 0.61;
        compound.opacity = 0.63;
        compound.guide = true;
        let expected_contours = compound.world_contours(96);
        let invisible = || {
            Shape::new(
                Geom::Line {
                    a: Pt::ZERO,
                    b: Pt::new(1.0, 1.0),
                },
                Style {
                    fill: Fill::None,
                    stroke: None,
                },
            )
        };
        let originals = vec![invisible(), compound.clone(), invisible()];
        *studio.doc.layers[0].kind.shapes_mut().unwrap() = originals.clone();
        studio.selection = vec![(0, compound.id)];
        studio
            .doc
            .motion
            .set_key(compound.id, Prop::Opacity, 1.0, 0.8, Ease::Linear);
        let original_motion = studio.doc.motion.clone();
        let mut reference = studio.doc.clone();
        reference.layers[0].find_mut(compound.id).unwrap().guide = false;
        let reference =
            image::load_from_memory(&crate::compositor::export_png(&reference, 1).unwrap())
                .unwrap()
                .to_rgba8();
        studio.release_compound();
        assert_eq!(studio.history.len(), 1);
        let parts = studio.doc.layers[0].kind.shapes().unwrap();
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0].id, originals[0].id);
        assert_eq!(parts[3].id, originals[2].id);
        for (part, expected) in parts[1..3].iter().zip(&expected_contours) {
            assert!(part.guide);
            assert_eq!(part.style.stroke, compound.style.stroke);
            assert!(matches!(part.style.fill, Fill::Linear { .. }));
            assert_eq!(part.opacity, compound.opacity);
            assert_eq!(part.rotation, compound.rotation);
            let contours = part.world_contours(96);
            assert_eq!(contours.len(), 1);
            for (actual, expected) in contours[0].iter().zip(expected) {
                assert!(
                    (*actual - *expected).length() < 0.0001,
                    "released contour moved"
                );
            }
        }
        let png = crate::compositor::export_png(&studio.doc, 1).unwrap();
        assert!(
            image::load_from_memory(&png)
                .unwrap()
                .to_rgba8()
                .pixels()
                .all(|pixel| pixel[3] == 0)
        );
        let svg = crate::svg::export(&studio.doc).unwrap();
        for part in &parts[1..3] {
            assert!(!svg.contains(&format!("id=\"oma-{}\"", part.id)));
        }
        studio.undo();
        assert_eq!(studio.doc.layers[0].kind.shapes().unwrap(), originals);
        assert_eq!(studio.doc.motion, original_motion);
        studio.redo();
        studio.selection = studio.doc.layers[0]
            .kind
            .shapes()
            .unwrap()
            .iter()
            .filter(|shape| shape.guide)
            .map(|shape| (0, shape.id))
            .collect();
        studio.release_selected_guides();
        let png = crate::compositor::export_png(&studio.doc, 1).unwrap();
        let released = image::load_from_memory(&png).unwrap().to_rgba8();
        assert!(released.pixels().any(|pixel| pixel[3] > 0));
        assert!(
            released
                .as_raw()
                .iter()
                .zip(reference.as_raw())
                .all(|(a, b)| a.abs_diff(*b) <= 2),
            "splitting restarted or moved the gradient"
        );
    }

    #[test]
    fn mixed_guide_geometry_operations_cannot_print_guides_or_hide_artwork() {
        for guide_first in [false, true] {
            let mut studio = Studio::new();
            studio.doc.layers = vec![Layer::vector("Mixed")];
            let mut artwork = Shape::new(
                Geom::Rect {
                    origin: Pt::new(10.0, 10.0),
                    size: Pt::new(30.0, 30.0),
                    radius: 0.0,
                },
                Style::default(),
            );
            artwork.guide = guide_first;
            let mut guide = Shape::new(
                Geom::Rect {
                    origin: Pt::new(20.0, 20.0),
                    size: Pt::new(30.0, 30.0),
                    radius: 0.0,
                },
                Style::default(),
            );
            guide.guide = !guide_first;
            let originals = vec![artwork, guide];
            *studio.doc.layers[0].kind.shapes_mut().unwrap() = originals.clone();
            studio.selection = originals.iter().map(|shape| (0, shape.id)).collect();
            studio.combine_selected();
            assert_eq!(studio.doc.layers[0].kind.shapes().unwrap(), originals);
            studio.pathfinder(BoolOp::Union);
            assert_eq!(studio.doc.layers[0].kind.shapes().unwrap(), originals);
            studio.divide_selection();
            assert_eq!(studio.doc.layers[0].kind.shapes().unwrap(), originals);
            assert_eq!(studio.history.len(), 0);
            for shape in studio.doc.layers[0].kind.shapes_mut().unwrap() {
                shape.guide = true;
            }
            studio.pathfinder(BoolOp::Union);
            assert_eq!(studio.doc.layers[0].kind.shapes().unwrap().len(), 1);
            assert!(studio.doc.layers[0].kind.shapes().unwrap()[0].guide);
        }
    }
    #[test]
    fn combining_rotated_guides_preserves_world_geometry_gradient_and_atomic_undo() {
        let mut studio = Studio::new();
        studio.doc = Document::new("Combine guides", 128.0, 128.0, 72.0);
        studio.doc.transparent = true;
        studio.doc.layers = vec![Layer::vector("Guides")];
        let style = Style {
            fill: Fill::Linear {
                from: [0.0, 0.0],
                to: [1.0, 1.0],
                c0: Rgba::rgb(210, 30, 50),
                c1: Rgba::rgb(30, 80, 210),
            },
            stroke: None,
        };
        let mut a = Shape::new(
            Geom::Rect {
                origin: Pt::new(15.0, 15.0),
                size: Pt::new(30.0, 30.0),
                radius: 0.0,
            },
            style.clone(),
        );
        a.rotation = 0.55;
        a.guide = true;
        let mut b = Shape::new(
            Geom::Ellipse {
                center: Pt::new(98.0, 86.0),
                radii: Pt::new(19.0, 12.0),
            },
            style,
        );
        b.rotation = -0.2;
        b.guide = true;
        let originals = vec![a, b];
        let expected: Vec<_> = originals
            .iter()
            .flat_map(|shape| shape.world_contours(96))
            .collect();
        *studio.doc.layers[0].kind.shapes_mut().unwrap() = originals.clone();
        studio.selection = originals.iter().map(|shape| (0, shape.id)).collect();
        let mut reference = studio.doc.clone();
        for shape in reference.layers[0].kind.shapes_mut().unwrap() {
            shape.guide = false;
        }
        let reference =
            image::load_from_memory(&crate::compositor::export_png(&reference, 1).unwrap())
                .unwrap()
                .to_rgba8();
        studio.combine_selected();
        assert_eq!(studio.history.len(), 1);
        let combined = studio.doc.layers[0].kind.shapes().unwrap();
        assert_eq!(combined.len(), 1);
        assert!(combined[0].guide);
        assert_eq!(combined[0].rotation, 0.0);
        assert_eq!(combined[0].world_contours(96), expected);
        let mut released = studio.doc.clone();
        released.layers[0].kind.shapes_mut().unwrap()[0].guide = false;
        let released =
            image::load_from_memory(&crate::compositor::export_png(&released, 1).unwrap())
                .unwrap()
                .to_rgba8();
        for y in 0..60 {
            for x in 0..60 {
                assert!(
                    released
                        .get_pixel(x, y)
                        .0
                        .iter()
                        .zip(reference.get_pixel(x, y).0)
                        .all(|(a, b)| a.abs_diff(b) <= 2),
                    "Combine moved the source gradient"
                );
            }
        }
        studio.undo();
        assert_eq!(studio.doc.layers[0].kind.shapes().unwrap(), originals);
    }
}
