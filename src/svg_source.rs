//! Retained vector definitions shared by SVG paint and filter graphs.
use crate::geom::Bounds;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tiny_skia::Transform;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SvgPattern {
    pub definitions: Arc<str>,
    pub id: String,
    pub transform: [f32; 6],
    pub opacity: f32,
}

/// Rebind definition IDs for an independent exported object.
/// Args: normalized SVG definitions and a unique prefix. Returns collision-free XML.
pub fn namespace(source: &str, prefix: &str) -> String {
    let mut output = source.to_string();
    let wrapper = format!("<svg>{source}</svg>");
    let Ok(xml) = roxmltree::Document::parse(&wrapper) else {
        return output;
    };
    let ids: Vec<_> = xml
        .descendants()
        .filter_map(|n| n.attribute("id"))
        .map(str::to_string)
        .collect();
    for id in ids {
        output = output
            .replace(&format!("id=\"{id}\""), &format!("id=\"{prefix}{id}\""))
            .replace(&format!("url(#{id})"), &format!("url(#{prefix}{id})"))
            .replace(
                &format!("href=\"#{id}\""),
                &format!("href=\"#{prefix}{id}\""),
            );
    }
    output
}

/// Encode a stored affine matrix.
/// Args: six SVG coefficients. Returns the corresponding drawing transform.
pub fn transform(matrix: [f32; 6]) -> Transform {
    let [a, b, c, d, e, f] = matrix;
    Transform::from_row(a, b, c, d, e, f)
}

/// Format an affine map for portable SVG attributes.
/// Args: drawing transform. Returns a matrix expression.
pub fn matrix(t: Transform) -> String {
    format!(
        "matrix({} {} {} {} {} {})",
        t.sx, t.ky, t.kx, t.sy, t.tx, t.ty
    )
}

impl SvgPattern {
    /// Bind a reusable vector pattern to the current object bounds.
    /// Args: unique definition prefix and object bounds. Returns definitions and the paint URL.
    pub fn definition(&self, prefix: &str, bounds: Bounds) -> (String, String) {
        let mut definitions = String::new();
        let reference = self.append_definition(&mut definitions, prefix, bounds);
        (definitions, reference)
    }

    /// Append a pattern instance while sharing its vector resources.
    /// Args: output definitions, instance prefix and current object bounds. Returns the paint URL.
    pub fn append_definition(
        &self,
        definitions: &mut String,
        prefix: &str,
        bounds: Bounds,
    ) -> String {
        let placement = Transform::from_translate(bounds.min.x, bounds.min.y)
            .pre_scale(bounds.width().max(0.001), bounds.height().max(0.001))
            .pre_concat(transform(self.transform));
        let id = format!("{prefix}paint");
        let resource_prefix = append_source(definitions, &self.definitions);
        definitions.push_str(&format!(
            "<pattern id=\"{id}\" href=\"#{resource_prefix}{}\" patternTransform=\"{}\"/>",
            self.id,
            matrix(placement)
        ));
        format!("url(#{id})")
    }

    /// Render vector paint on a path at the requested viewport resolution.
    /// Args: destination, path, object bounds, view placement, opacity, blend and mask.
    /// Returns without changing the destination when source definitions are invalid.
    pub fn draw(
        &self,
        pm: &mut tiny_skia::Pixmap,
        path: &tiny_skia::Path,
        bounds: Bounds,
        placement: Transform,
        opacity: f32,
        blend: tiny_skia::BlendMode,
        mask: Option<&tiny_skia::Mask>,
        winding: bool,
    ) {
        let (defs, paint) = self.definition("oma-pattern-", bounds);
        let source = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\"><defs>{defs}</defs><path d=\"{}\" fill=\"{paint}\" fill-opacity=\"{}\" fill-rule=\"{}\"/></svg>",
            pm.width(),
            pm.height(),
            path_data(path),
            self.opacity.clamp(0., 1.),
            if winding { "nonzero" } else { "evenodd" }
        );
        let Ok(tree) = usvg::Tree::from_str(&source, &options()) else {
            return;
        };
        let Some(mut surface) = tiny_skia::Pixmap::new(pm.width(), pm.height()) else {
            return;
        };
        resvg::render(&tree, placement, &mut surface.as_mut());
        pm.draw_pixmap(
            0,
            0,
            surface.as_ref(),
            &tiny_skia::PixmapPaint {
                opacity,
                blend_mode: blend,
                ..Default::default()
            },
            Transform::identity(),
            mask,
        );
    }
}

/// Write exact curve segments without flattening them.
/// Args: native path. Returns portable SVG path data.
pub fn path_data(path: &tiny_skia::Path) -> String {
    use tiny_skia::PathSegment::*;
    let mut out = String::new();
    for segment in path.segments() {
        match segment {
            MoveTo(p) => out.push_str(&format!("M{} {}", p.x, p.y)),
            LineTo(p) => out.push_str(&format!("L{} {}", p.x, p.y)),
            QuadTo(a, b) => out.push_str(&format!("Q{} {} {} {}", a.x, a.y, b.x, b.y)),
            CubicTo(a, b, c) => {
                out.push_str(&format!("C{} {} {} {} {} {}", a.x, a.y, b.x, b.y, c.x, c.y))
            }
            Close => out.push('Z'),
        }
    }
    out
}

/// Parse only self-contained vector source.
/// Returns SVG options that resolve embedded data while refusing file or network references.
pub fn options() -> usvg::Options<'static> {
    let mut options = usvg::Options::default();
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    options
}

/// Protect advanced paint from older readers that ignore new fields.
/// Args: document to save. Returns whether the SVG paint project format is required.
pub fn requires_advanced_paint(doc: &crate::document::Document) -> bool {
    let gradient = |g: &crate::gradient::Gradient| {
        g.radial_axis.is_some()
            || g.focal != [0., 0.]
            || g.inner_radius != 0.
            || g.spread != crate::gradient::Spread::Pad
    };
    doc.layers.iter().any(|layer| {
        layer.kind.shapes().is_some_and(|shapes| {
            shapes.iter().any(|shape| {
                matches!(&shape.style.fill, crate::document::Fill::Pattern(_))
                    || shape.style.fill.gradient().as_ref().is_some_and(gradient)
                    || shape
                        .style
                        .stroke
                        .as_ref()
                        .and_then(|s| s.gradient.as_ref())
                        .is_some_and(gradient)
            })
        })
    })
}

/// Keep only resources reachable from selected definitions.
/// Args: self-contained definition XML and root IDs. Returns the transitive vector resource closure.
pub fn subset(source: &str, ids: &[String]) -> String {
    use std::collections::{HashMap, HashSet};
    let wrapper = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\">{source}</svg>"
    );
    let Ok(xml) = roxmltree::Document::parse(&wrapper) else {
        return source.to_string();
    };
    let roots: Vec<_> = xml
        .root_element()
        .children()
        .filter(|n| n.is_element())
        .collect();
    let mut owners = HashMap::new();
    for (index, root) in roots.iter().enumerate() {
        for node in root.descendants() {
            if let Some(id) = node.attribute("id") {
                owners.insert(id, index);
            }
        }
    }
    let mut selected = HashSet::new();
    let mut pending = ids.to_vec();
    while let Some(id) = pending.pop() {
        let Some(&index) = owners.get(id.as_str()) else {
            continue;
        };
        if !selected.insert(index) {
            continue;
        }
        for node in roots[index].descendants() {
            for attr in node.attributes() {
                let value = attr.value();
                if attr.name() == "href" {
                    if let Some(id) = value.strip_prefix('#') {
                        pending.push(id.to_string());
                    }
                }
                for part in value.split("url(").skip(1) {
                    if let Some(value) = part.split(')').next() {
                        if let Some(id) = value.trim().trim_matches(['\'', '"']).strip_prefix('#') {
                            pending.push(id.to_string());
                        }
                    }
                }
            }
        }
    }
    roots
        .iter()
        .enumerate()
        .filter(|(i, _)| selected.contains(i))
        .map(|(_, node)| &wrapper[node.range()])
        .collect()
}

/// Serialize source definitions into inert, consistently quoted SVG XML.
/// Args: definition XML. Returns canonical XML, excluding executable content.
pub fn canonicalize(source: &str) -> Option<String> {
    let wrapper = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\">{source}</svg>"
    );
    let xml = roxmltree::Document::parse(&wrapper).ok()?;
    Some(xml.root_element().children().map(xml_node).collect())
}

/// Encode one resource while excluding executable content.
/// Args: parsed SVG node. Returns equivalent standalone XML with escaped attributes.
pub(crate) fn xml_node(node: roxmltree::Node<'_, '_>) -> String {
    fn escaped(value: &str) -> String {
        value
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('"', "&quot;")
    }
    if node.is_text() {
        return escaped(node.text().unwrap_or_default());
    }
    if !node.is_element()
        || matches!(
            node.tag_name().name(),
            "script" | "foreignObject" | "animate" | "animateTransform" | "set"
        )
    {
        return String::new();
    }
    let name = node.tag_name().name();
    let mut out = format!("<{name}");
    for attr in node.attributes().filter(|a| !a.name().starts_with("on")) {
        out.push_str(&format!(" {}=\"{}\"", attr.name(), escaped(attr.value())));
    }
    out.push('>');
    for child in node.children() {
        out.push_str(&xml_node(child));
    }
    out.push_str(&format!("</{name}>"));
    out
}

/// Share vector resources in the native file instead of repeating them per object.
/// Args: document copy being saved. Returns the packed source table.
pub fn pack_sources(
    doc: &mut crate::document::Document,
) -> std::collections::BTreeMap<String, Arc<str>> {
    use std::collections::{BTreeMap, HashMap};
    let mut sources = BTreeMap::new();
    let mut seen = HashMap::new();
    visit_sources(doc, &mut |source| {
        let pointer = Arc::as_ptr(source) as *const u8 as usize;
        let key = seen.entry(pointer).or_insert_with(|| {
            let key = format!("@oma-svg-source-{}", sources.len());
            sources.insert(key.clone(), source.clone());
            key
        });
        *source = Arc::from(key.as_str());
    });
    sources
}

/// Restore shared definitions after reading a project.
/// Args: decoded document and its packed source table. Returns an error for missing resources.
pub fn unpack_sources(
    doc: &mut crate::document::Document,
    sources: &std::collections::BTreeMap<String, Arc<str>>,
) -> Result<(), String> {
    let mut error = None;
    visit_sources(doc, &mut |source| {
        if let Some(shared) = sources.get(source.as_ref()) {
            *source = shared.clone();
        } else if source.starts_with("@oma-svg-source-") {
            error = Some(format!("Missing vector source {source}"));
        }
    });
    error.map_or(Ok(()), Err)
}

/// Visit each source reference without copying its XML.
/// Args: mutable document and an operation on each shared definition reference.
fn visit_sources(doc: &mut crate::document::Document, visit: &mut impl FnMut(&mut Arc<str>)) {
    for layer in &mut doc.layers {
        for shape in layer.kind.shapes_mut().into_iter().flatten() {
            if let crate::document::Fill::Pattern(pattern) = &mut shape.style.fill {
                visit(&mut pattern.definitions);
            }
        }
    }
}

/// Reuse a source definition across exported objects.
/// Args: output definitions and shared source XML. Returns its content-based namespace.
pub fn append_source(definitions: &mut String, source: &Arc<str>) -> String {
    use sha2::{Digest, Sha256};
    let prefix = format!("oma-resource-{:x}-", Sha256::digest(source.as_bytes()));
    let marker = format!("<!--{}-->", prefix.trim_end_matches('-'));
    if !definitions.contains(&marker) {
        definitions.push_str(&marker);
        definitions.push_str(&namespace(source, &prefix));
    }
    prefix
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{document::Fill, formats::svg::read};

    #[test]
    fn svg_sources_share_prune_pack_and_reopen_without_pattern_id_collisions() {
        let shapes = (0..120)
            .map(|i| {
                format!(
                    "<rect x=\"{}\" y=\"0\" width=\"5\" height=\"10\" fill=\"url(#paint)\"/>",
                    i * 5
                )
            })
            .collect::<String>();
        let source = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="600" height="10"><defs><pattern id="paint" patternUnits="userSpaceOnUse" width="4" height="4"><path d="M0 0H2V4H0Z" fill="#28a"/></pattern><pattern id="unused" width="1" height="1"><rect width="1" height="1"/></pattern></defs>{shapes}</svg>"##
        );
        let (doc, _) = read(&source, "Shared resources").unwrap();
        let paints: Vec<_> = doc
            .layers
            .iter()
            .filter_map(|l| l.kind.shapes())
            .flatten()
            .filter_map(|s| {
                if let Fill::Pattern(p) = &s.style.fill {
                    Some(p)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(paints.len(), 120);
        assert!(Arc::ptr_eq(
            &paints[0].definitions,
            &paints[119].definitions
        ));
        assert!(!paints[0].definitions.contains("unused"));
        let encoded = crate::project::encode(&doc).unwrap();
        let file: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(file["version"], 14);
        assert_eq!(file["svg_sources"].as_object().unwrap().len(), 1);
        let restored = crate::project::decode(&encoded).unwrap();
        let exported = crate::svg::export(&restored).unwrap();
        let xml = roxmltree::Document::parse(&exported).unwrap();
        let ids: Vec<_> = xml
            .descendants()
            .filter_map(|n| n.attribute("id"))
            .collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len());
        assert_eq!(exported.matches("M0 0H2V4H0Z").count(), 0);
        assert_eq!(exported.matches("<!--oma-resource-").count(), 1);
        let actual = crate::compositor::render_export_at(&restored, 1., 0., true).unwrap();
        assert!(actual.pixels().iter().any(|p| p.alpha() > 0));
        let tree = usvg::Tree::from_str(&source, &options()).unwrap();
        let mut expected = tiny_skia::Pixmap::new(600, 10).unwrap();
        resvg::render(&tree, Transform::identity(), &mut expected.as_mut());
        let sum: u64 = actual
            .data()
            .iter()
            .zip(expected.data())
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum();
        assert!(sum as f64 / (actual.data().len() as f64) < 0.1);
    }
}

#[cfg(test)]
mod independent_tests {
    #[test]
    #[ignore = "requires rsvg-convert; uses only disposable home-project scratch"]
    fn svg_paint_independent_librsvg_and_disk_reopen() {
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="180" height="140"><defs><radialGradient id="radial" gradientUnits="userSpaceOnUse" cx="40" cy="40" r="34" fx="32" fy="35" fr="4" gradientTransform="matrix(1 .1 .35 .8 -8 4)"><stop stop-color="#28a"/><stop offset=".4" stop-color="#6d8"/><stop offset="1" stop-color="#f64"/></radialGradient><linearGradient id="reflect" gradientUnits="userSpaceOnUse" x2="20" y2="12" spreadMethod="reflect"><stop stop-color="#164"/><stop offset="1" stop-color="#fe8"/></linearGradient><symbol id="tile" viewBox="0 0 10 10"><path d="M0 0H5V10H0Z" fill="#28a"/><circle cx="7" cy="5" r="2" fill="#e73"/></symbol><pattern id="paint" patternUnits="userSpaceOnUse" width="12" height="10" patternTransform="skewX(15)"><use href="#tile" width="12" height="10"/></pattern></defs><path d="M8 8H82V95H8Z" fill="url(#radial)"/><path d="M94 8H172V65H94Z" fill="url(#reflect)"/><path d="M8 103H172V132H8Z" fill="url(#paint)"/></svg>"##;
        let (doc, notes) = crate::formats::svg::read(source, "Independent paint").unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        assert!(doc.layers.iter().all(|l| l.kind.pixels().is_none()));
        let dir = std::env::current_dir()
            .unwrap()
            .join(".artifacts")
            .join(format!(
                "svg-paint-{}-{}",
                std::process::id(),
                crate::document::next_id()
            ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("paint.oma");
        crate::project::save_to(&doc, &path).unwrap();
        let reopened = crate::project::load_from(&path).unwrap();
        let svg = crate::svg::export(&reopened).unwrap();
        assert!(!svg.contains("<image"));
        let svg_path = dir.join("paint.svg");
        std::fs::write(&svg_path, svg).unwrap();
        let png = dir.join("reference.png");
        let result = std::process::Command::new("rsvg-convert")
            .arg(&svg_path)
            .arg("-o")
            .arg(&png)
            .status()
            .unwrap();
        assert!(result.success());
        let expected = tiny_skia::Pixmap::decode_png(&std::fs::read(&png).unwrap()).unwrap();
        let actual = crate::compositor::render_export_at(&reopened, 1., 0., true).unwrap();
        let sum: u64 = actual
            .data()
            .iter()
            .zip(expected.data())
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum();
        let mean = sum as f64 / actual.data().len() as f64;
        if mean < 1.5 {
            std::fs::remove_dir_all(&dir).unwrap();
        } else {
            actual.save_png(dir.join("native.png")).unwrap();
            std::fs::write(dir.join("source.svg"), source).unwrap();
            for (label, (x, y, w, h)) in [
                ("radial", (8, 8, 74, 87)),
                ("reflect", (94, 8, 78, 57)),
                ("pattern", (8, 103, 164, 29)),
            ] {
                let sum: u64 = (y..y + h)
                    .flat_map(|y| (x..x + w).map(move |x| (x, y)))
                    .map(|(x, y)| {
                        let at = ((y * actual.width() + x) * 4) as usize;
                        actual.data()[at..at + 4]
                            .iter()
                            .zip(&expected.data()[at..at + 4])
                            .map(|(a, b)| u64::from(a.abs_diff(*b)))
                            .sum::<u64>()
                    })
                    .sum();
                println!(
                    "{label} independent mean={}",
                    sum as f64 / (w * h * 4) as f64
                );
            }
            println!("Independent failure artifacts: {}", dir.display());
        }
        println!("independent librsvg SVG paint mean channel error={mean}");
        assert!(
            mean < 1.5,
            "independent librsvg SVG paint mean channel error={mean}"
        );
    }
}
