//! SVG object import after standards-based CSS, transform and path normalization.
use crate::artwork_mask::{ArtworkMask, MaskMode, MaskSource};
use crate::color::{Blend, Rgba};
use crate::document::{Cap, Document, Fill, Join, Layer, Pixels, Shape, Stroke, Style};
use crate::geom::{Anchor, Geom, PathContour, Pt, TypeRun};
use std::collections::HashMap;
use tiny_skia::{PathSegment, Transform};

const MAX_PIXELS: u64 = 64 * 1024 * 1024;
const MAX_ELEMENTS: usize = 100_000;

#[derive(Default)]
struct Metadata {
    name: String,
    hidden: bool,
    locked: bool,
}

fn editable_element(tag: &str) -> bool {
    matches!(
        tag,
        "svg"
            | "g"
            | "a"
            | "path"
            | "rect"
            | "circle"
            | "ellipse"
            | "polygon"
            | "polyline"
            | "line"
            | "image"
            | "text"
            | "use"
    )
}

#[derive(Clone, Copy, Default)]
struct SvgVisibility {
    display_none: bool,
    visibility_hidden: bool,
}

// Use the same static selector model as usvg. Stylesheet rules are sorted by
// specificity by simplecss; the cascade below also retains !important precedence.
struct CssElement<'a, 'input>(roxmltree::Node<'a, 'input>);
impl simplecss::Element for CssElement<'_, '_> {
    fn parent_element(&self) -> Option<Self> {
        self.0.parent_element().map(Self)
    }
    fn prev_sibling_element(&self) -> Option<Self> {
        self.0.prev_sibling_element().map(Self)
    }
    fn has_local_name(&self, name: &str) -> bool {
        self.0.tag_name().name() == name
    }
    fn attribute_matches(&self, name: &str, operator: simplecss::AttributeOperator) -> bool {
        self.0
            .attribute(name)
            .is_some_and(|value| operator.matches(value))
    }
    fn pseudo_class_matches(&self, class: simplecss::PseudoClass) -> bool {
        matches!(class, simplecss::PseudoClass::FirstChild)
            && self.0.prev_sibling_element().is_none()
    }
}

fn cascaded_visibility(
    node: roxmltree::Node<'_, '_>,
    sheet: &simplecss::StyleSheet<'_>,
    property: &str,
) -> Option<String> {
    let mut winner: Option<(String, bool)> = None;
    let mut set = |value: &str, important: bool| {
        let value = value.trim().to_ascii_lowercase();
        let valid = matches!(value.as_str(), "inherit" | "initial" | "unset" | "revert")
            || if property == "visibility" {
                matches!(value.as_str(), "visible" | "hidden" | "collapse")
            } else {
                matches!(
                    value.as_str(),
                    "none"
                        | "inline"
                        | "block"
                        | "list-item"
                        | "inline-block"
                        | "table"
                        | "inline-table"
                        | "table-row-group"
                        | "table-header-group"
                        | "table-footer-group"
                        | "table-row"
                        | "table-column-group"
                        | "table-column"
                        | "table-cell"
                        | "table-caption"
                        | "contents"
                        | "flow"
                        | "flow-root"
                        | "flex"
                        | "inline-flex"
                        | "grid"
                        | "inline-grid"
                        | "ruby"
                        | "ruby-base"
                        | "ruby-text"
                        | "ruby-base-container"
                        | "ruby-text-container"
                )
            };
        if valid && !winner.as_ref().is_some_and(|(_, old)| *old && !important) {
            winner = Some((value, important));
        }
    };
    if let Some(value) = node.attribute(property) {
        set(value, false);
    }
    for rule in &sheet.rules {
        if rule.selector.matches(&CssElement(node)) {
            for declaration in &rule.declarations {
                if declaration.name.eq_ignore_ascii_case(property) {
                    set(declaration.value, declaration.important);
                }
            }
        }
    }
    for declaration in simplecss::DeclarationTokenizer::from(node.attribute("style").unwrap_or(""))
    {
        if declaration.name.eq_ignore_ascii_case(property) {
            set(declaration.value, declaration.important);
        }
    }
    winner.map(|(value, _)| value)
}

fn xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn preserve_hidden_artwork(
    xml: &roxmltree::Document<'_>,
    source: &str,
    edits: &mut Vec<(std::ops::Range<usize>, String)>,
) -> HashMap<roxmltree::NodeId, SvgVisibility> {
    let mut sheet = simplecss::StyleSheet::new();
    for node in xml.descendants().filter(|n| n.has_tag_name("style")) {
        if node.attribute("type").is_some_and(|t| t != "text/css") {
            continue;
        }
        if let Some(text) = node.text() {
            sheet.parse_more(text);
            let mut visible_sheet = simplecss::StyleSheet::parse(text);
            for rule in &mut visible_sheet.rules {
                rule.declarations.retain(|d| {
                    !d.name.eq_ignore_ascii_case("display")
                        && !d.name.eq_ignore_ascii_case("visibility")
                });
            }
            edits.push((
                node.range(),
                format!(
                    "<style type=\"text/css\">{}</style>",
                    xml_text(&visible_sheet.to_string())
                ),
            ));
        }
    }
    let mut states: HashMap<roxmltree::NodeId, SvgVisibility> = HashMap::new();
    for node in xml
        .descendants()
        .filter(|n| n.is_element() && !n.has_tag_name("style"))
    {
        let parent = node
            .parent_element()
            .and_then(|p| states.get(&p.id()))
            .copied()
            .unwrap_or_default();
        let display = cascaded_visibility(node, &sheet, "display");
        let visibility = cascaded_visibility(node, &sheet, "visibility");
        let state = SvgVisibility {
            display_none: match display.as_deref() {
                Some("none") => true,
                Some("inherit") => parent.display_none,
                _ => false,
            },
            visibility_hidden: match visibility.as_deref() {
                Some("visible" | "initial") => false,
                Some("hidden" | "collapse") => true,
                _ => parent.visibility_hidden,
            },
        };
        states.insert(node.id(), state);
        let hidden_text = node
            .ancestors()
            .find(|a| a.has_tag_name("text"))
            .and_then(|a| states.get(&a.id()))
            .is_some_and(|s| s.visibility_hidden || s.display_none);
        let retain = (editable_element(node.tag_name().name()) || hidden_text)
            && !node.ancestors().filter(|a| a.is_element()).any(|a| {
                matches!(
                    a.tag_name().name(),
                    "defs" | "clipPath" | "mask" | "pattern" | "marker" | "symbol"
                )
            });
        let display = if !retain && state.display_none {
            "none"
        } else {
            "inline"
        };
        let visibility = if !retain && state.visibility_hidden {
            "hidden"
        } else {
            "visible"
        };
        let mut style = String::new();
        for declaration in
            simplecss::DeclarationTokenizer::from(node.attribute("style").unwrap_or(""))
        {
            if declaration.name.eq_ignore_ascii_case("display")
                || declaration.name.eq_ignore_ascii_case("visibility")
            {
                continue;
            }
            style.push_str(&format!(
                "{}:{}{};",
                declaration.name,
                declaration.value,
                if declaration.important {
                    " !important"
                } else {
                    ""
                }
            ));
        }
        style.push_str(&format!("display:{display};visibility:{visibility}"));
        if let Some(attr) = node.attributes().find(|a| a.name() == "style") {
            edits.push((attr.range_value(), xml_text(&style)));
        } else {
            let start = node.range().start + 1;
            if let Some(offset) =
                source[start..].find(|c: char| c.is_whitespace() || c == '/' || c == '>')
            {
                edits.push((
                    start + offset..start + offset,
                    format!(" style=\"{}\"", xml_text(&style)),
                ));
            }
        }
    }
    states
}

pub fn read(svg: &str, name: &str) -> Result<(Document, Vec<String>), String> {
    if svg.len() > 256 * 1024 * 1024 {
        return Err("SVG exceeds the 256 MiB import limit".into());
    }
    // Desktop design tools commonly include the SVG 1.1 DOCTYPE. Accept that
    // declaration, as usvg does, without fetching its DTD or external entities.
    // Bound the initial XML parse as well as the editable-element count below.
    let xml = roxmltree::Document::parse_with_options(
        svg,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: (MAX_ELEMENTS * 4) as u32,
            entity_resolver: None,
        },
    )
    .map_err(|e| format!("Invalid SVG: {e}"))?;
    if xml.descendants().filter(|n| n.is_element()).count() > MAX_ELEMENTS {
        return Err("SVG has too many elements".into());
    }
    let mut metadata = HashMap::new();
    let mut edits = Vec::new();
    let mut warnings = Vec::new();
    let visibility = preserve_hidden_artwork(&xml, svg, &mut edits);
    // Keep hidden artwork editable: usvg deliberately discards display:none nodes.
    for (index, node) in xml.descendants().filter(|n| n.is_element()).enumerate() {
        if index >= MAX_ELEMENTS {
            return Err("SVG has too many elements".into());
        }
        let tag = node.tag_name().name();
        if matches!(
            tag,
            "script" | "foreignObject" | "animate" | "animateTransform" | "set"
        ) {
            warn(&mut warnings, format!("SVG {tag} content is not imported."));
        }
        if !editable_element(tag) {
            continue;
        }
        let generated = format!("oma-import-{index}");
        let id = node.attribute("id").unwrap_or(&generated).to_string();
        if node.attribute("id").is_none() {
            let start = node.range().start + 1;
            let offset = svg[start..]
                .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
                .ok_or("Invalid SVG element")?;
            edits.push((start + offset..start + offset, format!(" id=\"{id}\"")));
        }
        let state = visibility.get(&node.id()).copied().unwrap_or_default();
        let meta = Metadata {
            name: node
                .attribute(("http://www.inkscape.org/namespaces/inkscape", "label"))
                .or_else(|| {
                    node.children()
                        .find(|n| n.has_tag_name("title"))
                        .and_then(|n| n.text())
                })
                .unwrap_or(&id)
                .to_string(),
            locked: node.attribute((
                "http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd",
                "insensitive",
            )) == Some("true"),
            // Visibility inherits but a child may override it. Native group
            // visibility cannot express that, so put it on individual objects.
            hidden: state.display_none
                || visibility
                    .get(&xml.root_element().id())
                    .is_some_and(|v| v.display_none)
                || (!matches!(tag, "svg" | "g" | "a") && state.visibility_hidden),
        };
        for attr in node.attributes() {
            if tag == "image"
                && attr.name() == "href"
                && !attr.value().starts_with("data:")
                && !attr.value().starts_with('#')
            {
                warn(
                    &mut warnings,
                    format!(
                        "Linked image {} was not loaded. Embed it in the source document first.",
                        meta.name
                    ),
                );
            }
        }
        metadata.insert(id, meta);
    }
    let mut normalized = svg.to_string();
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    for (range, replacement) in edits {
        normalized.replace_range(range, &replacement);
    }
    let mut options = usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    let tree = usvg::Tree::from_str(&normalized, &options)
        .map_err(|e| format!("Could not parse SVG: {e}"))?;
    let (w, h) = (tree.size().width(), tree.size().height());
    let mut doc = Document::new(name, 1., 1., 96.);
    doc.width = w;
    doc.height = h;
    doc.artboards = crate::document::Artboard::tiled(1, w, h);
    doc.transparent = true;
    doc.layers.clear();
    let mut reader = Reader {
        doc,
        warnings,
        metadata,
        tree: &tree,
        pixel_bytes: 0,
        mask_sources: HashMap::new(),
    };
    reader.children(tree.root(), None, 0)?;
    if reader.doc.layers.is_empty() {
        return Err("SVG contains no supported artwork".into());
    }
    reader.doc.validate_hierarchy()?;
    Ok((reader.doc, reader.warnings))
}

struct Reader<'a> {
    doc: Document,
    warnings: Vec<String>,
    metadata: HashMap<String, Metadata>,
    tree: &'a usvg::Tree,
    pixel_bytes: u64,
    mask_sources: HashMap<usize, MaskSource>,
}
impl Reader<'_> {
    fn reserve(&mut self, w: u32, h: u32) -> Result<(), String> {
        self.pixel_bytes = self
            .pixel_bytes
            .saturating_add(u64::from(w) * u64::from(h) * 4);
        if self.pixel_bytes > 512 * 1024 * 1024 {
            return Err("SVG decoded layers exceed 512 MiB".into());
        }
        Ok(())
    }
    fn name(&self, id: &str, fallback: &str) -> String {
        self.metadata
            .get(id)
            .map(|m| m.name.clone())
            .unwrap_or_else(|| {
                if id.is_empty() {
                    fallback.to_string()
                } else {
                    id.to_string()
                }
            })
    }
    fn meta(&self, layer: &mut Layer, id: &str) {
        if let Some(m) = self.metadata.get(id) {
            layer.visible &= !m.hidden;
            layer.locked = m.locked;
        }
    }
    fn children(
        &mut self,
        group: &usvg::Group,
        parent: Option<u64>,
        depth: usize,
    ) -> Result<(), String> {
        if depth > 64 {
            return Err("SVG group nesting exceeds 64 levels".into());
        }
        for node in group.children() {
            if self.doc.layers.len() >= MAX_ELEMENTS {
                return Err("SVG has too many layers".into());
            }
            match node {
                usvg::Node::Group(g) => {
                    let filters = native_filters(g);
                    if filters.is_none() {
                        self.raster(
                            node,
                            parent,
                            "SVG filter appearance retained as a pixel layer",
                        )?;
                        continue;
                    }
                    let mut layer = Layer::group(self.name(g.id(), "Group"));
                    layer.parent = parent;
                    layer.opacity = g.opacity().get();
                    layer.blend = blend(g.blend_mode());
                    layer.pass_through = !g.isolate()
                        && g.opacity().get() == 1.
                        && g.blend_mode() == usvg::BlendMode::Normal;
                    self.meta(&mut layer, g.id());
                    layer.filters = filters.unwrap();
                    if let Some(clip) = g.clip_path() {
                        layer.artwork_masks.extend(self.clip_masks(
                            clip,
                            g.abs_transform(),
                            depth + 1,
                        )?);
                    }
                    if let Some(mask) = g.mask() {
                        layer.artwork_masks.extend(self.svg_masks(
                            mask,
                            g.abs_transform(),
                            depth + 1,
                        )?);
                    }
                    if !layer.artwork_masks.is_empty() {
                        layer.pass_through = false;
                    }
                    self.children(g, Some(layer.id), depth + 1)?;
                    self.doc.layers.push(layer);
                }
                usvg::Node::Path(path) => {
                    if !self.path(path, parent)? {
                        self.raster(
                            node,
                            parent,
                            "SVG paint or stroke retained as a pixel layer",
                        )?;
                    }
                }
                usvg::Node::Image(image) => self.image(node, image, parent)?,
                usvg::Node::Text(text) => {
                    if !self.text(text, parent)? {
                        warn(&mut self.warnings,"Complex SVG text retained as editable outlines; text content cannot be edited as type.".into());
                        self.children(text.flattened(), parent, depth + 1)?;
                    }
                }
            }
        }
        Ok(())
    }
    fn mask_source(
        &mut self,
        root: &usvg::Group,
        name: &str,
        depth: usize,
    ) -> Result<MaskSource, String> {
        let key = root as *const usvg::Group as usize;
        if let Some(source) = self.mask_sources.get(&key) {
            return Ok(*source);
        }
        let mut layer = Layer::group(name);
        layer.pass_through = false;
        self.children(root, Some(layer.id), depth + 1)?;
        let source = MaskSource {
            layer: layer.id,
            shape: None,
        };
        self.doc.layers.push(layer);
        self.mask_sources.insert(key, source);
        Ok(source)
    }
    fn clip_masks(
        &mut self,
        clip: &usvg::ClipPath,
        transform: Transform,
        depth: usize,
    ) -> Result<Vec<ArtworkMask>, String> {
        if depth > 64 {
            return Err("SVG clip nesting exceeds 64 levels".into());
        }
        let mut mask = ArtworkMask::new(self.mask_source(
            clip.root(),
            &self.name(clip.id(), "Clip"),
            depth,
        )?);
        mask.transform = matrix(transform.pre_concat(clip.transform()));
        let mut masks = vec![mask];
        if let Some(other) = clip.clip_path() {
            masks.extend(self.clip_masks(other, transform, depth + 1)?);
        }
        Ok(masks)
    }
    fn svg_masks(
        &mut self,
        source: &usvg::Mask,
        transform: Transform,
        depth: usize,
    ) -> Result<Vec<ArtworkMask>, String> {
        if depth > 64 {
            return Err("SVG mask nesting exceeds 64 levels".into());
        }
        let mut mask = ArtworkMask::new(self.mask_source(
            source.root(),
            &self.name(source.id(), "Mask"),
            depth,
        )?);
        mask.mode = if source.kind() == usvg::MaskType::Alpha {
            MaskMode::Alpha
        } else {
            MaskMode::Luminance
        };
        mask.transform = matrix(transform);
        let r = source.rect();
        mask.region = Some(crate::geom::Bounds::from_min_size(
            Pt::new(r.x(), r.y()),
            Pt::new(r.width(), r.height()),
        ));
        let mut masks = vec![mask];
        if let Some(other) = source.mask() {
            masks.extend(self.svg_masks(other, transform, depth + 1)?);
        }
        Ok(masks)
    }
    fn path(&mut self, path: &usvg::Path, parent: Option<u64>) -> Result<bool, String> {
        let transform = path.abs_transform();
        let Some(transformed) = path.data().clone().transform(transform) else {
            return Ok(false);
        };
        let geom = path_geom(
            &transformed,
            path.fill()
                .is_some_and(|f| f.rule() == usvg::FillRule::NonZero),
        );
        let Some(geom) = geom else {
            return Ok(true);
        };
        let bounds = geom.bbox();
        let fill = if let Some(f) = path.fill() {
            let Some(fill) = paint(f.paint(), f.opacity().get(), transform, bounds) else {
                return Ok(false);
            };
            fill
        } else {
            Fill::None
        };
        let stroke = if let Some(s) = path.stroke() {
            let Some(stroke_paint) = paint(s.paint(), s.opacity().get(), transform, bounds) else {
                return Ok(false);
            };
            let (sx, sy) = transform.get_scale();
            let axes_dot = transform.sx * transform.kx + transform.ky * transform.sy;
            if (sx - sy).abs() > 0.01
                || axes_dot.abs() > 0.0001 * (sx * sy).max(1.)
                || s.linejoin() == usvg::LineJoin::MiterClip
                || s.linejoin() == usvg::LineJoin::Miter
                    && (s.miterlimit().get() - 4.).abs() > 0.001
                || s.dasharray().is_some_and(|a| a.len() != 2)
            {
                return self.outlined_stroke(path, parent, geom, fill);
            }
            Some(Stroke {
                alignment: crate::document::StrokeAlignment::Center,
                gradient: stroke_paint.gradient(),
                color: stroke_paint.solid_or(Rgba::BLACK),
                width: s.width().get() * sx,
                cap: match s.linecap() {
                    usvg::LineCap::Butt => Cap::Butt,
                    usvg::LineCap::Round => Cap::Round,
                    usvg::LineCap::Square => Cap::Square,
                },
                join: match s.linejoin() {
                    usvg::LineJoin::Round => Join::Round,
                    usvg::LineJoin::Bevel => Join::Bevel,
                    _ => Join::Miter,
                },
                dash: s.dasharray().map(|a| (a[0] * sx, a[1] * sx)),
                dash_offset: s.dashoffset() * sx,
            })
        } else {
            None
        };
        let stroke_first = path.paint_order() == usvg::PaintOrder::StrokeAndFill;
        let separate = !fill.is_none()
            && stroke.is_some()
            && (stroke_first || geom.path_contours().iter().any(|(_, closed)| !closed));
        let border = separate.then(|| {
            Shape::new(
                geom.clone(),
                Style {
                    fill: Fill::None,
                    stroke: stroke.clone(),
                },
            )
        });
        let geom = if fill.is_none() {
            geom
        } else {
            closed_fill(geom)
        };
        let mut shape = Shape::new(
            geom,
            Style {
                fill,
                stroke: if separate { None } else { stroke },
            },
        );
        shape.name = self.name(path.id(), "Path");
        shape.visible = path.is_visible();
        let mut layer = Layer::vector(shape.name.clone());
        layer.parent = parent;
        self.meta(&mut layer, path.id());
        let mut shapes = vec![shape];
        if let Some(mut border) = border {
            border.name = format!("{} stroke", shapes[0].name);
            border.visible = shapes[0].visible;
            if stroke_first {
                shapes.insert(0, border);
            } else {
                shapes.push(border);
            }
        }
        layer.kind.shapes_mut().unwrap().extend(shapes);
        self.doc.layers.push(layer);
        Ok(true)
    }
    fn outlined_stroke(
        &mut self,
        path: &usvg::Path,
        parent: Option<u64>,
        geom: Geom,
        fill: Fill,
    ) -> Result<bool, String> {
        let Some(stroke) = path.stroke() else {
            return Ok(false);
        };
        let transform = path.abs_transform();
        let scale = tiny_skia::PathStroker::compute_resolution_scale(&transform);
        let settings = stroke.to_tiny_skia();
        let dashed = settings
            .dash
            .as_ref()
            .map(|dash| path.data().dash(dash, scale));
        let source = match &dashed {
            Some(Some(path)) => path,
            Some(None) => return Ok(false),
            None => path.data(),
        };
        let Some(outline) = source
            .stroke(&settings, scale)
            .and_then(|outline| outline.transform(transform))
        else {
            return Ok(false);
        };
        let Some(outline) = path_geom(&outline, true) else {
            return Ok(true);
        };
        let Some(paint) = paint(
            stroke.paint(),
            stroke.opacity().get(),
            transform,
            outline.bbox(),
        ) else {
            return Ok(false);
        };
        let name = self.name(path.id(), "Path");
        let mut border = Shape::new(
            outline,
            Style {
                fill: paint,
                stroke: None,
            },
        );
        border.name = format!("{name} stroke");
        border.visible = path.is_visible();
        let mut shapes = vec![];
        if !fill.is_none() {
            let mut inside = Shape::new(closed_fill(geom), Style { fill, stroke: None });
            inside.name = name.clone();
            inside.visible = path.is_visible();
            shapes.push(inside);
        }
        if path.paint_order() == usvg::PaintOrder::StrokeAndFill {
            shapes.insert(0, border);
        } else {
            shapes.push(border);
        }
        let mut layer = Layer::vector(name);
        layer.parent = parent;
        self.meta(&mut layer, path.id());
        layer.kind.shapes_mut().unwrap().extend(shapes);
        self.doc.layers.push(layer);
        warn(
            &mut self.warnings,
            "SVG stroke settings retained as editable vector outlines.".into(),
        );
        Ok(true)
    }
    fn image(
        &mut self,
        node: &usvg::Node,
        image: &usvg::Image,
        parent: Option<u64>,
    ) -> Result<(), String> {
        let t = image.abs_transform();
        let data = match image.kind() {
            usvg::ImageKind::JPEG(d)
            | usvg::ImageKind::PNG(d)
            | usvg::ImageKind::GIF(d)
            | usvg::ImageKind::WEBP(d) => d,
            _ => return self.raster(node, parent, "Embedded SVG image retained as pixels"),
        };
        if t.kx.abs() > 0.0001 || t.ky.abs() > 0.0001 || t.sx <= 0. || t.sy <= 0. {
            return self.raster(node, parent, "Transformed SVG image retained as pixels");
        }
        let rgba = super::decode_image(data)?;
        self.reserve(rgba.w, rgba.h)?;
        let mut layer = Layer::placed_raster(
            self.name(image.id(), "Image"),
            rgba,
            Pt::new(t.tx, t.ty),
            Pt::new(image.size().width() * t.sx, image.size().height() * t.sy),
        );
        layer.parent = parent;
        layer.visible = image.is_visible();
        self.meta(&mut layer, image.id());
        self.doc.layers.push(layer);
        Ok(())
    }
    fn text(&mut self, text: &usvg::Text, parent: Option<u64>) -> Result<bool, String> {
        let t = text.abs_transform();
        if t.kx.abs() > 0.0001
            || t.ky.abs() > 0.0001
            || t.sx <= 0.
            || t.sy <= 0.
            || text.chunks().len() != 1
            || text.rotate().iter().any(|v| v.abs() > 0.001)
            || text.dx().iter().any(|v| v.abs() > 0.001)
            || text.dy().iter().any(|v| v.abs() > 0.001)
            || text.writing_mode() != usvg::WritingMode::LeftToRight
        {
            return Ok(false);
        }
        let chunk = &text.chunks()[0];
        if chunk.spans().is_empty() || !matches!(chunk.text_flow(), usvg::TextFlow::Linear) {
            return Ok(false);
        }
        let span = &chunk.spans()[0];
        if span.font_size().get() * t.sy < 1. || !(0.01..=10.).contains(&(t.sx / t.sy)) {
            return Ok(false);
        }
        if chunk.spans().iter().any(|span| {
            span.small_caps()
                || !matches!(
                    span.dominant_baseline(),
                    usvg::DominantBaseline::Auto | usvg::DominantBaseline::Alphabetic
                )
                || !matches!(
                    span.alignment_baseline(),
                    usvg::AlignmentBaseline::Auto
                        | usvg::AlignmentBaseline::Baseline
                        | usvg::AlignmentBaseline::Alphabetic
                )
                || span.baseline_shift().iter().any(|shift| {
                    !matches!(
                        shift,
                        usvg::BaselineShift::Baseline | usvg::BaselineShift::Number(0.)
                    )
                })
                || span.decoration().underline().is_some()
                || span.decoration().overline().is_some()
                || span.decoration().line_through().is_some()
        }) {
            return Ok(false);
        }
        if span.stroke().is_some()
            || span.text_length().is_some()
            || span.word_spacing().abs() > 0.001
        {
            return Ok(false);
        }
        let fill = if let Some(f) = span.fill() {
            let usvg::Paint::Color(c) = f.paint() else {
                return Ok(false);
            };
            Fill::Solid(color(*c, f.opacity().get()))
        } else {
            Fill::None
        };
        for other in chunk.spans().iter().skip(1) {
            let other_fill = match other.fill() {
                Some(paint) => match paint.paint() {
                    usvg::Paint::Color(color_value) => {
                        Fill::Solid(color(*color_value, paint.opacity().get()))
                    }
                    _ => return Ok(false),
                },
                None => Fill::None,
            };
            if other_fill != fill
                || other.stroke().is_some()
                || other.text_length().is_some()
                || other.font_size() != span.font_size()
                || other.letter_spacing() != span.letter_spacing()
                || other.word_spacing() != span.word_spacing()
                || other.apply_kerning() != span.apply_kerning()
                || other.is_visible() != span.is_visible()
                || other.font().families() != span.font().families()
                || other.font().style() != span.font().style()
                || other.font().weight() != span.font().weight()
                || other.font().stretch() != span.font().stretch()
            {
                return Ok(false);
            }
        }
        let Some(glyph) = text
            .layouted()
            .iter()
            .flat_map(|s| s.positioned_glyphs.iter())
            .next()
        else {
            return Ok(false);
        };
        if text
            .layouted()
            .iter()
            .flat_map(|span| &span.positioned_glyphs)
            .any(|other| other.font != glyph.font)
        {
            return Ok(false);
        }
        let Some(face) = self.tree.fontdb().face(glyph.font) else {
            return Ok(false);
        };
        let native_axes = self
            .tree
            .fontdb()
            .with_face_data(glyph.font, |bytes, index| {
                let Ok(font) = rustybuzz::ttf_parser::Face::parse(bytes, index) else {
                    return false;
                };
                index == 0
                    && font.variation_axes().into_iter().all(|axis| {
                        chunk.spans().iter().all(|span| {
                            let explicit = span
                                .font()
                                .variations()
                                .iter()
                                .find(|value| value.tag == axis.tag.to_bytes());
                            let value = explicit.map(|value| value.value).unwrap_or_else(|| {
                                if axis.tag.to_bytes() == *b"opsz"
                                    && span.font_optical_sizing() == usvg::FontOpticalSizing::Auto
                                {
                                    span.font_size().get()
                                } else {
                                    axis.def_value
                                }
                            });
                            (value.clamp(axis.min_value, axis.max_value) - axis.def_value).abs()
                                < 0.0001
                        })
                    })
            })
            .unwrap_or(false);
        if !native_axes {
            return Ok(false);
        }
        let font = match &face.source {
            usvg::fontdb::Source::File(p) | usvg::fontdb::Source::SharedFile(p, _) => {
                p.to_string_lossy().into_owned()
            }
            _ => return Ok(false),
        };
        let mut run = TypeRun {
            content: chunk.text().to_string(),
            origin: Pt::new(
                chunk.x().unwrap_or(0.) * t.sx + t.tx,
                chunk.y().unwrap_or(0.) * t.sy + t.ty,
            ),
            px: span.font_size().get() * t.sy,
            tracking: span.letter_spacing() * t.sx,
            font,
            kern: span.apply_kerning(),
            ..Default::default()
        };
        if (t.sx - t.sy).abs() > 0.0001 {
            run.spans.push(crate::geom::CharSpan {
                start: 0,
                end: run.content.chars().count(),
                hscale: Some(t.sx / t.sy * 100.),
                ..Default::default()
            });
        }
        let width = crate::text::measure(&run).0;
        run.origin.x -= match chunk.anchor() {
            usvg::TextAnchor::Start => 0.,
            usvg::TextAnchor::Middle => width * 0.5,
            usvg::TextAnchor::End => width,
        };
        run.contours = crate::text::shape(&run);
        let mut shape = Shape::new(Geom::Text(run), Style { fill, stroke: None });
        shape.name = self.name(text.id(), "Text");
        shape.visible = span.is_visible();
        let mut layer = Layer::vector(shape.name.clone());
        layer.parent = parent;
        self.meta(&mut layer, text.id());
        layer.kind.shapes_mut().unwrap().push(shape);
        self.doc.layers.push(layer);
        Ok(true)
    }
    fn raster(
        &mut self,
        node: &usvg::Node,
        parent: Option<u64>,
        reason: &str,
    ) -> Result<(), String> {
        let Some(b) = node.abs_layer_bounding_box() else {
            return Ok(());
        };
        // Raster buffers sit on the document pixel grid. A fractional buffer
        // origin would resample the same antialiased edge again on placement.
        let left = b.x().floor();
        let top = b.y().floor();
        let (w, h) = (
            (b.right().ceil() - left).max(1.) as u32,
            (b.bottom().ceil() - top).max(1.) as u32,
        );
        if u64::from(w) * u64::from(h) > MAX_PIXELS {
            return Err("SVG layer exceeds 64 megapixels".into());
        }
        self.reserve(w, h)?;
        let mut pm = tiny_skia::Pixmap::new(w, h).ok_or("Could not allocate SVG layer")?;
        // render_node subtracts the absolute bounds but does not apply ancestor
        // transforms. Groups apply their own local transform during rendering;
        // paths and images already have their local transform in their parents.
        let ancestors = match node {
            usvg::Node::Group(group) => group.abs_transform().pre_concat(
                group
                    .transform()
                    .invert()
                    .ok_or("Singular SVG group transform")?,
            ),
            usvg::Node::Text(text) => text.abs_transform().pre_concat(
                text.flattened()
                    .transform()
                    .invert()
                    .ok_or("Singular SVG text transform")?,
            ),
            _ => node.abs_transform(),
        };
        let root_transform = Transform::from_translate(-left, -top)
            .pre_concat(ancestors)
            .pre_translate(b.x(), b.y());
        resvg::render_node(node, root_transform, &mut pm.as_mut())
            .ok_or("Could not render SVG layer")?;
        let name = self.name(node.id(), "Appearance");
        let mut layer = Layer::placed_raster(
            name.clone(),
            Pixels::from_pixmap(&pm)?,
            Pt::new(left, top),
            Pt::new(w as f32, h as f32),
        );
        layer.parent = parent;
        if let usvg::Node::Group(group) = node {
            layer.blend = blend(group.blend_mode());
        }
        self.meta(&mut layer, node.id());
        self.doc.layers.push(layer);
        warn(&mut self.warnings, format!("{reason}: {name}."));
        Ok(())
    }
}
fn native_filters(g: &usvg::Group) -> Option<crate::filter::FilterStack> {
    let mut stack = crate::filter::FilterStack::default();
    for filter in g.filters() {
        if filter.primitives().len() != 1 {
            return None;
        }
        match filter.primitives()[0].kind() {
            usvg::filter::Kind::GaussianBlur(blur)
                if (blur.std_dev_x().get() - blur.std_dev_y().get()).abs() < 0.001 =>
            {
                let (sx, sy) = g.abs_transform().get_scale();
                if (sx - sy).abs() > 0.001 {
                    return None;
                }
                stack.items.push(crate::filter::Fx::Blur {
                    std: blur.std_dev_x().get() * sx,
                });
            }
            _ => return None,
        }
    }
    Some(stack)
}
fn matrix(t: Transform) -> [f32; 6] {
    [t.sx, t.ky, t.kx, t.sy, t.tx, t.ty]
}

fn color(c: usvg::Color, a: f32) -> Rgba {
    Rgba::new(
        c.red,
        c.green,
        c.blue,
        (a.clamp(0., 1.) * 255.).round() as u8,
    )
}
fn paint(p: &usvg::Paint, alpha: f32, t: Transform, b: crate::geom::Bounds) -> Option<Fill> {
    match p {
        usvg::Paint::Color(c) => Some(Fill::Solid(color(*c, alpha))),
        usvg::Paint::LinearGradient(g) if g.spread_method() == usvg::SpreadMethod::Pad => {
            let xf = t.pre_concat(g.transform());
            let mut from = tiny_skia::Point::from_xy(g.x1(), g.y1());
            xf.map_point(&mut from);
            // A gradient is a scalar field: its normal transforms by the
            // inverse transpose, not like a geometric line segment. Mapping
            // both endpoints changes diagonal gradients under unequal scaling.
            let dx = f64::from(g.x2() - g.x1());
            let dy = f64::from(g.y2() - g.y1());
            let length_squared = dx * dx + dy * dy;
            let determinant =
                f64::from(xf.sx) * f64::from(xf.sy) - f64::from(xf.kx) * f64::from(xf.ky);
            if length_squared <= 0.0 || determinant.abs() < 1e-20 {
                return None;
            }
            let nx =
                (f64::from(xf.sy) * dx - f64::from(xf.ky) * dy) / (determinant * length_squared);
            let ny =
                (-f64::from(xf.kx) * dx + f64::from(xf.sx) * dy) / (determinant * length_squared);
            let normal_squared = nx * nx + ny * ny;
            if !normal_squared.is_finite() || normal_squared <= 0.0 {
                return None;
            }
            let to = tiny_skia::Point::from_xy(
                from.x + (nx / normal_squared) as f32,
                from.y + (ny / normal_squared) as f32,
            );
            let from = [
                (from.x - b.min.x) / b.width().max(0.001),
                (from.y - b.min.y) / b.height().max(0.001),
            ];
            let to = [
                (to.x - b.min.x) / b.width().max(0.001),
                (to.y - b.min.y) / b.height().max(0.001),
            ];
            let stops: Vec<_> = g
                .stops()
                .iter()
                .map(|s| crate::gradient::GradientStop {
                    offset: s.offset().get(),
                    color: color(s.color(), alpha * s.opacity().get()),
                })
                .collect();
            if stops.len() == 2 && stops[0].offset == 0. && stops[1].offset == 1. {
                Some(Fill::Linear {
                    from,
                    to,
                    c0: stops[0].color,
                    c1: stops[1].color,
                })
            } else {
                Some(Fill::Gradient(crate::gradient::Gradient {
                    kind: crate::gradient::GradientKind::Linear,
                    from,
                    to,
                    stops,
                }))
            }
        }
        usvg::Paint::RadialGradient(g)
            if g.spread_method() == usvg::SpreadMethod::Pad
                && g.fr().get() == 0.
                && (g.fx() - g.cx()).abs() < 0.0001
                && (g.fy() - g.cy()).abs() < 0.0001 =>
        {
            let xf = t.pre_concat(g.transform());
            let (sx, sy) = xf.get_scale();
            if (sx - sy).abs() > 0.0001 * sx.max(sy).max(1.)
                || (xf.sx * xf.kx + xf.ky * xf.sy).abs() > 0.0001 * (sx * sy).max(1.)
            {
                return None;
            }
            let mut center = tiny_skia::Point::from_xy(g.cx(), g.cy());
            let mut radius = tiny_skia::Point::from_xy(g.cx() + g.r().get(), g.cy());
            xf.map_point(&mut center);
            xf.map_point(&mut radius);
            let point = |p: tiny_skia::Point| {
                [
                    (p.x - b.min.x) / b.width().max(0.001),
                    (p.y - b.min.y) / b.height().max(0.001),
                ]
            };
            Some(Fill::Gradient(crate::gradient::Gradient {
                kind: crate::gradient::GradientKind::Radial,
                from: point(center),
                to: point(radius),
                stops: g
                    .stops()
                    .iter()
                    .map(|stop| crate::gradient::GradientStop {
                        offset: stop.offset().get(),
                        color: color(stop.color(), alpha * stop.opacity().get()),
                    })
                    .collect(),
            }))
        }
        _ => None,
    }
}
fn blend(mode: usvg::BlendMode) -> Blend {
    match mode {
        usvg::BlendMode::Normal => Blend::Normal,
        usvg::BlendMode::Multiply => Blend::Multiply,
        usvg::BlendMode::Screen => Blend::Screen,
        usvg::BlendMode::Overlay => Blend::Overlay,
        usvg::BlendMode::Darken => Blend::Darken,
        usvg::BlendMode::Lighten => Blend::Lighten,
        usvg::BlendMode::ColorDodge => Blend::ColorDodge,
        usvg::BlendMode::ColorBurn => Blend::ColorBurn,
        usvg::BlendMode::HardLight => Blend::HardLight,
        usvg::BlendMode::SoftLight => Blend::SoftLight,
        usvg::BlendMode::Difference => Blend::Difference,
        usvg::BlendMode::Exclusion => Blend::Exclusion,
        usvg::BlendMode::Hue => Blend::Hue,
        usvg::BlendMode::Saturation => Blend::Saturation,
        usvg::BlendMode::Color => Blend::Color,
        usvg::BlendMode::Luminosity => Blend::Luminosity,
    }
}
fn warn(warnings: &mut Vec<String>, s: String) {
    if !warnings.contains(&s) {
        warnings.push(s);
    }
}

fn closed_fill(mut geom: Geom) -> Geom {
    match &mut geom {
        Geom::Path { closed, .. } => *closed = true,
        Geom::Paths { paths, .. } => paths.iter_mut().for_each(|path| path.closed = true),
        _ => {}
    }
    geom
}

fn path_geom(path: &tiny_skia::Path, winding: bool) -> Option<Geom> {
    let mut paths = Vec::<(Vec<Anchor>, bool)>::new();
    let mut anchors = Vec::<Anchor>::new();
    let mut closed = false;
    for segment in path.segments() {
        match segment {
            PathSegment::MoveTo(p) => {
                if !anchors.is_empty() {
                    paths.push((std::mem::take(&mut anchors), closed));
                }
                closed = false;
                anchors.push(Anchor::corner(Pt::new(p.x, p.y)));
            }
            PathSegment::LineTo(p) => anchors.push(Anchor::corner(Pt::new(p.x, p.y))),
            PathSegment::QuadTo(c, p) => {
                let previous = anchors.last_mut()?;
                let end = Pt::new(p.x, p.y);
                let control = Pt::new(c.x, c.y);
                previous.h_out = (control - previous.pt) * (2. / 3.);
                let mut a = Anchor::corner(end);
                a.h_in = (control - end) * (2. / 3.);
                anchors.push(a);
            }
            PathSegment::CubicTo(c1, c2, p) => {
                let previous = anchors.last_mut()?;
                previous.h_out = Pt::new(c1.x, c1.y) - previous.pt;
                let mut a = Anchor::corner(Pt::new(p.x, p.y));
                a.h_in = Pt::new(c2.x, c2.y) - a.pt;
                anchors.push(a);
            }
            PathSegment::Close => {
                closed = true;
                if anchors.len() > 1 && (anchors[0].pt - anchors.last()?.pt).length() < 0.0001 {
                    let last = anchors.pop()?;
                    anchors[0].h_in = last.h_in;
                }
            }
        }
    }
    if !anchors.is_empty() {
        paths.push((anchors, closed));
    }
    if paths.is_empty() {
        None
    } else if paths.len() == 1 && !winding {
        let (anchors, closed) = paths.pop()?;
        Some(Geom::Path { anchors, closed })
    } else {
        Some(Geom::Paths {
            paths: paths
                .into_iter()
                .map(|(anchors, closed)| PathContour { anchors, closed })
                .collect(),
            winding,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_stroke_variants_remain_editable_vectors() {
        for source in [
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="160" height="100"><path d="M15 70L45 12L73 70" fill="#c72" stroke="#258" stroke-width="14" stroke-miterlimit="1.5"/></svg>"##,
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="160" height="100"><path transform="matrix(1.7 .1 .3 .8 8 9)" d="M5 5C15 75 40 5 65 70" fill="none" stroke="#258" stroke-width="6" stroke-linecap="round" stroke-dasharray="8 3 2 3"/></svg>"##,
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="160" height="100"><path d="M25 25H120V75H25Z" fill="#fc8" stroke="#258" stroke-width="18" paint-order="stroke fill"/></svg>"##,
        ] {
            let (doc, notes, maximum, mean) = compare_render(source);
            assert!(
                doc.layers.iter().all(|layer| layer.kind.pixels().is_none()),
                "{notes:?}"
            );
            assert!(maximum <= 64 && mean < 0.05, "max={maximum}, mean={mean}");
            doc.validate_hierarchy().unwrap();
        }
    }

    #[test]
    fn svg_radial_paint_keeps_editable_stops_and_radius() {
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="80"><defs><radialGradient id="p" gradientUnits="userSpaceOnUse" cx="37" cy="30" r="40"><stop stop-color="#128"/><stop offset=".4" stop-color="#4cc" stop-opacity=".6"/><stop offset="1" stop-color="white"/></radialGradient></defs><g transform="translate(5 6) scale(.8)"><path d="M5 5H95V75H5Z" fill="url(#p)"/></g></svg>"##;
        let (doc, notes, maximum, mean) = compare_render(source);
        assert!(notes.is_empty(), "{notes:?}");
        assert!(doc.layers.iter().all(|layer| layer.kind.pixels().is_none()));
        let gradient = doc
            .layers
            .iter()
            .filter_map(|layer| layer.kind.shapes())
            .flatten()
            .find_map(|shape| shape.style.fill.gradient())
            .unwrap();
        assert_eq!(gradient.kind, crate::gradient::GradientKind::Radial);
        assert_eq!(gradient.stops.len(), 3);
        assert!(maximum <= 1 && mean < 0.02, "max={maximum}, mean={mean}");
    }

    #[test]
    fn svg_stretched_equivalent_spans_remain_editable_text() {
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="240" height="80"><text x="12" y="45" font-family="sans-serif" font-size="28" fill="#248" transform="translate(4 6) scale(1.4 .8)"><tspan>Vec</tspan><tspan>tor</tspan></text></svg>"##;
        let (doc, notes, maximum, mean) = compare_render(source);
        assert!(notes.is_empty(), "{notes:?}");
        let run = doc
            .layers
            .iter()
            .filter_map(|layer| layer.kind.shapes())
            .flatten()
            .find_map(|shape| {
                if let Geom::Text(run) = &shape.geom {
                    Some(run)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(run.content, "Vector");
        assert!((run.spans[0].hscale.unwrap() - 175.).abs() < 0.001);
        assert!(mean < 0.1, "max={maximum}, mean={mean}");
        let restored = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(&restored).unwrap(),
            serde_json::to_value(&doc).unwrap()
        );
    }

    #[test]
    fn svg_inactive_variation_settings_do_not_outline_static_font_text() {
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="240" height="80"><text x="12" y="45" font-family="Liberation Sans" font-size="28" fill="#248" style="font-variation-settings:'wdth' 75" transform="scale(1.4 .8)">Vector</text></svg>"##;
        let (doc, notes, _, mean) = compare_render(source);
        assert!(notes.is_empty(), "{notes:?}");
        assert!(
            doc.layers
                .iter()
                .filter_map(|layer| layer.kind.shapes())
                .flatten()
                .any(|shape| matches!(shape.geom, Geom::Text(_)))
        );
        assert!(mean < 0.2, "mean={mean}");
    }

    #[test]
    fn svg_compound_curves_keep_bezier_handles() {
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><path fill="#369" fill-rule="evenodd" d="M10 50C10 5 90 5 90 50C90 95 10 95 10 50ZM30 50C30 75 70 75 70 50C70 25 30 25 30 50Z"/></svg>"##;
        let (doc, _, maximum, mean) = compare_render(source);
        let shape = doc
            .layers
            .iter()
            .filter_map(|layer| layer.kind.shapes())
            .flatten()
            .next()
            .unwrap();
        let Geom::Paths { paths, winding } = &shape.geom else {
            panic!("Compound curves became flattened points")
        };
        assert_eq!(paths.len(), 2);
        assert!(!winding);
        assert!(
            paths
                .iter()
                .all(|path| path.anchors.iter().any(|anchor| anchor.h_out.length() > 0.))
        );
        assert!(maximum <= 1 && mean < 0.02, "max={maximum}, mean={mean}");
    }

    #[test]
    fn svg_open_fill_and_nonzero_winding_keep_their_appearance() {
        for source in [
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="90"><path d="M10 75L50 10L90 75" fill="#fc8" stroke="#258" stroke-width="8"/></svg>"##,
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="90"><path d="M50 5L77 80L7 33H93L23 80Z" fill="#369" fill-rule="nonzero"/></svg>"##,
        ] {
            let (doc, notes, maximum, mean) = compare_render(source);
            assert!(notes.is_empty(), "{notes:?}");
            assert!(doc.layers.iter().all(|layer| layer.kind.pixels().is_none()));
            assert!(maximum <= 3 && mean < 0.02, "max={maximum}, mean={mean}");
        }
    }

    #[test]
    fn svg_text_styles_keep_their_appearance_when_type_cannot_represent_them() {
        for source in [
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="180" height="65"><text x="10" y="45" font-size="28" fill="#248">A<tspan baseline-shift="super">2</tspan></text></svg>"##,
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="180" height="65"><text x="10" y="45" font-size="28" fill="#248">Vec<tspan fill="#c42">tor</tspan></text></svg>"##,
        ] {
            let (doc, notes, maximum, mean) = compare_render(source);
            assert!(notes.iter().any(|note| note.contains("editable outlines")));
            assert!(doc.layers.iter().all(|layer| layer.kind.pixels().is_none()));
            assert!(maximum <= 3 && mean < 0.02, "max={maximum}, mean={mean}");
        }
    }

    #[test]
    fn large_vector_canvas_keeps_thousands_of_objects_editable() {
        use std::fmt::Write;

        let mut source = String::from(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="100000" height="80000"><g fill="#168f72">"##,
        );
        for index in 0..2000 {
            let x = index % 50 * 2000;
            let y = index / 50 * 2000;
            write!(
                source,
                r#"<rect x="{x}" y="{y}" width="800" height="800"/>"#
            )
            .unwrap();
        }
        source.push_str("</g></svg>");
        assert!(source.len() < 128 * 1024);

        let (doc, notes) = read(&source, "Large vector canvas").unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(doc.size(), Pt::new(100000., 80000.));
        assert!(doc.layers.iter().all(|layer| layer.kind.pixels().is_none()));
        assert_eq!(
            doc.layers
                .iter()
                .map(|layer| layer.kind.shapes().map_or(0, <[Shape]>::len))
                .sum::<usize>(),
            2000,
        );
        let restored = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
        assert_eq!(restored.size(), doc.size());
        assert_eq!(restored.layers.len(), doc.layers.len());

        let mut view = crate::compositor::View::default();
        view.fit(
            doc.size(),
            crate::geom::Bounds::from_min_size(Pt::ZERO, Pt::new(1000., 800.)),
        );
        let rendered = crate::compositor::render_view(
            &restored,
            view,
            1000,
            800,
            crate::compositor::Draft::none(),
        )
        .unwrap();
        assert_eq!((rendered.width(), rendered.height()), (1000, 800));
        assert!(
            rendered
                .pixels()
                .iter()
                .any(|pixel| u16::from(pixel.green()) > u16::from(pixel.red()) + 20)
        );
    }

    #[test]
    fn large_vector_canvas_budgets_raster_fallbacks_by_object_size() {
        let source = |size| {
            format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="100000" height="80000"><defs><pattern id="p" width="4" height="4" patternUnits="userSpaceOnUse"><rect width="2" height="2"/></pattern></defs><path d="M10 10h{size}v{size}H10Z" fill="url(#p)"/></svg>"##,
            )
        };
        let (doc, notes) = read(&source(30), "Small fallback").unwrap();
        assert_eq!(doc.size(), Pt::new(100000., 80000.));
        let pixels = doc
            .layers
            .iter()
            .find_map(|layer| layer.kind.pixels())
            .unwrap();
        assert!(pixels.w < 100 && pixels.h < 100);
        assert_eq!(notes.len(), 1);
        assert!(
            read(&source(9000), "Large fallback")
                .unwrap_err()
                .contains("SVG layer exceeds 64 megapixels")
        );
    }

    #[test]
    fn svg_11_doctype_keeps_transformed_artwork_editable() {
        // Affinity exports use this prolog and percentage dimensions, including
        // the BestWNC logo/icon that exposed the File > Place rejection.
        let source = r##"<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">
<svg xmlns="http://www.w3.org/2000/svg" width="100%" height="100%" viewBox="0 0 120 60">
  <g transform="translate(12 8)"><path id="brand-mark" d="M0 0H32V20H0Z" style="fill:#dd513b"/></g>
</svg>"##;
        let (doc, notes, maximum, mean) = compare_render(source);
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!((doc.width, doc.height), (120.0, 60.0));
        let mark = doc
            .layers
            .iter()
            .find(|layer| layer.name == "brand-mark")
            .unwrap()
            .kind
            .shapes()
            .unwrap();
        assert_eq!(mark.len(), 1);
        assert_eq!(mark[0].geom.bbox().min, Pt::new(12.0, 8.0));
        assert_eq!(mark[0].geom.bbox().size(), Pt::new(32.0, 20.0));
        assert_eq!(mark[0].style.fill, Fill::Solid(Rgba::rgb(221, 81, 59)));
        assert!(maximum <= 1 && mean < 0.01, "max={maximum}, mean={mean}");
        doc.validate_hierarchy().unwrap();
    }

    #[test]
    fn svg_doctype_does_not_resolve_external_entities() {
        for uri in ["file:///etc/hostname", "https://example.invalid/entity.txt"] {
            let source = format!(
                r#"<!DOCTYPE svg [<!ENTITY external SYSTEM "{uri}">]>
<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24">
  <text x="0" y="16">&external;</text>
</svg>"#,
            );
            let error = read(&source, "External entity").unwrap_err();
            assert!(error.contains("unknown entity reference"), "{error}");
        }
    }

    fn compare_render(source: &str) -> (Document, Vec<String>, u8, f64) {
        let (doc, notes) = read(source, "SVG appearance").unwrap();
        let actual =
            super::super::decode_image(&crate::compositor::export_png(&doc, 1).unwrap()).unwrap();
        let mut options = usvg::Options::default();
        options.fontdb_mut().load_system_fonts();
        let tree = usvg::Tree::from_str(source, &options).unwrap();
        let mut expected = tiny_skia::Pixmap::new(actual.w, actual.h).unwrap();
        resvg::render(&tree, Transform::identity(), &mut expected.as_mut());
        let actual = actual.to_pixmap().unwrap();
        let mut maximum = 0;
        let mut sum = 0u64;
        for (a, b) in actual.data().iter().zip(expected.data()) {
            let delta = a.abs_diff(*b);
            maximum = maximum.max(delta);
            sum += u64::from(delta);
        }
        (doc, notes, maximum, sum as f64 / actual.data().len() as f64)
    }

    #[test]
    fn fallback_paths_and_images_keep_ancestor_transforms() {
        let pattern = r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="48"><defs><pattern id="p" width="4" height="4" patternUnits="userSpaceOnUse"><rect width="2" height="4" fill="red"/></pattern></defs><g transform="translate(13 7) scale(2)"><rect x="1" y="2" width="12" height="10" fill="url(#p)"/></g></svg>"##;
        let (doc, _, max, _) = compare_render(pattern);
        assert_eq!(max, 0);
        assert!(doc.layers.iter().any(|l| l.kind.pixels().is_some()));
        let image = crate::photo::RgbaImage::new(
            2,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
            ],
        )
        .unwrap();
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(image.encode_png().unwrap());
        let source = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="48"><g transform="translate(30 6) rotate(25)"><image width="16" height="16" href="data:image/png;base64,{encoded}"/></g></svg>"#
        );
        let (_, _, max, mean) = compare_render(&source);
        assert!(max <= 1 && mean < 0.01, "max={max}, mean={mean}");
    }

    #[test]
    fn multi_stop_gradient_and_gradient_stroke_import_stay_editable() {
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="64"><defs><linearGradient id="g"><stop offset="0" stop-color="red"/><stop offset=".4" stop-color="green" stop-opacity=".5"/><stop offset="1" stop-color="blue"/></linearGradient></defs><rect x="12" y="12" width="64" height="36" fill="url(#g)" stroke="url(#g)" stroke-width="6"/></svg>"##;
        let (doc, notes, max, mean) = compare_render(source);
        assert!(notes.is_empty(), "{notes:?}");
        let shape = doc
            .layers
            .iter()
            .filter_map(|l| l.kind.shapes())
            .flatten()
            .find(|s| s.style.fill.gradient().is_some())
            .unwrap();
        assert_eq!(shape.style.fill.gradient().unwrap().stops.len(), 3);
        assert_eq!(
            shape
                .style
                .stroke
                .as_ref()
                .unwrap()
                .gradient
                .as_ref()
                .unwrap()
                .stops
                .len(),
            3
        );
        assert!(max <= 2 && mean < 0.1, "max={max}, mean={mean}");
    }

    #[test]
    fn two_stop_gradient_preserves_affine_scalar_field_as_native_fill() {
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="48"><defs><linearGradient id="p" x2="1" y2="1"><stop offset="0" stop-color="red"/><stop offset="1" stop-color="blue"/></linearGradient></defs><g transform="translate(8 4) scale(2 1)"><rect width="18" height="20" fill="url(#p)"/></g></svg>"##;
        let (doc, notes, max, mean) = compare_render(source);
        assert!(notes.is_empty(), "{notes:?}");
        assert!(
            doc.layers
                .iter()
                .any(|l| l.kind.shapes().is_some_and(|shapes| shapes
                    .iter()
                    .any(|s| matches!(s.style.fill, Fill::Linear { .. }))))
        );
        assert!(max <= 1 && mean < 0.02, "max={max}, mean={mean}");
    }

    #[test]
    fn fallback_filter_group_keeps_ancestor_transform_and_blend() {
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="48"><defs><filter id="p" x="-50%" y="-50%" width="200%" height="200%"><feOffset dx="3" dy="2"/><feGaussianBlur stdDeviation="1"/></filter></defs><g transform="translate(15 7) scale(2)"><g filter="url(#p)" style="mix-blend-mode:multiply"><rect width="10" height="8" fill="red"/></g></g></svg>"##;
        let (doc, _, max, mean) = compare_render(source);
        assert!(
            doc.layers
                .iter()
                .any(|l| l.kind.pixels().is_some() && l.blend == Blend::Multiply)
        );
        assert!(max <= 1 && mean < 0.02, "max={max}, mean={mean}");
    }
    #[test]
    fn stylesheet_hidden_groups_keep_artwork_and_honor_css_cascade() {
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="60">
          <style>
            svg > g.hidden { display: none }
            #shown { display: inline }
            .forced { display: none !important }
            #specific { display: inline !important }
            .invisible { visibility: hidden }
            .invisible > rect { visibility: visible }
          </style>
          <g id="hidden" class="hidden"><rect id="hidden-art" width="8" height="8"/></g>
          <g id="shown" class="hidden" display="none"><rect id="shown-art" width="8" height="8"/></g>
          <g id="inline" class="hidden" style="display:inline"><rect width="8" height="8"/></g>
          <g id="forced" class="forced" style="display:inline"><rect width="8" height="8"/></g>
          <g id="inline-important" class="forced" style="display:inline !important"><rect width="8" height="8"/></g>
          <g id="specific" class="forced"><rect width="8" height="8"/></g>
          <g id="visibility" class="invisible"><rect id="override-art" width="8" height="8"/></g>
        </svg>"##;
        let (doc, notes) = read(source, "CSS layers").unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        let find = |name: &str| doc.layers.iter().position(|l| l.name == name).unwrap();
        assert!(!doc.layers[find("hidden")].visible);
        assert!(!doc.layer_visible(find("hidden-art")));
        assert_eq!(doc.layers.iter().filter(|l| !l.is_group).count(), 7);
        for name in [
            "shown",
            "shown-art",
            "inline",
            "inline-important",
            "specific",
            "override-art",
        ] {
            assert!(doc.layer_visible(find(name)), "{name}");
        }
        assert!(!doc.layers[find("forced")].visible);
    }

    #[test]
    fn imports_css_nested_layers_hidden_content_and_cubic_handles() {
        let (doc,notes)=read(r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape" width="80" height="60"><g id="outer" inkscape:label="Artwork" opacity="0.5" transform="translate(5 7)"><path id="curve" style="fill:none;stroke:#ff0000;stroke-width:2" d="M0 0 C10 0 20 20 30 20"/><g id="hidden" style="display:none"><rect width="10" height="10" fill="blue"/></g></g></svg>"#,"sample").unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!((doc.width, doc.height), (80., 60.));
        let outer = doc.layers.iter().find(|l| l.name == "Artwork").unwrap();
        assert!(outer.is_group);
        assert_eq!(outer.opacity, 0.5);
        let curve = doc.layers.iter().find(|l| l.name == "curve").unwrap();
        assert_eq!(curve.parent, Some(outer.id));
        let Geom::Path { anchors, .. } = &curve.kind.shapes().unwrap()[0].geom else {
            panic!()
        };
        assert_eq!(anchors[0].pt, Pt::new(5., 7.));
        assert_eq!(anchors[0].h_out, Pt::new(10., 0.));
        let hidden = doc.layers.iter().find(|l| l.name == "hidden").unwrap();
        assert!(!hidden.visible);
        assert!(doc.layers.iter().any(|l| l.parent == Some(hidden.id)));
    }
    #[test]
    fn vector_clips_keep_child_geometry_and_match_svg_pixels() {
        let source = r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="30"><defs><clipPath id="clip"><rect x="3" y="2" width="12" height="10"/></clipPath></defs><g transform="translate(6 4)" clip-path="url(#clip)"><rect width="30" height="25" fill="red"/></g></svg>"#;
        let (doc, notes) = read(source, "clip").unwrap();
        assert!(!notes.iter().any(|s| s.contains("pixel masks")));
        assert!(
            doc.layers
                .iter()
                .any(|l| l.is_group && !l.artwork_masks.is_empty())
        );
        assert!(doc.layers.iter().all(|l| l.mask.is_none()));
        assert_eq!(doc.layers.iter().filter(|l| !l.is_group).count(), 2);
        let actual =
            super::super::decode_image(&crate::compositor::export_png(&doc, 1).unwrap()).unwrap();
        let tree = usvg::Tree::from_str(source, &usvg::Options::default()).unwrap();
        let mut expected = tiny_skia::Pixmap::new(40, 30).unwrap();
        resvg::render(&tree, Transform::identity(), &mut expected.as_mut());
        let expected = Pixels::from_pixmap(&expected).unwrap();
        assert_eq!(actual.data.as_slice(), expected.data.as_slice());
        let exported = crate::svg::export(&doc).unwrap();
        let (roundtrip, _) = read(&exported, "again").unwrap();
        assert_eq!(
            roundtrip
                .layers
                .iter()
                .filter(|l| !l.artwork_masks.is_empty())
                .count(),
            1
        );
    }
}
