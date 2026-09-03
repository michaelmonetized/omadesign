//! `.oma` project files: JSON with PNG-packed rasters.

use crate::document::{Document, Pixels};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const VERSION: u32 = 3;

#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    doc: Document,
}

pub fn encode(doc: &Document) -> Result<String, String> {
    let mut packed = doc.clone();
    for layer in &mut packed.layers {
        if let Some(px) = layer.kind.pixels_mut() {
            *px = compress_pixels(px)?;
        }
        if let Some(mask) = layer.mask.as_mut() {
            *mask = compress_pixels(mask)?;
        }
    }
    serde_json::to_string(&File {
        version: VERSION,
        doc: packed,
    })
    .map_err(|e| e.to_string())
}

pub fn decode(s: &str) -> Result<Document, String> {
    let file: File = serde_json::from_str(s).map_err(|e| e.to_string())?;
    let mut doc = file.doc;
    for layer in &mut doc.layers {
        if let Some(px) = layer.kind.pixels_mut() {
            *px = decompress_pixels(px)?;
        }
        if let Some(mask) = layer.mask.as_mut() {
            *mask = decompress_pixels(mask)?;
        }
        if let Some(shapes) = layer.kind.shapes_mut() {
            for s in shapes {
                crate::text::fill_contours(&mut s.geom);
            }
        }
    }
    doc.ensure_ids();
    Ok(doc)
}

fn compress_pixels(px: &Pixels) -> Result<Pixels, String> {
    let pm = px.to_pixmap().ok_or("pixmap")?;
    let png = pm.encode_png().map_err(|e| e.to_string())?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(png);
    Ok(Pixels {
        w: px.w,
        h: px.h,
        data: b64.into_bytes(),
        version: px.version,
        cached_pm: std::cell::RefCell::new(None),
    })
}

fn decompress_pixels(px: &Pixels) -> Result<Pixels, String> {
    // Already raw?
    if px.data.len() == px.w as usize * px.h as usize * 4 {
        return Ok(px.clone());
    }
    let b64 = std::str::from_utf8(&px.data).map_err(|e| e.to_string())?;
    let png = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| e.to_string())?;
    let img = image::load_from_memory(&png).map_err(|e| e.to_string())?;
    let rgba = img.to_rgba8();
    Pixels::from_rgba(rgba.width(), rgba.height(), rgba.into_raw()).ok_or_else(|| "size".into())
}

pub fn save_to(doc: &Document, path: &Path) -> Result<(), String> {
    let s = encode(doc)?;
    std::fs::write(path, s).map_err(|e| e.to_string())
}

pub fn load_from(path: &Path) -> Result<Document, String> {
    let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    decode(&s)
}

pub fn dialog_save(name: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("omadesign", &["oma"])
        .set_file_name(if name.ends_with(".oma") {
            name.to_string()
        } else {
            format!("{name}.oma")
        })
        .save_file()
}

const PLACE_EXTS: &[&str] = &[
    "oma", "svg", "svgz", "png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "pdf", "ai",
    "eps", "psd", "psb", "afdesign",
];

pub fn dialog_open() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("All supported", PLACE_EXTS)
        .add_filter("omadesign", &["oma"])
        .add_filter(
            "Images",
            &["png", "jpg", "jpeg", "webp", "gif", "tif", "tiff", "bmp", "psd"],
        )
        .add_filter("Vector", &["svg", "svgz", "pdf", "ai", "eps"])
        .pick_file()
}

pub fn dialog_place() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Place", PLACE_EXTS)
        .add_filter(
            "Images",
            &["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "psd"],
        )
        .add_filter("Vector", &["svg", "svgz", "pdf", "ai", "eps"])
        .pick_file()
}

pub fn dialog_export(kind: &str, ext: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(kind, &[ext])
        .set_file_name(format!("export.{ext}"))
        .save_file()
}

pub fn dialog_folder() -> Option<PathBuf> {
    rfd::FileDialog::new().pick_folder()
}

fn recents_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return PathBuf::from(format!("{xdg}/omadesign/recent.json"));
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(format!("{home}/.config/omadesign/recent.json"))
}

pub fn load_recents() -> Vec<PathBuf> {
    load_recents_raw()
        .into_iter()
        .filter(|p| p.exists())
        .take(12)
        .collect()
}

pub fn load_recents_all() -> Vec<PathBuf> {
    load_recents_raw()
}

pub fn push_recent(path: &Path) {
    let mut v = load_recents_raw();
    v.retain(|p| p != path);
    v.insert(0, path.to_path_buf());
    v.truncate(24);
    write_recents(&v);
}

pub fn remove_recent(path: &Path) {
    let mut v = load_recents_raw();
    v.retain(|p| p != path);
    write_recents(&v);
}

fn load_recents_raw() -> Vec<PathBuf> {
    let Ok(s) = std::fs::read_to_string(recents_path()) else {
        return vec![];
    };
    serde_json::from_str::<Vec<String>>(&s)
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .collect()
}

fn write_recents(v: &[PathBuf]) {
    let strings: Vec<String> = v
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    if let Some(dir) = recents_path().parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(s) = serde_json::to_string_pretty(&strings) {
        let _ = std::fs::write(recents_path(), s);
    }
}

pub fn data_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME")
        && !xdg.is_empty()
    {
        return PathBuf::from(format!("{xdg}/omadesign"));
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(format!("{home}/.local/share/omadesign"))
}

pub fn swap_dir() -> PathBuf {
    data_dir()
}

pub fn new_swap_id() -> String {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let r = crate::document::next_id();
    format!("{t:016x}{r:016x}")
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SwapMeta {
    pub id: String,
    pub original: Option<PathBuf>,
    pub name: String,
    pub saved_at: u64,
    pub doc: crate::document::Document,
}

pub fn swap_path(id: &str) -> PathBuf {
    swap_dir().join(format!("{id}.oma.swp"))
}

pub fn write_swap(meta: &SwapMeta) -> Result<PathBuf, String> {
    let dir = swap_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = swap_path(&meta.id);
    let mut packed = meta.clone();
    for layer in &mut packed.doc.layers {
        if let Some(px) = layer.kind.pixels_mut() {
            *px = compress_pixels(px)?;
        }
        if let Some(mask) = layer.mask.as_mut() {
            *mask = compress_pixels(mask)?;
        }
    }
    let s = serde_json::to_string(&packed).map_err(|e| e.to_string())?;
    std::fs::write(&path, s).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn load_swap(path: &Path) -> Result<SwapMeta, String> {
    let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut meta: SwapMeta = serde_json::from_str(&s).map_err(|e| e.to_string())?;
    for layer in &mut meta.doc.layers {
        if let Some(px) = layer.kind.pixels_mut() {
            *px = decompress_pixels(px)?;
        }
        if let Some(mask) = layer.mask.as_mut() {
            *mask = decompress_pixels(mask)?;
        }
        if let Some(shapes) = layer.kind.shapes_mut() {
            for sh in shapes {
                crate::text::fill_contours(&mut sh.geom);
            }
        }
    }
    meta.doc.ensure_ids();
    Ok(meta)
}

pub fn delete_swap(id: &str) {
    let _ = std::fs::remove_file(swap_path(id));
}

pub fn list_swaps() -> Vec<(PathBuf, SwapMeta)> {
    let Ok(read) = std::fs::read_dir(swap_dir()) else {
        return vec![];
    };
    let mut out = vec![];
    for e in read.flatten() {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("swp") {
            continue;
        }
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !name.ends_with(".oma.swp") {
            continue;
        }
        if let Ok(meta) = load_swap(&p) {
            out.push((p, meta));
        }
    }
    out.sort_by(|a, b| b.1.saved_at.cmp(&a.1.saved_at));
    out
}

fn font_recents_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return PathBuf::from(format!("{xdg}/omadesign/font-recents.json"));
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(format!("{home}/.config/omadesign/font-recents.json"))
}

pub fn load_font_recents() -> Vec<String> {
    let Ok(s) = std::fs::read_to_string(font_recents_path()) else {
        return vec![];
    };
    serde_json::from_str(&s).unwrap_or_default()
}

pub fn push_font_recent(path: &str) {
    if path.is_empty() {
        return;
    }
    let mut v = load_font_recents();
    v.retain(|p| p != path);
    v.insert(0, path.to_string());
    v.truncate(5);
    if let Some(dir) = font_recents_path().parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(s) = serde_json::to_string_pretty(&v) {
        let _ = std::fs::write(font_recents_path(), s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{apply, Cmd, Shape, Style};
    use crate::geom::{Geom, Pt};

    #[test]
    fn roundtrip_vector() {
        let mut doc = Document::new("logo", 320.0, 200.0, 72.0);
        apply(
            &mut doc,
            &Cmd::AddShape {
                layer: 1,
                shape: Shape::new(
                    Geom::Ellipse {
                        center: Pt::new(80.0, 80.0),
                        radii: Pt::new(40.0, 30.0),
                    },
                    Style::default(),
                ),
            },
        );
        let s = encode(&doc).unwrap();
        let back = decode(&s).unwrap();
        assert_eq!(back.width, 320.0);
        assert_eq!(back.layers[1].kind.shapes().unwrap().len(), 1);
    }
}
