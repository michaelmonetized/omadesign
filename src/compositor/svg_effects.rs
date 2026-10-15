//! Evaluate retained SVG effects against current vector children at viewport scale.
use super::*;
use std::collections::HashSet;

/// Render a group's editable subtree through its source filter graph.
/// Args: document, group index, view transform, target size, motion time and overrides.
/// Returns a viewport-sized surface or a descriptive source error, without allocating a page image.
pub(super) fn render(
    doc: &Document,
    index: usize,
    transform: Transform,
    width: u32,
    height: u32,
    motion: Option<f32>,
    overrides: Option<&HashMap<u64, Pose>>,
) -> Result<Pixmap, String> {
    let group = doc.layers.get(index).ok_or("Missing SVG effect group")?;
    let mut ids = HashSet::from([group.id]);
    for _ in 0..64 {
        let before = ids.len();
        for layer in &doc.layers {
            if layer.parent.is_some_and(|id| ids.contains(&id)) {
                ids.insert(layer.id);
            }
            if !ids.contains(&layer.id) {
                continue;
            }
            for mask in layer
                .artwork_masks
                .iter()
                .filter(|_| layer.id != group.id)
                .chain(
                    layer
                        .kind
                        .shapes()
                        .into_iter()
                        .flatten()
                        .flat_map(|s| &s.artwork_masks),
                )
            {
                if let Some((index, _)) = mask.source.resolve(doc) {
                    ids.insert(doc.layers[index].id);
                }
            }
        }
        if ids.len() == before {
            break;
        }
    }
    let mut isolated = Document::new("SVG effect source", 1., 1., doc.dpi);
    isolated.width = doc.width;
    isolated.height = doc.height;
    isolated.layers = doc
        .layers
        .iter()
        .filter(|l| ids.contains(&l.id))
        .cloned()
        .collect();
    isolated.transparent = true;
    let mut poses = HashMap::new();
    for layer in &mut isolated.layers {
        if layer.parent.is_some_and(|id| !ids.contains(&id)) {
            layer.parent = None;
        }
        if layer.id == group.id {
            layer.parent = None;
            layer.opacity = 1.;
            layer.fill_opacity = 1.;
            layer.blend = crate::color::Blend::Normal;
            layer.blend_interior = false;
            layer.filters = Default::default();
            layer.mask = None;
            layer.artwork_masks.clear();
        }
        if let Some(shapes) = layer.kind.shapes_mut() {
            for shape in shapes {
                let pose = pose_of(shape.id, motion, doc, overrides);
                if !pose.is_identity() {
                    posed_paint(shape, pose);
                    if let Some(opacity) = pose.opacity {
                        shape.opacity = opacity;
                    }
                    poses.insert(shape.id, pose);
                }
            }
        }
    }
    let source = effect_source(&isolated, &poses)?;
    let tree =
        usvg::Tree::from_str(&source, &crate::svg_source::options()).map_err(|e| e.to_string())?;
    let mut surface = Pixmap::new(width, height).ok_or("Could not allocate SVG effect viewport")?;
    resvg::render(&tree, transform, &mut surface.as_mut());
    Ok(surface)
}

/// Omit redundant native isolation containers inside an already isolated filter.
/// Args: effect subtree and evaluated poses. Returns vector XML with required blends preserved.
fn effect_source(doc: &Document, poses: &HashMap<u64, Pose>) -> Result<String, String> {
    let mut source = crate::svg::export_posed(doc, poses)?;
    if doc.layers.iter().any(|layer| {
        layer.blend != crate::color::Blend::Normal
            || layer.filters.blends_backdrop()
            || layer.kind.shapes().into_iter().flatten().any(|shape| {
                shape.blend != crate::color::Blend::Normal || shape.filters.blends_backdrop()
            })
    }) {
        return Ok(source);
    }
    let xml = roxmltree::Document::parse(&source).map_err(|e| e.to_string())?;
    let mut edits: Vec<_> = xml
        .descendants()
        .filter(|node| {
            node.attribute("id")
                .is_some_and(|id| id.starts_with("oma-layer-"))
        })
        .filter_map(|node| {
            let style = node.attributes().find(|attr| attr.name() == "style")?;
            style.value().contains("isolation:isolate").then(|| {
                (
                    style.range_value(),
                    style.value().replace("isolation:isolate", "isolation:auto"),
                )
            })
        })
        .collect();
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    for (range, replacement) in edits {
        source.replace_range(range, &replacement);
    }
    Ok(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="180" height="140"><defs><filter id="fx" x="-40%" y="-40%" width="200%" height="200%" color-interpolation-filters="sRGB"><feGaussianBlur in="SourceAlpha" stdDeviation="2 3" result="blur"/><feOffset in="blur" dx="5" dy="4" result="offset"/><feFlood flood-color="#159" flood-opacity=".7" result="color"/><feComposite in="color" in2="offset" operator="in" result="shadow"/><feMerge><feMergeNode in="shadow"/><feMergeNode in="SourceGraphic"/></feMerge></filter></defs><g transform="matrix(1 .15 .2 .85 8 5)"><g opacity=".8" filter="url(#fx)"><path d="M15 15H110V95H15Z" fill="#d53"/></g></g></svg>"##;

    fn difference(a: &Pixmap, b: &Pixmap) -> (u8, f64) {
        let mut maximum = 0;
        let sum: u64 = a
            .data()
            .iter()
            .zip(b.data())
            .map(|(a, b)| {
                let d = a.abs_diff(*b);
                maximum = maximum.max(d);
                u64::from(d)
            })
            .sum();
        (maximum, sum as f64 / a.data().len() as f64)
    }

    #[test]
    fn svg_graph_preserves_children_roundtrip_scale_and_edited_operations() {
        let (mut doc, notes) = crate::formats::svg::read(SOURCE, "Graph").unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        assert!(doc.layers.iter().all(|l| l.kind.pixels().is_none()));
        let group = doc
            .layers
            .iter()
            .position(|l| l.svg_filter.is_some())
            .unwrap();
        assert!(
            doc.layers[group]
                .svg_filter
                .as_ref()
                .unwrap()
                .original_definitions
                .contains("feMerge")
        );
        let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
        let exported = crate::svg::export(&reopened).unwrap();
        assert!(!exported.contains("<image"));
        assert!(exported.contains("feComposite"));
        let expected_tree = usvg::Tree::from_str(SOURCE, &usvg::Options::default()).unwrap();
        for scale in [0.5, 1., 2.] {
            let actual = crate::compositor::render_export_at(&reopened, scale, 0., true).unwrap();
            let mut expected = Pixmap::new(actual.width(), actual.height()).unwrap();
            resvg::render(
                &expected_tree,
                Transform::from_scale(scale, scale),
                &mut expected.as_mut(),
            );
            let (maximum, mean) = difference(&actual, &expected);
            assert!(
                maximum <= 3 && mean < 0.1,
                "scale={scale} max={maximum} mean={mean}"
            );
        }
        let original = crate::compositor::export_png(&doc, 1).unwrap();
        let before = doc.layers[group].svg_filter.clone();
        let mut after = before.clone().unwrap();
        after.definitions = after.definitions.replace("dx=\"5\"", "dx=\"12\"").into();
        let command = crate::document::Cmd::SetSvgFilter {
            index: group,
            before,
            after: Some(after),
        };
        crate::document::apply(&mut doc, &command);
        assert_ne!(original, crate::compositor::export_png(&doc, 1).unwrap());
        crate::document::apply(&mut doc, &crate::document::invert_cmd(command));
        assert_eq!(original, crate::compositor::export_png(&doc, 1).unwrap());
    }

    #[test]
    fn svg_graph_large_bounds_only_allocate_the_requested_viewport() {
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100000" height="80000"><defs><filter id="fx" x="0" y="0" width="100000" height="80000" filterUnits="userSpaceOnUse"><feGaussianBlur stdDeviation="150"/><feColorMatrix type="saturate" values=".4"/></filter></defs><g filter="url(#fx)"><rect x="10" y="10" width="99980" height="79980" fill="#28b"/><circle cx="50000" cy="40000" r="5000" fill="#e42"/></g></svg>"##;
        let (doc, notes) = crate::formats::svg::read(source, "Large graph").unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        assert!(doc.layers.iter().all(|l| l.kind.pixels().is_none()));
        let index = doc
            .layers
            .iter()
            .position(|l| l.svg_filter.is_some())
            .unwrap();
        let expected_tree = usvg::Tree::from_str(source, &usvg::Options::default()).unwrap();
        for transform in [
            Transform::from_scale(0.004, 0.004),
            Transform::from_translate(-49900., -39900.),
        ] {
            let actual = render(&doc, index, transform, 400, 320, None, None).unwrap();
            assert_eq!(actual.data().len(), 400 * 320 * 4);
            let mut expected = Pixmap::new(400, 320).unwrap();
            resvg::render(&expected_tree, transform, &mut expected.as_mut());
            let (maximum, mean) = difference(&actual, &expected);
            if maximum > 3 || mean >= 0.1 {
                let dir = std::env::current_dir()
                    .unwrap()
                    .join(".artifacts")
                    .join(format!("svg-effects-large-{}", std::process::id()));
                std::fs::create_dir_all(&dir).unwrap();
                actual.save_png(dir.join("native.png")).unwrap();
                expected.save_png(dir.join("reference.png")).unwrap();
                std::fs::write(dir.join("native.svg"), crate::svg::export(&doc).unwrap()).unwrap();
                std::fs::write(dir.join("source.svg"), source).unwrap();
                println!(
                    "Large effect failure artifacts: {} transform={transform:?}",
                    dir.display()
                );
            }
            assert!(maximum <= 3 && mean < 0.1, "max={maximum} mean={mean}");
        }
        assert!(crate::project::encode(&doc).unwrap().len() < 30_000);
    }

    #[test]
    fn svg_graph_renders_on_a_flat_native_vector_layer() {
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="180" height="140"><defs><filter id="fx"><feOffset dx="9" dy="7"/></filter></defs><path filter="url(#fx)" d="M20 20H100V90H20Z" fill="#c52"/></svg>"##;
        let (mut doc, _) = crate::formats::svg::read(source, "Flat graph").unwrap();
        let graph = doc
            .layers
            .iter()
            .find_map(|l| l.svg_filter.clone())
            .unwrap();
        let mut layer = doc
            .layers
            .iter()
            .find(|l| l.kind.shapes().is_some_and(|s| !s.is_empty()))
            .unwrap()
            .clone();
        layer.parent = None;
        layer.svg_filter = Some(graph);
        doc.layers = vec![layer];
        assert!(!doc.layers[0].is_group);
        let tree = usvg::Tree::from_str(source, &crate::svg_source::options()).unwrap();
        let mut expected = Pixmap::new(180, 140).unwrap();
        resvg::render(&tree, Transform::identity(), &mut expected.as_mut());
        let actual = crate::compositor::render_export_at(&doc, 1., 0., true).unwrap();
        let (maximum, mean) = difference(&actual, &expected);
        assert!(maximum <= 3 && mean < 0.1, "max={maximum} mean={mean}");
    }

    #[test]
    fn svg_graph_applies_root_clips_after_effect_without_painting_clip_sources() {
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="180" height="140"><defs><filter id="fx"><feGaussianBlur stdDeviation="2"/><feOffset dx="3"/></filter><clipPath id="clip"><path d="M20 20H100V90H20Z"/></clipPath></defs><g filter="url(#fx)" clip-path="url(#clip)"><rect x="10" y="10" width="110" height="90" fill="#c52"/></g></svg>"##;
        let (doc, notes) = crate::formats::svg::read(source, "Root clip graph").unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        assert!(
            doc.layers
                .iter()
                .any(|l| l.svg_filter.is_some() && !l.artwork_masks.is_empty())
        );
        let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
        let tree = usvg::Tree::from_str(source, &crate::svg_source::options()).unwrap();
        let mut expected = Pixmap::new(180, 140).unwrap();
        resvg::render(&tree, Transform::identity(), &mut expected.as_mut());
        let actual = crate::compositor::render_export_at(&reopened, 1., 0., true).unwrap();
        let (maximum, mean) = difference(&actual, &expected);
        assert!(maximum <= 3 && mean < 0.1, "max={maximum} mean={mean}");
    }

    #[test]
    fn svg_graph_keeps_nested_clip_sources_and_recomputes_object_bbox_regions() {
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="180" height="140"><defs><filter id="fx"><feGaussianBlur stdDeviation="2"/><feOffset dx="3"/></filter><clipPath id="clip"><path d="M20 20H100V90H20Z"/></clipPath></defs><g filter="url(#fx)"><g clip-path="url(#clip)"><rect x="10" y="10" width="110" height="90" fill="#c52"/></g></g></svg>"##;
        let (mut doc, _) = crate::formats::svg::read(source, "Clip graph").unwrap();
        let tree = usvg::Tree::from_str(source, &crate::svg_source::options()).unwrap();
        let mut expected = Pixmap::new(180, 140).unwrap();
        resvg::render(&tree, Transform::identity(), &mut expected.as_mut());
        let actual = crate::compositor::render_export_at(&doc, 1., 0., true).unwrap();
        let (maximum, mean) = difference(&actual, &expected);
        assert!(maximum <= 3 && mean < 0.1, "max={maximum} mean={mean}");
        let unclipped = source.replace(r##" clip-path="url(#clip)""##, "");
        doc = crate::formats::svg::read(&unclipped, "Edit graph")
            .unwrap()
            .0;
        let shape = doc
            .layers
            .iter_mut()
            .filter_map(|l| l.kind.shapes_mut())
            .flatten()
            .find(|s| s.style.fill == Fill::Solid(Rgba::new(204, 85, 34, 255)))
            .unwrap();
        shape.geom.translate(Pt::new(30., 15.));
        let moved = unclipped.replace(r##"x="10" y="10""##, r##"x="40" y="25""##);
        let tree = usvg::Tree::from_str(&moved, &crate::svg_source::options()).unwrap();
        expected.fill(tiny_skia::Color::TRANSPARENT);
        resvg::render(&tree, Transform::identity(), &mut expected.as_mut());
        let actual = crate::compositor::render_export_at(&doc, 1., 0., true).unwrap();
        let (maximum, mean) = difference(&actual, &expected);
        assert!(
            maximum <= 3 && mean < 0.1,
            "moved max={maximum} mean={mean}"
        );
    }

    #[test]
    fn svg_graph_reductions_name_the_operation_and_keep_the_original_source() {
        let source = SOURCE.replace("<feMerge>", "<feUnsupportedThing/><feMerge>");
        let (doc, notes) = crate::formats::svg::read(&source, "Reduced graph").unwrap();
        assert!(
            notes.iter().any(|n| n.contains("feUnsupportedThing")),
            "{notes:?}"
        );
        let graph = doc
            .layers
            .iter()
            .find_map(|l| l.svg_filter.as_ref())
            .unwrap();
        assert!(graph.original_definitions.contains("feUnsupportedThing"));
        assert!(
            graph
                .reductions
                .iter()
                .any(|n| n.contains("feUnsupportedThing"))
        );
    }

    #[test]
    #[ignore = "requires independent rsvg-convert; writes and removes only its own home-project scratch"]
    fn svg_graph_independent_librsvg_and_disk_reopen() {
        let (doc, _) = crate::formats::svg::read(SOURCE, "Independent graph").unwrap();
        let dir = std::env::current_dir()
            .unwrap()
            .join(".artifacts")
            .join(format!(
                "svg-effects-{}-{}",
                std::process::id(),
                crate::document::next_id()
            ));
        std::fs::create_dir_all(&dir).unwrap();
        let oma = dir.join("graph.oma");
        crate::project::save_to(&doc, &oma).unwrap();
        let reopened = crate::project::load_from(&oma).unwrap();
        let svg = dir.join("graph.svg");
        std::fs::write(&svg, crate::svg::export(&reopened).unwrap()).unwrap();
        let png = dir.join("reference.png");
        let status = std::process::Command::new("rsvg-convert")
            .arg(&svg)
            .arg("-o")
            .arg(&png)
            .status()
            .unwrap();
        assert!(status.success());
        let expected = Pixmap::decode_png(&std::fs::read(&png).unwrap()).unwrap();
        let actual = crate::compositor::render_export_at(&reopened, 1., 0., true).unwrap();
        let (maximum, mean) = difference(&actual, &expected);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(mean < 1.5, "independent librsvg max={maximum} mean={mean}");
    }
}
