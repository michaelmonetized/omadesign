//! No-replace publication stays bound to the chosen destination directories.
use rustix::{
    fs::{self, AtFlags, Mode, OFlags},
    io::Errno,
};
use std::{collections::BTreeMap, fs::File, path::Path};

struct Directory {
    file: File,
    created: Option<(u64, u64)>,
}
pub(super) struct Destinations {
    root: File,
    root_path: std::path::PathBuf,
    directories: BTreeMap<String, Directory>,
    published: Vec<(String, String, (u64, u64))>,
    committed: bool,
}
fn identity(stat: &fs::Stat) -> (u64, u64) {
    (stat.st_dev as u64, stat.st_ino as u64)
}
fn at_identity(dir: &File, name: &str) -> Option<(u64, u64)> {
    fs::statat(dir, name, AtFlags::SYMLINK_NOFOLLOW)
        .ok()
        .map(|s| identity(&s))
}
impl Destinations {
    /// Bind the chosen root and real one-level subfolders before encoding.
    /// Args: user-selected root and validated folder components. Returns held directories with owned cleanup.
    pub fn new<'a>(root: &Path, folders: impl Iterator<Item = &'a str>) -> Result<Self, String> {
        let root_path = root.to_path_buf();
        let root = File::from(
            fs::open(
                root,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|e| e.to_string())?,
        );
        let mut output = Self {
            root,
            root_path,
            directories: BTreeMap::new(),
            published: vec![],
            committed: false,
        };
        output.directories.insert(
            String::new(),
            Directory {
                file: output.root.try_clone().map_err(|e| e.to_string())?,
                created: None,
            },
        );
        for name in folders.filter(|name| !name.is_empty()) {
            if output.directories.contains_key(name) {
                continue;
            }
            if !super::filename_part(name, false) {
                return Err("Export subfolders must be single portable names".into());
            }
            let created = match fs::mkdirat(&output.root, name, Mode::from_raw_mode(0o777)) {
                Ok(()) => at_identity(&output.root, name),
                Err(Errno::EXIST) => None,
                Err(e) => return Err(format!("Cannot create export subfolder {name}: {e}")),
            };
            match fs::openat(
                &output.root,
                name,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            ) {
                Ok(fd) => {
                    output.directories.insert(
                        name.into(),
                        Directory {
                            file: File::from(fd),
                            created,
                        },
                    );
                }
                Err(e) => {
                    if created.is_some() && at_identity(&output.root, name) == created {
                        let _ = fs::unlinkat(&output.root, name, AtFlags::REMOVEDIR);
                    }
                    return Err(format!(
                        "Export subfolder {name} must be a real directory, not a link or file: {e}"
                    ));
                }
            }
        }
        Ok(output)
    }

    fn directory(&self, folder: &str) -> Result<&Directory, String> {
        let dir = self
            .directories
            .get(folder)
            .ok_or("Missing export directory")?;
        if fs::stat(&self.root_path).ok().map(|s| identity(&s))
            != Some(identity(&fs::fstat(&self.root).map_err(|e| e.to_string())?))
        {
            return Err("Export destination changed during export".into());
        }
        if !folder.is_empty()
            && at_identity(&self.root, folder)
                != Some(identity(&fs::fstat(&dir.file).map_err(|e| e.to_string())?))
        {
            return Err("Export subfolder changed during export".into());
        }
        Ok(dir)
    }
    /// Check the actual held directory without following a destination link.
    /// Args: validated folder and filename. Returns whether any existing entry would collide.
    pub fn exists(&self, folder: &str, name: &str) -> Result<bool, String> {
        let dir = self.directory(folder)?;
        match fs::statat(&dir.file, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(_) => Ok(true),
            Err(Errno::NOENT) => Ok(false),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Link an owned staged file without replacing any existing output.
    /// Args: staged path, held folder and validated filename. Returns after publication and records its identity.
    pub fn publish(&mut self, source: &Path, folder: &str, name: &str) -> Result<(), String> {
        let dir = self.directory(folder)?;
        let parent = File::from(
            fs::open(
                source.parent().ok_or("Missing staging directory")?,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|e| e.to_string())?,
        );
        let source_name = source.file_name().ok_or("Missing staged filename")?;
        let original = fs::statat(&parent, source_name, AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|e| e.to_string())?;
        fs::linkat(&parent, source_name, &dir.file, name, AtFlags::empty())
            .map_err(|e| format!("Cannot publish {name} without replacing existing files: {e}"))?;
        self.published
            .push((folder.into(), name.into(), identity(&original)));
        Ok(())
    }
    pub fn commit(&mut self) {
        self.committed = true;
    }
}
impl Drop for Destinations {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        for (folder, name, original) in self.published.iter().rev() {
            let dir = &self.directories[folder];
            if at_identity(&dir.file, name) == Some(*original) {
                let _ = fs::unlinkat(&dir.file, name.as_str(), AtFlags::empty());
            }
        }
        for (name, dir) in &self.directories {
            if let Some(original) = dir.created
                && at_identity(&self.root, name) == Some(original)
            {
                let _ = fs::unlinkat(&self.root, name.as_str(), AtFlags::REMOVEDIR);
            }
        }
    }
}
