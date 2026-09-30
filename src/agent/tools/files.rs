//! Disk assets use the desktop codecs. Decoding happens on the MCP worker before
//! a revision-checked placement is queued on the editor thread.
use super::*;
use crate::{geom::Bounds, import::Imported};
use std::path::{Path, PathBuf};

pub enum Prepared {
    Import(Imported),
    Image(crate::layout_images::ImageFill),
    Read(Value),
}

pub fn path(args: &Value) -> Result<PathBuf, String> {
    let value = text(args, "path", "")?;
    let path = if let Some(relative) = value.strip_prefix("~/") {
        PathBuf::from(std::env::var_os("HOME").ok_or("Home directory unavailable")?).join(relative)
    } else {
        PathBuf::from(value)
    };
    if !path.is_absolute() {
        return Err(
            "Use an absolute file path (or ~/...). The project directory is in get_document."
                .into(),
        );
    }
    Ok(path)
}

pub fn regular(path: &Path, limit: u64) -> Result<(), String> {
    let metadata = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !metadata.is_file() {
        return Err("Choose a regular file".into());
    }
    if metadata.len() > limit {
        return Err(format!(
            "File exceeds the {} MiB decoding limit",
            limit / 1024 / 1024
        ));
    }
    Ok(())
}

pub fn prepare(name: &str, args: &Value) -> Option<Result<Prepared, String>> {
    match name {
        "import_file" => Some((|| {
            let path = path(args)?;
            regular(&path, 512 * 1024 * 1024)?;
            if args["destination"].as_str() == Some("photo") {
                return crate::photo::PhotoImage::load(&path)
                    .map(|p| Prepared::Import(Imported::Photo(Box::new(p))));
            }
            let imported = if crate::import::classify(&path) == "lottie" {
                let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                let import = crate::motion::import_lottie_bytes(&bytes)?;
                let mut doc = crate::document::Document::new(
                    path.file_stem().unwrap_or_default().to_string_lossy(),
                    import.width,
                    import.height,
                    96.0,
                );
                let mut layer = Layer::vector("Animation");
                *layer.kind.shapes_mut().unwrap() = import.shapes;
                doc.layers = vec![layer];
                doc.motion = import.motion;
                Imported::Document(doc)
            } else {
                crate::import::open_any(&path)?
            };
            let imported = match imported {
                Imported::Photo(photo) => {
                    Imported::Document(crate::formats::cli::photo_document(&photo)?)
                }
                other => other,
            };
            Ok(Prepared::Import(imported))
        })()),
        "set_image_fill" => Some((|| {
            let path = path(args)?;
            regular(&path, 96 * 1024 * 1024)?;
            crate::layout_images::load(&path).map(Prepared::Image)
        })()),
        "list_files" | "read_file" => Some(read(name, args).map(Prepared::Read)),
        _ => None,
    }
}

fn read(name: &str, args: &Value) -> Result<Value, String> {
    let path = path(args)?;
    if name == "list_files" {
        let query = text(args, "query", "")?.to_lowercase();
        let offset = args["offset"].as_u64().unwrap_or(0) as usize;
        let mut entries = vec![];
        let mut truncated = false;
        for entry in std::fs::read_dir(&path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.to_lowercase().contains(&query) {
                continue;
            }
            if entries.len() == 10_000 {
                truncated = true;
                break;
            }
            let metadata = entry.metadata().map_err(|e| e.to_string())?;
            entries.push(json!({"name":name,"path":entry.path(),"directory":metadata.is_dir(),"bytes":metadata.len(),"format":crate::import::classify(&entry.path())}));
        }
        entries.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        let total = entries.len();
        let shown: Vec<_> = entries.into_iter().skip(offset).take(100).collect();
        let next = offset.saturating_add(shown.len());
        return Ok(
            json!({"path":path,"entries":shown,"total":total,"next_offset":(next<total).then_some(next),"truncated":truncated}),
        );
    }
    regular(&path, 100 * 1024 * 1024)?;
    let mode = args["as"].as_str().unwrap_or("auto");
    if mode == "image"
        || (mode == "auto"
            && matches!(
                crate::import::classify(&path),
                "raster" | "raw" | "photo-settings"
            ))
    {
        let photo = crate::photo::PhotoImage::load(&path)?;
        let preview = photo.render_thumbnail(960);
        let bytes = preview.encode_png().ok_or("Could not encode preview")?;
        return Ok(
            json!({"content":[{"type":"image","mimeType":"image/png","data":base64::engine::general_purpose::STANDARD.encode(bytes)},{"type":"text","text":format!("{} · source {} × {}", path.display(),photo.full.w,photo.full.h)}]}),
        );
    }
    if !matches!(mode, "auto" | "text") {
        return Err("Choose auto, image or text".into());
    }
    let content = read_text(&path, args)?;
    Ok(json!({"path":path,"text":content}))
}

pub fn read_text(path: &Path, args: &Value) -> Result<String, String> {
    use std::io::Read;
    regular(path, 8 * 1024 * 1024)?;
    let mut content = String::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(8 * 1024 * 1024 + 1)
        .read_to_string(&mut content)
        .map_err(|e| format!("Not a readable UTF-8 text file: {e}"))?;
    if content.len() > 8 * 1024 * 1024 {
        return Err("Text file exceeds 8 MiB".into());
    }
    let line = match args.get("line") {
        Some(v) => v
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or("line must be a positive integer")?,
        None => 1,
    };
    let limit = match args.get("limit") {
        Some(v) => v.as_u64().ok_or("limit must be an integer")?,
        None => u64::MAX,
    };
    let result: String = content
        .split_inclusive('\n')
        .skip((line - 1) as usize)
        .take(limit.min(usize::MAX as u64) as usize)
        .collect();
    if serde_json::to_vec(&result)
        .map_err(|e| e.to_string())?
        .len()
        > 4 * 1024 * 1024
    {
        return Err("Text result is too large; use a smaller line/limit range".into());
    }
    Ok(result)
}

pub fn apply(
    studio: &mut Studio,
    name: &str,
    args: &Value,
    prepared: Prepared,
) -> Result<Value, String> {
    match prepared {
        Prepared::Read(result) => Ok(result),
        Prepared::Image(mut fill) => {
            let li = index(args, "layer")?;
            let id = index(args, "id")? as u64;
            let shape = object(studio, li, id)?;
            fill.fit = match args["fit"].as_str().unwrap_or("cover") {
                "cover" => crate::layout_images::ImageFit::Cover,
                "contain" => crate::layout_images::ImageFit::Contain,
                "stretch" => crate::layout_images::ImageFit::Stretch,
                _ => return Err("Unknown image fit".into()),
            };
            fill.focal = Pt::new(
                number(args, "focal_x", 0.5)?.clamp(0., 1.),
                number(args, "focal_y", 0.5)?.clamp(0., 1.),
            );
            let mut after = shape.layout.clone();
            after.image = Some(fill);
            studio.commit(Cmd::Batch(vec![Cmd::SetLayout {
                layer: li,
                id,
                before: shape.layout,
                after,
            }]));
            Ok(json!({"layer":li,"id":id,"revision":studio.canvas_gen}))
        }
        Prepared::Import(imported) => {
            let destination = args["destination"].as_str().unwrap_or("canvas");
            if destination == "photo" {
                let Imported::Photo(photo) = imported else {
                    return Err("Invalid photo import".into());
                };
                studio.photo.import_photo(*photo);
                studio.mark();
                return Ok(
                    json!({"photo":studio.photo.selected,"photo_revision":studio.photo.edit_revision,"revision":studio.canvas_gen}),
                );
            }
            if !matches!(destination, "canvas" | "frame") {
                return Err("Choose canvas, frame or photo".into());
            }
            let (width, height) = match &imported {
                Imported::Document(doc) => (doc.width, doc.height),
                Imported::Raster { image, .. } => (image.w as f32, image.h as f32),
                _ => return Err("Unsupported placement data".into()),
            };
            let scale = (studio.doc.width * 0.8 / width)
                .min(studio.doc.height * 0.8 / height)
                .min(1.);
            let w = number(
                args,
                "width",
                args["height"]
                    .as_f64()
                    .map_or(width * scale, |h| h as f32 * width / height),
            )?;
            let h = number(args, "height", w * height / width)?;
            if w <= 0. || h <= 0. {
                return Err("Placement dimensions must be positive".into());
            }
            let dest = Bounds::from_min_size(
                Pt::new(
                    number(args, "x", (studio.doc.width - w) * 0.5)?,
                    number(args, "y", (studio.doc.height - h) * 0.5)?,
                ),
                Pt::new(w, h),
            );
            let notes = match &imported {
                Imported::Document(doc) => doc.import_notes.clone(),
                _ => vec![],
            };
            if destination == "frame" {
                let li = index(args, "layer")?;
                let id = index(args, "id")? as u64;
                object(studio, li, id)?;
                studio.place_imported_in_frame(imported, dest.center(), (li, id), Some(dest))?;
            } else {
                studio.place_imported_at(
                    imported,
                    dest.center(),
                    Some((
                        Bounds::from_min_size(Pt::ZERO, Pt::new(width, height)),
                        dest,
                    )),
                )?;
            }
            Ok(
                json!({"selection":studio.selection,"layers":studio.doc.layers.len(),"notes":notes,"revision":studio.canvas_gen,"tool":name}),
            )
        }
    }
}

pub fn output_path(args: &Value) -> Result<PathBuf, String> {
    let path = path(args)?;
    if path.exists() && args["overwrite"].as_bool() != Some(true) {
        return Err("Destination exists. Choose a new filename, or set overwrite:true when the user requested replacement.".into());
    }
    if path.exists() && !path.is_file() {
        return Err("Destination must be a regular file".into());
    }
    if !path.parent().is_some_and(Path::is_dir) {
        return Err("Destination directory does not exist".into());
    }
    Ok(path)
}
