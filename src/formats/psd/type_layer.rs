//! PSD type records mapped to the shared native TypeRun and CharSpan model.
use super::{
    codec::Reader,
    descriptor::{self, Descriptor, Value as D},
    engine_data::{self, Value as E},
    warn,
};
use crate::{
    color::Rgba,
    document::{Fill, Shape, Style},
    geom::{CharSpan, Geom, Leading, Pt, TextAlign, TypeRun},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TypeData {
    pub text: Descriptor,
    pub warp: Descriptor,
    pub bounds: [f32; 4],
}

fn real(v: f32) -> E {
    E::Real(v as f64)
}
fn dict(items: Vec<(&str, E)>) -> E {
    E::Dict(items.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn array(values: &[f32]) -> E {
    E::Array(values.iter().map(|&v| real(v)).collect())
}
fn num(e: &E, key: &str, default: f32) -> f32 {
    e.get(key)
        .and_then(E::as_f64)
        .filter(|v| v.is_finite() && v.abs() < 1e7)
        .map_or(default, |v| v as f32)
}
fn font_path(name: &str, warnings: &mut Vec<String>) -> String {
    let normalize = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    let needle = normalize(name);
    if let Some(face) = crate::text::all_fonts_cached().iter().find(|f| {
        normalize(&f.name) == needle
            || f.path
                .file_stem()
                .is_some_and(|s| normalize(&s.to_string_lossy()) == needle)
    }) {
        return face.path.to_string_lossy().into_owned();
    }
    warn(
        warnings,
        format!(
            "PSD font '{name}' is unavailable. Text stays editable and retains the requested font name; its preview uses the native fallback font."
        ),
    );
    name.into()
}
fn font_name(font: &str) -> String {
    if !font.is_empty() && !std::path::Path::new(font).is_file() && !font.starts_with("omatype:") {
        return font.into();
    }
    let run = TypeRun {
        font: font.into(),
        ..Default::default()
    };
    crate::text::font_bytes_for(&run)
        .and_then(|b| {
            rustybuzz::ttf_parser::Face::parse(&b, 0)
                .ok()?
                .names()
                .into_iter()
                .find(|n| n.name_id == rustybuzz::ttf_parser::name_id::POST_SCRIPT_NAME)?
                .to_string()
        })
        .unwrap_or_else(|| "DejaVuSans".into())
}
fn color(style: &E) -> Rgba {
    let Some(c) = style.get("FillColor") else {
        return Rgba::BLACK;
    };
    let Some(v) = c.get("Values").and_then(E::as_array) else {
        return Rgba::BLACK;
    };
    let n = |i: usize| {
        v.get(i)
            .and_then(E::as_f64)
            .unwrap_or(if i == 0 { 1. } else { 0. }) as f32
    };
    let rgb = if num(c, "Type", 1.) == 2.0 {
        [
            (1. - n(1)) * (1. - n(4)),
            (1. - n(2)) * (1. - n(4)),
            (1. - n(3)) * (1. - n(4)),
        ]
    } else {
        [n(1), n(2), n(3)]
    };
    Rgba::new(
        crate::deep_color::byte(rgb[0]),
        crate::deep_color::byte(rgb[1]),
        crate::deep_color::byte(rgb[2]),
        crate::deep_color::byte(n(0)),
    )
}
fn merge(base: &E, over: &E) -> E {
    let mut out = base.clone();
    if let E::Dict(fields) = over {
        for (k, v) in fields {
            out.set(k, v.clone());
        }
    }
    out
}
fn chars_for_units(text: &str, units: usize) -> usize {
    let mut used = 0;
    text.chars()
        .take_while(|c| {
            used += c.len_utf16();
            used <= units
        })
        .count()
}

/// Import an editable Photoshop type layer.
/// Args: TySh bytes and conversion notes. Returns the native text shape and retained unsupported descriptor fields.
pub fn decode(bytes: &[u8], warnings: &mut Vec<String>) -> Result<(Shape, TypeData), String> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("PSD type block exceeds 16 MiB".into());
    }
    let mut r = Reader::new(bytes);
    if r.u16()? != 1 {
        return Err("Unsupported PSD type record version".into());
    }
    let mut transform = [0f32; 6];
    for v in &mut transform {
        let n = f64::from_be_bytes(r.take(8)?.try_into().unwrap());
        if !n.is_finite() || n.abs() > 1e7 {
            return Err("Invalid PSD text transform".into());
        }
        *v = n as f32;
    }
    if r.u16()? != 50 {
        return Err("Unsupported PSD text descriptor version".into());
    }
    let text = descriptor::read_versioned(&mut r)?;
    if r.u16()? != 1 {
        return Err("Unsupported PSD text warp version".into());
    }
    let warp = descriptor::read_versioned(&mut r)?;
    let mut bounds = [0.; 4];
    for v in &mut bounds {
        let value = f32::from_be_bytes(r.take(4)?.try_into().unwrap());
        *v = if value.is_finite() { value } else { 0. };
    }
    let content = text
        .get("Txt ")
        .and_then(D::text)
        .ok_or("PSD type descriptor has no text")?
        .replace('\r', "\n");
    if content.chars().count() > 1_048_576 {
        return Err("PSD type content exceeds the editable limit".into());
    }
    let mut run = TypeRun {
        content,
        origin: Pt::ZERO,
        transform: Some(transform),
        ..Default::default()
    };
    let mut fill = Fill::Solid(Rgba::BLACK);
    if warp
        .get("warpStyle")
        .and_then(D::text)
        .is_some_and(|v| v != "warpNone")
    {
        warn(
            warnings,
            "Photoshop text warp parameters are retained for export; native editing previews the unwarped text.",
        );
    }
    if text
        .get("Ornt")
        .and_then(D::text)
        .is_some_and(|v| v == "Vrtc")
    {
        warn(
            warnings,
            "Photoshop vertical type orientation is retained in the PSD descriptor; native type editing uses horizontal layout.",
        );
    }
    if let Some(D::Bytes { value, .. }) = text.get("EngineData") {
        let engine = engine_data::parse(value).map_err(|e| e.to_string())?;
        let fonts = engine
            .path(&["ResourceDict", "FontSet"])
            .and_then(E::as_array)
            .unwrap_or(&[]);
        let empty = E::dict();
        let base = engine
            .path(&["ResourceDict", "StyleSheetSet"])
            .and_then(E::as_array)
            .and_then(|a| a.first())
            .and_then(|s| s.get("StyleSheetData"))
            .unwrap_or(&empty);
        let runs = engine
            .path(&["EngineDict", "StyleRun", "RunArray"])
            .and_then(E::as_array)
            .unwrap_or(&[]);
        let lengths = engine
            .path(&["EngineDict", "StyleRun", "RunLengthArray"])
            .and_then(E::as_array)
            .unwrap_or(&[]);
        let mut offset = 0usize;
        for (i, item) in runs.iter().enumerate() {
            let style = merge(
                base,
                item.path(&["StyleSheet", "StyleSheetData"])
                    .unwrap_or(&empty),
            );
            let length = lengths.get(i).and_then(E::as_i64).unwrap_or(0).max(0) as usize;
            let start = chars_for_units(&run.content, offset);
            offset = offset.saturating_add(length);
            let end = chars_for_units(&run.content, offset).min(run.content.chars().count());
            let font = fonts
                .get(num(&style, "Font", 0.).max(0.) as usize)
                .and_then(|f| f.get("Name"))
                .and_then(E::as_str)
                .unwrap_or("DejaVuSans");
            let font = font_path(font, warnings);
            let px = num(&style, "FontSize", run.px).clamp(0.1, 100000.);
            let c = color(&style);
            if i == 0 {
                run.font = font.clone();
                run.px = px;
                run.tracking = num(&style, "Tracking", 0.) * px / 1000.;
                run.leading = num(&style, "Leading", 0.);
                fill = Fill::Solid(c);
            }
            if end > start {
                run.spans.push(CharSpan {
                    start,
                    end,
                    font: Some(font),
                    px: Some(px),
                    fill: Some(Fill::Solid(c)),
                    tracking: Some(num(&style, "Tracking", 0.)),
                    baseline_shift: Some(num(&style, "BaselineShift", 0.)),
                    hscale: Some(num(&style, "HorizontalScale", 1.) * 100.),
                    vscale: Some(num(&style, "VerticalScale", 1.) * 100.),
                    leading: if num(&style, "Leading", 0.) > 0. {
                        Some(Leading::Fixed(num(&style, "Leading", 0.)))
                    } else {
                        None
                    },
                    ..Default::default()
                });
            }
        }
        if let Some(props) = engine
            .path(&["EngineDict", "ParagraphRun", "RunArray"])
            .and_then(E::as_array)
            .and_then(|a| a.first())
            .and_then(|p| p.path(&["ParagraphSheet", "Properties"]))
        {
            run.align = match num(props, "Justification", 0.) as i32 {
                1 => TextAlign::End,
                2 => TextAlign::Center,
                3..=6 => TextAlign::Justify {
                    last: crate::geom::LastLine::Start,
                },
                _ => TextAlign::Start,
            };
        }
        if let Some(children) = engine
            .path(&["EngineDict", "Rendered", "Shapes", "Children"])
            .and_then(E::as_array)
        {
            if let Some(box_bounds) = children
                .first()
                .and_then(|v| v.path(&["Cookie", "Photoshop", "BoxBounds"]))
                .and_then(E::as_array)
                && box_bounds.len() >= 4
            {
                let left = box_bounds[0].as_f64().unwrap_or(0.) as f32;
                let right = box_bounds[2].as_f64().unwrap_or(0.) as f32;
                if right > left {
                    run.wrap_width = Some(right - left);
                }
            }
        }
    } else {
        warn(
            warnings,
            "PSD type has no EngineData. Text and transform remain editable; font/style uses native defaults.",
        );
    }
    let mut geom = Geom::Text(run);
    crate::text::fill_contours(&mut geom);
    let mut shape = Shape::new(
        geom,
        Style {
            fill,
            ..Default::default()
        },
    );
    shape.name = "Text".into();
    Ok((shape, TypeData { text, warp, bounds }))
}

fn engine_style(run: &TypeRun, index: usize, fill: &Fill, fonts: &mut Vec<String>) -> E {
    let style = crate::text::character_metrics(run, index);
    let font = font_name(style.font.as_deref().unwrap_or(&run.font));
    let fi = if let Some(i) = fonts.iter().position(|f| f == &font) {
        i
    } else {
        fonts.push(font);
        fonts.len() - 1
    };
    let c = match style.fill.as_ref().unwrap_or(fill) {
        Fill::Solid(c) => *c,
        _ => Rgba::BLACK,
    };
    dict(vec![
        ("Font", E::Int(fi as i64)),
        ("FontSize", real(style.px.unwrap_or(run.px))),
        ("Tracking", real(style.tracking.unwrap_or(0.))),
        (
            "Leading",
            real(
                style
                    .leading
                    .map_or(run.leading, |l| l.pixels(style.px.unwrap_or(run.px))),
            ),
        ),
        (
            "AutoLeading",
            E::Bool(style.leading.is_none() && run.leading <= 0.),
        ),
        ("AutoKerning", E::Bool(run.kern)),
        ("Kerning", E::Int(0)),
        ("HorizontalScale", real(style.hscale.unwrap_or(100.) / 100.)),
        ("VerticalScale", real(style.vscale.unwrap_or(100.) / 100.)),
        ("BaselineShift", real(style.baseline_shift.unwrap_or(0.))),
        (
            "FillColor",
            dict(vec![
                ("Type", E::Int(1)),
                (
                    "Values",
                    array(&[
                        c.a as f32 / 255.,
                        c.r as f32 / 255.,
                        c.g as f32 / 255.,
                        c.b as f32 / 255.,
                    ]),
                ),
            ]),
        ),
        ("FillFlag", E::Bool(true)),
        ("StrokeFlag", E::Bool(false)),
    ])
}

/// Export the current shared native type model into a Photoshop TySh block.
/// Args: native text shape, optional retained Photoshop descriptors and conversion notes.
/// Returns an editable type record with regenerated text, spans, fonts and transform.
pub fn encode(
    shape: &Shape,
    retained: Option<&TypeData>,
    warnings: &mut Vec<String>,
) -> Result<Vec<u8>, String> {
    let Geom::Text(run) = &shape.geom else {
        return Err("Expected native text".into());
    };
    if !matches!(shape.style.fill, Fill::Solid(_))
        || run.spans.iter().any(|s| s.fill.as_ref().is_some_and(|f| !matches!(f, Fill::Solid(_))))
    {
        return Err("Editable Photoshop type cannot retain non-solid text paint. Save .oma or export SVG to preserve the original paint.".into());
    }
    let mut text = retained
        .map(|d| d.text.clone())
        .unwrap_or_else(|| Descriptor::new("TxLr"));
    let mut engine = match text.get("EngineData") {
        Some(D::Bytes { value, .. }) => engine_data::parse(value).map_err(|e| e.to_string())?,
        _ => E::dict(),
    };
    let content = run.content.replace('\n', "\r") + "\r";
    let count = run.content.chars().count();
    let mut fonts = Vec::new();
    let mut cuts = vec![0, count];
    for span in &run.spans {
        cuts.push(span.start.min(count));
        cuts.push(span.end.min(count));
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut runs = Vec::new();
    let mut lengths = Vec::new();
    let mut first = None;
    for (i, pair) in cuts.windows(2).enumerate() {
        if pair[1] == pair[0] {
            continue;
        }
        let style = engine_style(run, pair[0], &shape.style.fill, &mut fonts);
        if first.is_none() {
            first = Some(style.clone());
        }
        runs.push(dict(vec![(
            "StyleSheet",
            dict(vec![("StyleSheetData", style)]),
        )]));
        let units = run
            .content
            .chars()
            .skip(pair[0])
            .take(pair[1] - pair[0])
            .map(char::len_utf16)
            .sum::<usize>()
            + usize::from(i + 1 == cuts.len() - 1);
        lengths.push(E::Int(units as i64));
    }
    if runs.is_empty() {
        let style = engine_style(run, 0, &shape.style.fill, &mut fonts);
        first = Some(style.clone());
        runs.push(dict(vec![(
            "StyleSheet",
            dict(vec![("StyleSheetData", style)]),
        )]));
        lengths.push(E::Int(1));
    }
    let paragraph = dict(vec![
        (
            "Justification",
            E::Int(match run.align {
                TextAlign::Start => 0,
                TextAlign::End => 1,
                TextAlign::Center => 2,
                TextAlign::Justify { .. } => 3,
            }),
        ),
        ("AutoLeading", real(1.2)),
        ("WordSpacing", array(&[0.8, 1., 1.33])),
        ("LetterSpacing", array(&[0., 0., 0.])),
        ("GlyphSpacing", array(&[1., 1., 1.])),
        ("FirstLineIndent", real(0.)),
        ("StartIndent", real(0.)),
        ("EndIndent", real(0.)),
        ("SpaceBefore", real(0.)),
        ("SpaceAfter", real(0.)),
    ]);
    let mut ed = engine.get("EngineDict").cloned().unwrap_or_else(E::dict);
    ed.set("Editor", dict(vec![("Text", E::String(content.clone()))]));
    ed.set(
        "StyleRun",
        dict(vec![
            (
                "DefaultRunData",
                dict(vec![(
                    "StyleSheet",
                    dict(vec![("StyleSheetData", E::dict())]),
                )]),
            ),
            ("RunArray", E::Array(runs)),
            ("RunLengthArray", E::Array(lengths)),
            ("IsJoinable", E::Int(2)),
        ]),
    );
    let mut paragraph_lengths = Vec::new();
    let mut paragraph_runs = Vec::new();
    for line in content.split_inclusive('\r') {
        paragraph_lengths.push(E::Int(line.encode_utf16().count() as i64));
        paragraph_runs.push(dict(vec![
            (
                "ParagraphSheet",
                dict(vec![
                    ("DefaultStyleSheet", E::Int(0)),
                    ("Properties", paragraph.clone()),
                ]),
            ),
            (
                "Adjustments",
                dict(vec![
                    ("Axis", array(&[1., 0., 1.])),
                    ("XY", array(&[0., 0.])),
                ]),
            ),
        ]));
    }
    ed.set(
        "ParagraphRun",
        dict(vec![
            (
                "DefaultRunData",
                dict(vec![(
                    "ParagraphSheet",
                    dict(vec![
                        ("DefaultStyleSheet", E::Int(0)),
                        ("Properties", E::dict()),
                    ]),
                )]),
            ),
            ("RunArray", E::Array(paragraph_runs)),
            ("RunLengthArray", E::Array(paragraph_lengths)),
            ("IsJoinable", E::Int(1)),
        ]),
    );
    ed.set("AntiAlias", E::Int(1));
    ed.set("UseFractionalGlyphWidths", E::Bool(true));
    engine.set("EngineDict", ed);
    let mut resource = engine.get("ResourceDict").cloned().unwrap_or_else(E::dict);
    resource.set(
        "FontSet",
        E::Array(
            fonts
                .iter()
                .map(|f| {
                    dict(vec![
                        ("Name", E::String(f.clone())),
                        ("Script", E::Int(0)),
                        ("FontType", E::Int(0)),
                        ("Synthetic", E::Int(0)),
                    ])
                })
                .collect(),
        ),
    );
    resource.set("TheNormalStyleSheet", E::Int(0));
    resource.set("TheNormalParagraphSheet", E::Int(0));
    resource.set(
        "StyleSheetSet",
        E::Array(vec![dict(vec![
            ("Name", E::String("Normal RGB".into())),
            ("StyleSheetData", first.unwrap()),
        ])]),
    );
    resource.set(
        "ParagraphSheetSet",
        E::Array(vec![dict(vec![
            ("Name", E::String("Normal RGB".into())),
            ("DefaultStyleSheet", E::Int(0)),
            ("Properties", paragraph),
        ])]),
    );
    engine.set("ResourceDict", resource.clone());
    engine.set("DocumentResources", resource);
    text.set("Txt ", D::Text(run.content.replace('\n', "\r")));
    text.set(
        "EngineData",
        D::Bytes {
            kind: *b"tdta",
            value: engine_data::write(&engine),
        },
    );
    for (key, class, value) in [
        ("textGridding", "textGridding", "None"),
        ("Ornt", "Ornt", "Hrzn"),
        ("AntA", "Annt", "AnSh"),
    ] {
        if text.get(key).is_none() {
            text.set(
                key,
                D::Enum {
                    class: class.into(),
                    value: value.into(),
                },
            );
        }
    }
    if run.on_path.is_some() || run.svg_text.is_some() {
        warn(
            warnings,
            "Native path/SVG text is exported as editable Photoshop point type; SVG path positioning remains editable in .oma/SVG.",
        );
    }
    let warp = retained.map(|d| d.warp.clone()).unwrap_or_else(|| {
        Descriptor::new("warp")
            .with(
                "warpStyle",
                D::Enum {
                    class: "warpStyle".into(),
                    value: "warpNone".into(),
                },
            )
            .with("warpValue", D::Double(0.))
            .with("warpPerspective", D::Double(0.))
            .with("warpPerspectiveOther", D::Double(0.))
            .with(
                "warpRotate",
                D::Enum {
                    class: "Ornt".into(),
                    value: "Hrzn".into(),
                },
            )
    });
    let m = run.transform.unwrap_or([1., 0., 0., 1., 0., 0.]);
    let mut transform = [
        m[0],
        m[1],
        m[2],
        m[3],
        m[4] + m[0] * run.origin.x + m[2] * run.origin.y,
        m[5] + m[1] * run.origin.x + m[3] * run.origin.y,
    ];
    if shape.rotation != 0. {
        let (s, c) = shape.rotation.sin_cos();
        let center = shape.geom.bbox().center();
        let rotation = [
            c,
            s,
            -s,
            c,
            center.x - c * center.x + s * center.y,
            center.y - s * center.x - c * center.y,
        ];
        transform = compose(rotation, transform);
    }
    let mut out = Vec::new();
    out.extend_from_slice(&1u16.to_be_bytes());
    for value in transform {
        out.extend_from_slice(&(value as f64).to_be_bytes());
    }
    out.extend_from_slice(&50u16.to_be_bytes());
    descriptor::write_versioned(&mut out, &text);
    out.extend_from_slice(&1u16.to_be_bytes());
    descriptor::write_versioned(&mut out, &warp);
    for v in retained.map_or([0.; 4], |d| d.bounds) {
        out.extend_from_slice(&v.to_be_bytes());
    }
    Ok(out)
}
fn compose(a: [f32; 6], b: [f32; 6]) -> [f32; 6] {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn non_solid_type_paint_is_not_silently_replaced_with_black() {
        for span in [false, true] {
            let mut doc = crate::formats::psd::decode(include_bytes!("fixtures/independent-type.psd"), "Paint").unwrap().document;
            let shape = &mut doc.layers[0].kind.shapes_mut().unwrap()[0];
            let fill = Fill::Radial {c0:Rgba::BLACK,c1:Rgba::WHITE};
            if span {
                let Geom::Text(run) = &mut shape.geom else {panic!()};
                run.spans[0].fill = Some(fill);
            } else {shape.style.fill = fill;}
            let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
            for large in [false,true] {
                let error = crate::formats::psd::encode(&reopened,large).err().expect("Unsupported paint must not become black editable type");
                assert!(error.contains("non-solid text paint"),"{error}");
            }
        }
    }
    #[test]
    fn independent_type_remains_editable_after_native_save_and_psd_export() {
        let fixture = include_bytes!("fixtures/independent-type.psd");
        let mut result = crate::formats::psd::decode(fixture, "Independent type").unwrap();
        assert!(
            !result
                .warnings
                .iter()
                .any(|n| n.contains("could not be read")),
            "{:?}",
            result.warnings
        );
        let shape = &result.document.layers[0].kind.shapes().unwrap()[0];
        let Geom::Text(run) = &shape.geom else {
            panic!("Text was reduced to pixels")
        };
        assert_eq!(run.content, "Hello Ω");
        assert_eq!(run.transform, Some([1., 0.125, 0.25, 1., 0., 1.]));
        assert!(run.spans.iter().any(|s| s.px == Some(24.)));
        result.document =
            crate::project::decode(&crate::project::encode(&result.document).unwrap()).unwrap();
        let shape = &mut result.document.layers[0].kind.shapes_mut().unwrap()[0];
        let Geom::Text(run) = &mut shape.geom else {
            panic!()
        };
        run.content = "Edited Ω and 😀".into();
        run.spans[1].end = run.content.chars().count();
        crate::text::fill_contours(&mut shape.geom);
        for large in [false, true] {
            let exported = crate::formats::psd::encode(&result.document, large).unwrap();
            let reopened = crate::formats::psd::decode(&exported.bytes, "Reopened")
                .unwrap()
                .document;
            let Geom::Text(run) = &reopened.layers[0].kind.shapes().unwrap()[0].geom else {
                panic!()
            };
            assert_eq!(run.content, "Edited Ω and 😀");
            assert_eq!(run.transform, Some([1., 0.125, 0.25, 1., 0., 1.]));
            assert!(run.spans.iter().any(|s| s.px == Some(24.)));
            if !large && let Ok(dir) = std::env::var("OMADESIGN_DEEP_QA_DIR") {
                std::fs::write(
                    std::path::Path::new(&dir).join("editable-type.psd"),
                    exported.bytes,
                )
                .unwrap();
            }
        }
    }
    #[test]
    fn native_layer_with_multiple_type_shapes_exports_each_as_editable_type() {
        let mut doc = crate::document::Document::new("Two type objects", 32., 32., 72.);
        doc.transparent = true;
        let mut layer = crate::document::Layer::vector("Type");
        for content in ["One", "Two"] {
            let mut shape = Shape::new(
                Geom::Text(TypeRun {
                    content: content.into(),
                    px: 10.,
                    origin: Pt::new(1., 12.),
                    ..Default::default()
                }),
                Style::default(),
            );
            crate::text::fill_contours(&mut shape.geom);
            layer.kind.shapes_mut().unwrap().push(shape);
        }
        doc.layers = vec![layer];
        let out = crate::formats::psd::encode(&doc, false).unwrap();
        let doc = crate::formats::psd::decode(&out.bytes, "Two")
            .unwrap()
            .document;
        let contents: Vec<_> = doc
            .layers
            .iter()
            .filter_map(|l| l.kind.shapes())
            .flatten()
            .filter_map(|s| {
                if let Geom::Text(t) = &s.geom {
                    Some(t.content.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(contents, ["One", "Two"]);
    }
}
