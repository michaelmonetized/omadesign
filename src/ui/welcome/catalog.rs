//! The welcome page is a filesystem browser. Traversal and metadata parsing run
//! off the event loop; the metadata reader skips packed raster bytes entirely.
//! One preview worker services only visible tiles, with bounded RAM/disk caches.

use crate::tools::Persona;
use eframe::egui::{self, TextureHandle};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant, UNIX_EPOCH};

const SCAN_ID: &str = "welcome-home-catalog-v3";
const PREVIEW_ID: &str = "welcome-home-previews-v3";
const CAPTURE_ROOT_ID: &str = "welcome-capture-catalog-root";
const REFRESH: Duration = Duration::from_secs(30);
const MAX_TEXTURES: usize = 96;
const MAX_DISK_PREVIEWS: usize = 256;
const PREVIEW_EDGE: u32 = 384;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct DocumentEntry {
    pub path: PathBuf,
    pub name: String,
    pub modified_ns: u128,
    pub size: u64,
    pub aspect: f32,
    pub modes: Vec<Persona>,
    /// Older files infer modes from content. A blank legacy document has no
    /// known mode, rather than pretending every file was made in Vector.
    pub inferred_modes: bool,
    pub error: Option<String>,
    pub recovered: bool,
}

impl DocumentEntry {
    pub fn matches_mode(&self, mode: Persona) -> bool {
        self.modes.contains(&mode)
    }
    fn key(&self) -> PreviewKey {
        PreviewKey {
            path: self.path.clone(),
            modified_ns: self.modified_ns,
            size: self.size,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct ProjectEntry {
    pub path: PathBuf,
    pub name: String,
    pub modified_ns: u128,
    #[serde(skip)]
    assets: Vec<(PathBuf, crate::brand::Asset)>,
    #[serde(skip)]
    documents: Vec<DocumentEntry>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(super) struct Catalog {
    pub documents: Vec<DocumentEntry>,
    pub recovered: Vec<DocumentEntry>,
    pub projects: Vec<ProjectEntry>,
    pub warnings: Vec<String>,
}

impl Catalog {
    pub fn documents_in(&self, path: &Path) -> Vec<&DocumentEntry> {
        self.documents
            .iter()
            .filter(|entry| entry.path.starts_with(path))
            .collect()
    }

    /// Direct project children, even when separated by ordinary directories.
    /// All project roots remain available in the top-level Projects browser.
    pub fn subprojects(&self, path: &Path) -> Vec<&ProjectEntry> {
        self.projects
            .iter()
            .filter(|project| {
                project.path != path
                    && project.path.starts_with(path)
                    && !self.projects.iter().any(|other| {
                        other.path != path
                            && other.path != project.path
                            && other.path.starts_with(path)
                            && project.path.starts_with(&other.path)
                    })
            })
            .collect()
    }

    fn sort(&mut self) {
        self.documents
            .sort_by(|a, b| b.modified_ns.cmp(&a.modified_ns).then(a.path.cmp(&b.path)));
        self.recovered
            .sort_by(|a, b| b.modified_ns.cmp(&a.modified_ns).then(a.path.cmp(&b.path)));
        self.projects
            .sort_by(|a, b| b.modified_ns.cmp(&a.modified_ns).then(a.path.cmp(&b.path)));
    }
    fn warning(&mut self, warning: String) {
        if self.warnings.len() < 128 {
            self.warnings.push(warning);
        } else if self.warnings.len() == 128 {
            self.warnings
                .push("Additional unreadable paths were skipped.".into());
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct Snapshot {
    pub catalog: Arc<Catalog>,
    pub scanning: bool,
    pub visited: usize,
}

struct ScanState {
    snapshot: Snapshot,
    rx: Option<mpsc::Receiver<Snapshot>>,
    stop: Arc<AtomicBool>,
    completed: Option<Instant>,
}
impl Default for ScanState {
    fn default() -> Self {
        Self {
            snapshot: Snapshot::default(),
            rx: None,
            stop: Arc::new(AtomicBool::new(false)),
            completed: None,
        }
    }
}
impl Drop for ScanState {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

type SharedScan = Arc<Mutex<ScanState>>;

fn cache_root() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".cache"))
        .join("omadesign/welcome-v3")
}
fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/nonexistent"))
}

/// A capture uses the same real filesystem scanner, with a context-local root.
/// Clear any previous account/home results before the first capture frame.
pub(super) fn set_capture_catalog_root(ctx: &egui::Context, root: &Path) {
    let root = root.to_path_buf();
    if ctx.data(|d| d.get_temp::<PathBuf>(egui::Id::new(CAPTURE_ROOT_ID))) == Some(root.clone()) {
        return;
    }
    cancel(ctx);
    ctx.data_mut(|d| {
        d.remove::<SharedScan>(egui::Id::new(SCAN_ID));
        d.remove::<SharedPreviews>(egui::Id::new(PREVIEW_ID));
        d.insert_temp(egui::Id::new(CAPTURE_ROOT_ID), root);
    });
}
fn state(ctx: &egui::Context) -> SharedScan {
    ctx.data_mut(|d| {
        let id = egui::Id::new(SCAN_ID);
        if let Some(state) = d.get_temp::<SharedScan>(id) {
            return state;
        }
        let state = Arc::new(Mutex::new(ScanState::default()));
        d.insert_temp(id, state.clone());
        state
    })
}

pub(super) fn snapshot(ctx: &egui::Context) -> Snapshot {
    let shared = state(ctx);
    let Ok(mut state) = shared.lock() else {
        return Snapshot::default();
    };
    loop {
        let result = state.rx.as_ref().map(|rx| rx.try_recv());
        match result {
            Some(Ok(snapshot)) => {
                let done = !snapshot.scanning;
                state.snapshot = snapshot;
                if done {
                    state.rx = None;
                    state.completed = Some(Instant::now());
                    break;
                }
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                state.rx = None;
                state.snapshot.scanning = false;
                Arc::make_mut(&mut state.snapshot.catalog)
                    .warning("File discovery stopped unexpectedly. Refresh to try again.".into());
                state.completed = Some(Instant::now());
                break;
            }
            _ => break,
        }
    }
    if state.rx.is_none() && state.completed.is_none_or(|at| at.elapsed() >= REFRESH) {
        start_scan(&mut state, ctx);
    }
    ctx.request_repaint_after(if state.snapshot.scanning {
        Duration::from_millis(150)
    } else {
        REFRESH
    });
    state.snapshot.clone()
}

pub(super) fn refresh(ctx: &egui::Context) {
    if let Ok(mut state) = state(ctx).lock() {
        start_scan(&mut state, ctx);
    }
}

pub(super) fn recovered(ctx: &egui::Context) -> Vec<DocumentEntry> {
    snapshot(ctx).catalog.recovered.clone()
}

pub(super) fn ready(ctx: &egui::Context) -> bool {
    let scanning = ctx
        .data(|d| d.get_temp::<SharedScan>(egui::Id::new(SCAN_ID)))
        .is_some_and(|shared| shared.lock().is_ok_and(|state| state.snapshot.scanning));
    !scanning && previews_ready(ctx)
}

pub(super) fn cancel(ctx: &egui::Context) {
    if let Some(shared) = ctx.data(|d| d.get_temp::<SharedScan>(egui::Id::new(SCAN_ID)))
        && let Ok(mut state) = shared.lock()
    {
        state.stop.store(true, Ordering::Relaxed);
        state.rx = None;
        state.snapshot.scanning = false;
        state.completed = None;
    }
    if let Some(shared) = ctx.data(|d| d.get_temp::<SharedPreviews>(egui::Id::new(PREVIEW_ID)))
        && let Ok(mut state) = shared.lock()
    {
        state.queue.clear();
    }
}

fn start_scan(state: &mut ScanState, ctx: &egui::Context) {
    state.stop.store(true, Ordering::Relaxed);
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::sync_channel(1);
    state.stop = stop.clone();
    state.rx = Some(rx);
    state.snapshot.scanning = true;
    let previous = state.snapshot.catalog.clone();
    let capture_root = ctx.data(|d| d.get_temp::<PathBuf>(egui::Id::new(CAPTURE_ROOT_ID)));
    let root = capture_root.clone().unwrap_or_else(home);
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        // The persistent index belongs to the user's real home. Never show or
        // overwrite it when capturing a supplied fixture directory.
        let disk = if capture_root.is_none() && previous.documents.is_empty() {
            fs::File::open(cache_root().join("index.json"))
                .ok()
                .and_then(|file| serde_json::from_reader::<_, Catalog>(BufReader::new(file)).ok())
        } else {
            None
        };
        let previous = disk.as_ref().unwrap_or(&previous);
        // Present the last complete index immediately; keep it while the new
        // scan fills in changes so a refresh does not empty/reflow both panes.
        if !previous.documents.is_empty() || !previous.projects.is_empty() {
            let _ = tx.try_send(Snapshot {
                catalog: Arc::new(previous.clone()),
                scanning: true,
                visited: 0,
            });
            ctx.request_repaint();
        }
        let mut publish = |catalog: &Catalog, visited: usize, complete: bool| {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            let mut catalog = catalog.clone();
            if !complete {
                let found: HashSet<_> = catalog.documents.iter().map(|d| d.path.clone()).collect();
                catalog.documents.extend(
                    previous
                        .documents
                        .iter()
                        .filter(|d| !found.contains(&d.path))
                        .cloned(),
                );
                let found: HashSet<_> = catalog.projects.iter().map(|p| p.path.clone()).collect();
                catalog.projects.extend(
                    previous
                        .projects
                        .iter()
                        .filter(|p| !found.contains(&p.path))
                        .cloned(),
                );
                catalog.recovered.clone_from(&previous.recovered);
            }
            catalog.sort();
            let snapshot = Snapshot {
                catalog: Arc::new(catalog),
                scanning: !complete,
                visited,
            };
            if complete {
                let _ = tx.send(snapshot);
            } else {
                let _ = tx.try_send(snapshot);
            }
            ctx.request_repaint();
        };
        let recovery_root = capture_root.is_none().then(crate::project::swap_dir);
        let result = scan(
            &root,
            previous,
            &stop,
            &mut publish,
            recovery_root.as_deref(),
        );
        if capture_root.is_none() && !stop.load(Ordering::Relaxed) {
            if let Ok(bytes) = serde_json::to_vec(&result) {
                let directory = cache_root();
                let _ = fs::create_dir_all(&directory);
                let temporary = directory.join(format!(
                    "index-{}-{}.tmp",
                    std::process::id(),
                    crate::document::next_id()
                ));
                if fs::write(&temporary, bytes).is_ok() {
                    let _ = fs::rename(temporary, directory.join("index.json"));
                }
            }
        }
    });
}

fn modified(meta: &fs::Metadata) -> u128 {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |time| time.as_nanos())
}
fn excluded(name: &std::ffi::OsStr) -> bool {
    let name = name.to_string_lossy();
    name.starts_with('.')
        || name.eq_ignore_ascii_case("trash")
        || name.eq_ignore_ascii_case("$recycle.bin")
}

fn scan(
    root: &Path,
    previous: &Catalog,
    stop: &AtomicBool,
    publish: &mut impl FnMut(&Catalog, usize, bool),
    recovery_root: Option<&Path>,
) -> Catalog {
    let known: HashMap<_, _> = previous
        .documents
        .iter()
        .chain(&previous.recovered)
        .map(|d| (&d.path, d))
        .collect();
    let mut catalog = Catalog::default();
    let mut pending = vec![root.to_owned()];
    let mut visited = 0;
    let mut published = Instant::now();
    while let Some(directory) = pending.pop() {
        if stop.load(Ordering::Relaxed) {
            return catalog;
        }
        // A queued directory can be replaced while scanning. Recheck before
        // opening it so a newly created link does not escape the home tree.
        match fs::symlink_metadata(&directory) {
            Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {}
            Ok(_) => continue,
            Err(error) => {
                catalog.warning(format!("{}: {error}", directory.display()));
                continue;
            }
        }
        let marker = directory.join(crate::brand::FOLDER);
        if fs::symlink_metadata(&marker)
            .is_ok_and(|m| !m.file_type().is_symlink() && (m.is_dir() || m.is_file()))
        {
            let mut project = ProjectEntry {
                name: directory
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                path: directory.clone(),
                modified_ns: fs::metadata(&directory).map_or(0, |m| modified(&m)),
                assets: Vec::new(),
                documents: Vec::new(),
            };
            if marker.is_dir() {
                match crate::brand::scan(&marker) {
                    Ok(mut bank) => {
                        project.name = bank.name;
                        bank.assets
                            .sort_by(|a, b| b.modified_ns.cmp(&a.modified_ns));
                        project.assets = bank
                            .assets
                            .into_iter()
                            .take(3)
                            .map(|asset| (bank.root.clone(), asset))
                            .collect();
                        for warning in bank.warnings {
                            catalog.warning(warning);
                        }
                    }
                    Err(error) => catalog.warning(error),
                }
            }
            catalog.projects.push(project);
        }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) => {
                catalog.warning(format!("Could not read {}: {error}", directory.display()));
                continue;
            }
        };
        for entry in entries {
            if stop.load(Ordering::Relaxed) {
                return catalog;
            }
            visited += 1;
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    catalog.warning(error.to_string());
                    continue;
                }
            };
            if excluded(&entry.file_name()) {
                continue;
            }
            let path = entry.path();
            // DirectoryEntry::file_type uses the directory's d_type on Linux;
            // don't stat millions of unrelated source/dependency files.
            let kind = match entry.file_type() {
                Ok(kind) => kind,
                Err(error) => {
                    catalog.warning(format!("{}: {error}", path.display()));
                    continue;
                }
            };
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file()
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("oma"))
            {
                match fs::symlink_metadata(&path) {
                    Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => {
                        catalog.documents.push(index_entry(
                            &path,
                            &meta,
                            false,
                            known.get(&path).copied(),
                            stop,
                        ));
                    }
                    Ok(_) => {}
                    Err(error) => catalog.warning(format!("{}: {error}", path.display())),
                }
            }
            if published.elapsed() >= Duration::from_millis(250) {
                publish(&catalog, visited, false);
                published = Instant::now();
            }
        }
    }
    // Recovery lives in hidden application state, deliberately outside home
    // discovery. Read only its explicit swap directory, never unrelated caches.
    if let Some(recovery_root) = recovery_root
        && let Ok(entries) = fs::read_dir(recovery_root)
    {
        for entry in entries.flatten() {
            if stop.load(Ordering::Relaxed) {
                return catalog;
            }
            let path = entry.path();
            if !path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with(".oma.swp"))
            {
                continue;
            }
            if let Ok(meta) = fs::symlink_metadata(&path)
                && meta.is_file()
                && !meta.file_type().is_symlink()
            {
                catalog.recovered.push(index_entry(
                    &path,
                    &meta,
                    true,
                    known.get(&path).copied(),
                    stop,
                ));
            }
        }
    }
    for project in &mut catalog.projects {
        project.documents = catalog
            .documents
            .iter()
            .filter(|d| d.path.starts_with(&project.path))
            .cloned()
            .collect();
        project
            .documents
            .sort_by(|a, b| b.modified_ns.cmp(&a.modified_ns));
        project
            .documents
            .truncate(3usize.saturating_sub(project.assets.len()));
        if let Some(latest) = catalog
            .documents
            .iter()
            .filter(|d| d.path.starts_with(&project.path))
            .map(|d| d.modified_ns)
            .max()
        {
            project.modified_ns = project.modified_ns.max(latest);
        }
    }
    catalog.sort();
    publish(&catalog, visited, true);
    catalog
}

/// `IgnoredAny` fields consume packed raster arrays without allocating them.
/// The reader checks cancellation between chunks even in a very large file.
struct CancellableReader<'a> {
    reader: BufReader<fs::File>,
    stop: &'a AtomicBool,
}
impl Read for CancellableReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.stop.load(Ordering::Relaxed) {
            return Err(std::io::Error::other("File discovery cancelled"));
        }
        self.reader.read(buf)
    }
}

#[derive(Deserialize)]
struct Header {
    #[serde(default)]
    version: Option<u32>,
    #[serde(default)]
    name: String,
    doc: DocHeader,
}
#[derive(Deserialize)]
struct DocHeader {
    width: f32,
    height: f32,
    #[serde(default)]
    workspace: Option<Persona>,
    #[serde(default)]
    layers: Vec<LayerHeader>,
    #[serde(default)]
    artboards: ArtboardsHeader,
    #[serde(default)]
    motion: MotionHeader,
    #[serde(default)]
    layout_tokens: Vec<serde::de::IgnoredAny>,
}
#[derive(Deserialize, Default)]
#[serde(untagged)]
enum ArtboardsHeader {
    Boards(Vec<crate::document::Artboard>),
    Count(u32),
    #[default]
    Missing,
}
#[derive(Deserialize)]
struct LayerHeader {
    #[serde(default)]
    name: String,
    kind: KindHeader,
}
#[derive(Deserialize)]
enum KindHeader {
    Vector {
        #[serde(default)]
        shapes: Vec<ShapeHeader>,
    },
    Raster {
        #[serde(default)]
        pixels: serde::de::IgnoredAny,
    },
}
#[derive(Deserialize)]
struct ShapeHeader {
    #[serde(default)]
    layout: LayoutHeader,
}
#[derive(Deserialize, Default)]
struct LayoutHeader {
    #[serde(default)]
    frame: bool,
    #[serde(default)]
    parent: Option<u64>,
}
#[derive(Deserialize, Default)]
struct MotionHeader {
    #[serde(default)]
    tracks: Vec<serde::de::IgnoredAny>,
}

fn index_entry(
    path: &Path,
    meta: &fs::Metadata,
    recovered: bool,
    known: Option<&DocumentEntry>,
    stop: &AtomicBool,
) -> DocumentEntry {
    let modified_ns = modified(meta);
    if let Some(known) = known
        && known.modified_ns == modified_ns
        && known.size == meta.len()
        && known.error.is_none()
    {
        return known.clone();
    }
    let mut entry = DocumentEntry {
        path: path.to_owned(),
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        modified_ns,
        size: meta.len(),
        aspect: 1.0,
        modes: Vec::new(),
        inferred_modes: true,
        error: None,
        recovered,
    };
    let read = || -> Result<Header, String> {
        let reader = CancellableReader {
            reader: BufReader::new(fs::File::open(path).map_err(|e| e.to_string())?),
            stop,
        };
        serde_json::from_reader(reader).map_err(|e| e.to_string())
    };
    match read() {
        Ok(header) => {
            if !recovered
                && header
                    .version
                    .is_none_or(|v| !(1..=crate::project::VERSION).contains(&v))
            {
                entry.error = Some("Unsupported omadesign document version".into());
                return entry;
            }
            if recovered && !header.name.is_empty() {
                entry.name = header.name;
            }
            let doc = header.doc;
            let dimensions = match &doc.artboards {
                ArtboardsHeader::Boards(boards) => boards
                    .iter()
                    .map(crate::document::Artboard::bounds)
                    .reduce(|a, b| a.union(b))
                    .map(|b| (b.width(), b.height())),
                ArtboardsHeader::Count(count) => {
                    let _ = count;
                    None
                }
                ArtboardsHeader::Missing => None,
            }
            .unwrap_or((doc.width, doc.height));
            let aspect = dimensions.0 / dimensions.1;
            if dimensions.0.is_finite()
                && dimensions.1.is_finite()
                && dimensions.0 > 0.0
                && dimensions.1 > 0.0
                && aspect.is_finite()
                && aspect > 0.0
            {
                entry.aspect = aspect;
            } else {
                entry.error = Some("Invalid document dimensions".into());
            }
            let mut vector = false;
            let mut raster = false;
            let mut layout = !doc.layout_tokens.is_empty();
            for layer in doc.layers {
                match layer.kind {
                    KindHeader::Vector { shapes } => {
                        vector |= !shapes.is_empty();
                        layout |= shapes
                            .iter()
                            .any(|s| s.layout.frame || s.layout.parent.is_some());
                    }
                    KindHeader::Raster { pixels } => {
                        let _ = pixels;
                        // Every legacy new document has a raster Background.
                        // Its presence alone does not establish a Raster mode.
                        raster |= layer.name != "Background";
                    }
                }
            }
            if let Some(mode) = doc.workspace {
                entry.modes.push(mode);
                entry.inferred_modes = false;
            } else {
                for (has, mode) in [
                    (vector, Persona::Design),
                    (raster, Persona::Pixel),
                    (layout, Persona::Layout),
                    (!doc.motion.tracks.is_empty(), Persona::Motion),
                ] {
                    if has {
                        entry.modes.push(mode);
                    }
                }
            }
        }
        Err(error) => entry.error = Some(error),
    }
    entry
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct PreviewKey {
    path: PathBuf,
    modified_ns: u128,
    size: u64,
}
impl PreviewKey {
    fn disk_path(&self) -> PathBuf {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        self.hash(&mut hash);
        cache_root()
            .join("previews")
            .join(format!("{:016x}.png", hash.finish()))
    }
}
#[derive(Clone)]
enum PreviewRequest {
    Document(DocumentEntry),
    Asset {
        root: PathBuf,
        asset: crate::brand::Asset,
    },
}
impl PreviewRequest {
    fn key(&self) -> PreviewKey {
        match self {
            Self::Document(entry) => entry.key(),
            Self::Asset { root, asset } => PreviewKey {
                path: asset.path(root),
                modified_ns: asset.modified_ns,
                size: asset.size,
            },
        }
    }
}

struct PreviewState {
    textures: HashMap<PreviewKey, (Result<TextureHandle, String>, u64)>,
    queue: Vec<PreviewRequest>,
    active: Option<PreviewKey>,
    rx: mpsc::Receiver<(PreviewKey, Result<egui::ColorImage, String>)>,
    tx: mpsc::SyncSender<(PreviewKey, Result<egui::ColorImage, String>)>,
    serial: u64,
}
type SharedPreviews = Arc<Mutex<PreviewState>>;

pub(super) struct Previews {
    shared: SharedPreviews,
    visible: HashSet<PreviewKey>,
    needed: Vec<PreviewRequest>,
}

impl Previews {
    pub(super) fn begin(ctx: &egui::Context) -> Self {
        let shared = ctx.data_mut(|d| {
            let id = egui::Id::new(PREVIEW_ID);
            if let Some(state) = d.get_temp::<SharedPreviews>(id) {
                return state;
            }
            let (tx, rx) = mpsc::sync_channel(1);
            let shared = Arc::new(Mutex::new(PreviewState {
                textures: HashMap::new(),
                queue: Vec::new(),
                active: None,
                rx,
                tx,
                serial: 0,
            }));
            d.insert_temp(id, shared.clone());
            shared
        });
        if let Ok(mut state) = shared.lock() {
            state.serial += 1;
            while let Ok((key, image)) = state.rx.try_recv() {
                if state.active.as_ref() == Some(&key) {
                    state.active = None;
                }
                let texture = image.map(|image| {
                    ctx.load_texture(
                        format!("welcome-{}", key.path.display()),
                        image,
                        egui::TextureOptions::LINEAR,
                    )
                });
                let serial = state.serial;
                state.textures.insert(key, (texture, serial));
            }
        }
        Self {
            shared,
            visible: HashSet::new(),
            needed: Vec::new(),
        }
    }

    pub(super) fn image(
        &mut self,
        ctx: &egui::Context,
        entry: &DocumentEntry,
    ) -> Option<Result<TextureHandle, String>> {
        let _ = ctx;
        self.request(PreviewRequest::Document(entry.clone()))
    }

    /// Reading aspect never schedules work; offscreen masonry can use a cached
    /// render's bounds without decoding files outside the visible viewport.
    pub(super) fn aspect(&self, entry: &DocumentEntry) -> Option<f32> {
        let state = self.shared.lock().ok()?;
        let (Ok(texture), _) = state.textures.get(&entry.key())? else {
            return None;
        };
        let size = texture.size_vec2();
        Some(size.x / size.y.max(1.0))
    }

    pub(super) fn project_images(
        &mut self,
        ctx: &egui::Context,
        project: &ProjectEntry,
    ) -> Vec<Option<Result<TextureHandle, String>>> {
        let _ = ctx;
        let mut images: Vec<_> = project
            .assets
            .iter()
            .take(3)
            .map(|(root, asset)| {
                self.request(PreviewRequest::Asset {
                    root: root.clone(),
                    asset: asset.clone(),
                })
            })
            .collect();
        for document in &project.documents {
            images.push(self.request(PreviewRequest::Document(document.clone())));
        }
        images
    }

    fn request(&mut self, request: PreviewRequest) -> Option<Result<TextureHandle, String>> {
        let key = request.key();
        if !self.visible.insert(key.clone()) {
            return self
                .shared
                .lock()
                .ok()?
                .textures
                .get(&key)
                .map(|(texture, _)| texture.clone());
        }
        if let Ok(mut state) = self.shared.lock() {
            let serial = state.serial;
            if let Some((texture, used)) = state.textures.get_mut(&key) {
                *used = serial;
                return Some(texture.clone());
            }
            if state.active.as_ref() != Some(&key) {
                self.needed.push(request);
            }
        }
        None
    }

    pub(super) fn finish(self, ctx: &egui::Context) {
        let Ok(mut state) = self.shared.lock() else {
            return;
        };
        // Replace the queue every frame: scrolled-away work is cancelled before
        // decoding starts. At most one already-running decode can complete late.
        state.queue = self.needed.into_iter().take(16).collect();
        while state.textures.len() > MAX_TEXTURES {
            let victim = state
                .textures
                .iter()
                .min_by_key(|(key, (_, at))| (self.visible.contains(key), *at))
                .map(|(key, _)| key.clone());
            if let Some(key) = victim {
                state.textures.remove(&key);
            } else {
                break;
            }
        }
        if state.active.is_none() && !state.queue.is_empty() {
            let request = state.queue.remove(0);
            let key = request.key();
            state.active = Some(key.clone());
            let tx = state.tx.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let image = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    render_preview(&request)
                }))
                .unwrap_or_else(|_| Err("The document preview could not be rendered".into()));
                let _ = tx.send((key, image));
                ctx.request_repaint();
            });
        }
    }
}

pub(super) fn previews_ready(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<SharedPreviews>(egui::Id::new(PREVIEW_ID)))
        .is_none_or(|shared| {
            shared
                .lock()
                .is_ok_and(|state| state.active.is_none() && state.queue.is_empty())
        })
}

fn render_preview(request: &PreviewRequest) -> Result<egui::ColorImage, String> {
    let key = request.key();
    // Check again on the worker: a file may have become a symlink since scan.
    for ancestor in key.path.ancestors() {
        let meta = fs::symlink_metadata(ancestor).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() {
            return Err("Linked files and directories are not followed".into());
        }
    }
    let path = key.disk_path();
    if let Ok(mut reader) = image::ImageReader::open(&path) {
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(PREVIEW_EDGE);
        limits.max_image_height = Some(PREVIEW_EDGE);
        limits.max_alloc = Some(u64::from(PREVIEW_EDGE) * u64::from(PREVIEW_EDGE) * 8);
        reader.limits(limits);
        if let Ok(image) = reader.decode() {
            if image.width() <= PREVIEW_EDGE && image.height() <= PREVIEW_EDGE {
                let image = image.to_rgba8();
                return Ok(egui::ColorImage::from_rgba_unmultiplied(
                    [image.width() as usize, image.height() as usize],
                    image.as_raw(),
                ));
            }
        }
    }
    let image = match request {
        PreviewRequest::Document(entry) => {
            if let Some(error) = &entry.error {
                return Err(error.clone());
            }
            let doc = crate::brand::load_preview_document(&entry.path, entry.recovered)?;
            render_document(&doc)?
        }
        PreviewRequest::Asset { root, asset } => {
            crate::brand::load_thumbnail(root, asset, PREVIEW_EDGE)?
        }
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
        let _ = image::save_buffer_with_format(
            &path,
            &image.data,
            image.w,
            image.h,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        );
        prune_disk_previews(parent);
    }
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [image.w as usize, image.h as usize],
        &image.data,
    ))
}

fn prune_disk_previews(directory: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut files: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            (meta.is_file() && entry.path().extension().is_some_and(|e| e == "png"))
                .then(|| (modified(&meta), entry.path()))
        })
        .collect();
    if files.len() <= MAX_DISK_PREVIEWS {
        return;
    }
    files.sort_by_key(|entry| entry.0);
    for (_, path) in files.iter().take(files.len() - MAX_DISK_PREVIEWS) {
        let _ = fs::remove_file(path);
    }
}

fn render_document(doc: &crate::document::Document) -> Result<crate::photo::RgbaImage, String> {
    use crate::geom::{Bounds, Pt};
    let mut bounds = doc
        .artboards
        .iter()
        .map(crate::document::Artboard::bounds)
        .reduce(|a, b| a.union(b))
        .unwrap_or_else(|| Bounds::from_min_size(Pt::ZERO, Pt::new(doc.width, doc.height)));
    for layer in doc.layers.iter().filter(|layer| layer.visible) {
        for shape in layer
            .kind
            .shapes()
            .unwrap_or_default()
            .iter()
            .filter(|s| s.visible && s.layout.frame)
        {
            bounds = bounds.union(shape.world_bbox());
        }
    }
    let width = bounds.width();
    let height = bounds.height();
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err("Invalid preview dimensions".into());
    }
    let scale = PREVIEW_EDGE as f32 / width.max(height);
    let w = (width * scale).round().max(1.0) as u32;
    let h = (height * scale).round().max(1.0) as u32;
    let view = crate::compositor::View {
        scale,
        offset: -bounds.min * scale,
    };
    let pixels = crate::compositor::render_view(doc, view, w, h, crate::compositor::Draft::none())
        .ok_or("Could not render document")?;
    let data = pixels
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let p = pixel.demultiply();
            [p.red(), p.green(), p.blue(), p.alpha()]
        })
        .collect();
    crate::photo::RgbaImage::new(w, h, data).ok_or("Invalid preview image".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "oma-welcome-{}-{}",
                std::process::id(),
                crate::document::next_id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn document(path: &Path, doc: &crate::document::Document) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, crate::project::encode(doc).unwrap()).unwrap();
    }
    fn collect(root: &Path, previous: &Catalog) -> Catalog {
        scan(
            root,
            previous,
            &AtomicBool::new(false),
            &mut |_, _, _| {},
            None,
        )
    }

    #[test]
    fn capture_catalog_root_is_context_local_and_discards_previous_files() {
        let original_home = std::env::var_os("HOME");
        let first = Temp::new();
        let second = Temp::new();
        let first_file = first.0.join("first.oma");
        let second_file = second.0.join("Project/second.oma");
        let doc = crate::document::Document::new("Capture", 20.0, 10.0, 72.0);
        document(&first_file, &doc);
        document(&second_file, &doc);
        fs::create_dir(second.0.join("Project/.omabrand")).unwrap();
        let ctx = egui::Context::default();
        let completed = |ctx: &egui::Context| {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let snapshot = snapshot(ctx);
                if !snapshot.scanning {
                    break snapshot;
                }
                assert!(Instant::now() < deadline, "Capture catalog did not finish");
                std::thread::sleep(Duration::from_millis(1));
            }
        };
        crate::ui::set_capture_catalog_root(&ctx, &first.0);
        let first_snapshot = completed(&ctx);
        assert_eq!(first_snapshot.catalog.documents.len(), 1);
        assert_eq!(first_snapshot.catalog.documents[0].path, first_file);
        assert!(first_snapshot.catalog.recovered.is_empty());
        crate::ui::set_capture_catalog_root(&ctx, &first.0);
        assert!(
            !snapshot(&ctx).scanning,
            "Repeated setup must not restart discovery"
        );

        crate::ui::set_capture_catalog_root(&ctx, &second.0);
        assert!(
            state(&ctx)
                .lock()
                .unwrap()
                .snapshot
                .catalog
                .documents
                .is_empty()
        );
        let second_snapshot = completed(&ctx);
        assert_eq!(second_snapshot.catalog.documents.len(), 1);
        assert_eq!(second_snapshot.catalog.documents[0].path, second_file);
        assert_eq!(second_snapshot.catalog.projects.len(), 1);
        assert!(second_snapshot.catalog.recovered.is_empty());
        assert_eq!(std::env::var_os("HOME"), original_home);
        let normal = egui::Context::default();
        assert!(
            normal
                .data(|d| d.get_temp::<PathBuf>(egui::Id::new(CAPTURE_ROOT_ID)))
                .is_none()
        );
    }

    #[test]
    fn traverses_unlimited_depth_excludes_hidden_trash_and_links_and_finds_nested_projects() {
        let temp = Temp::new();
        let doc = crate::document::Document::new("wide", 60.0, 20.0, 72.0);
        let parent = temp.0.join("Client");
        fs::create_dir_all(parent.join(".omabrand")).unwrap();
        let mut deep = parent.clone();
        for _ in 0..24 {
            deep = deep.join("nested");
        }
        fs::create_dir_all(deep.join(".omabrand")).unwrap();
        document(&deep.join("deep.oma"), &doc);
        document(&parent.join("first.oma"), &doc);
        document(&temp.0.join(".hidden/hidden.oma"), &doc);
        document(&temp.0.join("Trash/deleted.oma"), &doc);
        document(&parent.join(".hidden.oma"), &doc);
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&parent, temp.0.join("linked-dir")).unwrap();
            std::os::unix::fs::symlink(parent.join("first.oma"), temp.0.join("linked.oma"))
                .unwrap();
            std::os::unix::fs::symlink(&temp.0, deep.join("loop")).unwrap();
        }
        let catalog = collect(&temp.0, &Catalog::default());
        assert_eq!(catalog.documents.len(), 2);
        assert_eq!(catalog.projects.len(), 2);
        assert_eq!(catalog.documents_in(&parent).len(), 2);
        assert_eq!(catalog.subprojects(&parent)[0].path, deep);
        assert!(
            catalog
                .documents
                .iter()
                .all(|d| (d.aspect - 3.0).abs() < 0.01)
        );
    }

    #[test]
    fn metadata_skips_invalid_packed_pixels_and_uses_structural_modes() {
        let temp = Temp::new();
        let path = temp.0.join("mixed.oma");
        fs::write(&path, serde_json::to_vec(&serde_json::json!({
            "version": 5,
            "doc": {
                "width": 240, "height": 80,
                "layers": [
                    {"name": "Photo", "kind": {"Raster": {"pixels": {"w":999999,"h":999999,"data":[1,2,3]}}}},
                    {"kind": {"Vector": {"shapes": [{"layout":{"frame":true}}]}}}
                ],
                "motion": {"tracks": [{}]}
            }
        })).unwrap()).unwrap();
        let catalog = collect(&temp.0, &Catalog::default());
        let entry = &catalog.documents[0];
        assert!(entry.error.is_none(), "{:?}", entry.error);
        for mode in [
            Persona::Design,
            Persona::Pixel,
            Persona::Layout,
            Persona::Motion,
        ] {
            assert!(entry.matches_mode(mode));
        }
        assert!(entry.inferred_modes);
        assert_eq!(entry.aspect, 3.0);

        fs::write(&path, r#"{"version":5,"doc":{"width":240,"height":80,"workspace":"photo","layers":[{"name":"Photo","kind":{"Raster":{"pixels":{"data":[1,2,3]}}}}]}}"#).unwrap();
        let catalog = collect(&temp.0, &catalog);
        assert_eq!(catalog.documents[0].modes, vec![Persona::Photo]);
        assert!(!catalog.documents[0].inferred_modes);
    }

    #[test]
    fn refresh_updates_changed_files_removes_deleted_and_keeps_empty_projects() {
        let temp = Temp::new();
        fs::create_dir_all(temp.0.join("Empty/.omabrand")).unwrap();
        let path = temp.0.join("a.oma");
        document(
            &path,
            &crate::document::Document::new("a", 20.0, 10.0, 72.0),
        );
        let first = collect(&temp.0, &Catalog::default());
        assert_eq!(first.projects.len(), 1);
        document(
            &path,
            &crate::document::Document::new("a", 10.0, 30.0, 72.0),
        );
        let second = collect(&temp.0, &first);
        assert!((second.documents[0].aspect - 1.0 / 3.0).abs() < 0.001);
        fs::remove_file(&path).unwrap();
        let third = collect(&temp.0, &second);
        assert!(third.documents.is_empty());
        assert_eq!(third.projects.len(), 1);
    }

    #[test]
    fn newest_modified_first_invalid_files_visible_and_cancel_stops_traversal() {
        let temp = Temp::new();
        let old = temp.0.join("old.oma");
        let recent = temp.0.join("recent.oma");
        let doc = crate::document::Document::new("a", 20.0, 10.0, 72.0);
        document(&old, &doc);
        document(&recent, &doc);
        fs::File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(1))
            .unwrap();
        fs::File::options()
            .write(true)
            .open(&recent)
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(2))
            .unwrap();
        let catalog = collect(&temp.0, &Catalog::default());
        assert_eq!(
            catalog
                .documents
                .iter()
                .map(|d| &d.path)
                .collect::<Vec<_>>(),
            vec![&recent, &old]
        );
        let broken = temp.0.join("broken.oma");
        fs::write(&broken, "not json").unwrap();
        let catalog = collect(&temp.0, &catalog);
        assert!(
            catalog
                .documents
                .iter()
                .find(|d| d.path == broken)
                .unwrap()
                .error
                .is_some()
        );
        let stopped = scan(
            &temp.0,
            &catalog,
            &AtomicBool::new(true),
            &mut |_, _, _| panic!("Cancelled traversal must not publish"),
            None,
        );
        assert!(stopped.documents.is_empty());
    }

    #[test]
    fn renders_real_content_at_natural_aspect() {
        use crate::document::{Fill, Shape, Style};
        use crate::geom::{Geom, Pt};
        let mut doc = crate::document::Document::new("wide", 80.0, 20.0, 72.0);
        doc.layers[1].kind.shapes_mut().unwrap().push(Shape::new(
            Geom::Rect {
                origin: Pt::ZERO,
                size: Pt::new(80.0, 20.0),
                radius: 0.0,
            },
            Style {
                fill: Fill::Solid(crate::color::Rgba::new(233, 24, 55, 255)),
                stroke: None,
                ..Default::default()
            },
        ));
        let image = render_document(&doc).unwrap();
        assert_eq!((image.w, image.h), (384, 96));
        let center = ((image.h / 2 * image.w + image.w / 2) * 4) as usize;
        assert_eq!(&image.data[center..center + 4], &[233, 24, 55, 255]);
    }

    #[test]
    fn saved_and_recovery_previews_decode_object_masks_identically() {
        use crate::document::{Pixels, Shape, Style};
        use crate::geom::{Geom, Pt};
        let temp = Temp::new();
        let mut doc = crate::document::Document::new("masked", 40.0, 20.0, 72.0);
        let mut shape = Shape::new(
            Geom::Rect {
                origin: Pt::ZERO,
                size: Pt::new(40.0, 20.0),
                radius: 0.0,
            },
            Style::default(),
        );
        let mask = vec![0, 0, 0, 0, 255, 255, 255, 255];
        shape.mask = Pixels::from_rgba(2, 1, mask.clone());
        doc.layers[1].kind.shapes_mut().unwrap().push(shape);
        let path = temp.0.join("masked.oma");
        document(&path, &doc);
        let saved = crate::brand::load_preview_document(&path, false).unwrap();
        assert_eq!(
            saved.layers[1].kind.shapes().unwrap()[0]
                .mask
                .as_ref()
                .unwrap()
                .data,
            mask
        );
        let packed: serde_json::Value =
            serde_json::from_str(&crate::project::encode(&doc).unwrap()).unwrap();
        let recovery = temp.0.join("recovery.oma.swp");
        fs::write(&recovery, serde_json::to_vec(&serde_json::json!({"id":"test","original":null,"name":"Recovered design","saved_at":100,"doc":packed["doc"]})).unwrap()).unwrap();
        let entry = index_entry(
            &recovery,
            &fs::metadata(&recovery).unwrap(),
            true,
            None,
            &AtomicBool::new(false),
        );
        assert_eq!(entry.name, "Recovered design");
        assert!(entry.error.is_none());
        let recovered = crate::brand::load_preview_document(&recovery, true).unwrap();
        assert_eq!(
            render_document(&saved).unwrap(),
            render_document(&recovered).unwrap()
        );
    }
}
