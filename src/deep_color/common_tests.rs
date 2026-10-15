use super::*;
use crate::document::{Document, Fill, Layer, Shape, Style};
use crate::geom::{CharSpan, Geom, Pt, TypeRun};

#[test]
fn common_native_svg_graph_refusal_precedes_group_raster_vector_dispatch() {
    let (imported, _) = crate::formats::svg::read(r##"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><defs><filter id="f"><feGaussianBlur stdDeviation="1"/></filter></defs><g filter="url(#f)"><rect width="2" height="2"/></g></svg>"##, "Graph guard").unwrap();
    let graph = imported
        .layers
        .iter()
        .find_map(|layer| layer.svg_filter.clone())
        .unwrap();
    assert!(!graph.filters.is_empty());
    for kind in 0..3 {
        let mut doc = Document::new("Native graph guard", 2., 2., 72.);
        doc.color.depth = Depth::F32;
        let mut layer = match kind {
            0 => Layer::group("Group graph"),
            1 => Layer::placed_raster(
                "Raster graph",
                Pixels::from_samples(1, 1, Depth::F32, vec![1.25, 0.123456, -0.1, 0.75]).unwrap(),
                Pt::ZERO,
                Pt::splat(2.),
            ),
            _ => {
                let mut layer = Layer::vector("Vector graph");
                layer.kind.shapes_mut().unwrap().push(Shape::new(
                    Geom::Rect {
                        origin: Pt::ZERO,
                        size: Pt::splat(2.),
                        radius: 0.,
                    },
                    Style::default(),
                ));
                layer
            }
        };
        layer.svg_filter = Some(graph.clone());
        doc.layers = vec![layer];
        let before = serde_json::to_string(&doc).unwrap();
        assert!(
            composite(&doc, 1)
                .unwrap_err()
                .contains("float effect renderer")
        );
        assert_eq!(serde_json::to_string(&doc).unwrap(), before);
        let restored = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
        assert_eq!(
            restored.layers[0].svg_filter.as_ref().unwrap().filters,
            graph.filters
        );
        assert!(
            composite(&restored, 1)
                .unwrap_err()
                .contains("float effect renderer")
        );
    }
}

#[test]
fn common_native_rich_span_paint_refuses_precision_reduction_without_losing_style() {
    let mut doc = Document::new("Rich paint guard", 8., 8., 72.);
    doc.color.depth = Depth::U16;
    let mut layer = Layer::vector("Rich text");
    let paint = Fill::Pattern(crate::svg_source::SvgPattern {
        definitions: Arc::from("<invalid-pattern-source"),
        id: "p".into(),
        transform: [1., 0., 0., 1., 0., 0.],
        opacity: 1.,
    });
    layer.kind.shapes_mut().unwrap().push(Shape::new(
        Geom::Text(TypeRun {
            content: "AB".into(),
            px: 12.,
            spans: vec![CharSpan {
                start: 0,
                end: 2,
                font: Some("sans-serif".into()),
                px: Some(18.),
                fill: Some(paint),
                publishing_style: Some("Retained style".into()),
                ..Default::default()
            }],
            ..Default::default()
        }),
        Style::default(),
    ));
    doc.layers = vec![layer];
    assert!(
        composite(&doc, 1)
            .unwrap_err()
            .contains("float paint renderer")
    );
    let restored = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    let Geom::Text(run) = &restored.layers[0].kind.shapes().unwrap()[0].geom else {
        panic!("text lost")
    };
    assert_eq!(
        run.spans[0].publishing_style.as_deref(),
        Some("Retained style")
    );
    assert_eq!(run.spans[0].px, Some(18.));
    assert!(matches!(run.spans[0].fill, Some(Fill::Pattern(_))));
    assert!(
        composite(&restored, 1)
            .unwrap_err()
            .contains("float paint renderer")
    );
    assert!(
        crate::compositor::render_export(&restored, 1)
            .unwrap_err()
            .contains("precision")
    );
}
