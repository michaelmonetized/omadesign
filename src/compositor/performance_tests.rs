use super::*;
use crate::{
    color::Blend,
    document::{Pixels, Stroke, StrokeAlignment, Style},
    filter::{FilterStack, Fx},
    geom::{Anchor, Bounds},
};

fn shape() -> Shape {
    Shape::new(
        Geom::Rect {
            origin: Pt::new(21.25, 18.75),
            size: Pt::new(29.5, 23.25),
            radius: 3.0,
        },
        Style {
            fill: Fill::Linear {
                from: [0.1, 0.2],
                to: [0.9, 0.7],
                c0: Rgba::new(219, 18, 124, 203),
                c1: Rgba::new(21, 176, 228, 177),
            },
            stroke: Some(Stroke {
                color: Rgba::new(42, 219, 94, 190),
                width: 3.5,
                ..Default::default()
            }),
        },
    )
}

fn backdrop() -> Pixmap {
    let mut pm = Pixmap::new(121, 97).unwrap();
    pm.fill(Rgba::new(102, 77, 34, 163).to_skia());
    fill_solid(
        &mut pm,
        25.25,
        7.75,
        52.5,
        63.5,
        Rgba::new(190, 15, 180, 187),
    );
    pm
}

fn assert_pixels_equal(actual: &Pixmap, expected: &Pixmap, context: &str) {
    let differences: Vec<_> = actual
        .data()
        .iter()
        .zip(expected.data())
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(index, (a, b))| {
            (
                index / 4 % actual.width() as usize,
                index / 4 / actual.width() as usize,
                index % 4,
                *a,
                *b,
            )
        })
        .collect();
    assert!(
        differences.is_empty(),
        "{context}: {} channels differ; first pixels (x, y, channel, actual, expected): {:?}",
        differences.len(),
        &differences[..differences.len().min(8)]
    );
}

// The original effect rendering path: render fresh content, then preserve the
// distinction between independent appearance and filters applied to the source.
fn reference_filtered(pm: &mut Pixmap, shape: &Shape, t: Transform, pose: Pose) {
    let mut painted = shape.clone();
    posed_paint(&mut painted, pose);
    let shape = &painted;
    let mask = object_mask(pm, shape, t, pose, None);
    let alpha = pose.opacity.unwrap_or(shape.opacity).clamp(0.0, 1.0);
    let bounds = pose
        .map_bounds(shape.world_bbox())
        .inflate(crate::filter::svg_pad(&shape.filters).ceil().max(8.0));
    let mut content = Pixmap::new(
        bounds.width().ceil().max(1.0) as u32,
        bounds.height().ceil().max(1.0) as u32,
    )
    .unwrap();
    let mut opaque = pose;
    opaque.opacity = Some(1.0);
    draw_shape_inner(
        &mut content,
        shape,
        Transform::from_translate(-bounds.min.x, -bounds.min.y),
        1.0,
        tiny_skia::BlendMode::SourceOver,
        opaque,
        None,
    );
    let transform = t.pre_concat(Transform::from_translate(bounds.min.x, bounds.min.y));
    if shape.filters.independent() || shape.fill_opacity < 1.0 || shape.blend_interior {
        crate::filter::composite(
            pm,
            content,
            &shape.filters,
            transform,
            shape.blend.to_skia(),
            alpha,
            shape.fill_opacity,
            shape.blend_interior,
            mask.as_deref(),
        );
    } else {
        crate::filter::apply(&mut content, &shape.filters);
        pm.draw_pixmap(
            0,
            0,
            content.as_ref(),
            &PixmapPaint {
                opacity: alpha,
                blend_mode: shape.blend.to_skia(),
                quality: tiny_skia::FilterQuality::Bilinear,
            },
            transform,
            mask.as_deref(),
        );
    }
}

fn compare_filtered(shape: &Shape, transform: Transform, pose: Pose) {
    let mut expected = backdrop();
    reference_filtered(&mut expected, shape, transform, pose);
    for _ in 0..2 {
        let mut actual = backdrop();
        draw_shape(
            &mut actual,
            shape,
            transform,
            1.0,
            tiny_skia::BlendMode::SourceOver,
            pose,
        );
        assert_pixels_equal(
            &actual,
            &expected,
            &format!("effects {:?}, {:?}", shape.filters, pose),
        );
    }
}

#[test]
fn cached_effect_pixels_match_fresh_masks_blends_motion_and_camera_transforms() {
    effects_cache::clear();
    let stacks = [
        vec![Fx::Blur { std: 1.6 }],
        vec![Fx::Shadow {
            blend: Blend::Normal,
            opacity: 1.0,
            knockout: false,
            spread: 0.0,
            dx: 3.25,
            dy: -2.5,
            blur: 1.6,
            color: Rgba::new(7, 26, 49, 171),
        }],
        vec![
            Fx::OuterGlow {
                blur: 1.6,
                spread: 2.0,
                color: Rgba::new(255, 220, 90, 151),
                blend: Blend::Normal,
                opacity: 1.0,
            },
            Fx::ColorOverlay {
                color: Rgba::new(74, 110, 231, 96),
                blend: Blend::Normal,
                opacity: 1.0,
            },
        ],
        vec![Fx::ColorOverlay {
            color: Rgba::new(74, 110, 231, 96),
            blend: Blend::Multiply,
            opacity: 0.7,
        }],
        vec![Fx::Turbulence {
            fractal: false,
            base: 0.05,
            octaves: 2,
            seed: 31,
        }],
    ];
    for items in stacks {
        for legacy_composite in [false, true] {
            let mut shape = shape();
            shape.filters = FilterStack {
                enabled: true,
                legacy_composite,
                items: items.clone(),
            };
            for transform in [
                Transform::identity(),
                Transform::from_row(1.33, 0.21, -0.14, 0.87, 5.25, -8.5),
                Transform::from_row(0.54, -0.13, 0.08, 0.71, -9.75, 12.25),
            ] {
                compare_filtered(&shape, transform, Pose::identity());
            }
            for variant in 0..5 {
                let mut changed = shape.clone();
                match variant {
                    0 => changed.opacity = 0.63,
                    1 => changed.blend = Blend::Screen,
                    2 => changed.fill_opacity = 0.43,
                    3 => changed.blend_interior = true,
                    _ => {
                        changed.mask = Some(
                            Pixels::from_rgba(
                                2,
                                2,
                                vec![
                                    255, 255, 255, 255, 80, 80, 80, 255, 173, 173, 173, 255, 0, 0,
                                    0, 255,
                                ],
                            )
                            .unwrap(),
                        )
                    }
                }
                compare_filtered(
                    &changed,
                    Transform::from_translate(6.25, -4.75),
                    Pose::identity(),
                );
            }
            compare_filtered(
                &shape,
                Transform::identity(),
                Pose {
                    dx: 13.25,
                    dy: -8.5,
                    rotation: 0.27,
                    width_scale: 1.12,
                    stroke_width: Some(1.5),
                    fill_color: Some(Rgba::new(231, 190, 42, 215)),
                    ..Pose::identity()
                },
            );
        }
    }
}

#[test]
fn effect_cache_invalidates_paint_geometry_filter_pose_and_image_edits() {
    effects_cache::clear();
    let mut shape = shape();
    let bounds = Bounds::from_min_size(Pt::ZERO, Pt::new(16.0, 16.0));
    let render = |shape: &Shape, pose| {
        effects_cache::render(shape, pose, bounds, || Some(Pixmap::new(16, 16).unwrap())).unwrap()
    };
    let first = render(&shape, Pose::identity());
    assert!(std::sync::Arc::ptr_eq(
        &first,
        &render(&shape, Pose::identity())
    ));
    shape.opacity = 0.25;
    assert!(std::sync::Arc::ptr_eq(
        &first,
        &render(
            &shape,
            Pose {
                opacity: Some(0.7),
                ..Pose::identity()
            }
        )
    ));
    let mut last = first;
    for change in 0..6 {
        match change {
            0 => shape.style.fill = Fill::Solid(Rgba::BLACK),
            1 => shape.style.stroke.as_mut().unwrap().width = 9.0,
            2 => shape.rotation = 0.2,
            3 => shape.filters.items.push(Fx::Blur { std: 2.0 }),
            4 => shape.geom.translate(Pt::new(4.0, -3.0)),
            _ => shape.corners = [1.0, 2.0, 3.0, 4.0],
        }
        let next = render(&shape, Pose::identity());
        assert!(!std::sync::Arc::ptr_eq(&last, &next));
        last = next;
    }
    let next = render(
        &shape,
        Pose {
            fill_reveal: Some(0.5),
            ..Pose::identity()
        },
    );
    assert!(!std::sync::Arc::ptr_eq(&last, &next));
    let mut source = Pixmap::new(2, 2).unwrap();
    source.fill(Rgba::new(17, 98, 166, 193).to_skia());
    shape.layout.image =
        Some(crate::layout_images::from_bytes(&source.encode_png().unwrap()).unwrap());
    let image = render(&shape, Pose::identity());
    assert!(!std::sync::Arc::ptr_eq(&last, &image));
    shape.layout.image.as_mut().unwrap().focal = Pt::new(0.1, 0.9);
    assert!(!std::sync::Arc::ptr_eq(
        &image,
        &render(&shape, Pose::identity())
    ));
}

fn reference_layer(
    pm: &mut Pixmap,
    layer: &Layer,
    transform: Transform,
    doc: &Document,
    overrides: &HashMap<u64, Pose>,
) {
    let mut content = Pixmap::new(pm.width(), pm.height()).unwrap();
    draw_content(
        &mut content,
        layer,
        transform,
        None,
        None,
        1.0,
        tiny_skia::BlendMode::SourceOver,
        None,
        doc,
        Some(overrides),
    );
    if let Some(pixels) = &layer.mask {
        let mut placed = Pixmap::new(pm.width(), pm.height()).unwrap();
        pixels
            .with_pm(|mask| {
                placed.draw_pixmap(
                    0,
                    0,
                    mask.as_ref(),
                    &PixmapPaint {
                        quality: tiny_skia::FilterQuality::Bilinear,
                        ..Default::default()
                    },
                    transform.pre_concat(layer_pixel_transform(layer)),
                    None,
                )
            })
            .unwrap();
        content.apply_mask(&tiny_skia::Mask::from_pixmap(
            placed.as_ref(),
            tiny_skia::MaskType::Luminance,
        ));
    }
    if layer.filters.independent() || layer.fill_opacity < 1.0 || layer.blend_interior {
        crate::filter::composite(
            pm,
            content,
            &layer.filters,
            Transform::identity(),
            layer.blend.to_skia(),
            layer.opacity,
            layer.fill_opacity,
            layer.blend_interior,
            None,
        );
    } else {
        if layer.filters.active() {
            crate::filter::apply(&mut content, &layer.filters);
        }
        pm.draw_pixmap(
            0,
            0,
            content.as_ref(),
            &PixmapPaint {
                opacity: layer.opacity,
                blend_mode: layer.blend.to_skia(),
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
}

#[test]
fn bounded_layer_surfaces_match_full_viewport_compositing() {
    let mut doc = Document::new("bounded isolation", 121.0, 97.0, 72.0);
    let mut vector = Layer::vector("vector");
    let mut second = shape();
    second.geom.translate(Pt::new(8.0, 12.0));
    second.style.stroke.as_mut().unwrap().width = 1.0;
    second.blend = Blend::Multiply;
    second.rotation = -0.23;
    *vector.kind.shapes_mut().unwrap() = vec![shape(), second];
    for shape in vector.kind.shapes_mut().unwrap() {
        shape.geom.translate(Pt::new(20.0, 22.0));
    }
    let region = layer_region(
        &vector,
        Transform::identity(),
        None,
        None,
        None,
        &doc,
        None,
        121,
        97,
    )
    .unwrap();
    assert!(region.x() > 0 && region.y() > 0 && region.width() < 121 && region.height() < 97);
    let mut raster = Layer::placed_raster(
        "raster",
        Pixels::from_rgba(8, 8, [29, 137, 219, 183].repeat(64)).unwrap(),
        Pt::new(29.25, 23.75),
        Pt::new(36.5, 31.25),
    );
    if let LayerKind::Raster {
        rotation, shear, ..
    } = &mut raster.kind
    {
        *rotation = 0.27;
        *shear = 4.0;
    }
    for original in [vector, raster] {
        for variant in 0..5 {
            let mut layer = original.clone();
            layer.opacity = 0.67;
            match variant {
                0 => {}
                1 => layer.blend = Blend::Overlay,
                2 => layer.fill_opacity = 0.53,
                3 => {
                    layer.mask = Some(
                        Pixels::from_rgba(
                            2,
                            2,
                            vec![
                                255, 255, 255, 255, 80, 80, 80, 255, 173, 173, 173, 255, 0, 0, 0,
                                255,
                            ],
                        )
                        .unwrap(),
                    );
                    layer.mask_origin = Pt::new(12.0, 10.0);
                    layer.mask_size = Pt::new(70.0, 56.0);
                }
                _ => layer.filters.items = vec![Fx::Blur { std: 1.0 }],
            }
            let overrides = layer
                .kind
                .shapes()
                .map(|shapes| {
                    HashMap::from([(
                        shapes[1].id,
                        Pose {
                            dx: -3.75,
                            dy: 5.25,
                            stroke_width: Some(if variant == 0 { 13.0 } else { 0.0 }),
                            ..Pose::identity()
                        },
                    )])
                })
                .unwrap_or_default();
            doc.layers = vec![layer];
            let layer = &doc.layers[0];
            for transform in [
                Transform::identity(),
                Transform::from_row(0.63, 0.07, -0.09, 0.74, 25.25, 10.5),
                Transform::from_translate(-30.75, 14.25),
            ] {
                let mut expected = backdrop();
                reference_layer(&mut expected, layer, transform, &doc, &overrides);
                let mut actual = backdrop();
                draw_layer(
                    &mut actual,
                    layer,
                    transform,
                    None,
                    None,
                    None,
                    &doc,
                    Some(&overrides),
                );
                assert_pixels_equal(
                    &actual,
                    &expected,
                    &format!("bounded layer variant={variant}, {:?}", transform),
                );
            }
        }
    }
}

#[test]
fn offscreen_rejection_preserves_wide_strokes_cubic_hulls_and_motion() {
    let mut ordinary = shape();
    ordinary.geom.translate(Pt::new(-88.0, 0.0));
    ordinary.style.stroke.as_mut().unwrap().width = 40.0;
    ordinary.style.stroke.as_mut().unwrap().alignment = StrokeAlignment::Outside;
    let mut curve = shape();
    curve.geom = Geom::Path {
        anchors: vec![
            Anchor {
                pt: Pt::new(-30.0, -20.0),
                h_out: Pt::new(140.0, 55.0),
                ..Anchor::corner(Pt::new(-30.0, -20.0))
            },
            Anchor {
                pt: Pt::new(-30.0, 90.0),
                h_in: Pt::new(130.0, -55.0),
                ..Anchor::corner(Pt::new(-30.0, 90.0))
            },
        ],
        closed: false,
    };
    for shape in [ordinary, curve] {
        for pose in [
            Pose::identity(),
            Pose {
                dx: 28.5,
                rotation: 0.2,
                width_scale: 1.3,
                ..Pose::identity()
            },
        ] {
            for transform in [
                Transform::identity(),
                Transform::from_row(0.75, 0.2, -0.3, 1.1, 12.25, 5.75),
            ] {
                let mut expected = backdrop();
                draw_shape_inner(
                    &mut expected,
                    &shape,
                    transform,
                    1.0,
                    tiny_skia::BlendMode::SourceOver,
                    pose,
                    None,
                );
                let mut actual = backdrop();
                draw_shape(
                    &mut actual,
                    &shape,
                    transform,
                    1.0,
                    tiny_skia::BlendMode::SourceOver,
                    pose,
                );
                assert_pixels_equal(&actual, &expected, "culling");
            }
        }
    }
}

#[test]
#[ignore = "manual release timing; do not measure under concurrent builds"]
fn benchmark_cached_effects() {
    let mut doc = Document::new("Effect cache benchmark", 1024.0, 768.0, 72.0);
    doc.layers = vec![Layer::vector("Effects")];
    let shapes = doc.layers[0].kind.shapes_mut().unwrap();
    for i in 0..96 {
        let mut shape = shape();
        shape
            .geom
            .translate(Pt::new((i % 12) as f32 * 78.0, (i / 12) as f32 * 80.0));
        shape.filters.items = vec![Fx::Blur { std: 4.0 }];
        shapes.push(shape);
    }
    for cold in [true, false] {
        effects_cache::clear();
        let _ = render_view(&doc, View::default(), 1024, 768, Draft::none()).unwrap();
        let mut times = Vec::new();
        for i in 0..21 {
            if cold {
                effects_cache::clear();
            }
            let start = std::time::Instant::now();
            std::hint::black_box(
                render_view(
                    &doc,
                    View {
                        scale: 1.0,
                        offset: Pt::new((i % 7) as f32 * 0.25, 0.0),
                    },
                    1024,
                    768,
                    Draft::none(),
                )
                .unwrap(),
            );
            times.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        times.sort_by(f64::total_cmp);
        eprintln!(
            "effect-cache cold={cold} median_ms={:.3} p95_ms={:.3}",
            times[10], times[19]
        );
    }
}
