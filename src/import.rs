//! Open and place vector, raster, and convertible proprietary files.

use crate::photo::{self, RgbaImage};
use std::path::Path;
use std::process::Command;

#[derive(Clone)]
pub enum Imported {
    Document(crate::document::Document),
    Raster { name: String, image: RgbaImage },
    Svg { name: String, svg: String },
}

pub fn classify(path: &Path) -> &'static str {
    let ext = ext(path);
    match ext.as_str() {
        "oma" => "oma",
        "svg" | "svgz" => "svg",
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tif" | "tiff" => "raster",
        "pdf" => "pdf",
        "ai" => "ai",
        "eps" | "ps" => "eps",
        "psd" | "psb" => "psd",
        "afdesign" | "afphoto" | "afpub" => "affinity",
        "json" | "lottie" => "lottie",
        _ => "unknown",
    }
}

pub fn open_any(path: &Path) -> Result<Imported, String> {
    let name = file_name(path);
    let kind = classify(path);
    match kind {
        "oma" => {
            let doc = crate::project::load_from(path)?;
            Ok(Imported::Document(doc))
        }
        "svg" => {
            let svg = std::fs::read_to_string(path).map_err(|e| format!("read failed: {e}"))?;
            Ok(Imported::Svg { name, svg })
        }
        "raster" => {
            let image = photo::load_file(path).ok_or_else(|| format!("could not decode {name}"))?;
            Ok(Imported::Raster { name, image })
        }
        "pdf" | "ai" => import_pdf_like(path, &name),
        "eps" => import_eps(path, &name),
        "psd" => import_psd(path, &name),
        "affinity" => Err(format!(
            "{name} is an Affinity file. Export SVG or PDF from Affinity, then open that."
        )),
        "lottie" => Err("use File → Import Lottie for that JSON".into()),
        _ => {
            if looks_like_svg(path) {
                let svg = std::fs::read_to_string(path).map_err(|e| format!("read failed: {e}"))?;
                return Ok(Imported::Svg { name, svg });
            }
            if let Some(image) = photo::load_file(path) {
                return Ok(Imported::Raster { name, image });
            }
            if looks_like_pdf(path) {
                return import_pdf_like(path, &name);
            }
            Err(format!(
                "can't open {name} ({kind}). Place SVG, PNG, JPEG, WebP, GIF, TIFF, BMP, PDF, AI, EPS, or PSD — or save as .oma."
            ))
        }
    }
}

pub fn support_matrix() -> &'static str {
    "\
Native: .oma, SVG, PNG, JPEG, WebP, GIF, BMP, TIFF
Converted via system tools: PDF, AI (PDF-based), EPS, PSD
Unsupported (export SVG/PDF first): Affinity .afdesign / .afphoto / .afpub, old Illustrator .ai
"
}

fn ext(path: &Path) -> String {
    path.extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn looks_like_svg(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let head = std::str::from_utf8(&bytes[..bytes.len().min(512)]).unwrap_or("");
    let t = head.trim_start();
    t.starts_with("<svg") || (t.starts_with("<?xml") && t.contains("<svg")) || t.contains("<svg")
}

fn looks_like_pdf(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    bytes.starts_with(b"%PDF")
}

fn import_pdf_like(path: &Path, name: &str) -> Result<Imported, String> {
    if let Some(svg) = run_to_temp_svg(&[
        "pdftocairo",
        "-svg",
        "-f",
        "1",
        "-l",
        "1",
        &path.to_string_lossy(),
    ]) {
        return Ok(Imported::Svg {
            name: name.to_string(),
            svg,
        });
    }
    if let Some(svg) = run_inkscape_svg(path) {
        return Ok(Imported::Svg {
            name: name.to_string(),
            svg,
        });
    }
    if let Some(image) = rasterize_with(&["pdftoppm", "-png", "-f", "1", "-l", "1", "-singlefile"], path)
    {
        return Ok(Imported::Raster {
            name: name.to_string(),
            image,
        });
    }
    Err(format!(
        "couldn't convert {name}. Install poppler (pdftocairo) or Inkscape."
    ))
}

fn import_eps(path: &Path, name: &str) -> Result<Imported, String> {
    let tmp = temp_path("pdf");
    let out = Command::new("gs")
        .args([
            "-dSAFER",
            "-dBATCH",
            "-dNOPAUSE",
            "-sDEVICE=pdfwrite",
            &format!("-sOutputFile={}", tmp.display()),
            &path.to_string_lossy(),
        ])
        .output();
    if out.map(|o| o.status.success() && tmp.exists()).unwrap_or(false) {
        let r = import_pdf_like(&tmp, name);
        let _ = std::fs::remove_file(&tmp);
        return r;
    }
    if let Some(image) = rasterize_with(&["convert", "-density", "144"], path) {
        return Ok(Imported::Raster {
            name: name.to_string(),
            image,
        });
    }
    Err(format!(
        "couldn't convert {name}. Install Ghostscript (gs) or ImageMagick."
    ))
}

fn import_psd(path: &Path, name: &str) -> Result<Imported, String> {
    if let Some(image) = rasterize_with(&["magick"], path) {
        return Ok(Imported::Raster {
            name: name.to_string(),
            image,
        });
    }
    if let Some(image) = rasterize_with(&["convert"], path) {
        return Ok(Imported::Raster {
            name: name.to_string(),
            image,
        });
    }
    Err(format!(
        "couldn't convert {name}. Install ImageMagick (`magick` / `convert`)."
    ))
}

fn run_inkscape_svg(path: &Path) -> Option<String> {
    let tmp = temp_path("svg");
    let ok = Command::new("inkscape")
        .args([
            &path.to_string_lossy(),
            "--export-type=svg",
            "--export-filename",
            &tmp.to_string_lossy(),
        ])
        .output()
        .map(|o| o.status.success() && tmp.exists())
        .unwrap_or(false);
    if !ok {
        return None;
    }
    let s = std::fs::read_to_string(&tmp).ok();
    let _ = std::fs::remove_file(&tmp);
    s
}

fn run_to_temp_svg(cmd: &[&str]) -> Option<String> {
    if cmd.is_empty() {
        return None;
    }
    let tmp = temp_path("svg");
    let mut args: Vec<String> = cmd[1..].iter().map(|s| s.to_string()).collect();
    args.push(tmp.to_string_lossy().into_owned());
    let ok = Command::new(cmd[0])
        .args(&args)
        .output()
        .map(|o| o.status.success() && tmp.exists())
        .unwrap_or(false);
    if !ok {
        return None;
    }
    let s = std::fs::read_to_string(&tmp).ok();
    let _ = std::fs::remove_file(&tmp);
    s
}

fn rasterize_with(cmd: &[&str], path: &Path) -> Option<RgbaImage> {
    if cmd.is_empty() {
        return None;
    }
    let tmp = temp_path("png");
    let mut args: Vec<String> = cmd[1..].iter().map(|s| s.to_string()).collect();
    args.push(path.to_string_lossy().into_owned());
    args.push(tmp.to_string_lossy().into_owned());
    let ok = Command::new(cmd[0])
        .args(&args)
        .output()
        .map(|o| o.status.success() && tmp.exists())
        .unwrap_or(false);
    if !ok {
        let _ = std::fs::remove_file(&tmp);
        return None;
    }
    let img = photo::load_file(&tmp);
    let _ = std::fs::remove_file(&tmp);
    img
}

fn temp_path(ext: &str) -> std::path::PathBuf {
    let n = crate::document::next_id();
    std::env::temp_dir().join(format!("omadesign-import-{n}.{ext}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_common() {
        assert_eq!(classify(Path::new("a.oma")), "oma");
        assert_eq!(classify(Path::new("a.SVG")), "svg");
        assert_eq!(classify(Path::new("a.png")), "raster");
        assert_eq!(classify(Path::new("a.pdf")), "pdf");
        assert_eq!(classify(Path::new("a.afdesign")), "affinity");
    }

    #[test]
    fn affinity_is_a_clear_error() {
        let err = match open_any(Path::new("/tmp/mark.afdesign")) {
            Err(e) => e,
            Ok(_) => panic!("expected error"),
        };
        assert!(err.contains("Affinity"), "{err}");
    }
}
