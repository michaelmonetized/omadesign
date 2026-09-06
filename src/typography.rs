//! Portable project typography; font paths stay inside `.omabrand/fonts`.
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

pub const FILE: &str = ".omatype";
const MAX_FILE: u64 = 64 * 1024;
const MAX_FONT: u64 = 16 * 1024 * 1024;
const MAX_TOTAL: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FontRole {
    pub name: String,
    /// Relative to `.omabrand`, never an installed font path.
    pub font: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeKit {
    pub version: u8,
    pub name: String,
    pub roles: Vec<FontRole>,
}
impl Default for TypeKit {
    fn default() -> Self {
        Self {
            version: 1,
            name: "Brand typography".into(),
            roles: vec![],
        }
    }
}
#[derive(Clone, Debug)]
pub struct LoadedFont {
    pub role: String,
    pub file: String,
    pub id: String,
    pub family: String,
    pub bytes: Arc<Vec<u8>>,
}
#[derive(Clone, Debug)]
pub struct LoadedKit {
    pub kit: TypeKit,
    pub fonts: Vec<LoadedFont>,
    pub stamp: u128,
}

/// Stable FNV-1a 128, with byte comparison on registration to reject collisions.
pub fn fingerprint(bytes: &[u8]) -> u128 {
    bytes
        .iter()
        .fold(0x6c62272e07bb014262b821756295c58d, |hash, byte| {
            (hash ^ u128::from(*byte)).wrapping_mul(0x0000000001000000000000000000013b)
        })
}
fn read(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Choose a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err(format!("{} exceeds the size limit", path.display()));
    }
    Ok(bytes)
}
fn name(value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.chars().count() > 100 || value.chars().any(char::is_control)
    {
        Err("Use a name between 1 and 100 characters without line breaks".into())
    } else {
        Ok(())
    }
}
fn relative_font(value: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if value.contains('\\')
        || !path.starts_with("fonts")
        || path.components().count() < 2
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || !path
            .extension()
            .and_then(|v| v.to_str())
            .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "ttf" | "otf"))
    {
        return Err("Font files must be relative TTF or OTF paths inside fonts/".into());
    }
    Ok(path)
}
fn validate(kit: &TypeKit) -> Result<(), String> {
    if kit.version != 1 {
        return Err("Unsupported typography version; this app reads version 1".into());
    }
    name(&kit.name)?;
    if kit.roles.len() > 64 {
        return Err("A typography kit can hold up to 64 roles".into());
    }
    let mut names = HashSet::new();
    for role in &kit.roles {
        name(&role.name)?;
        if !names.insert(role.name.trim().to_lowercase()) {
            return Err(format!("Role name {:?} is repeated", role.name));
        }
        relative_font(&role.font)?;
    }
    Ok(())
}
pub fn encode(kit: &TypeKit) -> Result<String, String> {
    validate(kit)?;
    let result = serde_json::to_string_pretty(kit).map_err(|e| e.to_string())?;
    if result.len() as u64 > MAX_FILE {
        return Err("Typography file is too large".into());
    }
    Ok(result + "\n")
}
pub fn decode(json: &str) -> Result<TypeKit, String> {
    if json.len() as u64 > MAX_FILE {
        return Err("Typography file is too large".into());
    }
    let kit = serde_json::from_str(json).map_err(|e| format!("Invalid typography JSON: {e}"))?;
    validate(&kit)?;
    Ok(kit)
}
fn safe_font(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let mut path = root.to_path_buf();
    for part in relative_font(relative)?.components() {
        path.push(part);
        if fs::symlink_metadata(&path)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("Linked font files and folders are not followed".into());
        }
    }
    Ok(path)
}
fn font(bytes: Vec<u8>, role: &FontRole) -> Result<LoadedFont, String> {
    if bytes.len() as u64 > MAX_FONT
        || rustybuzz::Face::from_slice(&bytes, 0).is_none()
        || ab_glyph::FontRef::try_from_slice(&bytes).is_err()
    {
        return Err(format!("{} is not a supported TTF or OTF font", role.font));
    }
    Ok(LoadedFont {
        role: role.name.clone(),
        file: role.font.clone(),
        id: format!("omatype:{:032x}", fingerprint(&bytes)),
        family: crate::text::font_name(&bytes).unwrap_or_else(|| role.name.clone()),
        bytes: Arc::new(bytes),
    })
}
fn load_fonts(root: &Path, kit: TypeKit, stamp: u128) -> Result<LoadedKit, String> {
    if fs::symlink_metadata(root).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err("Linked brand folders are not followed".into());
    }
    let mut total = 0;
    let mut fonts = Vec::new();
    for role in &kit.roles {
        let loaded = cached_font(&safe_font(root, &role.font)?, role)?;
        total += loaded.bytes.len();
        if total > MAX_TOTAL {
            return Err("Typography fonts exceed the 64 MB collection limit".into());
        }
        fonts.push(loaded);
    }
    Ok(LoadedKit { kit, fonts, stamp })
}

// Refresh metadata on each library scan; re-read and reshape only changed files.
fn cached_font(path: &Path, role: &FontRole) -> Result<LoadedFont, String> {
    #[derive(PartialEq, Eq)]
    struct Stamp {
        modified: std::time::SystemTime,
        size: u64,
        #[cfg(unix)]
        identity: (u64, u64, i64, i64),
    }
    type Entry = (Stamp, LoadedFont);
    static CACHE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<PathBuf, Entry>>> =
        std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
    let stamp = Stamp {
        modified: metadata.modified().map_err(|e| e.to_string())?,
        size: metadata.len(),
        #[cfg(unix)]
        identity: {
            use std::os::unix::fs::MetadataExt;
            (
                metadata.dev(),
                metadata.ino(),
                metadata.ctime(),
                metadata.ctime_nsec(),
            )
        },
    };
    if let Some(mut loaded) = cache
        .lock()
        .map_err(|_| "Font cache is unavailable")?
        .get(path)
        .filter(|(cached, _)| *cached == stamp)
        .map(|(_, loaded)| loaded.clone())
    {
        loaded.role = role.name.clone();
        loaded.file = role.font.clone();
        return Ok(loaded);
    }
    let loaded = font(read(path, MAX_FONT)?, role)?;
    let mut cache = cache.lock().map_err(|_| "Font cache is unavailable")?;
    if cache.len() >= 256 {
        cache.clear();
    }
    cache.insert(path.to_owned(), (stamp, loaded.clone()));
    Ok(loaded)
}

pub fn load_file(path: &Path) -> Result<LoadedKit, String> {
    let bytes = read(path, MAX_FILE)?;
    let kit = decode(std::str::from_utf8(&bytes).map_err(|_| "Typography JSON must be UTF-8")?)?;
    load_fonts(
        &path.parent().unwrap_or(Path::new(".")).join(".omabrand"),
        kit,
        fingerprint(&bytes),
    )
}
pub fn load(root: &Path) -> Result<Option<LoadedKit>, String> {
    let path = root.join(FILE);
    if !path.exists() {
        return Ok(None);
    }
    load_file(&path).map(Some)
}
pub fn register(loaded: &LoadedKit) -> Result<(), String> {
    for font in &loaded.fonts {
        crate::text::register_memory_font(&font.id, &font.family, Arc::clone(&font.bytes))?;
    }
    Ok(())
}
fn document_root(path: &Path) -> Option<&Path> {
    path.parent()?
        .ancestors()
        .find(|root| root.join(FILE).is_file() || root.join(".omabrand/fonts").is_dir())
}

/// Archived faces remain usable after a role is renamed or removed.
fn archived_fonts(root: &Path) -> Result<Vec<LoadedFont>, String> {
    let bank = root.join(".omabrand");
    let directory = bank.join("fonts");
    if !directory.exists() {
        return Ok(vec![]);
    }
    if fs::symlink_metadata(&bank).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err("Linked brand folders are not followed".into());
    }
    let mut pending = vec![(directory, 0)];
    let mut fonts = Vec::new();
    let mut total = 0;
    while let Some((directory, depth)) = pending.pop() {
        if depth > 8 {
            return Err("Font folders are nested too deeply".into());
        }
        for entry in fs::read_dir(&directory).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if metadata.file_type().is_symlink() {
                return Err("Linked font files and folders are not followed".into());
            }
            if metadata.is_dir() {
                pending.push((path, depth + 1));
                continue;
            }
            if !path
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "ttf" | "otf"))
            {
                continue;
            }
            if fonts.len() >= 256 {
                return Err("Font archive exceeds 256 files".into());
            }
            let relative = path
                .strip_prefix(&bank)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .into_owned();
            let loaded = cached_font(
                &safe_font(&bank, &relative)?,
                &FontRole {
                    name: "Archived font".into(),
                    font: relative,
                },
            )?;
            total += loaded.bytes.len();
            if total > MAX_TOTAL {
                return Err("Font archive exceeds 64 MB".into());
            }
            fonts.push(loaded);
        }
    }
    Ok(fonts)
}

pub fn load_for_document(path: &Path) -> Result<(), String> {
    if let Some(root) = document_root(path) {
        register_archive(root)?;
    }
    Ok(())
}

pub(crate) fn register_archive(root: &Path) -> Result<(), String> {
    for font in archived_fonts(root)? {
        crate::text::register_memory_font(&font.id, &font.family, Arc::clone(&font.bytes))?;
    }
    Ok(())
}

/// Copy only the native artwork's font dependencies. Returned paths are absolute
/// and newly created, so the caller can roll back a larger asset import safely.
pub fn copy_document_fonts(source: &Path, destination: &Path) -> Result<Vec<PathBuf>, String> {
    fn references(value: &serde_json::Value, out: &mut HashSet<String>) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    if key == "font"
                        && let Some(id) = value.as_str().filter(|id| id.starts_with("omatype:"))
                    {
                        out.insert(id.into());
                    } else {
                        references(value, out);
                    }
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    references(value, out);
                }
            }
            _ => {}
        }
    }
    let bytes = read(source, 64 * 1024 * 1024)?;
    let value = serde_json::from_slice(&bytes).map_err(|e| format!("Invalid artwork JSON: {e}"))?;
    let mut ids = HashSet::new();
    references(&value, &mut ids);
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let source_root = document_root(source)
        .ok_or("This artwork's project fonts are missing. Copy its .omabrand/fonts folder too.")?;
    let fonts = archived_fonts(source_root)?;
    let needed: Vec<_> = ids
        .iter()
        .map(|id| {
            fonts
                .iter()
                .find(|font| &font.id == id)
                .ok_or_else(|| format!("The artwork needs a missing project font: {id}"))
        })
        .collect::<Result<_, _>>()?;
    copy_fonts(needed, destination)
}

fn copy_fonts<'a>(
    needed: impl IntoIterator<Item = &'a LoadedFont>,
    destination: &Path,
) -> Result<Vec<PathBuf>, String> {
    if fs::symlink_metadata(destination.join(".omabrand"))
        .is_ok_and(|meta| meta.file_type().is_symlink())
        || fs::symlink_metadata(destination.join(".omabrand/fonts"))
            .is_ok_and(|meta| meta.file_type().is_symlink())
    {
        return Err("Linked font folders are not followed".into());
    }
    let directory = destination.join(".omabrand/fonts");
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let directory = directory.canonicalize().map_err(|e| e.to_string())?;
    let mut copied = Vec::new();
    let result = (|| {
        for font in needed {
            let extension = Path::new(&font.file).extension().unwrap().to_string_lossy();
            let path = directory.join(format!(
                "{}.{extension}",
                font.id.trim_start_matches("omatype:")
            ));
            if path.exists() {
                if fs::symlink_metadata(&path)
                    .map_err(|e| e.to_string())?
                    .file_type()
                    .is_symlink()
                    || read(&path, MAX_FONT)? != *font.bytes
                {
                    return Err("A different font already uses this archived filename".into());
                }
                continue;
            }
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)
                .map_err(|e| e.to_string())?;
            copied.push(path);
            file.write_all(&font.bytes)
                .and_then(|_| file.sync_all())
                .map_err(|e| e.to_string())?;
        }
        // Use the same limits as a cold reopen, including faces kept after
        // their roles were removed. Roll back new copies if the archive is full.
        archived_fonts(destination)?;
        Ok(())
    })();
    if let Err(error) = result {
        for path in copied {
            let _ = fs::remove_file(path);
        }
        return Err(error);
    }
    Ok(copied)
}

fn stamp(root: &Path) -> Result<Option<u128>, String> {
    if !root.join(FILE).exists() {
        return Ok(None);
    }
    Ok(Some(fingerprint(&read(&root.join(FILE), MAX_FILE)?)))
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("Choose a project folder")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp = parent.join(format!(
        ".omatype-{}-{}.tmp",
        std::process::id(),
        crate::document::next_id()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(&temp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
pub fn save(root: &Path, kit: &TypeKit, expected: Option<u128>) -> Result<LoadedKit, String> {
    let json = encode(kit)?;
    if stamp(root)? != expected {
        return Err("Typography changed on disk. Reload before saving.".into());
    }
    // Prepare every face before replacing the role file; a failed import never
    // leaves a manifest pointing at files that its rollback removes.
    let loaded = load_fonts(
        &root.join(".omabrand"),
        kit.clone(),
        fingerprint(json.as_bytes()),
    )?;
    atomic_write(&root.join(FILE), json.as_bytes())?;
    Ok(loaded)
}
fn unique_role(kit: &TypeKit, proposed: &str) -> String {
    let mut result = proposed.chars().take(90).collect::<String>();
    let stem = result.clone();
    let mut number = 2;
    while kit
        .roles
        .iter()
        .any(|role| role.name.eq_ignore_ascii_case(&result))
    {
        result = format!("{stem} ({number})");
        number += 1;
    }
    result
}
/// Copy valid fonts first; roll back only this operation's new files on failure.
fn append(
    root: &Path,
    incoming: Vec<(String, String, Vec<u8>)>,
    expected: Option<u128>,
) -> Result<LoadedKit, String> {
    let current = load(root)?;
    if current.as_ref().map(|c| c.stamp) != expected {
        return Err("Typography changed on disk. Reload before importing.".into());
    }
    let mut kit = current.map(|c| c.kit).unwrap_or_default();
    let fonts = root.join(".omabrand/fonts");
    if fs::symlink_metadata(root.join(".omabrand")).is_ok_and(|meta| meta.file_type().is_symlink())
    {
        return Err("Linked brand folders are not followed".into());
    }
    fs::create_dir_all(&fonts).map_err(|e| e.to_string())?;
    if fs::symlink_metadata(&fonts)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Linked font folders are not followed".into());
    }
    let mut added = Vec::new();
    let result = (|| {
        for (role, extension, bytes) in incoming {
            let filename = format!("{:032x}.{extension}", fingerprint(&bytes));
            let target = fonts.join(&filename);
            let entry = FontRole {
                name: unique_role(&kit, &role),
                font: format!("fonts/{filename}"),
            };
            font(bytes.clone(), &entry)?;
            if target.exists() {
                if read(&target, MAX_FONT)? != bytes {
                    return Err("A different font already uses that filename".into());
                }
            } else {
                let mut file = OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&target)
                    .map_err(|e| e.to_string())?;
                added.push(target);
                file.write_all(&bytes)
                    .and_then(|_| file.sync_all())
                    .map_err(|e| e.to_string())?;
            }
            kit.roles.push(entry);
        }
        save(root, &kit, expected)
    })();
    if result.is_err() {
        for file in added {
            let _ = fs::remove_file(file);
        }
    }
    result
}
pub fn add_fonts(
    root: &Path,
    paths: &[PathBuf],
    expected: Option<u128>,
) -> Result<LoadedKit, String> {
    if paths.len() > 64 {
        return Err("Choose up to 64 fonts".into());
    }
    let mut incoming = Vec::new();
    let mut total = 0;
    for path in paths {
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(extension.as_str(), "ttf" | "otf") {
            return Err("Choose TTF or OTF font files".into());
        }
        let bytes = read(path, MAX_FONT)?;
        total += bytes.len();
        if total > MAX_TOTAL {
            return Err("Choose fonts totalling no more than 64 MB".into());
        }
        incoming.push((
            crate::text::font_name(&bytes).unwrap_or_else(|| "Font".into()),
            extension,
            bytes,
        ));
    }
    append(root, incoming, expected)
}
pub fn import_file(
    source: &Path,
    destination: &Path,
    expected: Option<u128>,
) -> Result<LoadedKit, String> {
    let loaded = load_file(source)?;
    let incoming = loaded
        .fonts
        .into_iter()
        .map(|font| {
            let extension = Path::new(&font.file)
                .extension()
                .unwrap()
                .to_string_lossy()
                .to_string();
            (font.role, extension, (*font.bytes).clone())
        })
        .collect();
    append(destination, incoming, expected)
}
pub fn export_copy(source: &Path, destination: &Path) -> Result<(), String> {
    if destination.join(FILE).exists() {
        return Err("The destination already has a typography kit".into());
    }
    let loaded = load(source)?.ok_or("This project has no typography kit")?;
    let fonts = archived_fonts(source)?;
    let copied = copy_fonts(&fonts, destination)?;
    let mut kit = loaded.kit;
    for (role, font) in kit.roles.iter_mut().zip(&loaded.fonts) {
        let extension = Path::new(&font.file).extension().unwrap().to_string_lossy();
        role.font = format!(
            "fonts/{}.{extension}",
            font.id.trim_start_matches("omatype:")
        );
    }
    if let Err(error) = save(destination, &kit, None) {
        for path in copied {
            let _ = fs::remove_file(path);
        }
        return Err(error);
    }
    Ok(())
}

pub(crate) fn validate_document_fonts(doc: &crate::document::Document) -> Result<(), String> {
    for layer in &doc.layers {
        for shape in layer.kind.shapes().unwrap_or_default() {
            if let crate::geom::Geom::Text(run) = &shape.geom
                && run.font.starts_with("omatype:")
                && !crate::text::project_font_loaded(&run.font)
            {
                return Err("This artwork needs a missing project font. Copy its .omabrand/fonts folder beside the project.".into());
            }
        }
    }
    Ok(())
}

fn document_fonts(doc: &crate::document::Document) -> Result<Vec<LoadedFont>, String> {
    let mut seen = HashSet::new();
    let mut fonts = Vec::new();
    let mut total = 0;
    for layer in &doc.layers {
        for shape in layer.kind.shapes().unwrap_or_default() {
            let crate::geom::Geom::Text(run) = &shape.geom else {
                continue;
            };
            if !run.font.starts_with("omatype:") || !seen.insert(run.font.clone()) {
                continue;
            }
            let bytes = crate::text::project_font_bytes(&run.font).ok_or_else(|| {
                format!("Cannot save without the artwork's project font: {}. Load its typography kit first.", run.font)
            })?;
            total += bytes.len();
            if fonts.len() >= 256 || total > MAX_TOTAL {
                return Err("The artwork's font archive exceeds 256 faces or 64 MB".into());
            }
            let extension = if bytes.starts_with(b"OTTO") {
                "otf"
            } else {
                "ttf"
            };
            fonts.push(LoadedFont {
                role: "Archived font".into(),
                file: format!(
                    "fonts/{}.{extension}",
                    run.font.trim_start_matches("omatype:")
                ),
                id: run.font.clone(),
                family: crate::text::label_for(&run.font),
                bytes,
            });
        }
    }
    Ok(fonts)
}

/// Persist the portable faces used by live artwork before its project is saved.
/// Return only new files, allowing the document writer to roll back a failed save.
pub(crate) fn archive_document_fonts(
    doc: &crate::document::Document,
    destination: &Path,
) -> Result<Vec<PathBuf>, String> {
    let fonts = document_fonts(doc)?;
    if fonts.is_empty() {
        return Ok(vec![]);
    }
    let root = destination
        .ancestors()
        .find(|root| {
            root.join(".omabrand").is_dir()
                || root.join(FILE).is_file()
                || root.join(".omacolors").is_file()
        })
        .unwrap_or(destination);
    copy_fonts(&fonts, root)
}

/// Recovery owns this exact archive rather than sharing an ancestor project's files.
pub(crate) fn archive_recovery_fonts(
    doc: &crate::document::Document,
    destination: &Path,
) -> Result<(Vec<PathBuf>, HashSet<String>), String> {
    for path in [
        destination.to_owned(),
        destination.join(".omabrand"),
        destination.join(".omabrand/fonts"),
    ] {
        if fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Err("Linked recovery font folders are not followed".into());
        }
    }
    let fonts = document_fonts(doc)?;
    let ids = fonts
        .iter()
        .map(|font| font.id.trim_start_matches("omatype:").to_owned())
        .collect();
    let copied = if fonts.is_empty() {
        vec![]
    } else {
        copy_fonts(&fonts, destination)?
    };
    Ok((copied, ids))
}

#[cfg(test)]
#[path = "typography_tests.rs"]
mod tests;
