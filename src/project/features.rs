use crate::document::{Document, Fill, StrokeAlignment};
use crate::geom::Geom;
use crate::gradient::{Gradient, Spread};

pub(super) const RICH_TEXT: u32 = 14;
pub(super) const ADVANCED_GRADIENT: u32 = 15;
pub(super) const VECTOR_PATTERN: u32 = 16;
pub(super) const SVG_FILTER: u32 = 17;
pub(super) const PUBLISHING: u32 = 23;
pub(super) const EXPRESSIONS: u32 = 25;
pub(super) const COMPOSITIONS: u32 = 26;
pub(super) const SCENE3D: u32 = 27;

/// Identify implemented project readers.
/// Args: envelope version. Returns false for reserved formats whose models are absent.
pub(super) fn supports_version(version: u32) -> bool {
    matches!(version, 1..=17 | 23 | 25..=27)
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
        .unwrap_or(SCENE3D)
}

fn local_required_version(doc: &Document) -> u32 {
    let mut version = if doc.mask_sources().is_empty() { 5 } else { 13 };
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
        for reserved in SVG_FILTER + 1..PUBLISHING {
            neutral["version"] = reserved.into();
            assert!(
                super::super::decode(&neutral.to_string())
                    .unwrap_err()
                    .contains("Unsupported")
            );
        }
    }
}
