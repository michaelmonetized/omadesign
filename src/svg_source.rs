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
    let header = "<svg xmlns:xlink=\"http://www.w3.org/1999/xlink\">";
    let wrapper = format!("{header}{source}</svg>");
    let Ok(xml) = roxmltree::Document::parse(&wrapper) else {
        return source.to_string();
    };
    let ids: std::collections::HashSet<_> = xml
        .descendants()
        .filter_map(|n| n.attribute("id"))
        .collect();
    let mut edits = Vec::new();
    for node in xml.descendants().filter(|n| n.is_element()) {
        for attr in node.attributes() {
            let value = attr.value();
            let replacement = if attr.name() == "id" {
                format!("{prefix}{value}")
            } else if attr.name() == "href"
                && value.strip_prefix('#').is_some_and(|id| ids.contains(id))
            {
                format!("#{prefix}{}", &value[1..])
            } else {
                let lower = value.to_ascii_lowercase();
                let mut output = String::new();
                let mut cursor = 0;
                while let Some(start) = lower[cursor..].find("url(").map(|i| cursor + i) {
                    let Some(end) = lower[start + 4..].find(')').map(|i| start + 4 + i) else {
                        break;
                    };
                    output.push_str(&value[cursor..start]);
                    let argument = value[start + 4..end].trim().trim_matches(['\'', '"']);
                    if let Some(id) = argument.strip_prefix('#').filter(|id| ids.contains(id)) {
                        output.push_str(&format!("url(#{prefix}{id})"));
                    } else {
                        output.push_str(&value[start..=end]);
                    }
                    cursor = end + 1;
                }
                output.push_str(&value[cursor..]);
                output
            };
            if replacement != value && attr.range_value().start >= header.len() {
                let escaped = replacement
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('"', "&quot;")
                    .replace('\'', "&apos;");
                edits.push((
                    attr.range_value().start - header.len()..attr.range_value().end - header.len(),
                    escaped,
                ));
            }
        }
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut output = source.to_string();
    for (range, text) in edits {
        output.replace_range(range, &text);
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
        let Ok(tree) = scene(&source) else {
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SvgFilterGraph {
    pub definitions: Arc<str>,
    pub original_definitions: Arc<str>,
    pub filters: Vec<String>,
    pub transform: [f32; 6],
    #[serde(default)]
    pub reductions: Vec<String>,
}

impl SvgFilterGraph {
    /// Apply the retained filter graph in its original coordinate system.
    /// Args: export definitions, unique prefix and current editable vector content.
    /// Returns a vector group; invalid singular placement leaves the content intact.
    pub fn wrap(&self, definitions: &mut String, _prefix: &str, content: String) -> String {
        let placement = transform(self.transform);
        let Some(inverse) = placement.invert() else {
            return content;
        };
        let prefix = append_source(definitions, &self.definitions);
        let filters = self
            .filters
            .iter()
            .map(|id| format!("url(#{prefix}{id})"))
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "<g transform=\"{}\" filter=\"{filters}\"><g transform=\"{}\">{content}</g></g>",
            matrix(placement),
            matrix(inverse)
        )
    }
}

/// Identify SVG operations the current renderer cannot execute.
/// Args: filter XML. Returns one explicit reduction per unsupported primitive or external input.
pub fn reductions(source: &str) -> Vec<String> {
    let Ok(xml) = roxmltree::Document::parse_with_options(
        source,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: 400_000,
            entity_resolver: None,
        },
    ) else {
        return vec!["Invalid SVG effect source".into()];
    };
    let mut notes = Vec::new();
    for node in xml.descendants().filter(|n| n.is_element()) {
        let tag = node.tag_name().name();
        if tag.starts_with("fe")
            && !matches!(
                tag,
                "feBlend"
                    | "feColorMatrix"
                    | "feComponentTransfer"
                    | "feComposite"
                    | "feConvolveMatrix"
                    | "feDiffuseLighting"
                    | "feDisplacementMap"
                    | "feDistantLight"
                    | "feDropShadow"
                    | "feFlood"
                    | "feFuncR"
                    | "feFuncG"
                    | "feFuncB"
                    | "feFuncA"
                    | "feGaussianBlur"
                    | "feImage"
                    | "feMerge"
                    | "feMergeNode"
                    | "feMorphology"
                    | "feOffset"
                    | "fePointLight"
                    | "feSpecularLighting"
                    | "feSpotLight"
                    | "feTile"
                    | "feTurbulence"
            )
        {
            notes.push(format!(
                "SVG filter operation {tag} is not rendered; original source retained."
            ));
        }
        if tag == "feImage"
            && node.attributes().any(|a| {
                a.name() == "href" && !a.value().starts_with('#') && !a.value().starts_with("data:")
            })
        {
            notes.push("SVG filter linked feImage is not loaded; embed the image first.".into());
        }
        for attribute in ["in", "in2"] {
            if let Some(
                input @ ("BackgroundImage" | "BackgroundAlpha" | "FillPaint" | "StrokePaint"),
            ) = node.attribute(attribute)
            {
                notes.push(format!(
                    "SVG filter input {input} is reduced by the renderer; original source retained."
                ));
            }
        }
    }
    notes
}

/// Parse only self-contained vector source.
/// Returns SVG options that resolve embedded data while refusing file or network references.
pub fn options() -> usvg::Options<'static> {
    let mut options = crate::text::svg_type::options();
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
    let fill = |fill: &crate::document::Fill| match fill {
        crate::document::Fill::Pattern(_) => true,
        crate::document::Fill::Gradient(value) => gradient(value),
        _ => false,
    };
    doc.layers.iter().any(|layer| {
        layer.svg_filter.is_some()
            || layer.kind.shapes().is_some_and(|shapes| {
                shapes.iter().any(|shape| {
                    fill(&shape.style.fill)
                        || shape
                            .style
                            .stroke
                            .as_ref()
                            .and_then(|s| s.gradient.as_ref())
                            .is_some_and(gradient)
                        || matches!(&shape.geom, crate::geom::Geom::Text(run) if run.spans.iter().filter_map(|span|span.fill.as_ref()).any(fill))
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
        let key = seen.entry(source.clone()).or_insert_with(|| {
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
        if let Some(graph) = &mut layer.svg_filter {
            visit(&mut graph.definitions);
            visit(&mut graph.original_definitions);
        }
        for shape in layer.kind.shapes_mut().into_iter().flatten() {
            visit_fill_source(&mut shape.style.fill, visit);
            if let crate::geom::Geom::Text(run) = &mut shape.geom {
                for fill in run.spans.iter_mut().filter_map(|span| span.fill.as_mut()) {
                    visit_fill_source(fill, visit);
                }
            }
        }
    }
    if let Some(replica) = &mut doc.collaboration {
        crate::collaboration::resources::visit_sources(replica, visit);
    }
}

fn visit_fill_source(fill: &mut crate::document::Fill, visit: &mut impl FnMut(&mut Arc<str>)) {
    if let crate::document::Fill::Pattern(pattern) = fill {
        visit(&mut pattern.definitions);
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
    fn integrated_svg_sources_keep_text_inside_pattern_definitions() {
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="32"><defs><pattern id="p" width="64" height="32" patternUnits="userSpaceOnUse"><text x="2" y="24" font-size="20" font-family="Oma Missing Family">ABCD</text></pattern></defs><rect width="64" height="32" fill="url(#p)"/></svg>"##;
        let tree = usvg::Tree::from_str(source, &options()).unwrap();
        let mut pixels = tiny_skia::Pixmap::new(64, 32).unwrap();
        resvg::render(&tree, Transform::identity(), &mut pixels.as_mut());
        assert!(
            pixels
                .pixels()
                .iter()
                .filter(|pixel| pixel.alpha() > 0)
                .count()
                > 20,
            "retained pattern text must not disappear from an empty font database"
        );
    }

    #[test]
    fn svg_project_guard_covers_each_advanced_gradient_field_on_fill_and_stroke() {
        use crate::gradient::{Gradient, GradientKind, Spread};
        let (plain, _) = read(r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><rect width="20" height="20" fill="#fff" stroke="#000"/></svg>"##, "Guard").unwrap();
        let version = |doc: &crate::document::Document| {
            serde_json::from_str::<serde_json::Value>(&crate::project::encode(doc).unwrap())
                .unwrap()["version"]
                .as_u64()
                .unwrap()
        };
        assert_eq!(version(&plain), 5);
        let legacy = Gradient::new(
            GradientKind::Radial,
            crate::color::Rgba::BLACK,
            crate::color::Rgba::WHITE,
        );
        for field in 0..6 {
            let mut gradient = legacy.clone();
            match field {
                0 => {}
                1 => gradient.radial_axis = Some([0.2, 0.4]),
                2 => gradient.focal = [0.1, -0.2],
                3 => gradient.inner_radius = 0.15,
                4 => gradient.spread = Spread::Repeat,
                _ => gradient.spread = Spread::Reflect,
            }
            for stroke in [false, true] {
                let mut doc = plain.clone();
                let shape = doc
                    .layers
                    .iter_mut()
                    .filter_map(|l| l.kind.shapes_mut())
                    .flatten()
                    .next()
                    .unwrap();
                if stroke {
                    shape.style.stroke.as_mut().unwrap().gradient = Some(gradient.clone());
                } else {
                    shape.style.fill = Fill::Gradient(gradient.clone());
                }
                assert_eq!(
                    version(&doc),
                    if field == 0 { 5 } else { 15 },
                    "field={field} stroke={stroke}"
                );
                let reopened =
                    crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
                assert_eq!(requires_advanced_paint(&reopened), field != 0);
            }
        }
    }

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
        assert_eq!(file["version"], 16);
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

struct Scene {
    source: Arc<str>,
    tree: Arc<usvg::Tree>,
    bytes: usize,
}
thread_local! {
    static SCENES: std::cell::RefCell<std::collections::VecDeque<Scene>> = const { std::cell::RefCell::new(std::collections::VecDeque::new()) };
}

/// Reuse parsed vectors while each camera or export gets its own requested resolution.
/// Args: exact current SVG source. Returns a parsed scene, never a page-sized raster cache.
pub fn scene(source: &str) -> Result<Arc<usvg::Tree>, String> {
    if let Some(tree) = SCENES.with_borrow_mut(|scenes| {
        let index = scenes.iter().position(|s| s.source.as_ref() == source)?;
        let entry = scenes.remove(index)?;
        let tree = entry.tree.clone();
        scenes.push_front(entry);
        Some(tree)
    }) {
        return Ok(tree);
    }
    let mut options = options();
    if source.contains("<text") {
        options.fontdb_mut().load_system_fonts();
    }
    let tree = Arc::new(
        usvg::Tree::from_str(source, &options)
            .map_err(|e| format!("Invalid SVG vector source: {e}"))?,
    );
    let bytes = source.len().saturating_mul(32);
    if bytes <= 16 * 1024 * 1024 && !source.contains("data:") && !source.contains("<image") {
        SCENES.with_borrow_mut(|scenes| {
            scenes.push_front(Scene {
                source: Arc::from(source),
                tree: tree.clone(),
                bytes,
            });
            let mut total = 0usize;
            scenes.retain(|s| {
                total = total.saturating_add(s.bytes);
                total <= 64 * 1024 * 1024
            });
            scenes.truncate(8);
        });
    }
    Ok(tree)
}

#[cfg(test)]
mod scene_tests {
    use super::*;
    #[test]
    fn svg_scene_cache_reuses_vectors_across_view_scales_and_invalidates_edits() {
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100000" height="80000"><defs><filter id="fx"><feGaussianBlur stdDeviation="20"/></filter></defs><g filter="url(#fx)"><rect width="80000" height="70000" fill="#28a"/></g></svg>"##;
        let first = scene(source).unwrap();
        let second = scene(source).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        for scale in [0.002, 0.003] {
            let mut pixels = tiny_skia::Pixmap::new(200, 160).unwrap();
            resvg::render(
                &second,
                Transform::from_scale(scale, scale),
                &mut pixels.as_mut(),
            );
            assert_eq!(pixels.data().len(), 128000);
        }
        let changed = scene(&source.replace("#28a", "#e42")).unwrap();
        assert!(!Arc::ptr_eq(&first, &changed));
        SCENES.with_borrow(|scenes| {
            assert!(scenes.len() <= 8);
            assert!(scenes.iter().map(|s| s.bytes).sum::<usize>() <= 64 * 1024 * 1024);
        });
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

#[cfg(test)]
mod reference_tests {
    use super::*;
    #[test]
    fn svg_resource_namespaces_preserve_quoted_urls_and_linked_patterns() {
        let defs = r##"<linearGradient id="g"><stop stop-color="#28a"/><stop offset="1" stop-color="#fa6"/></linearGradient><pattern id="p" patternUnits="userSpaceOnUse" width="12" height="12"><rect width="8" height="12" style="fill: url( '#g' )"/></pattern><pattern id="alias" xlink:href="#p"/>"##;
        let draw = |defs: &str, id: &str| {
            let source = format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="48" height="36"><defs>{defs}</defs><path d="M0 0H48V36H0Z" fill="url(#{id})"/></svg>"##
            );
            let tree = usvg::Tree::from_str(&source, &options()).unwrap();
            let mut pixels = tiny_skia::Pixmap::new(48, 36).unwrap();
            resvg::render(&tree, Transform::identity(), &mut pixels.as_mut());
            pixels
        };
        let expected = draw(defs, "alias");
        assert!(expected.pixels().iter().any(|p| p.alpha() > 0));
        let actual = draw(&namespace(defs, "copy-"), "copy-alias");
        assert_eq!(expected.data(), actual.data());
    }
}
