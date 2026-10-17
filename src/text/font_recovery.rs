//! User-picked font discovery and explicit collision-safe installation.
use super::*;
use crate::ml::Progress;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::{Read, Write},
    time::{Duration, Instant},
};
#[derive(Clone, Debug)]
pub(crate) struct Need {
    pub family: String,
    pub style: String,
    pub request: Option<(u16, u8)>,
}
#[derive(Clone, Debug)]
pub(crate) struct Found {
    pub path: PathBuf,
    pub names: Vec<String>,
    pub size: u64,
    pub sha256: [u8; 32],
}
#[derive(Clone, Default)]
pub(crate) struct Search {
    pub hits: Vec<Found>,
    pub folders: u64,
    pub read: u64,
    pub skipped: u64,
    pub unreadable: u64,
    pub end: Option<String>,
}
const FILE_LIMIT: u64 = 256 << 20;
fn regular(path: &Path) -> Result<File, String> {
    #[cfg(unix)]
    let f: File = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::CLOEXEC
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK,
        rustix::fs::Mode::empty(),
    )
    .map_err(|e| format!("Could not read {}: {e}", path.display()))?
    .into();
    #[cfg(not(unix))]
    let f = File::open(path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    let m = f.metadata().map_err(|e| e.to_string())?;
    if !m.is_file() || m.len() > FILE_LIMIT {
        return Err("Choose a regular font file no larger than 256 MiB".into());
    }
    Ok(f)
}
fn read(path: &Path) -> Result<Vec<u8>, String> {
    let file = regular(path)?;
    let mut bytes = vec![];
    file.take(FILE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > FILE_LIMIT {
        return Err("Font file grew beyond 256 MiB".into());
    }
    Ok(bytes)
}
fn parsed(bytes: Vec<u8>) -> Result<usvg::fontdb::Database, String> {
    let mut db = usvg::fontdb::Database::new();
    db.load_font_data(bytes);
    if db.is_empty() {
        return Err("This file is not a supported OpenType font".into());
    }
    Ok(db)
}
fn names(db: &usvg::fontdb::Database, needs: &[Need]) -> Vec<String> {
    let mut names = BTreeSet::new();
    for face in db.faces() {
        let style = db
            .with_face_data(face.id, |data, index| {
                rustybuzz::ttf_parser::Face::parse(data, index)
                    .ok()
                    .and_then(|f| font_inventory::name(&f, &[17, 2]))
            })
            .flatten()
            .unwrap_or_default();
        for need in needs {
            if face
                .families
                .iter()
                .any(|(n, _)| n.eq_ignore_ascii_case(&need.family))
                && need.request.map_or_else(
                    || need.style.is_empty() || style.eq_ignore_ascii_case(&need.style),
                    |(weight, slant)| {
                        face.weight.0 == weight
                            && match face.style {
                                usvg::fontdb::Style::Italic => slant == 1,
                                usvg::fontdb::Style::Oblique => slant == 2,
                                _ => slant == 0,
                            }
                    },
                )
            {
                names.insert(format!("{} {}", face.families[0].0, style));
            }
        }
    }
    names.into_iter().collect()
}
fn blocked(parent: &Path, name: &str) -> bool {
    let n = name.to_lowercase();
    if [
        "app",
        "appex",
        "bundle",
        "component",
        "fcpbundle",
        "framework",
        "imovielibrary",
        "kext",
        "musiclibrary",
        "photolibrary",
        "photoslibrary",
        "plugin",
        "tvlibrary",
        "xpc",
    ]
    .contains(
        &Path::new(&n)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or(""),
    ) {
        return true;
    }
    if [
        "program files",
        "program files (x86)",
        "program files (arm)",
        "programdata",
        "$recycle.bin",
        "system volume information",
    ]
    .contains(&n.as_str())
    {
        return true;
    }
    let p = parent
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    if p == "users" && ["shared", "public"].contains(&n.as_str())
        || p == "library"
            && [
                "application support",
                "containers",
                "group containers",
                "caches",
                "preferences",
            ]
            .contains(&n.as_str())
    {
        return true;
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let is_home = home.as_deref() == Some(parent)
        || parent
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|s| s.eq_ignore_ascii_case("home") || s.eq_ignore_ascii_case("users"));
    if is_home
        && (n.starts_with('.')
            || ["library", "appdata", "applications", "snap"].contains(&n.as_str()))
    {
        return true;
    }
    let siblings = |a: &str, b: &str| parent.join(a).is_dir() && parent.join(b).is_dir();
    if siblings("etc", "usr") && (parent.join("bin").exists() || parent.join("sbin").exists()) {
        return [
            "bin",
            "boot",
            "dev",
            "efi",
            "etc",
            "gnu",
            "lib",
            "lib32",
            "lib64",
            "libx32",
            "lost+found",
            "nix",
            "opt",
            "proc",
            "root",
            "run",
            "sbin",
            "snap",
            "sys",
            "usr",
            "var",
        ]
        .contains(&n.as_str());
    }
    if siblings("Users", "System")
        || siblings("Users", "Library")
        || siblings("Users", "Applications")
    {
        return [
            "applications",
            "bin",
            "cores",
            "dev",
            "etc",
            "home",
            "library",
            "net",
            "network",
            "opt",
            "private",
            "sbin",
            "system",
            "tmp",
            "usr",
            "var",
        ]
        .contains(&n.as_str());
    }
    if siblings("Windows", "Users") || siblings("Windows", "Program Files") {
        return [
            "windows",
            "windows.old",
            "recovery",
            "perflogs",
            "boot",
            "config.msi",
            "msocache",
            "$windows.~bt",
            "$windows.~ws",
            "$winreagent",
            "$sysreset",
        ]
        .contains(&n.as_str());
    }
    false
}
fn allowed(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("Choose an absolute folder path".into());
    }
    let path = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    if !path.is_dir() {
        return Err("Choose a folder".into());
    }
    for env in [
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
        "XDG_STATE_HOME",
        "HOST_XDG_CONFIG_HOME",
        "HOST_XDG_DATA_HOME",
        "HOST_XDG_CACHE_HOME",
        "HOST_XDG_STATE_HOME",
    ] {
        if let Some(root) = std::env::var_os(env).and_then(|p| std::fs::canonicalize(p).ok())
            && path.starts_with(root)
        {
            return Err("Choose a folder containing your font files, outside app data".into());
        }
    }
    let mut parent = PathBuf::new();
    for part in path.components() {
        if let std::path::Component::Normal(name) = part
            && blocked(&parent, &name.to_string_lossy())
        {
            return Err("Font search does not enter system folders, app data or packages".into());
        }
        parent.push(part);
    }
    Ok(path)
}
/// Search only a user-picked folder for exact missing font families and styles.
/// Args: absolute folder, needs, 1–600 second deadline, shared partial results and cancellation.
/// Returns a stopped/done snapshot; no installation or document mutation occurs.
pub(crate) fn search(
    folder: &Path,
    needs: &[Need],
    seconds: u32,
    state: &Mutex<Search>,
    progress: &dyn Progress,
) -> Result<Search, String> {
    if needs.is_empty() || needs.len() > 1000 || !(1..=600).contains(&seconds) {
        return Err("Choose 1–1000 missing fonts and a search time from 1 to 600 seconds".into());
    }
    let root = allowed(folder)?;
    let start = Instant::now();
    let mut todo = vec![(root, 0)];
    let mut result = Search::default();
    let mut entries = 0u64;
    let end = 'walk: loop {
        if progress.cancelled() {
            break "Stopped";
        }
        if start.elapsed() >= Duration::from_secs(seconds.into()) {
            break "Time limit reached";
        }
        let Some((dir, depth)) = todo.pop() else {
            break "Done";
        };
        let reader = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(_) => {
                result.unreadable += 1;
                continue;
            }
        };
        result.folders += 1;
        for entry in reader {
            if progress.cancelled() {
                break 'walk "Stopped";
            }
            if start.elapsed() >= Duration::from_secs(seconds.into()) {
                break 'walk "Time limit reached";
            }
            entries += 1;
            if entries > 10_000_000
                || todo.len() >= 1_000_000
                || result.read >= 10_000
                || result.hits.len() >= 1000
            {
                break 'walk "Search limit reached";
            }
            let Ok(entry) = entry else {
                result.unreadable += 1;
                continue;
            };
            let Ok(kind) = entry.file_type() else {
                result.unreadable += 1;
                continue;
            };
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if kind.is_symlink() {
                result.skipped += 1;
                continue;
            }
            if kind.is_dir() {
                if depth >= 64
                    || name.starts_with('.')
                    || blocked(&dir, &name)
                    || allowed(&entry.path()).is_err()
                {
                    result.skipped += 1;
                } else {
                    todo.push((entry.path(), depth + 1));
                }
                continue;
            }
            if !kind.is_file()
                || !["ttf", "otf", "ttc", "otc"].contains(
                    &entry
                        .path()
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .as_str(),
                )
            {
                continue;
            }
            let bytes = match read(&entry.path()) {
                Ok(b) => b,
                Err(_) => {
                    result.unreadable += 1;
                    continue;
                }
            };
            result.read += 1;
            let size = bytes.len() as u64;
            let sha256 = Sha256::digest(&bytes).into();
            if let Ok(db) = parsed(bytes) {
                let names = names(&db, needs);
                if !names.is_empty() {
                    result.hits.push(Found {
                        path: entry.path(),
                        names,
                        size,
                        sha256,
                    });
                }
            }
            *state
                .lock()
                .map_err(|_| "Font search state is unavailable")? = result.clone();
            progress.report("Searching font files", result.read as usize, 10_000);
        }
    };
    result.end = Some(end.into());
    *state
        .lock()
        .map_err(|_| "Font search state is unavailable")? = result.clone();
    Ok(result)
}
/// Install explicitly selected, unchanged font files without overwriting an existing file.
/// Args: search hits, app-owned destination and cancellation. Returns verified installed paths.
pub(crate) fn install(
    found: &[Found],
    destination: &Path,
    progress: &dyn Progress,
) -> Result<Vec<PathBuf>, String> {
    if found.is_empty() || found.len() > 1000 {
        return Err("Choose 1–1000 font files to add".into());
    }
    let total = found
        .iter()
        .try_fold(0u64, |n, f| n.checked_add(f.size))
        .ok_or("Font sizes overflow")?;
    if total > 1 << 30 {
        return Err("Add fewer font files: selected files exceed 1 GiB".into());
    }
    std::fs::create_dir_all(destination).map_err(|e| e.to_string())?;
    let count = found.len();
    let mut installed = vec![];
    for (i, found) in found.iter().enumerate() {
        progress.check()?;
        let bytes = read(&found.path)?;
        if bytes.len() as u64 != found.size
            || <[u8; 32]>::from(Sha256::digest(&bytes)) != found.sha256
        {
            return Err(
                "A font file changed after the search. Search again before adding it".into(),
            );
        }
        let db = parsed(bytes.clone())?;
        let original = found
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("Font filename is not readable")?;
        let mut accepted = None;
        for n in 0..1000 {
            let name = if n == 0 {
                original.into()
            } else {
                format!("{n}-{original}")
            };
            let path = destination.join(name);
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut f) => {
                    let result = (|| {
                        for chunk in bytes.chunks(64 * 1024) {
                            progress.check()?;
                            f.write_all(chunk).map_err(|e| e.to_string())?;
                        }
                        f.sync_all().map_err(|e| e.to_string())
                    })();
                    drop(f);
                    if let Err(error) = result {
                        let _ = std::fs::remove_file(&path);
                        return Err(error);
                    }
                    accepted = Some(path);
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if read(&path).is_ok_and(|old| old == bytes) {
                        accepted = Some(path);
                        break;
                    }
                }
                Err(error) => return Err(error.to_string()),
            }
        }
        let path = accepted.ok_or("Every numbered filename is already occupied")?;
        let label = db
            .faces()
            .next()
            .and_then(|f| f.families.first())
            .map(|f| f.0.clone())
            .unwrap_or_else(|| original.into());
        register_font(FontFace {
            name: label,
            path: path.clone(),
        });
        installed.push(path);
        progress.report("Adding font files", i + 1, count);
    }
    Ok(installed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Scratch(PathBuf);
    impl Scratch {
        fn new(name: &str) -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(1);
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join(".artifacts/font-recovery-tests")
                .join(format!(
                    "{}-{name}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn source() -> Vec<u8> {
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/assets/fonts/EBGaramond.ttf"
        ))
        .unwrap()
    }
    fn need() -> Need {
        let bytes = source();
        let face = rustybuzz::ttf_parser::Face::parse(&bytes, 0).unwrap();
        Need {
            family: font_inventory::name(&face, &[16, 1]).unwrap(),
            style: font_inventory::name(&face, &[17, 2]).unwrap(),
            request: None,
        }
    }
    fn found(path: PathBuf) -> Found {
        let bytes = read(&path).unwrap();
        Found {
            path,
            names: vec![],
            size: bytes.len() as u64,
            sha256: Sha256::digest(&bytes).into(),
        }
    }
    #[test]
    fn craft_find_font_recovery_search_exact_alias_partial_stop_and_folder_guards() {
        let root = Scratch::new("search");
        let source = source();
        std::fs::write(root.0.join("actual.ttf"), &source).unwrap();
        std::fs::write(root.0.join("malformed.otf"), b"Not a font").unwrap();
        for folder in [".hidden", "Other.app"] {
            std::fs::create_dir(root.0.join(folder)).unwrap();
            std::fs::write(root.0.join(folder).join("skip.ttf"), &source).unwrap();
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.0.join("actual.ttf"), root.0.join("linked.ttf")).unwrap();
        let state = Mutex::new(Search::default());
        let result = search(&root.0, &[need()], 60, &state, &crate::ml::NoProgress).unwrap();
        assert_eq!(result.end.as_deref(), Some("Done"));
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].path, root.0.join("actual.ttf"));
        assert_eq!(
            result.hits[0].sha256,
            <[u8; 32]>::from(Sha256::digest(&source))
        );
        assert!(result.skipped >= 2);
        assert!(
            search(
                &root.0.join("Other.app"),
                &[need()],
                60,
                &state,
                &crate::ml::NoProgress
            )
            .is_err()
        );
        assert!(
            search(
                Path::new("relative"),
                &[need()],
                60,
                &state,
                &crate::ml::NoProgress
            )
            .is_err()
        );
        assert!(search(&root.0, &[need()], 0, &state, &crate::ml::NoProgress).is_err());
        struct Stop;
        impl Progress for Stop {
            fn report(&self, _: &str, _: usize, _: usize) {}
            fn cancelled(&self) -> bool {
                true
            }
        }
        assert_eq!(
            search(&root.0, &[need()], 60, &state, &Stop)
                .unwrap()
                .end
                .as_deref(),
            Some("Stopped")
        );
        let missing = Need {
            family: "Not this actual font family".into(),
            style: String::new(),
            request: None,
        };
        assert!(
            search(&root.0, &[missing], 60, &state, &crate::ml::NoProgress)
                .unwrap()
                .hits
                .is_empty()
        );
    }
    #[test]
    fn craft_find_font_recovery_add_preserves_collision_original_and_changed_source() {
        let root = Scratch::new("add");
        let src = root.0.join("src");
        let dest = root.0.join("fonts");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dest).unwrap();
        let path = src.join("font.ttf");
        std::fs::write(&path, source()).unwrap();
        let candidate = found(path.clone());
        std::fs::write(dest.join("font.ttf"), b"Preserve accepted existing file").unwrap();
        let result = install(&[candidate.clone()], &dest, &crate::ml::NoProgress).unwrap();
        assert_eq!(result, vec![dest.join("1-font.ttf")]);
        assert_eq!(
            std::fs::read(dest.join("font.ttf")).unwrap(),
            b"Preserve accepted existing file"
        );
        assert_eq!(read(&result[0]).unwrap(), source());
        let again = install(&[candidate.clone()], &dest, &crate::ml::NoProgress).unwrap();
        assert_eq!(again, result);
        assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 2);
        std::fs::write(&path, b"changed source").unwrap();
        assert!(install(&[candidate], &dest, &crate::ml::NoProgress).is_err());
        assert_eq!(read(&result[0]).unwrap(), source());
    }
    #[test]
    fn craft_find_font_recovery_cancel_removes_only_its_partial_copy_and_rejects_invalid_bytes() {
        let root = Scratch::new("cancel");
        let path = root.0.join("actual.ttf");
        std::fs::write(&path, source()).unwrap();
        struct Cancel(AtomicUsize);
        impl Progress for Cancel {
            fn report(&self, _: &str, _: usize, _: usize) {}
            fn cancelled(&self) -> bool {
                self.0.fetch_add(1, Ordering::Relaxed) >= 3
            }
        }
        let destination = root.0.join("fonts");
        std::fs::create_dir_all(&destination).unwrap();
        std::fs::write(destination.join("keep"), b"keep").unwrap();
        assert!(
            install(
                &[found(path.clone())],
                &destination,
                &Cancel(AtomicUsize::new(0))
            )
            .is_err()
        );
        assert_eq!(std::fs::read_dir(&destination).unwrap().count(), 1);
        assert_eq!(read(&path).unwrap(), source());
        let bad = root.0.join("bad.ttf");
        std::fs::write(&bad, b"not an actual font").unwrap();
        assert!(install(&[found(bad)], &destination, &crate::ml::NoProgress).is_err());
        assert_eq!(std::fs::read(destination.join("keep")).unwrap(), b"keep");
    }
}
