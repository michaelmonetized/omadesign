//! Small theme libraries use the existing guarded atomic writer, independently of egui memory.
use super::Themes;
use std::path::{Path, PathBuf};

#[derive(serde::Serialize, serde::Deserialize)]
struct File {
    version: u8,
    themes: Themes,
}

pub(super) fn path(ctx: &eframe::egui::Context) -> PathBuf {
    ctx.data(|d| d.get_temp::<PathBuf>(eframe::egui::Id::new("color-theme-file")))
        .unwrap_or_else(|| crate::palette::path().with_file_name("color-themes.json"))
}
fn validate(book: &Themes) -> Result<(), String> {
    if book.entries.len() > 256 || book.next == u64::MAX {
        return Err("Color theme library is too large".into());
    }
    let mut ids = std::collections::HashSet::new();
    for entry in &book.entries {
        if entry.id == 0
            || entry.id > book.next
            || !ids.insert(entry.id)
            || entry.colors.len() != 5
            || entry.name.trim().is_empty()
            || entry.name.chars().count() > 64
            || entry.name.chars().any(char::is_control)
        {
            return Err("Invalid color theme. Original library was preserved".into());
        }
    }
    Ok(())
}
fn read(path: &Path) -> Result<Themes, String> {
    let Some((bytes, stamp)) = crate::app::libraries::palette_bytes(path)? else {
        return Ok(Themes {
            loaded: true,
            ..Default::default()
        });
    };
    let file: File =
        serde_json::from_slice(&bytes).map_err(|e| format!("Cannot read color themes: {e}"))?;
    if file.version != 1 {
        return Err("Unsupported color theme library version; file preserved".into());
    }
    validate(&file.themes)?;
    Ok(Themes {
        stamp: Some(stamp),
        loaded: true,
        ..file.themes
    })
}

/// Read a bounded theme collection without replacing an unreadable file.
/// Args: library path. Returns loaded themes or a visible error with saving disabled.
pub(super) fn load(path: &Path) -> Themes {
    read(path).unwrap_or_else(|error| Themes {
        error,
        ..Default::default()
    })
}

/// Persist a validated theme collection with stale-file protection and atomic replacement.
/// Args: destination and loaded collection. Returns success only after writing the complete file.
pub(super) fn save(path: &Path, book: &mut Themes) -> Result<(), String> {
    if !book.loaded {
        return Err("Reload the color theme library before saving".into());
    }
    validate(book)?;
    if crate::app::libraries::file_stamp(path)? != book.stamp {
        return Err("Color themes changed on disk. Reload before saving".into());
    }
    let bytes = serde_json::to_vec(&File {
        version: 1,
        themes: book.clone(),
    })
    .map_err(|e| e.to_string())?;
    if bytes.len() > crate::palette::MAX_FILE_BYTES {
        return Err("Color theme library exceeds 4 MiB".into());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    crate::export::write_atomic(path, &bytes, &crate::ml::NoProgress)?;
    book.stamp = crate::app::libraries::file_stamp(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;
    #[test]
    fn craft_color_theme_library_preserves_corrupt_future_and_concurrently_changed_files() {
        let ctx = super::super::tests::context();
        let path = path(&ctx);
        let mut book = load(&path);
        assert!(book.loaded);
        book.next = 1;
        book.entries.push(super::super::Theme {
            id: 1,
            name: "Original".into(),
            colors: vec![Rgba::WHITE; 5],
            rule: None,
        });
        save(&path, &mut book).unwrap();
        let original = std::fs::read(&path).unwrap();
        assert!(load(&path).loaded);
        std::fs::write(
            &path,
            b"{\"version\":2,\"themes\":{\"next\":0,\"entries\":[]}}",
        )
        .unwrap();
        assert!(save(&path, &mut book).is_err());
        let future = std::fs::read(&path).unwrap();
        let mut failed = load(&path);
        assert!(!failed.loaded);
        assert!(save(&path, &mut failed).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), future);
        std::fs::write(&path, b"not JSON").unwrap();
        let mut corrupt = load(&path);
        assert!(!corrupt.loaded);
        assert!(save(&path, &mut corrupt).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"not JSON");
        std::fs::write(&path, &original).unwrap();
        let mut book = load(&path);
        book.entries[0].colors.pop();
        assert!(save(&path, &mut book).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
}
