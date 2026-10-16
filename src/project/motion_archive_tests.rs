use super::*;
use crate::{
    document::{Layer, Shape, Style},
    geom::{CharSpan, Geom, TypeRun},
    motion::{Ease, Prop, compositions::Composition},
};
use std::sync::Arc;

fn plain() -> Document {
    let mut d = Document::new("Archive", 24., 24., 96.);
    d.layers.clear();
    d.artboards.clear();
    d.artboardless = true;
    d.transparent = true;
    d
}

#[test]
fn motion_feature_versions_keep_all_common_guards_and_nested_highest_feature() {
    let mut d = plain();
    for v in [0, VERSION + 1, u32::MAX] {
        assert!(!supports_version(v));
    }
    for v in 1..=27 {
        assert!(supports_version(v));
    }
    d.motion
        .set_expression(7, Prop::X, Some("time".into()))
        .unwrap();
    assert_eq!(features::required_version(&d), 25);
    d.motion.expressions.clear();
    d.motion.set_key(7, Prop::X, 0., 1., Ease::Hold);
    assert_eq!(features::required_version(&d), 26);
    d.motion.tracks.clear();
    d.motion.set_key(
        7,
        Prop::X,
        0.,
        1.,
        Ease::CubicBezier {
            x1: 0.1,
            y1: -2.,
            x2: 0.9,
            y2: 3.,
        },
    );
    assert_eq!(features::required_version(&d), 26);
    d.motion.tracks.clear();
    d.publishing.bleed = [1.; 4];
    assert_eq!(features::required_version(&d), 23);
    let mut child = plain();
    child.motion.scene3d = Some(crate::scene3d::Scene::default());
    d.motion.compositions.push(Composition::new(child));
    assert_eq!(features::required_version(&d), 27);
    let mut envelope: serde_json::Value = serde_json::from_str(&encode(&d).unwrap()).unwrap();
    assert_eq!(envelope["version"], 27);
    assert!(
        decode(&envelope.to_string()).unwrap().motion.compositions[0]
            .document
            .motion
            .scene3d
            .is_some()
    );
    envelope["version"] = 26.into();
    assert!(
        decode(&envelope.to_string())
            .unwrap_err()
            .contains("requires version 27")
    );
    let mut nested = plain();
    for _ in 0..9 {
        let mut p = plain();
        p.motion.compositions.push(Composition::new(nested));
        nested = p;
    }
    assert!(encode(&nested).unwrap_err().contains("CompositionDepth"));
}

#[test]
fn nested_archive_shares_svg_sources_normalizes_fonts_pixels_and_preview() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24"><defs><pattern id="p" width="4" height="4" patternUnits="userSpaceOnUse"><path d="M0 0H2V4H0Z" fill="#28a"/></pattern><filter id="f"><feGaussianBlur stdDeviation=".1"/></filter></defs><g filter="url(#f)"><rect width="24" height="24" fill="url(#p)"/></g></svg>"##;
    let (mut child, notes) = crate::formats::svg::read(svg, "Child").unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    let pattern = child
        .layers
        .iter()
        .filter_map(|l| l.kind.shapes())
        .flatten()
        .find_map(|s| {
            if let crate::document::Fill::Pattern(p) = &s.style.fill {
                Some(p.clone())
            } else {
                None
            }
        })
        .unwrap();
    child.publishing.bleed = [1., 2., 3., 4.];
    let bytes = Arc::new(include_bytes!("../../tests/assets/fonts/EBGaramond.ttf").to_vec());
    let font = format!("omatype:{:032x}", crate::typography::fingerprint(&bytes));
    crate::text::register_memory_font(&font, "Child span font", bytes).unwrap();
    let mut text = Layer::vector("Child type");
    text.kind.shapes_mut().unwrap().push(Shape::new(
        Geom::Text(TypeRun {
            content: "AB".into(),
            px: 12.,
            spans: vec![CharSpan {
                start: 0,
                end: 2,
                publishing_style: Some("Child range".into()),
                font: Some(font.clone()),
                px: Some(14.),
                fill: Some(crate::document::Fill::Pattern(pattern.clone())),
                ..Default::default()
            }],
            ..Default::default()
        }),
        Style::default(),
    ));
    child.layers.push(text);
    let mut raster = Layer::raster("Straight pixels", 2, 2).unwrap();
    *raster.kind.pixels_mut().unwrap() = Pixels::from_rgba(2, 2, vec![90; 16]).unwrap();
    child.layers.push(raster);
    let mut root = plain();
    root.layers = vec![Layer::vector("Parent pattern")];
    root.layers[0].kind.shapes_mut().unwrap().push(Shape::new(
        Geom::Rect {
            origin: crate::geom::Pt::ZERO,
            size: crate::geom::Pt::splat(24.),
            radius: 0.,
        },
        Style {
            fill: crate::document::Fill::Pattern(pattern),
            stroke: None,
        },
    ));
    root.motion.compositions.push(Composition::new(child));
    let encoded = encode(&root).unwrap();
    let envelope: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(envelope["version"], 26);
    assert!(envelope["svg_sources"].as_object().unwrap().len() <= 3);
    let reopened = decode(&encoded).unwrap();
    check(&reopened, &font);
    let output = std::env::var_os("CRAFT_MOTION_COMMON_QA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap()
                .join(".artifacts/motion-common-archive")
        });
    std::fs::create_dir_all(&output).unwrap();
    let path = output.join("nested.oma");
    save_to(&root, &path).unwrap();
    check(&load_from(&path).unwrap(), &font);
    let motion_path = output.join("nested.omamotion");
    crate::motion::compositions::save_to(&root, &motion_path).unwrap();
    check(
        &crate::motion::compositions::load_from(&motion_path).unwrap(),
        &font,
    );
    check(
        &crate::brand::load_full_preview_document(&path, false).unwrap(),
        &font,
    );
    let mut missing = envelope;
    missing["svg_sources"] = serde_json::json!({});
    assert!(
        decode(&missing.to_string())
            .unwrap_err()
            .contains("Missing vector source")
    );
    fn check(doc: &Document, font: &str) {
        let child = &doc.motion.compositions[0].document;
        let px = child.layers.iter().find_map(|l| l.kind.pixels()).unwrap();
        assert_eq!(px.data.as_slice(), &[90; 16]);
        assert!(child.layers.iter().any(|l| l.svg_filter.is_some()));
        assert_eq!(child.publishing.bleed, [1., 2., 3., 4.]);
        let run = child
            .layers
            .iter()
            .filter_map(|l| l.kind.shapes())
            .flatten()
            .find_map(|s| {
                if let Geom::Text(t) = &s.geom {
                    Some(t)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(run.spans[0].font.as_deref(), Some(font));
        assert_eq!(run.spans[0].px, Some(14.));
        assert_eq!(
            run.spans[0].publishing_style.as_deref(),
            Some("Child range")
        );
        assert!(!run.contours.is_empty());
        let crate::document::Fill::Pattern(a) = &doc.layers[0].kind.shapes().unwrap()[0].style.fill
        else {
            panic!()
        };
        let Some(crate::document::Fill::Pattern(b)) = &run.spans[0].fill else {
            panic!()
        };
        assert!(Arc::ptr_eq(&a.definitions, &b.definitions));
        assert!(a.definitions.contains("<pattern"));
        crate::typography::validate_document_fonts(doc).unwrap();
    }
}

#[test]
fn common_motion_retained_feature_gates_scan_base_all_writes_and_nested_children() {
    use crate::collaboration::{Cell, Replica, snapshot};
    let empty = plain();
    let mut artwork = plain();
    artwork
        .motion
        .set_expression(7, Prop::X, Some("time".into()))
        .unwrap();
    for (gate, doc) in [
        (25, artwork.clone()),
        (26, {
            let mut d = plain();
            d.motion.set_key(7, Prop::X, 0., 1., Ease::Hold);
            d
        }),
        (26, {
            let mut d = plain();
            d.motion.set_key(
                7,
                Prop::X,
                0.,
                1.,
                Ease::CubicBezier {
                    x1: 0.,
                    y1: -2.,
                    x2: 1.,
                    y2: 3.,
                },
            );
            d
        }),
        (27, {
            let mut d = plain();
            let mut c = plain();
            c.motion.scene3d = Some(crate::scene3d::Scene::default());
            d.motion.compositions.push(Composition::new(c));
            d
        }),
    ] {
        for base in [false, true] {
            let mut replica = Replica::new(
                "feature-gates".into(),
                "a".repeat(64),
                snapshot(if base { &doc } else { &empty }).unwrap(),
            );
            if base {
                replica.record(&empty).unwrap();
            } else {
                replica.record(&doc).unwrap();
                assert!(replica.undo());
            }
            let mut current = replica.document(&empty).unwrap();
            assert!(
                current.motion.compositions.is_empty()
                    && current.motion.scene3d.is_none()
                    && current.motion.expressions.is_empty()
            );
            current.collaboration = Some(replica);
            assert_eq!(features::required_version(&current), gate);
            let text = encode(&current).unwrap();
            let mut envelope: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!(envelope["version"], gate);
            decode(&text).unwrap();
            envelope["version"] = (gate - 1).into();
            assert!(
                decode(&envelope.to_string())
                    .unwrap_err()
                    .contains(&format!("requires version {gate}"))
            );
            envelope["version"] = (VERSION + 1).into();
            envelope["doc"] = serde_json::json!({"not_a_typed_document":true});
            assert!(
                decode(&envelope.to_string())
                    .unwrap_err()
                    .contains("Unsupported")
            );
        }
    }
    let mut replica = Replica::new(
        "variant-gates".into(),
        "a".repeat(64),
        snapshot(&empty).unwrap(),
    );
    replica.base.insert(
        "/motion/tracks/0/keys/0/ease".into(),
        Cell::Variant("Hold".into()),
    );
    let mut doc = empty;
    doc.collaboration = Some(replica);
    assert_eq!(features::required_version(&doc), 26);
}

#[test]
fn admitted_motion_envelope_uses_the_native_writer_and_preserves_output_owner_on_cancel() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Cancel(AtomicUsize);
    impl crate::ml::Progress for Cancel {
        fn report(&self, _: &str, _: usize, _: usize) {}
        fn cancelled(&self) -> bool {
            self.0.fetch_add(1, Ordering::SeqCst) > 0
        }
    }
    let mut doc = plain();
    doc.motion.compositions.push(Composition::new(plain()));
    let native = encode_admitted(&doc).unwrap();
    let portable = crate::motion::compositions::export_admitted(&doc).unwrap();
    let native_json: serde_json::Value = serde_json::from_slice(native.as_slice()).unwrap();
    let portable_json: serde_json::Value = serde_json::from_slice(portable.as_slice()).unwrap();
    assert_eq!(portable_json["format"], "omadesign-motion");
    assert_eq!(portable_json["version"], 1);
    assert_eq!(portable_json["project"], native_json);
    let lease = portable.lease().clone();
    assert!(lease.bytes() >= portable.len() as u64);
    let root = std::env::current_dir()
        .unwrap()
        .join(".artifacts")
        .join(format!("motion-owner-{}", crate::document::next_id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("keep.omamotion");
    std::fs::write(&path, b"accepted destination").unwrap();
    assert!(
        save_encoded_to(&doc, &path, &portable, &Cancel(AtomicUsize::new(0)))
            .unwrap_err()
            .contains("Cancelled")
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"accepted destination");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    assert!(Arc::ptr_eq(&lease, portable.lease()));
    save_encoded_to(&doc, &path, &portable, &crate::ml::NoProgress).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), portable.as_slice());
    assert!(Arc::ptr_eq(&lease, portable.lease()));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn portable_motion_cap_refuses_before_replacing_the_destination() {
    let mut doc = plain();
    doc.name = "x".repeat(64 * 1024 * 1024);
    let root = std::env::current_dir()
        .unwrap()
        .join(".artifacts")
        .join(format!("motion-cap-{}", crate::document::next_id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("bounded.omamotion");
    std::fs::write(&path, b"accepted destination").unwrap();
    assert!(
        crate::motion::compositions::save_to(&doc, &path)
            .unwrap_err()
            .contains("64 MiB")
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"accepted destination");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn common_native_child_motion_refuses_the_rgba8_export_boundary() {
    let mut doc = crate::collaboration::native_tests::fixture();
    for _ in 0..crate::motion::compositions::MAX_DEPTH {
        let mut parent = plain();
        parent.motion.compositions.push(Composition::new(doc));
        doc = parent;
    }
    let source = encode(&doc).unwrap();
    for result in [
        crate::compositor::render_export(&doc, 1),
        crate::compositor::render_export_at(&doc, 1., 0., true),
    ] {
        assert!(result.unwrap_err().contains("precision"));
    }
    assert_eq!(encode(&doc).unwrap(), source);
    let reopened = decode(&source).unwrap();
    let original = crate::motion::compositions::documents(&doc).unwrap();
    let decoded = crate::motion::compositions::documents(&reopened).unwrap();
    crate::collaboration::native_tests::assert_native_equal(
        original.last().unwrap(),
        decoded.last().unwrap(),
    );
}

#[test]
fn layer_only_exchange_guard_refuses_complex_motion_and_keeps_flat_design_routes() {
    let mut doc = plain();
    doc.motion.set_key(7, Prop::X, 0., 1., Ease::Hold);
    doc.motion
        .set_expression(7, Prop::Y, Some("time".into()))
        .unwrap();
    crate::motion::require_flat_artwork(&doc, "Flat writer").unwrap();
    doc.motion.compositions.push(Composition::new(plain()));
    assert!(
        crate::motion::require_flat_artwork(&doc, "Flat writer")
            .unwrap_err()
            .contains("MotionUnsupported")
    );
    doc.motion.compositions.clear();
    doc.motion.scene3d = Some(crate::scene3d::Scene::default());
    assert!(
        crate::motion::require_flat_artwork(&doc, "Flat writer")
            .unwrap_err()
            .contains("MotionUnsupported")
    );
}

#[test]
fn streaming_motion_bounds_refuse_current_and_retained_trees_before_typed_fields() {
    use serde_json::json;
    let mut deep = json!({});
    for _ in 0..9 {
        deep = json!({"motion":{"compositions":[{"document":deep}]}});
    }
    let wide = json!({"motion":{"compositions":vec![json!({"document":{}});129]}});
    for (document, error) in [(deep, "CompositionDepth"), (wide, "CompositionLimit")] {
        let current = json!({"version":27,"doc":document});
        assert!(decode(&current.to_string()).unwrap_err().contains(error));
        let retained = json!({"version":27,"doc":{"collaboration":{"base":{"/motion/compositions":{"Value":current["doc"]["motion"]["compositions"]}}}}});
        assert!(decode(&retained.to_string()).unwrap_err().contains(error));
    }
    let state = json!({"Value":vec![json!({"document":{}});128]});
    let multiple_states = json!({"version":27,"doc":{"collaboration":{"base":{"/motion/compositions":state},"changes":[{"edit":{"type":"write","writes":[{"path":"/motion/compositions","value":state}]}}]}}});
    validate_retained_json(&multiple_states.to_string()).unwrap();
}

/// Build supported flat artwork with authored rest, expressions and advanced keys.
/// Args: none. Returns a small unprofiled U8 document with one ordinary vector shape.
fn flat_exchange_fixture() -> Document {
    let mut doc = plain();
    let mut layer = Layer::vector("Rest artwork");
    let shape = Shape::new(
        Geom::Rect {
            origin: crate::geom::Pt::new(2., 3.),
            size: crate::geom::Pt::splat(12.),
            radius: 0.,
        },
        Style {
            fill: crate::document::Fill::Solid(crate::color::Rgba::rgb(20, 90, 160)),
            stroke: None,
        },
    );
    let id = shape.id;
    layer.kind.shapes_mut().unwrap().push(shape);
    doc.layers.push(layer);
    doc.motion.set_key(id, Prop::X, 0., 0., Ease::Hold);
    doc.motion.set_key(
        id,
        Prop::X,
        1.,
        5.,
        Ease::CubicBezier {
            x1: 0.1,
            y1: -2.,
            x2: 0.9,
            y2: 3.,
        },
    );
    doc.motion
        .set_expression(id, Prop::Y, Some("time * 2".into()))
        .unwrap();
    doc
}

#[test]
fn common_flat_u8_writer_entries_allow_rest_expressions_and_advanced_keys() {
    let doc = flat_exchange_fixture();
    assert!(!crate::deep_color::has_native(&doc));
    let before = serde_json::to_value(&doc).unwrap();
    let pdf = crate::formats::pdf::write(&doc).unwrap().0;
    assert_eq!(
        lopdf::Document::load_mem(&pdf).unwrap().get_pages().len(),
        1
    );
    let idml = crate::formats::idml::write(&doc).unwrap().0;
    let reopened = crate::formats::idml::read(&idml, "Flat Motion rest")
        .unwrap()
        .0;
    assert!(
        reopened
            .layers
            .iter()
            .flat_map(|layer| layer.kind.shapes().unwrap_or_default())
            .any(|shape| shape.style.fill == doc.layers[0].kind.shapes().unwrap()[0].style.fill)
    );
    let pixels = crate::deep_color::composite(&doc, 1).unwrap();
    let direct =
        crate::deep_color::composite_with_cancel(&doc, 1, &crate::deep_color::storage::check)
            .unwrap();
    assert_eq!(
        pixels.native.as_ref().unwrap().samples,
        direct.native.as_ref().unwrap().samples
    );
    assert_eq!((pixels.w, pixels.h), (24, 24));
    assert!(
        pixels
            .native
            .as_ref()
            .unwrap()
            .samples
            .chunks_exact(4)
            .any(|rgba| rgba[3] == 1.)
    );
    assert_eq!(serde_json::to_value(&doc).unwrap(), before);
}

#[test]
fn common_writer_entries_refuse_compositions_and_scenes_without_source_mutation() {
    for scene in [false, true] {
        let mut doc = flat_exchange_fixture();
        if scene {
            doc.motion.scene3d = Some(crate::scene3d::Scene::default());
        } else {
            doc.motion.compositions.push(Composition::new(plain()));
        }
        let before = serde_json::to_value(&doc).unwrap();
        for (writer, result) in [
            ("PDF", crate::formats::pdf::write(&doc).map(|_| ())),
            ("IDML", crate::formats::idml::write(&doc).map(|_| ())),
            (
                "Native float compositor",
                crate::deep_color::composite(&doc, 1).map(|_| ()),
            ),
            (
                "Native float compositor",
                crate::deep_color::composite_with_cancel(
                    &doc,
                    1,
                    &crate::deep_color::storage::check,
                )
                .map(|_| ()),
            ),
        ] {
            let error = result.unwrap_err();
            assert!(
                error.starts_with(&format!("MotionUnsupported: {writer}")),
                "{error}"
            );
        }
        assert_eq!(serde_json::to_value(&doc).unwrap(), before);
        assert_eq!(
            crate::deep_color::composite_with_cancel(&doc, 1, &|| Err("Cancelled".to_string()))
                .unwrap_err(),
            "Cancelled"
        );
        crate::compositor::render_export_preview(&doc, 32)
            .expect("RGBA8 previews remain available");
        assert_eq!(serde_json::to_value(&doc).unwrap(), before);
    }
}

#[test]
fn common_native_pdf_error_remains_first_and_float_export_refuses_complex_motion() {
    const PDF_ERROR: &str = "PDF cannot preserve native pixel or mask samples yet. Export PSD, TIFF or an explicit 8-bit document copy.";
    for mode in ["flat", "composition", "scene"] {
        let mut doc = flat_exchange_fixture();
        doc.color.depth = crate::deep_color::Depth::F32;
        if mode == "composition" {
            doc.motion.compositions.push(Composition::new(plain()));
        }
        if mode == "scene" {
            doc.motion.scene3d = Some(crate::scene3d::Scene::default());
        }
        let before = serde_json::to_value(&doc).unwrap();
        assert_eq!(crate::formats::pdf::write(&doc).unwrap_err(), PDF_ERROR);
        let result = crate::export::encode_admitted(
            &crate::export::Source::Document(doc.clone()),
            crate::export::Settings {
                format: crate::export::Format::Tiff,
                scale: 1,
                ai: None,
            },
            &crate::ml::NoProgress,
        );
        if mode == "flat" {
            let bytes = result.unwrap().0;
            let pixels = crate::deep_color::tiff::decode(bytes.as_slice())
                .unwrap()
                .unwrap();
            assert_eq!((pixels.w, pixels.h), (24, 24));
            assert_eq!(
                pixels.native.as_ref().unwrap().depth,
                crate::deep_color::Depth::F32
            );
        } else {
            assert!(
                result
                    .unwrap_err()
                    .contains("MotionUnsupported: Native float compositor")
            );
        }
        assert_eq!(serde_json::to_value(&doc).unwrap(), before);
    }
}
