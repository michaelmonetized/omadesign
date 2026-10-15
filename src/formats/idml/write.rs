use super::*;
use crate::{
    document::{Artboard, Document, Layer, LayerKind, Shape},
    geom::{Geom, TextAlign, TypeRun},
};
use base64::Engine as _;
use std::{
    collections::{HashMap, HashSet},
    io::{Cursor, Write},
};

/// Write standard IDML parts. Args: editable document. Returns ZIP bytes and explicit conversion notes.
pub fn write(doc: &Document) -> Result<(Vec<u8>, Vec<String>), String> {
    doc.validate_hierarchy()?;
    if doc.artboards.len() > 1024 || doc.layers.len() > 16384 {
        return Err("IDML export exceeds page or item budget".into());
    }
    let mut ex = Exporter {
        doc,
        notes: vec![],
        colors: doc.publishing.swatches.clone(),
        stories: BTreeMap::new(),
        frames: HashMap::new(),
    };
    let boards = if doc.artboards.is_empty() {
        vec![Artboard::new(0, Pt::ZERO, Pt::new(doc.width, doc.height))]
    } else {
        doc.artboards.clone()
    };
    for a in &boards {
        if !a.size.x.is_finite() || !a.size.y.is_finite() || a.size.x <= 0. || a.size.y <= 0. {
            return Err("IDML requires finite positive pages".into());
        }
        if a.rotation != 0. {
            warn(
                &mut ex.notes,
                "IDML artboard rotation is exported as an unrotated page.",
            );
        }
    }
    for l in &doc.layers {
        if doc.publishing.inherited_layers.contains(&l.id)
            && doc.publishing.inherited_fingerprints.get(&l.id) != Some(&fingerprint(l)?)
        {
            return Err("A master preview was edited. Save .oma to retain that edit; IDML master overrides are not supported.".into());
        }
    }
    for id in &doc.publishing.inherited_layers {
        let assigned = |page: &Artboard| {
            doc.publishing
                .page_masters
                .get(&page.id)
                .is_some_and(|master| !master.is_empty() && master != "n")
        };
        let active = if let Some(page) = doc.publishing.inherited_pages.get(id) {
            boards.iter().any(|a| a.id == *page && assigned(a))
        } else {
            boards.iter().any(assigned)
        };
        if active && !doc.layers.iter().any(|l| l.id == *id) {
            return Err("A master preview was deleted from an assigned page. Save .oma to retain that edit; IDML master overrides are not supported.".into());
        }
    }
    let inherited: HashSet<_> = doc.publishing.inherited_layers.iter().copied().collect();
    for l in &doc.layers {
        if inherited.contains(&l.id) {
            continue;
        }
        for s in l.kind.shapes().unwrap_or(&[]) {
            if let Geom::Text(t) = &s.geom {
                ex.frames.insert(s.id, t);
            }
        }
    }
    ex.validate_threads()?;
    let mut map = String::new();
    let mut parts = Vec::new();
    for source in doc
        .publishing
        .masters
        .iter()
        .map(|m| &m.xml)
        .chain(doc.publishing.master_stories.values())
    {
        let tree = roxmltree::Document::parse(source)
            .map_err(|e| format!("Invalid retained IDML master: {e}"))?;
        for id in tree
            .descendants()
            .filter_map(|n| n.attribute("ItemLayer"))
            .filter(|id| !id.is_empty() && *id != "n")
        {
            if !doc.publishing.master_layers.contains_key(id) {
                return Err(format!(
                    "Missing retained master layer {id}; reopen the original IDML source."
                ));
            }
        }
    }
    for (i, story) in doc.publishing.master_stories.values().enumerate() {
        let name = format!("Stories/MasterStory_{i}.xml");
        parts.push((name.clone(), package("Story", story).into_bytes()));
        map.push_str(&format!("<idPkg:Story src=\"{name}\"/>"));
    }
    for master in &doc.publishing.masters {
        roxmltree::Document::parse(&master.xml)
            .map_err(|e| format!("Invalid retained IDML master: {e}"))?;
        let name = format!("MasterSpreads/MasterSpread_{}.xml", parts.len());
        parts.push((
            name.clone(),
            package("MasterSpread", &master.xml).into_bytes(),
        ));
        map.push_str(&format!("<idPkg:MasterSpread src=\"{name}\"/>"));
    }
    for (i, a) in boards.iter().enumerate() {
        let offset = Pt::new(-a.size.x * 0.5, -a.size.y * 0.5) - a.origin;
        let mut body = format!(
            "<Spread Self=\"spread{i}\" ItemTransform=\"1 0 0 1 0 0\" PageCount=\"1\" BindingLocation=\"0\" AllowPageShuffle=\"false\"><Page Self=\"page{}\" Name=\"{}\" GeometricBounds=\"0 0 {} {}\" ItemTransform=\"1 0 0 1 {} {}\" AppliedMaster=\"{}\" MasterPageTransform=\"1 0 0 1 0 0\" PageSide=\"{}\" OverrideList=\"\"/>  ",
            a.id,
            xml(&a.name),
            a.size.y,
            a.size.x,
            -a.size.x * 0.5,
            -a.size.y * 0.5,
            xml(doc
                .publishing
                .page_masters
                .get(&a.id)
                .map(String::as_str)
                .unwrap_or("n")),
            xml(doc
                .publishing
                .page_sides
                .get(&a.id)
                .map(String::as_str)
                .unwrap_or("RightHand"))
        );
        for l in &doc.layers {
            if inherited.contains(&l.id) {
                continue;
            }
            if l.parent.is_some() || l.is_group {
                warn(
                    &mut ex.notes,
                    "IDML layer groups are exported as separate layers; group effects are not retained.",
                );
            }
            if !l.filters.items.is_empty() || l.mask.is_some() || !l.artwork_masks.is_empty() {
                warn(
                    &mut ex.notes,
                    "IDML live layer effects and masks are unsupported; save .oma to retain their appearance.",
                );
            }
            let mut items = String::new();
            match &l.kind {
                LayerKind::Vector { shapes } => {
                    for s in shapes {
                        if boards
                            .iter()
                            .position(|a| a.local_bounds().contains(shape_bounds(s).center()))
                            .unwrap_or(0)
                            == i
                        {
                            if doc.publishing.links.contains_key(&l.id) {
                                items.push_str(&ex.image(l, shape_bounds(s), offset)?);
                            } else {
                                items.push_str(&ex.shape(s, l, offset)?);
                            }
                        }
                    }
                }
                LayerKind::Raster {
                    origin,
                    size,
                    pixels,
                    ..
                } => {
                    let size = if size.x > 0. && size.y > 0. {
                        *size
                    } else {
                        Pt::new(pixels.w as f32, pixels.h as f32)
                    };
                    let b = Bounds::from_min_size(*origin, size);
                    if boards
                        .iter()
                        .position(|a| a.local_bounds().contains(b.center()))
                        .unwrap_or(0)
                        == i
                    {
                        items.push_str(&ex.image(l, b, offset)?);
                    }
                }
            }
            body.push_str(&items);
        }
        body.push_str("</Spread>");
        let name = format!("Spreads/Spread_{i}.xml");
        parts.push((name.clone(), package("Spread", &body).into_bytes()));
        map.push_str(&format!("<idPkg:Spread src=\"{name}\"/>"));
    }
    for (id, story) in &ex.stories {
        let name = format!("Stories/Story_{id}.xml");
        parts.push((name.clone(), package("Story", story).into_bytes()));
        map.push_str(&format!("<idPkg:Story src=\"{name}\"/>"));
    }
    let bleed = if doc.publishing.is_empty() {
        [doc.bleed; 4]
    } else {
        doc.publishing.bleed
    };
    let pref = format!(
        "<DocumentPreference PageWidth=\"{}\" PageHeight=\"{}\" FacingPages=\"{}\" DocumentBleedTopOffset=\"{}\" DocumentBleedInsideOrLeftOffset=\"{}\" DocumentBleedBottomOffset=\"{}\" DocumentBleedOutsideOrRightOffset=\"{}\" DocumentBleedUniformSize=\"{}\"/>",
        boards[0].size.x,
        boards[0].size.y,
        doc.publishing.facing_pages,
        bleed[0],
        bleed[1],
        bleed[2],
        bleed[3],
        bleed.iter().all(|v| *v == bleed[0])
    );
    parts.push((
        "Resources/Preferences.xml".into(),
        package("Preferences", &pref).into_bytes(),
    ));
    let mut styles="<RootParagraphStyleGroup Self=\"pg\"><ParagraphStyle Self=\"ParagraphStyle/$ID/[No paragraph style]\" Name=\"$ID/[No paragraph style]\"/>".to_string();
    for s in
        doc.publishing.styles.iter().filter(|s| {
            s.kind == "ParagraphStyle" && s.id != "ParagraphStyle/$ID/[No paragraph style]"
        })
    {
        styles.push_str(&style_xml(s));
    }
    styles.push_str("</RootParagraphStyleGroup><RootCharacterStyleGroup Self=\"cg\"><CharacterStyle Self=\"CharacterStyle/$ID/[No character style]\" Name=\"$ID/[No character style]\"/>");
    for s in
        doc.publishing.styles.iter().filter(|s| {
            s.kind == "CharacterStyle" && s.id != "CharacterStyle/$ID/[No character style]"
        })
    {
        styles.push_str(&style_xml(s));
    }
    styles.push_str("</RootCharacterStyleGroup>");
    parts.push((
        "Resources/Styles.xml".into(),
        package("Styles", &styles).into_bytes(),
    ));
    let mut colors: String = "<Swatch Self=\"Swatch/None\" Name=\"None\"/>".into();
    for (id, c) in &ex.colors {
        colors += &format!(
            "<Color Self=\"{}\" Name=\"{}\" Model=\"Process\" Space=\"RGB\" ColorValue=\"{} {} {}\"/>",
            xml(id),
            xml(id),
            c.r,
            c.g,
            c.b
        );
    }
    parts.push((
        "Resources/Graphic.xml".into(),
        package("Graphic", &colors).into_bytes(),
    ));
    parts.push((
        "Resources/Fonts.xml".into(),
        package("Fonts", "").into_bytes(),
    ));
    let mut layers = String::new();
    for (id, source) in &doc.publishing.master_layers {
        let tree = roxmltree::Document::parse(source)
            .map_err(|e| format!("Invalid retained IDML layer: {e}"))?;
        if tag(tree.root_element()) != "Layer"
            || tree.root_element().attribute("Self") != Some(id.as_str())
        {
            return Err("Retained IDML layer has an invalid ID".into());
        }
        layers.push_str(source);
    }
    for l in &doc.layers {
        if !inherited.contains(&l.id) {
            layers += &format!(
                "<Layer Self=\"{}\" Name=\"{}\" Visible=\"{}\" Locked=\"{}\" IgnoreWrap=\"false\" ShowGuides=\"true\" LockGuides=\"false\" UI=\"true\"/>",
                ex.layer_id(l.id),
                xml(&l.name),
                l.visible,
                l.locked
            );
        }
    }
    let design = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><?aid style=\"50\" type=\"document\" readerVersion=\"6.0\" featureSet=\"257\" product=\"16.0(1)\"?><Document xmlns:idPkg=\"{NS}\" DOMVersion=\"16.0\" Self=\"d\" Name=\"{}\"><idPkg:Preferences src=\"Resources/Preferences.xml\"/><idPkg:Graphic src=\"Resources/Graphic.xml\"/><idPkg:Styles src=\"Resources/Styles.xml\"/><idPkg:Fonts src=\"Resources/Fonts.xml\"/>{layers}{map}</Document>",
        xml(&doc.name)
    );
    parts.insert(0, ("designmap.xml".into(), design.into_bytes()));
    parts.push(("META-INF/container.xml".into(), b"<?xml version=\"1.0\"?><container xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\" version=\"1.0\"><rootfiles><rootfile full-path=\"designmap.xml\" media-type=\"text/xml\"/></rootfiles></container>".to_vec()));
    let mut retained_total = parts.iter().map(|(_, bytes)| bytes.len()).sum::<usize>();
    for (name, bytes) in &doc.publishing.master_assets {
        retained_total = retained_total
            .checked_add(bytes.len())
            .ok_or("IDML export size overflow")?;
        if bytes.len() as u64 > MAX_PART || retained_total > MAX_TOTAL {
            return Err("IDML export exceeds expanded package budget".into());
        }
        if name == "mimetype"
            || name.starts_with('/')
            || name.contains('\\')
            || name
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == "..")
            || parts.iter().any(|(part, _)| part == name)
        {
            return Err("Retained IDML asset has an unsafe or conflicting package path".into());
        }
        parts.push((name.clone(), bytes.clone()));
    }
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    w.start_file(
        "mimetype",
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )
    .map_err(|e| e.to_string())?;
    w.write_all(MIMETYPE.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut total = 0;
    for (name, bytes) in parts {
        if !doc.publishing.master_assets.contains_key(&name) {
            roxmltree::Document::parse(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?)
                .map_err(|e| format!("Invalid generated IDML {name}: {e}"))?;
        }
        total += bytes.len();
        if bytes.len() as u64 > MAX_PART || total > MAX_TOTAL {
            return Err("IDML export exceeds expanded package budget".into());
        }
        w.start_file(
            name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .map_err(|e| e.to_string())?;
        w.write_all(&bytes).map_err(|e| e.to_string())?;
    }
    Ok((
        w.finish().map_err(|e| e.to_string())?.into_inner(),
        ex.notes,
    ))
}
struct Exporter<'a> {
    doc: &'a Document,
    notes: Vec<String>,
    colors: BTreeMap<String, Rgba>,
    stories: BTreeMap<u64, String>,
    frames: HashMap<u64, &'a TypeRun>,
}
impl Exporter<'_> {
    fn layer_id(&self, id: u64) -> String {
        let mut name = format!("layer{id}");
        while self.doc.publishing.master_layers.contains_key(&name) {
            name.push('_');
        }
        name
    }
    fn validate_threads(&self) -> Result<(), String> {
        for (&id, run) in &self.frames {
            if let Some(t) = &run.thread {
                for (other, back) in [(t.prev, true), (t.next, false)] {
                    if let Some(other) = other {
                        let other = self
                            .frames
                            .get(&other)
                            .ok_or("IDML export thread points to a missing frame")?;
                        let thread = other
                            .thread
                            .as_ref()
                            .ok_or("IDML export thread is incomplete")?;
                        if thread.story != t.story
                            || if back {
                                thread.next != Some(id)
                            } else {
                                thread.prev != Some(id)
                            }
                        {
                            return Err("IDML export thread links are inconsistent".into());
                        }
                    }
                }
                let mut seen = HashSet::new();
                let mut at = Some(id);
                while let Some(id) = at {
                    if !seen.insert(id) {
                        return Err("IDML export thread is cyclic".into());
                    }
                    at = self
                        .frames
                        .get(&id)
                        .and_then(|t| t.thread.as_ref())
                        .and_then(|t| t.next);
                }
            }
        }
        Ok(())
    }
    fn color(&mut self, fill: &Fill) -> String {
        match fill {
            Fill::None => "Swatch/None".into(),
            _ => {
                if !matches!(fill, Fill::Solid(_)) {
                    warn(
                        &mut self.notes,
                        "IDML gradient fills are exported as their first color.",
                    );
                }
                let c = fill.clone().solid_or(Rgba::BLACK);
                if c.a != 255 {
                    warn(
                        &mut self.notes,
                        "IDML per-paint alpha is not retained; object opacity is retained.",
                    );
                }
                let id = format!("Color/oma{:02x}{:02x}{:02x}", c.r, c.g, c.b);
                self.colors.insert(id.clone(), c);
                id
            }
        }
    }
    fn shape(&mut self, s: &Shape, l: &Layer, offset: Pt) -> Result<String, String> {
        let mut common = format!(
            "Self=\"item{}\" Name=\"{}\" ItemLayer=\"{}\" Visible=\"{}\" Locked=\"{}\" ItemTransform=\"1 0 0 1 {} {}\"",
            s.id,
            xml(&s.name),
            self.layer_id(l.id),
            s.visible,
            s.locked,
            offset.x,
            offset.y
        );
        let fill = self.color(&s.style.fill);
        let stroke = s
            .style
            .stroke
            .as_ref()
            .map(|s| (self.color(&Fill::Solid(s.color)), s.width))
            .unwrap_or(("Swatch/None".into(), 0.));
        common += &format!(
            " FillColor=\"{fill}\" StrokeColor=\"{}\" StrokeWeight=\"{}\"",
            stroke.0, stroke.1
        );
        let opacity = format!(
            "<TransparencySetting><BlendingSetting Opacity=\"{}\" BlendMode=\"Normal\"/></TransparencySetting>",
            (s.opacity * l.opacity * 100.).clamp(0., 100.)
        );
        if !s.filters.items.is_empty() || s.mask.is_some() || !s.artwork_masks.is_empty() {
            warn(
                &mut self.notes,
                "IDML live object effects and masks are unsupported; save .oma to retain their appearance.",
            );
        }
        if let Geom::Text(t) = &s.geom {
            let story = t.thread.as_ref().map_or(s.id, |t| t.story);
            if t.thread.as_ref().is_none_or(|t| t.prev.is_none()) {
                let xml = self.story(story, t)?;
                self.stories.insert(story, xml);
            }
            let b = shape_bounds(s);
            let prev = t
                .thread
                .as_ref()
                .and_then(|t| t.prev)
                .map_or("n".into(), |id| format!("item{id}"));
            let next = t
                .thread
                .as_ref()
                .and_then(|t| t.next)
                .map_or("n".into(), |id| format!("item{id}"));
            let inset = t.frame.as_ref().map_or([0.; 4], |f| f.inset);
            let valign = t.frame.as_ref().map_or("TopAlign", |f| match f.valign {
                crate::text_geometry::VAlign::Center => "CenterAlign",
                crate::text_geometry::VAlign::Bottom => "BottomAlign",
                _ => "TopAlign",
            });
            if t.on_path.is_some() {
                warn(
                    &mut self.notes,
                    "IDML text on a path exports as editable rectangular text.",
                );
            }
            return Ok(format!(
                "<TextFrame {common} ParentStory=\"story{story}\" PreviousTextFrame=\"{prev}\" NextTextFrame=\"{next}\" GeometricBounds=\"{} {} {} {}\">{}<TextFramePreference TextColumnCount=\"1\" VerticalJustification=\"{valign}\"><Properties><InsetSpacing type=\"list\"><ListItem type=\"unit\">{}</ListItem><ListItem type=\"unit\">{}</ListItem><ListItem type=\"unit\">{}</ListItem><ListItem type=\"unit\">{}</ListItem></InsetSpacing></Properties></TextFramePreference>{opacity}</TextFrame>",
                b.min.y,
                b.min.x,
                b.max.y,
                b.max.x,
                rect_path(b),
                inset[0],
                inset[1],
                inset[2],
                inset[3]
            ));
        }
        let geom = s.geom.to_path();
        let paths = match &geom {
            Geom::Path { anchors, closed } => vec![(anchors.clone(), *closed)],
            Geom::Paths { paths, .. } => paths
                .iter()
                .map(|p| (p.anchors.clone(), p.closed))
                .collect(),
            _ => vec![],
        };
        let center = s.geom.bbox().center();
        let mut path = String::new();
        for (anchors, closed) in paths {
            path.push_str(&format!(
                "<GeometryPathType PathOpen=\"{}\"><PathPointArray>",
                !closed
            ));
            for a in anchors {
                let p = a.pt.rotate_about(center, s.rotation);
                let i = (a.pt + a.h_in).rotate_about(center, s.rotation);
                let o = (a.pt + a.h_out).rotate_about(center, s.rotation);
                path.push_str(&format!("<PathPointType Anchor=\"{} {}\" LeftDirection=\"{} {}\" RightDirection=\"{} {}\"/>",p.x,p.y,i.x,i.y,o.x,o.y));
            }
            path.push_str("</PathPointArray></GeometryPathType>");
        }
        Ok(format!(
            "<Polygon {common}><Properties><PathGeometry>{path}</PathGeometry></Properties>{opacity}</Polygon>"
        ))
    }
    fn image(&mut self, l: &Layer, b: Bounds, offset: Pt) -> Result<String, String> {
        let uri = self
            .doc
            .publishing
            .links
            .get(&l.id)
            .cloned()
            .unwrap_or_else(|| format!("Links/image{}.png", l.id));
        let mut data = String::new();
        if let Some(px) = l.kind.pixels() {
            if px.data.len() > MAX_TOTAL {
                return Err("IDML image exceeds pixel budget".into());
            }
            let mut png = Cursor::new(Vec::new());
            image::write_buffer_with_format(
                &mut png,
                &px.data,
                px.w,
                px.h,
                image::ColorType::Rgba8,
                image::ImageFormat::Png,
            )
            .map_err(|e| e.to_string())?;
            data = format!(
                "<Properties><Contents><![CDATA[{}]]></Contents></Properties>",
                base64::engine::general_purpose::STANDARD.encode(png.into_inner())
            );
        }
        if let LayerKind::Raster {
            rotation, shear, ..
        } = &l.kind
        {
            if *rotation != 0. || *shear != 0. {
                warn(
                    &mut self.notes,
                    "IDML rotated or sheared raster placement exports into its rectangular frame.",
                );
            }
        }
        Ok(format!(
            "<Rectangle Self=\"graphic{}\" Name=\"{}\" ItemLayer=\"{}\" Visible=\"{}\" Locked=\"{}\" ItemTransform=\"1 0 0 1 {} {}\" FillColor=\"Swatch/None\" StrokeColor=\"Swatch/None\" StrokeWeight=\"0\" GeometricBounds=\"{} {} {} {}\">{}<Image Self=\"image{}\" ItemTransform=\"1 0 0 1 0 0\" GeometricBounds=\"{} {} {} {}\">{data}<Link Self=\"link{}\" LinkResourceURI=\"{}\" LinkResourceFormat=\"PNG\" StoredState=\"Embedded\" LinkClassID=\"35906\" LinkClientID=\"257\"/></Image><TransparencySetting><BlendingSetting Opacity=\"{}\" BlendMode=\"Normal\"/></TransparencySetting></Rectangle>",
            l.id,
            xml(&l.name),
            self.layer_id(l.id),
            l.visible,
            l.locked,
            offset.x,
            offset.y,
            b.min.y,
            b.min.x,
            b.max.y,
            b.max.x,
            rect_path(b),
            l.id,
            b.min.y,
            b.min.x,
            b.max.y,
            b.max.x,
            l.id,
            xml(&uri),
            l.opacity * 100.
        ))
    }
    fn story(&self, id: u64, t: &TypeRun) -> Result<String, String> {
        let mut out = format!(
            "<Story Self=\"story{id}\" TrackChanges=\"false\"><StoryPreference OpticalMarginAlignment=\"false\" FrameType=\"TextFrameType\" StoryOrientation=\"Horizontal\" StoryDirection=\"LeftToRightDirection\"/>"
        );
        let mut start = 0;
        for p in t.content.split_inclusive('\n') {
            let count = p.chars().count();
            let style = crate::text::paragraph_style(t, start);
            let align = match style.align {
                TextAlign::Start => "LeftAlign",
                TextAlign::Center => "CenterAlign",
                TextAlign::End => "RightAlign",
                TextAlign::Justify { last } => match last {
                    crate::geom::LastLine::Start => "LeftJustified",
                    crate::geom::LastLine::Center => "CenterJustified",
                    crate::geom::LastLine::End => "RightJustified",
                    crate::geom::LastLine::Justify => "FullyJustified",
                },
            };
            out += &format!(
                "<ParagraphStyleRange AppliedParagraphStyle=\"{}\" Justification=\"{align}\" Hyphenation=\"{}\">",
                xml(style
                    .publishing_style
                    .as_deref()
                    .unwrap_or("ParagraphStyle/$ID/[No paragraph style]")),
                style.hyphenate
            );
            for s in t.character_style_runs(start, start + count) {
                let font = font_name(s.font.as_deref().unwrap_or(&t.font));
                let leading = s.leading.map_or(t.line_height(), |l| l.pixels(t.px));
                out += &format!(
                    "<CharacterStyleRange AppliedCharacterStyle=\"{}\" PointSize=\"{}\" Tracking=\"{}\" HorizontalScale=\"{}\" VerticalScale=\"{}\" BaselineShift=\"{}\" NoBreak=\"{}\"><Properties><AppliedFont type=\"string\">{}</AppliedFont><Leading type=\"unit\">{leading}</Leading></Properties>",
                    xml(s
                        .publishing_style
                        .as_deref()
                        .unwrap_or("CharacterStyle/$ID/[No character style]")),
                    t.px,
                    s.tracking.unwrap_or(t.tracking / t.px.max(1.) * 1000.),
                    s.hscale.unwrap_or(100.),
                    s.vscale.unwrap_or(100.),
                    s.baseline_shift.unwrap_or(0.),
                    s.no_break,
                    xml(&font)
                );
                let text = t
                    .content
                    .chars()
                    .skip(s.start)
                    .take(s.end - s.start)
                    .collect::<String>();
                for chunk in text.split_inclusive(['\n', '\t']) {
                    let content = chunk.trim_end_matches(['\n', '\t']);
                    if !content.is_empty() {
                        out += &format!("<Content>{}</Content>", xml(content));
                    }
                    if chunk.ends_with('\n') {
                        out.push_str("<Br/>");
                    } else if chunk.ends_with('\t') {
                        out.push_str("<Tab/>");
                    }
                }
                out.push_str("</CharacterStyleRange>");
            }
            out.push_str("</ParagraphStyleRange>");
            start += count;
        }
        if t.content.is_empty() {
            out.push_str("<ParagraphStyleRange AppliedParagraphStyle=\"ParagraphStyle/$ID/[No paragraph style]\"><CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/$ID/[No character style]\"><Content/></CharacterStyleRange></ParagraphStyleRange>");
        }
        out.push_str("</Story>");
        Ok(out)
    }
}
fn font_name(path: &str) -> String {
    crate::text::fonts()
        .iter()
        .find(|f| f.path.to_string_lossy() == path)
        .map(|f| f.name.clone())
        .unwrap_or_else(|| "sans-serif".into())
}
fn shape_bounds(s: &Shape) -> Bounds {
    if let Geom::Text(t) = &s.geom {
        crate::text_geometry::frame_bounds(t).unwrap_or_else(|| s.geom.bbox())
    } else {
        s.geom.bbox()
    }
}
fn rect_path(b: Bounds) -> String {
    let mut points = String::new();
    for p in [
        b.min,
        Pt::new(b.max.x, b.min.y),
        b.max,
        Pt::new(b.min.x, b.max.y),
    ] {
        points += &format!(
            "<PathPointType Anchor=\"{} {}\" LeftDirection=\"{} {}\" RightDirection=\"{} {}\"/>",
            p.x, p.y, p.x, p.y, p.x, p.y
        );
    }
    format!(
        "<Properties><PathGeometry><GeometryPathType PathOpen=\"false\"><PathPointArray>{points}</PathPointArray></GeometryPathType></PathGeometry></Properties>"
    )
}
fn style_xml(s: &crate::publishing::PublishingStyle) -> String {
    let mut attrs = format!(" Self=\"{}\" Name=\"{}\"", xml(&s.id), xml(&s.name));
    let mut props = String::new();
    for (k, v) in &s.attributes {
        if matches!(k.as_str(), "Self" | "Name") {
            continue;
        }
        if matches!(k.as_str(), "BasedOn" | "AppliedFont" | "Leading") {
            let ty = match k.as_str() {
                "BasedOn" => "object",
                "Leading" if v != "Auto" => "unit",
                _ => "string",
            };
            props += &format!("<{k} type=\"{ty}\">{}</{k}>", xml(v));
        } else {
            attrs += &format!(" {k}=\"{}\"", xml(v));
        }
    }
    format!(
        "<{}{attrs}><Properties>{props}</Properties></{}>",
        s.kind, s.kind
    )
}
