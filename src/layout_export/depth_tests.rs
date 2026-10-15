use super::*;
use crate::document::{Layer, Style};
use crate::layout::{LayoutBreakpoint, make_frame};
use crate::layout_prototype::{Interaction, PrototypeAction as Action, Transition, Trigger};

fn interaction(action: Action) -> Interaction {
    Interaction {
        action,
        trigger: Trigger::Click,
        transition: Transition::Instant,
        duration_ms: 0,
    }
}
fn button(
    doc: &mut Document,
    layer: usize,
    parent: u64,
    name: &str,
    action: Action,
    x: f32,
    y: f32,
) -> u64 {
    let mut node = crate::document::Shape::new(
        Geom::Rect {
            origin: Pt::new(x, y),
            size: Pt::new(72., 24.),
            radius: 0.,
        },
        Style::default(),
    );
    node.name = name.into();
    node.layout.parent = Some(parent);
    node.layout.interactions.push(interaction(action));
    let id = node.id;
    doc.layers[layer].kind.shapes_mut().unwrap().push(node);
    id
}
fn graph() -> (Document, u64, u64, u64, u64) {
    let mut doc = Document::new("Prototype C/L/P/V", 600., 400., 96.);
    doc.layers = vec![
        Layer::vector("A"),
        Layer::vector("B"),
        Layer::vector("Overlays"),
    ];
    let mut a = make_frame(Pt::ZERO, Pt::new(600., 400.));
    a.name = "Screen A".into();
    let mut b = make_frame(Pt::new(700., 0.), Pt::new(600., 400.));
    b.name = "Screen B".into();
    let mut o1 = make_frame(Pt::new(0., 500.), Pt::new(240., 160.));
    o1.name = "Overlay one".into();
    let mut o2 = make_frame(Pt::new(300., 500.), Pt::new(160., 100.));
    o2.name = "Overlay two".into();
    let ids = (a.id, b.id, o1.id, o2.id);
    doc.layers[0].kind.shapes_mut().unwrap().push(a);
    doc.layers[1].kind.shapes_mut().unwrap().push(b);
    doc.layers[2].kind.shapes_mut().unwrap().extend([o1, o2]);
    let go = button(
        &mut doc,
        0,
        ids.0,
        "Go B",
        Action::Navigate { target: ids.1 },
        10.,
        10.,
    );
    doc.find_shape_mut(0, go).unwrap().layout.interactions[0].transition = Transition::SlideLeft;
    doc.find_shape_mut(0, go).unwrap().layout.interactions[0].duration_ms = 5000;
    button(
        &mut doc,
        0,
        ids.0,
        "Open one",
        Action::OpenOverlay { target: ids.2 },
        10.,
        50.,
    );
    button(
        &mut doc,
        0,
        ids.0,
        "Self",
        Action::Navigate { target: ids.0 },
        10.,
        90.,
    );
    button(&mut doc, 1, ids.1, "Back A", Action::Back, 710., 10.);
    button(&mut doc, 0, ids.0, "Back root", Action::Back, 10., 130.);
    button(
        &mut doc,
        2,
        ids.2,
        "Open two",
        Action::OpenOverlay { target: ids.3 },
        10.,
        510.,
    );
    button(
        &mut doc,
        2,
        ids.2,
        "Close one",
        Action::CloseOverlay,
        10.,
        550.,
    );
    button(&mut doc, 2, ids.3, "Back one", Action::Back, 310., 510.);
    let mut private = make_frame(Pt::new(900., 900.), Pt::new(100., 100.));
    private.name = "PRIVATE SECRET".into();
    doc.layers[2].kind.shapes_mut().unwrap().push(private);
    (doc, ids.0, ids.1, ids.2, ids.3)
}
#[test]
fn document_wide_reachable_graph_reopens_without_private_siblings_and_invalid_links_fail() {
    let (doc, a, b, o1, o2) = graph();
    let before = crate::project::encode(&doc).unwrap();
    let reopened = crate::project::decode(&before).unwrap();
    let html = export_html(&reopened, 0, a).unwrap();
    for id in [a, b, o1, o2] {
        assert!(html.contains(&format!("id=\"screen-{id}\"")));
    }
    assert!(!html.contains("PRIVATE SECRET"));
    assert_eq!(html.matches("<template ").count(), 4);
    assert_eq!(html, export_html(&reopened, 0, a).unwrap());
    assert_eq!(before, crate::project::encode(&doc).unwrap());
    let mut broken = reopened;
    broken
        .find_shape_mut(0, a)
        .unwrap()
        .layout
        .interactions
        .push(interaction(Action::Navigate { target: u64::MAX }));
    assert!(
        export_html(&broken, 0, a)
            .unwrap_err()
            .contains("Invalid prototype link")
    );
}

#[test]
fn native_present_navigation_changes_canvas_size_without_flattening_transition() {
    let (mut doc, a, b, _, _) = graph();
    if let Geom::Rect { size, .. } = &mut doc.find_shape_mut(1, b).unwrap().geom {
        size.y = 220.;
    }
    let go = doc.layers[0]
        .kind
        .shapes()
        .unwrap()
        .iter()
        .find(|s| s.name == "Go B")
        .unwrap()
        .id;
    let mut state = crate::layout_prototype::PrototypeState::default();
    state.start(&doc, a).unwrap();
    let (_, before) = crate::ui::layout_presentation(&doc, &state, 600.).unwrap();
    assert!(state.activate(&doc, go, Trigger::Click).unwrap());
    let (_, after) = crate::ui::layout_presentation(&doc, &state, 600.).unwrap();
    assert_eq!((before.width, before.height), (600., 400.));
    assert_eq!((after.width, after.height), (600., 220.));
    assert_eq!(state.last_transition, Transition::SlideLeft);
    assert_eq!(state.duration_ms, 5000);
    if let Ok(directory) = std::env::var("OMADESIGN_LAYOUT_PROOF") {
        let path = std::path::Path::new(&directory);
        std::fs::write(
            path.join("transition.html"),
            export_html(&doc, 0, a).unwrap(),
        )
        .unwrap();
        std::fs::write(path.join("transition-expected.json"), serde_json::json!({
            "kind":"actual native Present materialized canvas sizes; native UI stretches old texture into new canvas rectangle",
            "before":[before.width,before.height],"after":[after.width,after.height],
            "transition":"SlideLeft","duration":5000
        }).to_string()).unwrap();
    }
}

#[test]
fn native_present_canvas_clips_an_oversized_overlay_without_changing_the_document() {
    let (mut doc, a, b, overlay, _) = graph();
    crate::layout::set_bounds(
        &mut doc.find_shape_mut(2, overlay).unwrap().geom,
        crate::geom::Bounds::from_min_size(Pt::new(0., 500.), Pt::new(800., 160.)),
    );
    let outside = button(
        &mut doc,
        2,
        overlay,
        "Outside canvas",
        Action::Navigate { target: b },
        700.,
        510.,
    );
    doc.find_shape_mut(2, outside).unwrap().style.fill =
        Fill::Solid(crate::color::Rgba::from_hex(0xfa00dd));
    let doc = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    let before = crate::project::encode(&doc).unwrap();
    let open = doc.layers[0]
        .kind
        .shapes()
        .unwrap()
        .iter()
        .find(|s| s.name == "Open one")
        .unwrap()
        .id;
    let mut state = crate::layout_prototype::PrototypeState::default();
    state.start(&doc, a).unwrap();
    state.activate(&doc, open, Trigger::Click).unwrap();
    let (_, canvas) = crate::ui::layout_presentation(&doc, &state, 600.).unwrap();
    assert_eq!((canvas.width, canvas.height), (600., 400.));
    let pixels = crate::compositor::render_export(&canvas, 1).unwrap();
    assert_eq!((pixels.width(), pixels.height()), (600, 400));
    let outside_bounds = canvas
        .layers
        .iter()
        .filter_map(|layer| layer.kind.shapes())
        .flatten()
        .find(|shape| shape.id == outside)
        .unwrap()
        .geom
        .bbox();
    assert!(outside_bounds.min.x >= pixels.width() as f32);
    let outside_sentinel_pixels = pixels
        .pixels()
        .iter()
        .filter(|p| p.red() == 250 && p.green() == 0 && p.blue() == 221)
        .count();
    assert_eq!(outside_sentinel_pixels, 0);
    assert_eq!(before, crate::project::encode(&doc).unwrap());
    if let Ok(directory) = std::env::var("OMADESIGN_LAYOUT_PROOF") {
        let path = std::path::Path::new(&directory);
        std::fs::write(
            path.join("wide-overlay.html"),
            export_html(&doc, 0, a).unwrap(),
        )
        .unwrap();
        std::fs::write(path.join("headless-present.json"),serde_json::json!({"kind":"actual native Present materialization and CPU compositor; not visible native or physical acceptance","canvas":{"width":canvas.width,"height":canvas.height},"overlays":state.overlays,"outside":outside,"outsideArtworkX":outside_bounds.min.x,"outsideSentinelPixelCount":outside_sentinel_pixels,"sourceUnchanged":true}).to_string()).unwrap();
    }
}
#[test]
fn conflicting_variant_rules_are_scoped_to_emitted_state_and_export_deterministically() {
    let (mut doc, a, _, _, _) = graph();
    let mut main = make_frame(Pt::new(0., 1000.), Pt::new(180., 80.));
    main.layout.stack = Some(crate::layout::AutoStack::default());
    main.layout.breakpoints.push(LayoutBreakpoint {
        max_width: 600.,
        gap: Some(7.),
        ..Default::default()
    });
    let mid = main.id;
    doc.layers[2].kind.shapes_mut().unwrap().push(main);
    let mut label = Shape::new(
        Geom::Text(crate::geom::TypeRun {
            content: "Café Ω".into(),
            px: 26.,
            font: concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/assets/fonts/EBGaramond.ttf"
            )
            .into(),
            origin: Pt::new(10., 1030.),
            ..Default::default()
        }),
        Style::default(),
    );
    let label_id = label.id;
    label.name = "Responsive label".into();
    label.layout.parent = Some(mid);
    label.layout.width = Sizing::Hug;
    label.layout.height = Sizing::Hug;
    label.layout.breakpoints = vec![LayoutBreakpoint {
        max_width: 600.,
        text_size: Some(22.),
        ..Default::default()
    }];
    crate::text::fill_contours(&mut label.geom);
    doc.layers[2].kind.shapes_mut().unwrap().push(label);
    crate::layout_components::make_component(&mut doc, 2, mid).unwrap();
    let v1 = crate::layout_components::add_variant(&mut doc, mid, "Compact", Pt::new(250., 1000.))
        .unwrap();
    let v2 =
        crate::layout_components::add_variant(&mut doc, mid, "Wide", Pt::new(500., 1000.)).unwrap();
    for (id, gap, axis) in [
        (v1, 31., StackAxis::Horizontal),
        (v2, 53., StackAxis::Vertical),
    ] {
        let shape = doc.find_shape_mut(2, id).unwrap();
        shape.layout.breakpoints = vec![LayoutBreakpoint {
            max_width: 600.,
            gap: Some(gap),
            direction: Some(axis),
            ..Default::default()
        }];
    }
    for (variant, px) in [(v1, 18.), (v2, 34.)] {
        let target = crate::layout::descendants(&doc, 2, variant)
            .into_iter()
            .find(|id| matches!(doc.find_shape(2, *id).unwrap().geom, Geom::Text(_)))
            .unwrap();
        doc.find_shape_mut(2, target).unwrap().layout.breakpoints = vec![LayoutBreakpoint {
            max_width: 600.,
            text_size: Some(px),
            ..Default::default()
        }];
    }
    let property = crate::layout_components::expose_property(
        &mut doc,
        2,
        mid,
        label_id,
        "Label",
        crate::layout_components::PropertyKind::Text,
    )
    .unwrap();
    let instance =
        crate::layout_components::insert_instance(&mut doc, mid, 0, Pt::new(120., 100.), Some(a))
            .unwrap();
    doc.find_shape_mut(0, instance).unwrap().layout.interactions = vec![
        Interaction {
            transition: Transition::SlideRight,
            duration_ms: 900,
            ..interaction(Action::SetVariant { component: v1 })
        },
        Interaction {
            trigger: Trigger::Hover,
            transition: Transition::Dissolve,
            duration_ms: 700,
            ..interaction(Action::SetVariant { component: v2 })
        },
    ];
    let other =
        crate::layout_components::insert_instance(&mut doc, mid, 0, Pt::new(330., 100.), Some(a))
            .unwrap();
    crate::layout_components::set_property(
        &mut doc,
        0,
        instance,
        property,
        crate::layout_components::PropertyValue::Text("Local café Ω".into()),
    )
    .unwrap();
    let label_instance =
        crate::layout_components::property_object(&doc, 0, instance, property).unwrap();
    let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    let mut state = crate::layout_prototype::PrototypeState::default();
    state.start(&reopened, a).unwrap();
    state.activate(&reopened, instance, Trigger::Click).unwrap();
    assert_eq!(state.active_variants.get(&instance), Some(&v1));
    for name in ["Go B", "Back A"] {
        let source = reopened
            .layers
            .iter()
            .filter_map(|l| l.kind.shapes())
            .flatten()
            .find(|s| s.name == name)
            .unwrap()
            .id;
        state.activate(&reopened, source, Trigger::Click).unwrap();
    }
    assert_eq!(state.active_variants.get(&instance), Some(&v1));
    state.activate(&reopened, instance, Trigger::Hover).unwrap();
    assert_eq!(state.active_variants.get(&instance), Some(&v2));
    state.clear_hover();
    assert_eq!(state.active_variants.get(&instance), Some(&v1));
    assert!(!state.activate(&reopened, instance, Trigger::Click).unwrap());
    let preview = state.preview_document(&reopened).unwrap();
    let Geom::Text(label) = &preview.find_shape(0, label_instance).unwrap().geom else {
        panic!()
    };
    assert_eq!(label.content, "Local café Ω");
    assert_eq!(label.px, 18.);
    let html = export_html(&reopened, 0, a).unwrap();
    assert_eq!(html, export_html(&reopened, 0, a).unwrap());
    assert!(html.contains(&format!(
        "[data-state=\"variant-screen-{a}-{instance}-{v1}\"][data-node=\"{instance}\"]"
    )));
    assert!(!html.contains(&format!("{{[data-node=\"{instance}\"]")));
    assert!(!html.contains(&format!("id=\"screen-{v1}\"")));
    let mut compound = reopened.layout_snapshot();
    let actions = &mut compound
        .find_shape_mut(0, instance)
        .unwrap()
        .layout
        .interactions;
    let mut first = actions[0].clone();
    first.action = Action::SetVariant { component: v2 };
    actions.insert(0, first);
    let mut ordered = crate::layout_prototype::PrototypeState::default();
    ordered.start(&compound, a).unwrap();
    ordered
        .activate(&compound, instance, Trigger::Click)
        .unwrap();
    assert_eq!(ordered.active_variants.get(&instance), Some(&v1));
    if let Ok(directory) = std::env::var("OMADESIGN_LAYOUT_PROOF") {
        let path = std::path::Path::new(&directory);
        assert!(path.is_absolute() && directory.starts_with("/home/"));
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(path.join("prototype.html"), &html).unwrap();
        std::fs::write(
            path.join("compound.html"),
            export_html(&compound, 0, a).unwrap(),
        )
        .unwrap();
        std::fs::write(
            path.join("prototype.oma"),
            crate::project::encode(&reopened).unwrap(),
        )
        .unwrap();
        std::fs::write(
            path.join("ids.json"),
            serde_json::json!({"a":a,"instance":instance,"other":other,"label":label_instance,"compact":v1,"wide":v2}).to_string(),
        )
        .unwrap();
        let mut expectations = Vec::new();
        for width in [390., 599., 600., 601., 768., 1440.] {
            for (name, trigger) in [
                ("base", None),
                ("compact", Some(Trigger::Click)),
                ("wide", Some(Trigger::Hover)),
            ] {
                let mut state = crate::layout_prototype::PrototypeState::default();
                state.start(&reopened, a).unwrap();
                if let Some(trigger) = trigger {
                    state.activate(&reopened, instance, trigger).unwrap();
                }
                let (preview, _) =
                    crate::ui::layout_presentation(&reopened, &state, width).unwrap();
                let mut shapes = Vec::new();
                for id in [instance, other, label_instance] {
                    let shape = preview.find_shape(0, id).unwrap();
                    let b = shape.geom.bbox();
                    let resolved = crate::layout::resolve_for_width(&shape.layout, width);
                    shapes.push(serde_json::json!({"id":id,"x":b.min.x,"y":b.min.y,"width":b.width(),"height":b.height(),"baseline":if let Geom::Text(run)=&shape.geom{Some(crate::text::caret_pt(run,0).y)}else{None},"px":if let Geom::Text(run)=&shape.geom{Some(run.px)}else{None},"gap":resolved.stack.as_ref().map(|s|s.gap),"direction":resolved.stack.as_ref().map(|s|s.direction)}));
                }
                expectations.push(serde_json::json!({"width":width,"state":name,"shapes":shapes}));
            }
        }
        std::fs::write(
            path.join("variant-expected.json"),
            serde_json::to_string(&expectations).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn advanced_grid_and_real_font_baselines_export_from_reopened_native_artwork() {
    use crate::layout::{AutoStack, GridPlacement, GridTrack, GridTrackSize as Size, Sizing};
    let mut doc = Document::new("Grid oracle", 600., 250., 96.);
    doc.layers = vec![Layer::vector("Grid")];
    let mut root = make_frame(Pt::ZERO, Pt::new(600., 250.));
    root.name = "Grid oracle".into();
    root.layout.stack = Some(AutoStack {
        flow: StackFlow::Grid,
        padding: [0.; 4],
        gap: 10.,
        cross_gap: 10.,
        column_tracks: vec![
            GridTrack {
                size: Size::Fixed(80.),
                ..Default::default()
            },
            GridTrack::default(),
            GridTrack {
                size: Size::Fraction(2.),
                ..Default::default()
            },
        ],
        row_tracks: vec![
            GridTrack {
                size: Size::Fixed(40.),
                ..Default::default()
            },
            GridTrack::default(),
        ],
        ..Default::default()
    });
    let id = root.id;
    doc.layers[0].kind.shapes_mut().unwrap().push(root);
    let mut children = Vec::new();
    for (name, column, row, span) in [
        ("Fixed track", 0, 0, 1),
        ("Spanning fraction tracks", 1, 0, 2),
        ("Second row", 0, 1, 3),
    ] {
        let mut child = make_frame(Pt::ZERO, Pt::new(20., 20.));
        child.name = name.into();
        child.layout = crate::layout::FrameLayout {
            parent: Some(id),
            width: Sizing::Fill,
            height: Sizing::Fill,
            grid: GridPlacement {
                column: Some(column),
                row: Some(row),
                column_span: span,
                ..Default::default()
            },
            ..Default::default()
        };
        children.push(child.id);
        doc.layers[0].kind.shapes_mut().unwrap().push(child);
    }
    crate::layout::reflow(&mut doc, 0, id);
    let mut doc = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    crate::layout::reflow(&mut doc, 0, id);
    for (child, x, y, w, h) in [
        (children[0], 0., 0., 80., 40.),
        (children[1], 90., 0., 510., 40.),
        (children[2], 0., 50., 600., 200.),
    ] {
        let bounds = doc.find_shape(0, child).unwrap().geom.bbox();
        for (actual, expected) in [
            (bounds.min.x, x),
            (bounds.min.y, y),
            (bounds.width(), w),
            (bounds.height(), h),
        ] {
            assert!((actual - expected).abs() < 0.02, "{actual} != {expected}");
        }
    }
    let html = export_html(&doc, 0, id).unwrap();
    assert_eq!(html, export_html(&doc, 0, id).unwrap());
    let mut baseline = Document::new("Baseline oracle", 600., 250., 96.);
    baseline.layers = vec![Layer::vector("Text")];
    let mut root = make_frame(Pt::ZERO, Pt::new(600., 200.));
    root.name = "Baseline oracle".into();
    root.layout.height = Sizing::Hug;
    root.layout.stack = Some(AutoStack {
        direction: StackAxis::Horizontal,
        align: StackAlign::Baseline,
        padding: [0.; 4],
        gap: 10.,
        ..Default::default()
    });
    let bid = root.id;
    baseline.layers[0].kind.shapes_mut().unwrap().push(root);
    for (index, px) in [48., 24.].into_iter().enumerate() {
        let run = crate::geom::TypeRun {
            content: "Café Ωñ".into(),
            px,
            font: concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/assets/fonts/EBGaramond.ttf"
            )
            .into(),
            origin: Pt::new(index as f32 * 200., 80.),
            ..Default::default()
        };
        let mut text = Shape::new(Geom::Text(run), Style::default());
        text.name = format!("Unicode {px}");
        text.layout.parent = Some(bid);
        text.layout.width = Sizing::Hug;
        text.layout.height = Sizing::Hug;
        crate::text::fill_contours(&mut text.geom);
        baseline.layers[0].kind.shapes_mut().unwrap().push(text);
    }
    crate::layout::reflow(&mut baseline, 0, bid);
    let mut baseline = crate::project::decode(&crate::project::encode(&baseline).unwrap()).unwrap();
    crate::layout::reflow(&mut baseline, 0, bid);
    let expected:Vec<_>=baseline.layers[0].kind.shapes().unwrap().iter().map(|shape|{let b=shape.geom.bbox();serde_json::json!({"name":shape.name,"x":b.min.x,"y":b.min.y,"width":b.width(),"height":b.height(),"baseline":if let Geom::Text(run)=&shape.geom {Some(run.origin.y)}else{None}})}).collect();
    let mut fractions = baseline.layout_snapshot();
    let stack = fractions
        .find_shape_mut(0, bid)
        .unwrap()
        .layout
        .stack
        .as_mut()
        .unwrap();
    stack.flow = StackFlow::Grid;
    stack.cross_gap = 0.;
    stack.column_tracks = vec![GridTrack {
        size: Size::Fixed(600.),
        ..Default::default()
    }];
    stack.row_tracks = vec![
        GridTrack::default(),
        GridTrack {
            size: Size::Fraction(2.),
            ..Default::default()
        },
    ];
    for (row, shape) in fractions.layers[0]
        .kind
        .shapes_mut()
        .unwrap()
        .iter_mut()
        .filter(|shape| shape.id != bid)
        .enumerate()
    {
        shape.layout.grid = GridPlacement {
            column: Some(0),
            row: Some(row as u32),
            ..Default::default()
        };
    }
    crate::layout::reflow(&mut fractions, 0, bid);
    let fractions = crate::project::decode(&crate::project::encode(&fractions).unwrap()).unwrap();
    let fraction_height = fractions.find_shape(0, bid).unwrap().geom.bbox().height();
    let second_baseline = fractions.layers[0]
        .kind
        .shapes()
        .unwrap()
        .iter()
        .find_map(|shape| {
            if let Geom::Text(run) = &shape.geom {
                (run.px == 24.).then_some(crate::text::caret_pt(run, 0).y)
            } else {
                None
            }
        })
        .unwrap();
    assert!(fraction_height >= 3. * (48.336 + 14.304));
    assert!((fraction_height / 3. + 24.168 - second_baseline).abs() < 0.02);
    let fraction_expected:Vec<_>=fractions.layers[0].kind.shapes().unwrap().iter().map(|shape|{let b=shape.geom.bbox();serde_json::json!({"name":shape.name,"x":b.min.x,"y":b.min.y,"width":b.width(),"height":b.height(),"baseline":if let Geom::Text(run)=&shape.geom {Some(crate::text::caret_pt(run,0).y)}else{None}})}).collect();
    if let Ok(directory) = std::env::var("OMADESIGN_LAYOUT_PROOF") {
        let directory = std::path::Path::new(&directory);
        std::fs::create_dir_all(directory).unwrap();
        for (name, content) in [
            ("grid.html", html),
            ("grid.svg", crate::svg::export_frame(&doc, 0, id).unwrap()),
            ("baseline.html", export_html(&baseline, 0, bid).unwrap()),
            ("baseline-fr.html", export_html(&fractions, 0, bid).unwrap()),
            (
                "baseline-fr-expected.json",
                serde_json::to_string(&fraction_expected).unwrap(),
            ),
            (
                "baseline.svg",
                crate::svg::export_frame(&baseline, 0, bid).unwrap(),
            ),
            (
                "baseline-expected.json",
                serde_json::to_string(&expected).unwrap(),
            ),
        ] {
            std::fs::write(directory.join(name), content).unwrap();
        }
    }
}

#[test]
fn named_properties_and_tracks_use_the_provisional_gate_while_legacy_documents_stay_readable() {
    let mut doc = Document::new("Compatibility", 200., 100., 96.);
    doc.layers = vec![Layer::vector("UI")];
    let root = make_frame(Pt::ZERO, Pt::new(100., 50.));
    let id = root.id;
    doc.layers[0].kind.shapes_mut().unwrap().push(root);
    let plain = crate::project::encode(&doc).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&plain).unwrap()["version"],
        5
    );
    crate::layout_components::make_component(&mut doc, 0, id).unwrap();
    crate::layout_components::expose_property(
        &mut doc,
        0,
        id,
        id,
        "Shown",
        crate::layout_components::PropertyKind::Visibility,
    )
    .unwrap();
    let encoded = crate::project::encode(&doc).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(value["version"], 14);
    value["version"] = serde_json::json!(13);
    assert!(
        crate::project::decode(&value.to_string())
            .unwrap_err()
            .contains("Advanced layout")
    );
    assert!(crate::project::decode(&plain).is_ok());
}

#[test]
fn nested_component_variant_contexts_preserve_local_text_and_new_node_ids_deterministically() {
    use crate::layout_components as c;
    let (mut doc, a, _, _, _) = graph();
    let button = make_frame(Pt::new(0., 1000.), Pt::new(140., 60.));
    let main = button.id;
    let mut label = Shape::new(
        Geom::Text(crate::geom::TypeRun {
            content: "Source".into(),
            origin: Pt::new(10., 1030.),
            ..Default::default()
        }),
        Style::default(),
    );
    let label_id = label.id;
    label.name = "Nested label".into();
    label.layout.parent = Some(main);
    crate::text::fill_contours(&mut label.geom);
    doc.layers[2]
        .kind
        .shapes_mut()
        .unwrap()
        .extend([button, label]);
    c::make_component(&mut doc, 2, main).unwrap();
    let selected = c::add_variant(&mut doc, main, "Selected", Pt::new(200., 1000.)).unwrap();
    doc.find_shape_mut(2, selected).unwrap().opacity = 0.4;
    let card = make_frame(Pt::new(0., 1200.), Pt::new(200., 120.));
    let card_id = card.id;
    doc.layers[2].kind.shapes_mut().unwrap().push(card);
    let nested = c::insert_instance(&mut doc, main, 2, Pt::new(10., 1210.), Some(card_id)).unwrap();
    doc.find_shape_mut(2, nested)
        .unwrap()
        .layout
        .interactions
        .push(interaction(Action::SetVariant {
            component: selected,
        }));
    c::make_component(&mut doc, 2, card_id).unwrap();
    let alternate = c::add_variant(&mut doc, card_id, "Alternate", Pt::new(250., 1200.)).unwrap();
    doc.find_shape_mut(2, alternate).unwrap().opacity = 0.6;
    let mut extra = make_frame(Pt::new(410., 1290.), Pt::new(10., 10.));
    extra.name = "New variant child".into();
    extra.layout.parent = Some(alternate);
    doc.layers[2].kind.shapes_mut().unwrap().push(extra);
    let instance = c::insert_instance(&mut doc, card_id, 0, Pt::new(120., 100.), Some(a)).unwrap();
    doc.find_shape_mut(0, instance)
        .unwrap()
        .layout
        .interactions
        .push(interaction(Action::SetVariant {
            component: alternate,
        }));
    let nested_instance = crate::layout::descendants(&doc, 0, instance)
        .into_iter()
        .find(|id| {
            matches!(
                doc.find_shape(0, *id).unwrap().layout.component,
                Some(c::ComponentBinding::Instance { .. })
            )
        })
        .unwrap();
    let text = crate::layout::descendants(&doc, 0, nested_instance)
        .into_iter()
        .find(|id| matches!(doc.find_shape(0, *id).unwrap().geom, Geom::Text(_)))
        .unwrap();
    if let Geom::Text(run) = &mut doc.find_shape_mut(0, text).unwrap().geom {
        run.content = "User retained Ω".into();
    }
    let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    let html = export_html(&reopened, 0, a).unwrap();
    assert_eq!(html, export_html(&reopened, 0, a).unwrap());
    assert!(html.contains(&format!(
        "variant-variant-screen-{a}-{instance}-{alternate}-{nested_instance}-{selected}"
    )));
    assert!(html.contains("User retained Ω"));
    assert!(!html.contains("PRIVATE SECRET"));
    if let Ok(directory) = std::env::var("OMADESIGN_LAYOUT_PROOF") {
        let path = std::path::Path::new(&directory);
        std::fs::write(path.join("components.html"), html).unwrap();
        std::fs::write(path.join("component-ids.json"),serde_json::json!({"outer":instance,"nested":nested_instance,"label":text,"alternate":alternate,"selected":selected}).to_string()).unwrap();
    }
    assert!(doc.find_shape(2, label_id).is_some());
}

#[test]
fn prototype_ids_remain_exact_above_javascript_number_precision() {
    let mut doc = Document::new("Exact IDs", 200., 100., 96.);
    doc.layers = vec![Layer::vector("UI")];
    let mut a = make_frame(Pt::ZERO, Pt::new(100., 50.));
    let mut b = make_frame(Pt::new(110., 0.), Pt::new(100., 50.));
    a.id = 9_007_199_254_741_100;
    b.id = a.id + 1;
    let root = a.id;
    a.layout
        .interactions
        .push(interaction(Action::Navigate { target: b.id }));
    doc.layers[0].kind.shapes_mut().unwrap().extend([a, b]);
    let html = export_html(&doc, 0, root).unwrap();
    assert!(html.contains("let current=\"9007199254741100\""));
    assert!(html.contains("&quot;target&quot;:&quot;9007199254741101&quot;"));
    if let Ok(directory) = std::env::var("OMADESIGN_LAYOUT_PROOF") {
        std::fs::write(
            std::path::Path::new(&directory).join("exact-ids.html"),
            html,
        )
        .unwrap();
    }
}

#[test]
fn capped_hug_and_fraction_tracks_keep_the_hand_derived_budget_in_exports() {
    use crate::layout::{AutoStack, GridPlacement, GridTrack, GridTrackSize as Size, Sizing};
    let mut doc = Document::new("Track limits", 600., 40., 96.);
    doc.layers = vec![Layer::vector("Grid")];
    let mut root = make_frame(Pt::ZERO, Pt::new(600., 40.));
    root.layout.stack = Some(AutoStack {
        flow: StackFlow::Grid,
        padding: [0.; 4],
        gap: 10.,
        cross_gap: 0.,
        column_tracks: vec![
            GridTrack {
                size: Size::Fixed(80.),
                ..Default::default()
            },
            GridTrack {
                size: Size::Hug,
                min: Some(20.),
                max: Some(60.),
            },
            GridTrack {
                size: Size::Fraction(1.),
                min: Some(80.),
                max: Some(100.),
            },
            GridTrack {
                size: Size::Fraction(2.),
                ..Default::default()
            },
        ],
        row_tracks: vec![GridTrack {
            size: Size::Fixed(40.),
            ..Default::default()
        }],
        ..Default::default()
    });
    let id = root.id;
    root.name = "Track limits".into();
    doc.layers[0].kind.shapes_mut().unwrap().push(root);
    let mut ids = Vec::new();
    for column in 0..4 {
        let mut child = make_frame(Pt::ZERO, Pt::new(if column == 1 { 120. } else { 20. }, 20.));
        child.name = format!("Limit {column}");
        child.layout = crate::layout::FrameLayout {
            parent: Some(id),
            width: if column == 1 {
                Sizing::Fixed
            } else {
                Sizing::Fill
            },
            height: Sizing::Fill,
            grid: GridPlacement {
                column: Some(column),
                row: Some(0),
                ..Default::default()
            },
            ..Default::default()
        };
        ids.push(child.id);
        doc.layers[0].kind.shapes_mut().unwrap().push(child);
    }
    crate::layout::reflow(&mut doc, 0, id);
    let doc = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    for (index, x, width) in [
        (0, 0., 80.),
        (1, 90., 120.),
        (2, 160., 100.),
        (3, 270., 330.),
    ] {
        let b = doc.find_shape(0, ids[index]).unwrap().geom.bbox();
        assert!((b.min.x - x).abs() < 0.02);
        assert!((b.width() - width).abs() < 0.02);
    }
    if let Ok(directory) = std::env::var("OMADESIGN_LAYOUT_PROOF") {
        std::fs::write(
            std::path::Path::new(&directory).join("grid-limits.html"),
            export_html(&doc, 0, id).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn legacy_uniform_grid_retains_one_pixel_tracks_when_the_viewport_is_narrow() {
    let mut doc = Document::new("Dense legacy grid", 240., 40., 96.);
    doc.layers = vec![Layer::vector("Grid")];
    let mut root = make_frame(Pt::ZERO, Pt::new(240., 40.));
    let id = root.id;
    root.layout.stack = Some(crate::layout::AutoStack {
        flow: StackFlow::Grid,
        columns: 256,
        padding: [0.; 4],
        gap: 0.,
        cross_gap: 0.,
        ..Default::default()
    });
    doc.layers[0].kind.shapes_mut().unwrap().push(root);
    let mut last = 0;
    for column in 0..256 {
        let mut child = Shape::new(
            Geom::Rect {
                origin: Pt::ZERO,
                size: Pt::new(20., 20.),
                radius: 0.,
            },
            Style::default(),
        );
        child.name = format!("Legacy column {column}");
        child.layout.parent = Some(id);
        child.layout.width = Sizing::Fill;
        child.layout.height = Sizing::Fill;
        last = child.id;
        doc.layers[0].kind.shapes_mut().unwrap().push(child);
    }
    crate::layout::reflow(&mut doc, 0, id);
    let doc = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    let bounds = doc.find_shape(0, last).unwrap().geom.bbox();
    assert_eq!(bounds.min.x, 255.);
    assert_eq!(bounds.width(), 1.);
    if let Ok(directory) = std::env::var("OMADESIGN_LAYOUT_PROOF") {
        std::fs::write(
            std::path::Path::new(&directory).join("grid-legacy.html"),
            export_html(&doc, 0, id).unwrap(),
        )
        .unwrap();
    }
}
