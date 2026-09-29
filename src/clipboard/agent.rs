//! Broader clipboard contract for chat; the canvas reader remains unchanged.
use super::*;

#[derive(Debug)]
pub enum Content {
    Native(ClipboardContent),
    Raw { mime: String, bytes: Vec<u8> },
}

pub fn parse_text(text: &str) -> Result<ClipboardContent, String> {
    if text.len() > MAX_TEXT_BYTES && !internal_objects(text.as_bytes()) {
        return Err("Clipboard text exceeds 16 MiB".into());
    }
    let paths: Vec<_> = text
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect();
    if !paths.is_empty() && paths.iter().all(|p| p.is_absolute() && p.is_file()) {
        if paths.len() > MAX_FILES {
            return Err("Clipboard contains more than 256 files".into());
        }
        return Ok(ClipboardContent::Files(paths));
    }
    super::parse_text(text)
}

fn candidates(
    types: &[String],
    mut get: impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
) -> Result<Content, String> {
    let mut ordered = Vec::new();
    for mime in MIME_TYPES {
        if let Some(actual) = types.iter().find(|t| {
            t.eq_ignore_ascii_case(mime)
                || (!mime.contains(';')
                    && t.split(';')
                        .next()
                        .is_some_and(|base| base.eq_ignore_ascii_case(mime)))
        }) {
            ordered.push(actual.clone());
        }
    }
    // Preserve native vector/file/image priority, then all additional offered data.
    let text_at = ordered
        .iter()
        .position(|m| {
            matches!(
                m.split(';').next().unwrap_or(m),
                "text/plain" | "UTF8_STRING" | "STRING" | "TEXT"
            )
        })
        .unwrap_or(ordered.len());
    let extra: Vec<_> = types
        .iter()
        .filter(|m| m.contains('/') && !ordered.contains(m))
        .cloned()
        .collect();
    let (media, other): (Vec<_>, Vec<_>) = extra.into_iter().partition(|m| {
        m.starts_with("video/") || m.starts_with("audio/") || m == "application/pdf"
    });
    ordered.splice(text_at..text_at, media);
    // Browser rich text commonly offers text/html; retain normal short-text paste.
    ordered.extend(other);
    let mut error = None;
    for mime in ordered {
        let Some(bytes) = get(&mime)? else { continue };
        if bytes.is_empty() {
            continue;
        }
        if bytes.len() > MAX_BYTES {
            return Err("Clipboard exceeds 256 MiB".into());
        }
        let base = mime.split(';').next().unwrap_or(&mime);
        if matches!(base, "text/plain" | "UTF8_STRING" | "STRING" | "TEXT") {
            if base == "STRING" {
                return parse_text(&bytes.iter().map(|b| char::from(*b)).collect::<String>())
                    .map(Content::Native);
            }
            return parse_text(
                std::str::from_utf8(&bytes).map_err(|_| "Clipboard text is not UTF-8")?,
            )
            .map(Content::Native);
        }
        if let Some(known) = MIME_TYPES.iter().find(|known| {
            known
                .split(';')
                .next()
                .is_some_and(|m| m.eq_ignore_ascii_case(base))
        }) {
            match parse_mime(known, &bytes) {
                Ok(ClipboardContent::Empty) => {}
                Ok(c) => return Ok(Content::Native(c)),
                Err(e) if mime.contains("uri") || mime.contains("copied-files") => error = Some(e),
                Err(e) => return Err(e),
            }
        } else {
            return Ok(Content::Raw { mime, bytes });
        }
    }
    error.map_or(Ok(Content::Native(ClipboardContent::Empty)), Err)
}

/// Call on a worker, including reads from other clipboard-owning processes.
pub fn read() -> Result<Content, String> {
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            use wl_clipboard_rs::paste::{self, ClipboardType, Seat};
            if let Ok(types) = paste::get_mime_types(ClipboardType::Regular, Seat::Unspecified) {
                return candidates(&types.into_iter().collect::<Vec<_>>(), |mime| {
                    read_wayland_mime(mime).map(Some)
                });
            }
            if let Ok(list) = std::process::Command::new("wl-paste")
                .arg("--list-types")
                .output()
                && list.status.success()
            {
                let types: Vec<_> = String::from_utf8_lossy(&list.stdout)
                    .lines()
                    .map(str::to_owned)
                    .collect();
                return candidates(&types, |mime| {
                    use std::process::{Command, Stdio};
                    let mut child = Command::new("wl-paste")
                        .args(["--no-newline", "--type", mime])
                        .stdout(Stdio::piped())
                        .spawn()
                        .map_err(|e| e.to_string())?;
                    let mut output = child.stdout.take().ok_or("Clipboard pipe unavailable")?;
                    let (tx, rx) = std::sync::mpsc::channel();
                    std::thread::spawn(move || {
                        let mut bytes = vec![];
                        let result = output
                            .by_ref()
                            .take(MAX_BYTES as u64 + 1)
                            .read_to_end(&mut bytes)
                            .map(|_| bytes)
                            .map_err(|e| e.to_string());
                        let _ = tx.send(result);
                    });
                    let result = rx
                        .recv_timeout(std::time::Duration::from_secs(10))
                        .map_err(|_| "Clipboard transfer timed out".to_owned());
                    let _ = child.kill();
                    let _ = child.wait();
                    result?.map(Some)
                });
            }
        }
        use x11rb::protocol::xproto::ConnectionExt;
        let context = x11_clipboard::Context::new(None)
            .map_err(|e| format!("Could not open clipboard: {e}"))?;
        let types: Vec<String> = if let Some(atoms) = x11_targets(&context)? {
            atoms
                .into_iter()
                .filter_map(|a| {
                    context
                        .connection
                        .get_atom_name(a)
                        .ok()?
                        .reply()
                        .ok()
                        .map(|r| String::from_utf8_lossy(&r.name).into_owned())
                })
                .collect()
        } else {
            MIME_TYPES.iter().map(|s| s.to_string()).collect()
        };
        candidates(&types, |mime| {
            let atom = context.get_atom(mime).map_err(|e| e.to_string())?;
            x11_contents(&context, atom)
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        super::read().map(Content::Native)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn agent_clipboard_accepts_all_mime_types_without_changing_canvas_priority() {
        for mime in [
            "video/mp4",
            "audio/ogg",
            "application/pdf",
            "application/x-brand-data",
        ] {
            let content = candidates(&[mime.into()], |_| Ok(Some(vec![1, 2, 3]))).unwrap();
            assert!(matches!(content,Content::Raw{mime:m,bytes} if m==mime&&bytes==[1,2,3]));
        }
        let rich = candidates(&["text/html".into(), "text/plain".into()], |mime| {
            Ok(Some(if mime == "text/plain" {
                b"short text".to_vec()
            } else {
                b"<p>short text</p>".to_vec()
            }))
        })
        .unwrap();
        assert!(matches!(rich,Content::Native(ClipboardContent::Text(t)) if t=="short text"));
        let svg = candidates(&["image/svg+xml;charset=utf-8".into()], |_| {
            Ok(Some(b"<svg/>".to_vec()))
        })
        .unwrap();
        assert!(matches!(svg, Content::Native(ClipboardContent::Svg(_))));
        let offered = vec![
            "text/plain".into(),
            "image/svg+xml".into(),
            "application/pdf".into(),
        ];
        let content = candidates(&offered, |mime| {
            Ok(Some(if mime == "image/svg+xml" {
                b"<svg/>".to_vec()
            } else {
                b"fallback".to_vec()
            }))
        })
        .unwrap();
        assert!(matches!(content, Content::Native(ClipboardContent::Svg(_))));
        let path = std::env::temp_dir().join(format!(
            "omadesign-file-{}.pdf",
            crate::project::new_swap_id()
        ));
        std::fs::write(&path, b"pdf").unwrap();
        let text = path.to_str().unwrap();
        assert!(
            matches!(parse_text(text).unwrap(),ClipboardContent::Files(p) if p==[path.clone()])
        );
        assert!(super::super::parse_text(text).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
