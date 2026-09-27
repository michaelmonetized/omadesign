use super::*;
use rustybuzz::ttf_parser::Tag;

pub fn feature_value(run: &TypeRun, index: usize, tag: [u8; 4]) -> u32 {
    run.spans
        .iter()
        .find(|s| s.start <= index && index < s.end)
        .and_then(|s| s.features.iter().rev().find(|(t, _)| *t == tag))
        .or_else(|| run.features.iter().rev().find(|(t, _)| *t == tag))
        .map(|(_, v)| *v)
        .unwrap_or(match &tag {
            b"kern" => u32::from(run.kern),
            b"liga" | b"clig" => u32::from(run.liga),
            b"tnum" => u32::from(run.tnum),
            b"smcp" | b"c2sc" => u32::from(run.smcp),
            b"calt" => 1,
            _ => 0,
        })
}

/// GSUB/GPOS feature union, with localized stylistic-set labels when supplied.
pub fn font_features(path: &str) -> Vec<(Tag, Option<String>)> {
    type Features = Vec<(Tag, Option<String>)>;
    static CACHE: OnceLock<Mutex<HashMap<(String, u64), Features>>> = OnceLock::new();
    let key = (
        path.to_owned(),
        FONT_REVISION.load(std::sync::atomic::Ordering::Relaxed),
    );
    let cache = CACHE.get_or_init(Default::default);
    if let Some(value) = cache.lock().unwrap().get(&key).cloned() {
        return value;
    }
    let run = TypeRun {
        font: path.into(),
        ..Default::default()
    };
    let Some(bytes) = resolve_path(&run).and_then(|p| font_bytes(&p)) else {
        return vec![];
    };
    let Ok(face) = rustybuzz::ttf_parser::Face::parse(&bytes, 0) else {
        return vec![];
    };
    let mut result = std::collections::BTreeMap::new();
    for table in [face.tables().gsub, face.tables().gpos]
        .into_iter()
        .flatten()
    {
        for feature in table.features {
            result.entry(feature.tag).or_insert(None);
        }
    }
    // ttf-parser skips FeatureParams; read its checked offsets from the original table.
    let u16_at = |data: &[u8], at: usize| -> Option<usize> {
        Some(u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?) as usize)
    };
    for tag in [Tag::from_bytes(b"GSUB"), Tag::from_bytes(b"GPOS")] {
        let Some(data) = face.raw_face().table(tag) else {
            continue;
        };
        let Some(list) = u16_at(data, 6) else {
            continue;
        };
        let Some(count) = u16_at(data, list) else {
            continue;
        };
        for i in 0..count {
            let record = list + 2 + i * 6;
            let Some(raw) = data.get(record..record + 4) else {
                continue;
            };
            let tag = Tag::from_bytes(raw.try_into().unwrap());
            if raw[0..2] != *b"ss" {
                continue;
            }
            let Some(offset) = u16_at(data, record + 4) else {
                continue;
            };
            let feature = list + offset;
            let Some(params) = u16_at(data, feature).filter(|p| *p > 0) else {
                continue;
            };
            let Some(id) = u16_at(data, feature + params + 2) else {
                continue;
            };
            let name = face
                .names()
                .into_iter()
                .filter(|n| n.name_id == id as u16)
                .find_map(|n| n.to_string());
            result.insert(tag, name);
        }
    }
    let value: Vec<_> = result.into_iter().collect();
    cache.lock().unwrap().insert(key, value.clone());
    value
}

pub(super) fn ranged_features(
    run: &TypeRun,
    mapping: &[(usize, usize, char)],
    visible_len: usize,
) -> Vec<rustybuzz::Feature> {
    let mut features = ot_features(run);
    features.extend(
        run.features
            .iter()
            .map(|(tag, value)| rustybuzz::Feature::new(Tag::from_bytes(tag), *value, ..)),
    );
    for span in &run.spans {
        let from = mapping
            .iter()
            .find(|(_, ch, _)| *ch >= span.start)
            .map_or(visible_len, |(byte, _, _)| *byte);
        let to = mapping
            .iter()
            .find(|(_, ch, _)| *ch >= span.end)
            .map_or(visible_len, |(byte, _, _)| *byte);
        if from < to {
            features.extend(span.features.iter().map(|(tag, value)| {
                rustybuzz::Feature::new(Tag::from_bytes(tag), *value, from..to)
            }));
        }
    }
    features
}

pub fn feature_css(features: &[([u8; 4], u32)]) -> String {
    features
        .iter()
        .filter(|(tag, _)| tag.iter().all(u8::is_ascii_alphanumeric))
        .map(|(tag, v)| format!("'{}' {v}", String::from_utf8_lossy(tag)))
        .collect::<Vec<_>>()
        .join(",")
}

#[derive(Clone)]
pub struct GlyphAlternate {
    pub tag: [u8; 4],
    pub value: u32,
    pub preview: TypeRun,
}
pub fn glyph_alternates(run: &TypeRun, start: usize, end: usize) -> Vec<GlyphAlternate> {
    if start >= end {
        return vec![];
    }
    let source = &run.content[char_to_byte(&run.content, start)..char_to_byte(&run.content, end)];
    let mut sample = run.clone();
    sample.origin = Pt::ZERO;
    sample.content = source.into();
    sample.wrap_width = None;
    sample.paragraphs.clear();
    sample.spans.clear();
    sample.contours.clear();
    let initial = compose(&sample);
    let ids: Vec<_> = initial
        .iter()
        .flat_map(|l| l.glyphs.iter().map(|g| g.id))
        .collect();
    if source.chars().count() > 1 && ids.len() != 1 {
        return vec![];
    }
    let mut seen = std::collections::HashSet::new();
    seen.insert(ids);
    let mut out = Vec::new();
    for (tag, _) in font_features(&run.font) {
        let tag = tag.to_bytes();
        if !(tag.starts_with(b"ss") || matches!(&tag, b"salt" | b"swsh" | b"aalt")) {
            continue;
        }
        for value in 1..=if tag == *b"aalt" { 8 } else { 1 } {
            let mut preview = sample.clone();
            preview.features.retain(|(t, _)| *t != tag);
            preview.features.push((tag, value));
            let ids: Vec<_> = compose(&preview)
                .iter()
                .flat_map(|l| l.glyphs.iter().map(|g| g.id))
                .collect();
            if seen.insert(ids) {
                out.push(GlyphAlternate {
                    tag,
                    value,
                    preview,
                });
            }
        }
    }
    out
}

#[cfg(test)] mod tests {
    use super::*;
    fn run(text:&str)->TypeRun {TypeRun{content:text.into(),font:concat!(env!("CARGO_MANIFEST_DIR"),"/tests/assets/fonts/EBGaramond.ttf").into(),px:36.,..Default::default()}}
    fn ids(run:&TypeRun)->Vec<u16> {compose(run).iter().flat_map(|l|l.glyphs.iter().map(|g|g.id)).collect()}
    #[test] fn discovers_real_font_features_and_names() {
        let r=run("abc");let features=font_features(&r.font);
        for tag in [*b"smcp",*b"c2sc",*b"ss01",*b"dlig",*b"frac",*b"sups",*b"subs",*b"swsh",*b"onum",*b"lnum",*b"tnum",*b"pnum"] {assert!(features.iter().any(|(t,_)|t.to_bytes()==tag),"{}",String::from_utf8_lossy(&tag));}
        assert!(!features.iter().any(|(t,_)|t.to_bytes()==*b"ss20"));
    }
    #[test] fn range_substitution_preserves_neighbor_and_utf8_clusters() {
        let mut r=run("éabc");let plain=ids(&r);
        r.set_character_style(1,3,|s|s.features=vec![(*b"smcp",1)]);
        let styled=ids(&r);assert_eq!(plain.len(),styled.len());assert_eq!(plain[0],styled[0]);assert_eq!(plain[3],styled[3]);assert_ne!(plain[1],styled[1]);
        for tag in [*b"ss01",*b"ss02",*b"ss03",*b"ss04",*b"ss05",*b"ss06",*b"ss07",*b"swsh",*b"smcp",*b"c2sc",*b"onum",*b"lnum",*b"tnum",*b"pnum",*b"frac",*b"ordn",*b"sups",*b"subs"] {let mut sample=run("abcdefghijklmnopqrstuvwxyz ABCDEFGHIJKLMNOPQRSTUVWXYZ 0123456789 1/2");sample.set_character_style(0,sample.content.chars().count(),|s|s.features=vec![(tag,1)]);assert!(!shape(&sample).is_empty());let encoded=serde_json::to_string(&sample).unwrap();assert_eq!(serde_json::from_str::<TypeRun>(&encoded).unwrap(),sample);}
    }
    #[test] fn ranged_ligatures_and_edit_inheritance() {
        let mut r=run("fi fi");let count=glyph_count(&r);r.set_character_style(0,2,|s|s.features=vec![(*b"liga",0)]);assert_eq!(glyph_count(&r),count+1);
        r.replace_text(1,1,"é",None);assert_eq!(feature_value(&r,1,*b"liga"),0);assert_eq!(feature_value(&r,4,*b"liga"),1);
        r.replace_text(0,3,"",None);assert!(r.spans.is_empty());
    }
    #[test] fn html_exports_range_features() {
        let mut r=run("one TWO");r.set_character_style(4,7,|s|s.features=vec![(*b"smcp",1)]);
        let html=paragraph_html(&r);assert!(html.contains("'smcp' 1"));assert!(html.contains("'smcp' 0"));assert!(html.contains("TWO</span>"));
    }
}

#[cfg(test)] mod editing_tests {
    use super::*;
    use crate::app::Studio;
    use eframe::egui::{self,Event};
    #[test] fn pending_features_clipboard_and_history_preserve_ranges() {
        let mut studio=Studio::new();studio.show_welcome=false;studio.active_layer=Some(1);
        studio.text_font=concat!(env!("CARGO_MANIFEST_DIR"),"/tests/assets/fonts/EBGaramond.ttf").into();
        studio.place_text(Pt::new(20.,60.));studio.type_insert("fi fi");
        {let edit=studio.type_edit.as_mut().unwrap();edit.anchor=0;edit.caret=2;}
        studio.patch_feature(*b"liga",0);
        let ctx=egui::Context::default();
        let _=ctx.run_ui(egui::RawInput{events:vec![Event::Copy],..Default::default()},|ui|studio.handle_shortcuts(ui.ctx()));
        {let edit=studio.type_edit.as_mut().unwrap();edit.anchor=5;edit.caret=5;}
        let _=ctx.run_ui(egui::RawInput{events:vec![Event::Paste("fi".into())],..Default::default()},|ui|studio.handle_shortcuts(ui.ctx()));
        assert_eq!(feature_value(&studio.selected_type().unwrap(),5,*b"liga"),0);
        studio.patch_feature(*b"smcp",1);studio.type_insert("a");
        let run=studio.selected_type().unwrap();assert_eq!(feature_value(&run,7,*b"smcp"),1);assert_eq!(feature_value(&run,6,*b"smcp"),0);
        studio.commit_type_edit();studio.undo();studio.redo();
        assert_eq!(studio.selected_type().unwrap().spans,run.spans);
        let encoded=crate::project::encode(&studio.doc).unwrap();assert!(encoded.contains("\"version\":11"));
        let decoded=crate::project::decode(&encoded).unwrap();let restored=decoded.layers[1].kind.shapes().unwrap().iter().find_map(|s|if let crate::geom::Geom::Text(t)=&s.geom{Some(t)}else{None}).unwrap();assert_eq!(restored.spans,run.spans);
    }
}
