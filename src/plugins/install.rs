use super::*;
use std::{
    fs,
    io::Read,
    path::{Component, Path},
};
const MAX_BUNDLE: u64 = 64 * 1024 * 1024;
pub fn root() -> PathBuf {
    crate::project::data_dir().join("plugins")
}
fn disabled(root: &Path) -> std::collections::BTreeSet<String> {
    fs::read(root.join("disabled.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
pub fn set_enabled(id: &str, enabled: bool) -> Result<(), String> {
    let root = root();
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let mut values = disabled(&root);
    if enabled {
        values.remove(id);
    } else {
        values.insert(id.into());
    }
    crate::formats::write_atomic(
        &root.join("disabled.json"),
        &serde_json::to_vec(&values).map_err(|e| e.to_string())?,
    )
}
pub fn discover() -> Catalog {
    discover_at(&root())
}
pub(super) fn discover_at(root: &Path) -> Catalog {
    let mut catalog = Catalog::default();
    let disabled = disabled(root);
    let mut entries = match fs::read_dir(root) {
        Ok(entries) => entries.filter_map(Result::ok).collect::<Vec<_>>(),
        Err(_) => return catalog,
    };
    entries.sort_by_key(|e| e.file_name());
    for entry in entries.into_iter().take(256) {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_dir() || entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path().join("main.lua");
        match inspect(&path) {
            Ok(mut plugin) => {
                plugin.enabled = !disabled.contains(&plugin.id);
                if catalog.plugins.iter().any(|p| p.id == plugin.id) {
                    catalog
                        .errors
                        .push(format!("Duplicate plugin id {}", plugin.id));
                } else {
                    catalog.plugins.push(plugin);
                }
            }
            Err(error) => catalog
                .errors
                .push(format!("{}: {error}", entry.file_name().to_string_lossy())),
        }
    }
    catalog
}
fn safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty() && path.components().all(|c| matches!(c, Component::Normal(_)))
}
fn copy_tree(source: &Path, dest: &Path, budget: &mut (u64, usize)) -> Result<(), String> {
    for entry in fs::read_dir(source).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err("Plugin bundles cannot contain symlinks".into());
        }
        budget.1 += 1;
        if budget.1 > 512 {
            return Err("Plugin bundle exceeds 512 files".into());
        }
        let target = dest.join(entry.file_name());
        if kind.is_dir() {
            fs::create_dir(&target).map_err(|e| e.to_string())?;
            copy_tree(&entry.path(), &target, budget)?;
        } else if kind.is_file() {
            budget.0 += entry.metadata().map_err(|e| e.to_string())?.len();
            if budget.0 > MAX_BUNDLE {
                return Err("Plugin bundle exceeds 64 MiB".into());
            }
            fs::copy(entry.path(), target).map_err(|e| e.to_string())?;
        } else {
            return Err("Plugin bundles contain only regular files and folders".into());
        }
    }
    Ok(())
}
pub fn install(source: &Path) -> Result<Plugin, String> {
    install_at(source, &root())
}
pub(super) fn install_at(source: &Path, root: &Path) -> Result<Plugin, String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let stage = root.join(format!(".install-{}", crate::document::next_id()));
    fs::create_dir(&stage).map_err(|e| e.to_string())?;
    let result = (|| {
        if source.is_dir() {
            copy_tree(source, &stage, &mut (0, 0))?;
        } else if source.extension().is_some_and(|s| s == "lua") {
            if fs::metadata(source).map_err(|e| e.to_string())?.len() > MAX_SCRIPT_BYTES {
                return Err("Lua source exceeds 2 MiB".into());
            }
            fs::copy(source, stage.join("main.lua")).map_err(|e| e.to_string())?;
        } else {
            let file = fs::File::open(source).map_err(|e| e.to_string())?;
            let mut archive = zip::ZipArchive::new(file)
                .map_err(|e| format!("Open a Lua file, plugin folder, or .omaplug ZIP: {e}"))?;
            if archive.len() > 512 {
                return Err("Plugin bundle exceeds 512 files".into());
            }
            let mut size = 0u64;
            for i in 0..archive.len() {
                let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
                let relative = file
                    .enclosed_name()
                    .ok_or("Plugin archive contains an unsafe path")?;
                if !safe_relative(&relative)
                    || file.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
                {
                    return Err("Plugin archive contains an unsafe path or symlink".into());
                }
                let path = stage.join(&relative);
                if file.is_dir() {
                    fs::create_dir_all(path).map_err(|e| e.to_string())?;
                    continue;
                }
                size = size.checked_add(file.size()).ok_or("Invalid bundle size")?;
                if size > MAX_BUNDLE {
                    return Err("Plugin bundle exceeds 64 MiB".into());
                }
                fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
                let mut bytes = Vec::new();
                file.by_ref()
                    .take(MAX_BUNDLE + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                if bytes.len() as u64 > MAX_BUNDLE {
                    return Err("Plugin file exceeds 64 MiB".into());
                }
                fs::write(path, bytes).map_err(|e| e.to_string())?;
            }
        }
        let mut plugin = inspect(&stage.join("main.lua"))?;
        let target = root.join(&plugin.id);
        let backup = root.join(format!(
            ".backup-{}-{}",
            plugin.id,
            crate::document::next_id()
        ));
        let exists = target.exists();
        if exists {
            fs::rename(&target, &backup).map_err(|e| e.to_string())?;
        }
        if let Err(error) = fs::rename(&stage, &target) {
            if exists {
                let _ = fs::rename(&backup, &target);
            }
            return Err(error.to_string());
        }
        plugin.path = target.join("main.lua");
        Ok(plugin)
    })();
    if stage.exists() {
        let _ = fs::remove_dir_all(&stage);
    }
    result
}
