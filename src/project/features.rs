use crate::document::{Document, Fill, StrokeAlignment};
use crate::geom::Geom;
use crate::gradient::{Gradient, Spread};
#[cfg(test)]
mod native_tests;
mod retained;
pub(crate) use retained::{serialized_version, value_version};

pub(super) const RICH_TEXT: u32 = 14;
pub(super) const ADVANCED_GRADIENT: u32 = 15;
pub(super) const VECTOR_PATTERN: u32 = 16;
pub(super) const SVG_FILTER: u32 = 17;
pub(super) const SOURCE: u32 = 18;
pub(super) const NATIVE: u32 = 19;
pub(super) const COLOR: u32 = 20;
pub(super) const PSD_TYPE: u32 = 21;
pub(super) const SMART: u32 = 22;
pub(super) const PUBLISHING: u32 = 23;
pub(super) const COLLABORATION: u32 = 24;
pub(super) const EXPRESSIONS: u32 = 25;
pub(super) const COMPOSITIONS: u32 = 26;
pub(super) const SCENE3D: u32 = 27;
pub(super) const ADVANCED_LAYOUT: u32 = 28;
pub(super) const SCREEN_EXPORT: u32 = 29;
pub(super) const EXPORT_RESOLUTION: u32 = 30;
pub(super) const EXPORT_FOLDERS: u32 = 31;
pub(super) const CHANNELS: u32 = 32;

/// Identify implemented project readers.
/// Args: envelope version. Returns false for reserved formats whose models are absent.
pub(super) fn supports_version(version: u32) -> bool {
    matches!(
        version,
        1..=17
            | SOURCE
            | NATIVE
            | COLOR
            | PSD_TYPE
            | SMART
            | PUBLISHING
            | COLLABORATION
            | EXPRESSIONS
            | COMPOSITIONS
            | SCENE3D
            | ADVANCED_LAYOUT
            | SCREEN_EXPORT
            | EXPORT_RESOLUTION
            | EXPORT_FOLDERS
            | CHANNELS
    )
}

/// Find the oldest reader that preserves this document.
/// Args: document with unpacked editable objects. Returns its highest required feature version.
pub(super) fn required_version(doc: &Document) -> u32 {
    crate::motion::compositions::documents(doc)
        .map(|docs| {
            docs.into_iter()
                .map(local_required_version)
                .max()
                .unwrap_or(5)
        })
        .unwrap_or(CHANNELS + 1)
}

fn local_required_version(doc: &Document) -> u32 {
    let mut version = if doc.mask_sources().is_empty() { 5 } else { 13 };
    if let Some(replica) = &doc.collaboration {
        version = version.max(retained::replica_version(replica).unwrap_or(CHANNELS + 1));
    }
    if !doc.channels.is_empty() { version=version.max(CHANNELS); }
    if !doc.screen_export.is_empty() {
        version = version.max(SCREEN_EXPORT);
    }
    if doc.screen_export.subfolders
        || doc
            .screen_export
            .rows
            .iter()
            .any(|row| !row.folder.is_empty())
    {
        version = version.max(EXPORT_FOLDERS);
    }
    if doc
        .screen_export
        .rows
        .iter()
        .any(|row| row.export_resolution.is_some())
    {
        version = version.max(EXPORT_RESOLUTION);
    }
    for source in &doc.sources {
        version = version.max(retained::source_version(source));
    }
    if doc.color.depth != crate::deep_color::Depth::U8 {
        version = version.max(NATIVE);
    }
    if doc.color.model != crate::deep_color::cms::Model::Rgb
        || !doc.color.icc_profile.is_empty()
        || doc.color.intent != crate::deep_color::cms::DocumentColor::default().intent
    {
        version = version.max(COLOR);
    }
    for native in crate::deep_color::buffers(doc).filter_map(|pixels| pixels.native.as_ref()) {
        version = version.max(NATIVE);
        if native.cmyk.is_some() || !native.icc_profile.is_empty() || native.intent != 1 {
            version = version.max(COLOR);
        }
    }
    if has_advanced_layout(doc) {
        version = version.max(ADVANCED_LAYOUT);
    }
    if !doc.motion.expressions.is_empty() {
        version = version.max(EXPRESSIONS);
    }
    if !doc.motion.compositions.is_empty()
        || doc
            .motion
            .tracks
            .iter()
            .flat_map(|t| &t.keys)
            .chain(doc.motion.compositions.iter().flat_map(|c| &c.time_remap))
            .any(|k| {
                matches!(
                    k.ease,
                    crate::motion::Ease::Hold | crate::motion::Ease::CubicBezier { .. }
                )
            })
    {
        version = version.max(COMPOSITIONS);
    }
    if doc.motion.scene3d.is_some() {
        version = version.max(SCENE3D);
    }
    if !doc.publishing.is_empty() {
        version = version.max(PUBLISHING);
    }
    for layer in &doc.layers {
        if layer.source_attachment.is_some() {
            version = version.max(SOURCE);
        }
        if layer.psd_type.is_some() {
            version = version.max(PSD_TYPE);
        }
        if layer.psd_smart.is_some() {
            version = version.max(SMART);
        }
        if layer.svg_filter.is_some() {
            version = version.max(SVG_FILTER);
        }
        if !layer.text_wrap.is_none() {
            version = version.max(10);
        }
        if layer.fill_opacity != 1. || layer.blend_interior || !layer.filters.items.is_empty() {
            version = version.max(7);
        }
        for shape in layer.kind.shapes().into_iter().flatten() {
            version = version.max(fill_version(&shape.style.fill));
            if let Some(stroke) = &shape.style.stroke {
                if stroke.alignment != StrokeAlignment::Center {
                    version = version.max(6);
                }
                if let Some(gradient) = &stroke.gradient {
                    version = version.max(gradient_version(gradient));
                }
            }
            if shape.mask.is_some() {
                version = version.max(6);
            }
            if shape.fill_opacity != 1. || shape.blend_interior || !shape.filters.items.is_empty() {
                version = version.max(7);
            }
            if !shape.text_wrap.is_none() {
                version = version.max(10);
            }
            let Geom::Text(run) = &shape.geom else {
                continue;
            };
            if run
                .paragraphs
                .iter()
                .any(|style| style.publishing_style.is_some())
                || run.spans.iter().any(|span| span.publishing_style.is_some())
            {
                version = version.max(PUBLISHING);
            }
            if run.wrap_width.is_some()
                || !run.paragraphs.is_empty()
                || !run.spans.is_empty()
                || matches!(run.align, crate::geom::TextAlign::Justify { .. })
            {
                version = version.max(8);
            }
            if run.on_path.is_some() {
                version = version.max(9);
            }
            if run.frame.is_some() || run.thread.is_some() {
                version = version.max(10);
            }
            if !run.features.is_empty() || run.spans.iter().any(|span| !span.features.is_empty()) {
                version = version.max(11);
            }
            if !run.manual_kern.is_empty()
                || run.spans.iter().any(|span| {
                    span.tracking.is_some()
                        || span.kerning.is_some()
                        || span.leading.is_some()
                        || span.baseline_shift.is_some()
                        || span.hscale.is_some()
                        || span.vscale.is_some()
                })
            {
                version = version.max(12);
            }
            if run.transform.is_some()
                || run.svg_text.is_some()
                || run
                    .spans
                    .iter()
                    .any(|span| span.font.is_some() || span.px.is_some() || span.fill.is_some())
            {
                version = version.max(RICH_TEXT);
            }
            for fill in run.spans.iter().filter_map(|span| span.fill.as_ref()) {
                version = version.max(fill_version(fill));
            }
        }
    }
    version
}

fn has_advanced_layout(doc: &Document) -> bool {
    use crate::layout::StackAlign;
    use crate::layout_components::ComponentBinding;

    let mut shapes: Vec<_> = doc
        .layers
        .iter()
        .filter_map(|layer| layer.kind.shapes())
        .flatten()
        .collect();
    while let Some(shape) = shapes.pop() {
        let layout = &shape.layout;
        if !layout.grid.is_auto()
            || layout.stack.as_ref().is_some_and(|stack| {
                !stack.column_tracks.is_empty()
                    || !stack.row_tracks.is_empty()
                    || stack.align == StackAlign::Baseline
            })
            || layout
                .breakpoints
                .iter()
                .any(|rule| rule.align == Some(StackAlign::Baseline))
        {
            return true;
        }
        match &layout.component {
            Some(ComponentBinding::Main { properties, .. }) if !properties.is_empty() => {
                return true;
            }
            Some(ComponentBinding::Instance { nodes, .. }) => {
                shapes.extend(nodes.iter().map(|node| node.baseline.as_ref()));
            }
            _ => {}
        }
    }
    false
}

fn fill_version(fill: &Fill) -> u32 {
    match fill {
        Fill::Pattern(_) => VECTOR_PATTERN,
        Fill::Gradient(gradient) => gradient_version(gradient),
        _ => 5,
    }
}

fn gradient_version(gradient: &Gradient) -> u32 {
    if gradient.radial_axis.is_some()
        || gradient.focal != [0., 0.]
        || gradient.inner_radius != 0.
        || gradient.spread != Spread::Pad
    {
        ADVANCED_GRADIENT
    } else {
        5
    }
}

/// Refuse new data declared readable by a reader that would discard it.
/// Args: document, envelope version and presence of a shared source table. Returns a compatibility error.
pub(super) fn validate_version(
    doc: &Document,
    declared: u32,
    pooled_sources: bool,
) -> Result<(), String> {
    crate::motion::compositions::documents(doc)?;
    let required = required_version(doc).max(if pooled_sources { VECTOR_PATTERN } else { 5 });
    if required >= RICH_TEXT && declared < required {
        return Err(format!(
            "Project declares version {declared}, but its editable data requires version {required}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Shape, Style};
    use crate::geom::{CharSpan, TypeRun};
    use std::sync::Arc;

    fn plain() -> Document {
        crate::formats::svg::read(r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24"><rect width="24" height="24" fill="#fff"/></svg>"##, "Version fixture").unwrap().0
    }

    fn pattern() -> Fill {
        Fill::Pattern(crate::svg_source::SvgPattern {
            definitions: Arc::from(
                r##"<pattern id="p" width="2" height="2" patternUnits="userSpaceOnUse"><path d="M0 0H1V2H0Z" fill="#345"/></pattern>"##,
            ),
            id: "p".into(),
            transform: [1., 0., 0., 1., 0., 0.],
            opacity: 1.,
        })
    }

    fn first_shape(doc: &mut Document) -> &mut Shape {
        &mut doc
            .layers
            .iter_mut()
            .find_map(|layer| layer.kind.shapes_mut())
            .unwrap()[0]
    }

    fn named_property(shape: &mut Shape) {
        use crate::layout_components::{ComponentBinding, ComponentProperty, PropertyKind};
        shape.visible = false;
        shape.layout.component = Some(ComponentBinding::Main {
            family: shape.id,
            variant: String::new(),
            slots: Default::default(),
            properties: vec![ComponentProperty {
                id: 1,
                name: "Private missing-slot field".into(),
                slot: 987654,
                kind: PropertyKind::Visibility,
            }],
        });
    }

    fn advanced_layout_cases() -> [(&'static str, fn(&mut Shape)); 9] {
        use crate::layout::{AutoStack, GridTrack, LayoutBreakpoint, StackAlign};
        [
            ("explicit first column", |shape| {
                shape.layout.grid.column = Some(0)
            }),
            ("explicit first row", |shape| {
                shape.layout.grid.row = Some(0)
            }),
            ("column span", |shape| shape.layout.grid.column_span = 2),
            ("row span", |shape| shape.layout.grid.row_span = 2),
            ("inactive column tracks", |shape| {
                shape.layout.stack = Some(AutoStack {
                    column_tracks: vec![GridTrack::default()],
                    ..Default::default()
                })
            }),
            ("inactive row tracks", |shape| {
                shape.layout.stack = Some(AutoStack {
                    row_tracks: vec![GridTrack::default()],
                    ..Default::default()
                })
            }),
            ("vertical baseline fallback", |shape| {
                shape.layout.stack = Some(AutoStack {
                    align: StackAlign::Baseline,
                    ..Default::default()
                })
            }),
            ("inactive baseline breakpoint", |shape| {
                shape.layout.breakpoints.push(LayoutBreakpoint {
                    max_width: 1.,
                    align: Some(StackAlign::Baseline),
                    ..Default::default()
                })
            }),
            ("hidden named property", named_property),
        ]
    }

    #[test]
    fn integrated_project_layout28_covers_stored_inactive_and_hidden_features() {
        for (name, edit) in advanced_layout_cases() {
            let mut doc = plain();
            edit(first_shape(&mut doc));
            doc.layers
                .iter_mut()
                .for_each(|layer| layer.visible = false);
            let encoded = super::super::encode(&doc).unwrap();
            let mut envelope: serde_json::Value = serde_json::from_str(&encoded).unwrap();
            assert_eq!(envelope["version"], ADVANCED_LAYOUT, "{name}");
            super::super::decode(&encoded).unwrap_or_else(|error| panic!("{name}: {error}"));
            envelope["version"] = SCENE3D.into();
            assert!(
                super::super::decode(&envelope.to_string())
                    .unwrap_err()
                    .contains("requires version 28"),
                "{name}"
            );
        }
    }

    #[test]
    fn integrated_project_layout28_checks_every_baseline_without_resolving_live_ids() {
        use crate::layout_components::{ComponentBinding, InstanceNode};
        for (name, edit) in advanced_layout_cases().into_iter().take(8) {
            let mut doc = plain();
            let mut baseline = first_shape(&mut doc).clone();
            edit(&mut baseline);
            let expected = baseline.layout.clone();
            let owner = first_shape(&mut doc);
            owner.layout.component = Some(ComponentBinding::Instance {
                main: 987650,
                nodes: vec![InstanceNode {
                    source: 987651,
                    instance: 987652,
                    slot: 987653,
                    nested_main: Some(987654),
                    baseline: Box::new(baseline),
                }],
            });
            assert_eq!(required_version(&doc), ADVANCED_LAYOUT, "{name}");
            let encoded = super::super::encode(&doc).unwrap();
            let mut envelope: serde_json::Value = serde_json::from_str(&encoded).unwrap();
            envelope["version"] = SCENE3D.into();
            assert!(
                super::super::decode(&envelope.to_string())
                    .unwrap_err()
                    .contains("requires version 28"),
                "{name}"
            );
            let mut reopened = super::super::decode(&encoded).unwrap();
            let Some(ComponentBinding::Instance { main, nodes }) =
                &first_shape(&mut reopened).layout.component
            else {
                panic!("baseline lost: {name}")
            };
            assert_eq!(*main, 987650);
            assert_eq!(nodes[0].instance, 987652);
            assert_eq!(nodes[0].baseline.layout, expected, "{name}");
        }
    }

    #[test]
    fn integrated_project_layout28_keeps_legacy_defaults_and_exact_supported_readers() {
        use crate::layout::{AutoStack, StackFlow};
        use crate::layout_components::{ComponentBinding, InstanceNode};
        let mut doc = plain();
        first_shape(&mut doc).layout.stack = Some(AutoStack {
            flow: StackFlow::Grid,
            ..Default::default()
        });
        assert_eq!(required_version(&doc), 5);
        first_shape(&mut doc).layout.component = Some(ComponentBinding::Main {
            family: 42,
            variant: "Legacy".into(),
            slots: Default::default(),
            properties: vec![],
        });
        assert_eq!(required_version(&doc), 5);
        let mut baseline = first_shape(&mut doc).clone();
        baseline.layout.component = None;
        first_shape(&mut doc).layout.component = Some(ComponentBinding::Instance {
            main: 1234,
            nodes: vec![InstanceNode {
                source: 1,
                instance: 2,
                slot: 3,
                nested_main: Some(4),
                baseline: Box::new(baseline),
            }],
        });
        assert_eq!(required_version(&doc), 5);
        let encoded = super::super::encode(&doc).unwrap();
        let mut envelope: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(envelope["version"], 5);
        for version in 0..=33 {
            let supported = matches!(
                version,
                1..=17 | 18 | 19 | 20 | 21 | 22 | 23 | 24 | 25 | 26 | 27 | 28 | 29 | 30 | 31 | 32
            );
            assert_eq!(supports_version(version), supported, "reader {version}");
            envelope["version"] = version.into();
            let result = super::super::decode(&envelope.to_string());
            if supported {
                result.unwrap_or_else(|error| panic!("reader {version}: {error}"));
            } else {
                assert!(
                    result.unwrap_err().contains("Unsupported"),
                    "reader {version}"
                );
            }
        }
    }

    #[test]
    fn integrated_project_layout28_nested_motion_save_reopen_preserves_baselines() {
        use crate::layout_components::{ComponentBinding, InstanceNode};
        let mut child = plain();
        let mut baseline = first_shape(&mut child).clone();
        baseline.layout.grid.row = Some(0);
        baseline.layout.stack = Some(crate::layout::AutoStack {
            row_tracks: vec![crate::layout::GridTrack::default()],
            ..Default::default()
        });
        let expected = baseline.layout.clone();
        first_shape(&mut child).layout.component = Some(ComponentBinding::Instance {
            main: 100,
            nodes: vec![InstanceNode {
                source: 101,
                instance: 102,
                slot: 103,
                nested_main: None,
                baseline: Box::new(baseline),
            }],
        });
        let mut middle = plain();
        middle
            .motion
            .compositions
            .push(crate::motion::compositions::Composition::new(child));
        let mut doc = plain();
        doc.motion
            .compositions
            .push(crate::motion::compositions::Composition::new(middle));
        let mut envelope: serde_json::Value =
            serde_json::from_str(&super::super::encode(&doc).unwrap()).unwrap();
        assert_eq!(envelope["version"], ADVANCED_LAYOUT);
        envelope["version"] = SCENE3D.into();
        assert!(
            super::super::decode(&envelope.to_string())
                .unwrap_err()
                .contains("requires version 28")
        );
        let directory = std::env::current_dir()
            .unwrap()
            .join(".artifacts")
            .join(format!("layout28-{}", crate::document::next_id()));
        assert!(directory.starts_with("/home"));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("nested.oma");
        super::super::save_to(&doc, &path).unwrap();
        let mut reopened = super::super::load_from(&path).unwrap();
        let child = &mut reopened.motion.compositions[0].document.motion.compositions[0].document;
        let Some(ComponentBinding::Instance { nodes, .. }) = &first_shape(child).layout.component
        else {
            panic!("nested baseline lost")
        };
        assert_eq!(nodes[0].baseline.layout, expected);
        assert_eq!(required_version(&reopened), ADVANCED_LAYOUT);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn integrated_project_layout28_rejects_malformed_metadata_without_deleting_missing_bindings() {
        use crate::layout_components::{ComponentBinding, InstanceNode};
        let mut valid = plain();
        named_property(first_shape(&mut valid));
        let encoded = super::super::encode(&valid).unwrap();
        let mut reopened = super::super::decode(&encoded).unwrap();
        assert_eq!(
            first_shape(&mut reopened).layout.component,
            first_shape(&mut valid).layout.component
        );
        for mode in 0..4 {
            let mut doc = valid.clone();
            let Some(ComponentBinding::Main { properties, .. }) =
                &mut first_shape(&mut doc).layout.component
            else {
                unreachable!()
            };
            match mode {
                0 => properties.push(properties[0].clone()),
                1 => properties[0].name = " ".into(),
                2 => properties[0].name = "x".repeat(121),
                _ => *properties = vec![properties[0].clone(); 257],
            }
            let mut parent = plain();
            parent
                .motion
                .compositions
                .push(crate::motion::compositions::Composition::new(doc));
            let error = super::super::decode(&super::super::encode(&parent).unwrap()).unwrap_err();
            assert!(
                error.contains("Component properties") || error.contains("256 properties"),
                "{error}"
            );
        }
        let mut doc = plain();
        let mut baseline = first_shape(&mut doc).clone();
        named_property(&mut baseline);
        first_shape(&mut doc).layout.component = Some(ComponentBinding::Instance {
            main: 100,
            nodes: vec![InstanceNode {
                source: 101,
                instance: 102,
                slot: 103,
                nested_main: None,
                baseline: Box::new(baseline),
            }],
        });
        assert_eq!(required_version(&doc), ADVANCED_LAYOUT);
        let error = super::super::decode(&super::super::encode(&doc).unwrap()).unwrap_err();
        assert!(error.contains("Instance baselines cannot contain component bindings"));
    }

    #[test]
    fn integrated_project_features_choose_the_highest_reader_and_reject_underdeclarations() {
        let mut doc = plain();
        assert_eq!(required_version(&doc), 5);
        let mut gradient = Gradient::new(
            crate::gradient::GradientKind::Radial,
            crate::color::Rgba::BLACK,
            crate::color::Rgba::WHITE,
        );
        doc.layers
            .iter_mut()
            .find_map(|layer| layer.kind.shapes_mut())
            .unwrap()[0]
            .style
            .fill = Fill::Gradient(gradient.clone());
        assert_eq!(required_version(&doc), 5);
        gradient.focal = [0.2, 0.1];
        doc.layers
            .iter_mut()
            .find_map(|layer| layer.kind.shapes_mut())
            .unwrap()[0]
            .style
            .fill = Fill::Gradient(gradient);
        assert_eq!(required_version(&doc), ADVANCED_GRADIENT);
        let mut json: serde_json::Value =
            serde_json::from_str(&super::super::encode(&doc).unwrap()).unwrap();
        assert_eq!(json["version"], ADVANCED_GRADIENT);
        json["version"] = RICH_TEXT.into();
        assert!(
            super::super::decode(&json.to_string())
                .unwrap_err()
                .contains("requires version 15")
        );

        let (mut graph, _) = crate::formats::svg::read(r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24"><defs><filter id="f"><feGaussianBlur stdDeviation="1"/></filter></defs><g filter="url(#f)"><rect width="20" height="20" fill="#369"/></g></svg>"##, "Graph version").unwrap();
        assert!(graph.layers.iter().any(|layer| layer.svg_filter.is_some()));
        graph.layers.extend(doc.layers);
        assert_eq!(required_version(&graph), SVG_FILTER);
        graph.layers.reverse();
        let encoded = super::super::encode(&graph).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&encoded).unwrap()["version"],
            SVG_FILTER
        );
        let mut underdeclared: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        underdeclared["version"] = VECTOR_PATTERN.into();
        assert!(
            super::super::decode(&underdeclared.to_string())
                .unwrap_err()
                .contains("requires version 17")
        );
    }

    #[test]
    fn integrated_project_span_patterns_keep_shared_sources_and_minimum_version() {
        let mut doc = plain();
        let paint = pattern();
        let shapes = doc
            .layers
            .iter_mut()
            .find_map(|layer| layer.kind.shapes_mut())
            .unwrap();
        shapes[0].style.fill = paint.clone();
        shapes.push(Shape::new(
            Geom::Text(TypeRun {
                content: "AB".into(),
                px: 12.,
                spans: vec![CharSpan {
                    start: 0,
                    end: 2,
                    fill: Some(paint),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            Style::default(),
        ));
        let encoded = super::super::encode(&doc).unwrap();
        let json: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(json["version"], VECTOR_PATTERN);
        assert_eq!(json["svg_sources"].as_object().unwrap().len(), 1);
        let reopened = super::super::decode(&encoded).unwrap();
        let shapes = reopened
            .layers
            .iter()
            .find_map(|layer| layer.kind.shapes())
            .unwrap();
        let Fill::Pattern(shape_paint) = &shapes[0].style.fill else {
            panic!("pattern fill lost")
        };
        let Geom::Text(run) = &shapes[1].geom else {
            panic!("editable type lost")
        };
        let Some(Fill::Pattern(span_paint)) = &run.spans[0].fill else {
            panic!("range pattern lost")
        };
        assert!(Arc::ptr_eq(
            &shape_paint.definitions,
            &span_paint.definitions
        ));
        assert!(span_paint.definitions.contains("<pattern"));
        let directory = std::env::current_dir()
            .unwrap()
            .join(".artifacts")
            .join(format!(
                "project-source-preview-{}",
                crate::document::next_id()
            ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("pooled.oma");
        std::fs::write(&path, &encoded).unwrap();
        let preview = crate::brand::load_preview_document(&path, false).unwrap();
        let preview_shapes = preview
            .layers
            .iter()
            .find_map(|layer| layer.kind.shapes())
            .unwrap();
        let Fill::Pattern(preview_paint) = &preview_shapes[0].style.fill else {
            panic!("library preview lost its pattern")
        };
        assert!(preview_paint.definitions.contains("<pattern"));
        let Geom::Text(preview_run) = &preview_shapes[1].geom else {
            panic!("library preview lost editable text")
        };
        let Some(Fill::Pattern(preview_span)) = &preview_run.spans[0].fill else {
            panic!("library preview lost range paint")
        };
        assert!(Arc::ptr_eq(
            &preview_paint.definitions,
            &preview_span.definitions
        ));
        std::fs::remove_dir_all(directory).unwrap();
        let plain = super::super::encode(&plain()).unwrap();
        let mut legacy: serde_json::Value = serde_json::from_str(&plain).unwrap();
        legacy["version"] = 1.into();
        super::super::decode(&legacy.to_string()).expect("plain legacy projects still open");
    }

    #[test]
    fn integrated_project_publishing_requires_its_reader_and_refuses_reserved_formats() {
        let mut doc = plain();
        assert!(doc.publishing.is_empty());
        assert_eq!(required_version(&doc), 5);
        doc.publishing.bleed = [1., 2., 3., 4.];
        doc.publishing
            .links
            .insert(42, "Links/retained-logo.png".into());
        let encoded = super::super::encode(&doc).unwrap();
        let mut envelope: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(envelope["version"], PUBLISHING);
        assert_eq!(
            super::super::decode(&encoded).unwrap().publishing,
            doc.publishing
        );
        envelope["version"] = SVG_FILTER.into();
        assert!(
            super::super::decode(&envelope.to_string())
                .unwrap_err()
                .contains("requires version 23")
        );

        let mut doc = plain();
        doc.layers
            .iter_mut()
            .find_map(|layer| layer.kind.shapes_mut())
            .unwrap()
            .push(Shape::new(
                Geom::Text(TypeRun {
                    content: "AB".into(),
                    px: 12.,
                    spans: vec![CharSpan {
                        start: 0,
                        end: 2,
                        publishing_style: Some("CharacterStyle/Retained".into()),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                Style::default(),
            ));
        assert_eq!(required_version(&doc), PUBLISHING);
        let shapes = doc
            .layers
            .iter_mut()
            .find_map(|layer| layer.kind.shapes_mut())
            .unwrap();
        let Geom::Text(run) = &mut shapes[1].geom else {
            panic!("expected editable text")
        };
        run.spans.clear();
        run.paragraphs.push(crate::geom::ParagraphStyle {
            publishing_style: Some("ParagraphStyle/Retained".into()),
            ..Default::default()
        });
        assert_eq!(required_version(&doc), PUBLISHING);
        assert!(super::super::decode(&super::super::encode(&doc).unwrap()).is_ok());

        let mut neutral: serde_json::Value =
            serde_json::from_str(&super::super::encode(&plain()).unwrap()).unwrap();
        for implemented in 1..=ADVANCED_LAYOUT {
            neutral["version"] = implemented.into();
            super::super::decode(&neutral.to_string()).unwrap();
        }
        for reserved in [CHANNELS + 1, u32::MAX] {
            neutral["version"] = reserved.into();
            assert!(
                super::super::decode(&neutral.to_string())
                    .unwrap_err()
                    .contains("Unsupported")
            );
        }
    }
}
