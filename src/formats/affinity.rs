//! Affinity import through the separately installed GPL Inkscape converter.
//! No converter code is linked into Omadesign, and imports never install software.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const MAX_INPUT: u64 = 512 * 1024 * 1024;
const MAX_SVG: u64 = 256 * 1024 * 1024;
const MAX_DIAGNOSTICS: u64 = 512 * 1024;
const TIMEOUT: Duration = Duration::from_secs(60);
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// Location of the optional converter, including an explicit user override.
pub fn converter_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("OMADESIGN_AFFINITY_CONVERTER") {
        return Some(PathBuf::from(path));
    }
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
        })?;
    Some(data_home.join("omadesign/affinity-import/convert"))
}

pub fn available() -> bool {
    converter_path().is_some_and(|path| path.is_file())
}

/// Read the container header as well as its magic: Affinity brushes, macros and
/// other add-ons share the magic, but are not layered documents.
fn validate_header(path: &Path) -> Result<(), String> {
    let mut file = File::open(path).map_err(|e| format!("Could not read Affinity file: {e}"))?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    if size > MAX_INPUT {
        return Err("Affinity documents larger than 512 MiB cannot be imported yet.".into());
    }
    let mut header = [0; 16];
    file.read_exact(&mut header)
        .map_err(|_| "The Affinity file is incomplete (missing document header).".to_string())?;
    if header[..4] != [0x00, 0xff, 0x4b, 0x41] {
        return Err("This file does not contain an Affinity document.".into());
    }
    let version = u16::from_le_bytes([header[4], header[5]]);
    if version < 7 {
        return Err(format!(
            "Affinity container version {version} is not supported by the converter."
        ));
    }
    if &header[8..12] != b"nsrP" {
        return Err("This is an Affinity add-on or settings file, not a layered document. Open its source document instead.".into());
    }
    if &header[12..16] != b"#Inf" {
        return Err(
            "The Affinity document header is damaged or uses an unsupported layout.".into(),
        );
    }
    Ok(())
}

/// Returns editable SVG and diagnostics. A successful converter exit does not
/// imply complete Affinity fidelity: upstream can emit partial SVG on errors.
pub fn read(path: &Path) -> Result<(String, Vec<String>), String> {
    validate_header(path)?;
    let converter = converter_path().filter(|p| p.is_file()).ok_or_else(|| {
        "Affinity import needs the optional converter. Run scripts/setup-affinity-import.sh from the Omadesign source folder, then reopen this file.".to_string()
    })?;
    let converter = fs::canonicalize(converter)
        .map_err(|e| format!("Could not locate the Affinity converter: {e}"))?;
    let input = fs::canonicalize(path).map_err(|e| e.to_string())?;
    let scratch = Scratch::new()?;
    let output_path = scratch.0.join("document.svg");
    let error_path = scratch.0.join("diagnostics.txt");
    let output = File::create(&output_path).map_err(|e| e.to_string())?;
    let errors = File::create(&error_path).map_err(|e| e.to_string())?;
    let mut child = Command::new(converter)
        .arg(&input)
        .current_dir(&scratch.0)
        .stdin(Stdio::null())
        .stdout(output)
        .stderr(errors)
        .spawn()
        .map_err(|e| format!("Could not start the Affinity converter: {e}. Run scripts/setup-affinity-import.sh to repair it."))?;
    let start = Instant::now();
    let status = loop {
        let over_limit = fs::metadata(&output_path).is_ok_and(|m| m.len() > MAX_SVG)
            || fs::metadata(&error_path).is_ok_and(|m| m.len() > MAX_DIAGNOSTICS);
        if over_limit || start.elapsed() >= TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(if over_limit {
                "The Affinity conversion exceeded its output limit. The original file is unchanged."
            } else {
                "The Affinity conversion took longer than 60 seconds and was stopped. The original file is unchanged."
            }.into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Could not wait for the Affinity converter: {e}"));
            }
        }
    };
    let diagnostics = bounded_read(&error_path, MAX_DIAGNOSTICS)?;
    let diagnostics = String::from_utf8_lossy(&diagnostics);
    if !status.success() {
        let detail = diagnostics
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("unsupported document structure");
        return Err(format!(
            "Affinity conversion failed: {}. This converter supports a subset of Affinity; multi-spread publications and some effects are not supported.",
            detail.chars().take(400).collect::<String>()
        ));
    }
    let svg = String::from_utf8(bounded_read(&output_path, MAX_SVG)?)
        .map_err(|_| "The Affinity converter returned invalid SVG text.".to_string())?;
    let xml = roxmltree::Document::parse(&svg)
        .map_err(|e| format!("The Affinity converter returned invalid SVG: {e}"))?;
    if xml.root_element().tag_name().name() != "svg" {
        return Err("The Affinity converter did not produce an SVG document.".into());
    }
    let mut warnings = vec![
        "Affinity import converts supported objects to SVG. Native Affinity adjustments, live effects, publishing features and edit history are not fully supported; keep the original Affinity file.".to_string(),
    ];
    if !diagnostics.trim().is_empty() {
        warnings.extend(summarize_diagnostics(&diagnostics));
    }
    if xml.descendants().filter(|n| n.is_element()).any(|n| {
        n.attributes()
            .any(|a| a.name() == "label" && a.value().starts_with("-Failed-"))
    }) {
        warnings.push("Some Affinity objects could not be converted. Layers whose names start with '-Failed-' mark the missing objects.".into());
    }
    Ok((svg, warnings))
}

fn summarize_diagnostics(raw: &str) -> Vec<String> {
    let mut notes = Vec::new();
    for line in raw.lines() {
        // Python tracebacks and local installation paths are not useful in an
        // artwork import report. Preserve the actual warning/exception reason.
        if line.starts_with(char::is_whitespace) || line.starts_with("Traceback ") {
            continue;
        }
        let text = line.trim();
        if text.is_empty() {
            continue;
        }
        let note = if let Some(kind) = text.strip_prefix("Failed to convert object type ") {
            let object = match kind {
                "Rstr" => "A pixel layer",
                "ImgN" => "A placed image",
                "ShpN" | "PCrv" => "A vector object",
                "TxtA" | "TxtF" | "TxtC" => "A text object",
                _ => "An Affinity object",
            };
            format!("{object} could not be converted.")
        } else if [
            "RuntimeError:",
            "AssertionError:",
            "KeyError:",
            "ValueError:",
            "TypeError:",
            "NotImplementedError:",
        ]
        .iter()
        .any(|prefix| text.starts_with(prefix))
        {
            let reason = text
                .split_once(':')
                .map(|(_, reason)| reason.trim())
                .unwrap_or("");
            if reason.is_empty() {
                "Part of the Affinity document uses an unsupported structure.".into()
            } else {
                format!("Affinity import detail: {reason}")
            }
        } else {
            text.chars().take(800).collect()
        };
        if !notes.contains(&note) {
            notes.push(note);
        }
        if notes.len() == 20 {
            notes.push("Additional converter messages were omitted. This import is incomplete; retain the original Affinity document.".into());
            break;
        }
    }
    if notes.is_empty() {
        notes.push("The Affinity converter reported an unsupported document feature.".into());
    }
    notes
}

fn bounded_read(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let mut result = Vec::new();
    File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit + 1)
        .read_to_end(&mut result)
        .map_err(|e| e.to_string())?;
    if result.len() as u64 > limit {
        return Err("The Affinity converter exceeded its output limit.".into());
    }
    Ok(result)
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self, String> {
        let base = std::env::temp_dir();
        for _ in 0..32 {
            let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path = base.join(format!(
                "omadesign-affinity-{}-{serial}",
                std::process::id()
            ));
            let mut builder = fs::DirBuilder::new();
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
                        "Could not create an Affinity import workspace: {e}"
                    ));
                }
            }
        }
        Err("Could not create a private Affinity import workspace.".into())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn distinguishes_documents_from_addons_with_the_same_magic() {
        let scratch = Scratch::new().unwrap();
        let file = scratch.0.join("header.af");
        let mut header = [
            0, 0xff, 0x4b, 0x41, 11, 0, 4, 0, b'n', b's', b'r', b'P', b'#', b'I', b'n', b'f',
        ];
        File::create(&file).unwrap().write_all(&header).unwrap();
        assert!(validate_header(&file).is_ok());
        header[8..12].copy_from_slice(b"urBR");
        fs::write(&file, header).unwrap();
        assert!(validate_header(&file).unwrap_err().contains("add-on"));
        header[0] = 1;
        fs::write(&file, header).unwrap();
        assert!(validate_header(&file).is_err());
    }

    #[test]
    fn reports_conversion_loss_without_python_tracebacks() {
        let notes = summarize_diagnostics(
            "Failed to convert object type Rstr\nTraceback (most recent call last):\n  File \"/private/install/convert.py\", line 12\n    image = convert()\nRuntimeError: Unsupported bitmap encoding\n",
        );
        assert!(notes.iter().any(|n| n.contains("pixel layer")));
        assert!(
            notes
                .iter()
                .any(|n| n.contains("Unsupported bitmap encoding"))
        );
        assert!(
            !notes
                .iter()
                .any(|n| n.contains("/private") || n.contains("Traceback"))
        );
    }
}
