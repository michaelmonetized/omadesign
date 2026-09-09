//! Layered interchange formats. Conversion notes are part of the imported document.
pub mod affinity;
pub mod cli;
pub mod openraster;
pub mod pdf;
pub mod psd;
pub mod raw;
pub mod svg;
pub mod xcf;

pub fn decode_image(bytes: &[u8]) -> Result<crate::document::Pixels, String> {
    use std::io::Cursor;
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(32768);
    limits.max_image_height = Some(32768);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let rgba = reader
        .decode()
        .map_err(|e| format!("Could not decode embedded image: {e}"))?
        .to_rgba8();
    if u64::from(rgba.width()) * u64::from(rgba.height()) > 64 * 1024 * 1024 {
        return Err("Embedded image exceeds 64 megapixels".into());
    }
    crate::document::Pixels::from_rgba(rgba.width(), rgba.height(), rgba.into_raw())
        .ok_or("Invalid embedded pixels".into())
}

pub fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    let temp = parent.join(format!(
        ".omadesign-export-{}-{}.tmp",
        std::process::id(),
        crate::document::next_id()
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        std::fs::rename(&temp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}
