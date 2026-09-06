use super::*;
use crate::document::LayerKind;
use lopdf::dictionary;

fn fixture(contents: &[&[u8]], resources: Dictionary, extra: Dictionary) -> Vec<u8> {
    let mut pdf = Pdf::with_version("1.7");
    let pages = pdf.new_object_id();
    let mut children = Vec::new();
    for content in contents {
        let stream = pdf.add_object(Stream::new(Dictionary::new(), content.to_vec()));
        let page = pdf.add_object(dictionary! {
            "Type"=>"Page", "Parent"=>pages, "Contents"=>stream,
        });
        children.push(Object::Reference(page));
    }
    pdf.objects.insert(
        pages,
        Object::Dictionary(dictionary! {
            "Type"=>"Pages", "Kids"=>children, "Count"=>contents.len() as i64,
            "MediaBox"=>vec![0.into(),0.into(),200.into(),100.into()], "Resources"=>resources,
        }),
    );
    let mut catalog = dictionary! {"Type"=>"Catalog","Pages"=>pages};
    for (key, value) in extra.iter() {
        catalog.set(key.clone(), value.clone());
    }
    let catalog = pdf.add_object(catalog);
    pdf.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    pdf.save_to(&mut bytes).unwrap();
    bytes
}

fn shapes(document: &Document) -> Vec<&Shape> {
    document
        .layers
        .iter()
        .filter_map(|layer| layer.kind.shapes())
        .flatten()
        .collect()
}

#[test]
fn reads_all_pages_with_inherited_resources_and_preserves_cubic_handles() {
    let bytes = fixture(
        &[
            b"q 2 0 0 2 5 7 cm 1 0 0 rg 0 0 m 5 15 10 20 20 25 c 30 0 l h f Q",
            b"0 1 0 rg 10 20 30 40 re f",
        ],
        Dictionary::new(),
        Dictionary::new(),
    );
    let (document, warnings) = read(&bytes, "multi.pdf").unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(document.artboards.len(), 2);
    assert_eq!(document.artboards[1].origin, Pt::new(248., 0.));
    assert_eq!((document.width, document.height), (448., 100.));
    let shapes = shapes(&document);
    assert_eq!(shapes.len(), 2);
    let Geom::Path { anchors, closed } = &shapes[0].geom else {
        panic!("Cubic path flattened")
    };
    assert!(*closed);
    assert_eq!(anchors[0].pt, Pt::new(5., 93.));
    assert_eq!(anchors[0].h_out, Pt::new(10., -30.));
    assert_eq!(anchors[1].h_in, Pt::new(-20., 10.));
    assert_eq!(shapes[1].world_bbox().min, Pt::new(258., 40.));
}

#[test]
fn applies_crop_rotation_and_user_unit() {
    let bytes = fixture(&[b"10 20 20 30 re f"], Dictionary::new(), Dictionary::new());
    let mut pdf = Pdf::load_mem(&bytes).unwrap();
    let id = pdf.get_pages()[&1];
    let page = pdf.get_dictionary_mut(id).unwrap();
    page.set(
        "CropBox",
        vec![10.into(), 20.into(), 210.into(), 120.into()],
    );
    page.set("Rotate", 90);
    page.set("UserUnit", 2);
    let mut bytes = Vec::new();
    pdf.save_to(&mut bytes).unwrap();
    let (document, _) = read(&bytes, "rotated.pdf").unwrap();
    assert_eq!(document.artboards[0].size, Pt::new(200., 400.));
    let bounds = shapes(&document)[0].world_bbox();
    assert_eq!(bounds.min, Pt::ZERO);
    assert_eq!(bounds.max, Pt::new(60., 40.));
}

#[test]
fn keeps_named_optional_content_groups_and_hidden_children_editable() {
    let resources = dictionary! {"Properties"=>dictionary! {
        "Outer"=>dictionary! {"Type"=>"OCG","Name"=>lopdf::text_string("Artwork")},
        "Inner"=>dictionary! {"Type"=>"OCG","Name"=>lopdf::text_string("曲線")},
    }};
    let bytes = fixture(
        &[b"/OC /Outer BDC /OC /Inner BDC 1 0 0 rg 0 0 20 20 re f EMC EMC"],
        resources,
        Dictionary::new(),
    );
    let (document, warnings) = read(&bytes, "layers.pdf").unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let inner = document
        .layers
        .iter()
        .find(|layer| layer.name == "曲線")
        .unwrap();
    let outer = document
        .layers
        .iter()
        .find(|layer| layer.name == "Artwork" && layer.is_group)
        .unwrap();
    assert!(inner.is_group);
    assert_eq!(inner.parent, Some(outer.id));
    assert_eq!(document.layers[0].parent, Some(inner.id));
}

#[test]
fn imports_standard_text_as_live_type() {
    let resources = dictionary! {"Font"=>dictionary! {"F1"=>dictionary! {
        "Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica","Encoding"=>"WinAnsiEncoding",
        "FirstChar"=>0,"Widths"=>vec![Object::Integer(500);256],
    }}};
    let bytes = fixture(
        &[b"BT /F1 12 Tf 1 0 0 1 15 60 Tm (Hello) Tj 0 -20 Td (World) Tj ET"],
        resources,
        Dictionary::new(),
    );
    let (document, _) = read(&bytes, "text.pdf").unwrap();
    let shapes = shapes(&document);
    let Geom::Text(run) = &shapes[0].geom else {
        panic!("Text not live")
    };
    assert_eq!(run.content, "Hello");
    assert_eq!(run.origin, Pt::new(15., 40.));
    assert_eq!(run.px, 12.);
    let Geom::Text(run) = &shapes[1].geom else {
        panic!("Text not live")
    };
    assert_eq!(run.content, "World");
    assert_eq!(run.origin, Pt::new(15., 60.));
}

#[test]
fn reports_unsupported_content_and_rejects_legacy_ai_and_recursive_forms() {
    let bytes = fixture(
        &[b"0 0 100 100 re W n /Gradient sh"],
        Dictionary::new(),
        Dictionary::new(),
    );
    let (_, warnings) = read(&bytes, "effects.pdf").unwrap();
    assert!(warnings.iter().any(|warning| warning.contains("shading")));
    assert!(
        read(b"%!PS-Adobe-3.0", "legacy.ai")
            .unwrap_err()
            .contains("legacy PostScript")
    );
    let bytes = fixture(&[b"/Loop Do"], Dictionary::new(), Dictionary::new());
    let mut pdf = Pdf::load_mem(&bytes).unwrap();
    let form = pdf.new_object_id();
    let resources = dictionary! {"XObject"=>dictionary! {"Loop"=>form}};
    pdf.objects.insert(form,Object::Stream(Stream::new(dictionary! {
        "Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),10.into(),10.into()],"Resources"=>resources.clone(),
    },b"/Loop Do".to_vec())));
    let page = pdf.get_pages()[&1];
    pdf.get_dictionary_mut(page)
        .unwrap()
        .set("Resources", resources);
    let mut bytes = Vec::new();
    pdf.save_to(&mut bytes).unwrap();
    assert!(
        read(&bytes, "recursive.pdf")
            .unwrap_err()
            .contains("nesting limit")
    );
}

fn export_document() -> Document {
    let mut document = Document::new("Round trip", 1., 1., 72.);
    document.width = 148.;
    document.height = 80.;
    document.layers.clear();
    document.artboards = vec![
        Artboard::new(0, Pt::ZERO, Pt::new(50., 80.)),
        Artboard::new(1, Pt::new(98., 0.), Pt::new(50., 80.)),
    ];
    let mut group = Layer::group("Hidden group");
    group.visible = false;
    let mut hidden = Layer::vector("Hidden red");
    hidden.parent = Some(group.id);
    hidden.kind.shapes_mut().unwrap().push(Shape::new(
        Geom::Rect {
            origin: Pt::new(5., 5.),
            size: Pt::new(15., 15.),
            radius: 0.,
        },
        Style {
            fill: Fill::Solid(Rgba::rgb(255, 0, 0)),
            stroke: None,
        },
    ));
    let pixels = Pixels::from_rgba(
        2,
        2,
        vec![
            255, 0, 0, 128, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 64,
        ],
    )
    .unwrap();
    let image = Layer::placed_raster("RGBA tile", pixels, Pt::new(10., 30.), Pt::new(20., 20.));
    let mut shape = Layer::vector("Page two");
    shape.kind.shapes_mut().unwrap().push(Shape::new(
        Geom::Rect {
            origin: Pt::new(108., 10.),
            size: Pt::new(20., 20.),
            radius: 0.,
        },
        Style {
            fill: Fill::Solid(Rgba::rgb(0, 0, 255)),
            stroke: None,
        },
    ));
    document.layers = vec![hidden, group, image, shape];
    document
}

#[test]
fn layered_pdf_round_trip_preserves_pages_hidden_groups_and_image_alpha() {
    let original = export_document();
    let (bytes, warnings) = write(&original).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let (loaded, warnings) = read(&bytes, "roundtrip.pdf").unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(loaded.artboards.len(), 2);
    let hidden = loaded
        .layers
        .iter()
        .position(|layer| layer.name == "Hidden group")
        .unwrap();
    assert!(!loaded.layer_visible(hidden));
    let pixel_layer = loaded
        .layers
        .iter()
        .find(|layer| matches!(layer.kind, LayerKind::Raster { .. }))
        .unwrap();
    assert_eq!(
        pixel_layer.kind.pixels().unwrap().data,
        original.layers[2].kind.pixels().unwrap().data
    );
    assert_eq!(
        pixel_layer.kind.raster_xform().unwrap(),
        (Pt::new(10., 30.), Pt::new(20., 20.), 0.)
    );
    let page_two = loaded
        .layers
        .iter()
        .flat_map(|layer| layer.kind.shapes().unwrap_or(&[]))
        .find(|shape| shape.world_bbox().min.x > 98.)
        .unwrap();
    assert_eq!(page_two.world_bbox().min, Pt::new(108., 10.));
}

#[test]
fn imports_clipped_axial_shading_with_a_cropped_luminosity_mask() {
    let bytes = fixture(
        &[b"20 10 40 20 re W n /Alpha gs /Gradient sh"],
        Dictionary::new(),
        Dictionary::new(),
    );
    let mut pdf = Pdf::load_mem(&bytes).unwrap();
    let mask=pdf.add_object(Stream::new(dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),200.into(),100.into()],"Resources"=>Dictionary::new()},b"0.5 g 0 0 200 100 re f".to_vec()));
    let resources = dictionary! {
        "ExtGState"=>dictionary! {"Alpha"=>dictionary! {"SMask"=>dictionary! {"S"=>"Luminosity","G"=>mask}}},
        "Shading"=>dictionary! {"Gradient"=>dictionary! {"ShadingType"=>2,"ColorSpace"=>"DeviceRGB","Coords"=>vec![0.into(),0.into(),100.into(),0.into()],"Extend"=>vec![true.into(),true.into()],"Function"=>dictionary! {"FunctionType"=>2,"N"=>1,"C0"=>vec![1.into(),0.into(),0.into()],"C1"=>vec![0.into(),0.into(),1.into()]}}},
    };
    let page = pdf.get_pages()[&1];
    pdf.get_dictionary_mut(page)
        .unwrap()
        .set("Resources", resources);
    let mut bytes = Vec::new();
    pdf.save_to(&mut bytes).unwrap();
    let (document, warnings) = read(&bytes, "masked-gradient.pdf").unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let layer = document
        .layers
        .iter()
        .find(|layer| !layer.is_group)
        .unwrap();
    assert_eq!(layer.mask_origin, Pt::new(20., 70.));
    assert_eq!(layer.mask_size, Pt::new(40., 20.));
    assert!(matches!(
        layer.kind.shapes().unwrap()[0].style.fill,
        Fill::Linear { .. }
    ));
    let pm = crate::compositor::render_export(&document, 1).unwrap();
    let covered = pm.pixel(40, 80).unwrap().demultiply();
    assert!(
        (123..=131).contains(&covered.green()),
        "Mask alpha incorrect: {covered:?}"
    );
    assert!(
        (198..=210).contains(&covered.red()),
        "Gradient position incorrect: {covered:?}"
    );
    assert_eq!(pm.pixel(10, 80).unwrap().demultiply().green(), 255);
}

#[test]
fn transparency_form_keeps_outer_soft_mask_when_inner_state_resets_it() {
    let bytes = fixture(
        &[b"/Alpha gs /Paint Do"],
        Dictionary::new(),
        Dictionary::new(),
    );
    let mut pdf = Pdf::load_mem(&bytes).unwrap();
    let bbox = vec![
        Object::Integer(0),
        Object::Integer(0),
        Object::Integer(200),
        Object::Integer(100),
    ];
    let mask=pdf.add_object(Stream::new(dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>bbox.clone(),"Resources"=>Dictionary::new()},b"0.5 g 0 0 200 100 re f".to_vec()));
    let paint=pdf.add_object(Stream::new(dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>bbox,"Group"=>dictionary! {"S"=>"Transparency","I"=>true},"Resources"=>dictionary! {"ExtGState"=>dictionary! {"Reset"=>dictionary! {"SMask"=>"None","ca"=>1}}}},b"/Reset gs 1 0 0 rg 0 0 200 100 re f".to_vec()));
    let resources = dictionary! {"XObject"=>dictionary! {"Paint"=>paint},"ExtGState"=>dictionary! {"Alpha"=>dictionary! {"SMask"=>dictionary! {"S"=>"Luminosity","G"=>mask}}}};
    let page = pdf.get_pages()[&1];
    pdf.get_dictionary_mut(page)
        .unwrap()
        .set("Resources", resources);
    let mut bytes = Vec::new();
    pdf.save_to(&mut bytes).unwrap();
    let (document, warnings) = read(&bytes, "form-mask.pdf").unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(
        document
            .layers
            .iter()
            .any(|layer| layer.is_group && layer.mask.is_some())
    );
    let pm = crate::compositor::render_export(&document, 1).unwrap();
    let color = pm.pixel(100, 50).unwrap().demultiply();
    assert_eq!(color.red(), 255);
    assert!(
        (124..=130).contains(&color.green()),
        "Outer mask lost: {color:?}"
    );
}

fn effects_document() -> Document {
    let mut document = Document::new("PDF effects", 160., 100., 72.);
    document.layers.clear();
    let mut backdrop = Layer::vector("Editable backdrop");
    backdrop.kind.shapes_mut().unwrap().push(Shape::new(
        Geom::Rect {
            origin: Pt::new(10., 10.),
            size: Pt::new(140., 80.),
            radius: 0.,
        },
        Style {
            fill: Fill::Solid(Rgba::rgb(40, 90, 170)),
            stroke: None,
        },
    ));
    let mut group = Layer::group("Masked gradient group");
    group.pass_through = false;
    group.opacity = 0.65;
    group.mask = Pixels::from_rgba(2, 1, vec![255, 255, 255, 255, 90, 90, 90, 255]);
    group.mask_origin = Pt::new(30., 20.);
    group.mask_size = Pt::new(100., 60.);
    let mut artwork = Layer::vector("Gradient and shadow");
    artwork.parent = Some(group.id);
    let mut shape = Shape::new(
        Geom::Rect {
            origin: Pt::new(35., 25.),
            size: Pt::new(75., 40.),
            radius: 8.,
        },
        Style {
            fill: Fill::Linear {
                from: [0., 0.],
                to: [1., 1.],
                c0: Rgba::rgb(250, 30, 20),
                c1: Rgba::rgb(30, 240, 170),
            },
            stroke: None,
        },
    );
    shape.filters.items.push(crate::filter::Fx::Shadow {
        dx: 5.,
        dy: 4.,
        blur: 3.,
        color: Rgba::new(0, 0, 0, 150),
    });
    artwork.kind.shapes_mut().unwrap().push(shape);
    let mut hidden = Layer::vector("Hidden gradient");
    hidden.visible = false;
    hidden.kind.shapes_mut().unwrap().push(Shape::new(
        Geom::Rect {
            origin: Pt::ZERO,
            size: Pt::new(160., 100.),
            radius: 0.,
        },
        Style {
            fill: Fill::Radial {
                c0: Rgba::rgb(255, 0, 255),
                c1: Rgba::rgb(0, 0, 0),
            },
            stroke: None,
        },
    ));
    document.layers = vec![backdrop, artwork, group, hidden];
    document
}

#[test]
fn pdf_effect_fallback_preserves_unaffected_vectors_and_hidden_layer_state() {
    let (bytes, notes) = write(&effects_document()).unwrap();
    assert!(
        notes
            .iter()
            .any(|note| note.contains("complex group appearance"))
    );
    let (loaded, _) = read(&bytes, "effects.pdf").unwrap();
    assert!(
        shapes(&loaded)
            .iter()
            .any(|shape| shape.style.fill == Fill::Solid(Rgba::rgb(40, 90, 170)))
    );
    assert!(
        loaded
            .layers
            .iter()
            .any(|layer| matches!(layer.kind, LayerKind::Raster { .. }))
    );
    let hidden = loaded
        .layers
        .iter()
        .position(|layer| layer.name == "Hidden gradient")
        .unwrap();
    assert!(!loaded.layer_visible(hidden));
    assert!(
        loaded
            .layers
            .iter()
            .any(|layer| layer.name == "Masked gradient group")
    );
}

#[test]
#[ignore = "external effect appearance check: requires Poppler pdftocairo"]
fn poppler_matches_masked_gradient_effects_and_backdrop_dependent_fallbacks() {
    let temp = std::env::temp_dir().join(format!(
        "omadesign-pdf-effects-{}-{}",
        std::process::id(),
        crate::document::next_id()
    ));
    std::fs::create_dir(&temp).unwrap();
    for (index, pass_through) in [false, true].into_iter().enumerate() {
        let mut document = effects_document();
        if pass_through {
            document.layers[2].pass_through = true;
            document.layers[1].blend = Blend::Multiply;
        }
        let expected = crate::compositor::render_export(&document, 1).unwrap();
        expected
            .save_png(temp.join(format!("expected-{index}.png")))
            .unwrap();
        let (bytes, notes) = write(&document).unwrap();
        if pass_through {
            assert!(notes.iter().any(|note| note.contains("backdrop-dependent")));
            let pdf = Pdf::load_mem(&bytes).unwrap();
            let embedded = pdf
                .objects
                .values()
                .filter_map(|object| object.as_stream().ok())
                .find(|stream| {
                    stream.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image")
                        && stream
                            .dict
                            .get(b"ColorSpace")
                            .and_then(Object::as_name)
                            .ok()
                            == Some(b"DeviceRGB")
                })
                .unwrap();
            let native_rgb = expected
                .pixels()
                .iter()
                .flat_map(|pixel| {
                    let pixel = pixel.demultiply();
                    [pixel.red(), pixel.green(), pixel.blue()]
                })
                .collect::<Vec<_>>();
            assert_eq!(
                decoded(embedded, STREAM_LIMIT).unwrap(),
                native_rgb,
                "Page fallback changed native composite pixels"
            );
        }
        let pdf = temp.join(format!("effects-{index}.pdf"));
        std::fs::write(&pdf, bytes).unwrap();
        let prefix = temp.join(format!("page-{index}"));
        // Poppler's Cairo renderer maps 1:1 image pixels without the Splash
        // backend's extra resampling. A separate pdftoppm test covers OCG/alpha.
        let output = std::process::Command::new("pdftocairo")
            .args(["-png", "-r", "72", "-singlefile"])
            .arg(&pdf)
            .arg(&prefix)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual = image::open(prefix.with_extension("png")).unwrap().to_rgb8();
        assert_eq!(actual.dimensions(), (expected.width(), expected.height()));
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
            / (expected.width() as f64 * expected.height() as f64 * 3.);
        if pass_through {
            assert_eq!(
                difference, 0.,
                "PDF page raster differs from native composite: {notes:?}"
            );
        } else {
            assert!(
                difference < 2.,
                "PDF effects differ from native composition by {difference}/255; {notes:?}"
            );
        }
    }
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
#[ignore = "external interoperability check: requires Poppler pdftoppm"]
fn poppler_renders_exported_pages_layers_and_soft_alpha() {
    let (bytes, _) = write(&export_document()).unwrap();
    let temp = std::env::temp_dir().join(format!(
        "omadesign-pdf-{}-{}",
        std::process::id(),
        crate::document::next_id()
    ));
    std::fs::create_dir(&temp).unwrap();
    let pdf = temp.join("layers.pdf");
    std::fs::write(&pdf, bytes).unwrap();
    let output = std::process::Command::new("pdftoppm")
        .args(["-png", "-r", "72"])
        .arg(&pdf)
        .arg(temp.join("page"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let page1 = image::open(temp.join("page-1.png")).unwrap().to_rgb8();
    let page2 = image::open(temp.join("page-2.png")).unwrap().to_rgb8();
    assert_eq!(page1.dimensions(), (50, 80));
    assert_eq!(
        page1.get_pixel(10, 10).0,
        [255, 255, 255],
        "Hidden OCG is rendered"
    );
    let translucent = page1.get_pixel(13, 33).0;
    assert!(
        translucent[0] > 250
            && (120..=130).contains(&translucent[1])
            && (120..=130).contains(&translucent[2]),
        "Unexpected image alpha/orientation: {translucent:?}"
    );
    assert_eq!(page1.get_pixel(13, 47).0, [0, 0, 255]);
    assert_eq!(page2.get_pixel(20, 20).0, [0, 0, 255]);
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
#[ignore = "external importer check: requires librsvg rsvg-convert"]
fn imports_cairo_generated_pdf_as_editable_vector_artwork() {
    let temp = std::env::temp_dir().join(format!(
        "omadesign-cairo-pdf-{}-{}",
        std::process::id(),
        crate::document::next_id()
    ));
    std::fs::create_dir(&temp).unwrap();
    let svg = temp.join("source.svg");
    let pdf = temp.join("source.pdf");
    std::fs::write(&svg,r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100" viewBox="0 0 200 100"><path d="M10 10 C20 0 30 0 40 10 L40 40 L10 40Z" fill="#ff0000"/><path d="M60 20L90 20L90 60L60 60Z" fill="#0000ff"/></svg>"##).unwrap();
    let output = std::process::Command::new("rsvg-convert")
        .arg(&svg)
        .arg("--format=pdf")
        .arg(format!("--output={}", pdf.display()))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let (document, warnings) = read(&std::fs::read(&pdf).unwrap(), "cairo.pdf").unwrap();
    assert_eq!(document.artboards.len(), 1);
    assert!((document.artboards[0].size.x - 150.).abs() < 0.01);
    assert_eq!(shapes(&document).len(), 2, "{warnings:?}");
    assert!(shapes(&document).iter().any(|shape|matches!(shape.geom,Geom::Path {ref anchors,..} if anchors.iter().any(|anchor|anchor.h_out!=Pt::ZERO))));
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
#[ignore = "private fixture visual QA: set OMA_PDF_FIXTURE and OMA_PDF_PREVIEW"]
fn render_private_pdf_fixture() {
    let input =
        std::path::PathBuf::from(std::env::var_os("OMA_PDF_FIXTURE").expect("OMA_PDF_FIXTURE"));
    let output =
        std::path::PathBuf::from(std::env::var_os("OMA_PDF_PREVIEW").expect("OMA_PDF_PREVIEW"));
    std::fs::create_dir_all(&output).unwrap();
    if std::env::var_os("OMA_PDF_INSPECT").is_some() {
        let mut parsed = Pdf::load_mem(&std::fs::read(&input).unwrap()).unwrap();
        parsed.decompress();
        parsed.save(output.join("decoded-source.pdf")).unwrap();
    }
    let (document, warnings) = read(
        &std::fs::read(&input).unwrap(),
        input.file_name().unwrap().to_str().unwrap(),
    )
    .unwrap();
    eprintln!(
        "pages={} layers={} groups={} shapes={} rasters={} warnings={warnings:#?}",
        document.artboards.len(),
        document.layers.len(),
        document
            .layers
            .iter()
            .filter(|layer| layer.is_group)
            .count(),
        shapes(&document).len(),
        document
            .layers
            .iter()
            .filter(|layer| matches!(layer.kind, LayerKind::Raster { .. }))
            .count()
    );
    for (index, board) in document.artboards.iter().take(4).enumerate() {
        let scale = 900. / board.size.x.max(board.size.y);
        let pm = crate::compositor::render_view(
            &document,
            crate::compositor::View {
                scale,
                offset: -board.origin * scale,
            },
            (board.size.x * scale).round() as u32,
            (board.size.y * scale).round() as u32,
            crate::compositor::Draft::none(),
        )
        .unwrap();
        pm.save_png(output.join(format!("native-{}.png", index + 1)))
            .unwrap();
    }
}
