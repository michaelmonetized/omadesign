use super::*;
use crate::artwork_mask::{ArtworkMask, MaskMode, MaskSource};
use crate::gradient::{Gradient, GradientKind, GradientStop};

fn rectangle(origin: Pt, size: Pt, fill: Fill) -> Shape {
    Shape::new(
        Geom::Rect {
            origin,
            size,
            radius: 0.,
        },
        Style { fill, stroke: None },
    )
}
fn vector_document() -> Document {
    let mut doc = Document::new("Vector exchange", 200., 120., 72.);
    doc.layers.clear();
    let mut background = Layer::vector("Backdrop");
    background.kind.shapes_mut().unwrap().push(rectangle(
        Pt::ZERO,
        Pt::new(200., 120.),
        Fill::Solid(Rgba::WHITE),
    ));
    let mut source = Layer::vector("Editable vector mask source");
    source.kind.shapes_mut().unwrap().push(rectangle(
        Pt::new(12., 12.),
        Pt::new(74., 80.),
        Fill::Solid(Rgba::BLACK),
    ));
    let mut group = Layer::group("Isolated translucent group");
    group.pass_through = false;
    group.opacity = 0.7;
    group.artwork_masks.push(ArtworkMask::new(MaskSource {
        layer: source.id,
        shape: None,
    }));
    group.artwork_masks[0].region = Some(crate::geom::Bounds::from_min_size(
        Pt::new(16., 16.),
        Pt::new(66., 70.),
    ));
    let mut layer = Layer::vector("Vector gradients");
    layer.parent = Some(group.id);
    let mut gradient = Gradient::new(
        GradientKind::Linear,
        Rgba::rgb(255, 0, 0),
        Rgba::rgb(0, 0, 255),
    );
    gradient.stops.insert(
        1,
        GradientStop {
            offset: 0.4,
            color: Rgba::new(0, 220, 30, 130),
        },
    );
    layer.kind.shapes_mut().unwrap().push(rectangle(
        Pt::new(8., 8.),
        Pt::new(82., 90.),
        Fill::Gradient(gradient),
    ));
    let mut radial = Gradient::new(
        GradientKind::Radial,
        Rgba::rgb(20, 240, 250),
        Rgba::rgb(210, 20, 120),
    );
    radial.to = [0.9, 0.5];
    let mut shape = rectangle(
        Pt::new(105., 12.),
        Pt::new(75., 75.),
        Fill::Gradient(radial),
    );
    shape.rotation = 0.12;
    layer.kind.shapes_mut().unwrap().push(shape);
    let mut radial_layer = Layer::vector("Radial artwork");
    radial_layer
        .kind
        .shapes_mut()
        .unwrap()
        .push(layer.kind.shapes_mut().unwrap().pop().unwrap());
    let mut hidden = Layer::vector("Hidden locked layer");
    hidden.visible = false;
    hidden.locked = true;
    hidden.kind.shapes_mut().unwrap().push(rectangle(
        Pt::ZERO,
        Pt::new(200., 120.),
        Fill::Solid(Rgba::BLACK),
    ));
    doc.layers = vec![background, source, layer, group, radial_layer, hidden];
    doc
}
fn subtype(object: &Object, name: &[u8]) -> bool {
    object
        .as_stream()
        .ok()
        .is_some_and(|s| s.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(name))
}

#[test]
fn pdf_exchange_keeps_vector_shadings_masks_clips_and_optional_layers() {
    let doc = vector_document();
    let original = serde_json::to_string(&doc).unwrap();
    let (bytes, notes) = write(&doc).unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    let pdf = Pdf::load_mem(&bytes).unwrap();
    assert!(
        !pdf.objects.values().any(|o| subtype(o, b"Image")),
        "vector paint or masks were rasterized"
    );
    assert!(
        pdf.objects
            .values()
            .filter_map(|o| o.as_dict().ok())
            .any(|d| d.get(b"ShadingType").and_then(Object::as_i64).ok() == Some(2))
    );
    assert!(
        pdf.objects
            .values()
            .filter_map(|o| o.as_dict().ok())
            .any(|d| d.get(b"ShadingType").and_then(Object::as_i64).ok() == Some(3))
    );
    assert!(
        pdf.objects
            .values()
            .filter_map(|o| o.as_dict().ok())
            .any(|d| d.get(b"SMask").is_ok())
    );
    assert!(
        pdf.objects
            .values()
            .filter_map(|o| o.as_stream().ok())
            .any(|s| decoded(s, STREAM_LIMIT)
                .ok()
                .and_then(|b| Content::decode(&b).ok())
                .is_some_and(|c| c.operations.iter().any(|o| o.operator == "W")))
    );
    let catalog = pdf.catalog().unwrap();
    let oc = dictionary(&pdf, catalog.get(b"OCProperties").unwrap()).unwrap();
    let defaults = dictionary(&pdf, oc.get(b"D").unwrap()).unwrap();
    assert_eq!(defaults.get(b"OFF").unwrap().as_array().unwrap().len(), 1);
    assert_eq!(
        defaults.get(b"Locked").unwrap().as_array().unwrap().len(),
        1
    );
    assert_eq!(
        serde_json::to_string(&doc).unwrap(),
        original,
        "export mutated source masks"
    );
    let (loaded, notes) = read(&bytes, "vectors.pdf").unwrap();
    assert!(
        notes
            .iter()
            .all(|n| !n.contains("unsupported") && !n.contains("could not")),
        "{notes:?}"
    );
    assert!(
        loaded
            .layers
            .iter()
            .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
            .any(|s| s
                .style
                .fill
                .gradient()
                .is_some_and(|g| g.kind == GradientKind::Radial))
    );
}

#[test]
#[ignore = "independent PDF rendering: requires Poppler utilities"]
fn pdf_exchange_poppler_matches_vector_masks_gradients_and_native_reopen() {
    let folder = std::env::temp_dir().join(format!(
        "pdf-vector-exchange-{}",
        crate::document::next_id()
    ));
    std::fs::create_dir_all(&folder).unwrap();
    let doc = vector_document();
    let native = folder.join("source.oma");
    crate::project::save_to(&doc, &native).unwrap();
    let reopened = crate::project::load_from(&native).unwrap();
    assert_eq!(reopened.mask_sources().len(), 1);
    let expected = crate::compositor::render_export(&reopened, 1).unwrap();
    let (bytes, notes) = write(&reopened).unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    let pdf = folder.join("vectors.pdf");
    std::fs::write(&pdf, &bytes).unwrap();
    let images = std::process::Command::new("pdfimages")
        .arg("-list")
        .arg(&pdf)
        .output()
        .unwrap();
    assert!(
        images.status.success(),
        "{}",
        String::from_utf8_lossy(&images.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&images.stdout).lines().count(),
        2,
        "{}",
        String::from_utf8_lossy(&images.stdout)
    );
    let output = std::process::Command::new("pdftocairo")
        .args(["-png", "-r", "72", "-singlefile"])
        .arg(&pdf)
        .arg(folder.join("poppler"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "PDF renderer diagnostics: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = image::open(folder.join("poppler.png")).unwrap().to_rgb8();
    assert_eq!(actual.dimensions(), (200, 120));
    let difference = expected
        .pixels()
        .iter()
        .zip(actual.pixels())
        .map(|(a, b)| {
            let a = a.demultiply();
            u64::from(a.red().abs_diff(b.0[0]))
                + u64::from(a.green().abs_diff(b.0[1]))
                + u64::from(a.blue().abs_diff(b.0[2]))
        })
        .sum::<u64>() as f64
        / (200. * 120. * 3.);
    expected.save_png(folder.join("native.png")).unwrap();
    let samples = [24, 40, 62, 76].map(|x| {
        let a = expected.pixel(x, 40).unwrap().demultiply();
        let b = actual.get_pixel(x, 40).0;
        ([a.red(), a.green(), a.blue()], b)
    });
    eprintln!(
        "vector exchange: {} bytes, 0 images, Poppler mean absolute channel difference {difference:.4}/255, alpha gradient samples {samples:?}",
        bytes.len()
    );
    if let Some(receipts) = std::env::var_os("OMA_PDF_EXCHANGE_RECEIPTS") {
        let receipts = std::path::PathBuf::from(receipts);
        std::fs::create_dir_all(&receipts).unwrap();
        for name in ["vectors.pdf", "source.oma", "native.png", "poppler.png"] {
            std::fs::copy(folder.join(name), receipts.join(name)).unwrap();
        }
    }
    for (native, pdf) in samples {
        assert!(
            native.iter().zip(pdf).all(|(a, b)| a.abs_diff(b) <= 3),
            "PDF gradient color/alpha interpolation differs: native {native:?}, PDF {pdf:?}"
        );
    }
    assert!(
        difference < 2.5,
        "vector PDF appearance differs by {difference}/255; evidence in {}",
        folder.display()
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn pdf_exchange_pixel_masks_keep_gradient_strokes_as_vector_artwork() {
    let mut doc = Document::new("Pixel mask with vector stroke", 80., 60., 72.);
    doc.layers.clear();
    let mut layer = Layer::vector("Masked gradient stroke");
    let gradient = Gradient::new(
        GradientKind::Linear,
        Rgba::rgb(255, 10, 20),
        Rgba::rgb(20, 30, 255),
    );
    let mut shape = rectangle(Pt::new(15., 15.), Pt::new(45., 30.), Fill::None);
    shape.style.stroke = Some(Stroke {
        gradient: Some(gradient),
        width: 6.,
        ..Default::default()
    });
    shape.mask = Pixels::from_rgba(2, 1, vec![255, 255, 255, 255, 128, 128, 128, 255]);
    shape.rotation = 0.1;
    layer.kind.shapes_mut().unwrap().push(shape);
    doc.layers.push(layer);
    let (bytes, notes) = write(&doc).unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    let pdf = Pdf::load_mem(&bytes).unwrap();
    let images = pdf
        .objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .filter(|s| s.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image"))
        .collect::<Vec<_>>();
    assert_eq!(
        images.len(),
        1,
        "only the native pixel mask may be an image"
    );
    assert_eq!(
        images[0]
            .dict
            .get(b"ColorSpace")
            .unwrap()
            .as_name()
            .unwrap(),
        b"DeviceGray"
    );
    let (loaded, notes) = read(&bytes, "gradient-stroke.pdf").unwrap();
    assert!(notes.iter().all(|n| !n.contains("could not")), "{notes:?}");
    assert!(
        loaded
            .layers
            .iter()
            .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
            .any(|s| s.style.fill.gradient().is_some())
    );
}

#[test]
fn pdf_exchange_mask_sources_survive_effect_fallback_and_shape_references() {
    for effect_on_source in [false, true] {
        for shape_source in [false, true] {
            let mut doc = vector_document();
            if shape_source {
                doc.layers[3].artwork_masks[0].source.shape =
                    Some(doc.layers[1].kind.shapes().unwrap()[0].id);
            }
            let index = if effect_on_source { 1 } else { 3 };
            doc.layers[index]
                .filters
                .items
                .push(crate::filter::Fx::Blur { std: 1.5 });
            let expected = crate::compositor::render_export(&doc, 1).unwrap();
            let (bytes, notes) = write(&doc).unwrap();
            assert!(
                notes.iter().any(|n| n.contains("unsupported effects")),
                "{notes:?}"
            );
            let (loaded, _) = read(&bytes, "effect-mask.pdf").unwrap();
            let actual = crate::compositor::render_export(&loaded, 1).unwrap();
            let difference = expected
                .data()
                .iter()
                .zip(actual.data())
                .map(|(a, b)| u64::from(a.abs_diff(*b)))
                .sum::<u64>() as f64
                / expected.data().len() as f64;
            assert!(
                difference < 1.,
                "source effect={effect_on_source}, shape source={shape_source}: mask lost or changed by {difference}/255"
            );
        }
    }
}

#[test]
fn pdf_exchange_group_and_luminance_sources_keep_native_visibility() {
    for visible in [true, false] {
        let mut doc = vector_document();
        doc.layers[1].kind.shapes_mut().unwrap()[0].fill_opacity = 0.5;
        let mut source_group = Layer::group("Mask source group");
        source_group.visible = visible;
        doc.layers[1].parent = Some(source_group.id);
        doc.layers[1].kind.shapes_mut().unwrap()[0].style.fill = Fill::Solid(Rgba::WHITE);
        doc.layers[3].artwork_masks[0].source = MaskSource {
            layer: source_group.id,
            shape: None,
        };
        doc.layers[3].artwork_masks[0].mode = MaskMode::Luminance;
        doc.layers.push(source_group);
        let expected = crate::compositor::render_export(&doc, 1).unwrap();
        let (bytes, notes) = write(&doc).unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        let (loaded, _) = read(&bytes, "group-mask.pdf").unwrap();
        let actual = crate::compositor::render_export(&loaded, 1).unwrap();
        let a = expected.pixel(40, 40).unwrap().demultiply();
        let b = actual.pixel(40, 40).unwrap().demultiply();
        assert!(
            a.red().abs_diff(b.red()) < 3
                && a.green().abs_diff(b.green()) < 3
                && a.blue().abs_diff(b.blue()) < 3,
            "mask source visible={visible}: {a:?} != {b:?}"
        );
    }
}

#[test]
#[ignore = "independent nonisolated PDF blending: requires Poppler Splash and Cairo"]
fn pdf_exchange_poppler_preserves_pixel_masked_pass_through_blends() {
    for pass_through in [true, false] {
        let mut doc = Document::new("Masked pass through", 80., 60., 72.);
        doc.layers.clear();
        let mut backdrop = Layer::vector("Backdrop");
        backdrop.kind.shapes_mut().unwrap().push(rectangle(
            Pt::ZERO,
            Pt::new(80., 60.),
            Fill::Solid(Rgba::rgb(80, 170, 220)),
        ));
        let mut group = Layer::group("Masked group");
        group.pass_through = pass_through;
        group.opacity = 0.7;
        group.mask = Pixels::from_rgba(
            3,
            1,
            vec![0, 0, 0, 255, 128, 128, 128, 255, 255, 255, 255, 255],
        );
        group.mask_origin = Pt::new(10., 10.);
        group.mask_size = Pt::new(60., 40.);
        let mut child = Layer::vector("Multiply child");
        child.parent = Some(group.id);
        child.blend = Blend::Multiply;
        child.kind.shapes_mut().unwrap().push(rectangle(
            Pt::new(10., 10.),
            Pt::new(60., 40.),
            Fill::Solid(Rgba::rgb(220, 70, 30)),
        ));
        doc.layers = vec![backdrop, child, group];
        let expected = crate::compositor::render_export(&doc, 1).unwrap();
        let (bytes, notes) = write(&doc).unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        let folder =
            std::env::temp_dir().join(format!("pdf-pass-through-{}", crate::document::next_id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("group.pdf");
        std::fs::write(&path, &bytes).unwrap();
        expected.save_png(folder.join("native.png")).unwrap();
        for (engine, name) in [("pdftoppm", "splash"), ("pdftocairo", "cairo")] {
            let output = std::process::Command::new(engine)
                .args(["-png", "-r", "72", "-singlefile"])
                .arg(&path)
                .arg(folder.join(name))
                .output()
                .unwrap();
            assert!(output.status.success());
            assert!(
                output.stderr.is_empty(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let splash = image::open(folder.join("splash.png")).unwrap().to_rgb8();
        let cairo = image::open(folder.join("cairo.png")).unwrap().to_rgb8();
        let points=[20,40,60].map(|x|{let a=expected.pixel(x,30).unwrap().demultiply();serde_json::json!({"x":x,"native":[a.red(),a.green(),a.blue()],"splash":splash.get_pixel(x,30).0,"cairo":cairo.get_pixel(x,30).0})});
        eprintln!("masked group pass_through={pass_through}: {points:?}");
        if let Some(receipts) = std::env::var_os("OMA_PDF_EXCHANGE_RECEIPTS") {
            let receipts = std::path::PathBuf::from(receipts).join(if pass_through {
                "pass-through"
            } else {
                "isolated-control"
            });
            std::fs::create_dir_all(&receipts).unwrap();
            for name in ["group.pdf", "native.png", "splash.png", "cairo.png"] {
                std::fs::copy(folder.join(name), receipts.join(name)).unwrap();
            }
            std::fs::write(
                receipts.join("samples.json"),
                serde_json::to_vec_pretty(&points).unwrap(),
            )
            .unwrap();
        }
        for x in [20, 40, 60] {
            let a = expected.pixel(x, 30).unwrap().demultiply();
            let b = splash.get_pixel(x, 30).0;
            assert!(
                a.red().abs_diff(b[0]) < 3
                    && a.green().abs_diff(b[1]) < 3
                    && a.blue().abs_diff(b[2]) < 3,
                "Splash masked group pass_through={pass_through}: {a:?} != {b:?}"
            );
            if !pass_through {
                let b = cairo.get_pixel(x, 30).0;
                assert!(
                    a.red().abs_diff(b[0]) < 3
                        && a.green().abs_diff(b[1]) < 3
                        && a.blue().abs_diff(b[2]) < 3,
                    "Cairo isolated control: {a:?} != {b:?}"
                );
            }
        }
        std::fs::remove_dir_all(folder).unwrap();
    }
}

#[test]
fn pdf_exchange_plain_vectors_avoid_per_object_transparency_forms() {
    let mut doc = Document::new("Plain vectors", 100., 80., 72.);
    doc.layers.clear();
    let mut layer = Layer::vector("Plain");
    for x in [10., 50.] {
        layer.kind.shapes_mut().unwrap().push(rectangle(
            Pt::new(x, 10.),
            Pt::new(25., 40.),
            Fill::Solid(Rgba::rgb(20, 80, 160)),
        ));
    }
    doc.layers.push(layer);
    let (bytes, notes) = write(&doc).unwrap();
    assert!(notes.is_empty());
    let pdf = Pdf::load_mem(&bytes).unwrap();
    assert!(
        !pdf.objects.values().any(|o| subtype(o, b"Form")),
        "ordinary vectors gained unnecessary page-sized transparency forms"
    );
    let (imported, notes) = read(&bytes, "plain.pdf").unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(
        crate::compositor::render_export(&doc, 1).unwrap().data(),
        crate::compositor::render_export(&imported, 1)
            .unwrap()
            .data()
    );
}

#[test]
fn pdf_exchange_applies_fill_opacity_once_to_overlapping_fill_and_stroke() {
    let mut doc = Document::new("Fill opacity", 80., 60., 72.);
    doc.layers.clear();
    let mut layer = Layer::vector("Translucent object");
    let mut shape = rectangle(
        Pt::new(15., 15.),
        Pt::new(40., 30.),
        Fill::Solid(Rgba::rgb(255, 0, 0)),
    );
    shape.fill_opacity = 0.5;
    shape.style.stroke = Some(Stroke {
        color: Rgba::rgb(0, 0, 255),
        width: 8.,
        ..Default::default()
    });
    layer.kind.shapes_mut().unwrap().push(shape);
    doc.layers.push(layer);
    let expected = crate::compositor::render_export(&doc, 1).unwrap();
    let (bytes, notes) = write(&doc).unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    let (loaded, _) = read(&bytes, "fill-opacity.pdf").unwrap();
    let actual = crate::compositor::render_export(&loaded, 1).unwrap();
    for (x, y) in [(16, 25), (35, 25)] {
        let a = expected.pixel(x, y).unwrap().demultiply();
        let b = actual.pixel(x, y).unwrap().demultiply();
        assert!(
            a.red().abs_diff(b.red()) < 2
                && a.green().abs_diff(b.green()) < 2
                && a.blue().abs_diff(b.blue()) < 2,
            "fill/stroke opacity applied twice at{x},{y}: {a:?}!={b:?}"
        );
    }
}
