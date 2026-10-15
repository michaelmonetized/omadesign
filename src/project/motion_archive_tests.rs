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
fn motion_feature_versions_keep_parent_guards_holes_and_nested_highest_feature() {
    let mut d = plain();
    for v in [18, 19, 20, 21, 22, 24, 28] {
        assert!(!supports_version(v));
    }
    for v in (1..=17).chain([23, 25, 26, 27]) {
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
    let mut raster = Layer::raster("Straight pixels", 2, 2);
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
        assert_eq!(px.data, vec![90; 16]);
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
