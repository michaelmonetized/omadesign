//! Portable `.omabrand` folders. Indexing never decodes assets or runs converters.
//! Call scanning, copying, and preview decoding on a worker, not the UI thread.

use crate::photo::RgbaImage;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

pub const FOLDER: &str = ".omabrand";
const MAX_DEPTH: usize = 12;
const MAX_ENTRIES: usize = 10_000;
const MAX_ASSETS: usize = 2_048;
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PIXELS: u64 = 16 * 1024 * 1024;
const MAX_EDGE: u32 = 16_384;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AssetKind {
    Raster,
    Svg,
    Document,
}

impl AssetKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Raster => "Image",
            Self::Svg => "SVG",
            Self::Document => "omadesign",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    /// Relative to Catalog::root; also the stable identity across rescans.
    pub relative_path: PathBuf,
    pub name: String,
    pub kind: AssetKind,
    pub size: u64,
    pub modified_ns: u128,
    search: String,
}

impl Asset {
    pub fn matches(&self, query: &str) -> bool {
        query
            .to_lowercase()
            .split_whitespace()
            .all(|word| self.search.contains(word))
    }

    /// Include the catalogue root in a cache key shared by multiple libraries.
    pub fn fingerprint(&self) -> (PathBuf, u64, u128) {
        (self.relative_path.clone(), self.size, self.modified_ns)
    }

    pub fn path(&self, root: &Path) -> PathBuf {
        root.join(&self.relative_path)
    }
}

#[derive(Clone, Debug)]
pub struct Catalog {
    pub name: String,
    pub root: PathBuf,
    pub assets: Vec<Asset>,
    /// Partial scans and invalid manifests are visible, without hiding valid assets.
    pub warnings: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    version: u32,
    name: String,
}

fn bank_root(folder: &Path) -> PathBuf {
    if folder.file_name().is_some_and(|name| name == FOLDER) {
        folder.to_owned()
    } else if folder.join(FOLDER).is_dir() {
        folder.join(FOLDER)
    } else {
        folder.to_owned()
    }
}

/// An explicit library wins. Otherwise find the closest ancestor's `.omabrand`.
/// Unsaved documents have no implicit library; do not search unrelated home files.
pub fn discover(document_path: Option<&Path>, explicit_folder: Option<&Path>) -> Option<PathBuf> {
    if let Some(folder) = explicit_folder {
        let root = bank_root(folder);
        return root.is_dir().then_some(root);
    }
    let path = document_path?;
    let start = if path.is_dir() { path } else { path.parent()? };
    start.ancestors().find_map(|parent| {
        let candidate = parent.join(FOLDER);
        fs::symlink_metadata(&candidate)
            .ok()
            .filter(|meta| meta.is_dir() && !meta.file_type().is_symlink())
            .map(|_| candidate)
    })
}

fn kind(path: &Path) -> Option<AssetKind> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "tif" | "tiff" | "bmp" | "gif" => Some(AssetKind::Raster),
        "svg" => Some(AssetKind::Svg),
        "oma" => Some(AssetKind::Document),
        _ => None,
    }
}

fn io_error(action: &str, path: &Path, error: impl std::fmt::Display) -> String {
    format!("{action} {}: {error}", path.display())
}

fn canonical_root(folder: &Path) -> Result<PathBuf, String> {
    let root = bank_root(folder);
    if !root.is_dir() {
        return Err(format!("Library folder does not exist: {}", root.display()));
    }
    root.canonicalize()
        .map_err(|e| io_error("Could not open", &root, e))
}

/// Returns regular files only. Never follows a symlink, including directory loops.
fn walk(root: &Path, warnings: &mut Vec<String>) -> Result<Vec<(PathBuf, fs::Metadata)>, String> {
    let mut pending = vec![(root.to_owned(), 0usize)];
    let mut files: Vec<(PathBuf, fs::Metadata)> = Vec::new();
    let mut seen = 0;
    while let Some((directory, depth)) = pending.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(e) if depth == 0 => return Err(io_error("Could not read", &directory, e)),
            Err(e) => {
                warnings.push(io_error("Could not read", &directory, e));
                continue;
            }
        };
        for entry in entries {
            seen += 1;
            if seen > MAX_ENTRIES {
                warnings.push(format!("Library scan stopped at {MAX_ENTRIES} entries."));
                files.sort_by(|a, b| a.0.cmp(&b.0));
                return Ok(files);
            }
            let entry = entry.map_err(|e| io_error("Could not read", &directory, e))?;
            let path = entry.path();
            let meta = match fs::symlink_metadata(&path) {
                Ok(meta) => meta,
                Err(e) => {
                    warnings.push(io_error("Could not inspect", &path, e));
                    continue;
                }
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                if depth < MAX_DEPTH {
                    pending.push((path, depth + 1));
                } else {
                    warnings.push(format!("Skipped deeply nested folder: {}", path.display()));
                }
            } else if meta.is_file() {
                files.push((path, meta));
            }
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

pub fn scan(folder: &Path) -> Result<Catalog, String> {
    let root = canonical_root(folder)?;
    let mut warnings = Vec::new();
    let fallback = if root.file_name().is_some_and(|name| name == FOLDER) {
        root.parent()
    } else {
        Some(root.as_path())
    };
    let mut name = fallback
        .and_then(Path::file_name)
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Brand library".into());
    let manifest_path = root.join("brand.json");
    if manifest_path.exists() {
        let manifest = safe_file(&root, Path::new("brand.json"))
            .and_then(|path| read_bounded(&path, 16 * 1024))
            .and_then(|bytes| {
                serde_json::from_slice::<Manifest>(&bytes).map_err(|e| e.to_string())
            });
        match manifest {
            Ok(manifest) if manifest.version == 1 && valid_name(&manifest.name).is_ok() => {
                name = manifest.name.trim().to_owned()
            }
            Ok(_) => {
                warnings.push("brand.json has an unsupported version or an empty name.".into())
            }
            Err(error) => warnings.push(format!("Could not read brand.json: {error}")),
        }
    }
    let mut assets = Vec::new();
    for (path, meta) in walk(&root, &mut warnings)? {
        let Some(kind) = kind(&path) else { continue };
        if assets.len() == MAX_ASSETS {
            warnings.push(format!("Showing the first {MAX_ASSETS} supported assets."));
            break;
        }
        let relative_path = path
            .strip_prefix(&root)
            .map_err(|e| e.to_string())?
            .to_owned();
        let name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let search = format!("{} {}", relative_path.to_string_lossy(), kind.label()).to_lowercase();
        assets.push(Asset {
            relative_path,
            name,
            kind,
            size: meta.len(),
            modified_ns: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos()),
            search,
        });
    }
    Ok(Catalog {
        name,
        root,
        assets,
        warnings,
    })
}

fn valid_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 100 || name.chars().any(char::is_control) {
        Err("Choose a library name between 1 and 100 characters.".into())
    } else {
        Ok(name)
    }
}

/// Create a project-local bank, or use the supplied `.omabrand` path directly.
pub fn create(folder: &Path, name: &str) -> Result<Catalog, String> {
    valid_name(name)?;
    let root = if folder.file_name().is_some_and(|name| name == FOLDER) {
        folder.to_owned()
    } else {
        folder.join(FOLDER)
    };
    fs::create_dir_all(&root).map_err(|e| io_error("Could not create", &root, e))?;
    save_name(&root, name)?;
    scan(&root)
}

/// Write only the optional manifest, with an atomic replace in the same folder.
pub fn save_name(folder: &Path, name: &str) -> Result<(), String> {
    let name = valid_name(name)?;
    let root = canonical_root(folder)?;
    let bytes = serde_json::to_vec_pretty(&Manifest {
        version: 1,
        name: name.to_owned(),
    })
    .map_err(|e| e.to_string())?;
    let temp = root.join(format!(
        ".brand-{}-{}.tmp",
        std::process::id(),
        crate::document::next_id()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        fs::rename(&temp, root.join("brand.json")).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn copy_new(source: &Path, destination: &Path) -> Result<(), String> {
    let meta = fs::symlink_metadata(source).map_err(|e| io_error("Could not read", source, e))?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_FILE_BYTES {
        return Err(format!(
            "Expected a regular file no larger than 64 MB: {}",
            source.display()
        ));
    }
    let mut source_file = File::open(source)
        .map_err(|e| io_error("Could not open", source, e))?
        .take(MAX_FILE_BYTES + 1);
    let mut target = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|e| io_error("Could not create", destination, e))?;
    let result = (|| {
        let copied = std::io::copy(&mut source_file, &mut target).map_err(|e| e.to_string())?;
        if copied > MAX_FILE_BYTES {
            return Err("File grew beyond the 64 MB import limit.".into());
        }
        target.sync_all().map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result
}

/// Copy imports into the bank without overwriting names. On error, roll back this
/// operation's new files; existing assets are never removed or changed.
pub fn add_files(folder: &Path, sources: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let root = canonical_root(folder)?;
    let project = if root.file_name().is_some_and(|name| name == FOLDER) {
        root.parent().ok_or("Library has no project folder")?
    } else {
        &root
    };
    if sources.len() > MAX_ASSETS {
        return Err("Choose at most 2048 assets at a time.".into());
    }
    let mut copied = Vec::new();
    let mut font_copies = Vec::new();
    let result = (|| {
        for source in sources {
            if kind(source).is_none() {
                return Err(format!("Unsupported brand asset: {}", source.display()));
            }
            let filename = source.file_name().ok_or("Asset has no filename")?;
            let mut destination = root.join(filename);
            let mut index = 2;
            while fs::symlink_metadata(&destination).is_ok() {
                let stem = source.file_stem().unwrap_or_default().to_string_lossy();
                let ext = source.extension().unwrap_or_default().to_string_lossy();
                destination = root.join(format!("{stem} ({index}).{ext}"));
                index += 1;
                if index > MAX_ENTRIES {
                    return Err("Too many assets with the same filename.".into());
                }
            }
            copy_new(source, &destination)?;
            copied.push(destination);
            if kind(source) == Some(AssetKind::Document) {
                font_copies.extend(crate::typography::copy_document_fonts(source, project)?);
            }
        }
        Ok(copied
            .iter()
            .map(|path| path.strip_prefix(&root).unwrap().to_owned())
            .collect())
    })();
    if result.is_err() {
        for path in copied.into_iter().chain(font_copies) {
            let _ = fs::remove_file(path);
        }
    }
    result
}

/// Copy the entire portable bank, including its typography kit and font archives,
/// into `destination_folder/.omabrand`. Existing banks are never replaced.
pub fn export_copy_bank(folder: &Path, destination_folder: &Path) -> Result<PathBuf, String> {
    let root = canonical_root(folder)?;
    let parent = destination_folder
        .canonicalize()
        .map_err(|e| io_error("Could not open", destination_folder, e))?;
    if parent.starts_with(&root) {
        return Err("Choose an export folder outside this library.".into());
    }
    let destination = parent.join(FOLDER);
    let typography = root
        .file_name()
        .filter(|name| *name == FOLDER)
        .and_then(|_| root.parent())
        .map(|project| project.join(crate::typography::FILE))
        .filter(|path| fs::symlink_metadata(path).is_ok());
    let destination_type = parent.join(crate::typography::FILE);
    if let Some(source) = &typography {
        let meta =
            fs::symlink_metadata(source).map_err(|e| io_error("Could not inspect", source, e))?;
        if !meta.is_file() || meta.file_type().is_symlink() {
            return Err("Typography kit must be a regular file, not a linked file.".into());
        }
        if fs::symlink_metadata(&destination_type).is_ok() {
            return Err(
                "The destination already has a typography kit; choose another folder.".into(),
            );
        }
        crate::typography::load_file(source)?;
    }
    let mut warnings = Vec::new();
    let files = walk(&root, &mut warnings)?;
    if !warnings.is_empty() {
        return Err(format!(
            "Cannot export an incomplete library: {}",
            warnings.join(" ")
        ));
    }
    if files.iter().map(|(_, meta)| meta.len()).sum::<u64>() > 2 * 1024 * 1024 * 1024 {
        return Err("This library exceeds the 2 GB copy limit.".into());
    }
    fs::create_dir(&destination).map_err(|e| io_error("Could not create", &destination, e))?;
    let mut copied_type = false;
    let result = (|| {
        if let Some(source) = &typography {
            copy_new(source, &destination_type)?;
            copied_type = true;
        }
        for (source, _) in files {
            let target = destination.join(source.strip_prefix(&root).map_err(|e| e.to_string())?);
            fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
            copy_new(&source, &target)?;
        }
        Ok(destination.clone())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&destination);
        if copied_type {
            let _ = fs::remove_file(destination_type);
        }
    }
    result
}

fn safe_file(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("Asset path must stay inside its library.".into());
    }
    let mut path = root.to_owned();
    for part in relative.components() {
        path.push(part);
        if fs::symlink_metadata(&path)
            .map_err(|e| io_error("Could not inspect", &path, e))?
            .file_type()
            .is_symlink()
        {
            return Err("Linked files are not followed in brand libraries.".into());
        }
    }
    let canonical = path
        .canonicalize()
        .map_err(|e| io_error("Could not open", &path, e))?;
    if !canonical.starts_with(root) {
        return Err("Asset is outside its library.".into());
    }
    Ok(canonical)
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|e| io_error("Could not open", path, e))?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > limit {
        return Err(format!("{} is too large for a preview.", path.display()));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("Asset grew beyond the preview limit.".into());
    }
    Ok(bytes)
}

fn decode_image(bytes: &[u8]) -> Result<image::DynamicImage, String> {
    use image::ImageDecoder;
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_EDGE);
    limits.max_image_height = Some(MAX_EDGE);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let decoder = reader
        .into_decoder()
        .map_err(|e| format!("Could not decode preview: {e}"))?;
    let (w, h) = decoder.dimensions();
    check_pixels(w, h)?;
    image::DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())
}

fn check_pixels(w: u32, h: u32) -> Result<u64, String> {
    let pixels = u64::from(w) * u64::from(h);
    if w == 0 || h == 0 || w > MAX_EDGE || h > MAX_EDGE || pixels > MAX_PIXELS {
        return Err("Preview image exceeds 16 megapixels or 16384 pixels on one edge.".into());
    }
    Ok(pixels)
}

/// Bounded full-resolution decode for an explicit placement worker. Unlike
/// import::open_any this never launches external conversion programs.
pub fn load_asset(path: &Path) -> Result<crate::import::Imported, String> {
    for ancestor in path.ancestors().filter(|p| !p.as_os_str().is_empty()) {
        let meta = fs::symlink_metadata(ancestor)
            .map_err(|e| io_error("Could not inspect", ancestor, e))?;
        if meta.file_type().is_symlink() {
            return Err("Linked files and folders are not followed in brand libraries.".into());
        }
    }
    let meta = fs::symlink_metadata(path).map_err(|e| io_error("Could not inspect", path, e))?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err("Choose a regular brand asset file.".into());
    }
    let kind = kind(path).ok_or("Unsupported brand asset format")?;
    let bytes = read_bounded(
        path,
        match kind {
            AssetKind::Svg => 4 * 1024 * 1024,
            AssetKind::Document => 16 * 1024 * 1024,
            AssetKind::Raster => MAX_FILE_BYTES,
        },
    )?;
    let name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    Ok(match kind {
        AssetKind::Raster => {
            let image = decode_image(&bytes)?.to_rgba8();
            let image = RgbaImage::new(image.width(), image.height(), image.into_raw())
                .ok_or("Invalid brand image")?;
            crate::import::Imported::Raster { name, image }
        }
        AssetKind::Svg => {
            let svg = String::from_utf8(bytes).map_err(|e| e.to_string())?;
            svg_document(&svg)?;
            crate::import::Imported::Svg { name, svg }
        }
        AssetKind::Document => {
            crate::typography::load_for_document(path)?;
            crate::import::Imported::Document(oma_document(&bytes)?)
        }
    })
}

/// Worker-safe native preview. Raster animations use their first frame. SVG
/// previews use the same supported geometry/paint subset as native SVG placement.
/// Large or malformed assets return an error for a labelled fallback tile.
pub fn load_thumbnail(folder: &Path, asset: &Asset, max_edge: u32) -> Result<RgbaImage, String> {
    let root = canonical_root(folder)?;
    let path = safe_file(&root, &asset.relative_path)?;
    let edge = max_edge.clamp(16, 512);
    let bytes = read_bounded(
        &path,
        match asset.kind {
            AssetKind::Svg => 4 * 1024 * 1024,
            AssetKind::Document => 16 * 1024 * 1024,
            AssetKind::Raster => MAX_FILE_BYTES,
        },
    )?;
    if asset.kind == AssetKind::Raster {
        let image = decode_image(&bytes)?.thumbnail(edge, edge).to_rgba8();
        return RgbaImage::new(image.width(), image.height(), image.into_raw())
            .ok_or("Invalid preview image".into());
    }
    let doc = match asset.kind {
        AssetKind::Svg => svg_document(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?)?,
        AssetKind::Document => {
            crate::typography::load_for_document(&path)?;
            oma_document(&bytes)?
        }
        AssetKind::Raster => unreachable!(),
    };
    render_document(&doc, edge)
}

fn svg_document(svg: &str) -> Result<crate::document::Document, String> {
    use crate::document::{Cap, Document, Fill, Join, Layer, Shape, Stroke, Style};
    use crate::shape_browser::{SvgPaint, svg_to_elements};
    if !svg.contains("<svg")
        || !svg.contains('>')
        || (!svg.contains("</svg>") && !svg.trim_end().ends_with("/>"))
    {
        return Err("This is not a complete SVG document.".into());
    }
    let elements = svg_to_elements(svg)?;
    if elements.len() > 2048 {
        return Err("SVG contains too many elements for a preview.".into());
    }
    let mut doc = Document::new("SVG", 1.0, 1.0, 72.0);
    doc.layers = vec![Layer::vector("SVG")];
    doc.artboards.clear();
    let mut bounds: Option<crate::geom::Bounds> = None;
    let mut budget = 0;
    for element in elements {
        check_geometry(&element.geom, &mut budget)?;
        let fill = match element.fill {
            SvgPaint::None => Fill::None,
            SvgPaint::Solid(color) => Fill::Solid(color),
            SvgPaint::Unspecified => Fill::Solid(crate::color::Rgba::BLACK),
        };
        let stroke = match element.stroke {
            SvgPaint::Solid(color) => Some(Stroke {
                color,
                width: element.stroke_width.max(0.25),
                cap: match element.stroke_cap.as_deref() {
                    Some("round") => Cap::Round,
                    Some("square") => Cap::Square,
                    _ => Cap::Butt,
                },
                join: match element.stroke_join.as_deref() {
                    Some("round") => Join::Round,
                    Some("bevel") => Join::Bevel,
                    _ => Join::Miter,
                },
                dash: None,
            }),
            _ => None,
        };
        let shape = Shape::new(element.geom, Style { fill, stroke });
        let b = shape.world_bbox();
        bounds = Some(bounds.map_or(b, |old| old.union(b)));
        doc.layers[0].kind.shapes_mut().unwrap().push(shape);
    }
    let bounds = bounds.ok_or("SVG has no supported geometry")?;
    let mut board = crate::document::Artboard::new(
        0,
        bounds.min,
        crate::geom::Pt::new(bounds.width().max(1.0), bounds.height().max(1.0)),
    );
    board.name = "SVG".into();
    doc.width = board.size.x;
    doc.height = board.size.y;
    doc.artboards.push(board);
    Ok(doc)
}

fn check_geometry(geom: &crate::geom::Geom, budget: &mut usize) -> Result<(), String> {
    use crate::geom::Geom;
    let count = match geom {
        Geom::Polygon { sides, .. } => *sides as usize,
        Geom::Star { points, .. } => (*points as usize).saturating_mul(2),
        Geom::Path { anchors, .. } => anchors.len().saturating_mul(24),
        Geom::Poly { contours, .. } => contours.iter().map(Vec::len).sum(),
        Geom::Text(run) => run
            .content
            .chars()
            .count()
            .saturating_mul(100)
            .max(run.contours.iter().map(Vec::len).sum()),
        _ => 96,
    };
    *budget = budget.saturating_add(count);
    if *budget > 250_000 {
        return Err("Too much vector detail for a library preview.".into());
    }
    let b = geom.bbox();
    if [b.min.x, b.min.y, b.max.x, b.max.y]
        .iter()
        .any(|n| !n.is_finite() || n.abs() > 10_000_000.0)
    {
        return Err("Asset contains invalid or excessively large geometry.".into());
    }
    Ok(())
}

fn check_filters(stack: &crate::filter::FilterStack, budget: &mut usize) -> Result<(), String> {
    use crate::filter::Fx;
    *budget = budget.saturating_add(stack.items.len());
    if *budget > 64 {
        return Err("Project contains too many effects for a library preview.".into());
    }
    for effect in &stack.items {
        let valid = match effect {
            Fx::Blur { std } => std.is_finite() && std.abs() <= 512.0,
            Fx::Shadow { blur, dx, dy, .. } | Fx::InnerShadow { blur, dx, dy, .. } => {
                blur.is_finite()
                    && blur.abs() <= 512.0
                    && dx.abs() <= 100_000.0
                    && dy.abs() <= 100_000.0
            }
            Fx::Morphology { radius, .. } => radius.is_finite() && radius.abs() <= 32.0,
            Fx::Turbulence { octaves, .. } => *octaves <= 8,
            _ => true,
        };
        if !valid {
            return Err("Project effect settings exceed the library preview limits.".into());
        }
    }
    Ok(())
}

fn oma_document(bytes: &[u8]) -> Result<crate::document::Document, String> {
    #[derive(Deserialize)]
    struct Project {
        version: u32,
        doc: crate::document::Document,
    }
    let Project { version, mut doc } = serde_json::from_slice(bytes)
        .map_err(|e| format!("Could not read project preview: {e}"))?;
    if !(1..=crate::project::VERSION).contains(&version) {
        return Err("Unsupported omadesign project version.".into());
    }
    if doc.layers.len() > 256 || doc.artboards.len() > 128 {
        return Err("Project has too many layers or artboards for a preview.".into());
    }
    crate::typography::validate_document_fonts(&doc)?;
    let mut pixels = 0;
    let mut geometry = 0;
    let mut effects = 0;
    for layer in &mut doc.layers {
        check_filters(&layer.filters, &mut effects)?;
        for px in [layer.kind.pixels_mut(), layer.mask.as_mut()]
            .into_iter()
            .flatten()
        {
            pixels += check_pixels(px.w, px.h)?;
            if pixels > MAX_PIXELS {
                return Err("Project raster layers exceed the 16 megapixel preview budget.".into());
            }
            if px.data.len() as u64 != u64::from(px.w) * u64::from(px.h) * 4 {
                use base64::Engine;
                let packed = base64::engine::general_purpose::STANDARD
                    .decode(&px.data)
                    .map_err(|e| e.to_string())?;
                let image = decode_image(&packed)?.to_rgba8();
                if image.dimensions() != (px.w, px.h) {
                    return Err("Project raster dimensions do not match its image.".into());
                }
                px.data = image.into_raw();
            }
        }
        if let Some(shapes) = layer.kind.shapes_mut() {
            if shapes.len() > 2048 {
                return Err("Project has too many objects for a preview.".into());
            }
            for shape in shapes {
                check_filters(&shape.filters, &mut effects)?;
                check_geometry(&shape.geom, &mut geometry)?;
                crate::text::fill_contours(&mut shape.geom);
            }
        }
    }
    Ok(doc)
}

fn render_document(doc: &crate::document::Document, edge: u32) -> Result<RgbaImage, String> {
    use crate::geom::{Bounds, Pt};
    let bounds = doc
        .artboards
        .iter()
        .map(crate::document::Artboard::bounds)
        .reduce(|a, b| a.union(b))
        .unwrap_or_else(|| Bounds::from_min_size(Pt::ZERO, Pt::new(doc.width, doc.height)));
    let width = bounds.width();
    let height = bounds.height();
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err("Asset has invalid page dimensions.".into());
    }
    let scale = (edge as f32 - 12.0) / width.max(height);
    let view = crate::compositor::View {
        scale,
        offset: Pt::splat(edge as f32 * 0.5) - bounds.center() * scale,
    };
    let pm =
        crate::compositor::render_view(doc, view, edge, edge, crate::compositor::Draft::none())
            .ok_or("Could not render library preview")?;
    let data = pm
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let p = pixel.demultiply();
            [p.red(), p.green(), p.blue(), p.alpha()]
        })
        .collect();
    RgbaImage::new(edge, edge, data).ok_or("Invalid project preview".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "oma-brand-test-{}-{}",
                std::process::id(),
                crate::document::next_id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn png(path: &Path, width: u32, height: u32) {
        image::RgbaImage::from_pixel(width, height, image::Rgba([215, 38, 64, 255]))
            .save(path)
            .unwrap();
    }

    #[test]
    fn nearest_bank_indexes_nested_assets_and_filters_without_decoding() {
        let temp = Temp::new();
        let project = temp.0.join("Campaign");
        let bank = create(&project, "Studio North").unwrap();
        let nested = project.join("drafts/posters");
        fs::create_dir_all(&nested).unwrap();
        let logos = bank.root.join("Logos");
        fs::create_dir(&logos).unwrap();
        // An invalid image still indexes: scan must not decode on the UI path.
        fs::write(logos.join("Coral Mark.PNG"), b"not decoded during scan").unwrap();
        fs::write(bank.root.join("notes.txt"), "private notes").unwrap();
        fs::write(bank.root.join("converter.pdf"), b"%PDF").unwrap();
        let document = nested.join("unsaved-name.oma");
        assert_eq!(discover(Some(&document), None), Some(project.join(FOLDER)));
        assert!(discover(None, None).is_none());
        let nearer = create(&project.join("drafts"), "Draft bank").unwrap();
        assert_eq!(discover(Some(&document), None), Some(nearer.root));
        assert_eq!(
            discover(Some(&document), Some(&bank.root)),
            Some(bank.root.clone())
        );
        let scan = scan(&project).unwrap();
        assert_eq!(scan.name, "Studio North");
        assert_eq!(scan.assets.len(), 1);
        let asset = &scan.assets[0];
        assert_eq!(asset.relative_path, PathBuf::from("Logos/Coral Mark.PNG"));
        assert!(asset.matches("LOGOS coral png"));
        assert!(!asset.matches("coral blue"));
        assert_eq!(
            asset.fingerprint(),
            super::scan(&project).unwrap().assets[0].fingerprint()
        );
        assert!(load_thumbnail(&scan.root, asset, 128).is_err());
        fs::write(bank.root.join("brand.json"), "{broken").unwrap();
        let scan = super::scan(&bank.root).unwrap();
        assert_eq!(scan.assets.len(), 1);
        assert_eq!(scan.warnings.len(), 1);
    }

    #[test]
    fn imports_are_unique_rollback_on_failure_and_export_is_portable() {
        let temp = Temp::new();
        let source = temp.0.join("mark.png");
        png(&source, 20, 10);
        let bank = create(&temp.0.join("Project"), "A / B").unwrap();
        let copied = add_files(&bank.root, &[source.clone(), source.clone()]).unwrap();
        assert_eq!(
            copied,
            [PathBuf::from("mark.png"), PathBuf::from("mark (2).png")]
        );
        let unsupported = temp.0.join("notes.txt");
        fs::write(&unsupported, "untouched").unwrap();
        assert!(add_files(&bank.root, &[source, unsupported]).is_err());
        assert_eq!(scan(&bank.root).unwrap().assets.len(), 2);
        assert!(!bank.root.join("mark (3).png").exists());
        save_name(&bank.root, "Renamed studio").unwrap();
        let destination = temp.0.join("Export");
        fs::create_dir(&destination).unwrap();
        let exported = export_copy_bank(&bank.root, &destination).unwrap();
        let catalog = scan(&exported).unwrap();
        assert_eq!(catalog.name, "Renamed studio");
        assert_eq!(catalog.assets.len(), 2);
        assert_eq!(
            fs::read(exported.join("mark.png")).unwrap(),
            fs::read(bank.root.join("mark.png")).unwrap()
        );
        assert!(export_copy_bank(&bank.root, &destination).is_err());
        assert!(export_copy_bank(&bank.root, &bank.root).is_err());
        assert!(save_name(&bank.root, "\n").is_err());
        assert_eq!(scan(&bank.root).unwrap().name, "Renamed studio");
    }

    #[test]
    fn native_assets_carry_font_dependencies_and_bank_exports_keep_the_role_manifest() {
        use crate::document::{Artboard, Document, Layer, Shape, Style};
        use crate::geom::{Geom, Pt, TypeRun};
        let temp = Temp::new();
        let source = temp.0.join("Source");
        fs::create_dir(&source).unwrap();
        let installed =
            crate::text::default_path().expect("a desktop font for the native text fixture");
        let mut bytes = fs::read(&installed).unwrap();
        // A valid face with unique trailing bytes avoids satisfying the import
        // through another parallel test's in-memory font registration.
        bytes.extend_from_slice(format!("brand-test-{}", crate::document::next_id()).as_bytes());
        let font = source.join("Portable.ttf");
        fs::write(&font, &bytes).unwrap();
        let kit = crate::typography::add_fonts(&source, &[font], None).unwrap();
        crate::typography::register(&kit).unwrap();
        let loaded = &kit.fonts[0];
        let mut doc = Document::new("Portable text", 1.0, 1.0, 72.0);
        doc.width = 320.0;
        doc.height = 100.0;
        doc.artboards = vec![Artboard::new(0, Pt::ZERO, Pt::new(320.0, 100.0))];
        doc.layers = vec![Layer::vector("Lettering")];
        doc.layers[0].kind.shapes_mut().unwrap().push(Shape::new(
            Geom::Text(TypeRun {
                origin: Pt::new(20.0, 60.0),
                content: "Portable glyphs".into(),
                px: 28.0,
                font: loaded.id.clone(),
                ..Default::default()
            }),
            Style::default(),
        ));
        let artwork = source.join("Lettering.oma");
        crate::project::save_to(&doc, &artwork).unwrap();
        let project = temp.0.join("Destination");
        let bank = create(&project, "Portable brand").unwrap();
        let kept = bank.root.join("keep.png");
        png(&kept, 8, 8);
        let unsupported = source.join("notes.txt");
        fs::write(&unsupported, "Not a brand asset").unwrap();
        let copied_font = bank.root.join(&loaded.file);
        assert!(add_files(&bank.root, &[artwork.clone(), unsupported]).is_err());
        assert!(!bank.root.join("Lettering.oma").exists());
        assert!(
            !copied_font.exists(),
            "failed batch retained its new font dependency"
        );
        assert!(kept.is_file());

        assert_eq!(
            add_files(&bank.root, &[artwork]).unwrap(),
            [PathBuf::from("Lettering.oma")]
        );
        assert_eq!(fs::read(&copied_font).unwrap(), bytes);
        assert!(
            !project.join(crate::typography::FILE).exists(),
            "asset import must not invent or replace roles"
        );
        fs::remove_dir_all(&source).unwrap();
        let crate::import::Imported::Document(imported) =
            load_asset(&bank.root.join("Lettering.oma")).unwrap()
        else {
            panic!("editable native artwork")
        };
        let Geom::Text(run) = &imported.layers[0].kind.shapes().unwrap()[0].geom else {
            panic!("live text")
        };
        assert_eq!(run.font, loaded.id);
        assert_eq!(run.content, "Portable glyphs");
        assert!(
            run.contours.len() > 1,
            "text was replaced by a missing-font placeholder"
        );

        crate::typography::save(&project, &kit.kit, None).unwrap();
        let manifest = fs::read(project.join(crate::typography::FILE)).unwrap();
        let exported_project = temp.0.join("Export");
        fs::create_dir(&exported_project).unwrap();
        let exported = export_copy_bank(&bank.root, &exported_project).unwrap();
        assert_eq!(
            fs::read(exported_project.join(crate::typography::FILE)).unwrap(),
            manifest
        );
        assert_eq!(fs::read(exported.join(&loaded.file)).unwrap(), bytes);
        let exported_kit = crate::typography::load(&exported_project).unwrap().unwrap();
        assert_eq!(exported_kit.kit, kit.kit);
        assert_eq!(exported_kit.fonts[0].id, loaded.id);

        let conflict = temp.0.join("Existing kit");
        fs::create_dir(&conflict).unwrap();
        fs::write(conflict.join(crate::typography::FILE), "keep existing kit").unwrap();
        assert!(export_copy_bank(&bank.root, &conflict).is_err());
        assert_eq!(
            fs::read_to_string(conflict.join(crate::typography::FILE)).unwrap(),
            "keep existing kit"
        );
        assert!(!conflict.join(FOLDER).exists());

        // Exercise a failure after the role manifest has already been copied.
        File::create(bank.root.join("too-large.bin"))
            .unwrap()
            .set_len(MAX_FILE_BYTES + 1)
            .unwrap();
        let failed = temp.0.join("Failed export");
        fs::create_dir(&failed).unwrap();
        fs::write(failed.join("keep.txt"), "untouched").unwrap();
        assert!(export_copy_bank(&bank.root, &failed).is_err());
        assert!(!failed.join(FOLDER).exists());
        assert!(!failed.join(crate::typography::FILE).exists());
        assert_eq!(
            fs::read_to_string(failed.join("keep.txt")).unwrap(),
            "untouched"
        );

        let mut missing = doc.clone();
        let Geom::Text(run) = &mut missing.layers[0].kind.shapes_mut().unwrap()[0].geom else {
            unreachable!()
        };
        run.font = format!("omatype:{:032x}", crate::document::next_id());
        let missing_path = bank.root.join("Missing-font.oma");
        fs::write(&missing_path, crate::project::encode(&missing).unwrap()).unwrap();
        assert!(
            load_asset(&missing_path)
                .err()
                .unwrap()
                .contains("missing project font")
        );
        let catalog = scan(&bank.root).unwrap();
        let asset = catalog
            .assets
            .iter()
            .find(|asset| asset.relative_path == Path::new("Missing-font.oma"))
            .unwrap();
        assert!(
            load_thumbnail(&bank.root, asset, 128)
                .unwrap_err()
                .contains("missing project font")
        );
    }

    #[test]
    fn previews_decode_pixels_native_svg_and_packed_project() {
        let temp = Temp::new();
        let bank = create(&temp.0, "Preview fixtures").unwrap();
        png(&bank.root.join("raster.png"), 80, 40);
        fs::write(bank.root.join("vector.svg"), r##"<svg viewBox="0 0 60 40"><path d="M 0 0 L 60 0 L 60 40 L 0 40 Z" fill="#D72640"/></svg>"##).unwrap();
        let mut doc = crate::document::Document::new("Packed pixels", 40.0, 20.0, 72.0);
        let pixels = doc.layers[0].kind.pixels_mut().unwrap();
        for pixel in pixels.data.as_chunks_mut::<4>().0 {
            pixel.copy_from_slice(&[215, 38, 64, 255]);
        }
        crate::project::save_to(&doc, &bank.root.join("project.oma")).unwrap();
        let catalog = scan(&bank.root).unwrap();
        assert_eq!(catalog.assets.len(), 3);
        for asset in &catalog.assets {
            let preview = load_thumbnail(&catalog.root, asset, 64).unwrap();
            assert!(preview.w <= 64 && preview.h <= 64);
            assert!(
                preview
                    .data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[0] > 180 && p[1] < 70 && p[2] < 100)
            );
            assert!(load_asset(&asset.path(&catalog.root)).is_ok());
        }
    }

    #[test]
    fn malformed_vectors_and_oversized_images_fail_before_rendering() {
        let temp = Temp::new();
        let bank = create(&temp.0, "Limits").unwrap();
        fs::write(bank.root.join("broken.svg"), "<svg><path d=\"M 0 0").unwrap();
        png(&bank.root.join("wide.png"), MAX_EDGE + 1, 1);
        let mut doc = crate::document::Document::new("Bad dimensions", 1.0, 1.0, 72.0);
        let pixels = doc.layers[0].kind.pixels_mut().unwrap();
        pixels.w = u32::MAX;
        let value = serde_json::json!({ "version": 3, "doc": doc });
        fs::write(
            bank.root.join("oversized.oma"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
        let mut effects = crate::document::Document::new("Expensive effects", 1.0, 1.0, 72.0);
        effects.layers[0]
            .filters
            .items
            .push(crate::filter::Fx::Turbulence {
                fractal: true,
                base: 0.1,
                octaves: u32::MAX,
                seed: 0,
            });
        crate::project::save_to(&effects, &bank.root.join("effects.oma")).unwrap();
        let catalog = scan(&bank.root).unwrap();
        for asset in &catalog.assets {
            assert!(
                load_thumbnail(&catalog.root, asset, 64).is_err(),
                "{}",
                asset.name
            );
            assert!(
                load_asset(&asset.path(&catalog.root)).is_err(),
                "{}",
                asset.name
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlink_loops_and_asset_escape_are_not_followed() {
        use std::os::unix::fs::symlink;
        let temp = Temp::new();
        let bank = create(&temp.0, "No linked assets").unwrap();
        let outside = temp.0.join("outside.png");
        png(&outside, 2, 2);
        symlink(&bank.root, bank.root.join("loop")).unwrap();
        symlink(&outside, bank.root.join("linked.png")).unwrap();
        png(&bank.root.join("inside.png"), 2, 2);
        let catalog = scan(&bank.root).unwrap();
        assert_eq!(catalog.assets.len(), 1);
        let mut asset = catalog.assets[0].clone();
        asset.relative_path = PathBuf::from("../outside.png");
        assert!(load_thumbnail(&bank.root, &asset, 64).is_err());
        asset.relative_path = PathBuf::from("linked.png");
        assert!(load_thumbnail(&bank.root, &asset, 64).is_err());
        assert!(add_files(&bank.root, &[bank.root.join("linked.png")]).is_err());
        symlink(&temp.0, bank.root.join("linked-folder")).unwrap();
        assert!(load_asset(&bank.root.join("linked-folder/outside.png")).is_err());
        symlink(&outside, bank.root.join("brand.json.new")).unwrap();
        fs::remove_file(bank.root.join("brand.json")).unwrap();
        fs::rename(
            bank.root.join("brand.json.new"),
            bank.root.join("brand.json"),
        )
        .unwrap();
        assert!(!scan(&bank.root).unwrap().warnings.is_empty());
        save_name(&bank.root, "Safe replace").unwrap();
        assert_eq!(scan(&bank.root).unwrap().name, "Safe replace");
        assert!(image::open(outside).is_ok());
    }
}
