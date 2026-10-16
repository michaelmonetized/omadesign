//! Open and place vector, raster, and convertible proprietary files.

use crate::photo::RgbaImage;
use std::path::Path;
use std::process::Command;

#[derive(Clone)]
pub enum Imported {
    Photo(Box<crate::photo::PhotoImage>),
    Document(crate::document::Document),
    Raster { name: String, image: RgbaImage },
    Svg { name: String, svg: String },
}

pub fn classify(path: &Path) -> &'static str {
    let ext = ext(path);
    if crate::formats::raw::is_extension(&ext) {
        return "raw";
    }
    match ext.as_str() {
        "oma" => "oma",
        "omamotion" => "motion-project",
        "gltf"|"glb"|"obj"|"stl"|"fbx"=>"model",
        "omaphoto" => "photo-settings",
        "svg" | "svgz" => "svg",
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tif" | "tiff" => "raster",
        "pdf" => "pdf",
        "ora" => "ora",
        "idml" => "idml",
        "xcf" => "xcf",
        "ai" => "ai",
        "eps" | "ps" => "eps",
        "psd" | "psb" => "psd",
        "af" | "affinity" | "afdesign" | "afphoto" | "afpub" | "aftemplate" | "afpackage" => {
            "affinity"
        }
        "json" | "lottie" => "lottie",
        _ => "unknown",
    }
}

pub fn open_any(path: &Path) -> Result<Imported, String> {
    open_with_progress(path, |_| {})
}

pub(crate) fn open_with_progress(
    path: &Path,
    stage: impl Fn(&'static str),
) -> Result<Imported, String> {
    stage("Reading document");
    let name = file_name(path);
    let kind = classify(path);
    if matches!(kind, "raw" | "photo-settings") {
        return crate::photo::PhotoImage::load(path).map(|photo| Imported::Photo(Box::new(photo)));
    }
    let document = |mut doc: crate::document::Document, notes: Vec<String>| {
        doc.import_notes.extend(notes);
        doc.validate_sources()?;
        doc.validate_hierarchy()?;
        Ok(Imported::Document(doc))
    };
    if kind == "oma" {
        return Ok(Imported::Document(crate::project::load_from(path)?));
    }
    if kind == "motion-project" {
        return Ok(Imported::Document(crate::motion::compositions::load_from(path)?));
    }
    if kind == "model" {
        let model=crate::scene3d::import_model(path)?;
        let mut scene=crate::scene3d::Scene::default();scene.add(model,name.clone())?;
        let mut doc=crate::document::Document::new(&name,640.,480.,96.);
        doc.layers.clear();doc.artboards.clear();doc.artboardless=true;doc.transparent=true;
        doc.motion.scene3d=Some(scene);return Ok(Imported::Document(doc));
    }
    if kind == "affinity" {
        stage("Converting Affinity artwork");
        let (svg, mut notes) = crate::formats::affinity::read(path)?;
        stage("Building editable vector objects");
        let (doc, svg_notes) = crate::formats::svg::read(&svg, &name)?;
        notes.extend(svg_notes);
        return document(doc, notes);
    }
    if kind == "lottie" {
        return Err("Use File → Import Lottie for that JSON".into());
    }
    if kind == "eps" {
        return import_eps(path, &name);
    }
    let admitted;
    let legacy;
    let bytes: &[u8] = if matches!(kind, "psd" | "raster") {
        admitted = crate::project::read_admitted(path)?;
        &admitted
    } else {legacy = read_bounded(path)?; &legacy};
    if kind == "svg" || looks_svg(&bytes) {
        stage("Reading SVG objects");
        let decoded;
        let data = if bytes.starts_with(&[0x1f, 0x8b]) {
            use std::io::Read;
            let mut output = Vec::new();
            flate2::read::GzDecoder::new(bytes)
                .take(256 * 1024 * 1024 + 1)
                .read_to_end(&mut output)
                .map_err(|e| format!("Invalid SVGZ: {e}"))?;
            if output.len() > 256 * 1024 * 1024 {
                return Err("SVGZ expands beyond 256 MiB".into());
            }
            decoded = output;
            decoded.as_slice()
        } else {
            bytes
        };
        let svg = std::str::from_utf8(data).map_err(|e| format!("Invalid SVG text: {e}"))?;
        let (doc, notes) = crate::formats::svg::read(svg, &name)?;
        return document(doc, notes);
    }
    if kind == "idml" {
        let root = path.parent().unwrap_or(Path::new(".")).canonicalize().map_err(|e|e.to_string())?;
        let (doc, notes) = crate::formats::idml::read_with_links(&bytes, &name, &|uri| {
            let candidate = match url::Url::parse(uri) { Ok(url) if url.scheme()=="file" => url.to_file_path().ok()?, Ok(_) => return None, Err(_) => root.join(uri) };
            let candidate=candidate.canonicalize().ok()?;
            if !candidate.starts_with(&root) { return None; }
            read_bounded(&candidate).ok()
        })?;
        return document(doc, notes);
    }
    if kind == "ora" {
        let (doc, notes) = crate::formats::openraster::read(&bytes, &name)?;
        return document(doc, notes);
    }
    if kind == "xcf" || crate::formats::xcf::looks_like(&bytes) {
        let (doc, notes) = crate::formats::xcf::read(&bytes, &name)?;
        return document(doc, notes);
    }
    if kind == "psd" || bytes.starts_with(b"8BPS") {
        let result = crate::formats::psd::decode(&bytes, &name)?;
        return document(result.document, result.warnings);
    }
    if kind == "pdf" || kind == "ai" || bytes.starts_with(b"%PDF-") {
        if bytes.starts_with(b"%!") {
            return import_eps(path, &name);
        }
        let (doc, mut notes) = crate::formats::pdf::read(&bytes, &name)?;
        if kind == "ai" {
            notes.insert(0,"Opened Illustrator's PDF-compatible artwork. Illustrator private data such as live effects, symbols and appearance stacks is not editable.".into());
        }
        return document(doc, notes);
    }
    if bytes.starts_with(&[0x00, 0xff, 0x4b, 0x41]) {
        stage("Converting Affinity artwork");
        let (svg, mut notes) = crate::formats::affinity::read(path)?;
        stage("Building editable vector objects");
        let (doc, more) = crate::formats::svg::read(&svg, &name)?;
        notes.extend(more);
        return document(doc, notes);
    }
    match crate::formats::decode_image(&bytes) {
        Ok(pixels) if pixels.native.is_some() => {
            let mut doc = crate::document::Document::new(&name, 1., 1., 72.);
            doc.width = pixels.w as f32;
            doc.height = pixels.h as f32;
            doc.transparent = true;
            if let Some(n)=&pixels.native {doc.color=crate::deep_color::cms::DocumentColor {model:if n.cmyk.is_some(){crate::deep_color::cms::Model::Cmyk}else{crate::deep_color::cms::Model::Rgb},depth:n.depth,icc_profile:n.icc_profile.to_vec(),intent:n.intent};}
            doc.artboards = crate::document::Artboard::tiled(1, doc.width, doc.height);
            doc.layers = vec![crate::document::Layer::placed_raster(
                &name,
                pixels,
                crate::geom::Pt::ZERO,
                crate::geom::Pt::new(doc.width, doc.height),
            )];
            document(doc, vec![])
        }
        Ok(pixels) => Ok(Imported::Raster {
            name,
            image: RgbaImage {
                w: pixels.w,
                h: pixels.h,
                data: {let allocation = std::sync::Arc::try_unwrap(pixels.data).map_err(|_| "Shared imported image storage")?;let (bytes, _memory) = allocation.into_parts(); bytes},
            },
        }),
        Err(error) if kind == "raster" => Err(error),
        Err(_) => Err(format!(
            "Cannot open {name}. Choose OMA, OMAPHOTO, camera RAW, SVG/SVGZ, PNG, JPEG, WebP, GIF, TIFF, BMP, PSD/PSB, XCF, IDML, PDF, AI, EPS, or an Affinity document."
        )),
    }
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let file =
        std::fs::File::open(path).map_err(|e| format!("Could not open {}: {e}", path.display()))?;
    if file.metadata().map_err(|e| e.to_string())?.len() > 512 * 1024 * 1024 {
        return Err("Choose a document smaller than 512 MiB".into());
    }
    let mut bytes = Vec::new();
    file.take(512 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 512 * 1024 * 1024 {
        return Err("Document exceeds 512 MiB".into());
    }
    Ok(bytes)
}

fn looks_svg(bytes: &[u8]) -> bool {
    std::str::from_utf8(&bytes[..bytes.len().min(1024)]).is_ok_and(|s| s.contains("<svg"))
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

fn import_pdf_like(path: &Path, name: &str) -> Result<Imported, String> {
    let (mut doc, notes) = crate::formats::pdf::read(&read_bounded(path)?, name)?;
    doc.import_notes = notes;
    doc.import_notes.push(
        "PostScript was converted to PDF; original PostScript layer metadata may not be retained."
            .into(),
    );
    Ok(Imported::Document(doc))
}

fn import_eps(path: &Path, name: &str) -> Result<Imported, String> {
    use std::fs::{self, File};
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    let input = fs::canonicalize(path).map_err(|e| format!("Could not open {name}: {e}"))?;
    if fs::metadata(&input).map_err(|e| e.to_string())?.len() > 512 * 1024 * 1024 {
        return Err("Choose a PostScript document smaller than 512 MiB".into());
    }
    let scratch = PostScriptScratch::new()?;
    let output = scratch.0.join("document.pdf");
    let diagnostics = scratch.0.join("diagnostics.txt");
    let mut child = Command::new("gs")
        .args(["-q", "-dSAFER", "-dBATCH", "-dNOPAUSE", "-sDEVICE=pdfwrite"])
        .arg(format!("-sOutputFile={}", output.display()))
        .arg("-f")
        .arg(&input)
        .current_dir(&scratch.0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(File::create(&diagnostics).map_err(|e| e.to_string())?)
        .spawn()
        .map_err(|e| {
            format!("Could not convert {name}: {e}. PostScript import requires Ghostscript (gs).")
        })?;
    let start = Instant::now();
    let status = loop {
        if start.elapsed() > Duration::from_secs(60)
            || fs::metadata(&output).is_ok_and(|m| m.len() > 512 * 1024 * 1024)
            || fs::metadata(&diagnostics).is_ok_and(|m| m.len() > 512 * 1024)
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err("PostScript conversion exceeded its time or output limit.".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Could not wait for PostScript conversion: {e}"));
            }
        }
    };
    if !status.success() {
        return Err(format!(
            "Ghostscript could not convert {name}. The file may be damaged or use unsupported PostScript features."
        ));
    }
    import_pdf_like(&output, name)
}

struct PostScriptScratch(std::path::PathBuf);

impl PostScriptScratch {
    fn new() -> Result<Self, String> {
        for _ in 0..32 {
            let path = std::env::temp_dir().join(format!(
                "omadesign-postscript-{}-{}",
                std::process::id(),
                crate::document::next_id()
            ));
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => {
                    return Err(format!(
                        "Could not create a PostScript import workspace: {e}"
                    ));
                }
            }
        }
        Err("Could not create a private PostScript import workspace".into())
    }
}

impl Drop for PostScriptScratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
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
        assert_eq!(classify(Path::new("a.XCF")), "xcf");
    }

    #[test]
    fn missing_affinity_file_reports_read_failure() {
        let err = match open_any(Path::new("/tmp/omadesign-no-such-file.afdesign")) {
            Err(e) => e,
            Ok(_) => panic!(),
        };
        assert!(err.contains("read Affinity"));
    }
}
