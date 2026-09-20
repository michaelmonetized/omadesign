//! Native clipboard import. Call `read` on a worker: clipboard owners are other processes.
use crate::photo::RgbaImage;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

const MAX_BYTES: usize = 256 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 16 * 1024 * 1024;
const MAX_PIXELS: u64 = 64_000_000;
const MAX_FILES: usize = 256;

fn internal_objects(bytes: &[u8]) -> bool {
    bytes.starts_with(b"omadesign-objects:") || bytes.starts_with(b"omadesign-shapes:")
}

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
    if internal_objects(text.as_bytes()) {
        return if text.len() <= MAX_BYTES {
            Ok(ClipboardContent::Text(text.to_owned()))
        } else {
            Err("Copied objects exceed 256 MiB".into())
        };
    }
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
    if !path.is_file() {
        return Err("Clipboard image must be a regular file".into());
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
    let mime = mime.split(';').next().unwrap_or(mime).trim();
    if bytes.is_empty() {
        return Ok(ClipboardContent::Empty);
    }
    if mime.starts_with("image/") && mime != "image/svg+xml" {
        return decode_image(bytes, "Pasted image".into());
    }
    let internal =
        matches!(mime, "text/plain" | "UTF8_STRING" | "STRING" | "TEXT") && internal_objects(bytes);
    if bytes.len() > MAX_BYTES || (bytes.len() > MAX_TEXT_BYTES && !internal) {
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

// File managers may provide previews, so prefer a local file. Browsers also
// advertise text/uri-list, but their remote image URL must yield to image bytes.
fn read_candidates(
    mut get: impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
) -> Result<ClipboardContent, String> {
    let mut file_error = None;
    for mime in MIME_TYPES {
        let Some(bytes) = get(mime)? else { continue };
        match parse_mime(mime, &bytes) {
            Ok(ClipboardContent::Empty) => continue,
            Ok(content) => return Ok(content),
            Err(error)
                if matches!(
                    *mime,
                    "text/uri-list" | "x-special/gnome-copied-files" | "application/x-kde4-urilist"
                ) =>
            {
                file_error = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    file_error.map_or(Ok(ClipboardContent::Empty), Err)
}

#[cfg(target_os = "linux")]
enum WaylandError {
    Unavailable(String),
    Content(String),
}

#[cfg(target_os = "linux")]
fn read_wayland() -> Result<ClipboardContent, WaylandError> {
    use wl_clipboard_rs::paste::{self, ClipboardType, Error, Seat};
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
    read_candidates(|mime| {
        let offered = types.get(mime).or_else(|| {
            types.iter().find(|offered| {
                !mime.contains(';')
                    && offered
                        .split(';')
                        .next()
                        .is_some_and(|base| base.eq_ignore_ascii_case(mime))
            })
        });
        let Some(offered) = offered else {
            return Ok(None);
        };
        read_wayland_mime(offered).map(Some)
    })
    .map_err(WaylandError::Content)
}

#[cfg(target_os = "linux")]
fn read_wayland_mime(mime: &str) -> Result<Vec<u8>, String> {
    use wl_clipboard_rs::paste::{self, ClipboardType, MimeType, Seat};
    let (mut pipe, _) = paste::get_contents(
        ClipboardType::Regular,
        Seat::Unspecified,
        MimeType::Specific(mime),
    )
    .map_err(|error| format!("Could not read clipboard: {error}"))?;
    // Bound both memory and time if a clipboard owner stops serving a transfer.
    let flags = rustix::fs::fcntl_getfl(&pipe).map_err(|e| e.to_string())?;
    rustix::fs::fcntl_setfl(&pipe, flags | rustix::fs::OFlags::NONBLOCK)
        .map_err(|e| e.to_string())?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    // Native object copies are UTF-8 text carrying compressed raster pixels and
    // may exceed ordinary text limits. Classify and enforce text limits below.
    let limit = MAX_BYTES;
    let mut bytes = Vec::new();
    let mut buffer = [0; 65536];
    loop {
        if std::time::Instant::now() >= deadline {
            return Err("Clipboard transfer timed out".into());
        }
        match pipe.read(&mut buffer) {
            Ok(0) => break,
            Ok(len) => {
                if bytes.len() + len > limit {
                    return Err("Clipboard content is too large".into());
                }
                bytes.extend_from_slice(&buffer[..len]);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(2))
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(bytes)
}

#[cfg(target_os = "linux")]
fn read_x11() -> Result<ClipboardContent, String> {
    let context =
        x11_clipboard::Context::new(None).map_err(|e| format!("Could not open clipboard: {e}"))?;
    let offered = x11_targets(&context)?;
    read_candidates(|mime| {
        let target = context.get_atom(mime).map_err(|e| e.to_string())?;
        if offered
            .as_ref()
            .is_some_and(|types| !types.contains(&target))
        {
            return Ok(None);
        }
        x11_contents(&context, target)
    })
}

#[cfg(target_os = "linux")]
fn x11_contents(context: &x11_clipboard::Context, target: u32) -> Result<Option<Vec<u8>>, String> {
    use x11rb::connection::Connection;
    use x11rb::protocol::{
        Event,
        xproto::{AtomEnum, ConnectionExt, Property},
    };
    let connection = &context.connection;
    let result = (|| -> Result<_, Box<dyn std::error::Error>> {
        let cookie = connection.convert_selection(
            context.window,
            context.atoms.clipboard,
            target,
            context.atoms.property,
            x11rb::CURRENT_TIME,
        )?;
        let sequence = cookie.sequence_number();
        cookie.check()?;
        connection.flush()?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut incremental = false;
        let mut bytes = Vec::new();
        loop {
            if std::time::Instant::now() >= deadline {
                return Err("Clipboard transfer timed out".into());
            }
            let Some((event, event_sequence)) = connection.poll_for_event_with_sequence()? else {
                // A fixed 50ms sleep per INCR chunk makes even ordinary copied
                // images take minutes. Let the owner feed data without UI work.
                std::thread::sleep(std::time::Duration::from_micros(100));
                continue;
            };
            if event_sequence < sequence {
                continue;
            }
            let read_property = match event {
                Event::SelectionNotify(event)
                    if event.requestor == context.window
                        && event.selection == context.atoms.clipboard
                        && event.target == target =>
                {
                    if event.property == 0 {
                        return Ok(None);
                    }
                    true
                }
                Event::PropertyNotify(event)
                    if incremental
                        && event.window == context.window
                        && event.atom == context.atoms.property
                        && event.state == Property::NEW_VALUE =>
                {
                    true
                }
                _ => false,
            };
            if !read_property {
                continue;
            }
            let remaining = MAX_BYTES - bytes.len();
            let reply = connection
                .get_property(
                    true,
                    context.window,
                    context.atoms.property,
                    AtomEnum::ANY,
                    0,
                    (remaining / 4 + 1) as u32,
                )?
                .reply()?;
            if reply.type_ == context.atoms.incr && !incremental {
                if reply
                    .value32()
                    .and_then(|mut values| values.next())
                    .is_some_and(|size| size as usize > MAX_BYTES)
                {
                    return Err("Clipboard content is too large".into());
                }
                incremental = true;
                connection.flush()?;
                continue;
            }
            if reply.type_ != target {
                // Legacy owners can return their original type instead of
                // refusing an unsupported target. Try the next offered format.
                return Ok(None);
            }
            if reply.bytes_after != 0 || reply.value.len() > remaining {
                return Err("Clipboard content is too large".into());
            }
            let done = !incremental || reply.value.is_empty();
            bytes.extend_from_slice(&reply.value);
            if done {
                return Ok(Some(bytes));
            }
            connection.flush()?;
        }
    })();
    result.map_err(|error| format!("Could not read clipboard: {error}"))
}

#[cfg(target_os = "linux")]
fn x11_targets(
    context: &x11_clipboard::Context,
) -> Result<Option<std::collections::HashSet<u32>>, String> {
    use x11rb::connection::Connection;
    use x11rb::protocol::{
        Event,
        xproto::{AtomEnum, ConnectionExt},
    };
    let connection = &context.connection;
    let result = (|| -> Result<_, Box<dyn std::error::Error>> {
        if connection
            .get_selection_owner(context.atoms.clipboard)?
            .reply()?
            .owner
            == 0
        {
            return Ok(Some(std::collections::HashSet::new()));
        }
        connection
            .convert_selection(
                context.window,
                context.atoms.clipboard,
                context.atoms.targets,
                context.atoms.property,
                x11rb::CURRENT_TIME,
            )?
            .check()?;
        connection.flush()?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            if std::time::Instant::now() >= deadline {
                return Err("Clipboard target discovery timed out".into());
            }
            match connection.poll_for_event()? {
                Some(Event::SelectionNotify(event))
                    if event.selection == context.atoms.clipboard
                        && event.target == context.atoms.targets =>
                {
                    if event.property == 0 {
                        return Ok(None);
                    }
                    // ICCCM TARGETS replies have type ATOM, not TARGETS. The
                    // generic x11-clipboard loader requires the requested type.
                    let reply = connection
                        .get_property(
                            true,
                            context.window,
                            event.property,
                            AtomEnum::ATOM,
                            0,
                            4096,
                        )?
                        .reply()?;
                    if reply.bytes_after != 0 {
                        return Err("Clipboard offers too many formats".into());
                    }
                    return Ok(reply.value32().map(|atoms| atoms.collect()));
                }
                Some(_) => {}
                None => std::thread::sleep(std::time::Duration::from_millis(2)),
            }
        }
    })();
    result.map_err(|error| format!("Could not inspect clipboard formats: {error}"))
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
    fn large_internal_objects_cross_native_text_limit_without_allowing_large_plain_text() {
        let plain = "A".repeat(MAX_TEXT_BYTES + 1);
        assert!(parse_text(&plain).is_err());
        assert!(parse_mime("text/plain", plain.as_bytes()).is_err());
        let payload = format!("omadesign-objects:{plain}");
        assert!(
            matches!(parse_text(&payload).unwrap(), ClipboardContent::Text(text) if text == payload)
        );
        assert!(
            matches!(parse_mime("text/plain;charset=utf-8", payload.as_bytes()).unwrap(), ClipboardContent::Text(text) if text == payload)
        );
        assert!(parse_mime("image/svg+xml", payload.as_bytes()).is_err());
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
    fn browser_remote_image_uri_yields_to_pixels_but_local_file_wins() {
        let pixels = RgbaImage::new(2, 1, [50, 100, 150, 255].repeat(2)).unwrap();
        let png = pixels.encode_png().unwrap();
        let read_offer = |uri: &str| {
            read_candidates(|mime| {
                Ok(match mime {
                    "text/uri-list" => Some(uri.as_bytes().to_vec()),
                    "image/png" => Some(png.clone()),
                    "text/plain" => Some(b"https://example.com/image.png".to_vec()),
                    _ => None,
                })
            })
        };
        assert!(
            matches!(read_offer("https://example.com/image.png").unwrap(), ClipboardContent::Image { image, .. } if image == pixels)
        );
        assert_eq!(
            read_offer("file:///tmp/image.png").unwrap(),
            ClipboardContent::Files(vec!["/tmp/image.png".into()])
        );
        assert!(
            read_candidates(|mime| Ok(
                (mime == "text/uri-list").then(|| b"file://remote/image.png".to_vec())
            ))
            .is_err()
        );
    }

    #[test]
    fn browser_remote_uri_with_plain_text_pastes_text() {
        let url = "https://example.com/image.png";
        let content = read_candidates(|mime| {
            Ok(matches!(mime, "text/uri-list" | "text/plain").then(|| url.as_bytes().to_vec()))
        })
        .unwrap();
        assert_eq!(content, ClipboardContent::Text(url.into()));
    }

    #[test]
    fn copied_files_decode_original_pixels_and_svg_without_rasterizing() {
        let root = std::env::temp_dir().join(format!(
            "omadesign-clipboard-{}-{}",
            std::process::id(),
            crate::document::next_id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let png_path = root.join("screen shot.png");
        let svg_path = root.join("vector.svg");
        let pixels = RgbaImage::new(1, 1, vec![50, 100, 150, 255]).unwrap();
        std::fs::write(&png_path, pixels.encode_png().unwrap()).unwrap();
        std::fs::write(&svg_path, "<svg xmlns=\"http://www.w3.org/2000/svg\"/>").unwrap();
        assert!(
            matches!(read_file(&png_path).unwrap(), ClipboardContent::Image { image, .. } if image == pixels)
        );
        assert!(matches!(
            read_file(&svg_path).unwrap(),
            ClipboardContent::Svg(_)
        ));
        assert_eq!(
            parse_text(png_path.to_str().unwrap()).unwrap(),
            ClipboardContent::Files(vec![png_path])
        );
        let oversized = std::fs::File::create(root.join("oversized.png")).unwrap();
        oversized.set_len(MAX_BYTES as u64 + 1).unwrap();
        assert!(
            read_file(&root.join("oversized.png"))
                .unwrap_err()
                .contains("too large")
        );
        std::fs::remove_dir_all(root).unwrap();
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
