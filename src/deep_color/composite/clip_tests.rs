use super::*;
use crate::{
    color::Rgba,
    document::{Fill, Shape, Stroke, Style},
    geom::{Geom, Pt},
    resources,
};
use std::{
    cell::Cell,
    sync::atomic::{AtomicU64, Ordering},
};

fn rect(x: f32, y: f32, w: f32, h: f32, fill: Fill) -> Shape {
    Shape::new(
        Geom::Rect {
            origin: Pt::new(x, y),
            size: Pt::new(w, h),
            radius: 0.,
        },
        Style { fill, stroke: None },
    )
}
fn frame(x: f32, y: f32, w: f32, h: f32) -> Shape {
    let mut shape = rect(x, y, w, h, Fill::None);
    shape.layout = crate::layout::FrameLayout::frame();
    shape
}
fn document(w: u32, h: u32, shapes: Vec<Shape>) -> Document {
    let mut doc = Document::new_with_options(
        "Native clips",
        w as f32,
        h as f32,
        72.,
        true,
        1,
        false,
        false,
    );
    doc.color.depth = Depth::F32;
    let mut layer = Layer::vector("Joint covered artwork");
    *layer.kind.shapes_mut().unwrap() = shapes;
    doc.layers = vec![layer];
    doc
}
fn near(actual: f32, expected: f64, tolerance: f64) {
    assert!(
        (actual as f64 - expected).abs() <= tolerance,
        "{actual:?} != {expected:?}"
    );
}
fn snapshot(doc: &Document) -> String {
    serde_json::to_string(doc).unwrap()
}
fn same(doc: &Document, before: &str) {
    assert_eq!(snapshot(doc), before);
}
fn mask(w: u32, h: u32, values: Vec<f32>) -> Pixels {
    Pixels::from_samples(w, h, Depth::F32, values).unwrap()
}

#[test]
fn float_clips_joint_half_pixels_disjoint_overlap_and_sub_byte_area() {
    for (fill_x, fill_w, clip_x, clip_w, expected) in [
        (0.5, 0.5, 0., 0.5, 0.),
        (0., 0.5, 0., 0.5, 0.5),
        (0.25, 0.5, 0., 0.5, 0.25),
        (0., 1. / 1024., 1. / 2048., 1. / 4096., 1. / 4096.),
    ] {
        let clip = frame(clip_x, 0., clip_w, 1.);
        let mut fill = rect(fill_x, 0., fill_w, 1., Fill::Solid(Rgba::WHITE));
        fill.layout.parent = Some(clip.id);
        let doc = document(1, 1, vec![fill, clip]);
        let before = snapshot(&doc);
        let output = composite(&doc, 1).unwrap();
        near(output.sample(0)[3], expected, 1e-10);
        if expected > 0. {
            assert_eq!(&output.sample(0)[..3], &[1., 1., 1.]);
        }
        let mut unclipped = doc.clone();
        unclipped.layers[0].kind.shapes_mut().unwrap()[1]
            .layout
            .clip = false;
        near(
            composite(&unclipped, 1).unwrap().sample(0)[3],
            fill_w as f64,
            1e-10,
        );
        for scale in [2, 4] {
            let p = composite(&doc, scale).unwrap();
            let sum: f64 = p
                .samples()
                .unwrap()
                .chunks_exact(4)
                .map(|p| p[3] as f64)
                .sum();
            assert!((sum / (scale * scale) as f64 - expected).abs() < 1e-9);
        }
        same(&doc, &before);
    }
}

#[test]
fn float_clips_joint_winding_holes_do_not_multiply_independent_averages() {
    let fill =
        tiny_skia::PathBuilder::from_rect(tiny_skia::Rect::from_xywh(0., 0., 1., 1.).unwrap());
    let mut builder = tiny_skia::PathBuilder::new();
    builder.push_rect(tiny_skia::Rect::from_xywh(0., 0., 1., 1.).unwrap());
    builder.push_rect(tiny_skia::Rect::from_xywh(0.25, 0.25, 0.5, 0.5).unwrap());
    let path = std::sync::Arc::new(builder.finish().unwrap());
    for (nonzero, expected) in [(true, 1.), (false, 0.75)] {
        let clips = [Some(coverage::Clip {
            path: path.clone(),
            transform: tiny_skia::Transform::identity(),
            nonzero,
        })];
        let mut area = 0.;
        coverage::rows_clipped(
            &fill,
            tiny_skia::Transform::identity(),
            true,
            &clips,
            tiny_skia::Transform::identity(),
            None,
            1,
            1,
            &|| Ok(()),
            |_, row| {
                area = row[0];
                Ok(())
            },
        )
        .unwrap();
        assert!((area - expected).abs() < 1e-12);
    }
}

fn weighted_document() -> Document {
    let clip = frame(0.25, 0.25, 0.25, 0.25);
    let mut fill = rect(0., 0., 1., 1., Fill::Solid(Rgba::rgb(255, 0, 0)));
    fill.layout.parent = Some(clip.id);
    fill.mask = Some(mask(
        2,
        2,
        vec![
            0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 0., 1., 1., 1., 1., 1.,
        ],
    ));
    document(1, 1, vec![fill, clip])
}

#[test]
fn float_clips_bilinear_native_mask_integrates_inside_joint_geometry_below_one_byte() {
    let doc = weighted_document();
    let before = snapshot(&doc);
    let output = composite(&doc, 1).unwrap();
    assert_eq!(&output.sample(0)[..3], &[1., 0., 0.]);
    near(output.sample(0)[3], 1. / 256., 1e-10);
    assert!(output.sample(0)[3] < 1. / 255.);
    assert_ne!(output.sample(0)[3], 0.25 * 0.0625);
    let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    assert_eq!(
        composite(&reopened, 1)
            .unwrap()
            .samples()
            .unwrap()
            .as_slice(),
        output.samples().unwrap().as_slice()
    );
    same(&doc, &before);
    let (encoded, _) = crate::export::encode_admitted(
        &crate::export::Source::Document(doc.clone()),
        crate::export::Settings {
            format: crate::export::Format::Tiff,
            ..Default::default()
        },
        &crate::ml::NoProgress,
    )
    .unwrap();
    let mut reader = tiff::decoder::Decoder::new(std::io::Cursor::new(encoded.as_slice())).unwrap();
    assert_eq!(
        reader
            .get_tag_u16_vec(tiff::tags::Tag::ExtraSamples)
            .unwrap(),
        [2]
    );
    let tiff::decoder::DecodingResult::F32(values) = reader.read_image().unwrap() else {
        panic!("native mask export reduced")
    };
    assert_eq!(values.as_slice(), output.samples().unwrap().as_slice());
    if let Some(directory) = std::env::var_os("OMADESIGN_FLOAT_CLIP_QA_DIR") {
        let directory = std::path::PathBuf::from(directory);
        assert!(directory.starts_with("/home"));
        let file = directory.join("joint-mask-f32.tif");
        assert!(!file.exists());
        std::fs::write(file, encoded.as_slice()).unwrap();
        let file = directory.join("joint-mask.oma");
        assert!(!file.exists());
        std::fs::write(file, crate::project::encode(&doc).unwrap()).unwrap();
    }
}

#[test]
fn float_clips_native_mask_same_cache_distinct_samples_alpha_and_hdr_are_authoritative() {
    let mut shapes = vec![];
    let alpha = 0.25001f32;
    for (i, gray) in [0.50001f32, 0.50002].into_iter().enumerate() {
        let mut fill = rect(i as f32, 0., 1., 1., Fill::Solid(Rgba::rgb(0, 255, 0)));
        fill.mask = Some(mask(1, 1, vec![gray, gray, gray, alpha]));
        shapes.push(fill);
    }
    assert_eq!(
        shapes[0].mask.as_ref().unwrap().data.as_slice(),
        shapes[1].mask.as_ref().unwrap().data.as_slice()
    );
    let mut doc = document(2, 1, shapes);
    let before = snapshot(&doc);
    let output = composite(&doc, 1).unwrap();
    for (i, gray) in [0.50001f32, 0.50002].into_iter().enumerate() {
        near(output.sample(i)[3], gray as f64 * alpha as f64, 2e-8);
    }
    assert!(output.sample(1)[3] > output.sample(0)[3]);
    same(&doc, &before);
    let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    let restored = composite(&reopened, 1).unwrap();
    assert_eq!(
        restored.samples().unwrap().as_slice(),
        output.samples().unwrap().as_slice()
    );
    for (original, reopened) in doc.layers[0]
        .kind
        .shapes()
        .unwrap()
        .iter()
        .zip(reopened.layers[0].kind.shapes().unwrap())
    {
        assert_eq!(
            original
                .mask
                .as_ref()
                .unwrap()
                .samples()
                .unwrap()
                .as_slice(),
            reopened
                .mask
                .as_ref()
                .unwrap()
                .samples()
                .unwrap()
                .as_slice()
        );
    }
    let original = vec![1.25, 0.50001, -0.125, 1., 1.25, 0.50002, -0.125, 1.];
    doc.layers.insert(
        0,
        Layer::placed_raster(
            "Native HDR",
            mask(2, 1, original.clone()),
            Pt::ZERO,
            Pt::new(2., 1.),
        ),
    );
    let before = snapshot(&doc);
    let result = composite(&doc, 1).unwrap();
    for (i, gray) in [0.50001f32, 0.50002].into_iter().enumerate() {
        let a = gray as f64 * alpha as f64;
        for c in 0..3 {
            near(
                result.sample(i)[c],
                (if c == 1 { a } else { 0. }) + original[i * 4 + c] as f64 * (1. - a),
                2e-7,
            );
        }
        assert!(result.sample(i)[0] > 1.);
        assert!(result.sample(i)[2] < 0.);
        assert_eq!(result.sample(i)[3], 1.);
    }
    same(&doc, &before);
}

#[test]
fn float_clips_nested_frame_rotation_mask_placement_and_tree_order_match_analytic_area() {
    let mut outer = frame(0., 0., 2., 2.);
    outer.rotation = std::f32::consts::FRAC_PI_2;
    let mut inner = frame(0., 0., 1., 2.);
    inner.rotation = std::f32::consts::FRAC_PI_2;
    inner.layout.parent = Some(outer.id);
    let mut fill = rect(-1., -1., 4., 4., Fill::Solid(Rgba::WHITE));
    fill.layout.parent = Some(inner.id);
    fill.mask = Some(mask(1, 1, vec![0.5, 0.5, 0.5, 1.]));
    let doc = document(2, 2, vec![fill, inner, outer]);
    let before = snapshot(&doc);
    let p = composite(&doc, 1).unwrap();
    for (i, expected) in [0.25, 0.25, 0.125, 0.125].into_iter().enumerate() {
        near(p.sample(i)[3], expected, 2e-6);
    }
    let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    assert_eq!(
        composite(&reopened, 1)
            .unwrap()
            .samples()
            .unwrap()
            .as_slice(),
        p.samples().unwrap().as_slice()
    );
    same(&doc, &before);
}

#[test]
fn float_clips_shape_rotation_moves_original_mask_texels_with_geometry() {
    let mut fill = rect(0., 0., 1., 1., Fill::Solid(Rgba::WHITE));
    fill.rotation = std::f32::consts::FRAC_PI_2;
    fill.mask = Some(mask(2, 1, vec![0., 0., 0., 1., 1., 1., 1., 1.]));
    let clip = frame(0., 0., 1., 0.5);
    fill.layout.parent = Some(clip.id);
    let doc = document(1, 1, vec![fill, clip]);
    let before = snapshot(&doc);
    near(composite(&doc, 1).unwrap().sample(0)[3], 1. / 16., 1e-7);
    same(&doc, &before);
}

#[test]
fn float_clips_mask_interpolates_premultiplied_luminance_and_original_alpha() {
    let clip = frame(0.25, 0., 0.25, 1.);
    let mut fill = rect(0., 0., 1., 1., Fill::Solid(Rgba::WHITE));
    fill.layout.parent = Some(clip.id);
    let alpha = 0.25001f32;
    fill.mask = Some(mask(2, 1, vec![1., 1., 1., alpha, 0., 0., 0., 0.75001]));
    let doc = document(1, 1, vec![fill, clip]);
    let before = snapshot(&doc);
    near(
        composite(&doc, 1).unwrap().sample(0)[3],
        alpha as f64 * 3. / 16.,
        1e-8,
    );
    same(&doc, &before);
    let mut fill = rect(0., 0., 1., 1., Fill::Solid(Rgba::WHITE));
    fill.mask = Some(mask(2, 1, vec![0., 0., 0., 1., 1., 1., 1., 0.]));
    let doc = document(1, 1, vec![fill]);
    let before = snapshot(&doc);
    assert_eq!(composite(&doc, 1).unwrap().sample(0), [0.; 4]);
    same(&doc, &before);
}

#[test]
fn float_clips_parent_rotation_keeps_gradient_coordinates_and_frame_painter_order() {
    let mut container = frame(0., 0., 1., 1.);
    container.rotation = std::f32::consts::FRAC_PI_2;
    let mut fill = rect(
        0.,
        0.,
        1.,
        1.,
        Fill::Linear {
            from: [0., 0.5],
            to: [1., 0.5],
            c0: Rgba::BLACK,
            c1: Rgba::WHITE,
        },
    );
    fill.layout.parent = Some(container.id);
    let doc = document(1, 1, vec![fill, container]);
    let before = snapshot(&doc);
    let p = composite(&doc, 2).unwrap();
    for (i, expected) in [0.25, 0.25, 0.75, 0.75].into_iter().enumerate() {
        for c in 0..3 {
            near(p.sample(i)[c], expected, 2e-6);
        }
        near(p.sample(i)[3], 1., 2e-6);
    }
    same(&doc, &before);
    let mut container = frame(0., 0., 2., 1.);
    container.style.fill = Fill::Solid(Rgba::rgb(0, 0, 255));
    let mut fill = rect(1., 0., 1., 1., Fill::Solid(Rgba::rgb(255, 0, 0)));
    fill.layout.parent = Some(container.id);
    let doc = document(2, 1, vec![fill, container]);
    let p = composite(&doc, 1).unwrap();
    assert_eq!(p.sample(0), [0., 0., 1., 1.]);
    assert_eq!(p.sample(1), [1., 0., 0., 1.]);
}

#[test]
fn float_clips_centered_stroke_is_intersected_before_coverage() {
    let clip = frame(0., 0., 0.5, 1.);
    let mut shape = Shape::new(
        Geom::Path {
            anchors: vec![
                crate::geom::Anchor::corner(Pt::new(0.5, 0.)),
                crate::geom::Anchor::corner(Pt::new(0.5, 1.)),
            ],
            closed: false,
        },
        Style {
            fill: Fill::None,
            stroke: Some(Stroke {
                width: 1. / 2048.,
                cap: crate::document::Cap::Butt,
                color: Rgba::WHITE,
                ..Default::default()
            }),
        },
    );
    shape.layout.parent = Some(clip.id);
    let geometric = document(1, 1, vec![shape, clip.clone()]);
    near(
        composite(&geometric, 1).unwrap().sample(0)[3],
        1. / 4096.,
        1e-10,
    );
    let mut masked = Shape::new(
        Geom::Path {
            anchors: vec![
                crate::geom::Anchor::corner(Pt::ZERO),
                crate::geom::Anchor::corner(Pt::splat(1.)),
            ],
            closed: false,
        },
        Style {
            fill: Fill::None,
            stroke: Some(Stroke {
                width: 4.,
                cap: crate::document::Cap::Butt,
                color: Rgba::WHITE,
                ..Default::default()
            }),
        },
    );
    masked.layout.parent = Some(clip.id);
    masked.mask = Some(mask(1, 1, vec![0.50001, 0.50001, 0.50001, 1.]));
    let doc = document(1, 1, vec![masked, clip]);
    let before = snapshot(&doc);
    near(
        composite(&doc, 1).unwrap().sample(0)[3],
        0.5 * 0.50001f32 as f64,
        3e-8,
    );
    same(&doc, &before);
}

#[test]
fn float_clips_cancel_pressure_early_cleanup_and_source_immutability_use_actual_pool() {
    let mut doc = weighted_document();
    doc.width = 32.;
    doc.height = 32.;
    doc.layers[0].kind.shapes_mut().unwrap()[0].mask = Some(mask(
        16,
        16,
        [0.50001, 0.50001, 0.50001, 0.70001].repeat(256),
    ));
    let pool = resources::global();
    let baseline = pool.usage().0;
    let before = snapshot(&doc);
    let peak = AtomicU64::new(baseline);
    let checks = Cell::new(0u32);
    let cancellation = || {
        peak.fetch_max(pool.usage().0, Ordering::SeqCst);
        checks.set(checks.get() + 1);
        if checks.get() > 80 {
            Err("Cancelled joint clip work".into())
        } else {
            Ok(())
        }
    };
    assert!(
        composite_with_cancel(&doc, 1, &cancellation)
            .unwrap_err()
            .contains("Cancelled joint clip work")
    );
    assert!(peak.load(Ordering::SeqCst) > baseline);
    assert_eq!(pool.usage().0, baseline);
    same(&doc, &before);
    let cancelled = || Err("Cancelled before masks".into());
    assert!(composite_with_cancel(&doc, 1, &cancelled).is_err());
    assert_eq!(pool.usage().0, baseline);
    let mut huge = doc.clone();
    huge.width = (pool.usage().1 / 16 + 4096) as f32;
    huge.height = 1.;
    let saved = snapshot(&huge);
    assert!(
        composite(&huge, 1)
            .unwrap_err()
            .contains("shared pixel budget")
    );
    assert_eq!(pool.usage().0, baseline);
    same(&huge, &saved);
    let out = composite(&doc, 1).unwrap();
    let native = out.native.as_ref().unwrap().samples.clone();
    drop(out);
    assert!(pool.usage().0 > baseline);
    drop(native);
    assert_eq!(pool.usage().0, baseline);
    same(&doc, &before);
}

#[test]
fn float_clips_unsupported_appearance_color_masks_and_invalid_hierarchy_keep_named_refusals() {
    let mut doc = weighted_document();
    let shapes = doc.layers[0].kind.shapes_mut().unwrap();
    shapes[1].opacity = 0.5;
    assert!(
        composite(&doc, 1)
            .unwrap_err()
            .contains("float appearance isolation")
    );
    doc.layers[0].kind.shapes_mut().unwrap()[1].opacity = 1.;
    doc.layers[0].kind.shapes_mut().unwrap()[0].style.stroke = Some(Stroke::default());
    assert!(
        composite(&doc, 1)
            .unwrap_err()
            .contains("joint float appearance isolation")
    );
    doc.layers[0].kind.shapes_mut().unwrap()[0].style.stroke = None;
    let original = doc.layers[0].kind.shapes().unwrap()[0].mask.clone();
    doc.layers[0].kind.shapes_mut().unwrap()[0]
        .mask
        .as_mut()
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .icc_profile = crate::deep_color::storage::adopt(
        include_bytes!("../../formats/psd/fixtures/srgb-cc0.icc").to_vec(),
    )
    .unwrap();
    let before = snapshot(&doc);
    assert!(
        composite(&doc, 1)
            .unwrap_err()
            .contains("native color-coverage pipeline")
    );
    same(&doc, &before);
    doc.layers[0].kind.shapes_mut().unwrap()[0].mask = original;
    doc.color.model = crate::deep_color::cms::Model::Cmyk;
    assert!(
        composite(&doc, 1)
            .unwrap_err()
            .contains("ink-aware float coverage")
    );
    doc.color.model = crate::deep_color::cms::Model::Rgb;
    let frame_id = doc.layers[0].kind.shapes().unwrap()[1].id;
    doc.layers[0].kind.shapes_mut().unwrap()[1].layout.parent = Some(frame_id);
    let before = snapshot(&doc);
    assert!(composite(&doc, 1).unwrap_err().contains("cycle"));
    same(&doc, &before);
    let mut byte = weighted_document();
    byte.color.depth = Depth::U8;
    byte.layers[0].kind.shapes_mut().unwrap()[0].mask = Some(
        Pixels::from_rgba(
            2,
            2,
            vec![0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255, 255],
        )
        .unwrap(),
    );
    assert!(crate::compositor::render_export(&byte, 1).is_ok());
    assert!(crate::formats::pdf::write(&byte).is_ok());
    assert!(
        crate::formats::pdf::write(&weighted_document())
            .unwrap_err()
            .contains("PDF cannot preserve native")
    );
}
