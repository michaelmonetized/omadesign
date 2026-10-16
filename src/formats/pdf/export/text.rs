//! Embedded text uses the same cached face and positioned glyphs as the canvas.
use super::*;
use crate::geom::TypeRun;
use rustybuzz::ttf_parser::{Face, GlyphId, Permissions};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

#[derive(Default)]
pub(super) struct Fonts {
    entries: HashMap<u128, Embedded>,
}
struct Embedded {
    bytes: Arc<Vec<u8>>,
    font: ObjectId,
    descendant: ObjectId,
    unicode: ObjectId,
    mapping: ObjectId,
    codes: BTreeMap<(u16, String), u16>,
    glyphs: Vec<(u16, String)>,
}

/// Validate text numbers before writing PDF operators.
/// Args: source run. Returns an error for nonfinite size, placement or composed glyphs.
pub(super) fn validate(run: &TypeRun) -> Result<(), String> {
    if [
        run.px,
        run.origin.x,
        run.origin.y,
        run.tracking,
        run.leading,
    ]
    .iter()
    .any(|n| !n.is_finite())
    {
        return Err("PDF text geometry must be finite".into());
    }
    if run
        .transform
        .is_some_and(|m| m.iter().any(|v| !v.is_finite()))
        || run
            .spans
            .iter()
            .any(|s| s.px.is_some_and(|v| !v.is_finite() || v <= 0.))
    {
        return Err("PDF text transforms and range sizes must be finite and positive".into());
    }
    if run.svg_text.is_none()
        && crate::text::compose(run)
            .iter()
            .flat_map(|l| &l.glyphs)
            .any(|g| {
                [g.x, g.y, g.hscale, g.vscale]
                    .iter()
                    .any(|n| !n.is_finite())
            })
    {
        return Err("PDF composed glyph geometry must be finite".into());
    }
    Ok(())
}

/// Write positioned, editable glyphs with their permitted full TrueType face.
/// Args: PDF, font registry, source object and output. Returns a reason when outlines are required.
pub(super) fn write(
    writer: &mut Writer,
    shape: &Shape,
    bounds: Bounds,
    out: &mut Scene,
) -> Result<(), String> {
    let Geom::Text(run) = &shape.geom else {
        return Err("object is not text".into());
    };
    if run.svg_text.is_some() {
        return Err(
            "positioned/path/vertical SVG source layout retains its exact painted outlines".into(),
        );
    }
    if run.on_path.is_some() || run.frame.is_some() {
        return Err("path and area layout retains its painted outlines".into());
    }
    if shape
        .style
        .stroke
        .as_ref()
        .is_some_and(|s| s.gradient.is_some())
    {
        return Err("gradient text stroke retains its vector outline".into());
    }
    let lines = crate::text::compose(run);
    for line in lines.iter() {
        let mut seen = HashSet::new();
        if line.glyphs.iter().any(|g| !seen.insert(g.cluster)) {
            return Err("multi-glyph text clusters retain their outlines".into());
        }
    }
    let mut resolved = HashMap::new();
    let mut faces: HashMap<String, (u128, u16)> = HashMap::new();
    for glyph in lines.iter().flat_map(|line| &line.glyphs) {
        let style = run.character_style(glyph.cluster);
        let font = style.font.unwrap_or_else(|| run.font.clone());
        let (key, glyph_count) = if let Some(face) = faces.get(&font) {
            *face
        } else {
            let mut effective = run.clone();
            effective.font = font.clone();
            let bytes = crate::text::font_bytes_for(&effective)
                .ok_or("the composer's resolved font bytes are unavailable")?;
            let face = Face::parse(&bytes, 0).map_err(|_| "the resolved font cannot be parsed")?;
            if !editable_font(&face) {
                return Err("the resolved face does not permit editable outline embedding".into());
            }
            if face.tables().glyf.is_none()
                || face.is_variable()
                || bytes.starts_with(b"ttcf")
                || bytes.len() > 16 * 1024 * 1024
            {
                return Err("this font container or variable/CFF face requires outlines".into());
            }
            let key = crate::typography::fingerprint(&bytes);
            if !writer.fonts.entries.contains_key(&key) {
                writer
                    .fonts
                    .entries
                    .insert(key, Embedded::new(&mut writer.pdf, bytes.clone(), &face)?);
            }
            let entry = (key, face.number_of_glyphs());
            faces.insert(font, entry);
            entry
        };
        if glyph.id >= glyph_count {
            return Err("the composed glyph does not belong to the resolved face".into());
        }
        resolved.insert(glyph.cluster, (key, style.px.unwrap_or(run.px).max(1.)));
    }
    let character_bytes = run
        .content
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(run.content.len()))
        .collect::<Vec<_>>();
    let text_bounds = shape.geom.bbox();
    let text_page_bounds = paint::local_bounds(bounds, text_bounds.center(), shape.rotation);
    op(&mut out.ops, "q", &[]);
    rotate_matrix(&mut out.ops, text_bounds.center(), shape.rotation);
    let [a, b, c, d, _, _] = run.transform.unwrap_or([1., 0., 0., 1., 0., 0.]);
    let mut last_mode = None;
    for line in lines.iter() {
        let mut clusters = line
            .glyphs
            .iter()
            .map(|g| g.cluster)
            .chain(std::iter::once(line.end))
            .collect::<Vec<_>>();
        clusters.sort_unstable();
        clusters.dedup();
        let ends = clusters
            .windows(2)
            .map(|p| (p[0], p[1]))
            .collect::<HashMap<_, _>>();
        let actual = run
            .content
            .chars()
            .skip(line.start)
            .take(line.end - line.start)
            .collect::<String>();
        out.ops.push(Operation::new(
            "BDC",
            vec![
                Object::Name(b"Span".to_vec()),
                Object::Dictionary(dictionary! {"ActualText"=>lopdf::text_string(&actual)}),
            ],
        ));
        for glyph in &line.glyphs {
            let end = ends.get(&glyph.cluster).copied().unwrap_or(line.end);
            let unicode = run.content[character_bytes[glyph.cluster]..character_bytes[end]]
                .chars()
                .filter(|&c| c != '\u{ad}')
                .collect::<String>();
            let &(key, px) = resolved
                .get(&glyph.cluster)
                .ok_or("PDF glyph face resolution failed")?;
            let embedded = writer
                .fonts
                .entries
                .get_mut(&key)
                .ok_or("PDF font registration failed")?;
            let cid = embedded.code(glyph.id, unicode)?;
            let font = embedded.font;
            let fill = run
                .character_style(glyph.cluster)
                .fill
                .unwrap_or_else(|| shape.style.fill.clone());
            let color = if let Fill::Solid(color) = fill {
                Some(color)
            } else {
                None
            };
            let state = writer.state(
                byte(color.map_or(1., |c| c.a as f32 / 255.)),
                255,
                Blend::Normal,
            );
            out.invoke("gs", "ExtGState", state);
            if let Some(color) = color {
                op(
                    &mut out.ops,
                    "rg",
                    &[
                        color.r as f32 / 255.,
                        color.g as f32 / 255.,
                        color.b as f32 / 255.,
                    ],
                );
            }
            let mut glyph_ops = Vec::new();
            op(&mut glyph_ops, "BT", &[]);
            let name = out.resource("Font", font);
            glyph_ops.push(Operation::new(
                "Tf",
                vec![Object::Name(name.into_bytes()), Object::Real(px)],
            ));
            let p = run.map_point(crate::geom::Pt::new(
                run.origin.x + glyph.x,
                run.origin.y + glyph.y,
            ));
            op(
                &mut glyph_ops,
                "Tm",
                &[
                    a * glyph.hscale,
                    b * glyph.hscale,
                    -c * glyph.vscale,
                    -d * glyph.vscale,
                    p.x,
                    p.y,
                ],
            );
            let show = Operation::new(
                "Tj",
                vec![Object::String(
                    cid.to_be_bytes().to_vec(),
                    lopdf::StringFormat::Hexadecimal,
                )],
            );
            if color.is_none() && !fill.is_none() {
                op(&mut out.ops, "q", &[]);
                out.ops.extend(glyph_ops.clone());
                op(&mut out.ops, "Tr", &[7.]);
                out.ops.push(show.clone());
                op(&mut out.ops, "ET", &[]);
                let mut painted = Shape::new(
                    Geom::Rect {
                        origin: text_bounds.min,
                        size: Pt::new(text_bounds.width(), text_bounds.height()),
                        radius: 0.,
                    },
                    crate::document::Style { fill, stroke: None },
                );
                painted.id = shape.id;
                writer.paint(&painted, false, text_page_bounds, out)?;
                op(&mut out.ops, "Q", &[]);
                continue;
            }
            out.ops.extend(glyph_ops);
            let mode = if color.is_some() { 0. } else { 3. };
            if last_mode != Some(mode) {
                op(&mut out.ops, "Tr", &[mode]);
                last_mode = Some(mode);
            }
            out.ops.push(show);
            op(&mut out.ops, "ET", &[]);
        }
        op(&mut out.ops, "EMC", &[]);
    }
    op(&mut out.ops, "Q", &[]);
    if let Some(stroke) = shape.style.stroke.as_ref().filter(|s| s.width > 0.) {
        let mut style = tiny_skia::Stroke {
            width: stroke.width,
            line_cap: stroke.cap.to_skia(),
            line_join: stroke.join.to_skia(),
            ..Default::default()
        };
        style.dash = stroke
            .dash
            .and_then(|(a, b)| tiny_skia::StrokeDash::new(vec![a, b], stroke.dash_offset));
        let outline = shape
            .get_cached_path(96)
            .and_then(|p| p.stroke(&style, 1.))
            .ok_or("the text stroke cannot be outlined")?;
        op(&mut out.ops, "q", &[]);
        let state = writer.state(stroke.color.a, 255, Blend::Normal);
        out.invoke("gs", "ExtGState", state);
        op(
            &mut out.ops,
            "rg",
            &[
                stroke.color.r as f32 / 255.,
                stroke.color.g as f32 / 255.,
                stroke.color.b as f32 / 255.,
            ],
        );
        skia_path(&mut out.ops, &outline);
        op(&mut out.ops, "f", &[]);
        op(&mut out.ops, "Q", &[]);
        writer.warnings.insert(format!(
            "PDF text object {} keeps its stroke as a vector outline; glyph fills remain embedded and editable.",
            shape.id
        ));
    }
    Ok(())
}

/// Preserve each text range's actual paint when editable PDF layout is unavailable.
/// Args: writer, current text object, page bounds and output. Returns an outline/source encoding error.
pub(super) fn outlines(
    writer: &mut Writer,
    shape: &Shape,
    bounds: Bounds,
    out: &mut Scene,
) -> Result<(), String> {
    let Geom::Text(run) = &shape.geom else {
        return Err("Expected text for PDF outlines".into());
    };
    if run.svg_text.is_some() {
        let painted = if run
            .svg_text
            .as_ref()
            .is_some_and(|s| s.base_fill != shape.style.fill)
        {
            crate::text::svg_type::object_paint(run, &shape.style.fill)
        } else {
            run.clone()
        };
        let layout = crate::text::svg_type::layout(&painted)
            .ok_or("PDF could not resolve the current SVG text source")?;
        let source = layout.tree.to_string(&usvg::WriteOptions::default());
        let (doc, notes) = crate::formats::svg::read(&source, "PDF source text outlines")?;
        let (doc, more) = fallback::prepare(&doc)?;
        for note in notes.into_iter().chain(more) {
            writer
                .warnings
                .insert(format!("PDF text object {}: {note}", shape.id));
        }
        op(&mut out.ops, "q", &[]);
        rotate_matrix(&mut out.ops, shape.geom.bbox().center(), shape.rotation);
        writer.children(
            &doc,
            None,
            paint::local_bounds(bounds, shape.geom.bbox().center(), shape.rotation),
            out,
            0,
            true,
        )?;
        op(&mut out.ops, "Q", &[]);
        return Ok(());
    }
    let center = shape.geom.bbox().center();
    for (fill, mut contours) in crate::text::paint_groups(run, &shape.style.fill) {
        if fill.is_none() {
            continue;
        }
        let mut clip = shape.clone();
        for point in contours.iter_mut().flatten() {
            *point = point.rotate_about(center, shape.rotation);
        }
        clip.geom = Geom::Poly {
            contours,
            winding: true,
        };
        clip.rotation = 0.;
        op(&mut out.ops, "q", &[]);
        path(&mut out.ops, &clip);
        op(&mut out.ops, "W", &[]);
        op(&mut out.ops, "n", &[]);
        let mut painted = shape.clone();
        painted.style.fill = fill;
        painted.style.stroke = None;
        writer.paint(&painted, false, bounds, out)?;
        op(&mut out.ops, "Q", &[]);
    }
    if shape.style.stroke.as_ref().is_some_and(|s| s.width > 0.) {
        writer.paint(shape, true, bounds, out)?;
    }
    Ok(())
}

/// Respect the face's editable embedding permissions.
/// Args: parsed face. Returns true only when full editable outline embedding is permitted.
fn editable_font(face: &Face<'_>) -> bool {
    matches!(
        face.permissions(),
        Some(Permissions::Installable | Permissions::Editable)
    ) && face.is_outline_embedding_allowed()
}
impl Embedded {
    /// Register a full embedded TrueType font.
    /// Args: PDF store, cached bytes and face. Returns font resources and pending glyph maps.
    fn new(pdf: &mut Pdf, bytes: Arc<Vec<u8>>, face: &Face<'_>) -> Result<Self, String> {
        let name = face
            .names()
            .into_iter()
            .filter(|n| n.name_id == 6)
            .find_map(|n| n.to_string())
            .unwrap_or_else(|| "OmadesignFont".into())
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect::<String>();
        let name = Object::Name(name.into_bytes());
        let scale = 1000. / face.units_per_em() as f32;
        let metric = |n: i16| Object::Real(n as f32 * scale);
        let bbox = face.global_bounding_box();
        let file = pdf.add_object(Stream::new(
            dictionary! {"Length1"=>bytes.len() as i64},
            bytes.as_ref().clone(),
        ));
        let descriptor=pdf.add_object(dictionary!{
            "Type"=>"FontDescriptor","FontName"=>name.clone(),"Flags"=>32,"FontBBox"=>vec![metric(bbox.x_min),metric(bbox.y_min),metric(bbox.x_max),metric(bbox.y_max)],
            "ItalicAngle"=>face.italic_angle(),"Ascent"=>metric(face.ascender()),"Descent"=>metric(face.descender()),"CapHeight"=>metric(face.capital_height().unwrap_or(face.ascender())),"StemV"=>80,"FontFile2"=>file,
        });
        let unicode = pdf.new_object_id();
        let mapping = pdf.new_object_id();
        let descendant=pdf.add_object(dictionary!{
            "Type"=>"Font","Subtype"=>"CIDFontType2","BaseFont"=>name.clone(),"FontDescriptor"=>descriptor,
            "CIDSystemInfo"=>dictionary!{"Registry"=>Object::string_literal("Adobe"),"Ordering"=>Object::string_literal("Identity"),"Supplement"=>0},
            "CIDToGIDMap"=>mapping,"DW"=>1000,
        });
        let font=pdf.add_object(dictionary!{"Type"=>"Font","Subtype"=>"Type0","BaseFont"=>name,"Encoding"=>"Identity-H","DescendantFonts"=>vec![Object::Reference(descendant)],"ToUnicode"=>unicode});
        Ok(Self {
            bytes,
            font,
            descendant,
            unicode,
            mapping,
            codes: BTreeMap::new(),
            glyphs: vec![(0, String::new())],
        })
    }
    /// Assign a stable CID to a glyph and Unicode sequence.
    /// Args: glyph ID and text. Returns its CID or a map-size error.
    fn code(&mut self, glyph: u16, text: String) -> Result<u16, String> {
        let key = (glyph, text);
        if let Some(id) = self.codes.get(&key) {
            return Ok(*id);
        }
        let code = u16::try_from(self.glyphs.len())
            .map_err(|_| "PDF font exceeds 65,535 distinct glyph mappings")?;
        self.glyphs.push(key.clone());
        self.codes.insert(key, code);
        Ok(code)
    }
}
impl Fonts {
    /// Finish shared font maps after every page has registered its glyphs.
    /// Args: PDF object store. Returns an error for an invalid embedded face.
    pub(super) fn finish(self, pdf: &mut Pdf) -> Result<(), String> {
        for embedded in self.entries.into_values() {
            let face =
                Face::parse(&embedded.bytes, 0).map_err(|_| "PDF embedded font is invalid")?;
            let mut mapping = Vec::with_capacity(embedded.glyphs.len() * 2);
            let mut widths = vec![];
            for (glyph, _) in &embedded.glyphs {
                mapping.extend_from_slice(&glyph.to_be_bytes());
                widths.push(Object::Real(
                    face.glyph_hor_advance(GlyphId(*glyph)).unwrap_or(0) as f32 * 1000.
                        / face.units_per_em() as f32,
                ));
            }
            pdf.get_dictionary_mut(embedded.descendant)
                .map_err(|e| e.to_string())?
                .set("W", vec![Object::Integer(0), Object::Array(widths)]);
            pdf.objects.insert(
                embedded.mapping,
                Object::Stream(Stream::new(Dictionary::new(), mapping)),
            );
            let mut cmap="/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /OmadesignUnicode def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n".to_string();
            let entries = embedded
                .glyphs
                .iter()
                .enumerate()
                .skip(1)
                .filter(|(_, (_, text))| !text.is_empty())
                .collect::<Vec<_>>();
            for group in entries.chunks(100) {
                cmap.push_str(&format!("{} beginbfchar\n", group.len()));
                for (cid, (_, unicode)) in group {
                    let target = unicode
                        .encode_utf16()
                        .map(|u| format!("{u:04X}"))
                        .collect::<String>();
                    cmap.push_str(&format!("<{cid:04X}> <{target}>\n"));
                }
                cmap.push_str("endbfchar\n");
            }
            cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
            pdf.objects.insert(
                embedded.unicode,
                Object::Stream(Stream::new(Dictionary::new(), cmap.into_bytes())),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pdf_exchange_rejects_restricted_and_print_only_font_embedding() {
        let run = crate::geom::TypeRun {
            font: [
                "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
                "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
                "/usr/share/fonts/noto/NotoSans-Regular.ttf",
            ]
            .into_iter()
            .find(|p| std::path::Path::new(p).is_file())
            .expect("PDF text fixtures need Liberation Sans or Noto Sans")
            .into(),
            ..Default::default()
        };
        let bytes = crate::text::font_bytes_for(&run).expect("test font");
        let face = Face::parse(&bytes, 0).unwrap();
        assert!(editable_font(&face));
        let offset = face
            .raw_face()
            .table(rustybuzz::ttf_parser::Tag::from_bytes(b"OS/2"))
            .unwrap()
            .as_ptr() as usize
            - bytes.as_ptr() as usize;
        for flags in [0x0002u16, 0x0004, 0x0200] {
            let mut restricted = bytes.as_ref().clone();
            restricted[offset + 8..offset + 10].copy_from_slice(&flags.to_be_bytes());
            assert!(
                !editable_font(&Face::parse(&restricted, 0).unwrap()),
                "fsType {flags:#x} allowed editable embedding"
            );
        }
    }
}
