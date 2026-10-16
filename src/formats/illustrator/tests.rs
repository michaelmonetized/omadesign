use super::*;
use crate::color::Rgba;
use crate::document::Fill;
use crate::geom::Geom;
use lopdf::{Object, Stream, dictionary};
use sha2::{Digest, Sha256};
use std::io::Write;

fn program(art: &str) -> Vec<u8> {
    format!("%!PS-Adobe-3.0\n%%Creator: Adobe Illustrator\n%AI3_Cropmarks: 0 0 200 100\n%AI5_BeginLayer\n1 1 1 1 0 0 1 0 79 128 255 0 50 0 Lb\n(Main) Ln\n{art}\nLB\n%%PageTrailer\n").into_bytes()
}

fn rectangle() -> &'static str {
    "10 20 m 50 20 l 50 60 l 10 60 l f"
}

fn pdf(data: &[u8], preview: bool, prefix: &str) -> Vec<u8> {
    let mut pdf = lopdf::Document::with_version("1.7");
    let pages = pdf.new_object_id();
    let cut = data.len() / 2;
    let first = pdf.add_object(Stream::new(dictionary! {}, data[..cut].to_vec()));
    let second = pdf.add_object(Stream::new(dictionary! {}, data[cut..].to_vec()));
    let mut private = dictionary! {"NumBlock"=>2};
    private.set(format!("{prefix}2"), second);
    private.set(format!("{prefix}1"), first);
    let mut page = dictionary! {"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),200.into(),100.into()],
    "PieceInfo"=>dictionary! {"Illustrator"=>dictionary! {"Private"=>private}}};
    if preview {
        let content = pdf.add_object(Stream::new(
            dictionary! {},
            b"0 0 1 rg 0 0 200 100 re f".to_vec(),
        ));
        page.set("Contents", content);
    }
    let page = pdf.add_object(page);
    pdf.objects.insert(
        pages,
        Object::Dictionary(dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}),
    );
    let catalog = pdf.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    pdf.trailer.set("Root", catalog);
    let mut out = Vec::new();
    pdf.save_to(&mut out).unwrap();
    out
}

fn zlib(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

fn encode85(data: &[u8]) -> String {
    let mut out = String::new();
    for part in data.chunks(4) {
        let mut bytes = [0; 4];
        bytes[..part.len()].copy_from_slice(part);
        let mut n = u32::from_be_bytes(bytes);
        let mut group = [b'!'; 5];
        for i in (0..5).rev() {
            group[i] = b'!' + (n % 85) as u8;
            n /= 85;
        }
        out.push_str(std::str::from_utf8(&group[..part.len() + 1]).unwrap());
    }
    out.push_str("~>");
    out
}

#[test]
fn illustrator_private_streams_override_preview_and_work_without_it() {
    let native = program(&format!("u\n0 1 1 0 1 0 0 Xa\n{}\nU", rectangle()));
    for (marker, packed) in [
        (b"%AI12_CompressedData".as_slice(), zlib(&native)),
        (
            b"%AI24_ZStandard_Data".as_slice(),
            zstd::stream::encode_all(native.as_slice(), 1).unwrap(),
        ),
    ] {
        let data = [marker, packed.as_slice()].concat();
        for preview in [true, false] {
            for prefix in ["AIPrivateData", "AIPDFPrivateData"] {
                let (doc, notes) = read(&pdf(&data, preview, prefix), "Private").unwrap();
                assert_eq!((doc.width, doc.height), (200., 100.));
                let main = doc.layers.iter().find(|l| l.name == "Main").unwrap();
                let group = doc
                    .layers
                    .iter()
                    .find(|l| l.is_group && l.parent == Some(main.id))
                    .unwrap();
                let layer = doc
                    .layers
                    .iter()
                    .find(|l| l.parent == Some(group.id))
                    .unwrap();
                let shape = &layer.kind.shapes().unwrap()[0];
                assert_eq!(shape.style.fill, Fill::Solid(Rgba::rgb(255, 0, 0)));
                assert!(shape.style.stroke.is_none());
                assert_eq!(shape.geom.bbox().min, crate::geom::Pt::new(10., 40.));
                assert!(notes[0].contains("editing data"));
                let reopened =
                    crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
                assert_eq!(reopened.layers.len(), doc.layers.len());
                assert_eq!(reopened.artboards, doc.artboards);
                assert_eq!(
                    reopened
                        .layers
                        .iter()
                        .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
                        .next()
                        .unwrap()
                        .geom,
                    shape.geom
                );
            }
        }
    }
}

#[test]
fn illustrator_eps_ascii85_and_legacy_editable_type() {
    let native = program(
        "0 0 1 0 0 1 0 Xa\n1 Xw\n1 A\n0 To\n1 0 0 1 25 70 Tp\n(Helvetica) 14 Tf\n(Hello \\050AI\\051) Tx\nTO",
    );
    for (codec, packed) in [
        ("AI9", zlib(&native)),
        (
            "AI24",
            zstd::stream::encode_all(native.as_slice(), 1).unwrap(),
        ),
    ] {
        let eps = format!(
            "%!PS-Adobe-3.0 EPSF-3.0\n%%EOF\n%AI9_PrivateDataBegin\n%{codec}_DataStream\n%{}\n%AI9_PrivateDataEnd\n",
            encode85(&packed)
        );
        let (doc, _) = read(eps.as_bytes(), "Type").unwrap();
        let shape = doc
            .layers
            .iter()
            .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
            .next()
            .unwrap();
        let Geom::Text(text) = &shape.geom else {
            panic!("type remains editable")
        };
        assert_eq!(text.content, "Hello (AI)");
        assert_eq!(text.px, 14.);
        assert_eq!(text.origin, crate::geom::Pt::new(25., 30.));
        assert_eq!(shape.style.fill, Fill::Solid(Rgba::rgb(0, 255, 0)));
        assert!(!shape.visible);
        assert!(shape.locked);
        assert!(text.contours.iter().any(|c| !c.is_empty()));
    }
    assert!(read(&native, "Legacy").is_ok());
}

#[test]
fn illustrator_artboard_dictionary_names_and_layout_survive_reopen() {
    let boards=b"%_/Document :\n%_/Dictionary :\n%_/Array :\n%_/Dictionary :\n%_(Front) /String (Name) ,\n%_0 100 /RealPoint (PositionPoint1) ,\n%_100 0 /RealPoint (PositionPoint2) ,\n%_; ,\n%_/Dictionary :\n%_(Back) /String (Name) ,\n%_150 100 /RealPoint (PositionPoint1) ,\n%_350 -50 /RealPoint (PositionPoint2) ,\n%_; ,\n%_; (ArtboardArray) ,\n%_; /NotRecorded ,\n%_;\n";
    let mut native = program(rectangle());
    let end = native
        .windows(b"%%PageTrailer".len())
        .position(|w| w == b"%%PageTrailer")
        .unwrap();
    native.splice(end..end, boards.iter().copied());
    let (doc, _) = read(&native, "Boards").unwrap();
    assert_eq!(doc.artboards.len(), 2);
    assert_eq!(doc.artboards[0].name, "Front");
    assert_eq!(doc.artboards[1].name, "Back");
    assert_eq!(doc.artboards[1].origin, crate::geom::Pt::new(150., 0.));
    assert_eq!(doc.artboards[1].size, crate::geom::Pt::new(200., 150.));
    let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    assert_eq!(reopened.artboards, doc.artboards);
}

#[test]
fn illustrator_compounds_and_unsupported_features_are_explicit() {
    let native = program(&format!(
        "*u\n{}\n60 20 m 80 20 l 80 40 l f\n*U\n(missing) 0 0 1 1 0 0 0 0 0 [1 0 0 1 0 0] p\n{}\n/Mesh X!\n/End X!",
        rectangle(),
        rectangle()
    ));
    let (doc, notes) = read(&native, "Compound").unwrap();
    let shape = doc
        .layers
        .iter()
        .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
        .next()
        .unwrap();
    let Geom::Paths { paths, .. } = &shape.geom else {
        panic!()
    };
    assert_eq!(paths.len(), 2);
    assert!(shape.style.stroke.is_none());
    assert!(notes.iter().any(|n| n.contains("Pattern paint")));
    assert!(notes.iter().any(|n| n.contains("gradient meshes")));
    assert_eq!(doc.import_notes, notes);
}

#[test]
fn illustrator_compound_children_keep_their_container_after_reopen() {
    let native = program(&format!(
        "*u\n{}\nu\n70 20 m 90 20 l 90 40 l 70 40 l f\nU\n*U",
        rectangle()
    ));
    let (doc, _) = read(&native, "Compound children").unwrap();
    doc.validate_hierarchy().unwrap();
    let main = doc.layers.iter().find(|l| l.name == "Main").unwrap();
    let compound = doc
        .layers
        .iter()
        .find(|l| {
            l.kind.shapes().is_some_and(|shapes| {
                shapes.iter().any(|shape| {
                    shape.geom.bbox()
                        == crate::geom::Bounds::from_min_size(
                            crate::geom::Pt::new(10., 40.),
                            crate::geom::Pt::splat(40.),
                        )
                })
            })
        })
        .unwrap();
    let container = doc
        .layers
        .iter()
        .find(|l| Some(l.id) == compound.parent && l.is_group)
        .unwrap();
    assert_eq!(container.parent, Some(main.id));
    let nested_group = doc
        .layers
        .iter()
        .find(|l| l.is_group && l.parent == Some(container.id))
        .unwrap();
    let nested_path = doc
        .layers
        .iter()
        .find(|l| l.parent == Some(nested_group.id) && !l.is_group)
        .unwrap();
    assert_eq!(nested_path.kind.shapes().unwrap().len(), 1);
    assert_eq!(compound.kind.shapes().unwrap().len(), 1);
    assert_eq!(
        nested_path.kind.shapes().unwrap()[0].geom.bbox(),
        crate::geom::Bounds::from_min_size(
            crate::geom::Pt::new(70., 60.),
            crate::geom::Pt::splat(20.),
        )
    );
    assert_eq!(
        doc.layers
            .iter()
            .filter_map(|l| l.kind.shapes())
            .map(|s| s.len())
            .sum::<usize>(),
        2
    );
    let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(&reopened.layers).unwrap(),
        serde_json::to_value(&doc.layers).unwrap()
    );
}

#[test]
fn illustrator_damaged_containers_fail_instead_of_silent_preview_success() {
    assert!(decode_private(b"%AI12_CompressedData broken").is_err());
    assert!(decode_private(b"%AI24_ZStandard_Data broken").is_err());
    assert!(ascii85(b"!~>").is_err());
    assert!(ascii85(b"uuuuu~>").is_err());
    assert!(read(b"%PDF-1.7 garbage", "Bad").is_err());
    assert!(read(&program("u 0 0 m 10 10 l S"), "Unbalanced").is_err());
}

#[test]
fn illustrator_embedded_picture_keeps_exact_rotation_and_labels_shear() {
    let samples = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0];
    for (matrix, expected_origin, expected_size, sheared) in [
        (
            "0 1 -1 0 40 60",
            crate::geom::Pt::new(38., 40.),
            crate::geom::Pt::new(2., 2.),
            false,
        ),
        (
            "1 0 0.5 1 10 80",
            crate::geom::Pt::new(10., 20.),
            crate::geom::Pt::new(3., 2.),
            true,
        ),
    ] {
        let mut art=format!("%AI5_BeginRaster\n/DeviceRGB XN\n[{matrix}] 0 0 2 2 2 2 8 3 0 0 1\n%%BeginData: {}\nXI\n",samples.len()+4).into_bytes();
        art.extend_from_slice(&samples);
        art.extend_from_slice(b"\n%%EndData\n%AI5_EndRaster\n");
        let mut input = program("");
        let at = input.windows(3).position(|w| w == b"LB\n").unwrap();
        input.splice(at..at, art);
        let (doc, notes) = read(&input, "Picture placement").unwrap();
        let layer = doc
            .layers
            .iter()
            .find(|l| l.kind.pixels().is_some())
            .unwrap();
        let (origin, size, rotation) = layer.kind.raster_xform().unwrap();
        assert_eq!(origin, expected_origin);
        assert_eq!(size, expected_size);
        assert_eq!(
            notes
                .iter()
                .any(|n| n.contains("Sheared Illustrator pictures")),
            sheared
        );
        let pixels = layer.kind.pixels().unwrap();
        assert!(pixels.data.chunks_exact(4).any(|p| p[3] > 0));
        if !sheared {
            assert!((rotation - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
            let transform = crate::compositor::layer_pixel_transform(layer);
            assert!((transform.tx - 40.).abs() < 1e-5);
            assert!((transform.ty - 40.).abs() < 1e-5);
            assert!((transform.ky - 1.).abs() < 1e-5);
            assert!((transform.kx + 1.).abs() < 1e-5);
            assert_eq!(&pixels.data[..4], &[255, 0, 0, 255]);
        }
    }
}

#[test]
fn illustrator_linked_picture_retains_original_once_and_reopens() {
    let directory = std::env::current_dir()
        .unwrap()
        .join(".artifacts")
        .join(format!(
            "illustrator-link-{}-{}",
            std::process::id(),
            crate::document::next_id()
        ));
    std::fs::create_dir_all(&directory).unwrap();
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _scratch = Scratch(directory.clone());
    let mut bytes = Vec::new();
    image::RgbaImage::from_pixel(2, 1, image::Rgba([17, 93, 201, 255]))
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    std::fs::write(directory.join("pixel.png"), &bytes).unwrap();
    let place = "%AI5_BeginPlace\n[1 0 0 1 0 0] 0 0 2 1 (pixel.png) `\n~\n%AI5_EndPlace\n";
    let (doc, notes) = read_with_path(
        &program(&place.repeat(2)),
        "Linked",
        Some(&directory.join("source.ai")),
    )
    .unwrap();
    assert!(!notes.iter().any(|n| n.contains("saved preview")));
    assert_eq!(doc.sources.len(), 2);
    assert_eq!(doc.sources[0].bytes(), bytes);
    assert_eq!(doc.sources[0].name, "pixel.png");
    assert_eq!(
        doc.layers
            .iter()
            .filter(|l| l.source_attachment.as_deref() == Some(doc.sources[0].id.as_str()))
            .count(),
        2
    );
    let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    assert_eq!(reopened.sources, doc.sources);
}

#[test]
fn illustrator_linked_native_pixels_retain_precision_and_refuse_damaged_decode() {
    let directory = std::env::current_dir()
        .unwrap()
        .join(".artifacts")
        .join(format!(
            "illustrator-native-link-{}-{}",
            std::process::id(),
            crate::document::next_id()
        ));
    std::fs::create_dir_all(&directory).unwrap();
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _scratch = Scratch(directory.clone());
    let image = image::ImageBuffer::<image::Rgba<u16>, _>::from_raw(
        1,
        2,
        vec![12345, 23456, 34567, 65535, 45678, 56789, 10101, 32768],
    )
    .unwrap();
    let mut encoded = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba16(image)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    let bytes = encoded.into_inner();
    let image_path = directory.join("native.png");
    let source_path = directory.join("source.ai");
    std::fs::write(&image_path, &bytes).unwrap();
    let source =
        program("%AI5_BeginPlace\n[1 0 0 1 0 0] 0 0 1 2 (native.png) `\n~\n%AI5_EndPlace\n");
    std::fs::write(&source_path, &source).unwrap();
    let imported = crate::import::open_any(&source_path).unwrap();
    let crate::import::Imported::Document(doc) = imported else {
        panic!("expected native artwork")
    };
    assert_eq!(doc.sources.len(), 2);
    let layer = doc
        .layers
        .iter()
        .find(|l| l.kind.pixels().is_some())
        .unwrap();
    let original = doc
        .source(layer.source_attachment.as_deref().unwrap())
        .unwrap();
    assert_eq!(original.bytes(), bytes);
    let pixels = layer.kind.pixels().unwrap();
    let native = pixels.native.as_ref().unwrap();
    assert_eq!(native.depth, crate::deep_color::Depth::U16);
    for (actual, expected) in native
        .samples
        .iter()
        .zip([45678, 56789, 10101, 32768, 12345, 23456, 34567, 65535])
    {
        assert!((actual - expected as f32 / 65535.).abs() < 1e-7);
    }
    let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    let reopened_pixels = reopened
        .layers
        .iter()
        .find_map(|l| l.kind.pixels())
        .unwrap();
    assert_eq!(reopened_pixels.native, pixels.native);
    assert_eq!(reopened.sources, doc.sources);
    std::fs::write(&image_path, b"\x89PNG\r\n\x1a\ntruncated").unwrap();
    let error = match crate::import::open_any(&source_path) {
        Err(error) => error,
        Ok(_) => panic!("damaged linked image was silently replaced"),
    };
    assert!(picture_import_failed(&error), "{error}");
    assert!(error.contains("native.png"), "{error}");
    assert_eq!(std::fs::read(&source_path).unwrap(), source);
    assert_eq!(original.bytes(), bytes);
}

#[test]
#[ignore = "Requires the independent real AI fixture directory through OMADESIGN_ILLUSTRATOR_FIXTURE"]
fn illustrator_independent_real_fixture_without_pdf_preview_and_oma_reopen() {
    independent_fixture("Test-Inkscape-LineIssue.ai", 1, false);
}

#[test]
#[ignore = "Requires the independent real AI fixture directory through OMADESIGN_ILLUSTRATOR_FIXTURE"]
fn illustrator_independent_font_template_without_pdf_preview_and_oma_reopen() {
    independent_fixture("font_template.ai", 208, true);
}

#[test]
#[ignore = "Requires the independent real AI fixture directory through OMADESIGN_ILLUSTRATOR_FIXTURE"]
fn illustrator_independent_reuters_without_pdf_preview_and_oma_reopen() {
    independent_fixture("reuters-dynamicText.ai", 5, true);
}

fn independent_fixture(file: &str, expected_boards: usize, needs_type: bool) {
    let location = std::env::var_os("OMADESIGN_ILLUSTRATOR_FIXTURE").expect(
        "set OMADESIGN_ILLUSTRATOR_FIXTURE to the independent fixture directory or one AI in it",
    );
    let location = Path::new(&location);
    let directory = if location.is_dir() {
        location
    } else {
        location.parent().unwrap()
    };
    let path = directory.join(file);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{file}: {e}"));
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    assert_eq!(
        sha256,
        match file {
            "Test-Inkscape-LineIssue.ai" =>
                "eb63800d64794a0e77dedc9ab83a371111595e8caa3ad0aecfa814c06385d867",
            "font_template.ai" =>
                "72e84d9aa84960d57fde0e6fe296da0fa93977c1a35627d837258f1aeb0d4cb9",
            "reuters-dynamicText.ai" =>
                "42d18b9f6bd525739e6d30b3e74ae56e841b9f589fbd0edc219d6c54935407e5",
            _ => unreachable!(),
        },
        "{file}: independent source identity"
    );
    let data = editing_data(&bytes).unwrap();
    let expected_stories = String::from_utf8_lossy(&data)
        .matches("/StoryIndex")
        .count();
    let (doc, notes) = read_with_path(&bytes, "Independent", Some(&path))
        .unwrap_or_else(|e| panic!("{file}: {e}"));
    assert_eq!(
        doc.artboards.len(),
        expected_boards,
        "{file}: real artboard count"
    );
    if file == "reuters-dynamicText.ai" {
        for (board, (name, bounds)) in doc.artboards.iter().zip([
            ("xs", [0., 0., 330., 749.26886]),
            ("sm", [1085.4232, 0., 510., 882.26886]),
            ("md", [2390.8464, 0., 660., 846.26886]),
            ("lg", [4009.2697, 0., 930., 659.26886]),
            ("xl", [5138.693, 0., 1200., 819.]),
        ]) {
            assert_eq!(board.name, name, "Reuters live artboard name");
            for (actual, expected) in [board.origin.x, board.origin.y, board.size.x, board.size.y]
                .iter()
                .zip(bounds)
            {
                assert!(
                    (actual - expected).abs() < 0.001,
                    "Reuters source artboard geometry: {actual} vs {expected}"
                );
            }
        }
    }
    let mut without = lopdf::Document::load_mem(&bytes).unwrap();
    for id in without.get_pages().values() {
        without.get_dictionary_mut(*id).unwrap().remove(b"Contents");
    }
    let mut stripped = Vec::new();
    without.save_to(&mut stripped).unwrap();
    assert_eq!(
        editing_data(&stripped).unwrap(),
        data,
        "{file}: stripping previews must not alter editing data"
    );
    let (alone, alone_notes) =
        read(&stripped, "Independent").unwrap_or_else(|e| panic!("{file} without preview: {e}"));
    assert_eq!(
        doc.artboards
            .iter()
            .map(|a| (&a.name, a.origin, a.size))
            .collect::<Vec<_>>(),
        alone
            .artboards
            .iter()
            .map(|a| (&a.name, a.origin, a.size))
            .collect::<Vec<_>>()
    );
    fn shapes(doc: &Document) -> Vec<(serde_json::Value, crate::document::Style)> {
        doc.layers
            .iter()
            .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
            .map(|s| (serde_json::to_value(&s.geom).unwrap(), s.style.clone()))
            .collect()
    }
    let original_shapes = shapes(&doc);
    let previewless_shapes = shapes(&alone);
    assert!(
        original_shapes == previewless_shapes,
        "{file}: preview-independent native artwork: {}",
        shape_difference(&original_shapes, &previewless_shapes)
    );
    assert_eq!(notes, alone_notes);
    assert!(!shapes(&doc).is_empty(), "{file}: actual native artwork");
    let text: Vec<_> = doc
        .layers
        .iter()
        .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
        .filter_map(|s| {
            if let Geom::Text(t) = &s.geom {
                Some(t)
            } else {
                None
            }
        })
        .collect();
    if needs_type {
        assert!(expected_stories > 0, "{file}: real source has live stories");
        assert!(
            !text.is_empty(),
            "{file}: real editable type must be reconstructed"
        );
        assert!(
            text.iter().any(|t| !t.content.trim().is_empty()),
            "{file}: text must retain characters"
        );
        let reference = path.with_extension("preview-text.html");
        let output = std::process::Command::new("pdftotext")
            .arg("-bbox-layout")
            .arg(&path)
            .arg(&reference)
            .output()
            .expect("independent text check requires Poppler pdftotext");
        assert!(
            output.status.success(),
            "{file}: independent Poppler text extraction failed"
        );
        let html = std::fs::read_to_string(&reference).unwrap();
        let words = html
            .lines()
            .filter(|l| l.trim_start().starts_with("<word "))
            .map(|line| {
                roxmltree::Document::parse(line.trim())
                    .unwrap()
                    .root_element()
                    .text()
                    .unwrap_or("")
                    .to_string()
            })
            .filter(|s| !s.trim().is_empty())
            .collect::<Vec<_>>();
        assert!(
            !words.is_empty(),
            "{file}: independent PDF text extraction is empty"
        );
        let placeholder = words
            .join(" ")
            .contains("File that was saved without PDF Content.");
        let mut qualification = serde_json::json!({"file":file,"sha256":sha256,"source_story_slots":expected_stories,"native_text_objects":text.len(),"independent_pdf_words":words.len(),"pdf_is_placeholder":placeholder,"warnings":notes});
        let bounds=doc.layers.iter().filter_map(|l|l.kind.shapes()).flatten().filter_map(|s|if let Geom::Text(t)=&s.geom{Some(serde_json::json!({"text":t.content,"origin":t.origin,"bounds":s.world_bbox()}))}else{None}).take(8).collect::<Vec<_>>();
        qualification["native_examples"] = serde_json::json!(bounds);
        if file == "reuters-dynamicText.ai" {
            assert!(
                placeholder,
                "Reuters intentionally has no PDF artwork preview"
            );
            let oracle_path = path.with_extension("independent-live-text.json");
            let output = std::process::Command::new("node")
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/inspect-ai-live-text.mjs"))
                .arg(&path)
                .arg(&oracle_path)
                .arg(&reference)
                .output()
                .expect("independent private-text check requires Node and qpdf");
            assert!(
                output.status.success(),
                "independent ATE extraction failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let oracle: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&oracle_path).unwrap()).unwrap();
            assert_eq!(oracle["sourceSha256"].as_str().unwrap(), sha256);
            assert_eq!(oracle["expectedNativeTextObjects"], 104);
            let live = oracle["liveStoryContent"].as_object().unwrap();
            let multiset = |values: Vec<String>| {
                let mut counts = std::collections::BTreeMap::new();
                for value in values {
                    *counts.entry(value).or_insert(0usize) += 1;
                }
                counts
            };
            let expected = multiset(
                live.values()
                    .map(|v| v.as_str().unwrap().to_owned())
                    .collect(),
            );
            let actual = multiset(text.iter().map(|t| t.content.clone()).collect());
            let matched = expected
                .iter()
                .map(|(s, n)| (*n).min(actual.get(s).copied().unwrap_or(0)))
                .sum::<usize>();
            let missing: Vec<_> = live
                .iter()
                .filter(|(_, v)| {
                    let content = v.as_str().unwrap();
                    actual.get(content).copied().unwrap_or(0)
                        != expected.get(content).copied().unwrap_or(0)
                })
                .map(|(index, _)| index)
                .collect();
            qualification["source_oracle"] = "independent qpdf/Node live ATE stories".into();
            qualification["expected_live_text_objects"] = 104.into();
            qualification["exact_live_text_objects_retained"] = matched.into();
            qualification["source_live_words"] = oracle["counts"]["liveWords"].clone();
            qualification["missing_story_indices"] = serde_json::json!(missing);
            std::fs::write(
                path.with_extension("native-type-qualification.json"),
                serde_json::to_string_pretty(&qualification).unwrap(),
            )
            .unwrap();
            assert_eq!(text.len(), 104, "Reuters live editable text count");
            assert!(
                actual == expected,
                "Reuters exact live characters/paragraphs: retained {matched}/104 source objects; missing story indices {missing:?}"
            );
        } else {
            assert!(!placeholder, "font fixture requires actual PDF artwork");
            let content = text
                .iter()
                .map(|t| t.content.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let matched = words
                .iter()
                .filter(|w| content.contains(w.as_str()))
                .count();
            qualification["source_oracle"] = "independent Poppler PDF artwork words".into();
            qualification["words_retained_as_editable_characters"] = matched.into();
            std::fs::write(
                path.with_extension("native-type-qualification.json"),
                serde_json::to_string_pretty(&qualification).unwrap(),
            )
            .unwrap();
            assert!(
                matched * 10 >= words.len() * 9,
                "{file}: only {matched}/{} independent PDF words survived as editable type",
                words.len()
            );
        }
    }
    let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    assert!(
        reopened
            .sources
            .iter()
            .any(|s| s.format == "illustrator" && s.bytes() == bytes),
        "{file}: original AI bytes must survive native import and reopen"
    );
    assert_eq!(
        shapes(&doc),
        shapes(&reopened),
        "{file}: native save/reopen"
    );
    assert_eq!(doc.artboards, reopened.artboards);
    assert_eq!(doc.import_notes, reopened.import_notes);
    independent_external_roundtrip(&path, &sha256, &reopened);
    if !needs_type {
        let svg = std::fs::read_to_string(directory.join("Test-Inkscape-LineIssue.svg")).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(svg.as_bytes())),
            "268749a71ee81813902185793bf5bda4d17acf12ca71fe62d2a2fd1237a80f52",
            "independent author SVG identity"
        );
        let reference = usvg::Tree::from_str(&svg, &usvg::Options::default()).unwrap();
        let marks = |doc: &Document| {
            let mut marks: Vec<_> = doc
                .layers
                .iter()
                .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
                .filter(|s| !s.guide && s.style.stroke.is_some())
                .map(|s| {
                    let b = s.geom.bbox();
                    let stroke = s.style.stroke.as_ref().unwrap();
                    [b.min.x, b.min.y, b.max.x, b.max.y, stroke.width]
                })
                .collect();
            marks.sort_by(|a, b| a[1].total_cmp(&b[1]).then(a[0].total_cmp(&b[0])));
            marks
        };
        let actual = marks(&doc);
        fn reference_marks(group: &usvg::Group, marks: &mut Vec<[f32; 5]>) {
            for node in group.children() {
                match node {
                    usvg::Node::Group(group) => reference_marks(group, marks),
                    usvg::Node::Path(path) => {
                        if let Some(stroke) = path.stroke() {
                            let bounds = path.abs_bounding_box();
                            let (sx, sy) = path.abs_transform().get_scale();
                            assert!((sx - sy).abs() < 1e-6, "author's reference stroke scale");
                            marks.push([
                                bounds.left(),
                                bounds.top(),
                                bounds.right(),
                                bounds.bottom(),
                                stroke.width().get() * sx,
                            ]);
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut expected = Vec::new();
        reference_marks(reference.root(), &mut expected);
        expected.sort_by(|a, b| a[1].total_cmp(&b[1]).then(a[0].total_cmp(&b[0])));
        assert_eq!(
            expected.len(),
            28,
            "independent author's reference has28 strokes"
        );
        assert_eq!(
            actual.len(),
            expected.len(),
            "author's SVG stroke object count"
        );
        let mut max_error = 0.0f32;
        for (a, b) in actual.iter().zip(&expected) {
            for (a, b) in a.iter().zip(b) {
                max_error = max_error.max((a - b).abs());
                assert!(
                    (a - b).abs() < 0.025,
                    "AI vs author's SVG paint/placement: {a} vs {b}"
                );
            }
        }
        std::fs::write(path.with_extension("native-stroke-qualification.json"), serde_json::to_string_pretty(&serde_json::json!({"file":file,"sha256":sha256,"native_strokes":actual.len(),"independent_svg_strokes":expected.len(),"max_bounds_or_width_error":max_error,"tolerance":0.025,"source_identity_preview_removal_and_oma_reopen":"passed"})).unwrap()).unwrap();
    }
    eprintln!(
        "Independent AI {file}: {} bytes, {} artboards, {} layers, {} native objects, {} editable text from {expected_stories} real story slots; {} notes",
        bytes.len(),
        doc.artboards.len(),
        doc.layers.len(),
        shapes(&doc).len(),
        text.len(),
        notes.len()
    );
}

/// Describe the first exact artwork difference without dumping the whole document.
/// Args: original and previewless geometry/style pairs. Returns a bounded JSON diagnostic; equality is unchanged.
fn shape_difference(
    original: &[(serde_json::Value, crate::document::Style)],
    previewless: &[(serde_json::Value, crate::document::Style)],
) -> serde_json::Value {
    fn difference(a: &serde_json::Value, b: &serde_json::Value, path: String) -> serde_json::Value {
        match (a, b) {
            (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
                for key in a.keys().chain(b.keys()) {
                    if a.get(key) != b.get(key) {
                        return difference(
                            a.get(key).unwrap_or(&serde_json::Value::Null),
                            b.get(key).unwrap_or(&serde_json::Value::Null),
                            format!("{path}/{key}"),
                        );
                    }
                }
            }
            (serde_json::Value::Array(a), serde_json::Value::Array(b)) => {
                for i in 0..a.len().max(b.len()) {
                    if a.get(i) != b.get(i) {
                        return difference(
                            a.get(i).unwrap_or(&serde_json::Value::Null),
                            b.get(i).unwrap_or(&serde_json::Value::Null),
                            format!("{path}/{i}"),
                        );
                    }
                }
            }
            _ => {}
        }
        let offset = a.as_str().zip(b.as_str()).map(|(a, b)| {
            a.chars()
                .zip(b.chars())
                .position(|(a, b)| a != b)
                .unwrap_or_else(|| a.chars().count().min(b.chars().count()))
        });
        let summary = |value: &serde_json::Value| {
            if let Some(text) = value.as_str() {
                serde_json::json!({"bytes":text.len(),"sha256":format!("{:x}",Sha256::digest(text.as_bytes())),"context":text.chars().skip(offset.unwrap_or(0).saturating_sub(40)).take(120).collect::<String>()})
            } else {
                serde_json::Value::String(value.to_string().chars().take(160).collect())
            }
        };
        serde_json::json!({"path":path,"first_differing_character":offset,"original":summary(a),"previewless":summary(b)})
    }
    for (index, (a, b)) in original.iter().zip(previewless).enumerate() {
        let (component, detail) = if a.0 != b.0 {
            ("geometry", difference(&a.0, &b.0, String::new()))
        } else if a.1 != b.1 {
            (
                "style",
                difference(
                    &serde_json::to_value(&a.1).unwrap(),
                    &serde_json::to_value(&b.1).unwrap(),
                    String::new(),
                ),
            )
        } else {
            continue;
        };
        return serde_json::json!({"original_count":original.len(),"previewless_count":previewless.len(),"shape_index":index,"component":component,"difference":detail});
    }
    serde_json::json!({"original_count":original.len(),"previewless_count":previewless.len()})
}

/// Check portable exports after native reopen.
/// `source` locates ignored QA outputs, `sha256` binds the original AI, and
/// `reopened` is the verified OMA codec result. Returns after independent SVG/PDF
/// parsing and bounded first-artboard renders contain actual visible artwork.
fn independent_external_roundtrip(source: &Path, sha256: &str, reopened: &Document) {
    use crate::document::LayerKind;
    const MAX_EDGE: u32 = 320;
    let board = reopened.artboards.first().expect("real AI has an artboard");
    assert!(
        board.origin.x.abs() < 0.001 && board.origin.y.abs() < 0.001,
        "the pinned corpus first artboard must start at the native origin"
    );
    assert!(board.size.x > 0. && board.size.y > 0.);
    let bounds = board.local_bounds();
    let native_visible_objects: usize = reopened
        .layers
        .iter()
        .enumerate()
        .filter(|(index, layer)| {
            !layer.is_group && layer.opacity > 0. && reopened.layer_visible(*index)
        })
        .map(|(_, layer)| match &layer.kind {
            LayerKind::Vector { shapes } => shapes
                .iter()
                .filter(|shape| {
                    shape.visible
                        && !shape.guide
                        && shape.opacity > 0.
                        && (!shape.style.fill.is_none()
                            || shape.style.stroke.as_ref().is_some_and(|s| s.width > 0.))
                        && bounds.intersects(
                            shape
                                .world_bbox()
                                .inflate(shape.style.stroke.as_ref().map_or(0., |s| s.width)),
                        )
                })
                .count(),
            LayerKind::Raster { .. } => usize::from(
                layer
                    .kind
                    .raster_bounds()
                    .is_some_and(|b| bounds.intersects(b)),
            ),
        })
        .sum();
    assert!(
        native_visible_objects > 0,
        "selected real artboard must contain visible native artwork"
    );
    let mut scene = reopened.clone();
    scene.artboards = vec![board.clone()];
    scene.width = board.size.x;
    scene.height = board.size.y;
    let svg_path = source.with_extension("external-roundtrip.svg");
    let pdf_path = source.with_extension("external-roundtrip.pdf");
    let svg_preview = source.with_extension("external-svg.png");
    let pdf_prefix = source.with_extension("external-pdf");
    let pdf_preview = source.with_extension("external-pdf.png");
    let receipt_path = source.with_extension("external-roundtrip.json");
    let svg = crate::svg::export(&scene).expect("real AI native artwork exports as SVG");
    std::fs::write(&svg_path, &svg).unwrap();
    let mut options = usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let svg_tree =
        usvg::Tree::from_str(&svg, &options).expect("independent usvg parser accepts exported SVG");
    assert!((svg_tree.size().width() - scene.width).abs() < 0.01);
    assert!((svg_tree.size().height() - scene.height).abs() < 0.01);
    assert!(!svg_tree.root().children().is_empty());
    let scale = MAX_EDGE as f32 / scene.width.max(scene.height);
    let svg_width = (scene.width * scale).round().clamp(1., MAX_EDGE as f32) as u32;
    let svg_height = (scene.height * scale).round().clamp(1., MAX_EDGE as f32) as u32;
    let mut svg_pixels = tiny_skia::Pixmap::new(svg_width, svg_height).unwrap();
    svg_pixels.fill(tiny_skia::Color::WHITE);
    resvg::render(
        &svg_tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut svg_pixels.as_mut(),
    );
    svg_pixels.save_png(&svg_preview).unwrap();
    let svg_nonwhite = svg_pixels
        .pixels()
        .iter()
        .filter(|pixel| pixel.red() < 250 || pixel.green() < 250 || pixel.blue() < 250)
        .count();
    assert!(
        svg_nonwhite >= 4,
        "independent SVG render must contain visible artwork, not an empty viewport"
    );
    let (pdf, pdf_notes) =
        crate::formats::pdf::write(&scene).expect("real AI native artwork exports as PDF");
    std::fs::write(&pdf_path, &pdf).unwrap();
    let syntax = std::process::Command::new("qpdf")
        .arg("--check")
        .arg(&pdf_path)
        .env("LC_ALL", "C")
        .output()
        .expect("external PDF roundtrip requires qpdf");
    assert!(
        syntax.status.success(),
        "qpdf rejects exported PDF: {} {}",
        String::from_utf8_lossy(&syntax.stdout),
        String::from_utf8_lossy(&syntax.stderr)
    );
    let info = std::process::Command::new("pdfinfo")
        .args(["-f", "1", "-l", "1"])
        .arg(&pdf_path)
        .env("LC_ALL", "C")
        .output()
        .expect("external PDF roundtrip requires Poppler pdfinfo");
    assert!(
        info.status.success(),
        "Poppler rejects exported PDF: {}",
        String::from_utf8_lossy(&info.stderr)
    );
    let info = String::from_utf8(info.stdout).unwrap();
    let pages = info
        .lines()
        .find_map(|line| line.strip_prefix("Pages:"))
        .unwrap()
        .trim()
        .parse::<usize>()
        .unwrap();
    assert_eq!(pages, 1, "only the selected artboard is exported for QA");
    let page_size = info
        .lines()
        .find_map(|line| {
            line.trim_start()
                .starts_with("Page")
                .then(|| line.split_once("size:").map(|(_, size)| size))
                .flatten()
        })
        .expect("pdfinfo reports first page dimensions")
        .split_whitespace()
        .collect::<Vec<_>>();
    let pdf_width = page_size[0].parse::<f32>().unwrap();
    let pdf_height = page_size[2].parse::<f32>().unwrap();
    assert!((pdf_width - board.size.x).abs() < 0.02);
    assert!((pdf_height - board.size.y).abs() < 0.02);
    let rendered = std::process::Command::new("pdftoppm")
        .args(["-f", "1", "-l", "1", "-singlefile", "-scale-to"])
        .arg(MAX_EDGE.to_string())
        .arg("-png")
        .arg(&pdf_path)
        .arg(&pdf_prefix)
        .env("LC_ALL", "C")
        .output()
        .expect("external PDF roundtrip requires Poppler pdftoppm");
    assert!(
        rendered.status.success(),
        "Poppler could not render exported PDF: {}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    let pdf_pixels = crate::formats::decode_image(&std::fs::read(&pdf_preview).unwrap())
        .expect("Poppler output is a readable PNG");
    assert!(pdf_pixels.w <= MAX_EDGE && pdf_pixels.h <= MAX_EDGE);
    assert!(pdf_pixels.w.abs_diff(svg_width) <= 1 && pdf_pixels.h.abs_diff(svg_height) <= 1);
    let pdf_nonwhite = pdf_pixels
        .data
        .chunks_exact(4)
        .filter(|pixel| pixel[3] > 0 && pixel[..3].iter().any(|channel| *channel < 250))
        .count();
    assert!(
        pdf_nonwhite >= 4,
        "independent PDF render must contain visible artwork, not an empty page"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(std::fs::read(source).unwrap())),
        sha256,
        "portable export checks must preserve original AI bytes"
    );
    let file_record = |path: &Path| {
        let bytes = std::fs::read(path).unwrap();
        serde_json::json!({"name":path.file_name().unwrap().to_string_lossy(),"bytes":bytes.len(),"sha256":format!("{:x}",Sha256::digest(&bytes))})
    };
    let receipt = serde_json::json!({
        "source":source.file_name().unwrap().to_string_lossy(),
        "source_sha256":sha256,
        "path":"AI import -> OMA encode/decode -> real SVG/PDF exporters -> usvg/resvg and qpdf/Poppler",
        "original_artboard_count":reopened.artboards.len(),
        "checked_artboard":{"name":board.name,"origin":board.origin,"size":board.size,"native_visible_objects":native_visible_objects},
        "export_scope":"One first artboard at native origin; original native geometry, sources, hierarchy and full original-artboard assertions remain unchanged",
        "svg":{"file":file_record(&svg_path),"parser":"usvg","renderer":"resvg","width":svg_tree.size().width(),"height":svg_tree.size().height(),"preview":file_record(&svg_preview),"preview_pixels":[svg_width,svg_height],"nonwhite_pixels":svg_nonwhite},
        "pdf":{"file":file_record(&pdf_path),"syntax_check":"qpdf --check passed","parser":"Poppler pdfinfo","pages":pages,"width_points":pdf_width,"height_points":pdf_height,"renderer":"Poppler pdftoppm first page only","preview":file_record(&pdf_preview),"preview_pixels":[pdf_pixels.w,pdf_pixels.h],"nonwhite_pixels":pdf_nonwhite,"export_notes":pdf_notes},
        "limits":{"maximum_preview_edge":MAX_EDGE,"maximum_pdf_pages_exported_and_rendered":1},
        "proof_boundary":"Valid expected-size portable exports and nonempty independently rendered artwork after native codec reopen on one selected real artboard. Does not establish universal pixel fidelity, full-document artboard fidelity, source hierarchy equivalence, exact font/run paint retention in external editors, or an Illustrator editing-data writer roundtrip."
    });
    std::fs::write(receipt_path, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
}
