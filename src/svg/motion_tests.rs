use super::*;
use crate::{
    color::Rgba,
    document::{Stroke, Style},
    gradient::{Gradient, GradientKind},
    motion::{Ease, Prop},
};

pub(crate) fn fixture(case: &str) -> Document {
    let mut doc = Document::new(case, 200., 150., 96.);
    doc.layers.clear();
    doc.artboards.clear();
    doc.artboardless = true;
    doc.transparent = true;
    doc.motion.duration = 4.;
    doc.motion.fps = 30.;
    doc.motion.looped = false;
    let mut g = Gradient::new(
        GradientKind::Radial,
        Rgba::rgb(20, 40, 200),
        Rgba::rgb(240, 180, 40),
    );
    g.from = [85. / 180., 55. / 120.];
    g.to = [165. / 180., 80. / 120.];
    g.radial_axis = Some(if case == "reflection" {
        [-30. / 180., -40. / 120.]
    } else {
        [30. / 180., 40. / 120.]
    });
    g.focal = [0.2, -0.1];
    g.inner_radius = 0.12;
    if case == "legacy" {
        g.radial_axis = None;
        g.focal = [0.; 2];
        g.inner_radius = 0.;
    }
    if case == "singular" {
        g.radial_axis = Some([0.; 2]);
    }
    if case == "subunit" {
        g.to = [85.5 / 180., 55. / 120.];
    }
    let mut shape = Shape::new(
        Geom::Rect {
            origin: Pt::new(10., 10.),
            size: Pt::new(180., 120.),
            radius: 0.,
        },
        Style {
            fill: if case == "stroke" {
                Fill::None
            } else {
                Fill::Gradient(g.clone())
            },
            stroke: None,
        },
    );
    if case == "stroke" || case == "both" {
        shape.style.stroke = Some(Stroke {
            width: 12.,
            gradient: Some(g),
            ..Stroke::default()
        });
    }
    shape.name = "Swatch".into();
    let id = shape.id;
    let mut layer = crate::document::Layer::vector("Art");
    layer.kind.shapes_mut().unwrap().push(shape);
    doc.layers.push(layer);
    if case == "expression" {
        doc.motion
            .set_expression(id, Prop::GradientAngle, Some("time * 90".into()))
            .unwrap();
    } else {
        doc.motion
            .set_key(id, Prop::GradientAngle, 0., 0., Ease::Linear);
        doc.motion
            .set_key(id, Prop::GradientAngle, 4., 360., Ease::Linear);
    }
    doc
}

#[test]
fn affine_gradient_motion_preserves_field_factors_and_retains_browser_fixtures() {
    let root = std::env::var_os("CRAFT_MOTION_COMMON_QA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap()
                .join(".artifacts/motion-common-output")
        });
    std::fs::create_dir_all(&root).unwrap();
    let mut cases = Vec::new();
    for case in [
        "fill",
        "stroke",
        "both",
        "reflection",
        "expression",
        "legacy",
    ] {
        let doc = fixture(case);
        let report = animated_export(&doc).unwrap();
        assert!(report.warnings.is_empty(), "{case}: {:?}", report.warnings);
        let xml = roxmltree::Document::parse(&report.svg).unwrap();
        let gradients: Vec<_> = xml
            .descendants()
            .filter(|n| n.tag_name().name() == "radialGradient")
            .collect();
        assert_eq!(gradients.len(), if case == "both" { 2 } else { 1 });
        for gradient in gradients {
            let animations: Vec<_> = gradient
                .children()
                .filter(|n| n.tag_name().name() == "animateTransform")
                .collect();
            assert_eq!(animations.len(), if case == "legacy" { 1 } else { 5 });
            assert!(
                animations[0]
                    .attribute("values")
                    .unwrap()
                    .split(';')
                    .any(|v| v.starts_with("360."))
            );
            if case != "legacy" {
                for (name, value) in [
                    ("cx", "0"),
                    ("cy", "0"),
                    ("r", "1"),
                    ("fx", "0.2"),
                    ("fy", "-0.1"),
                    ("fr", "0.12"),
                ] {
                    assert_eq!(gradient.attribute(name), Some(value));
                }
                assert!(
                    animations
                        .iter()
                        .skip(1)
                        .all(|n| n.attribute("additive") == Some("sum"))
                );
            }
        }
        std::fs::write(root.join(format!("{case}.svg")), &report.svg).unwrap();
        crate::project::save_to(&doc, &root.join(format!("{case}.oma"))).unwrap();
        let mut matrices = Vec::new();
        for time in [0., 1., 2., 4.] {
            let native = crate::compositor::render_export_at(&doc, 1., time, true).unwrap();
            native
                .save_png(root.join(format!("{case}-{time:.0}-native.png")))
                .unwrap();
            let mut shape = doc.layers[0].kind.shapes().unwrap()[0].clone();
            let pose = doc.motion.pose(shape.id, time);
            crate::compositor::posed_paint(&mut shape, pose);
            let g = paint_gradient(
                &shape,
                if case == "stroke" {
                    SpinTarget::Stroke
                } else {
                    SpinTarget::Fill
                },
            )
            .unwrap();
            let m = g.radial_transform(shape.geom.bbox());
            if case == "both" {
                let stroke = paint_gradient(&shape, SpinTarget::Stroke).unwrap();
                assert_eq!(stroke.radial_transform(shape.geom.bbox()), m);
            }
            matrices
                .push(serde_json::json!({"time":time,"matrix":[m.sx,m.ky,m.kx,m.sy,m.tx,m.ty]}));
        }
        cases.push(serde_json::json!({"case":case,"samples":matrices}));
    }
    std::fs::write(
        root.join("svg-native-reference.json"),
        serde_json::to_vec_pretty(&cases).unwrap(),
    )
    .unwrap();
}

#[test]
fn singular_and_expression_only_reductions_reach_report_and_legacy_errors() {
    let mut doc = fixture("singular");
    let id = doc.layers[0].kind.shapes().unwrap()[0].id;
    doc.motion.tracks.clear();
    doc.motion
        .set_expression(id, Prop::GradientAngle, Some("time * 90".into()))
        .unwrap();
    let report = animated_export(&doc).unwrap();
    assert!(
        report
            .warnings
            .iter()
            .any(|v| v.contains("singular") && v.contains("Swatch") && v.contains("Art")),
        "{:?}",
        report.warnings
    );
    assert!(export_animated(&doc).unwrap_err().contains("singular"));
    let subunit = fixture("subunit");
    assert!(
        animated_export(&subunit)
            .unwrap()
            .warnings
            .iter()
            .any(|w| w.contains("sub-unit") && w.contains("Swatch") && w.contains("Art"))
    );
    assert!(export_animated(&subunit).unwrap_err().contains("sub-unit"));
    doc.motion
        .set_expression(id, Prop::GradientAngle, Some("1 / 0".into()))
        .unwrap();
    assert!(
        animated_export(&doc)
            .err()
            .unwrap()
            .contains("DivideByZero")
    );
}
