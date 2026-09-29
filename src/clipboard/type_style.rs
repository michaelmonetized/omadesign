//! Rich text travels with the native clipboard ownership, alongside ordinary text.
//! Matching plain text alone is never evidence that formatting belongs to it.
use crate::geom::{CharSpan, TypeRun};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MIME: &str = "application/x-omadesign-text+json";
const MARKER: &str = "<!--omadesign-rich-text:";
const COMMON: [[u8; 4]; 7] = [
    *b"kern", *b"liga", *b"clig", *b"tnum", *b"smcp", *b"c2sc", *b"calt",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RichText {
    pub text: String,
    pub spans: Vec<CharSpan>,
    #[serde(default)]
    pub manual_kern: BTreeMap<usize, f32>,
}
impl RichText {
    pub fn from_run(run: &TypeRun, start: usize, end: usize) -> Self {
        let count = run.content.chars().count();
        let start = start.min(count);
        let end = end.max(start).min(count);
        let mut defaults: BTreeMap<_, _> = COMMON
            .into_iter()
            .map(|tag| {
                (
                    tag,
                    match &tag {
                        b"kern" => u32::from(run.kern),
                        b"liga" | b"clig" => u32::from(run.liga),
                        b"tnum" => u32::from(run.tnum),
                        b"smcp" | b"c2sc" => u32::from(run.smcp),
                        b"calt" => 1,
                        _ => 0,
                    },
                )
            })
            .collect();
        defaults.extend(run.features.iter().copied());
        // Include zero values for features used in another source range so they
        // cannot leak through inherited destination defaults after pasting.
        for span in &run.spans {
            for &(tag, _) in &span.features {
                defaults.entry(tag).or_insert(0);
            }
        }
        let base = crate::text::character_metrics(run, usize::MAX);
        let spans = run
            .character_style_runs(start, end)
            .into_iter()
            .map(|span| {
                let mut features = defaults.clone();
                features.extend(span.features.iter().copied());
                let mut style = span.clone();
                style.tracking = style.tracking.or(base.tracking);
                style.kerning = style.kerning.or(base.kerning);
                style.leading = style.leading.or(base.leading);
                style.baseline_shift = style.baseline_shift.or(base.baseline_shift);
                style.hscale = style.hscale.or(base.hscale);
                style.vscale = style.vscale.or(base.vscale);
                style.features = features.into_iter().collect();
                style.start = span.start - start;
                style.end = span.end - start;
                style
            })
            .collect();
        Self {
            text: run.content[crate::text::char_to_byte(&run.content, start)
                ..crate::text::char_to_byte(&run.content, end)]
                .into(),
            spans,
            manual_kern: run
                .manual_kern
                .range((start + 1)..end.max(start + 1))
                .map(|(&at, &value)| (at - start, value))
                .collect(),
        }
    }
    pub fn apply_styles(&self, run: &mut TypeRun, start: usize) {
        let end = start + self.text.chars().count();
        let tags: BTreeSet<_> = run
            .features
            .iter()
            .chain(run.spans.iter().flat_map(|s| &s.features))
            .map(|(tag, _)| *tag)
            .collect();
        let spans = self
            .spans
            .iter()
            .map(|span| {
                let mut span = span.clone();
                for &tag in &tags {
                    if !span.features.iter().any(|(t, _)| *t == tag) {
                        span.features.push((tag, 0));
                    }
                }
                span.start += start;
                span.end += start;
                span
            })
            .collect();
        run.replace_character_styles(start, end, spans);
        run.manual_kern.retain(|at, _| *at <= start || *at >= end);
        run.manual_kern.extend(
            self.manual_kern
                .iter()
                .map(|(&at, &value)| (start + at, value)),
        );
    }
    fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > super::MAX_TEXT_BYTES {
            return None;
        }
        let rich: Self = serde_json::from_slice(bytes).ok()?;
        let count = rich.text.chars().count();
        let mut end = 0;
        for span in &rich.spans {
            if span.start != end || span.end <= span.start || span.end > count {
                return None;
            }
            end = span.end;
        }
        if end != count
            || rich
                .manual_kern
                .iter()
                .any(|(&at, &value)| at == 0 || at >= count || !value.is_finite())
        {
            return None;
        }
        Some(rich)
    }
    fn html(&self) -> Result<String, String> {
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        let escaped = self
            .text
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        Ok(format!(
            "{MARKER}{}--><pre>{escaped}</pre>",
            STANDARD.encode(bytes)
        ))
    }
}
fn from_html(html: &str) -> Option<RichText> {
    let payload = html.split_once(MARKER)?.1.split_once("-->")?.0;
    RichText::decode(&STANDARD.decode(payload).ok()?)
}

#[cfg(not(test))]
pub fn write(rich: &RichText) -> Result<(), String> {
    let html = rich.html()?;
    #[cfg(target_os = "linux")]
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        use wl_clipboard_rs::copy::{MimeSource, MimeType, Options, Source};
        let json = serde_json::to_vec(rich).map_err(|e| e.to_string())?;
        if Options::new()
            .copy_multi(vec![
                MimeSource {
                    source: Source::Bytes(rich.text.clone().into_bytes().into()),
                    mime_type: MimeType::Text,
                },
                MimeSource {
                    source: Source::Bytes(json.into()),
                    mime_type: MimeType::Specific(MIME.into()),
                },
                MimeSource {
                    source: Source::Bytes(html.clone().into_bytes().into()),
                    mime_type: MimeType::Specific("text/html".into()),
                },
            ])
            .is_ok()
        {
            return Ok(());
        }
    }
    // Keep the X11 clipboard owner alive. Windows/macOS persist the same HTML
    // plus plain-text pair natively; HTML also carries our typed format there.
    static OWNER: std::sync::OnceLock<std::sync::Mutex<Option<arboard::Clipboard>>> =
        std::sync::OnceLock::new();
    let mut owner = OWNER.get_or_init(Default::default).lock().unwrap();
    if owner.is_none() {
        *owner = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
    }
    owner
        .as_mut()
        .unwrap()
        .set_html(html, Some(rich.text.clone()))
        .map_err(|e| e.to_string())
}

#[derive(Debug)]
pub struct Paste {
    pub text: String,
    pub rich: Option<RichText>,
}
/// Called on a worker. The rich format must be offered by the current owner,
/// and match the native text event when one was delivered by the window system.
#[cfg(not(test))]
pub fn read(event_text: Option<String>) -> Result<Paste, String> {
    let rich = read_rich().ok().flatten();
    let text = if let Some(text) = event_text {
        text
    } else if let Some(rich) = &rich {
        rich.text.clone()
    } else {
        read_plain()?
    };
    let rich = rich.filter(|rich| rich.text == text);
    Ok(Paste { text, rich })
}
#[cfg(not(test))]
fn read_plain() -> Result<String, String> {
    #[cfg(target_os = "linux")]
    {
        const TYPES: [&str; 6] = [
            "text/plain;charset=utf-8",
            "text/plain;charset=UTF-8",
            "UTF8_STRING",
            "text/plain",
            "STRING",
            "TEXT",
        ];
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            use wl_clipboard_rs::paste::{self, ClipboardType, Seat};
            match paste::get_mime_types(ClipboardType::Regular, Seat::Unspecified) {
                Ok(types) => {
                    for mime in TYPES {
                        if types.contains(mime) {
                            return String::from_utf8(super::read_wayland_mime(mime)?)
                                .map_err(|e| e.to_string());
                        }
                    }
                    return Err("The clipboard does not contain text".into());
                }
                Err(
                    paste::Error::ClipboardEmpty | paste::Error::NoMimeType | paste::Error::NoSeats,
                ) => return Err("The clipboard is empty".into()),
                Err(_) => {}
            }
        }
        let context = x11_clipboard::Context::new(None).map_err(|e| e.to_string())?;
        let offered = super::x11_targets(&context)?;
        for mime in TYPES {
            let target = context.get_atom(mime).map_err(|e| e.to_string())?;
            if offered.as_ref().is_some_and(|set| !set.contains(&target)) {
                continue;
            }
            if let Some(bytes) = super::x11_contents(&context, target)? {
                return String::from_utf8(bytes).map_err(|e| e.to_string());
            }
        }
        Err("The clipboard does not contain text".into())
    }
    #[cfg(not(target_os = "linux"))]
    {
        arboard::Clipboard::new()
            .and_then(|mut clipboard| clipboard.get_text())
            .map_err(|e| e.to_string())
    }
}
#[cfg(not(test))]
fn read_rich() -> Result<Option<RichText>, String> {
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            use wl_clipboard_rs::paste::{self, ClipboardType, Seat};
            let types = match paste::get_mime_types(ClipboardType::Regular, Seat::Unspecified) {
                Ok(types) => Some(types),
                Err(
                    paste::Error::ClipboardEmpty | paste::Error::NoMimeType | paste::Error::NoSeats,
                ) => return Ok(None),
                Err(_) => None,
            };
            if let Some(types) = types {
                if types.contains(MIME) {
                    return Ok(RichText::decode(&super::read_wayland_mime(MIME)?));
                }
                if types.contains("text/html") {
                    return Ok(from_html(&String::from_utf8_lossy(
                        &super::read_wayland_mime("text/html")?,
                    )));
                }
                return Ok(None); // A new plain-text owner invalidates all old styles.
            }
        }
        let context = x11_clipboard::Context::new(None).map_err(|e| e.to_string())?;
        let offered = super::x11_targets(&context)?;
        let target = context.get_atom("text/html").map_err(|e| e.to_string())?;
        if offered.as_ref().is_some_and(|set| !set.contains(&target)) {
            return Ok(None);
        }
        Ok(super::x11_contents(&context, target)?
            .and_then(|data| from_html(&String::from_utf8_lossy(&data))))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        Ok(clipboard
            .get()
            .html()
            .ok()
            .and_then(|html| from_html(&html)))
    }
}

// Unit tests use an isolated native-format transport, never the user's live
// clipboard. The same decode and provenance checks are used on its formats.
#[cfg(test)]
thread_local! {static TEST_FORMATS:std::cell::RefCell<(String,Option<String>)>=const {std::cell::RefCell::new((String::new(),None))};}
#[cfg(test)]
pub fn write(rich: &RichText) -> Result<(), String> {
    TEST_FORMATS.with(|slot| *slot.borrow_mut() = (rich.text.clone(), Some(rich.html().unwrap())));
    Ok(())
}
#[cfg(test)]
pub fn test_plain(text: &str) {
    TEST_FORMATS.with(|slot| *slot.borrow_mut() = (text.into(), None));
}
#[cfg(test)]
pub fn read(event_text: Option<String>) -> Result<Paste, String> {
    TEST_FORMATS.with(|slot| {
        let slot = slot.borrow();
        let text = event_text.unwrap_or_else(|| slot.0.clone());
        let rich = slot
            .1
            .as_ref()
            .and_then(|s| from_html(s))
            .filter(|r| r.text == text);
        Ok(Paste { text, rich })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copied_effective_features_override_different_destination_defaults() {
        let source = TypeRun {
            content: "fi AB 12".into(),
            liga: false,
            tnum: true,
            smcp: true,
            features: vec![(*b"ss01", 1), (*b"dlig", 1)],
            tracking: 2.,
            ..Default::default()
        };
        let mut source = source;
        source.set_character_style(3, 5, |s| s.features = vec![(*b"smcp", 0)]);
        let rich = RichText::from_run(&source, 0, 8);
        let mut target = TypeRun {
            content: source.content.clone(),
            liga: true,
            tnum: false,
            smcp: false,
            features: vec![(*b"ss02", 1)],
            ..Default::default()
        };
        rich.apply_styles(&mut target, 0);
        for i in 0..8 {
            for tag in COMMON.into_iter().chain([*b"ss01", *b"ss02", *b"dlig"]) {
                assert_eq!(
                    crate::text::feature_value(&source, i, tag),
                    crate::text::feature_value(&target, i, tag),
                    "{i} {tag:?}"
                );
            }
        }
        assert_eq!(
            crate::text::character_metrics(&source, 0).tracking,
            crate::text::character_metrics(&target, 0).tracking
        );
        let font = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/assets/fonts/EBGaramond.ttf"
        );
        source.font = font.into();
        target.font = font.into();
        let ids = |run: &TypeRun| {
            crate::text::compose(run)
                .iter()
                .flat_map(|line| line.glyphs.iter().map(|g| g.id))
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&source), ids(&target));
    }
    #[test]
    fn clipboard_materializes_metrics_and_reindexes_manual_pairs_once() {
        let mut source = TypeRun {
            content: "zAVz".into(),
            px: 20.,
            tracking: 2.,
            leading: 30.,
            kern: false,
            ..Default::default()
        };
        source.manual_kern.insert(1, 20.);
        source.manual_kern.insert(2, -50.);
        source.manual_kern.insert(3, 70.);
        source.set_character_style(2, 3, |s| {
            s.hscale = Some(125.);
            s.baseline_shift = Some(4.);
        });
        let rich = RichText::from_run(&source, 1, 3);
        assert_eq!(rich.manual_kern, BTreeMap::from([(1, -50.)]));
        let mut target = TypeRun {
            content: "xAVy".into(),
            tracking: 8.,
            leading: 80.,
            kern: true,
            ..Default::default()
        };
        rich.apply_styles(&mut target, 1);
        assert_eq!(target.manual_kern, BTreeMap::from([(2, -50.)]));
        for (from, to) in [(1, 1), (2, 2)] {
            let a = crate::text::character_metrics(&source, from);
            let b = crate::text::character_metrics(&target, to);
            assert_eq!(
                (
                    a.tracking,
                    a.kerning,
                    a.leading,
                    a.baseline_shift,
                    a.hscale,
                    a.vscale
                ),
                (
                    b.tracking,
                    b.kerning,
                    b.leading,
                    b.baseline_shift,
                    b.hscale,
                    b.vscale
                )
            );
        }
    }
    #[test]
    fn uniform_large_clipboard_uses_one_interval_and_batch_overlay_preserves_neighbors() {
        let source = TypeRun {
            content: "a".repeat(100_000),
            features: vec![(*b"smcp", 1)],
            ..Default::default()
        };
        let rich = RichText::from_run(&source, 0, 100_000);
        assert_eq!(rich.spans.len(), 1);
        let mut target = TypeRun {
            content: format!("x{}y", source.content),
            ..Default::default()
        };
        target.set_character_style(0, 100_002, |s| s.features = vec![(*b"tnum", 1)]);
        rich.apply_styles(&mut target, 1);
        assert_eq!(target.spans.len(), 3);
        assert_eq!(crate::text::feature_value(&target, 0, *b"tnum"), 1);
        assert_eq!(crate::text::feature_value(&target, 1, *b"tnum"), 0);
        assert_eq!(crate::text::feature_value(&target, 100_001, *b"tnum"), 1);
        target.replace_text(50_000, 50_001, "é", None);
        assert_eq!(target.spans.len(), 3);
    }
    #[test]
    fn identical_external_plain_text_cannot_reuse_rich_provenance() {
        let source = TypeRun {
            content: "identical".into(),
            smcp: true,
            ..Default::default()
        };
        let rich = RichText::from_run(&source, 0, 9);
        write(&rich).unwrap();
        assert!(read(Some("identical".into())).unwrap().rich.is_some());
        test_plain("identical");
        let plain = read(Some("identical".into())).unwrap();
        assert_eq!(plain.text, "identical");
        assert!(plain.rich.is_none());
        assert!(from_html("<p>identical</p>").is_none());
    }
    #[test]
    fn native_html_round_trip_keeps_unicode_and_rejects_invalid_intervals() {
        let source = TypeRun {
            content: "é <fi> 🦊".into(),
            smcp: true,
            ..Default::default()
        };
        let rich = RichText::from_run(&source, 0, source.content.chars().count());
        assert_eq!(from_html(&rich.html().unwrap()), Some(rich.clone()));
        let mut bad = rich;
        bad.spans[0].end = usize::MAX;
        assert!(RichText::decode(&serde_json::to_vec(&bad).unwrap()).is_none());
    }
}
