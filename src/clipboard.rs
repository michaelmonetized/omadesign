//! Native clipboard import. Call `read` on a worker: clipboard owners are other processes.
use crate::photo::RgbaImage;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

const MAX_BYTES: usize = 256 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 16 * 1024 * 1024;
const MAX_PIXELS: u64 = 64 * 1024 * 1024;
const MAX_FILES: usize = 256;

#[derive(Debug, PartialEq)]
pub enum ClipboardContent {
    Image { name: String, image: RgbaImage },
    Svg(String),
    Files(Vec<PathBuf>),
    Text(String),
    Empty,
}

// Prefer original vector/file data over preview images or a browser's URL text.
const MIME_TYPES: &[&str] = &[
    "image/svg+xml",
    "application/x-inkscape-svg",
    "x-special/gnome-copied-files",
    "application/x-kde4-urilist",
    "text/uri-list",
    "image/png",
    "image/tiff",
    "image/jpeg",
    "image/webp",
    "image/bmp",
    "image/x-bmp",
    "image/gif",
    "text/plain;charset=utf-8",
    "text/plain;charset=UTF-8",
    "UTF8_STRING",
    "text/plain",
    "STRING",
    "TEXT",
];

/// Reads the current system clipboard, never an application's previous copied selection.
pub fn read() -> Result<ClipboardContent, String> {
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            match read_wayland() {
                Ok(content) => return Ok(content),
                // Some compositors do not expose data-control. Their XWayland selection
                // remains useful; an empty Wayland selection must never use this fallback.
                Err(WaylandError::Unavailable(error)) => {
                    if std::env::var_os("DISPLAY").is_none() {
                        return Err(error);
                    }
                }
                Err(WaylandError::Content(error)) => return Err(error),
            }
        }
        read_x11()
    }
    #[cfg(not(target_os = "linux"))]
    {
        read_arboard()
    }
}

/// Classifies egui's text paste event, preserving plain text exactly as copied.
pub fn parse_text(text: &str) -> Result<ClipboardContent, String> {
    if text.len() > MAX_TEXT_BYTES {
        return Err("Clipboard text exceeds 16 MiB".into());
    }
    if text.is_empty() {
        return Ok(ClipboardContent::Empty);
    }
    let trimmed = text.trim_start_matches('\u{feff}').trim();
    if looks_like_svg(trimmed) {
        return Ok(ClipboardContent::Svg(trimmed.to_owned()));
    }
    let lines: Vec<_> = trimmed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    let file_lines = match lines.first().copied() {
        Some("copy" | "cut") => &lines[1..],
        _ => &lines[..],
    };
    if !file_lines.is_empty() && file_lines.iter().all(|line| line.starts_with("file:")) {
        return parse_file_list(trimmed);
    }
    // Some file managers expose the copied local filename as plain text only.
    let path = Path::new(trimmed);
    if !trimmed.contains(['\n', '\r'])
        && path.is_absolute()
        && is_image_path(path)
        && path.is_file()
    {
        return Ok(ClipboardContent::Files(vec![path.to_owned()]));
    }
    Ok(ClipboardContent::Text(text.to_owned()))
}

fn looks_like_svg(text: &str) -> bool {
    if !text.starts_with('<') {
        return false;
    }
    // XML declarations, comments and the DOCTYPE emitted by vector editors are valid.
    roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: 1_000_000,
            ..Default::default()
        },
    )
    .map(|doc| doc.root_element().tag_name().name() == "svg")
    .unwrap_or(false)
}

fn parse_file_list(text: &str) -> Result<ClipboardContent, String> {
    let mut paths = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') || matches!(line, "copy" | "cut") {
            continue;
        }
        let url = url::Url::parse(line).map_err(|_| "Invalid clipboard file URI")?;
        if url.scheme() != "file"
            || url.host_str().is_some_and(|host| host != "localhost")
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("Clipboard files must be local file:// URIs".into());
        }
        let path = url
            .to_file_path()
            .map_err(|_| "Invalid local clipboard file URI")?;
        if !path.is_absolute() || path.as_os_str().as_encoded_bytes().contains(&0) {
            return Err("Invalid local clipboard file path".into());
        }
        if !paths.contains(&path) {
            paths.push(path);
        }
        if paths.len() > MAX_FILES {
            return Err("Clipboard contains more than 256 files".into());
        }
    }
    if paths.is_empty() {
        Ok(ClipboardContent::Empty)
    } else {
        Ok(ClipboardContent::Files(paths))
    }
}

fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "svg" | "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tif" | "tiff"
            )
        })
}

/// Decodes a copied image file with the same limits as a clipboard image.
pub fn read_file(path: &Path) -> Result<ClipboardContent, String> {
    if !path.is_absolute() || !is_image_path(path) {
        return Err(format!(
            "Unsupported clipboard image file: {}",
            path.display()
        ));
    }
    let file =
        std::fs::File::open(path).map_err(|e| format!("Could not open {}: {e}", path.display()))?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("Clipboard image must be a regular file".into());
    }
    let is_svg = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"));
    let limit = if is_svg { MAX_TEXT_BYTES } else { MAX_BYTES };
    if metadata.len() > limit as u64 {
        return Err("Clipboard image file is too large".into());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("Clipboard image file is too large".into());
    }
    if is_svg {
        return String::from_utf8(bytes)
            .map(ClipboardContent::Svg)
            .map_err(|_| "SVG must be UTF-8 text".into());
    }
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    decode_image(&bytes, name)
}

fn decode_image(bytes: &[u8], name: String) -> Result<ClipboardContent, String> {
    if bytes.len() > MAX_BYTES {
        return Err("Clipboard image exceeds 256 MiB".into());
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let format = reader
        .format()
        .ok_or("Unsupported clipboard image format")?;
    let (width, height) = image::ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|e| format!("Invalid clipboard image: {e}"))?;
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err("Clipboard image exceeds 64 megapixels".into());
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(32768);
    limits.max_image_height = Some(32768);
    limits.max_alloc = Some(MAX_BYTES as u64);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|e| format!("Could not decode clipboard image: {e}"))?
        .to_rgba8();
    let image = RgbaImage::new(image.width(), image.height(), image.into_raw())
        .ok_or("Invalid clipboard pixels")?;
    Ok(ClipboardContent::Image { name, image })
}

fn parse_mime(mime: &str, bytes: &[u8]) -> Result<ClipboardContent, String> {
    if bytes.is_empty() {
        return Ok(ClipboardContent::Empty);
    }
    if mime.starts_with("image/") && mime != "image/svg+xml" {
        return decode_image(bytes, "Pasted image".into());
    }
    if bytes.len() > MAX_TEXT_BYTES {
        return Err("Clipboard text exceeds 16 MiB".into());
    }
    let text = if mime == "STRING" {
        // X11 STRING is ISO-8859-1, unlike UTF8_STRING and text/plain.
        bytes.iter().map(|byte| char::from(*byte)).collect()
    } else {
        String::from_utf8(bytes.to_vec()).map_err(|_| "Clipboard text is not valid UTF-8")?
    };
    match mime {
        "image/svg+xml" | "application/x-inkscape-svg" => Ok(ClipboardContent::Svg(text)),
        "text/uri-list" | "x-special/gnome-copied-files" | "application/x-kde4-urilist" => {
            parse_file_list(&text)
        }
        _ => parse_text(&text),
    }
}

#[cfg(target_os = "linux")]
enum WaylandError {
    Unavailable(String),
    Content(String),
}

#[cfg(target_os = "linux")]
fn read_wayland() -> Result<ClipboardContent, WaylandError> {
    use wl_clipboard_rs::paste::{self, ClipboardType, Error, MimeType, Seat};
    let types = match paste::get_mime_types(ClipboardType::Regular, Seat::Unspecified) {
        Ok(types) => types,
        Err(Error::ClipboardEmpty | Error::NoSeats | Error::NoMimeType) => {
            return Ok(ClipboardContent::Empty);
        }
        Err(error) => {
            return Err(WaylandError::Unavailable(format!(
                "Could not read Wayland clipboard: {error}"
            )));
        }
    };
    let Some(mime) = MIME_TYPES.iter().find(|mime| types.contains(**mime)) else {
        return Err(WaylandError::Content(
            "Clipboard has no supported image, SVG, file or text content".into(),
        ));
    };
    let (mut pipe, actual_mime) = paste::get_contents(
        ClipboardType::Regular,
        Seat::Unspecified,
        MimeType::Specific(mime),
    )
    .map_err(|error| WaylandError::Content(format!("Could not read clipboard: {error}")))?;
    // Bound both memory and time if a clipboard owner stops serving a transfer.
    let flags = rustix::fs::fcntl_getfl(&pipe).map_err(|e| WaylandError::Content(e.to_string()))?;
    rustix::fs::fcntl_setfl(&pipe, flags | rustix::fs::OFlags::NONBLOCK)
        .map_err(|e| WaylandError::Content(e.to_string()))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let limit = if mime.starts_with("image/") && *mime != "image/svg+xml" {
        MAX_BYTES
    } else {
        MAX_TEXT_BYTES
    };
    let mut bytes = Vec::new();
    let mut buffer = [0; 65536];
    loop {
        if std::time::Instant::now() >= deadline {
            return Err(WaylandError::Content("Clipboard transfer timed out".into()));
        }
        match pipe.read(&mut buffer) {
            Ok(0) => break,
            Ok(len) => {
                if bytes.len() + len > limit {
                    return Err(WaylandError::Content(
                        "Clipboard content is too large".into(),
                    ));
                }
                bytes.extend_from_slice(&buffer[..len]);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(2))
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(WaylandError::Content(error.to_string())),
        }
    }
    parse_mime(&actual_mime, &bytes).map_err(WaylandError::Content)
}

#[cfg(target_os = "linux")]
fn read_x11() -> Result<ClipboardContent, String> {
    let clipboard =
        x11_clipboard::Clipboard::new().map_err(|e| format!("Could not open clipboard: {e}"))?;
    let context = &clipboard.getter;
    for mime in MIME_TYPES {
        let target = context.get_atom(mime).map_err(|e| e.to_string())?;
        let bytes = clipboard
            .load(
                context.atoms.clipboard,
                target,
                context.atoms.property,
                std::time::Duration::from_secs(3),
            )
            .map_err(|e| format!("Could not read clipboard: {e}"))?;
        if !bytes.is_empty() {
            return parse_mime(mime, &bytes);
        }
    }
    Ok(ClipboardContent::Empty)
}

#[cfg(not(target_os = "linux"))]
fn read_arboard() -> Result<ClipboardContent, String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Could not open clipboard: {e}"))?;
    if let Ok(paths) = clipboard.get().file_list() {
        if !paths.is_empty() {
            if paths.len() > MAX_FILES || paths.iter().any(|path| !path.is_absolute()) {
                return Err("Invalid clipboard file list".into());
            }
            return Ok(ClipboardContent::Files(paths));
        }
    }
    if let Ok(image) = clipboard.get_image() {
        if image.width as u64 * image.height as u64 > MAX_PIXELS {
            return Err("Clipboard image exceeds 64 megapixels".into());
        }
        let width = u32::try_from(image.width).map_err(|_| "Invalid clipboard image width")?;
        let height = u32::try_from(image.height).map_err(|_| "Invalid clipboard image height")?;
        let image = RgbaImage::new(width, height, image.bytes.into_owned())
            .ok_or("Invalid clipboard pixels")?;
        return Ok(ClipboardContent::Image {
            name: "Pasted image".into(),
            image,
        });
    }
    match clipboard.get_text() {
        Ok(text) => parse_text(&text),
        Err(arboard::Error::ContentNotAvailable) => Ok(ClipboardContent::Empty),
        Err(error) => Err(format!("Could not read clipboard: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_svg_with_real_editor_doctype_and_preserves_text() {
        let svg = "\u{feff}<?xml version=\"1.0\"?>\n<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd\">\n<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"10\" height=\"10\"/></svg>";
        assert!(matches!(parse_text(svg).unwrap(), ClipboardContent::Svg(_)));
        for text in [
            "  hello\nworld  ",
            "<html>hello</html>",
            "https://example.com/image.png",
            "{\"shapes\":[]}",
        ] {
            assert_eq!(
                parse_text(text).unwrap(),
                ClipboardContent::Text(text.into())
            );
        }
    }

    #[test]
    fn file_lists_handle_spaces_unicode_comments_and_gnome_copy_cut() {
        for header in ["", "copy\n", "cut\n"] {
            let list = format!(
                "{header}# comment\r\nfile:///tmp/screen%20shot.png\r\nfile://localhost/tmp/%E2%98%83.svg\nfile:///tmp/screen%20shot.png\n"
            );
            let expected =
                ClipboardContent::Files(vec!["/tmp/screen shot.png".into(), "/tmp/☃.svg".into()]);
            assert_eq!(
                parse_mime("text/uri-list", list.as_bytes()).unwrap(),
                expected
            );
            assert_eq!(parse_text(&list).unwrap(), expected);
        }
    }

    #[test]
    fn file_lists_reject_remote_hosts_network_urls_and_nul_paths() {
        for uri in [
            "file://server/tmp/image.png",
            "https://example.com/image.png",
            "file:///tmp/a%00.png",
            "file:///tmp/a.png?query=1",
            "file:///tmp/a.png#fragment",
        ] {
            assert!(
                parse_mime("text/uri-list", uri.as_bytes()).is_err(),
                "{uri}"
            );
        }
    }

    #[test]
    fn image_mime_decodes_pixels_and_keeps_svg_editable() {
        let original = RgbaImage::new(2, 1, vec![255, 0, 0, 255, 0, 100, 200, 128]).unwrap();
        assert_eq!(
            parse_mime("image/png", &original.encode_png().unwrap()).unwrap(),
            ClipboardContent::Image {
                name: "Pasted image".into(),
                image: original
            }
        );
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\"/>";
        assert_eq!(
            parse_mime("image/svg+xml", svg.as_bytes()).unwrap(),
            ClipboardContent::Svg(svg.into())
        );
        assert!(parse_mime("image/png", b"broken PNG").is_err());
    }

    #[test]
    fn rejects_oversized_image_before_rgba_allocation() {
        // A tiny BMP header can declare a huge decompression target.
        let mut bmp = vec![0; 54];
        bmp[..2].copy_from_slice(b"BM");
        bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
        bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
        bmp[18..22].copy_from_slice(&32768u32.to_le_bytes());
        bmp[22..26].copy_from_slice(&32768u32.to_le_bytes());
        bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
        bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
        assert!(
            decode_image(&bmp, "Too large".into())
                .unwrap_err()
                .contains("64 megapixels")
        );
    }
}
