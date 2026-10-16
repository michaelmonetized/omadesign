use super::*;
use crate::{
    color::Rgba,
    document::{Fill, Shape, Stroke, Style},
    geom::{CharSpan, Geom, TypeRun},
    gradient::{Gradient, GradientKind, GradientStop, Spread},
    ml::NoProgress,
    resources,
};
use std::{
    cell::Cell,
    io::Cursor,
    sync::atomic::{AtomicU64, Ordering},
};

fn document(width: u32, height: u32, shapes: Vec<Shape>) -> Document {
    let mut doc = Document::new_with_options(
        "Float vector",
        width as f32,
        height as f32,
        72.,
        true,
        1,
        false,
        false,
    );
    doc.color.depth = Depth::F32;
    let mut layer = Layer::vector("Authored vectors");
    *layer.kind.shapes_mut().unwrap() = shapes;
    doc.layers = vec![layer];
    doc
}
fn rect(origin: Pt, size: Pt, fill: Fill) -> Shape {
    Shape::new(
        Geom::Rect {
            origin,
            size,
            radius: 0.,
        },
        Style { fill, stroke: None },
    )
}
fn close(actual: f32, expected: f64, tolerance: f64) {
    assert!(
        (actual as f64 - expected).abs() <= tolerance,
        "{actual:?} != {expected:?}"
    );
}
fn unchanged(doc: &Document, before: &str) {
    assert_eq!(serde_json::to_string(doc).unwrap(), before);
}

#[test]
fn float_vector_sub_byte_area_and_unrounded_alpha_are_analytic() {
    let mut shape = rect(
        Pt::ZERO,
        Pt::new(1. / 4096., 1.),
        Fill::Solid(Rgba::new(255, 0, 0, 1)),
    );
    shape.opacity = 0.37;
    shape.fill_opacity = 0.43;
    let mut doc = document(1, 1, vec![shape]);
    doc.layers[0].opacity = 0.61;
    let before = serde_json::to_string(&doc).unwrap();
    let result = composite(&doc, 1).unwrap();
    let p = result.sample(0);
    let expected =
        (1. / 4096.) * (1. / 255.) * (0.37f32 as f64) * (0.43f32 as f64) * (0.61f32 as f64);
    assert!(p[3] > 0. && p[3] < 1. / 255.);
    close(p[3], expected, expected * 3e-6);
    assert_eq!(&p[..3], &[1., 0., 0.]);
    unchanged(&doc, &before);
    if let Some(directory) = std::env::var_os("OMADESIGN_FLOAT_VECTOR_QA_DIR") {
        let directory = std::path::PathBuf::from(directory);
        assert!(directory.starts_with("/home"));
        let (encoded, _) = crate::export::encode_admitted(
            &crate::export::Source::Document(doc.clone()),
            crate::export::Settings {
                format: crate::export::Format::Tiff,
                ..Default::default()
            },
            &NoProgress,
        )
        .unwrap();
        let file = directory.join("float-vector-sub-byte.tif");
        assert!(!file.exists());
        std::fs::write(file, encoded.as_slice()).unwrap();
    }
    for scale in [2, 4] {
        let result = composite(&doc, scale).unwrap();
        let sum: f64 = result
            .samples()
            .unwrap()
            .chunks_exact(4)
            .map(|p| p[3] as f64)
            .sum();
        assert!((sum / ((scale * scale) as f64) - expected).abs() < expected * 4e-6);
    }
}

#[test]
fn float_vector_preserves_hdr_negative_and_distinct_authoring_under_same_display_bytes() {
    let mut doc = document(
        2,
        1,
        vec![rect(
            Pt::ZERO,
            Pt::new(2., 1.),
            Fill::Solid(Rgba::new(0, 255, 0, 1)),
        )],
    );
    let values = vec![1.25, 0.50001, -0.125, 1., 1.25, 0.50002, -0.125, 1.];
    let pixels = Pixels::from_samples(2, 1, Depth::F32, values.clone()).unwrap();
    assert_eq!(&pixels.data[..4], &pixels.data[4..]);
    doc.layers.insert(
        0,
        Layer::placed_raster("Native", pixels, Pt::ZERO, Pt::new(2., 1.)),
    );
    let before = serde_json::to_string(&doc).unwrap();
    let output = composite(&doc, 1).unwrap();
    let alpha = 1. / 255.;
    for i in 0..2 {
        let p = output.sample(i);
        for c in 0..3 {
            let source = if c == 1 { 1. } else { 0. };
            close(
                p[c],
                source * alpha + (values[i * 4 + c] as f64) * (1. - alpha),
                2e-7,
            );
        }
        assert!(p[0] > 1.);
        assert!(p[2] < 0.);
    }
    assert!(output.sample(1)[1] > output.sample(0)[1]);
    unchanged(&doc, &before);
    let (encoded, _) = crate::export::encode_admitted(
        &crate::export::Source::Document(doc.clone()),
        crate::export::Settings {
            format: crate::export::Format::Tiff,
            ..Default::default()
        },
        &NoProgress,
    )
    .unwrap();
    let mut decoder = tiff::decoder::Decoder::new(Cursor::new(encoded.as_slice())).unwrap();
    assert_eq!(decoder.dimensions().unwrap(), (2, 1));
    assert_eq!(
        decoder
            .get_tag_u16_vec(tiff::tags::Tag::ExtraSamples)
            .unwrap(),
        [2]
    );
    let tiff::decoder::DecodingResult::F32(decoded) = decoder.read_image().unwrap() else {
        panic!("float TIFF reduced")
    };
    assert_eq!(decoded.as_slice(), output.samples().unwrap().as_slice());
    if let Some(directory) = std::env::var_os("OMADESIGN_FLOAT_VECTOR_QA_DIR") {
        let file = std::path::PathBuf::from(directory).join("float-vector-hdr.tif");
        assert!(file.starts_with("/home"));
        assert!(!file.exists());
        std::fs::write(file, encoded.as_slice()).unwrap();
    }
    unchanged(&doc, &before);
}

#[test]
fn float_vector_native_rgb_tiff_declares_straight_alpha_and_exact_u16_samples() {
    let mut doc = document(1, 1, vec![]);
    doc.color.depth = Depth::U16;
    doc.layers = vec![Layer::placed_raster(
        "Authored 16-bit alpha",
        Pixels::from_samples(
            1,
            1,
            Depth::U16,
            vec![
                1000. / 65535.,
                2000. / 65535.,
                3000. / 65535.,
                16000. / 65535.,
            ],
        )
        .unwrap(),
        Pt::ZERO,
        Pt::splat(1.),
    )];
    let before = serde_json::to_string(&doc).unwrap();
    let (encoded, _) = crate::export::encode_admitted(
        &crate::export::Source::Document(doc.clone()),
        crate::export::Settings {
            format: crate::export::Format::Tiff,
            ..Default::default()
        },
        &NoProgress,
    )
    .unwrap();
    let mut decoder = tiff::decoder::Decoder::new(Cursor::new(encoded.as_slice())).unwrap();
    assert_eq!(
        decoder
            .get_tag_u16_vec(tiff::tags::Tag::ExtraSamples)
            .unwrap(),
        [2]
    );
    assert_eq!(
        decoder
            .get_tag_u16_vec(tiff::tags::Tag::BitsPerSample)
            .unwrap(),
        [16; 4]
    );
    let tiff::decoder::DecodingResult::U16(samples) = decoder.read_image().unwrap() else {
        panic!("native TIFF depth changed")
    };
    assert_eq!(samples, [1000, 2000, 3000, 16000]);
    if let Some(directory) = std::env::var_os("OMADESIGN_FLOAT_VECTOR_QA_DIR") {
        let file = std::path::PathBuf::from(directory).join("float-vector-u16-alpha.tif");
        assert!(file.starts_with("/home"));
        assert!(!file.exists());
        std::fs::write(file, encoded.as_slice()).unwrap();
    }
    unchanged(&doc, &before);
}

#[test]
fn float_vector_fill_and_center_stroke_apply_shape_layer_and_native_mask_opacity_once() {
    let mut shape = rect(Pt::ZERO, Pt::splat(1.), Fill::Solid(Rgba::rgb(255, 0, 0)));
    shape.style.stroke = Some(Stroke {
        width: 2.,
        color: Rgba::rgb(0, 0, 255),
        ..Default::default()
    });
    shape.opacity = 0.5;
    let mut doc = document(1, 1, vec![shape]);
    doc.layers[0].opacity = 0.3;
    doc.layers[0].fill_opacity = 0.4;
    doc.layers[0].mask =
        Some(Pixels::from_samples(1, 1, Depth::F32, vec![0.50001, 0.50001, 0.50001, 1.]).unwrap());
    let output = composite(&doc, 1).unwrap();
    let p = output.sample(0);
    assert_eq!(&p[..3], &[0., 0., 1.]);
    close(
        p[3],
        0.5 * (0.3f32 as f64) * (0.4f32 as f64) * (0.50001f32 as f64),
        2e-8,
    );
}

#[test]
fn float_vector_winding_intersections_and_curve_area_preserve_fractional_geometry() {
    let mut pb = tiny_skia::PathBuilder::new();
    for x in [0., 0.25] {
        pb.push_rect(tiny_skia::Rect::from_xywh(x, 0., 0.5, 1.).unwrap());
    }
    let path = pb.finish().unwrap();
    for (nonzero, expected) in [(true, 0.75), (false, 0.5)] {
        let mut area = 0.;
        coverage::rows(
            &path,
            tiny_skia::Transform::identity(),
            nonzero,
            1,
            1,
            &|| Ok(()),
            |_, row| {
                area += row[0];
                Ok(())
            },
        )
        .unwrap();
        assert!((area - expected).abs() < 1e-12);
    }
    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(0., 0.);
    pb.line_to(1., 1.);
    pb.line_to(1., 0.);
    pb.line_to(0., 1.);
    pb.close();
    let mut area = 0.;
    coverage::rows(
        &pb.finish().unwrap(),
        tiny_skia::Transform::identity(),
        false,
        1,
        1,
        &|| Ok(()),
        |_, row| {
            area += row[0];
            Ok(())
        },
    )
    .unwrap();
    assert!((area - 0.5).abs() < 1e-12);
    let circle = Shape::new(
        Geom::Ellipse {
            center: Pt::splat(0.5),
            radii: Pt::splat(0.25),
        },
        Style {
            fill: Fill::Solid(Rgba::WHITE),
            stroke: None,
        },
    );
    let value = composite(&document(1, 1, vec![circle]), 1)
        .unwrap()
        .sample(0)[3];
    close(value, std::f64::consts::PI / 16., 0.00008);
    assert!((value * 255. - (value * 255.).round()).abs() > 0.001);
}

#[test]
fn float_vector_gradient_alpha_channels_hard_stops_spread_and_affine_radial_are_unrounded() {
    let mut gradient = Gradient::new(
        GradientKind::Linear,
        Rgba::new(0, 0, 0, 0),
        Rgba::new(255, 100, 40, 1),
    );
    gradient.from = [0., 0.];
    gradient.to = [1., 0.];
    let doc = document(
        7,
        1,
        vec![rect(
            Pt::ZERO,
            Pt::new(7., 1.),
            Fill::Gradient(gradient.clone()),
        )],
    );
    let output = composite(&doc, 1).unwrap();
    for x in 0..7 {
        let p = output.sample(x);
        close(p[3], ((x as f64 + 0.5) / 7.) / 255., 2e-10);
        close(p[0], 1., 2e-7);
        close(p[1], 100. / 255., 2e-7);
    }
    let bounds = crate::geom::Bounds {
        min: Pt::ZERO,
        max: Pt::new(4., 4.),
    };
    let color = crate::deep_color::cms::DocumentColor::default();
    gradient.stops = vec![
        GradientStop {
            offset: 0.,
            color: Rgba::BLACK,
        },
        GradientStop {
            offset: 0.5,
            color: Rgba::BLACK,
        },
        GradientStop {
            offset: 0.5,
            color: Rgba::WHITE,
        },
        GradientStop {
            offset: 1.,
            color: Rgba::WHITE,
        },
    ];
    for spread in [Spread::Pad, Spread::Repeat, Spread::Reflect] {
        gradient.spread = spread;
        let paint = paint::Paint::new(
            &Fill::Gradient(gradient.clone()),
            bounds,
            0.,
            &color,
            &|| Ok(()),
        )
        .unwrap();
        let point = match spread {
            Spread::Pad => Pt::new(2., 2.),
            Spread::Repeat => Pt::new(6., 2.),
            Spread::Reflect => Pt::new(6., 2.),
        };
        assert_eq!(paint.sample(point).unwrap(), [1.; 4]);
    }
    let mut radial = Gradient::new(GradientKind::Radial, Rgba::BLACK, Rgba::WHITE);
    radial.radial_axis = Some([0.125, 0.25]);
    radial.inner_radius = 0.25;
    let paint = paint::Paint::new(&Fill::Gradient(radial), bounds, 0., &color, &|| Ok(())).unwrap();
    let p = paint.sample(Pt::new(3., 2.)).unwrap();
    close(p[0], 1. / 3., 2e-7);
    close(p[3], 1., 0.);
}

#[test]
fn float_vector_whole_and_solid_span_text_share_original_curves_affine_placement_and_archive() {
    let mut run = TypeRun {
        content: "Bé".into(),
        origin: Pt::new(4., 25.),
        px: 24.,
        transform: Some([1., 0.125, 0.2, 0.9, 1., -0.5]),
        ..Default::default()
    };
    run.contours = crate::text::shape(&run);
    let mut whole = Shape::new(
        Geom::Text(run.clone()),
        Style {
            fill: Fill::Solid(Rgba::new(12, 80, 200, 173)),
            stroke: None,
        },
    );
    whole.rotation = 0.12;
    let doc = document(50, 36, vec![whole.clone()]);
    let before = serde_json::to_string(&doc).unwrap();
    let a = composite(&doc, 1).unwrap();
    run.spans.push(CharSpan {
        start: 0,
        end: 2,
        fill: Some(whole.style.fill.clone()),
        ..Default::default()
    });
    let mut span = whole;
    span.geom = Geom::Text(run);
    let spans = document(50, 36, vec![span]);
    let b = composite(&spans, 1).unwrap();
    for (a, b) in a.samples().unwrap().iter().zip(b.samples().unwrap().iter()) {
        assert!((a - b).abs() < 2e-6);
    }
    assert!(
        a.samples()
            .unwrap()
            .chunks_exact(4)
            .any(|p| p[3] > 0. && (p[3] * 255. - (p[3] * 255.).round()).abs() > 0.01)
    );
    let reopened = crate::project::decode(&crate::project::encode(&spans).unwrap()).unwrap();
    assert_eq!(
        composite(&reopened, 1)
            .unwrap()
            .samples()
            .unwrap()
            .as_slice(),
        b.samples().unwrap().as_slice()
    );
    unchanged(&doc, &before);
}

#[test]
fn float_vector_real_admission_cancel_pressure_and_final_owner_preserve_source() {
    let shape = rect(
        Pt::new(0.125, 0.25),
        Pt::new(63.5, 31.5),
        Fill::Solid(Rgba::new(12, 90, 180, 127)),
    );
    let doc = document(64, 32, vec![shape]);
    let before = serde_json::to_string(&doc).unwrap();
    let pool = resources::global();
    let baseline = pool.usage().0;
    let admitted = pool.allocate_now::<f32, _>(4, &|| Ok(())).unwrap();
    let (samples, memory) = admitted.into_parts();
    let retained = std::sync::Arc::downgrade(&memory);
    let cancelled_before_adoption = || Err("Cancelled native adoption".to_string());
    assert!(
        Pixels::from_admitted_samples(
            1,
            1,
            Depth::F32,
            samples,
            memory,
            &cancelled_before_adoption
        )
        .unwrap_err()
        .contains("Cancelled native adoption")
    );
    assert!(retained.upgrade().is_none());
    assert_eq!(pool.usage().0, baseline);
    let peak = AtomicU64::new(baseline);
    let calls = Cell::new(0u32);
    let cancelled = || {
        peak.fetch_max(pool.usage().0, Ordering::SeqCst);
        calls.set(calls.get() + 1);
        if calls.get() > 100 {
            Err("Cancelled float vector".into())
        } else {
            Ok(())
        }
    };
    assert!(
        composite_with_cancel(&doc, 1, &cancelled)
            .unwrap_err()
            .contains("Cancelled")
    );
    assert!(peak.load(Ordering::SeqCst) > baseline);
    assert_eq!(pool.usage().0, baseline);
    unchanged(&doc, &before);
    let output = composite(&doc, 1).unwrap();
    let samples = output.native.as_ref().unwrap().samples.clone();
    drop(output);
    assert!(pool.usage().0 > baseline);
    let clone = samples.clone();
    drop(samples);
    assert!(pool.usage().0 > baseline);
    drop(clone);
    assert_eq!(pool.usage().0, baseline);
    let mut huge = doc.clone();
    huge.width = (pool.usage().1 / 16 + 4096) as f32;
    huge.height = 1.;
    let saved = serde_json::to_string(&huge).unwrap();
    let started = std::time::Instant::now();
    assert!(
        composite(&huge, 1)
            .unwrap_err()
            .contains("shared pixel budget")
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    assert_eq!(pool.usage().0, baseline);
    unchanged(&huge, &saved);
    unchanged(&doc, &before);
}

#[test]
fn float_vector_unsupported_paths_refuse_without_touching_source_or_byte_routes() {
    let base = rect(Pt::ZERO, Pt::splat(2.), Fill::Solid(Rgba::WHITE));
    let mut cases = Vec::new();
    let mut shape = base.clone();
    shape.layout.frame = true;
    shape.opacity = 0.5;
    cases.push((shape, "float appearance isolation"));
    let mut shape = base.clone();
    shape.mask = Some(Pixels::from_samples(1, 1, Depth::F32, vec![1.25, 0., 0., 1.]).unwrap());
    cases.push((shape, "unit interval"));
    let mut shape = base.clone();
    shape.style.stroke = Some(Stroke {
        alignment: crate::document::StrokeAlignment::Inside,
        ..Default::default()
    });
    cases.push((shape, "float clip renderer"));
    let mut shape = base.clone();
    shape.style.fill = Fill::Gradient(Gradient::new(GradientKind::Shape, Rgba::BLACK, Rgba::WHITE));
    cases.push((shape, "float paint renderer"));
    let mut shape = base.clone();
    let mut gradient = Gradient::new(GradientKind::Linear, Rgba::BLACK, Rgba::WHITE);
    gradient.to = gradient.from;
    shape.style.fill = Fill::Gradient(gradient);
    cases.push((shape, "singular field"));
    for (shape, message) in cases {
        let doc = document(2, 2, vec![shape]);
        let before = serde_json::to_string(&doc).unwrap();
        assert!(
            composite(&doc, 1).unwrap_err().contains(message),
            "{message}"
        );
        unchanged(&doc, &before);
    }
    let doc = document(2, 2, vec![base]);
    assert!(
        crate::formats::pdf::write(&doc)
            .unwrap_err()
            .contains("PDF cannot preserve native")
    );
    let mut byte = doc.clone();
    byte.color.depth = Depth::U8;
    assert!(crate::compositor::render_export(&byte, 1).is_ok());
    assert!(crate::formats::pdf::write(&byte).is_ok());
}

#[test]
fn float_vector_tagged_solid_keeps_actual_profile_intent_and_profiled_gradient_refusal() {
    let profile = include_bytes!("../../formats/psd/fixtures/srgb-cc0.icc").to_vec();
    let color = Rgba::new(12, 90, 180, 127);
    let mut doc = document(
        1,
        1,
        vec![rect(Pt::ZERO, Pt::splat(1.), Fill::Solid(color))],
    );
    doc.color.icc_profile = profile.clone();
    doc.color.intent = 1;
    let expected = crate::deep_color::cms::transform(
        &[12. / 255., 90. / 255., 180. / 255.],
        crate::deep_color::cms::Model::Rgb,
        &profile,
        false,
        1,
    )
    .unwrap();
    let before = serde_json::to_string(&doc).unwrap();
    let output = composite(&doc, 1).unwrap();
    let p = output.sample(0);
    for c in 0..3 {
        close(p[c], expected[c] as f64, 2e-7);
    }
    close(p[3], 127. / 255., 1e-7);
    assert_eq!(
        output.native.as_ref().unwrap().icc_profile.as_slice(),
        profile
    );
    assert_eq!(output.native.as_ref().unwrap().intent, 1);
    unchanged(&doc, &before);
    doc.layers[0].kind.shapes_mut().unwrap()[0].style.fill = Fill::Gradient(Gradient::new(
        GradientKind::Linear,
        Rgba::BLACK,
        Rgba::WHITE,
    ));
    let before = serde_json::to_string(&doc).unwrap();
    assert!(composite(&doc, 1).unwrap_err().contains("batched ICC"));
    unchanged(&doc, &before);
}

#[test]
fn float_vector_hairline_butt_stroke_and_dash_keep_sub_byte_area() {
    let mut stroke = Stroke {
        width: 1. / 4096.,
        cap: crate::document::Cap::Butt,
        color: Rgba::WHITE,
        ..Default::default()
    };
    for (dash, fraction) in [(None, 1.), (Some((0.25, 0.25)), 0.5)] {
        stroke.dash = dash;
        let shape = Shape::new(
            Geom::Path {
                anchors: vec![
                    crate::geom::Anchor::corner(Pt::new(0.5, 0.)),
                    crate::geom::Anchor::corner(Pt::new(0.5, 1.)),
                ],
                closed: false,
            },
            Style {
                fill: Fill::None,
                stroke: Some(stroke.clone()),
            },
        );
        let p = composite(&document(1, 1, vec![shape]), 1)
            .unwrap()
            .sample(0);
        close(p[3], fraction / 4096., 1e-8);
        assert!(p[3] > 0. && p[3] < 1. / 255.);
    }
}
