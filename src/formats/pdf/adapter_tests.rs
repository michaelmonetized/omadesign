//! Real output checks for the PDF adapters on the common SVG source model.
use super::*;
use crate::artwork_mask::{ArtworkMask, MaskSource};
use crate::geom::{Bounds, CharSpan, TypeRun};
use crate::gradient::{Gradient, GradientKind, GradientStop, Spread};

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
fn backdrop(doc: &mut Document) {
    let mut layer = Layer::vector("White backdrop");
    layer.kind.shapes_mut().unwrap().push(rect(
        Pt::ZERO,
        Pt::new(doc.width, doc.height),
        Fill::Solid(Rgba::WHITE),
    ));
    doc.layers.insert(0, layer);
}
fn radial() -> Gradient {
    let mut g = Gradient::new(
        GradientKind::Radial,
        Rgba::new(245, 35, 20, 255),
        Rgba::new(20, 70, 235, 80),
    );
    g.from = [0.43, 0.48];
    g.to = [0.85, 0.64];
    g.radial_axis = Some([-0.2, 0.58]);
    g.focal = [0.17, -0.12];
    g.inner_radius = 0.14;
    g.stops.insert(
        1,
        GradientStop {
            offset: 0.42,
            color: Rgba::new(30, 230, 75, 150),
        },
    );
    g
}
fn fields() -> Document {
    let mut doc = Document::new("Affine focal radial and periodic fields", 300., 240., 72.);
    doc.layers.clear();
    backdrop(&mut doc);
    let mut layer = Layer::vector("Editable PDF fields");
    for (index, spread) in [Spread::Pad, Spread::Repeat, Spread::Reflect, Spread::Repeat]
        .into_iter()
        .enumerate()
    {
        let mut g = radial();
        g.spread = spread;
        if index == 3 {
            g.kind = GradientKind::Linear;
            g.from = [0.25, 0.4];
            g.to = [0.48, 0.55];
        }
        let mut s = rect(
            Pt::new(
                15. + (index % 2) as f32 * 150.,
                15. + (index / 2) as f32 * 120.,
            ),
            Pt::new(115., 85.),
            Fill::Gradient(g.clone()),
        );
        s.rotation = 0.07;
        s.style.stroke = Some(Stroke {
            width: 5.,
            color: Rgba::new(0, 0, 0, 40),
            gradient: Some(g),
            ..Default::default()
        });
        layer.kind.shapes_mut().unwrap().push(s);
    }
    doc.layers.push(layer);
    doc
}
fn native_text() -> Document {
    let mut doc = Document::new("Native mixed face size paint affine text", 520., 190., 72.);
    doc.layers.clear();
    backdrop(&mut doc);
    let mut run = TypeRun {
        content: "office café SMALL office Ω".into(),
        font: "/usr/share/fonts/noto/NotoSans-Regular.ttf".into(),
        px: 25.,
        origin: Pt::new(18., 55.),
        transform: Some([0.94, 0.11, 0.18, 1.08, 5., -2.]),
        ..Default::default()
    };
    run.spans = vec![
        CharSpan {
            start: 7,
            end: 11,
            font: Some("/usr/share/fonts/liberation/LiberationSerif-Regular.ttf".into()),
            px: Some(37.),
            fill: Some(Fill::Solid(Rgba::new(180, 35, 90, 185))),
            ..Default::default()
        },
        CharSpan {
            start: 12,
            end: 17,
            px: Some(15.),
            fill: Some(Fill::Gradient(Gradient::new(
                GradientKind::Linear,
                Rgba::rgb(235, 50, 20),
                Rgba::rgb(20, 90, 220),
            ))),
            ..Default::default()
        },
        CharSpan {
            start: 18,
            end: 24,
            fill: Some(pattern_fill()),
            ..Default::default()
        },
    ];
    run.contours = crate::text::shape(&run);
    let mut layer = Layer::vector("Native editable text");
    let mut shape = Shape::new(
        Geom::Text(run),
        Style {
            fill: Fill::Solid(Rgba::rgb(15, 35, 65)),
            stroke: Some(Stroke {
                width: 0.8,
                color: Rgba::new(30, 45, 70, 140),
                ..Default::default()
            }),
        },
    );
    shape.rotation = 0.04;
    layer.kind.shapes_mut().unwrap().push(shape);
    doc.layers.push(layer);
    doc
}
fn pattern_fill() -> Fill {
    let (doc, _) =
        crate::formats::svg::read(include_str!("fixtures/pattern.svg"), "Pattern paint").unwrap();
    doc.layers
        .iter()
        .flat_map(|l| l.kind.shapes().into_iter().flatten())
        .find_map(|s| matches!(s.style.fill, Fill::Pattern(_)).then(|| s.style.fill.clone()))
        .unwrap()
}
fn filtered(source_mask: bool) -> Document {
    let (mut doc, notes) = crate::formats::svg::read(
        include_str!("fixtures/filter.svg"),
        "Edited retained filter",
    )
    .unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    doc.artboards = vec![crate::document::Artboard::new(
        0,
        Pt::new(180., 110.),
        Pt::new(160., 120.),
    )];
    let graph = doc
        .layers
        .iter()
        .position(|l| l.svg_filter.is_some())
        .unwrap();
    let graph_id = doc.layers[graph].id;
    doc.layers[graph].name = "Edited filter graph".into();
    for layer in &mut doc.layers {
        for shape in layer.kind.shapes_mut().into_iter().flatten() {
            shape.geom.translate(Pt::new(6., -3.));
            shape.style.fill = Fill::Solid(Rgba::rgb(125, 35, 190));
        }
    }
    doc.layers[graph].svg_filter.as_mut().unwrap().definitions = doc.layers[graph]
        .svg_filter
        .as_ref()
        .unwrap()
        .definitions
        .replace("dx=\"12\"", "dx=\"18\"")
        .into();
    if source_mask {
        let mut layer = Layer::vector("Target using current filtered source");
        let mut s = rect(
            Pt::new(190., 118.),
            Pt::new(135., 100.),
            Fill::Solid(Rgba::rgb(230, 150, 25)),
        );
        s.artwork_masks.push(ArtworkMask::new(MaskSource {
            layer: graph_id,
            shape: None,
        }));
        layer.kind.shapes_mut().unwrap().push(s);
        doc.layers.push(layer);
    } else {
        let mut source = Layer::vector("Hidden live mask");
        source.kind.shapes_mut().unwrap().push(rect(
            Pt::new(200., 120.),
            Pt::new(90., 80.),
            Fill::Solid(Rgba::new(0, 0, 0, 160)),
        ));
        doc.layers[graph]
            .artwork_masks
            .push(ArtworkMask::new(MaskSource {
                layer: source.id,
                shape: None,
            }));
        doc.layers.push(source);
    }
    backdrop(&mut doc);
    doc
}

/// Render and inspect the actual exported file before applying strict comparison checks.
/// Args: authored fixture and receipt name. Returns the PDF object store and mean channel difference.
fn inspect(doc: &Document, name: &str, limit: f64) -> (Pdf, f64) {
    let persistent = std::env::var_os("OMA_PDF_EXCHANGE_RECEIPTS");
    let folder = persistent
        .as_ref()
        .map(|p| std::path::PathBuf::from(p).join(format!("adapter-{name}")))
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(".artifacts")
                .join(format!("pdf-adapter-{name}-{}", crate::document::next_id()))
        });
    std::fs::create_dir_all(&folder).unwrap();
    let source = folder.join("source.oma");
    crate::project::save_to(doc, &source).unwrap();
    let reopened = crate::project::load_from(&source).unwrap();
    let expected = crate::compositor::render_export(&reopened, 1).unwrap();
    let (bytes, notes) = write(&reopened).unwrap();
    let path = folder.join("output.pdf");
    std::fs::write(&path, &bytes).unwrap();
    for program in ["qpdf", "pdffonts", "pdfimages", "pdftotext"] {
        let output = std::process::Command::new(program)
            .args(match program {
                "qpdf" => vec!["--check"],
                "pdfimages" => vec!["-list"],
                "pdftotext" => vec!["-layout"],
                _ => vec![],
            })
            .arg(&path)
            .args(if program == "pdftotext" {
                vec!["-"]
            } else {
                vec![]
            })
            .output()
            .unwrap();
        std::fs::write(
            folder.join(format!("{program}.txt")),
            [output.stdout.clone(), output.stderr.clone()].concat(),
        )
        .unwrap();
        if program == "pdftotext" && name == "native-text" {
            assert!(
                String::from_utf8_lossy(&output.stdout).contains("office café SMALL office Ω"),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
        }
        assert!(
            output.status.success() && output.stderr.is_empty(),
            "{program}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = std::process::Command::new("pdftocairo")
        .args(["-png", "-r", "72", "-singlefile"])
        .arg(&path)
        .arg(folder.join("cairo"))
        .output()
        .unwrap();
    assert!(
        output.status.success() && output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let board = &doc.artboards[0];
    let actual = image::open(folder.join("cairo.png")).unwrap().to_rgb8();
    assert_eq!(
        actual.dimensions(),
        (board.size.x as u32, board.size.y as u32)
    );
    let mut native = image::RgbImage::new(actual.width(), actual.height());
    let mut total = 0u64;
    let mut max = 0u8;
    let mut painted = 0usize;
    for (x, y, p) in actual.enumerate_pixels() {
        let n = expected
            .pixel(x + board.origin.x as u32, y + board.origin.y as u32)
            .unwrap()
            .demultiply();
        let n = [n.red(), n.green(), n.blue()];
        painted += usize::from(n.iter().any(|v| *v < 245));
        native.put_pixel(x, y, image::Rgb(n));
        for (a, b) in n.into_iter().zip(p.0) {
            let d = a.abs_diff(b);
            total += u64::from(d);
            max = max.max(d);
        }
    }
    native.save(folder.join("native.png")).unwrap();
    let mean = total as f64 / f64::from(actual.width() * actual.height() * 3);
    let mut ranges = vec![];
    if name == "native-text" {
        let shape = doc
            .layers
            .iter()
            .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
            .find(|s| matches!(s.geom, Geom::Text(_)))
            .unwrap();
        let Geom::Text(run) = &shape.geom else {
            unreachable!()
        };
        let lines = crate::text::compose(run);
        for (start, end) in [(7, 11), (12, 17), (18, 24)] {
            let mut points = lines
                .iter()
                .flat_map(|l| &l.glyphs)
                .filter(|g| g.cluster >= start && g.cluster < end)
                .flat_map(|g| crate::text::glyph_contours(run, g))
                .flatten()
                .map(|p| {
                    run.map_point(p)
                        .rotate_about(shape.geom.bbox().center(), shape.rotation)
                });
            let mut region = Bounds::from_pt(points.next().unwrap());
            for point in points {
                region.union_pt(point);
            }
            let region = region.inflate(1.);
            let mut sum = 0u64;
            let mut count = 0u64;
            for (x, y, p) in actual.enumerate_pixels() {
                if region.contains(Pt::new(
                    x as f32 + board.origin.x,
                    y as f32 + board.origin.y,
                )) {
                    sum += native
                        .get_pixel(x, y)
                        .0
                        .into_iter()
                        .zip(p.0)
                        .map(|(a, b)| u64::from(a.abs_diff(b)))
                        .sum::<u64>();
                    count += 3;
                }
            }
            ranges.push((start, end, sum as f64 / count.max(1) as f64));
        }
    }
    std::fs::write(folder.join("result.json"),serde_json::to_vec_pretty(&serde_json::json!({"name":name,"bytes":bytes.len(),"mean_channel_error":mean,"max_channel_error":max,"painted_pixels":painted,"text_range_errors":ranges,"notes":notes})).unwrap()).unwrap();
    eprintln!(
        "PDF adapter {name}: {} bytes, native/Cairo mean {mean:0.4}/255, max {max}, notes {notes:?}",
        bytes.len()
    );
    assert!(
        painted > 100,
        "{name} fixture contains no meaningful artwork"
    );
    assert!(
        ranges.iter().all(|(_, _, error)| *error < 4.),
        "{name} effective range font/paint changed: {ranges:?}"
    );
    assert!(
        mean < limit,
        "{name} native/PDF error {mean}; evidence {}",
        folder.display()
    );
    let pdf = Pdf::load_mem(&bytes).unwrap();
    if persistent.is_none() {
        std::fs::remove_dir_all(folder).unwrap();
    }
    (pdf, mean)
}

#[test]
#[ignore = "independent PDF adapter qualification requires qpdf and Poppler"]
fn pdf_adapter_affine_periodic_fields_stroke_alpha_real_output() {
    let (pdf, _) = inspect(&fields(), "fields", 1.5);
    assert!(
        !pdf.objects
            .values()
            .filter_map(|o| o.as_stream().ok())
            .any(|s| s.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image"))
    );
    for kind in [2, 3] {
        assert!(
            pdf.objects
                .values()
                .filter_map(|o| o.as_dict().ok())
                .any(|d| d.get(b"ShadingType").and_then(Object::as_i64).ok() == Some(kind))
        );
    }
}

#[test]
#[ignore = "independent PDF stroke qualification requires qpdf and Poppler"]
fn pdf_adapter_periodic_field_covers_the_full_wide_stroke() {
    let mut doc = Document::new("Wide periodic stroke", 100., 80., 72.);
    doc.layers.clear();
    backdrop(&mut doc);
    let mut gradient = Gradient::new(
        GradientKind::Linear,
        Rgba::new(240, 30, 70, 240),
        Rgba::new(20, 80, 230, 90),
    );
    gradient.spread = Spread::Repeat;
    let mut shape = rect(Pt::new(45., 35.), Pt::new(10., 10.), Fill::None);
    shape.style.stroke = Some(Stroke {
        width: 70.,
        color: Rgba::new(0, 0, 0, 40),
        gradient: Some(gradient),
        ..Default::default()
    });
    let mut layer = Layer::vector("Wide editable gradient stroke");
    layer.kind.shapes_mut().unwrap().push(shape);
    doc.layers.push(layer);
    let (pdf, _) = inspect(&doc, "wide-stroke", 1.5);
    assert!(
        !pdf.objects
            .values()
            .filter_map(|o| o.as_stream().ok())
            .any(|s| s.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image"))
    );
}

#[test]
#[ignore = "independent PDF outline qualification requires qpdf and Poppler"]
fn pdf_adapter_outlined_range_rotates_about_the_whole_text_object() {
    let mut doc = Document::new("Rotated combining painted text", 420., 190., 72.);
    doc.layers.clear();
    backdrop(&mut doc);
    let mut run = TypeRun {
        content: "a\u{323}\u{30a} mixed paint".into(),
        font: "/usr/share/fonts/noto/NotoSans-Regular.ttf".into(),
        px: 35.,
        origin: Pt::new(50., 90.),
        spans: vec![CharSpan {
            start: 0,
            end: 3,
            fill: Some(Fill::Solid(Rgba::rgb(220, 30, 70))),
            ..Default::default()
        }],
        ..Default::default()
    };
    assert!(crate::text::compose(&run).iter().any(|line| {
        let mut clusters = std::collections::HashSet::new();
        line.glyphs.iter().any(|g| !clusters.insert(g.cluster))
    }));
    run.contours = crate::text::shape(&run);
    let mut shape = Shape::new(
        Geom::Text(run),
        Style {
            fill: Fill::Solid(Rgba::rgb(20, 60, 200)),
            stroke: None,
        },
    );
    shape.rotation = 0.25;
    let mut layer = Layer::vector("Whole-object rotated range outlines");
    layer.kind.shapes_mut().unwrap().push(shape);
    doc.layers.push(layer);
    let (_, notes) = write(&doc).unwrap();
    assert!(notes.iter().any(|s| s.contains("multi-glyph")), "{notes:?}");
    inspect(&doc, "rotated-outlined-range", 1.5);
}

#[test]
#[ignore = "independent PDF pattern qualification requires qpdf and Poppler"]
fn pdf_adapter_pattern_real_output() {
    let (mut pattern, notes) =
        crate::formats::svg::read(include_str!("fixtures/pattern.svg"), "Vector pattern").unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    backdrop(&mut pattern);
    let (pdf, _) = inspect(&pattern, "pattern", 2.5);
    assert!(
        pdf.objects
            .values()
            .filter_map(|o| o.as_stream().ok())
            .any(|s| s.dict.get(b"PatternType").and_then(Object::as_i64).ok() == Some(1))
    );
    assert!(
        !pdf.objects
            .values()
            .filter_map(|o| o.as_stream().ok())
            .any(|s| s.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image"))
    );
}

#[test]
#[ignore = "independent PDF type qualification requires qpdf and Poppler"]
fn pdf_adapter_mixed_type_real_output() {
    let (pdf, _) = inspect(&native_text(), "native-text", 2.);
    let fonts = pdf
        .objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .filter(|d| d.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"CIDFontType2"))
        .count();
    assert_eq!(fonts, 2);
    let modes = pdf
        .get_pages()
        .values()
        .flat_map(|id| {
            Content::decode(&pdf.get_page_content(*id))
                .unwrap()
                .operations
        })
        .filter(|o| o.operator == "Tr")
        .map(|o| o.operands[0].as_float().unwrap())
        .collect::<Vec<_>>();
    assert!(modes.contains(&0.) && modes.contains(&7.), "{modes:?}");
    assert!(!modes.contains(&1.) && !modes.contains(&2.), "{modes:?}");
    let (_, notes) = write(&native_text()).unwrap();
    assert!(
        notes
            .iter()
            .any(|s| s.contains("stroke as a vector outline"))
    );
}
#[test]
#[ignore = "independent retained filter qualification requires qpdf and Poppler"]
fn pdf_adapter_edited_filter_graph_masks_and_offset_artboard_real_output() {
    for source_mask in [false, true] {
        let doc = filtered(source_mask);
        let (_, notes) = write(&doc).unwrap();
        assert!(
            notes
                .iter()
                .any(|s| s.contains("Edited filter graph") && s.contains("RGBA8")),
            "{notes:?}"
        );
        inspect(
            &doc,
            if source_mask {
                "filtered-source"
            } else {
                "filtered-target"
            },
            0.5,
        );
    }
}

#[test]
#[ignore = "independent PDF source type qualification requires qpdf and Poppler"]
fn pdf_adapter_source_text_keeps_actual_range_paints() {
    let (mut doc, notes) = crate::formats::svg::read(
        include_str!("fixtures/source-text.svg"),
        "Positioned path vertical source text",
    )
    .unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    backdrop(&mut doc);
    let (_, notes) = write(&doc).unwrap();
    assert!(notes.iter().any(|s| s.contains("outlines")), "{notes:?}");
    inspect(&doc, "source-text", 2.);
}

#[test]
fn pdf_adapter_rejects_invalid_graphs_without_dropping_the_effect() {
    let mut doc = filtered(false);
    let index = doc
        .layers
        .iter()
        .position(|l| l.svg_filter.is_some())
        .unwrap();
    let saved = doc.layers[index].svg_filter.clone();
    doc.layers[index].svg_filter.as_mut().unwrap().transform = [0.; 6];
    assert!(write(&doc).unwrap_err().contains("Edited filter graph"));
    doc.layers[index].svg_filter = saved;
    doc.layers[index].svg_filter.as_mut().unwrap().filters = vec!["missing-effect".into()];
    assert!(write(&doc).unwrap_err().contains("missing-effect"));
}

#[test]
#[ignore = "independent nested vector PDF qualification requires qpdf and Poppler"]
fn pdf_adapter_native_frames_keep_nested_rotation_clips_and_group_opacity_vector() {
    let mut doc = Document::new("Vector frame hierarchy", 180., 140., 72.);
    doc.layers = vec![Layer::vector("Nested vector frames")];
    let mut outer = rect(Pt::new(15., 15.), Pt::new(145., 105.), Fill::None);
    outer.layout = crate::layout::FrameLayout::frame();
    outer.rotation = 0.12;
    outer.opacity = 0.65;
    let mut inner = Shape::new(
        Geom::Ellipse {
            center: Pt::new(90., 65.),
            radii: Pt::new(55., 35.),
        },
        Style {
            fill: Fill::None,
            stroke: None,
        },
    );
    inner.layout = crate::layout::FrameLayout::frame();
    inner.layout.parent = Some(outer.id);
    inner.rotation = -0.19;
    let mut first = rect(
        Pt::new(25., 25.),
        Pt::new(120., 75.),
        Fill::Linear {
            from: [0., 0.],
            to: [1., 0.8],
            c0: Rgba::rgb(220, 25, 60),
            c1: Rgba::rgb(20, 135, 225),
        },
    );
    first.layout.parent = Some(inner.id);
    let mut second = rect(
        Pt::new(75., 55.),
        Pt::new(70., 55.),
        Fill::Solid(Rgba::new(10, 175, 70, 180)),
    );
    second.layout.parent = Some(inner.id);
    second.blend = Blend::Multiply;
    *doc.layers[0].kind.shapes_mut().unwrap() = vec![outer, inner, first, second];
    backdrop(&mut doc);
    let (pdf, _) = inspect(&doc, "nested-vector-frames", 0.6);
    assert!(
        !pdf.objects
            .values()
            .filter_map(|o| o.as_stream().ok())
            .any(|s| s.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image"))
    );
}

#[test]
#[ignore = "independent frame mask-source PDF qualification requires qpdf and Poppler"]
fn pdf_adapter_frame_artwork_mask_sources_preserve_children_as_vector() {
    let mut doc = Document::new("Vector frame source", 160., 120., 72.);
    doc.layers = vec![Layer::vector("Mask source hierarchy")];
    let mut source = rect(Pt::new(25., 20.), Pt::new(95., 75.), Fill::None);
    source.layout = crate::layout::FrameLayout::frame();
    source.layout.clip = false;
    source.rotation = 0.15;
    let mut child = Shape::new(
        Geom::Ellipse {
            center: Pt::new(72., 58.),
            radii: Pt::new(35., 26.),
        },
        Style {
            fill: Fill::Solid(Rgba::WHITE),
            stroke: None,
        },
    );
    child.layout.parent = Some(source.id);
    let mut target = rect(
        Pt::new(15., 15.),
        Pt::new(130., 90.),
        Fill::Linear {
            from: [0., 0.],
            to: [1., 0.8],
            c0: Rgba::rgb(230, 40, 70),
            c1: Rgba::rgb(20, 125, 210),
        },
    );
    target.artwork_masks.push(ArtworkMask::new(MaskSource {
        layer: doc.layers[0].id,
        shape: Some(source.id),
    }));
    *doc.layers[0].kind.shapes_mut().unwrap() = vec![source, child, target];
    backdrop(&mut doc);
    let (pdf, _) = inspect(&doc, "frame-vector-mask-source", 0.6);
    assert!(
        !pdf.objects
            .values()
            .filter_map(|o| o.as_stream().ok())
            .any(|s| s.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image"))
    );
}
