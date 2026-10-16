//! Count the actual serial case files and guard each write against its scheduled disk allowance.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
#[path = "storage_root.rs"]
pub(super) mod root;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Scheduled {
    case: String,
    source_sha256: String,
    temporary_bytes: u64,
    minimum_free_bytes: u64,
    files: Vec<PathBuf>,
    #[serde(default)]
    destination: Option<root::Destination>,
}

/// Read the explicitly scheduled case, without changing ordinary corpus paths.
/// Args: none. Returns the private plan when selected, or no exception.
fn scheduled() -> Result<Option<(Scheduled, String)>, String> {
    let Some(plan) = std::env::var_os("OMA_RAW_SCHEDULED_CASE_STORAGE") else {
        return Ok(None);
    };
    let plan = PathBuf::from(plan);
    if !plan.is_absolute() || !plan.starts_with("/home") {
        return Err("Scheduled case storage must remain under /home".into());
    }
    let bytes = fs::read(&plan).map_err(|e| e.to_string())?;
    let scheduled = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    Ok(Some((scheduled, format!("{:x}", Sha256::digest(&bytes)))))
}

fn approved(scheduled: &Scheduled) -> bool {
    scheduled.case == "coverage-6565"
        && scheduled.source_sha256
            == "dcc5a4abe3498e6f25e89bb491995fd12c3b669d9277c71b45b49210d3e56280"
        && scheduled.temporary_bytes == 3 * TEMP_BUDGET
        && scheduled.minimum_free_bytes == 6 * TEMP_BUDGET
        && scheduled.files.len() <= 64
}

pub(super) struct CaseStorage {
    directory: PathBuf,
    external: BTreeSet<PathBuf>,
    limit: u64,
    minimum_free: u64,
    schedule_sha256: Option<String>,
    peak: AtomicU64,
    samples: AtomicU64,
    mount: Option<root::Mounted>,
    _directory: Option<File>,
    receipts: Option<PathBuf>,
}

impl CaseStorage {
    /// Retain the default allowance or the one explicitly scheduled X2D exception.
    /// Args: exact case, original source, reference paths and newly owned scratch directory. Returns a write guard.
    pub(super) fn new(
        case: &Case,
        source: &Path,
        references: &[(&Reference, PathBuf)],
        directory: &Path,
    ) -> Result<Self, String> {
        let mut external = BTreeSet::from([source.to_path_buf()]);
        external.extend(references.iter().map(|(_, path)| path.clone()));
        let mut limit = TEMP_BUDGET;
        let mut minimum_free = 0;
        let mut schedule_sha256 = None;
        let mut destination = None;
        if let Some((scheduled, digest)) = scheduled()? {
            if scheduled.case == case.id {
                if !approved(&scheduled)
                    || case.source.sha256.as_deref() != Some(scheduled.source_sha256.as_str())
                {
                    return Err("Unrecognized scheduled case storage exception".into());
                }
                external.extend(scheduled.files);
                limit = scheduled.temporary_bytes;
                minimum_free = scheduled.minimum_free_bytes;
                schedule_sha256 = Some(digest);
                destination = scheduled.destination;
            }
        }
        let (directory, pinned, mount) = if directory.starts_with("/home") {
            if destination.is_some() {
                return Err("Scheduled Mac case cannot fall back to /home".into());
            }
            (directory.to_path_buf(), None, None)
        } else {
            let mount = root::Mounted::open(
                destination
                    .as_ref()
                    .ok_or("No scheduled external destination")?,
            )?;
            let pinned = mount.pin_directory(directory)?;
            (root::fd_path(&pinned), Some(pinned), Some(mount))
        };
        let receipts = if mount.is_some() {
            let path = PathBuf::from(
                std::env::var_os("OMA_RAW_CONSUMER_RECEIPTS")
                    .ok_or("External case needs its protected home receipt directory")?,
            );
            if !path.starts_with("/home") || path.canonicalize().map_err(|e| e.to_string())? != path
            {
                return Err(
                    "External case receipts must remain in their canonical home directory".into(),
                );
            }
            Some(path)
        } else {
            None
        };
        let storage = Self {
            directory,
            external,
            limit,
            minimum_free,
            schedule_sha256,
            peak: AtomicU64::new(0),
            samples: AtomicU64::new(0),
            mount,
            _directory: pinned,
            receipts,
        };
        if storage.available()?
            < limit
                .checked_add(minimum_free)
                .ok_or("Storage allowance overflow")?
        {
            return Err(
                "Storage pressure: full case reservation cannot retain the required free space"
                    .into(),
            );
        }
        storage.check_next(0)?;
        Ok(storage)
    }

    /// Read currently available filesystem bytes. Args: none. Returns the live statvfs count or an I/O error.
    fn available(&self) -> Result<u64, String> {
        if let Some(mount) = &self.mount {
            mount.validate()?;
        }
        let stat = rustix::fs::statvfs(&self.directory).map_err(|e| e.to_string())?;
        stat.f_bavail
            .checked_mul(stat.f_frsize)
            .ok_or("Available storage overflows".into())
    }

    /// Count original, reference and newly owned files. Args: none. Returns actual bytes, refusing links or unknown file types.
    fn used(&self) -> Result<u64, String> {
        let mut paths = self.external.clone();
        for directory in std::iter::once(&self.directory).chain(self.receipts.iter()) {
            let entries = fs::read_dir(directory).map_err(|e| e.to_string())?;
            for (index, entry) in entries.enumerate() {
                if index >= 64 {
                    return Err("Case scratch contains too many files".into());
                }
                paths.insert(entry.map_err(|e| e.to_string())?.path());
            }
        }
        paths.into_iter().try_fold(0u64, |used, path| {
            if !path.is_absolute()
                || (!path.starts_with("/home")
                    && !(self.mount.is_some() && path.parent() == Some(self.directory.as_path())))
            {
                return Err(
                    "Case file is outside its protected sources or pinned destination".into(),
                );
            }
            let size = match fs::symlink_metadata(&path) {
                Ok(meta) if meta.is_file() => meta.len(),
                Ok(_) => return Err("Case storage only accepts regular files".into()),
                Err(error)
                    if error.kind() == std::io::ErrorKind::NotFound
                        && path.parent() == Some(self.directory.as_path()) =>
                {
                    0
                }
                Err(error) => return Err(error.to_string()),
            };
            used.checked_add(size)
                .ok_or("Case storage count overflows".into())
        })
    }

    /// Check actual files and one prospective write before touching disk.
    /// Args: next byte count. Returns an error while the original and existing outputs are still intact.
    pub(super) fn check_next(&self, additional: u64) -> Result<(), String> {
        let used = self.used()?;
        admit(
            used,
            additional,
            self.available()?,
            self.limit,
            self.minimum_free,
        )?;
        self.peak.fetch_max(used, Ordering::Relaxed);
        self.samples.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Bound a codec's full output before encoding or writing it.
    /// Args: actual developed dimensions. Returns scheduling failure before the operation can exceed its case allowance.
    pub(super) fn export(&self, dimensions: (u32, u32)) -> Result<(), String> {
        let maximum = u64::from(dimensions.0)
            .checked_mul(u64::from(dimensions.1))
            .and_then(|n| n.checked_mul(9))
            .and_then(|n| n.checked_add(1024 * 1024))
            .ok_or("Export storage estimate overflows")?;
        self.check_next(maximum)
    }

    /// Copy an original into the owned case directory in guarded chunks.
    /// Args: source and newly created destination. Returns after preserving exact bytes, or fails without touching the source.
    pub(super) fn copy(&self, source: &Path, destination: &mut File) -> Result<(), String> {
        let mut input = File::open(source).map_err(|e| e.to_string())?;
        let mut buffer = [0u8; 256 * 1024];
        loop {
            let count = input.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            self.check_next(count as u64)?;
            destination
                .write_all(&buffer[..count])
                .map_err(|e| e.to_string())?;
        }
        destination.sync_all().map_err(|e| e.to_string())
    }

    /// Capture case storage before owned scratch cleanup. Args: none. Returns compact limits, peaks and current counts without paths.
    pub(super) fn report(&self) -> Value {
        json!({"temporary_limit_bytes":self.limit,"minimum_remaining_free_bytes":self.minimum_free,"schedule_sha256":self.schedule_sha256,"destination":self.mount.as_ref().map(|mount| mount.report()),"peak_actual_case_file_bytes":self.peak.load(Ordering::Relaxed),"write_checks":self.samples.load(Ordering::Relaxed),"before_owned_cleanup_bytes":self.used().ok(),"final_free_bytes":self.available().ok(),"boundary":"Serial fixture logistics only; no product image-size limit. Original and references across volumes plus owned workflow files counted; every export write guarded. SSHFS API/fsync is not physical Mac durability."})
    }
}

impl crate::ml::Progress for CaseStorage {
    fn report(&self, _: &str, _: usize, _: usize) {}
    fn cancelled(&self) -> bool {
        false
    }
    fn check(&self) -> Result<(), String> {
        self.check_next(256 * 1024)
    }
}

/// Bound one prospective write. Args: actual bytes, next write, available bytes, allowance and free-space margin. Returns refusal before growth.
fn admit(
    used: u64,
    additional: u64,
    available: u64,
    limit: u64,
    minimum_free: u64,
) -> Result<(), String> {
    if used
        .checked_add(additional)
        .is_none_or(|total| total > limit)
    {
        return Err("Case exceeds its approved temporary storage allowance".into());
    }
    let remaining = limit
        .checked_sub(used)
        .ok_or("Case storage exceeds allowance")?;
    if available
        < minimum_free
            .checked_add(remaining)
            .ok_or("Storage allowance overflow")?
    {
        return Err(
            "Storage pressure: cannot retain the remaining case allowance and required free space"
                .into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_refused_real_copy_preserves_original_reference_and_destination() {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join(".qa-runs");
        fs::create_dir_all(&parent).unwrap();
        let root = parent.join(format!(
            "raw-case-storage-{}-{}",
            std::process::id(),
            crate::document::next_id()
        ));
        fs::create_dir(&root).unwrap();
        let (source, reference, work) = (
            root.join("source"),
            root.join("reference"),
            root.join("work"),
        );
        fs::create_dir(&work).unwrap();
        fs::write(&source, b"ab").unwrap();
        fs::write(&reference, b"cde").unwrap();
        let output = work.join("copy");
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&output)
            .unwrap();
        let guard = CaseStorage {
            directory: work.clone(),
            external: BTreeSet::from([source.clone(), reference.clone()]),
            limit: 5,
            minimum_free: 0,
            schedule_sha256: None,
            peak: AtomicU64::new(0),
            samples: AtomicU64::new(0),
            mount: None,
            _directory: None,
            receipts: None,
        };
        guard.check_next(0).unwrap();
        assert!(
            guard
                .copy(&source, &mut file)
                .unwrap_err()
                .contains("allowance")
        );
        drop(file);
        assert_eq!(fs::read(&source).unwrap(), b"ab");
        assert_eq!(fs::read(&reference).unwrap(), b"cde");
        assert!(fs::read(&output).unwrap().is_empty());
        fs::remove_file(output).unwrap();
        fs::remove_dir(work).unwrap();
        fs::remove_file(source).unwrap();
        fs::remove_file(reference).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn prospective_storage_failure_does_not_exclude_original_or_reference_bytes() {
        assert!(admit(10, 10, 40, 20, 20).is_ok());
        assert!(admit(10, 11, 40, 20, 20).is_err());
        assert!(admit(10, 1, 29, 20, 20).is_err());
        assert!(admit(21, 0, 100, 20, 20).is_err());
        assert!(admit(u64::MAX, 1, u64::MAX, u64::MAX, 0).is_err());
    }
}
