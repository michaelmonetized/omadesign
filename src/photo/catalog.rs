//! Durable photo metadata, independent of pixels and optional source sidecars.
//!
//! `catalog.jsonl` contains a version header and SHA-256 checked, complete entry
//! replacements. A checkpoint atomically replaces that one file, so no snapshot
//! and journal pair can disagree after a crash. Damaged originals are preserved
//! before recovery writes anything. Use the mutation methods to persist changes;
//! the public entry map is available for browsing, not unsaved editing.
//! Own catalogs on background jobs: opening, importing, editing metadata,
//! relinking, loading and checkpointing synchronously access storage.
//! Bump the format version when serialized entry or development fields change.
use super::{DevelopParams, PhotoImage, edits};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions, ReadDir};
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const FORMAT_VERSION: u32 = 1;
pub const JOURNAL_FILE: &str = "catalog.jsonl";
pub const MAX_ENTRIES: usize = 100_000;
pub const MAX_METADATA_BYTES: usize = 256 * 1024 * 1024;
const FORMAT: &str = "omadesign-photo-catalog";
const LOCK_FILE: &str = ".catalog.lock";
const MAX_ENTRY_BYTES: usize = 3 * 1024 * 1024;
const MAX_RECORD_BYTES: usize = 4 * 1024 * 1024;
const MAX_JOURNAL_BYTES: u64 = 1024 * 1024 * 1024;
const CHECKPOINT_BYTES: u64 = 512 * 1024 * 1024;
const MAX_DEPTH: usize = 128;
const MAX_ERRORS: usize = 256;

/// One imported original and its saved adjustments.
///
/// IDs belong to imports, while `content_sha256` belongs to bytes. Identical
/// copies at different paths therefore remain separate, independently editable
/// entries. `source_identity` records the metadata used for source-bound sidecars.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogEntry {
    pub id: String,
    pub path: PathBuf,
    pub name: String,
    pub content_sha256: String,
    pub source_identity: edits::SourceIdentity,
    pub settings_path: PathBuf,
    pub rating: u8,
    pub keywords: BTreeSet<String>,
    pub stack: Option<String>,
    pub develop: DevelopParams,
}

/// Combined browsing criteria; empty fields match every entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CatalogFilter {
    pub min_rating: u8,
    pub keyword: Option<String>,
    pub stack: Option<String>,
    pub text: String,
}

/// Import results, including bounded error details and the complete error count.
#[derive(Clone, Debug, Default)]
pub struct ImportReport {
    pub imported: usize,
    pub skipped: usize,
    pub error_count: usize,
    pub errors: Vec<String>,
    pub stopped: bool,
}

impl ImportReport {
    /// Record an import error. `message` is retained within a bounded detail list;
    /// the total count still includes errors whose details cannot be retained.
    fn error(&mut self, message: String) {
        self.error_count += 1;
        if self.errors.len() < MAX_ERRORS {
            let end = message
                .char_indices()
                .map(|(index, _)| index)
                .find(|index| *index >= 2048)
                .unwrap_or(message.len());
            self.errors.push(message[..end].to_owned());
        }
    }
}

#[derive(Debug)]
pub struct Catalog {
    pub entries: BTreeMap<String, CatalogEntry>,
    pub recovery_notes: Vec<String>,
    directory: PathBuf,
    metadata_bytes: usize,
    journal_bytes: u64,
    needs_reopen: bool,
    _lock: File,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    format: String,
    version: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u32,
    sha256: String,
    entry: CatalogEntry,
}

#[derive(Deserialize)]
struct VersionProbe {
    version: u64,
    format: Option<String>,
}

struct LogLine {
    bytes: Vec<u8>,
    oversized: bool,
    terminated: bool,
    read: u64,
}

#[derive(Default)]
struct Recovered {
    entries: BTreeMap<String, CatalogEntry>,
    metadata_bytes: usize,
    journal_bytes: u64,
    damaged: usize,
    notes: Vec<String>,
}

impl Catalog {
    /// Locate catalog storage. Takes no arguments; returns the canonical
    /// directory supplied to `open`, suitable for reopening on a background job.
    pub fn path(&self) -> &Path {
        &self.directory
    }

    /// Open a durable catalog. `directory` holds its journal and writer lock;
    /// returns the exclusively locked catalog or an error without discarding data.
    pub fn open(directory: impl AsRef<Path>) -> Result<Self, String> {
        let directory = directory.as_ref();
        scan_log(&directory.join(JOURNAL_FILE), |_, line| {
            reject_newer(&line.bytes)
        })?;
        fs::create_dir_all(directory).map_err(storage_error)?;
        let directory = directory.canonicalize().map_err(storage_error)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join(LOCK_FILE))
            .map_err(storage_error)?;
        lock.try_lock().map_err(|error| {
            format!("The photo catalog is already open, or cannot be locked: {error}")
        })?;
        let journal = directory.join(JOURNAL_FILE);
        let existed = journal.try_exists().map_err(storage_error)?;
        let recovered = recover(&journal)?;
        let mut catalog = Self {
            entries: recovered.entries,
            recovery_notes: recovered.notes,
            directory,
            metadata_bytes: recovered.metadata_bytes,
            journal_bytes: recovered.journal_bytes,
            needs_reopen: false,
            _lock: lock,
        };
        if recovered.damaged > 0 {
            let backup = catalog.preserve_damaged_log()?;
            catalog.recovery_notes.push(format!(
                "Skipped {} damaged catalog records. Original bytes preserved in {}.",
                recovered.damaged,
                backup.display()
            ));
            catalog.checkpoint()?;
        } else if !existed {
            catalog.checkpoint()?;
        }
        Ok(catalog)
    }

    /// Import a folder recursively. `directory` supplies originals, never pixels;
    /// returns counts and explicit errors, stopping at catalog or traversal limits.
    /// Existing canonical paths are skipped; identical copies get distinct IDs.
    pub fn import_folder(&mut self, directory: impl AsRef<Path>) -> ImportReport {
        let mut report = ImportReport::default();
        if let Err(error) = self.ensure_writable() {
            report.error(error);
            report.stopped = true;
            return report;
        }
        let directory = directory.as_ref();
        let root = (|| {
            if !fs::symlink_metadata(directory)
                .map_err(storage_error)?
                .is_dir()
            {
                return Err("Choose a regular photo folder, not a symbolic link.".into());
            }
            fs::read_dir(directory).map_err(storage_error)
        })();
        let mut folders: Vec<ReadDir> = match root {
            Ok(folder) => vec![folder],
            Err(error) => {
                report.error(format!("{}: {error}", directory.display()));
                return report;
            }
        };
        let mut known_paths: BTreeSet<PathBuf> = self
            .entries
            .values()
            .map(|entry| entry.path.clone())
            .collect();
        while let Some(folder) = folders.last_mut() {
            let entry = match folder.next() {
                Some(Ok(entry)) => entry,
                Some(Err(error)) => {
                    report.error(storage_error(error));
                    continue;
                }
                None => {
                    folders.pop();
                    continue;
                }
            };
            let path = entry.path();
            let kind = match entry.file_type() {
                Ok(kind) => kind,
                Err(error) => {
                    report.error(format!("{}: {error}", path.display()));
                    continue;
                }
            };
            if kind.is_symlink() {
                report.skipped += 1;
            } else if kind.is_dir() {
                if folders.len() >= MAX_DEPTH {
                    report.error(format!(
                        "{}: Photo folders exceed {MAX_DEPTH} levels.",
                        path.display()
                    ));
                    continue;
                }
                match fs::read_dir(&path) {
                    Ok(folder) => folders.push(folder),
                    Err(error) => report.error(format!("{}: {error}", path.display())),
                }
            } else if kind.is_file() && matches!(crate::import::classify(&path), "raster" | "raw") {
                let canonical = match path.canonicalize() {
                    Ok(path) => path,
                    Err(error) => {
                        report.error(format!("{}: {error}", path.display()));
                        continue;
                    }
                };
                if known_paths.contains(&canonical) {
                    report.skipped += 1;
                    continue;
                }
                if self.entries.len() >= MAX_ENTRIES {
                    report.error(format!(
                        "The photo catalog holds at most {MAX_ENTRIES} entries. Remaining files were not imported."
                    ));
                    report.stopped = true;
                    break;
                }
                let imported = (|| {
                    let (path, identity, content_sha256) = fingerprint(&canonical)?;
                    let develop = edits::load(&path, &identity)?.unwrap_or_default();
                    let mut id = unique_name();
                    while self.entries.contains_key(&id) {
                        id = unique_name();
                    }
                    self.upsert(CatalogEntry {
                        id,
                        name: path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        settings_path: edits::sidecar_path(&path),
                        path,
                        content_sha256,
                        source_identity: identity,
                        rating: 0,
                        keywords: BTreeSet::new(),
                        stack: None,
                        develop,
                    })
                })();
                match imported {
                    Ok(()) => {
                        known_paths.insert(canonical);
                        report.imported += 1;
                    }
                    Err(error) => {
                        report.error(format!("{}: {error}", path.display()));
                        if self.needs_reopen || self.metadata_bytes >= MAX_METADATA_BYTES {
                            report.stopped = true;
                            break;
                        }
                    }
                }
            }
        }
        report
    }

    /// Save a star rating. `id` selects an entry and `rating` is zero through five;
    /// returns only after the complete replacement is durable.
    pub fn set_rating(&mut self, id: &str, rating: u8) -> Result<(), String> {
        let mut entry = self.entry(id)?.clone();
        entry.rating = rating;
        self.upsert(entry)
    }

    /// Save keywords. `id` selects an entry; `keywords` supplies trimmed labels;
    /// returns after validation and durable storage, removing exact duplicates.
    pub fn set_keywords(
        &mut self,
        id: &str,
        keywords: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<(), String> {
        let mut entry = self.entry(id)?.clone();
        let mut labels = BTreeSet::new();
        for keyword in keywords {
            let keyword = keyword.as_ref().trim();
            validate_label(keyword)?;
            labels.insert(keyword.to_owned());
            if labels.len() > 128 {
                return Err("Use at most 128 keywords per photograph.".into());
            }
        }
        entry.keywords = labels;
        self.upsert(entry)
    }

    /// Assign a stack. `id` selects an entry and `stack` names its group or clears
    /// it with `None`; returns after the group label is durably saved.
    pub fn set_stack(&mut self, id: &str, stack: Option<String>) -> Result<(), String> {
        let mut entry = self.entry(id)?.clone();
        entry.stack = stack.map(|label| label.trim().to_owned());
        self.upsert(entry)
    }

    /// Save development settings. `id` selects an entry and `develop` supplies
    /// validated adjustments; returns after storage without changing the original.
    pub fn set_develop(&mut self, id: &str, develop: &DevelopParams) -> Result<(), String> {
        edits::validate(develop)?;
        let mut entry = self.entry(id)?.clone();
        entry.develop = develop.clone();
        self.upsert(entry)
    }

    /// Find entries. `filter` combines a minimum rating, keyword, stack and text;
    /// returns borrowed matches in ID order without reading or decoding originals.
    pub fn filter(&self, filter: &CatalogFilter) -> Vec<&CatalogEntry> {
        let text = filter.text.trim().to_lowercase();
        self.entries
            .values()
            .filter(|entry| {
                entry.rating >= filter.min_rating
                    && filter.keyword.as_ref().is_none_or(|keyword| {
                        let keyword = keyword.trim().to_lowercase();
                        entry
                            .keywords
                            .iter()
                            .any(|label| label.to_lowercase() == keyword)
                    })
                    && filter.stack.as_ref().is_none_or(|stack| {
                        entry.stack.as_ref().is_some_and(|label| label == stack)
                    })
                    && (text.is_empty()
                        || entry.name.to_lowercase().contains(&text)
                        || entry.path.to_string_lossy().to_lowercase().contains(&text)
                        || entry
                            .keywords
                            .iter()
                            .any(|keyword| keyword.to_lowercase().contains(&text)))
            })
            .collect()
    }

    /// Relink a moved original. `id` selects the entry and `path` supplies its new
    /// location; returns after matching content and saving its new sidecar identity.
    /// Modification times may change; development settings remain authoritative.
    pub fn relink(&mut self, id: &str, path: impl AsRef<Path>) -> Result<(), String> {
        let mut entry = self.entry(id)?.clone();
        let (path, identity, hash) = fingerprint(path.as_ref())?;
        if hash != entry.content_sha256 {
            return Err("This file does not match the catalog original's SHA-256 content.".into());
        }
        if self
            .entries
            .values()
            .any(|other| other.id != id && other.path == path)
        {
            return Err("This original already belongs to another catalog entry.".into());
        }
        entry.name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        entry.settings_path = edits::sidecar_path(&path);
        entry.path = path;
        entry.source_identity = identity;
        self.upsert(entry)
    }

    /// Load an original for editing. `id` selects the entry; returns a decoded
    /// photograph with catalog settings applied even when its sidecar is missing.
    pub fn load_photo(&self, id: &str) -> Result<PhotoImage, String> {
        let entry = self.entry(id)?;
        edits::validate(&entry.develop)?;
        let (path, identity, hash) = fingerprint(&entry.path)?;
        if hash != entry.content_sha256 {
            return Err(
                "The catalog original's content changed. Relink the matching original.".into(),
            );
        }
        let mut photo = PhotoImage::load(&path)?;
        if photo.source_identity.as_ref() != Some(&identity) {
            return Err("The original changed while opening. Open it again.".into());
        }
        photo.develop = entry.develop.clone();
        photo.source_identity = Some(identity);
        photo.settings_path = Some(entry.settings_path.clone());
        Ok(photo)
    }

    /// Compact the catalog. Takes no arguments; returns after a complete journal
    /// replacement and directory sync, retaining the writer lock across renames.
    pub fn checkpoint(&mut self) -> Result<(), String> {
        self.ensure_writable()?;
        if self.entries.len() > MAX_ENTRIES {
            return Err(format!("The photo catalog exceeds {MAX_ENTRIES} entries."));
        }
        let mut metadata_bytes = 0usize;
        for (id, entry) in &self.entries {
            if id != &entry.id {
                return Err("A catalog entry does not match its ID.".into());
            }
            metadata_bytes += encode_entry(entry)?.len();
            if metadata_bytes > MAX_METADATA_BYTES {
                return Err("Photo catalog metadata exceeds 256 MiB.".into());
            }
        }
        let temp = self
            .directory
            .join(format!(".catalog-checkpoint-{}", unique_name()));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(storage_error)?;
        let result = (|| {
            let mut writer = BufWriter::new(file);
            let mut bytes = serde_json::to_vec(&Header {
                format: FORMAT.into(),
                version: FORMAT_VERSION,
            })
            .map_err(encoding_error)?;
            bytes.push(b'\n');
            writer.write_all(&bytes).map_err(storage_error)?;
            let mut written = bytes.len() as u64;
            for entry in self.entries.values() {
                let record = encode_record(entry)?;
                writer.write_all(&record).map_err(storage_error)?;
                written += record.len() as u64;
            }
            writer.flush().map_err(storage_error)?;
            writer.get_ref().sync_all().map_err(storage_error)?;
            fs::rename(&temp, self.directory.join(JOURNAL_FILE)).map_err(storage_error)?;
            self.journal_bytes = written;
            self.metadata_bytes = metadata_bytes;
            if let Err(error) = sync_directory(&self.directory) {
                self.needs_reopen = true;
                return Err(error);
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }

    /// Look up an entry. `id` is a stable import ID; returns the entry or a clear
    /// missing-entry error without reading its original file.
    fn entry(&self, id: &str) -> Result<&CatalogEntry, String> {
        self.entries
            .get(id)
            .ok_or_else(|| format!("There is no catalog photograph with ID {id}."))
    }

    /// Check writer health. Takes no arguments; returns an error after uncertain
    /// storage failures so the journal must be recovered before another write.
    fn ensure_writable(&self) -> Result<(), String> {
        if self.needs_reopen {
            Err("Reopen the photo catalog to recover the failed write before saving again.".into())
        } else {
            Ok(())
        }
    }

    /// Persist one complete entry. `entry` supplies the replacement; returns only
    /// after its checked record is synced, then updates the in-memory map.
    fn upsert(&mut self, entry: CatalogEntry) -> Result<(), String> {
        self.ensure_writable()?;
        let encoded = encode_entry(&entry)?;
        let previous_bytes = self
            .entries
            .get(&entry.id)
            .map(encode_entry)
            .transpose()?
            .map_or(0, |bytes| bytes.len());
        let metadata_bytes = self
            .metadata_bytes
            .checked_sub(previous_bytes)
            .and_then(|bytes| bytes.checked_add(encoded.len()))
            .ok_or(
                "Catalog entries changed outside the saving methods. Checkpoint before saving.",
            )?;
        if metadata_bytes > MAX_METADATA_BYTES {
            return Err(
                "Photo catalog metadata exceeds 256 MiB. Remaining files were not imported.".into(),
            );
        }
        if !self.entries.contains_key(&entry.id) && self.entries.len() >= MAX_ENTRIES {
            return Err(format!(
                "The photo catalog holds at most {MAX_ENTRIES} entries."
            ));
        }
        if self.entries.get(&entry.id) == Some(&entry) {
            return Ok(());
        }
        let record = encode_record(&entry)?;
        if self.journal_bytes + record.len() as u64 > CHECKPOINT_BYTES {
            self.checkpoint()?;
        }
        let result = (|| {
            let mut file = OpenOptions::new()
                .append(true)
                .open(self.directory.join(JOURNAL_FILE))
                .map_err(storage_error)?;
            if file.metadata().map_err(storage_error)?.len() != self.journal_bytes {
                return Err(
                    "The photo catalog changed outside this writer. Reopen it before saving."
                        .into(),
                );
            }
            file.write_all(&record).map_err(storage_error)?;
            file.sync_data().map_err(storage_error)
        })();
        if let Err(error) = result {
            self.needs_reopen = true;
            return Err(format!(
                "{error} Reopen the catalog to recover before saving again."
            ));
        }
        self.journal_bytes += record.len() as u64;
        self.metadata_bytes = metadata_bytes;
        self.entries.insert(entry.id.clone(), entry);
        Ok(())
    }

    /// Preserve damaged input. Takes no arguments; returns the synced backup path
    /// only after copying every original byte without replacing an older backup.
    fn preserve_damaged_log(&self) -> Result<PathBuf, String> {
        let backup = self
            .directory
            .join(format!("catalog.damaged-{}.jsonl", unique_name()));
        let mut original = File::open(self.directory.join(JOURNAL_FILE)).map_err(storage_error)?;
        let mut preserved = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&backup)
            .map_err(storage_error)?;
        io::copy(&mut original, &mut preserved).map_err(storage_error)?;
        preserved.sync_all().map_err(storage_error)?;
        sync_directory(&self.directory)?;
        Ok(backup)
    }
}

/// Read bounded log lines. `path` supplies a journal and `visit` inspects each
/// numbered line; returns its byte length or an error without modifying it.
fn scan_log(
    path: &Path,
    mut visit: impl FnMut(usize, LogLine) -> Result<(), String>,
) -> Result<u64, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(storage_error(error)),
    };
    let metadata = file.metadata().map_err(storage_error)?;
    if !metadata.is_file() || metadata.len() > MAX_JOURNAL_BYTES {
        return Err("Choose a regular catalog journal no larger than 1 GiB.".into());
    }
    let mut reader = BufReader::new(file.take(MAX_JOURNAL_BYTES + 1));
    let mut length = 0;
    let mut number = 0;
    while let Some(line) = read_line(&mut reader).map_err(storage_error)? {
        length += line.read;
        if length > MAX_JOURNAL_BYTES {
            return Err("The photo catalog journal exceeds 1 GiB.".into());
        }
        number += 1;
        visit(number, line)?;
    }
    Ok(length)
}

/// Read one bounded line. `reader` supplies bytes; returns the next line and
/// damage flags, draining oversized lines without allocating their entire length.
fn read_line(reader: &mut impl BufRead) -> io::Result<Option<LogLine>> {
    let mut line = LogLine {
        bytes: Vec::new(),
        oversized: false,
        terminated: false,
        read: 0,
    };
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok((line.read > 0).then_some(line));
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let count = newline.map_or(available.len(), |index| index + 1);
        let content = count - usize::from(newline.is_some());
        let retained = content.min(MAX_RECORD_BYTES - line.bytes.len());
        line.bytes.extend_from_slice(&available[..retained]);
        line.oversized |= retained < content;
        line.read += count as u64;
        reader.consume(count);
        if newline.is_some() {
            line.terminated = true;
            return Ok(Some(line));
        }
    }
}

/// Refuse future data. `bytes` supplies one JSON line; returns an error for a
/// newer version or another format before recovery can rewrite any catalog file.
fn reject_newer(bytes: &[u8]) -> Result<(), String> {
    if let Ok(probe) = serde_json::from_slice::<VersionProbe>(bytes) {
        if probe.version > u64::from(FORMAT_VERSION) {
            return Err(format!(
                "Photo catalog version {} is newer than supported version {FORMAT_VERSION}. The catalog was left unchanged.",
                probe.version
            ));
        }
        if probe.format.as_ref().is_some_and(|format| format != FORMAT) {
            return Err(
                "This is a different catalog format. The catalog was left unchanged.".into(),
            );
        }
    }
    Ok(())
}

/// Recover independent replacements. `path` supplies the journal; returns all
/// valid entries, including records after corruption, without changing its bytes.
fn recover(path: &Path) -> Result<Recovered, String> {
    let mut recovered = Recovered::default();
    recovered.journal_bytes = scan_log(path, |number, line| {
        reject_newer(&line.bytes)?;
        let mut damage = (!line.terminated).then(|| "Record has no final newline".to_owned());
        let result = (|| {
            if line.oversized {
                return Err("Record exceeds the 4 MiB limit".to_owned());
            }
            if number == 1 {
                if let Ok(header) = serde_json::from_slice::<Header>(&line.bytes)
                    && header.format == FORMAT
                    && header.version == FORMAT_VERSION
                {
                    return Ok(None);
                }
                damage = Some("The photo catalog header was damaged or missing".into());
            }
            let record: Record = serde_json::from_slice(&line.bytes).map_err(encoding_error)?;
            if record.version != FORMAT_VERSION {
                return Err("Unsupported record version".into());
            }
            let encoded = encode_entry(&record.entry)?;
            if digest(&encoded) != record.sha256 {
                return Err("SHA-256 check failed".into());
            }
            Ok(Some((record.entry, encoded.len())))
        })();
        if let Err(error) = &result {
            damage = Some(error.clone());
        }
        if let Some(error) = damage {
            recovered.damaged += 1;
            if recovered.notes.len() < 16 {
                recovered
                    .notes
                    .push(format!("Catalog line {number}: {error}."));
            }
        }
        match result {
            Ok(Some((entry, bytes))) => {
                let old_bytes = recovered
                    .entries
                    .get(&entry.id)
                    .map(encode_entry)
                    .transpose()?
                    .map_or(0, |bytes| bytes.len());
                let total = recovered.metadata_bytes - old_bytes + bytes;
                if total > MAX_METADATA_BYTES
                    || (!recovered.entries.contains_key(&entry.id)
                        && recovered.entries.len() >= MAX_ENTRIES)
                {
                    return Err("The photo catalog exceeds its 100000-entry or 256 MiB metadata limit. It was left unchanged.".into());
                }
                recovered.metadata_bytes = total;
                recovered.entries.insert(entry.id.clone(), entry);
            }
            Ok(None) | Err(_) => {}
        }
        Ok(())
    })?;
    if recovered.journal_bytes == 0 && path.try_exists().map_err(storage_error)? {
        recovered.damaged += 1;
        recovered
            .notes
            .push("The photo catalog journal was empty.".into());
    }
    Ok(recovered)
}

/// Validate a metadata label. `label` is a keyword or stack name; returns an
/// error for empty, control-bearing or overlong labels.
fn validate_label(label: &str) -> Result<(), String> {
    if label.is_empty() || label.len() > 256 || label.chars().any(char::is_control) {
        Err("Keywords and stack names need 1–256 bytes without control characters.".into())
    } else {
        Ok(())
    }
}

/// Validate and encode an entry. `entry` supplies bounded metadata and settings;
/// returns canonical JSON bytes used for both storage and integrity checks.
fn encode_entry(entry: &CatalogEntry) -> Result<Vec<u8>, String> {
    edits::validate(&entry.develop)?;
    if entry.id.is_empty()
        || entry.id.len() > 96
        || !entry
            .id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        || entry.rating > 5
        || entry.keywords.len() > 128
        || entry.name.is_empty()
        || entry.name.len() > 4096
        || !entry.path.is_absolute()
        || entry
            .path
            .to_str()
            .is_none_or(|path| path.len() > 16 * 1024)
        || entry.settings_path != edits::sidecar_path(&entry.path)
        || entry.source_identity.modified_nanos >= 1_000_000_000
        || entry.content_sha256.len() != 64
        || !entry
            .content_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err("Photo catalog entry contains invalid or excessive metadata.".into());
    }
    for keyword in &entry.keywords {
        validate_label(keyword)?;
    }
    if let Some(stack) = &entry.stack {
        validate_label(stack)?;
    }
    let bytes = serde_json::to_vec(entry).map_err(encoding_error)?;
    if bytes.len() > MAX_ENTRY_BYTES {
        return Err("A photo catalog entry exceeds its 3 MiB metadata limit.".into());
    }
    Ok(bytes)
}

/// Encode a replacement. `entry` supplies validated metadata; returns one
/// newline-terminated versioned record with its canonical entry's SHA-256 hash.
fn encode_record(entry: &CatalogEntry) -> Result<Vec<u8>, String> {
    let encoded = encode_entry(entry)?;
    let mut bytes = serde_json::to_vec(&Record {
        version: FORMAT_VERSION,
        sha256: digest(&encoded),
        entry: entry.clone(),
    })
    .map_err(encoding_error)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_RECORD_BYTES {
        return Err("A photo catalog record exceeds 4 MiB.".into());
    }
    Ok(bytes)
}

/// Hash an original without decoding. `path` supplies a regular file; returns its
/// canonical path, current sidecar identity and streamed content hash, rejecting
/// files whose size or modification metadata changes during the read.
fn fingerprint(path: &Path) -> Result<(PathBuf, edits::SourceIdentity, String), String> {
    if !fs::symlink_metadata(path).map_err(storage_error)?.is_file() {
        return Err("Choose a regular original photograph, not a symbolic link.".into());
    }
    let path = path.canonicalize().map_err(storage_error)?;
    let identity = edits::SourceIdentity::read(&path)?;
    let file = File::open(&path).map_err(storage_error)?;
    let mut reader = file.take(identity.size_bytes.saturating_add(1));
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let read = reader.read(&mut buffer).map_err(storage_error)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size += read as u64;
    }
    if size != identity.size_bytes || edits::SourceIdentity::read(&path)? != identity {
        return Err(
            "The original changed while its content was being checked. Import it again.".into(),
        );
    }
    Ok((path, identity, format!("{:x}", hasher.finalize())))
}

/// Hash metadata bytes. `bytes` supplies canonical JSON; returns lowercase
/// SHA-256 text so journal records can be checked independently.
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Create a distinct name. Takes no arguments; returns a time, process and
/// counter-based import or temporary-file ID independent of source content.
fn unique_name() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{time:032x}-{:08x}-{:016x}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

/// Persist directory changes. `directory` owns a renamed or newly created file;
/// returns after the directory entry is synced to storage.
fn sync_directory(directory: &Path) -> Result<(), String> {
    File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(storage_error)
}

/// Describe a storage failure. `error` supplies the OS cause; returns user-facing
/// catalog context without hiding the underlying reason.
fn storage_error(error: io::Error) -> String {
    format!("Could not store or read the photo catalog: {error}")
}

/// Describe invalid JSON. `error` supplies the parser cause; returns a message
/// suitable for import errors or recovery notes.
fn encoding_error(error: serde_json::Error) -> String {
    format!("Could not read or encode photo catalog metadata: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join(".artifacts/photo-catalog-tests")
                .join(unique_name());
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn originals(&self) -> PathBuf {
            let path = self.0.join("originals");
            fs::create_dir_all(&path).unwrap();
            path
        }

        fn catalog(&self) -> PathBuf {
            self.0.join("library")
        }

        fn photo(&self, name: &str) -> PathBuf {
            let path = self.originals().join(name);
            let pixels = super::super::RgbaImage::new(1, 1, vec![71, 82, 93, 255]).unwrap();
            fs::write(&path, pixels.encode_png().unwrap()).unwrap();
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn imports_over_ten_thousand_real_images_without_retaining_pixels() {
        let fixture = Fixture::new();
        let seed = fixture.photo("seed.png");
        let originals = fixture.0.join("large/nested");
        fs::create_dir_all(&originals).unwrap();
        for index in 0..10_001 {
            fs::hard_link(&seed, originals.join(format!("copy-{index:05}.png"))).unwrap();
        }
        let mut catalog = Catalog::open(fixture.catalog()).unwrap();
        let report = catalog.import_folder(fixture.0.join("large"));
        assert_eq!(report.imported, 10_001, "{:?}", report.errors);
        assert_eq!(report.error_count, 0);
        assert!(!report.stopped);
        assert_eq!(catalog.entries.len(), 10_001);
        assert_eq!(
            catalog
                .entries
                .values()
                .map(|entry| &entry.content_sha256)
                .collect::<BTreeSet<_>>()
                .len(),
            1
        );
        assert!(catalog.metadata_bytes < MAX_METADATA_BYTES);
        assert!(MAX_ENTRIES >= 100_000);
        let ids = catalog.entries.keys().cloned().collect::<Vec<_>>();
        catalog.checkpoint().unwrap();
        assert!(Catalog::open(fixture.catalog()).is_err());
        drop(catalog);
        let mut reopened = Catalog::open(fixture.catalog()).unwrap();
        assert_eq!(reopened.entries.keys().cloned().collect::<Vec<_>>(), ids);
        let report = reopened.import_folder(fixture.0.join("large"));
        assert_eq!(report.imported, 0);
        assert_eq!(report.skipped, 10_001);
        assert_eq!(report.error_count, 0);
    }

    #[test]
    fn restart_and_checkpoint_preserve_sidecar_edits_ratings_keywords_and_stacks() {
        let fixture = Fixture::new();
        let source = fixture.photo("Portrait.png");
        fixture.photo("Other.png");
        let develop = DevelopParams {
            exposure: 1.5,
            rotate: 90,
            crop: Some([0.1, 0.2, 0.9, 0.8]),
            ..Default::default()
        };
        edits::save(
            &source,
            &edits::SourceIdentity::read(&source).unwrap(),
            &develop,
        )
        .unwrap();
        let mut catalog = Catalog::open(fixture.catalog()).unwrap();
        assert_eq!(catalog.import_folder(fixture.originals()).imported, 2);
        let id = catalog
            .entries
            .values()
            .find(|entry| entry.name == "Portrait.png")
            .unwrap()
            .id
            .clone();
        assert_eq!(catalog.entries[&id].develop, develop);
        catalog.set_rating(&id, 5).unwrap();
        catalog
            .set_keywords(&id, [" Portrait ", "Client", "Client"])
            .unwrap();
        catalog.set_stack(&id, Some(" Selects ".into())).unwrap();
        let develop = DevelopParams {
            exposure: -0.75,
            ..develop
        };
        catalog.set_develop(&id, &develop).unwrap();
        let expected = catalog.entries.clone();
        drop(catalog);
        let mut catalog = Catalog::open(fixture.catalog()).unwrap();
        assert_eq!(catalog.entries, expected);
        let filter = CatalogFilter {
            min_rating: 4,
            keyword: Some("client".into()),
            stack: Some("Selects".into()),
            text: "PORTRAIT".into(),
        };
        assert_eq!(
            catalog
                .filter(&filter)
                .iter()
                .map(|entry| &entry.id)
                .collect::<Vec<_>>(),
            vec![&id]
        );
        assert_eq!(catalog.load_photo(&id).unwrap().develop, develop);
        catalog.checkpoint().unwrap();
        drop(catalog);
        let mut catalog = Catalog::open(fixture.catalog()).unwrap();
        assert_eq!(catalog.entries, expected);
        catalog.set_stack(&id, None).unwrap();
        catalog.set_keywords(&id, Vec::<String>::new()).unwrap();
        assert!(catalog.filter(&filter).is_empty());
    }

    #[test]
    fn large_valid_brush_settings_survive_import_journal_and_checkpoint() {
        use super::super::local::{BrushStroke, LocalAdjustment, LocalMask};

        let fixture = Fixture::new();
        let source = fixture.photo("brush.png");
        let mut develop = DevelopParams {
            local_adjustments: vec![LocalAdjustment {
                mask: LocalMask::Brush {
                    strokes: vec![BrushStroke {
                        points: (0..8192).map(|index| [index as f32 / 8191., 0.5]).collect(),
                        radius: 0.1,
                        hardness: 0.5,
                        flow: 0.75,
                        erase: false,
                    }],
                },
                exposure: 0.75,
                ..Default::default()
            }],
            ..Default::default()
        };
        edits::validate(&develop).unwrap();
        let identity = edits::SourceIdentity::read(&source).unwrap();
        let sidecar = edits::encode(&identity, &develop).unwrap();
        assert!(sidecar.len() > 64 * 1024);
        fs::write(edits::sidecar_path(&source), sidecar).unwrap();
        let mut catalog = Catalog::open(fixture.catalog()).unwrap();
        let report = catalog.import_folder(fixture.originals());
        assert_eq!(report.imported, 1, "{:?}", report.errors);
        let id = catalog.entries.keys().next().unwrap().clone();
        assert_eq!(catalog.entries[&id].develop, develop);
        assert!(encode_record(&catalog.entries[&id]).unwrap().len() > 64 * 1024);
        develop.local_adjustments[0].exposure = -1.25;
        catalog.set_develop(&id, &develop).unwrap();
        catalog.set_rating(&id, 4).unwrap();
        fs::remove_file(edits::sidecar_path(&source)).unwrap();
        drop(catalog);
        let mut catalog = Catalog::open(fixture.catalog()).unwrap();
        assert_eq!(catalog.entries[&id].develop, develop);
        assert_eq!(catalog.entries[&id].rating, 4);
        assert_eq!(catalog.load_photo(&id).unwrap().develop, develop);
        catalog.checkpoint().unwrap();
        drop(catalog);
        let catalog = Catalog::open(fixture.catalog()).unwrap();
        assert_eq!(catalog.entries[&id].develop, develop);
    }

    #[test]
    fn middle_corruption_and_truncated_tail_recover_later_complete_upserts() {
        let fixture = Fixture::new();
        fixture.photo("first.png");
        fixture.photo("second.png");
        let mut catalog = Catalog::open(fixture.catalog()).unwrap();
        assert_eq!(catalog.import_folder(fixture.originals()).imported, 2);
        let ids = catalog.entries.keys().cloned().collect::<Vec<_>>();
        catalog.set_rating(&ids[0], 4).unwrap();
        catalog.set_keywords(&ids[1], ["Recovered"]).unwrap();
        let expected = catalog.entries.clone();
        drop(catalog);
        let journal = fixture.catalog().join(JOURNAL_FILE);
        let original = fs::read_to_string(&journal).unwrap();
        let mut lines = original.lines().map(str::to_owned).collect::<Vec<_>>();
        let halfway = lines[1].len() / 2;
        lines[1].truncate(halfway);
        let mut damaged = lines.join("\n").into_bytes();
        damaged.extend_from_slice(b"\n{\"version\":1,\"sha256\":\"torn");
        fs::write(&journal, &damaged).unwrap();
        let mut catalog = Catalog::open(fixture.catalog()).unwrap();
        assert_eq!(catalog.entries, expected);
        assert!(!catalog.recovery_notes.is_empty());
        let backup = fs::read_dir(fixture.catalog())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("catalog.damaged-")
            })
            .unwrap();
        assert_eq!(fs::read(&backup).unwrap(), damaged);
        catalog.set_rating(&ids[0], 5).unwrap();
        drop(catalog);
        let catalog = Catalog::open(fixture.catalog()).unwrap();
        assert!(catalog.recovery_notes.is_empty());
        assert_eq!(catalog.entries[&ids[0]].rating, 5);
        assert_eq!(fs::read(&backup).unwrap(), damaged);
    }

    #[test]
    fn checksum_damage_skips_only_the_bad_replacement() {
        let fixture = Fixture::new();
        fixture.photo("original.png");
        let mut catalog = Catalog::open(fixture.catalog()).unwrap();
        catalog.import_folder(fixture.originals());
        let id = catalog.entries.keys().next().unwrap().clone();
        catalog.set_rating(&id, 3).unwrap();
        catalog.set_keywords(&id, ["Before damage"]).unwrap();
        drop(catalog);
        let journal = fixture.catalog().join(JOURNAL_FILE);
        let text = fs::read_to_string(&journal).unwrap();
        let mut lines = text.lines().map(str::to_owned).collect::<Vec<_>>();
        let mut record: serde_json::Value = serde_json::from_str(lines.last().unwrap()).unwrap();
        record["entry"]["rating"] = 5.into();
        *lines.last_mut().unwrap() = serde_json::to_string(&record).unwrap();
        fs::write(&journal, format!("{}\n", lines.join("\n"))).unwrap();
        let catalog = Catalog::open(fixture.catalog()).unwrap();
        assert_eq!(catalog.entries[&id].rating, 3);
        assert!(catalog.entries[&id].keywords.is_empty());
        assert!(
            catalog
                .recovery_notes
                .iter()
                .any(|note| note.contains("SHA-256"))
        );
    }

    #[test]
    fn complete_final_record_without_newline_is_retained() {
        let fixture = Fixture::new();
        fixture.photo("original.png");
        let mut catalog = Catalog::open(fixture.catalog()).unwrap();
        catalog.import_folder(fixture.originals());
        let id = catalog.entries.keys().next().unwrap().clone();
        catalog.set_rating(&id, 5).unwrap();
        drop(catalog);
        let path = fixture.catalog().join(JOURNAL_FILE);
        let mut bytes = fs::read(&path).unwrap();
        assert_eq!(bytes.pop(), Some(b'\n'));
        fs::write(&path, bytes).unwrap();
        let catalog = Catalog::open(fixture.catalog()).unwrap();
        assert_eq!(catalog.entries[&id].rating, 5);
        assert!(!catalog.recovery_notes.is_empty());
    }

    #[test]
    fn oversized_middle_line_is_drained_before_recovering_good_records() {
        let fixture = Fixture::new();
        fixture.photo("original.png");
        let mut catalog = Catalog::open(fixture.catalog()).unwrap();
        catalog.import_folder(fixture.originals());
        let expected = catalog.entries.clone();
        drop(catalog);
        let path = fixture.catalog().join(JOURNAL_FILE);
        let bytes = fs::read(&path).unwrap();
        let header_end = bytes.iter().position(|byte| *byte == b'\n').unwrap() + 1;
        let mut damaged = bytes[..header_end].to_vec();
        damaged.resize(damaged.len() + MAX_RECORD_BYTES + 1, b'x');
        damaged.push(b'\n');
        damaged.extend_from_slice(&bytes[header_end..]);
        fs::write(&path, damaged).unwrap();
        let catalog = Catalog::open(fixture.catalog()).unwrap();
        assert_eq!(catalog.entries, expected);
        assert!(
            catalog
                .recovery_notes
                .iter()
                .any(|note| note.contains("4 MiB"))
        );
    }

    #[test]
    fn relink_uses_content_not_mtime_and_keeps_edits_without_a_sidecar() {
        let fixture = Fixture::new();
        let source = fixture.photo("original.png");
        let develop = DevelopParams {
            exposure: 1.25,
            rotate: 90,
            ..Default::default()
        };
        edits::save(
            &source,
            &edits::SourceIdentity::read(&source).unwrap(),
            &develop,
        )
        .unwrap();
        let original = fs::read(&source).unwrap();
        let mut catalog = Catalog::open(fixture.catalog()).unwrap();
        assert_eq!(catalog.import_folder(fixture.originals()).imported, 1);
        let id = catalog.entries.keys().next().unwrap().clone();
        let before = catalog.entries[&id].source_identity.clone();
        let moved = fixture.0.join("moved.png");
        fs::rename(&source, &moved).unwrap();
        fs::remove_file(edits::sidecar_path(&source)).unwrap();
        File::options()
            .write(true)
            .open(&moved)
            .unwrap()
            .set_times(
                fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(1_234_567)),
            )
            .unwrap();
        assert_ne!(edits::SourceIdentity::read(&moved).unwrap(), before);
        catalog.relink(&id, &moved).unwrap();
        assert_eq!(
            catalog.entries[&id].source_identity,
            edits::SourceIdentity::read(&moved).unwrap()
        );
        assert_eq!(
            catalog.entries[&id].settings_path,
            edits::sidecar_path(&moved)
        );
        assert_eq!(catalog.load_photo(&id).unwrap().develop, develop);
        drop(catalog);
        let mut catalog = Catalog::open(fixture.catalog()).unwrap();
        let photo = catalog.load_photo(&id).unwrap();
        assert_eq!(photo.develop, develop);
        assert_eq!(photo.source.as_deref(), Some(moved.as_path()));
        assert_eq!(photo.settings_path, Some(edits::sidecar_path(&moved)));
        let wrong = fixture.0.join("wrong.png");
        fs::write(&wrong, vec![0; original.len()]).unwrap();
        assert!(catalog.relink(&id, &wrong).unwrap_err().contains("SHA-256"));
        assert_eq!(fs::read(&moved).unwrap(), original);
        assert!(!edits::sidecar_path(&moved).exists());
    }

    #[test]
    fn writer_lock_survives_checkpoint_and_releases_on_drop() {
        let fixture = Fixture::new();
        let mut catalog = Catalog::open(fixture.catalog()).unwrap();
        assert!(
            Catalog::open(fixture.catalog())
                .unwrap_err()
                .contains("already open")
        );
        catalog.checkpoint().unwrap();
        assert!(Catalog::open(fixture.catalog()).is_err());
        drop(catalog);
        assert!(Catalog::open(fixture.catalog()).is_ok());
    }

    #[test]
    fn future_header_or_record_is_refused_without_creating_or_rewriting_files() {
        for future_record in [false, true] {
            let fixture = Fixture::new();
            fs::create_dir_all(fixture.catalog()).unwrap();
            let bytes = if future_record {
                format!(
                    "{{\"format\":\"{FORMAT}\",\"version\":1}}\ninvalid\n{{\"version\":2,\"future\":true}}\n"
                )
            } else {
                format!("{{\"format\":\"{FORMAT}\",\"version\":2}}\n")
            };
            let path = fixture.catalog().join(JOURNAL_FILE);
            fs::write(&path, &bytes).unwrap();
            assert!(
                Catalog::open(fixture.catalog())
                    .unwrap_err()
                    .contains("newer")
            );
            assert_eq!(fs::read(&path).unwrap(), bytes.as_bytes());
            assert_eq!(fs::read_dir(fixture.catalog()).unwrap().count(), 1);
        }
    }

    #[test]
    fn validation_rejects_bad_settings_and_labels_before_writing() {
        let fixture = Fixture::new();
        fixture.photo("original.png");
        let mut catalog = Catalog::open(fixture.catalog()).unwrap();
        catalog.import_folder(fixture.originals());
        let id = catalog.entries.keys().next().unwrap().clone();
        let journal = fixture.catalog().join(JOURNAL_FILE);
        let before = fs::read(&journal).unwrap();
        assert!(catalog.set_rating(&id, 6).is_err());
        assert!(catalog.set_keywords(&id, ["bad\nkeyword"]).is_err());
        assert!(
            catalog
                .set_keywords(&id, (0..129).map(|index| index.to_string()))
                .is_err()
        );
        assert!(catalog.set_stack(&id, Some(" ".into())).is_err());
        assert!(
            catalog
                .set_develop(
                    &id,
                    &DevelopParams {
                        exposure: f32::NAN,
                        ..Default::default()
                    }
                )
                .is_err()
        );
        assert!(
            catalog
                .set_develop(
                    &id,
                    &DevelopParams {
                        crop: Some([0.9, 0.0, 0.2, 1.0]),
                        ..Default::default()
                    }
                )
                .is_err()
        );
        assert_eq!(fs::read(&journal).unwrap(), before);
    }

    #[test]
    fn import_does_not_decode_and_source_content_is_checked_even_with_matching_metadata() {
        let fixture = Fixture::new();
        let source = fixture.originals().join("undecodable.png");
        fs::write(&source, b"not pixels").unwrap();
        let mut catalog = Catalog::open(fixture.catalog()).unwrap();
        let report = catalog.import_folder(fixture.originals());
        assert_eq!(report.imported, 1);
        assert_eq!(report.error_count, 0);
        let id = catalog.entries.keys().next().unwrap().clone();
        assert!(catalog.load_photo(&id).is_err());
        let metadata = fs::metadata(&source).unwrap();
        fs::write(&source, b"new pixels").unwrap();
        File::options()
            .write(true)
            .open(&source)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(metadata.modified().unwrap()))
            .unwrap();
        assert_eq!(
            edits::SourceIdentity::read(&source).unwrap(),
            catalog.entries[&id].source_identity
        );
        assert!(
            catalog
                .load_photo(&id)
                .err()
                .unwrap()
                .contains("content changed")
        );
    }

    #[cfg(unix)]
    #[test]
    fn recursive_import_ignores_symlink_loops_and_reports_bad_sidecars() {
        let fixture = Fixture::new();
        let source = fixture.photo("invalid-settings.png");
        fs::write(edits::sidecar_path(&source), b"invalid settings").unwrap();
        let nested = fixture.originals().join("nested");
        fs::create_dir(&nested).unwrap();
        let valid = fixture.photo("valid.png");
        fs::rename(&valid, nested.join("valid.png")).unwrap();
        std::os::unix::fs::symlink(fixture.originals(), nested.join("loop")).unwrap();
        let mut catalog = Catalog::open(fixture.catalog()).unwrap();
        let report = catalog.import_folder(fixture.originals());
        assert_eq!(report.imported, 1);
        assert_eq!(report.skipped, 1);
        assert_eq!(report.error_count, 1);
        assert!(report.errors[0].contains("invalid-settings.png"));
        assert!(fs::read(edits::sidecar_path(&source)).unwrap() == b"invalid settings");
        let report = catalog.import_folder(fixture.0.join("missing"));
        assert_eq!(report.error_count, 1);
    }
}
