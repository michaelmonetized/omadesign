//! Persistent prompt attachments. All filesystem reads and encoding run on workers.
use crate::clipboard::{ClipboardContent, agent::Content};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::mpsc,
};

pub const MAX_ATTACHMENTS: usize = 20;
pub const MAX_FILE: u64 = 100 * 1024 * 1024;
pub const MAX_IMAGE: usize = 20 * 1024 * 1024;
pub const INLINE_CAP: usize = 5 * 1024 * 1024;
pub const TURN_BASE64_CAP: usize = 25 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Kind {
    Text,
    Image,
    Svg,
    Video,
    Audio,
    File,
    Objects,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub kind: Kind,
    pub mime: String,
    pub name: String,
    pub size: u64,
    pub source: PathBuf,
    #[serde(default)]
    pub dimensions: Option<[u32; 2]>,
    #[serde(default)]
    pub delivery: String,
    #[serde(default)]
    pub preview: Option<PathBuf>,
    #[serde(skip)]
    pub thumbnail: Option<crate::photo::RgbaImage>,
}
impl Attachment {
    pub fn token(&self) -> String {
        format!("[📎 {} · {}]", self.name, self.id)
    }
    pub fn available(&self) -> bool {
        self.source.is_file()
    }
    pub fn tooltip(&self) -> String {
        format!(
            "{}\n{} · {} bytes{}\n{}",
            self.source.display(),
            self.mime,
            self.size,
            self.dimensions
                .map(|[w, h]| format!(" · {w}×{h}"))
                .unwrap_or_default(),
            self.delivery
        )
    }
}
pub enum Input {
    Clipboard(Option<String>),
    Files(Vec<PathBuf>),
    Native(ClipboardContent),
}
pub struct Insert {
    pub at: usize,
    pub end: usize,
    pub text: String,
    pub attachments: Vec<Attachment>,
}
pub struct Job {
    pub thread: String,
    pub at: usize,
    pub end: usize,
    pub replaced: String,
    pub rx: mpsc::Receiver<Result<Prepared, String>>,
}
pub struct Prepared {
    pub text: String,
    pub attachments: Vec<Attachment>,
}
pub struct Turn {
    pub request: String,
    pub attachments: Vec<Attachment>,
    pub blocks: Vec<Value>,
}
pub struct TurnJob {
    pub rx: mpsc::Receiver<Result<Turn, String>>,
    pub request: String,
    pub attachments: Vec<Attachment>,
}

pub fn spawn(
    input: Input,
    directory: PathBuf,
    thread: String,
    range: std::ops::Range<usize>,
    replaced: String,
    ctx: eframe::egui::Context,
) -> Job {
    let (tx, rx) = mpsc::channel();
    let owner = thread.clone();
    std::thread::spawn(move || {
        let result = ingest(input, &directory);
        let _ = tx.send(result);
        ctx.request_repaint();
    });
    Job {
        thread: owner,
        at: range.start,
        end: range.end,
        replaced,
        rx,
    }
}
fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("Attachments must be regular files".into());
    }
    if meta.len() > limit {
        return Err(format!(
            "{} exceeds the {} MiB attachment limit",
            path.display(),
            limit / 1024 / 1024
        ));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("Attachment grew beyond its size limit".into());
    }
    Ok(bytes)
}
fn identity() -> String {
    crate::project::new_swap_id()
}
fn metadata(path: PathBuf, mime: String, kind: Kind) -> Result<Attachment, String> {
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("Attachments must be regular files".into());
    }
    if meta.len() > MAX_FILE {
        return Err("Attachment exceeds 100 MiB; choose a smaller file".into());
    }
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '[' | ']'))
        .take(100)
        .collect();
    Ok(Attachment {
        id: {
            use std::hash::{Hash, Hasher};
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            identity().hash(&mut hash);
            format!("{:016x}", hash.finish())
        },
        kind,
        mime,
        name,
        size: meta.len(),
        source: path,
        dimensions: None,
        delivery: String::new(),
        preview: None,
        thumbnail: None,
    })
}
fn stored(
    dir: &Path,
    name: &str,
    mime: &str,
    kind: Kind,
    bytes: &[u8],
) -> Result<Attachment, String> {
    if bytes.len() as u64 > MAX_FILE {
        return Err("Attachment exceeds 100 MiB".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("Could not save attachment: {e}"))?;
    let path = dir.join(format!("{}-{name}", identity()));
    std::fs::write(&path, bytes).map_err(|e| format!("Could not save attachment: {e}"))?;
    let mut a = metadata(path, mime.into(), kind)?;
    a.name = name.into();
    Ok(a)
}
fn image(dir: &Path, name: &str, image: crate::photo::RgbaImage) -> Result<Attachment, String> {
    if u64::from(image.w) * u64::from(image.h) > 64_000_000 {
        return Err("Clipboard image exceeds 64 megapixels".into());
    }
    let scaled = image.downscaled(2048);
    let png = scaled
        .encode_png()
        .ok_or("Could not encode attachment image")?;
    if png.len() > MAX_IMAGE {
        return Err("Image attachment exceeds 20 MiB after encoding".into());
    }
    let mut a = stored(dir, name, "image/png", Kind::Image, &png)?;
    a.dimensions = Some([scaled.w, scaled.h]);
    a.thumbnail = Some(scaled.downscaled(48));
    Ok(a)
}
fn classify(path: &Path) -> (Kind, &'static str) {
    match path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "svg" => (Kind::Svg, "image/svg+xml"),
        "png" => (Kind::Image, "image/png"),
        "jpg" | "jpeg" => (Kind::Image, "image/jpeg"),
        "webp" => (Kind::Image, "image/webp"),
        "gif" => (Kind::Image, "image/gif"),
        "tiff" | "tif" => (Kind::Image, "image/tiff"),
        "bmp" => (Kind::Image, "image/bmp"),
        "mp4" | "m4v" => (Kind::Video, "video/mp4"),
        "mov" => (Kind::Video, "video/quicktime"),
        "webm" => (Kind::Video, "video/webm"),
        "mkv" => (Kind::Video, "video/x-matroska"),
        "mp3" => (Kind::Audio, "audio/mpeg"),
        "wav" => (Kind::Audio, "audio/wav"),
        "ogg" => (Kind::Audio, "audio/ogg"),
        "flac" => (Kind::Audio, "audio/flac"),
        "m4a" => (Kind::Audio, "audio/mp4"),
        "aac" => (Kind::Audio, "audio/aac"),
        "opus" => (Kind::Audio, "audio/ogg"),
        "aif" | "aiff" => (Kind::Audio, "audio/aiff"),
        "avi" => (Kind::Video, "video/x-msvideo"),
        "pdf" => (Kind::File, "application/pdf"),
        "txt" | "log" | "md" | "csv" => (Kind::Text, "text/plain"),
        "json" => (Kind::Text, "application/json"),
        _ => (Kind::File, "application/octet-stream"),
    }
}
fn native(content: ClipboardContent, dir: &Path) -> Result<Prepared, String> {
    let mut attachments = vec![];
    match content {
        ClipboardContent::Empty => {
            return Err("The clipboard contains no attachment or text".into());
        }
        ClipboardContent::Text(text)
            if text.starts_with("omadesign-objects:") || text.starts_with("omadesign-shapes:") =>
        {
            let payload = text.split_once(':').unwrap().1;
            let doc = if text.starts_with("omadesign-objects:") {
                crate::project::decode(payload)?
            } else {
                let shapes: Vec<crate::document::Shape> = serde_json::from_str(payload)
                    .map_err(|e| format!("Invalid copied objects: {e}"))?;
                let mut doc = crate::document::Document::new("Copied objects", 1024., 768., 96.);
                doc.artboards.clear();
                doc.layers = vec![crate::document::Layer::vector("Objects")];
                *doc.layers[0].kind.shapes_mut().unwrap() = shapes;
                doc
            };
            let mut a = stored(
                dir,
                "copied-objects.oma",
                "application/json",
                Kind::Objects,
                crate::project::encode(&doc)?.as_bytes(),
            )?;
            if doc.width > 0. && doc.height > 0. {
                let scale = (2048. / doc.width.max(doc.height)).min(1.);
                if let Some(pm) = crate::compositor::render_view(
                    &doc,
                    crate::compositor::View {
                        scale,
                        offset: crate::geom::Pt::ZERO,
                    },
                    (doc.width * scale).ceil().max(1.) as u32,
                    (doc.height * scale).ceil().max(1.) as u32,
                    crate::compositor::Draft::none(),
                ) && let Ok(png) = pm.encode_png()
                {
                    let preview = dir.join(format!("{}-objects.png", a.id));
                    std::fs::write(&preview, png).map_err(|e| e.to_string())?;
                    a.preview = Some(preview);
                    a.dimensions = Some([pm.width(), pm.height()]);
                }
            }
            attachments.push(a);
        }
        ClipboardContent::Text(text) => {
            if text.chars().count() <= 2000 && text.lines().count() <= 40 {
                return Ok(Prepared { text, attachments });
            }
            if text.len() > 16 * 1024 * 1024 {
                return Err("Clipboard text exceeds 16 MiB".into());
            }
            attachments.push(stored(
                dir,
                "pasted-text.txt",
                "text/plain",
                Kind::Text,
                text.as_bytes(),
            )?);
        }
        ClipboardContent::Svg(text) => {
            if text.len() > 16 * 1024 * 1024 {
                return Err("SVG exceeds 16 MiB".into());
            }
            attachments.push(stored(
                dir,
                "pasted-vector.svg",
                "image/svg+xml",
                Kind::Svg,
                text.as_bytes(),
            )?);
        }
        ClipboardContent::Image { name: _, image: im } => {
            attachments.push(image(dir, "pasted-image.png", im)?)
        }
        ClipboardContent::Files(files) => {
            if files.len() > MAX_ATTACHMENTS {
                return Err("A prompt can contain at most 20 attachments".into());
            }
            for path in files {
                let path = path
                    .canonicalize()
                    .map_err(|e| format!("Attachment: {e}"))?;
                let (kind, mime) = classify(&path);
                let mut a = metadata(path, mime.into(), kind)?;
                if a.kind == Kind::Image {
                    if let ClipboardContent::Image { image: im, .. } =
                        crate::clipboard::read_file(&a.source)?
                    {
                        let preview = image(dir, "image-preview.png", im)?;
                        a.preview = Some(preview.source);
                        a.dimensions = preview.dimensions;
                        a.thumbnail = preview.thumbnail;
                    }
                }
                attachments.push(a);
            }
        }
    }
    let text = attachments
        .iter()
        .map(Attachment::token)
        .collect::<Vec<_>>()
        .join(" ");
    Ok(Prepared { text, attachments })
}
pub fn ingest(input: Input, dir: &Path) -> Result<Prepared, String> {
    match input {
        Input::Files(files) => native(ClipboardContent::Files(files), dir),
        Input::Native(c) => native(c, dir),
        Input::Clipboard(fallback) => {
            // Prefer the richest native format; text events are a fallback for
            // clipboard owners or platforms that cannot serve native bytes.
            let content = match crate::clipboard::agent::read() {
                Ok(Content::Native(ClipboardContent::Empty)) | Err(_)
                    if fallback.as_ref().is_some_and(|t| !t.is_empty()) =>
                {
                    Content::Native(crate::clipboard::agent::parse_text(
                        fallback.as_ref().unwrap(),
                    )?)
                }
                result => result?,
            };
            match content {
                Content::Native(c) => native(c, dir),
                Content::Raw { mime, bytes } => {
                    let (kind, name) = if mime.starts_with("audio/") {
                        (Kind::Audio, "pasted-audio")
                    } else if mime.starts_with("video/") {
                        (Kind::Video, "pasted-video")
                    } else if mime == "application/pdf" {
                        (Kind::File, "pasted-document.pdf")
                    } else {
                        (Kind::File, "pasted-file")
                    };
                    let a = stored(dir, name, &mime, kind, &bytes)?;
                    Ok(Prepared {
                        text: a.token(),
                        attachments: vec![a],
                    })
                }
            }
        }
    }
}
pub fn typed_len(request: &str, attachments: &[Attachment]) -> usize {
    attachments
        .iter()
        .fold(request.to_owned(), |text, a| text.replace(&a.token(), ""))
        .len()
}
pub fn remove(request: &mut String, attachments: &mut Vec<Attachment>, id: &str) {
    if let Some(a) = attachments.iter().find(|a| a.id == id) {
        *request = request.replace(&a.token(), "");
    }
    attachments.retain(|a| a.id != id);
}
pub fn reconcile(request: &str, attachments: &mut Vec<Attachment>) {
    attachments.retain(|a| request.contains(&a.token()));
}

pub fn payload(
    request: String,
    mut attachments: Vec<Attachment>,
    capabilities: &Value,
) -> Result<Turn, String> {
    if attachments.len() > MAX_ATTACHMENTS {
        return Err("A prompt can contain at most 20 attachments".into());
    }
    let mut blocks = vec![];
    let mut encoded = 0;
    for a in &mut attachments {
        let meta = std::fs::metadata(&a.source).map_err(|_| {
            format!(
                "{} is unavailable; remove it and attach the file again",
                a.name
            )
        })?;
        if !meta.is_file() || meta.len() > MAX_FILE {
            return Err(format!("{} exceeds the 100 MiB file limit", a.name));
        }
        let uri = url::Url::from_file_path(&a.source)
            .map_err(|_| "Attachment path is not absolute")?
            .to_string();
        let image = capabilities["image"].as_bool() == Some(true) && a.kind == Kind::Image;
        let audio = capabilities["audio"].as_bool() == Some(true) && a.kind == Kind::Audio;
        let embedded = capabilities["embeddedContext"].as_bool() == Some(true)
            && !matches!(a.kind, Kind::Video | Kind::Image | Kind::Audio)
            && meta.len() <= INLINE_CAP as u64;
        let source = if image {
            a.preview.as_ref().unwrap_or(&a.source)
        } else {
            &a.source
        };
        let source_size = std::fs::metadata(source).map_err(|e| e.to_string())?.len();
        let limit = if image { MAX_IMAGE } else { INLINE_CAP };
        let estimate = ((source_size + 2) / 3 * 4) as usize;
        let inline = (image || audio || embedded)
            && source_size <= limit as u64
            && encoded + estimate <= TURN_BASE64_CAP;
        if inline {
            let bytes = read_bounded(source, limit as u64)?;
            if image || audio {
                let data = base64::engine::general_purpose::STANDARD.encode(bytes);
                encoded += data.len();
                blocks.push(json!({"type":if image{"image"}else{"audio"},"mimeType":if image{"image/png"}else{&a.mime},"data":data}));
                a.delivery = if image {
                    "Sent as an image"
                } else {
                    "Sent as audio"
                }
                .into();
            } else {
                let resource = if let Ok(text) = String::from_utf8(bytes.clone()) {
                    json!({"uri":uri,"mimeType":a.mime,"text":text})
                } else {
                    let blob = base64::engine::general_purpose::STANDARD.encode(bytes);
                    encoded += blob.len();
                    json!({"uri":uri,"mimeType":a.mime,"blob":blob})
                };
                blocks.push(json!({"type":"resource","resource":resource}));
                a.delivery = "Sent as embedded context".into();
            }
        } else {
            blocks.push(json!({"type":"resource_link","uri":uri,"name":a.name,"mimeType":a.mime,"size":meta.len()}));
            a.delivery =
                "Sent as a path; this agent cannot view this type or the inline limit was reached"
                    .into();
        }
        let dims = a
            .dimensions
            .map(|[w, h]| format!(", {w}×{h}"))
            .unwrap_or_default();
        blocks.push(json!({"type":"text","text":format!("{}: {} ({}, {} bytes{}). {}. Read-only access to this user attachment is allowed.",a.token(),a.source.display(),a.mime,meta.len(),dims,a.delivery)}));
        if a.kind == Kind::Objects {
            let bytes = read_bounded(&a.source, MAX_FILE)?;
            let summary = serde_json::from_slice::<Value>(&bytes)
                .map(|v| {
                    format!(
                        "Copied Omadesign objects: {}",
                        v.to_string().chars().take(4000).collect::<String>()
                    )
                })
                .unwrap_or_else(|_| "Copied Omadesign native objects".into());
            blocks.push(json!({"type":"text","text":summary}));
            if capabilities["image"].as_bool() == Some(true)
                && let Some(path) = &a.preview
            {
                let bytes = read_bounded(path, MAX_IMAGE as u64)?;
                let data = base64::engine::general_purpose::STANDARD.encode(bytes);
                if encoded + data.len() <= TURN_BASE64_CAP {
                    encoded += data.len();
                    blocks.push(json!({"type":"image","mimeType":"image/png","data":data}));
                }
            }
        }
    }
    Ok(Turn {
        request,
        attachments,
        blocks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dir() -> PathBuf {
        let p = std::env::temp_dir().join(format!("omadesign-attachments-{}", identity()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn attachment_ingest_preserves_short_text_and_external_paths_and_bounds_resources() {
        let dir = dir();
        let short = native(ClipboardContent::Text("Hello — 你好\n".into()), &dir).unwrap();
        assert_eq!(short.text, "Hello — 你好\n");
        assert!(short.attachments.is_empty());
        for text in ["x".repeat(2001), "log\n".repeat(41)] {
            let prepared = native(ClipboardContent::Text(text.clone()), &dir).unwrap();
            assert_eq!(prepared.attachments.len(), 1);
            assert!(!prepared.text.contains(&text));
            assert_eq!(
                std::fs::read_to_string(&prepared.attachments[0].source).unwrap(),
                text
            );
        }
        let mut files = vec![];
        for name in ["brand.pdf", "clip.mp4", "sound.mp3", "strange.bin"] {
            let path = dir.join(name);
            std::fs::write(&path, b"fixture").unwrap();
            files.push(path);
        }
        let prepared = ingest(Input::Files(files.clone()), &dir).unwrap();
        assert_eq!(
            prepared
                .attachments
                .iter()
                .map(|a| &a.id)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            4,
            "Files pasted together need distinct identities"
        );
        assert_eq!(
            prepared
                .attachments
                .iter()
                .map(|a| a.source.clone())
                .collect::<Vec<_>>(),
            files
        );
        assert_eq!(
            prepared
                .attachments
                .iter()
                .map(|a| a.kind.clone())
                .collect::<Vec<_>>(),
            [Kind::File, Kind::Video, Kind::Audio, Kind::File]
        );
        let big = dir.join("large.bin");
        std::fs::File::create(&big)
            .unwrap()
            .set_len(MAX_FILE + 1)
            .unwrap();
        assert!(
            ingest(Input::Files(vec![big]), &dir)
                .err()
                .unwrap()
                .contains("100 MiB")
        );
        assert!(
            ingest(Input::Files(vec![files[0].clone(); 21]), &dir)
                .err()
                .unwrap()
                .contains("20 attachments")
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn attachment_images_svg_objects_and_history_roundtrip() {
        let dir = dir();
        let image = crate::photo::RgbaImage::new(4096, 2, vec![255; 4096 * 2 * 4]).unwrap();
        let prepared = native(
            ClipboardContent::Image {
                name: "Screenshot".into(),
                image,
            },
            &dir,
        )
        .unwrap();
        let a = &prepared.attachments[0];
        assert_eq!(a.dimensions, Some([2048, 1]));
        assert!(a.thumbnail.is_some());
        let svg = native(
            ClipboardContent::Svg("<svg xmlns=\"http://www.w3.org/2000/svg\"/>".into()),
            &dir,
        )
        .unwrap();
        assert_eq!(svg.attachments[0].kind, Kind::Svg);
        let doc = crate::document::Document::new("Copy", 64., 48., 96.);
        let text = format!(
            "omadesign-objects:{}",
            crate::project::encode(&doc).unwrap()
        );
        let objects = native(ClipboardContent::Text(text), &dir).unwrap();
        assert_eq!(objects.attachments[0].kind, Kind::Objects);
        assert!(objects.attachments[0].preview.as_ref().unwrap().is_file());
        let entry = super::super::workspace::Entry {
            role: "user".into(),
            text: prepared.text,
            id: String::new(),
            status: String::new(),
            attachments: prepared.attachments,
        };
        let bytes = serde_json::to_vec(&entry).unwrap();
        assert!(bytes.len() < 4096);
        let loaded: super::super::workspace::Entry = serde_json::from_slice(&bytes).unwrap();
        assert!(loaded.attachments[0].available());
        std::fs::remove_file(&loaded.attachments[0].source).unwrap();
        assert!(!loaded.attachments[0].available());
        let old: super::super::workspace::Entry =
            serde_json::from_str(r#"{"role":"user","text":"Older conversation"}"#).unwrap();
        assert!(old.attachments.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn attachment_payload_enforces_typed_and_inline_caps_and_removal() {
        let dir = dir();
        let a = stored(
            &dir,
            "huge-text.txt",
            "text/plain",
            Kind::Text,
            &vec![b'x'; INLINE_CAP + 1],
        )
        .unwrap();
        let turn = payload(a.token(), vec![a.clone()], &json!({"embeddedContext":true})).unwrap();
        assert_eq!(turn.blocks[0]["type"], "resource_link");
        assert!(
            turn.blocks[1]["text"]
                .as_str()
                .unwrap()
                .contains("Sent as a path")
        );
        let mut request = format!("Palette {} please", a.token());
        let mut items = vec![a.clone()];
        assert_eq!(typed_len(&request, &items), 15);
        remove(&mut request, &mut items, &a.id);
        assert_eq!(request, "Palette  please");
        assert!(items.is_empty());
        let data = vec![7; 4 * 1024 * 1024];
        let mut audio = vec![];
        for i in 0..6 {
            audio.push(stored(&dir, &format!("{i}.wav"), "audio/wav", Kind::Audio, &data).unwrap());
        }
        let turn = payload("audio".into(), audio, &json!({"audio":true})).unwrap();
        let total = turn
            .blocks
            .iter()
            .filter_map(|b| b["data"].as_str())
            .map(str::len)
            .sum::<usize>();
        assert!(total <= TURN_BASE64_CAP);
        assert!(turn.blocks.iter().any(|b| b["type"] == "resource_link"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
