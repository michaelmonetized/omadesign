//! Small portable libraries share one bounded serializer and guarded atomic writer.
use serde::Serialize;
use std::{
    io::{self, Write},
    path::Path,
};
struct Output(Vec<u8>);
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > crate::palette::MAX_FILE_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::other("Preset library exceeds 4 MiB"));
        }
        self.0.try_reserve(bytes.len()).map_err(io::Error::other)?;
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
/// Serialize without allocating an unbounded intermediate output.
/// Args: library value. Returns at most four MiB or an error before further growth.
pub(crate) fn encoded<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let mut output = Output(Vec::new());
    serde_json::to_writer(&mut output, value).map_err(|e| e.to_string())?;
    Ok(output.0)
}
/// Replace only the exact library version that the editor loaded.
/// Args: destination, captured file fingerprint and prepared value. Returns the new fingerprint after atomic replacement.
pub(crate) fn save<T: Serialize>(
    path: &Path,
    expected: Option<u64>,
    value: &T,
) -> Result<Option<u64>, String> {
    if crate::app::libraries::file_stamp(path)? != expected {
        return Err("The library changed on disk. Reload before saving".into());
    }
    let bytes = encoded(value)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    crate::export::write_atomic(path, &bytes, &crate::ml::NoProgress)?;
    crate::app::libraries::file_stamp(path)
}
