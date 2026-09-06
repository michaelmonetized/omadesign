//! `.oma` project files: JSON with PNG-packed rasters.

use crate::document::{Document, Pixels};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub(crate) const VERSION: u32 = 4;

#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    doc: Document,
}

pub fn encode(doc: &Document) -> Result<String, String> {
    let mut packed = doc.clone();
    pack_rasters(&mut packed)?;
    serde_json::to_string(&File {
        version: VERSION,
        doc: packed,
    })
    .map_err(|e| e.to_string())
}

pub fn decode(s: &str) -> Result<Document, String> {
    let file: File = serde_json::from_str(s).map_err(|e| e.to_string())?;
    if !(1..=VERSION).contains(&file.version) {
        return Err("Unsupported omadesign project version".into());
    }
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
    doc.validate_hierarchy()?;
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
        cached_uniform: Default::default(),
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
    use std::io::Write;
    let s = encode(doc)?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let copied_fonts = crate::typography::archive_document_fonts(doc, parent)?;
    let temporary = parent.join(format!(
        ".oma-save-{}-{}.tmp",
        std::process::id(),
        crate::document::next_id()
    ));
    let mut created = false;
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        created = true;
        if let Ok(metadata) = std::fs::metadata(path)
            && metadata.is_file()
        {
            file.set_permissions(metadata.permissions())
                .map_err(|e| e.to_string())?;
        }
        file.write_all(s.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        std::fs::rename(&temporary, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        if created {
            let _ = std::fs::remove_file(temporary);
        }
        for font in copied_fonts {
            let _ = std::fs::remove_file(font);
        }
    }
    result
}

pub fn load_from(path: &Path) -> Result<Document, String> {
    let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    if s.contains("omatype:") {
        crate::typography::load_for_document(path)?;
    }
    let doc = decode(&s)?;
    crate::typography::validate_document_fonts(&doc)?;
    Ok(doc)
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
    "oma",
    "svg",
    "svgz",
    "png",
    "jpg",
    "jpeg",
    "webp",
    "gif",
    "bmp",
    "tif",
    "tiff",
    "pdf",
    "ai",
    "eps",
    "ps",
    "psd",
    "psb",
    "ora",
    "af",
    "afdesign",
    "afphoto",
    "afpub",
    "aftemplate",
    "afpackage",
];

pub fn dialog_open() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("All supported", PLACE_EXTS)
        .add_filter("omadesign", &["oma"])
        .add_filter(
            "Layered documents",
            &[
                "psd",
                "psb",
                "ora",
                "af",
                "afdesign",
                "afphoto",
                "afpub",
                "aftemplate",
                "afpackage",
            ],
        )
        .add_filter(
            "Images",
            &[
                "png", "jpg", "jpeg", "webp", "gif", "tif", "tiff", "bmp", "psd",
            ],
        )
        .add_filter("Vector", &["svg", "svgz", "pdf", "ai", "eps"])
        .pick_file()
}

pub fn dialog_place() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Place", PLACE_EXTS)
        .add_filter(
            "Images",
            &[
                "png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "psd",
            ],
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
    let strings: Vec<String> = v.iter().map(|p| p.to_string_lossy().into_owned()).collect();
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

fn pack_rasters(doc: &mut Document) -> Result<(), String> {
    for layer in &mut doc.layers {
        if let Some(px) = layer.kind.pixels_mut() {
            *px = compress_pixels(px)?;
        }
        if let Some(mask) = layer.mask.as_mut() {
            *mask = compress_pixels(mask)?;
        }
    }
    Ok(())
}

/// A worker writes a temporary file; only the UI may publish it after checking
/// that the document is still open and unsaved. Dropping it cancels the write.
pub(crate) struct PreparedSwap {
    temporary: PathBuf,
    destination: PathBuf,
    font_copies: Vec<PathBuf>,
    font_ids: std::collections::HashSet<String>,
    committed: bool,
}

impl PreparedSwap {
    pub(crate) fn commit(mut self) -> Result<PathBuf, String> {
        std::fs::rename(&self.temporary, &self.destination).map_err(|e| e.to_string())?;
        self.committed = true;
        // JSON is now committed. Remove only obsolete font files from this
        // snapshot's private archive; a failed preparation never prunes them.
        let archive = swap_font_archive(&self.destination);
        if let Ok(files) = std::fs::read_dir(archive.join(".omabrand/fonts")) {
            for file in files.flatten() {
                let path = file.path();
                if file.file_type().is_ok_and(|kind| kind.is_file())
                    && path.file_stem().and_then(|s| s.to_str()).is_some_and(|id| {
                        id.len() == 32
                            && id.bytes().all(|b| b.is_ascii_hexdigit())
                            && !self.font_ids.contains(id)
                    })
                {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
        remove_empty_font_archive(&archive);
        Ok(self.destination.clone())
    }
}

impl Drop for PreparedSwap {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.temporary);
        if !self.committed {
            for path in &self.font_copies {
                let _ = std::fs::remove_file(path);
            }
            remove_empty_font_archive(&swap_font_archive(&self.destination));
        }
    }
}

pub(crate) fn prepare_swap(
    mut meta: SwapMeta,
    destination: PathBuf,
) -> Result<PreparedSwap, String> {
    use std::io::Write;

    pack_rasters(&mut meta.doc)?;
    if let Some(dir) = destination.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let temporary = destination.with_extension(format!("{}.tmp", new_swap_id()));
    let file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    let mut prepared = PreparedSwap {
        temporary,
        destination,
        font_copies: vec![],
        font_ids: Default::default(),
        committed: false,
    };
    (prepared.font_copies, prepared.font_ids) = crate::typography::archive_recovery_fonts(
        &meta.doc,
        &swap_font_archive(&prepared.destination),
    )?;
    let mut writer = std::io::BufWriter::new(file);
    serde_json::to_writer(&mut writer, &meta).map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;
    writer.get_ref().sync_all().map_err(|e| e.to_string())?;
    Ok(prepared)
}

pub fn load_swap(path: &Path) -> Result<SwapMeta, String> {
    let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut meta: SwapMeta = serde_json::from_str(&s).map_err(|e| e.to_string())?;
    let archive = swap_font_archive(path);
    if archive.is_dir() {
        crate::typography::register_archive(&archive)?;
    } else if let Some(original) = &meta.original {
        crate::typography::load_for_document(original)?;
    }
    crate::typography::validate_document_fonts(&meta.doc)?;
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
    let _ = delete_swap_at(&swap_path(id));
}

fn swap_font_archive(path: &Path) -> PathBuf {
    path.with_extension("swp-fonts")
}

fn remove_empty_font_archive(path: &Path) {
    for directory in [
        path.join(".omabrand/fonts"),
        path.join(".omabrand"),
        path.to_owned(),
    ] {
        let _ = std::fs::remove_dir(directory);
    }
}

pub fn delete_swap_at(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    match std::fs::remove_dir_all(swap_font_archive(path)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
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
        // Listing needs metadata only. Keep older snapshots visible even when
        // restoring one needs a missing font; Recover reports that error.
        if let Some(meta) = std::fs::read_to_string(&p)
            .ok()
            .and_then(|json| serde_json::from_str::<SwapMeta>(&json).ok())
        {
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
    use crate::document::{Cmd, Shape, Style, apply};
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

    #[test]
    fn save_as_keeps_portable_fonts_and_failures_preserve_the_previous_file() {
        use crate::document::Layer;
        use crate::geom::TypeRun;
        use std::fs;
        const CHILD_PATH: &str = "OMADESIGN_SAVE_FONT_REOPEN_TEST";
        if let Some(path) = std::env::var_os(CHILD_PATH) {
            let path = PathBuf::from(path);
            let recovery = path.extension().is_some_and(|extension| extension == "swp");
            let mut doc = if recovery {
                let meta = load_swap(&path).unwrap();
                assert!(
                    meta.original.is_none(),
                    "unsaved recovery must carry its own fonts"
                );
                let legacy = path.parent().unwrap().join("legacy.oma.swp");
                assert!(load_swap(&legacy).is_err());
                assert!(
                    list_swaps()
                        .iter()
                        .any(|(path, meta)| path == &legacy && meta.name == "Legacy missing font"),
                    "missing legacy fonts hid the recoverable snapshot"
                );
                meta.doc
            } else {
                load_from(&path).unwrap()
            };
            let Geom::Text(run) = &mut doc.layers[0].kind.shapes_mut().unwrap()[0].geom else {
                panic!("saved artwork must remain live text")
            };
            assert!(crate::text::project_font_loaded(&run.font));
            let original = run.contours.clone();
            run.content.push_str(" Ω");
            run.contours = crate::text::shape(run);
            assert!(run.contours.len() > 1 && run.contours != original);
            if !recovery {
                save_to(&doc, &path).unwrap();
            }
            return;
        }
        struct Temp(PathBuf);
        impl Drop for Temp {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let temp = Temp(std::env::temp_dir().join(format!(
            "oma-save-fonts-{}-{}",
            std::process::id(),
            crate::document::next_id()
        )));
        fs::create_dir(&temp.0).unwrap();
        let mut bytes =
            fs::read(crate::text::default_path().expect("a desktop text font")).unwrap();
        bytes.extend_from_slice(format!("save-test-{}", crate::document::next_id()).as_bytes());
        let id = format!("omatype:{:032x}", crate::typography::fingerprint(&bytes));
        crate::text::register_memory_font(
            &id,
            "Imported project face",
            std::sync::Arc::new(bytes.clone()),
        )
        .unwrap();
        let mut doc = Document::new("Placed portable text", 1.0, 1.0, 72.0);
        doc.layers = vec![Layer::vector("Text")];
        doc.layers[0].kind.shapes_mut().unwrap().push(Shape::new(
            Geom::Text(TypeRun {
                content: "Save and travel".into(),
                font: id.clone(),
                px: 28.0,
                ..Default::default()
            }),
            Style::default(),
        ));
        let project = temp.0.join("Saved project");
        let drafts = project.join("drafts");
        fs::create_dir_all(&drafts).unwrap();
        fs::write(project.join(".omacolors"), "[]").unwrap();
        let path = drafts.join("artwork.oma");
        save_to(&doc, &path).unwrap();
        let extension = if bytes.starts_with(b"OTTO") {
            "otf"
        } else {
            "ttf"
        };
        let relative_font = format!(
            ".omabrand/fonts/{}.{extension}",
            id.trim_start_matches("omatype:")
        );
        assert_eq!(fs::read(project.join(&relative_font)).unwrap(), bytes);
        assert!(
            !drafts.join(".omabrand").exists(),
            "nested Save As split the existing project library"
        );
        assert!(
            !project.join(".omatype").exists(),
            "saving artwork must not invent or overwrite font roles"
        );
        let moved = temp.0.join("Moved project");
        fs::rename(&project, &moved).unwrap();
        let path = moved.join("drafts/artwork.oma");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "project::tests::save_as_keeps_portable_fonts_and_failures_preserve_the_previous_file", "--nocapture"])
            .env(CHILD_PATH, &path).output().unwrap();
        assert!(
            output.status.success(),
            "fresh-process edit failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let reopened = load_from(&path).unwrap();
        assert!(
            matches!(&reopened.layers[0].kind.shapes().unwrap()[0].geom, Geom::Text(run) if run.content == "Save and travel Ω")
        );

        let previous = fs::read(&path).unwrap();
        let mut missing = doc.clone();
        let Geom::Text(run) = &mut missing.layers[0].kind.shapes_mut().unwrap()[0].geom else {
            unreachable!()
        };
        run.font = format!("omatype:missing-{}", crate::document::next_id());
        assert!(
            save_to(&missing, &path)
                .unwrap_err()
                .contains("project font")
        );
        assert_eq!(
            fs::read(&path).unwrap(),
            previous,
            "missing font replaced the previous document"
        );

        let failed = temp.0.join("Failed write");
        let blocked = failed.join("blocked.oma");
        fs::create_dir_all(&blocked).unwrap();
        fs::write(blocked.join("keep.txt"), "untouched").unwrap();
        assert!(save_to(&doc, &blocked).is_err());
        assert!(
            !failed.join(&relative_font).exists(),
            "failed document write retained its new font copy"
        );
        assert_eq!(
            fs::read_to_string(blocked.join("keep.txt")).unwrap(),
            "untouched"
        );
        assert!(fs::read_dir(&failed).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".oma-save-")
        }));

        let recovery_home = temp.0.join("Recovery data");
        let recovery_dir = recovery_home.join("omadesign");
        let snapshot = recovery_dir.join("unsaved.oma.swp");
        let meta = SwapMeta {
            id: "unsaved".into(),
            original: None,
            name: doc.name.clone(),
            saved_at: 1,
            doc: doc.clone(),
        };
        prepare_swap(meta.clone(), snapshot.clone())
            .unwrap()
            .commit()
            .unwrap();
        let archive = swap_font_archive(&snapshot);
        let original_archive = archive.join(&relative_font);
        assert_eq!(fs::read(&original_archive).unwrap(), bytes);
        let snapshot_bytes = fs::read(&snapshot).unwrap();
        let mut legacy = meta.clone();
        legacy.name = "Legacy missing font".into();
        legacy.doc = missing.clone();
        let legacy_path = recovery_dir.join("legacy.oma.swp");
        fs::write(&legacy_path, serde_json::to_string(&legacy).unwrap()).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "project::tests::save_as_keeps_portable_fonts_and_failures_preserve_the_previous_file", "--nocapture"])
            .env(CHILD_PATH, &snapshot).env("XDG_DATA_HOME", &recovery_home).output().unwrap();
        assert!(
            output.status.success(),
            "fresh-process unsaved recovery failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let mut replacement_bytes = bytes.clone();
        replacement_bytes.extend_from_slice(b"replacement");
        let replacement_id = format!(
            "omatype:{:032x}",
            crate::typography::fingerprint(&replacement_bytes)
        );
        crate::text::register_memory_font(
            &replacement_id,
            "Replacement recovery face",
            std::sync::Arc::new(replacement_bytes.clone()),
        )
        .unwrap();
        let mut replacement = meta.clone();
        let Geom::Text(run) = &mut replacement.doc.layers[0].kind.shapes_mut().unwrap()[0].geom
        else {
            unreachable!()
        };
        run.font = replacement_id.clone();
        let replacement_archive = archive.join(format!(
            ".omabrand/fonts/{}.{extension}",
            replacement_id.trim_start_matches("omatype:")
        ));
        let pending = prepare_swap(replacement.clone(), snapshot.clone()).unwrap();
        assert!(replacement_archive.exists());
        drop(pending);
        assert!(
            !replacement_archive.exists(),
            "canceled recovery left its new font dependency"
        );
        assert_eq!(fs::read(&snapshot).unwrap(), snapshot_bytes);
        assert_eq!(fs::read(&original_archive).unwrap(), bytes);
        assert!(prepare_swap(legacy, snapshot.clone()).is_err());
        assert_eq!(
            fs::read(&snapshot).unwrap(),
            snapshot_bytes,
            "failed snapshot replaced working recovery"
        );
        assert!(original_archive.exists());
        prepare_swap(replacement, snapshot.clone())
            .unwrap()
            .commit()
            .unwrap();
        assert!(
            !original_archive.exists(),
            "per-swap archive kept obsolete fonts"
        );
        assert_eq!(fs::read(&replacement_archive).unwrap(), replacement_bytes);
        delete_swap_at(&snapshot).unwrap();
        assert!(!snapshot.exists() && !archive.exists());
        assert!(
            legacy_path.exists(),
            "deleting one recovery removed another"
        );
        delete_swap_at(&legacy_path).unwrap();
        assert_eq!(fs::read_dir(&recovery_dir).unwrap().count(), 0);

        // Removed role archives still count toward the cold loader's limit.
        // Appending a dependency must fail atomically, not save a document that
        // can only reopen while its font remains in the current process cache.
        let full = temp.0.join("Full archive");
        let full_fonts = full.join(".omabrand/fonts");
        fs::create_dir_all(&full_fonts).unwrap();
        let mut padded = bytes.clone();
        padded.resize(16 * 1024 * 1024, 0);
        let existing = full_fonts.join("archived-0.ttf");
        fs::write(&existing, &padded).unwrap();
        for index in 1..4 {
            fs::hard_link(&existing, full_fonts.join(format!("archived-{index}.ttf"))).unwrap();
        }
        let full_doc = full.join("artwork.oma");
        fs::write(&full_doc, &previous).unwrap();
        assert!(save_to(&doc, &full_doc).unwrap_err().contains("64 MB"));
        assert_eq!(fs::read(&full_doc).unwrap(), previous);
        assert_eq!(fs::read_dir(&full_fonts).unwrap().count(), 4);
        assert!(!full.join(&relative_font).exists());
    }
}
