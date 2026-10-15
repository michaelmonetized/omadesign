use super::*;
use crate::{
    document::{Artboard, Document, Layer, Shape},
    geom::{Anchor, CharSpan, Geom, Leading, ParagraphStyle, TextAlign, TypeRun},
    publishing::{MasterSpread, PublishingStyle},
    text_geometry::{TextFrame, TextThread, VAlign},
};
use base64::Engine as _;
use std::{
    collections::{HashMap, HashSet},
    io::{Cursor, Read, Write},
};

/// Read editable publishing content. Args: ZIP bytes and document name. Returns document and conversion notes.
pub fn read(bytes: &[u8], name: &str) -> Result<(Document, Vec<String>), String> {
    read_with_links(bytes, name, &|_| None)
}

/// Read with a caller-controlled local link reader. Args: package, name, URI callback. Returns editable document and notes.
pub fn read_with_links(
    bytes: &[u8],
    name: &str,
    link: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<(Document, Vec<String>), String> {
    read_with_budget(bytes, name, link, MAX_TOTAL)
}

/// Limit owned import buffers. Args: package, name, link reader, byte allowance. Returns document or a budget error.
pub(super) fn read_with_budget(
    bytes: &[u8],
    name: &str,
    link: &dyn Fn(&str) -> Option<Vec<u8>>,
    allowance: usize,
) -> Result<(Document, Vec<String>), String> {
    let parts = unzip(bytes)?;
    let design = parts
        .get("designmap.xml")
        .ok_or("IDML has no designmap.xml")?;
    let dm = parse(design)?;
    if tag(dm.root_element()) != "Document" {
        return Err("IDML designmap root must be Document".into());
    }
    let mut sources = Vec::new();
    let mut seen = HashSet::new();
    for n in dm.root_element().children().filter(|n| n.is_element()) {
        if let Some(src) = n.attribute("src") {
            if !seen.insert(src) {
                return Err(format!("Duplicate IDML include: {src}"));
            }
            sources.push((
                src,
                parts
                    .get(src)
                    .ok_or_else(|| format!("Missing IDML part: {src}"))?
                    .as_slice(),
            ));
        }
    }
    let trees = sources
        .iter()
        .map(|(name, bytes)| parse(bytes).map(|doc| (*name, doc)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut im = Importer {
        doc: Document::new(name, 1., 1., 72.),
        notes: vec![],
        colors: BTreeMap::from([
            ("Color/Black".into(), Rgba::BLACK),
            ("Color/Paper".into(), Rgba::WHITE),
        ]),
        stories: HashMap::new(),
        story_colors: HashMap::new(),
        frames: Vec::new(),
        parts: &parts,
        link,
        budget: AllocationBudget { used: 0, allowance },
    };
    im.doc.layers.clear();
    im.doc.artboards.clear();
    im.doc.transparent = false;
    for (_, tree) in &trees {
        im.resources(tree.root_element())?;
    }
    for (_, tree) in &trees {
        for n in tree
            .descendants()
            .filter(|n| tag(*n) == "Story" && n.attribute("Self").is_some())
        {
            im.story(n)?;
        }
    }
    for (_, tree) in &trees {
        for n in tree
            .descendants()
            .filter(|n| tag(*n) == "MasterSpread" && n.attribute("Self").is_some())
        {
            im.doc.publishing.masters.push(MasterSpread {
                id: attr(n, "Self"),
                name: attr(n, "Name"),
                xml: tree.input_text()[n.range()].into(),
            });
        }
    }
    let master_stories: HashSet<String> = im
        .doc
        .publishing
        .masters
        .iter()
        .map(|m| parse(m.xml.as_bytes()))
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .flat_map(|tree| {
            tree.descendants()
                .filter_map(|n| n.attribute("ParentStory"))
                .map(str::to_owned)
        })
        .collect();
    for (_, tree) in &trees {
        for n in tree.descendants().filter(|n| {
            tag(*n) == "Story"
                && n.attribute("Self")
                    .is_some_and(|id| master_stories.contains(id))
        }) {
            im.doc
                .publishing
                .master_stories
                .insert(attr(n, "Self"), tree.input_text()[n.range()].into());
        }
    }
    im.master_dependencies(dm.root_element())?;
    let mut x = 0.;
    for (_, tree) in &trees {
        for n in tree
            .descendants()
            .filter(|n| tag(*n) == "Spread" && n.attribute("Self").is_some())
        {
            x = im.spread(n, x)?;
        }
    }
    if im.doc.artboards.is_empty() {
        return Err("IDML has no pages".into());
    }
    im.threads()?;
    im.stories.clear();
    im.reserve_layout()?;
    im.doc.width = x.max(1.) - 48.;
    im.doc.height = im
        .doc
        .artboards
        .iter()
        .map(|a| a.origin.y + a.size.y)
        .fold(1., f32::max);
    im.doc.publishing.swatches = im.colors.clone();
    im.doc.bleed = im.doc.publishing.bleed.into_iter().fold(0., f32::max);
    im.doc.show_bleed = im.doc.bleed > 0.;
    crate::text_geometry::reflow(&mut im.doc);
    for l in &im.doc.layers {
        if im.doc.publishing.inherited_layers.contains(&l.id) {
            im.doc
                .publishing
                .inherited_fingerprints
                .insert(l.id, fingerprint(l)?);
        }
    }
    im.doc.validate_hierarchy()?;
    Ok((im.doc, im.notes))
}
fn parse(bytes: &[u8]) -> Result<roxmltree::Document<'_>, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "IDML XML is not UTF-8")?;
    roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 200_000,
            ..Default::default()
        },
    )
    .map_err(|e| format!("Invalid IDML XML: {e}"))
}
fn unzip(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
    if bytes.len() > MAX_TOTAL {
        return Err("IDML package exceeds 256 MiB".into());
    }
    let mut zip =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("Invalid IDML ZIP: {e}"))?;
    if zip.is_empty() || zip.len() > 4096 {
        return Err("IDML needs 1–4096 ZIP entries".into());
    }
    let mut parts = BTreeMap::new();
    let mut total = 0;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = f.name().to_string();
        if i == 0 && (name != "mimetype" || f.compression() != zip::CompressionMethod::Stored) {
            return Err("IDML must start with stored mimetype".into());
        }
        if f.is_dir() {
            continue;
        }
        if name.starts_with('/')
            || name.contains('\\')
            || name
                .split('/')
                .any(|p| p == ".." || p == "." || p.is_empty())
        {
            return Err("Unsafe IDML package path".into());
        }
        if parts.contains_key(&name) {
            return Err("Duplicate IDML ZIP entry".into());
        }
        if f.size() > MAX_PART || total as u64 + f.size() > MAX_TOTAL as u64 {
            return Err("IDML expanded data exceeds its budget".into());
        }
        let mut data = Vec::new();
        f.by_ref()
            .take(MAX_PART + 1)
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        total += data.len();
        if data.len() as u64 > MAX_PART || total > MAX_TOTAL {
            return Err("IDML expanded data exceeds its budget".into());
        }
        parts.insert(name, data);
    }
    if parts.get("mimetype").map(Vec::as_slice) != Some(MIMETYPE.as_bytes()) {
        return Err("Not an IDML package".into());
    }
    Ok(parts)
}
struct Frame {
    xml: String,
    id: u64,
    story: String,
    prev: String,
    next: String,
}
struct AllocationBudget {
    used: usize,
    allowance: usize,
}
impl AllocationBudget {
    /// Count retained image allocations. Args: decoded pixels. Returns an error above the import allowance.
    fn charge_pixels(&mut self, pixels: &crate::document::Pixels) -> Result<(), String> {
        let (samples, cmyk, profile) = pixels.native.as_ref().map_or((0, 0, 0), |native| {
            (
                native.samples.bytes(),
                native.cmyk.as_ref().map_or(0, |inks| inks.bytes()),
                native.icc_profile.bytes(),
            )
        });
        for bytes in [pixels.data.bytes(), samples, cmyk, profile] {
            self.charge(usize::try_from(bytes).map_err(|_| "IDML allocation budget overflow")?)?;
        }
        Ok(())
    }

    /// Reserve owned buffers before duplication. Args: byte count. Returns an error above the aggregate allowance.
    fn charge(&mut self, bytes: usize) -> Result<(), String> {
        self.used = self
            .used
            .checked_add(bytes)
            .ok_or("IDML allocation budget overflow")?;
        if self.used > self.allowance {
            return Err(
                "IDML owned text, image and layout buffers exceed the allocation budget".into(),
            );
        }
        Ok(())
    }
}
/// Count allocated text buffers. Args: native run. Returns capacities including nested style strings.
fn run_storage(run: &TypeRun) -> usize {
    run.content.capacity()
        + run.font.capacity()
        + run.spans.capacity() * std::mem::size_of::<CharSpan>()
        + run.paragraphs.capacity() * std::mem::size_of::<ParagraphStyle>()
        + run.features.capacity() * std::mem::size_of::<([u8; 4], u32)>()
        + run
            .spans
            .iter()
            .map(|s| {
                s.font.as_ref().map_or(0, String::capacity)
                    + s.publishing_style.as_ref().map_or(0, String::capacity)
                    + s.features.capacity() * std::mem::size_of::<([u8; 4], u32)>()
            })
            .sum::<usize>()
        + run
            .paragraphs
            .iter()
            .map(|p| {
                p.publishing_style.as_ref().map_or(0, String::capacity)
                    + p.hyphen.as_ref().map_or(0, |h| h.language.capacity())
            })
            .sum::<usize>()
}
#[derive(Default)]
struct ByteCount(usize);
impl Write for ByteCount {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .ok_or_else(|| std::io::Error::other("IDML layout size overflow"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
struct Importer<'a> {
    doc: Document,
    notes: Vec<String>,
    colors: BTreeMap<String, Rgba>,
    stories: HashMap<String, TypeRun>,
    story_colors: HashMap<String, Rgba>,
    frames: Vec<Frame>,
    parts: &'a BTreeMap<String, Vec<u8>>,
    link: &'a dyn Fn(&str) -> Option<Vec<u8>>,
    budget: AllocationBudget,
}
impl Importer<'_> {
    fn master_dependencies(&mut self, design: Node<'_, '_>) -> Result<(), String> {
        for source in self
            .doc
            .publishing
            .masters
            .iter_mut()
            .map(|m| &mut m.xml)
            .chain(self.doc.publishing.master_stories.values_mut())
        {
            self.budget.charge(source.capacity())?;
            let tree = parse(source.as_bytes())?;
            let mut edits = Vec::new();
            let mut embedded = HashSet::new();
            for n in tree.descendants() {
                if let Some(id) = n
                    .attribute("ItemLayer")
                    .filter(|s| !s.is_empty() && *s != "n")
                {
                    if !self.doc.publishing.master_layers.contains_key(id) {
                        let mut definitions = design
                            .children()
                            .filter(|n| tag(*n) == "Layer" && n.attribute("Self") == Some(id));
                        let layer = definitions
                            .next()
                            .ok_or_else(|| format!("Missing retained master layer {id}"))?;
                        if definitions.next().is_some() {
                            return Err("Duplicate retained master layer ID".into());
                        }
                        let xml = &design.document().input_text()[layer.range()];
                        self.budget.charge(xml.len())?;
                        self.doc
                            .publishing
                            .master_layers
                            .insert(id.into(), xml.into());
                    }
                }
                if let Some(uri) = n.attribute("LinkResourceURI") {
                    let fallback = format!("Links/{}", uri.rsplit('/').next().unwrap_or(""));
                    if let Some((name, bytes)) = self
                        .parts
                        .get_key_value(uri)
                        .or_else(|| self.parts.get_key_value(&fallback))
                    {
                        if !self.doc.publishing.master_assets.contains_key(name) {
                            self.budget.charge(bytes.len())?;
                            self.doc
                                .publishing
                                .master_assets
                                .insert(name.clone(), bytes.clone());
                        }
                    } else if let Some(image) = n
                        .ancestors()
                        .find(|n| matches!(tag(*n), "Image" | "PDF" | "EPS"))
                    {
                        if image.descendants().any(|n| tag(n) == "Contents")
                            || embedded.contains(&image.range().start)
                        {
                            continue;
                        }
                        if let Some(bytes) = (self.link)(uri) {
                            if bytes.len() as u64 > MAX_PART {
                                return Err("IDML retained master image exceeds part budget".into());
                            }
                            self.budget
                                .charge(bytes.capacity() + bytes.len().div_ceil(3) * 12 + 128)?;
                            let contents = format!(
                                "<Contents><![CDATA[{}]]></Contents>",
                                base64::engine::general_purpose::STANDARD.encode(&bytes)
                            );
                            let edit = if let Some(properties) = child(image, "Properties") {
                                let range = properties.range();
                                let text = &source[range.clone()];
                                if text.ends_with("/>") {
                                    let name = text[1..]
                                        .split(|c: char| c.is_whitespace() || c == '/' || c == '>')
                                        .next()
                                        .unwrap_or("Properties");
                                    (range.end - 2..range.end, format!(">{contents}</{name}>"))
                                } else {
                                    let at = range.start
                                        + text
                                            .rfind("</")
                                            .ok_or("Invalid IDML master properties")?;
                                    (at..at, contents)
                                }
                            } else {
                                let at = image
                                    .first_child()
                                    .ok_or("IDML master image has no link")?
                                    .range()
                                    .start;
                                (at..at, format!("<Properties>{contents}</Properties>"))
                            };
                            embedded.insert(image.range().start);
                            edits.push(edit);
                            if let Some(state) = n.attribute_node("StoredState") {
                                edits.push((state.range_value(), "Embedded".into()));
                            } else if let Some(uri) = n.attribute_node("LinkResourceURI") {
                                let at = uri.range_qname().start;
                                edits.push((at..at, "StoredState=\"Embedded\" ".into()));
                            }
                        }
                    }
                }
            }
            let growth = edits
                .iter()
                .map(|(range, text)| text.len().saturating_sub(range.len()))
                .sum::<usize>();
            if source.len().saturating_add(growth) as u64 > MAX_PART {
                return Err("IDML retained master XML exceeds part budget".into());
            }
            let capacity = source.capacity();
            source
                .try_reserve_exact(growth)
                .map_err(|_| "IDML retained master XML allocation failed")?;
            self.budget
                .charge(source.capacity().saturating_sub(capacity))?;
            edits.sort_by_key(|(range, _)| range.start);
            for (range, text) in edits.into_iter().rev() {
                source.replace_range(range, &text);
            }
        }
        Ok(())
    }
    fn reserve_layout(&mut self) -> Result<(), String> {
        let runs: HashMap<_, _> = self
            .doc
            .layers
            .iter()
            .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
            .filter_map(|s| {
                if let Geom::Text(run) = &s.geom {
                    Some((s.id, run))
                } else {
                    None
                }
            })
            .collect();
        for (&id, run) in &runs {
            let head = runs
                .get(&run.thread.as_ref().map_or(id, |t| t.story))
                .ok_or("Missing IDML story head")?;
            let mut count = ByteCount::default();
            serde_json::to_writer(
                &mut count,
                &(run, head, usize::MAX, Some(u64::MAX), usize::MAX),
            )
            .map_err(|e| e.to_string())?;
            self.budget.charge(count.0.saturating_mul(2))?;
        }
        let transient = runs.values().map(|run| run_storage(run)).sum::<usize>();
        self.budget.charge(transient.saturating_mul(6))
    }
    fn resources(&mut self, root: Node<'_, '_>) -> Result<(), String> {
        for n in root.descendants().filter(|n| n.is_element()) {
            match tag(n) {
                "DocumentPreference" => {
                    let a = attributes(n);
                    self.doc.publishing.facing_pages = n.attribute("FacingPages") == Some("true");
                    self.doc.publishing.bleed = [
                        number(&a, "DocumentBleedTopOffset", 0.)?,
                        number(&a, "DocumentBleedInsideOrLeftOffset", 0.)?,
                        number(&a, "DocumentBleedBottomOffset", 0.)?,
                        number(&a, "DocumentBleedOutsideOrRightOffset", 0.)?,
                    ];
                }
                "Color" => {
                    let v = numbers(n.attribute("ColorValue").unwrap_or("0 0 0"))?;
                    let c = match (n.attribute("Space"), v.as_slice()) {
                        (Some("CMYK"), [c, m, y, k]) => Rgba::rgb(
                            (255. * (1. - c / 100.) * (1. - k / 100.)) as u8,
                            (255. * (1. - m / 100.) * (1. - k / 100.)) as u8,
                            (255. * (1. - y / 100.) * (1. - k / 100.)) as u8,
                        ),
                        (_, [r, g, b]) => Rgba::rgb(*r as u8, *g as u8, *b as u8),
                        _ => {
                            warn(
                                &mut self.notes,
                                "IDML Lab/spot color is approximated; keep the original for color-managed printing.",
                            );
                            Rgba::BLACK
                        }
                    };
                    self.colors.insert(attr(n, "Self"), c);
                }
                "ParagraphStyle" | "CharacterStyle" => {
                    self.doc.publishing.styles.push(PublishingStyle {
                        id: attr(n, "Self"),
                        name: attr(n, "Name"),
                        kind: tag(n).into(),
                        attributes: attributes(n),
                    });
                }
                "Table"
                | "Footnote"
                | "Endnote"
                | "Note"
                | "Change"
                | "Hyperlink"
                | "TextVariableInstance"
                | "XMLStory" => warn(
                    &mut self.notes,
                    format!(
                        "IDML {} is unsupported and was omitted; retain the source document.",
                        tag(n)
                    ),
                ),
                "Gradient" => warn(
                    &mut self.notes,
                    "IDML gradient paint is unsupported and is approximated by black.",
                ),
                _ => {}
            }
        }
        Ok(())
    }
    fn effective(&self, id: &str, seen: &mut HashSet<String>) -> Result<Attributes, String> {
        if id.is_empty() || id == "n" {
            return Ok(Attributes::new());
        }
        if !seen.insert(id.into()) || seen.len() > 64 {
            return Err("Cyclic or deep IDML style inheritance".into());
        }
        let Some(s) = self.doc.publishing.styles.iter().find(|s| s.id == id) else {
            return Ok(Attributes::new());
        };
        let mut a = if let Some(base) = s.attributes.get("BasedOn") {
            self.effective(base, seen)?
        } else {
            Attributes::new()
        };
        a.extend(s.attributes.clone());
        Ok(a)
    }
    fn resolve(&self, n: Node<'_, '_>, key: &str, base: &Attributes) -> Result<Attributes, String> {
        let mut a = base.clone();
        a.extend(self.effective(n.attribute(key).unwrap_or(""), &mut HashSet::new())?);
        a.extend(attributes(n));
        Ok(a)
    }
    fn font(&mut self, a: &Attributes) -> String {
        let name = a.get("AppliedFont").map(String::as_str).unwrap_or("");
        if name.is_empty() {
            return String::new();
        }
        let style = a.get("FontStyle").map(String::as_str).unwrap_or("");
        let faces = crate::text::fonts();
        if let Some(f) = faces
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(&format!("{name} {style}")))
            .or_else(|| faces.iter().find(|f| f.name.eq_ignore_ascii_case(name)))
        {
            return f.path.to_string_lossy().into_owned();
        }
        warn(
            &mut self.notes,
            format!(
                "IDML font {name} {style} is not installed; system fallback is displayed. The named style is retained."
            ),
        );
        String::new()
    }
    fn story(&mut self, n: Node<'_, '_>) -> Result<(), String> {
        let mut run = TypeRun {
            px: 12.,
            ..Default::default()
        };
        let mut first = true;
        let mut story_color = None;
        for p in n.children().filter(|n| tag(*n) == "ParagraphStyleRange") {
            let a = self.resolve(p, "AppliedParagraphStyle", &Attributes::new())?;
            if a.keys().any(|k| {
                !matches!(
                    k.as_str(),
                    "Self"
                        | "Name"
                        | "BasedOn"
                        | "AppliedParagraphStyle"
                        | "AppliedCharacterStyle"
                        | "AppliedFont"
                        | "FontStyle"
                        | "PointSize"
                        | "Tracking"
                        | "Leading"
                        | "HorizontalScale"
                        | "VerticalScale"
                        | "BaselineShift"
                        | "NoBreak"
                        | "Justification"
                        | "Hyphenation"
                        | "FillColor"
                )
            }) {
                warn(
                    &mut self.notes,
                    "Unsupported IDML paragraph formatting remains in named style definitions but is not rendered.",
                );
            }
            let start = run.content.chars().count();
            let align = match a.get("Justification").map(String::as_str) {
                Some("CenterAlign") => TextAlign::Center,
                Some("RightAlign") => TextAlign::End,
                Some("FullyJustified") => TextAlign::Justify {
                    last: crate::geom::LastLine::Justify,
                },
                Some("LeftJustified") => TextAlign::Justify {
                    last: crate::geom::LastLine::Start,
                },
                Some("CenterJustified") => TextAlign::Justify {
                    last: crate::geom::LastLine::Center,
                },
                Some("RightJustified") => TextAlign::Justify {
                    last: crate::geom::LastLine::End,
                },
                _ => TextAlign::Start,
            };
            run.paragraphs.push(ParagraphStyle {
                start,
                align,
                publishing_style: p.attribute("AppliedParagraphStyle").map(str::to_owned),
                hyphenate: a.get("Hyphenation").is_some_and(|s| s == "true"),
                ..Default::default()
            });
            for c in p.children().filter(|n| tag(*n) == "CharacterStyleRange") {
                let a = self.resolve(c, "AppliedCharacterStyle", &a)?;
                if a.keys().any(|k| {
                    !matches!(
                        k.as_str(),
                        "Self"
                            | "Name"
                            | "BasedOn"
                            | "AppliedParagraphStyle"
                            | "AppliedCharacterStyle"
                            | "AppliedFont"
                            | "FontStyle"
                            | "PointSize"
                            | "Tracking"
                            | "Leading"
                            | "HorizontalScale"
                            | "VerticalScale"
                            | "BaselineShift"
                            | "NoBreak"
                            | "Justification"
                            | "Hyphenation"
                            | "FillColor"
                    )
                }) {
                    warn(
                        &mut self.notes,
                        "Unsupported IDML character formatting remains in named style definitions but is not rendered.",
                    );
                }
                let color = a
                    .get("FillColor")
                    .and_then(|id| self.colors.get(id))
                    .copied()
                    .unwrap_or(Rgba::BLACK);
                if story_color.is_none() {
                    story_color = Some(color);
                }
                let font = self.font(&a);
                let size = number(&a, "PointSize", 12.)?.clamp(0.1, 10000.);
                if first {
                    run.px = size;
                    run.font = font.clone();
                    first = false;
                }
                let start = run.content.chars().count();
                content(c, &mut run.content);
                let leading = a
                    .get("Leading")
                    .filter(|v| v.as_str() != "Auto")
                    .map(|_| number(&a, "Leading", size * 1.2).map(Leading::Fixed))
                    .transpose()?
                    .unwrap_or(Leading::Fixed(size * 1.2));
                run.spans.push(CharSpan {
                    start,
                    end: run.content.chars().count(),
                    publishing_style: c.attribute("AppliedCharacterStyle").map(str::to_owned),
                    font: (font != run.font).then_some(font),
                    px: Some(size),
                    fill: Some(Fill::Solid(color)),
                    tracking: Some(number(&a, "Tracking", 0.)?),
                    leading: Some(leading),
                    hscale: Some(number(&a, "HorizontalScale", 100.)?),
                    vscale: Some(number(&a, "VerticalScale", 100.)?),
                    baseline_shift: Some(number(&a, "BaselineShift", 0.)?),
                    no_break: a.get("NoBreak").is_some_and(|v| v == "true"),
                    ..Default::default()
                });
            }
        }
        self.budget.charge(run_storage(&run))?;
        self.story_colors
            .insert(attr(n, "Self"), story_color.unwrap_or(Rgba::BLACK));
        if self.stories.insert(attr(n, "Self"), run).is_some() {
            return Err("Duplicate IDML story ID".into());
        }
        Ok(())
    }
    fn spread(&mut self, n: Node<'_, '_>, x: f32) -> Result<f32, String> {
        let transform = Matrix::read(n)?;
        let pages = n
            .children()
            .filter(|n| tag(*n) == "Page")
            .collect::<Vec<_>>();
        if pages.len() > 1024 || self.doc.artboards.len() + pages.len() > 1024 {
            return Err("IDML exceeds 1024 pages".into());
        }
        let boxes = pages
            .iter()
            .map(|p| bounds(*p, Matrix::read(*p)?.then(transform)))
            .collect::<Result<Vec<_>, String>>()?;
        let min = Pt::new(
            boxes.iter().map(|b| b.min.x).fold(f32::INFINITY, f32::min),
            boxes.iter().map(|b| b.min.y).fold(f32::INFINITY, f32::min),
        );
        if !min.x.is_finite() {
            return Err("IDML spread has no page geometry".into());
        }
        let offset = Pt::new(x, 0.) - min;
        for (page, b) in pages.iter().zip(&boxes) {
            if b.width() <= 0. || b.height() <= 0. {
                return Err("IDML page has empty geometry".into());
            }
            let mut a = Artboard::new(self.doc.artboards.len(), b.min + offset, b.size());
            a.name = attr(*page, "Name");
            if page
                .attribute("OverrideList")
                .is_some_and(|v| !v.is_empty())
            {
                warn(
                    &mut self.notes,
                    "IDML master page-item overrides are unsupported; inherited previews can differ from the source.",
                );
            }
            if let Some(side) = page.attribute("PageSide") {
                self.doc.publishing.page_sides.insert(a.id, side.into());
            }
            if let Some(master) = page
                .attribute("AppliedMaster")
                .filter(|s| *s != "n" && !s.is_empty())
            {
                self.doc.publishing.page_masters.insert(a.id, master.into());
                if let Some(source) = self
                    .doc
                    .publishing
                    .masters
                    .iter()
                    .find(|m| m.id == master)
                    .cloned()
                {
                    let master = parse(source.xml.as_bytes())?;
                    let root = master.root_element();
                    let master_pages = root
                        .children()
                        .filter(|n| tag(*n) == "Page")
                        .collect::<Vec<_>>();
                    let side = page.attribute("PageSide");
                    let selected = master_pages
                        .iter()
                        .copied()
                        .find(|p| side.is_some() && p.attribute("PageSide") == side)
                        .or_else(|| master_pages.first().copied());
                    if let Some(mp) = selected {
                        let mb = bounds(mp, Matrix::read(mp)?)?;
                        self.items(
                            root,
                            Matrix::translated(a.origin - mb.min),
                            Some(a.id),
                            Some(mb),
                        )?;
                    }
                } else {
                    warn(
                        &mut self.notes,
                        format!("IDML page references missing master {master}."),
                    );
                }
            }
            self.doc.artboards.push(a);
        }
        self.items(n, transform.then(Matrix::translated(offset)), None, None)?;
        Ok(boxes.iter().map(|b| b.max.x + offset.x).fold(x, f32::max) + 48.)
    }
    fn items(
        &mut self,
        root: Node<'_, '_>,
        parent: Matrix,
        inherited_page: Option<u64>,
        clip: Option<Bounds>,
    ) -> Result<(), String> {
        let inherited = inherited_page.is_some();
        for n in root.children().filter(|n| n.is_element()) {
            if self.doc.layers.len() > 16384 {
                return Err("IDML exceeds 16384 items".into());
            }
            if n.ancestors().count() > 64 {
                return Err("IDML group depth exceeds 64".into());
            }
            let kind = tag(n);
            if kind == "Group" {
                self.items(n, Matrix::read(n)?.then(parent), inherited_page, clip)?;
                continue;
            }
            if !matches!(
                kind,
                "TextFrame" | "Rectangle" | "Oval" | "Polygon" | "GraphicLine"
            ) {
                continue;
            }
            let m = Matrix::read(n)?.then(parent);
            let b = bounds(n, m)?;
            if let Some(clip) = clip {
                let source = bounds(n, Matrix::read(n)?)?;
                if !clip.contains(source.center()) {
                    continue;
                }
            }
            let a = attributes(n);
            let mut name = attr(n, "Name");
            if name.is_empty() {
                name = attr(n, "Self");
            }
            let mut layer = Layer::vector(&name);
            layer.visible = n.attribute("Visible") != Some("false");
            layer.locked = inherited || n.attribute("Locked") == Some("true");
            let mut opacity = 1.;
            if let Some(o) = n.descendants().find(|n| tag(*n) == "BlendingSetting") {
                opacity = number(&attributes(o), "Opacity", 100.)? / 100.;
                if o.attribute("BlendMode").is_some_and(|v| v != "Normal") {
                    warn(
                        &mut self.notes,
                        "IDML blend modes other than Normal are not retained.",
                    );
                }
            }
            layer.opacity = opacity;
            if let Some(image) = n
                .children()
                .find(|n| matches!(tag(*n), "Image" | "PDF" | "EPS"))
            {
                let uri = image
                    .descendants()
                    .find(|n| tag(*n) == "Link")
                    .map(|n| attr(n, "LinkResourceURI"))
                    .unwrap_or_default();
                let contents = image
                    .descendants()
                    .find(|n| tag(*n) == "Contents")
                    .and_then(|n| n.text());
                let bytes = if let Some(text) = contents {
                    let clean = text.split_whitespace().collect::<String>();
                    Some(
                        base64::engine::general_purpose::STANDARD
                            .decode(clean)
                            .map_err(|_| "Invalid IDML image base64")?,
                    )
                } else {
                    self.parts
                        .get(&uri)
                        .cloned()
                        .or_else(|| {
                            let file = uri.rsplit('/').next().unwrap_or("");
                            self.parts.get(&format!("Links/{file}")).cloned()
                        })
                        .or_else(|| (self.link)(&uri))
                };
                if let Some(bytes) = bytes {
                    match crate::formats::decode_image(&bytes) {
                        Ok(px) => {
                            self.budget.charge_pixels(&px)?;
                            layer.kind = Layer::placed_raster(&name, px, b.min, b.size()).kind;
                        }
                        Err(e) => {
                            warn(
                                &mut self.notes,
                                format!("IDML placed graphic {uri} could not be decoded: {e}"),
                            );
                        }
                    }
                } else {
                    warn(
                        &mut self.notes,
                        format!(
                            "IDML linked graphic is missing: {uri}. Its link and frame are retained."
                        ),
                    );
                }
                if layer.kind.pixels().is_none() {
                    layer.kind.shapes_mut().unwrap().push(Shape::new(
                        Geom::Rect {
                            origin: b.min,
                            size: b.size(),
                            radius: 0.,
                        },
                        Style {
                            fill: Fill::None,
                            stroke: Some(crate::document::Stroke::default()),
                        },
                    ));
                }
                if !uri.is_empty() {
                    self.doc.publishing.links.insert(layer.id, uri);
                }
                if Matrix::read(image)?.0 != Matrix::ID.0 {
                    warn(
                        &mut self.notes,
                        "IDML placed-image cropping/transforms are fitted to the frame. The link is retained.",
                    );
                }
            } else if kind == "TextFrame" {
                let story = attr(n, "ParentStory");
                let source = self
                    .stories
                    .get(&story)
                    .ok_or_else(|| format!("Missing IDML story {story}"))?;
                let mut run = if inherited {
                    self.budget.charge(run_storage(source))?;
                    source.clone()
                } else {
                    self.budget.charge(source.font.len())?;
                    TypeRun {
                        px: source.px,
                        tracking: source.tracking,
                        leading: source.leading,
                        align: source.align,
                        font: source.font.clone(),
                        kern: source.kern,
                        liga: source.liga,
                        tnum: source.tnum,
                        smcp: source.smcp,
                        ..Default::default()
                    }
                };
                let affine = m.0[..4] != Matrix::ID.0[..4];
                let local = if affine {
                    if (m.0[0] * m.0[3] - m.0[1] * m.0[2]).abs() < 1e-10 {
                        return Err("IDML text frame has a singular transform".into());
                    }
                    run.transform = Some(m.0);
                    bounds(n, Matrix::ID)?
                } else {
                    b
                };
                run.origin = Pt::new(local.min.x, local.min.y + run.px * 0.85);
                let mut frame = TextFrame {
                    size: local.size(),
                    ..Default::default()
                };
                if let Some(options) = child(n, "TextFramePreference") {
                    let mut values = attributes(options);
                    if let Some(inset) = options.descendants().find(|n| tag(*n) == "InsetSpacing") {
                        let list = inset
                            .children()
                            .filter(|n| tag(*n) == "ListItem")
                            .filter_map(|n| n.text())
                            .collect::<Vec<_>>();
                        if !list.is_empty() {
                            values.insert("InsetSpacing".into(), list.join(" "));
                        }
                    }
                    let options = values;
                    frame.valign = match options.get("VerticalJustification").map(String::as_str) {
                        Some("CenterAlign") => VAlign::Center,
                        Some("BottomAlign") => VAlign::Bottom,
                        _ => VAlign::Top,
                    };
                    if let Some(inset) = options.get("InsetSpacing") {
                        let v = numbers(inset)?;
                        frame.inset = match v.as_slice() {
                            [v] => [*v; 4],
                            [a, b, c, d] => [*a, *b, *c, *d],
                            _ => return Err("Invalid IDML text inset".into()),
                        };
                    }
                    if number(&options, "TextColumnCount", 1.)? > 1. {
                        warn(
                            &mut self.notes,
                            "IDML multicolumn frames import as one editable column.",
                        );
                    }
                }
                run.frame = Some(frame);
                let mut shape = Shape::new(
                    Geom::Text(run),
                    Style {
                        fill: Fill::Solid(
                            self.story_colors
                                .get(&story)
                                .copied()
                                .unwrap_or(Rgba::BLACK),
                        ),
                        stroke: None,
                    },
                );
                shape.name = name;
                if !inherited {
                    self.frames.push(Frame {
                        xml: attr(n, "Self"),
                        id: shape.id,
                        story,
                        prev: attr(n, "PreviousTextFrame"),
                        next: attr(n, "NextTextFrame"),
                    });
                }
                layer.kind.shapes_mut().unwrap().push(shape);
            } else {
                let paths = n
                    .descendants()
                    .filter(|n| tag(*n) == "GeometryPathType")
                    .map(|path| {
                        let anchors = path
                            .descendants()
                            .filter(|n| tag(*n) == "PathPointType")
                            .map(|p| {
                                let point = |key: &str| -> Result<Pt, String> {
                                    let v = numbers(
                                        p.attribute(key)
                                            .or_else(|| p.attribute("Anchor"))
                                            .unwrap_or(""),
                                    )?;
                                    if v.len() != 2 {
                                        return Err("Invalid IDML path point".into());
                                    }
                                    Ok(m.point(Pt::new(v[0], v[1])))
                                };
                                let pt = point("Anchor")?;
                                Ok(Anchor {
                                    pt,
                                    h_in: point("LeftDirection")? - pt,
                                    h_out: point("RightDirection")? - pt,
                                    radius: 0.,
                                })
                            })
                            .collect::<Result<Vec<_>, String>>()?;
                        Ok(crate::geom::PathContour {
                            anchors,
                            closed: path.attribute("PathOpen") != Some("true"),
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let geom = if !paths.is_empty() {
                    if paths.len() == 1 {
                        let p = paths.into_iter().next().unwrap();
                        Geom::Path {
                            anchors: p.anchors,
                            closed: p.closed,
                        }
                    } else {
                        Geom::Paths {
                            paths,
                            winding: true,
                        }
                    }
                } else if kind == "Oval" {
                    Geom::Ellipse {
                        center: b.center(),
                        radii: b.size() * 0.5,
                    }
                } else {
                    Geom::Rect {
                        origin: b.min,
                        size: b.size(),
                        radius: 0.,
                    }
                };
                layer
                    .kind
                    .shapes_mut()
                    .unwrap()
                    .push(Shape::new(geom, style(&a, &self.colors)?));
            }
            if inherited {
                self.doc.publishing.inherited_layers.push(layer.id);
                self.doc
                    .publishing
                    .inherited_pages
                    .insert(layer.id, inherited_page.unwrap());
            }
            self.doc.layers.push(layer);
        }
        Ok(())
    }
    fn threads(&mut self) -> Result<(), String> {
        let lookup: HashMap<_, _> = self
            .frames
            .iter()
            .enumerate()
            .map(|(i, f)| (f.xml.as_str(), i))
            .collect();
        if lookup.len() != self.frames.len() {
            return Err("Duplicate IDML text frame ID".into());
        }
        let mut visited = HashSet::new();
        let mut story_heads = HashSet::new();
        let mut validated = Vec::new();
        for (head, f) in self
            .frames
            .iter()
            .enumerate()
            .filter(|(_, f)| f.prev.is_empty() || f.prev == "n")
        {
            if !story_heads.insert(&f.story) {
                return Err("IDML story has disconnected text frames".into());
            }
            let mut at = Some(head);
            while let Some(i) = at {
                if !visited.insert(i) {
                    return Err("Cyclic IDML text thread".into());
                }
                let f = &self.frames[i];
                let resolve = |id: &str| -> Result<Option<usize>, String> {
                    if id.is_empty() || id == "n" {
                        Ok(None)
                    } else {
                        lookup
                            .get(id)
                            .copied()
                            .map(Some)
                            .ok_or_else(|| format!("Missing IDML threaded frame {id}"))
                    }
                };
                let prev = resolve(&f.prev)?;
                let next = resolve(&f.next)?;
                if f.story != self.frames[head].story
                    || prev.is_some_and(|p| self.frames[p].next != f.xml)
                    || next.is_some_and(|p| self.frames[p].prev != f.xml)
                {
                    return Err("Inconsistent IDML text thread".into());
                }
                validated.push((i, head, prev, next));
                at = next;
            }
        }
        if visited.len() != self.frames.len() {
            return Err("IDML thread has a cycle or no first frame".into());
        }
        let mut runs: HashMap<_, _> = self
            .doc
            .layers
            .iter_mut()
            .filter_map(|l| l.kind.shapes_mut())
            .flatten()
            .filter_map(|s| {
                if let Geom::Text(run) = &mut s.geom {
                    Some((s.id, run))
                } else {
                    None
                }
            })
            .collect();
        for (i, head, prev, next) in validated {
            let f = &self.frames[i];
            let run = runs.get_mut(&f.id).ok_or("IDML text frame disappeared")?;
            if i == head {
                let mut source = self
                    .stories
                    .remove(&f.story)
                    .ok_or("Missing IDML story head")?;
                source.origin = run.origin;
                source.transform = run.transform;
                source.frame = run.frame.take();
                **run = source;
            }
            run.thread = Some(TextThread {
                story: self.frames[head].id,
                prev: prev.map(|p| self.frames[p].id),
                next: next.map(|p| self.frames[p].id),
            });
        }
        Ok(())
    }
}
fn content(n: Node<'_, '_>, text: &mut String) {
    for c in n.children().filter(|n| n.is_element()) {
        match tag(c) {
            "Content" => {
                for t in c.descendants().filter(|n| n.is_text()) {
                    text.push_str(t.text().unwrap_or(""));
                }
            }
            "Br" => text.push('\n'),
            "Tab" => text.push('\t'),
            "Table" | "Note" | "Footnote" | "Endnote" | "TextFrame" => {}
            _ => content(c, text),
        }
    }
}
