//! Filesystem actions run off-thread. Native saves carry portable font archives;
//! no operation overwrites a destination. Delete uses the desktop trash.
use super::catalog;
use crate::{app::Studio, document::Document, tools::Persona, ui::jobs};
use eframe::egui;
use std::{
    fs,
    path::{Path, PathBuf},
};

const JOB: &str = "welcome-file-action";
#[derive(Clone)]
pub(super) enum Action {
    Reveal(PathBuf),
    Trash(Vec<PathBuf>),
    Transfer {
        paths: Vec<PathBuf>,
        destination: PathBuf,
        copy: bool,
    },
    Clone(Vec<PathBuf>),
    Template(Vec<PathBuf>),
    MoveProject {
        source: PathBuf,
        destination: PathBuf,
    },
    New {
        project: PathBuf,
        mode: Persona,
    },
}
struct Done {
    changes: Vec<(PathBuf, Option<PathBuf>)>,
    open: Option<PathBuf>,
    messages: Vec<String>,
}
pub(super) fn busy(ctx: &egui::Context) -> bool {
    jobs::is_running::<Done>(ctx, JOB)
}
pub(super) fn start(ctx: &egui::Context, studio: &mut Studio, action: Action) {
    if busy(ctx) || studio.browser_file_activity() || crate::ui::library::saving(ctx) {
        studio.status = "Wait for the current files to finish opening or saving".into();
        return;
    }
    studio.status = "Working…".into();
    jobs::start(ctx, JOB, move || execute(action));
}
pub(super) fn tick(ctx: &egui::Context, studio: &mut Studio) {
    if let Some(result) = jobs::poll::<Done>(ctx, JOB) {
        match result {
            Ok(done) => {
                for (old, new) in done.changes {
                    studio.relocate_browser_paths(&old, new.as_deref());
                    super::workspace::relocate(ctx, &old, new.as_deref());
                }
                studio.status = done.messages.join(" · ");
                if let Some(path) = done.open {
                    studio.open_path(path);
                }
                catalog::refresh(ctx);
            }
            Err(error) => studio.status = error,
        }
    }
}
pub(crate) fn template_root() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share")
        })
        .join("omadesign/templates")
}
fn run(program: &str, args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().into())
    }
}
fn execute(action: Action) -> Result<Done, String> {
    let mut done = Done {
        changes: vec![],
        open: None,
        messages: vec![],
    };
    match action {
        Action::Reveal(path) => {
            let folder = if path.is_dir() {
                path.as_path()
            } else {
                path.parent().ok_or("No parent folder")?
            };
            run("xdg-open", &[folder.as_os_str()])?;
            done.messages.push(format!("Revealed {}", path.display()));
        }
        Action::MoveProject {
            source,
            destination,
        } => {
            let physical_source = source.canonicalize().map_err(|e| e.to_string())?;
            let physical_destination = destination.canonicalize().map_err(|e| e.to_string())?;
            if physical_destination.starts_with(&physical_source) {
                return Err("A project cannot move inside itself".into());
            }
            let name = physical_source.file_name().ok_or("No project name")?;
            let target = destination.join(name);
            if let Some(backup) = move_project(&physical_source, &physical_destination.join(name))? {
                done.messages
                    .push(format!("Source backup kept at {}", backup.display()));
            }
            done.messages
                .push(format!("Moved project to {}", target.display()));
            // Tabs and catalog selections retain the caller's lexical paths.
            // Also cover tabs originally opened through the canonical root.
            if physical_source != source {
                done.changes.push((physical_source, Some(target.clone())));
            }
            done.changes.push((source, Some(target)));
        }
        Action::New { project, mode } => {
            let mut document = Document::new("Untitled", 1280., 800., 72.);
            document.workspace = Some(mode);
            document.grid.visible = false;
            let target = save_unique(&document, &project, "Untitled")?;
            done.open = Some(target);
        }
        action => {
            let (paths, destination, copy, suffix, trash) = match action {
                Action::Trash(paths) => (paths, None, false, "", true),
                Action::Transfer {
                    paths,
                    destination,
                    copy,
                } => (paths, Some(destination), copy, "", false),
                Action::Clone(paths) => (paths, None, true, " copy", false),
                Action::Template(paths) => {
                    let root = template_root();
                    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
                    (paths, Some(root), true, "", false)
                }
                _ => unreachable!(),
            };
            for source in paths {
                let result = (|| -> Result<(), String> {
                    if trash {
                        run(
                            "gio",
                            &["trash".as_ref(), "--".as_ref(), source.as_os_str()],
                        )?;
                        done.changes.push((source.clone(), None));
                        done.messages
                            .push(format!("Moved {} to Trash", source.display()));
                    } else {
                        let folder = destination
                            .as_deref()
                            .or_else(|| source.parent())
                            .ok_or("No destination")?;
                        let target = transfer_document(&source, folder, copy, suffix)?;
                        if !copy {
                            done.changes.push((source.clone(), Some(target.clone())));
                        }
                        done.messages.push(format!(
                            "{} {}",
                            if copy { "Created" } else { "Moved to" },
                            target.display()
                        ));
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    done.messages.push(format!("{}: {error}", source.display()));
                }
            }
        }
    }
    Ok(done)
}
// Rename is atomic on one filesystem. Across mounts, stage a complete copy,
// check that the source stayed unchanged, and only then retire the source.
fn move_project(source: &Path, target: &Path) -> Result<Option<PathBuf>, String> {
    match rustix::fs::renameat_with(
        rustix::fs::CWD,
        source,
        rustix::fs::CWD,
        target,
        rustix::fs::RenameFlags::NOREPLACE,
    ) {
        Ok(()) => return Ok(None),
        Err(rustix::io::Errno::XDEV) => {}
        Err(error) => return Err(format!("Could not move to {}: {error}", target.display())),
    }
    let stage = target
        .parent()
        .ok_or("No destination folder")?
        .join(format!(".oma-project-{}", crate::document::next_id()));
    fs::create_dir(&stage).map_err(|e| e.to_string())?;
    let result = (|| {
        let before = project_manifest(source)?;
        copy_tree(source, &stage)?;
        if project_manifest(source)? != before {
            return Err("Project changed during the move; original kept".into());
        }
        rename_new(&stage, target)?;
        let retired = source.with_file_name(format!(".oma-moved-{}", crate::document::next_id()));
        rename_new(source, &retired)
            .map_err(|e| format!("Copy kept at {}; {e}", target.display()))?;
        if project_manifest(&retired).as_ref() != Ok(&before) {
            let _ = rename_new(&retired, source);
            return Err(format!(
                "Project changed during the move; copies kept at {} and {}",
                target.display(),
                if source.exists() { source } else { &retired }.display()
            ));
        }
        Ok(fs::remove_dir_all(&retired).err().map(|_| retired))
    })();
    if stage.exists() {
        let _ = fs::remove_dir_all(&stage);
    }
    result
}
#[derive(PartialEq, Eq)]
struct EntryStamp {
    path: PathBuf,
    size: u64,
    modified: std::time::SystemTime,
    mode: u32,
    link: Option<PathBuf>,
}
fn project_manifest(root: &Path) -> Result<Vec<EntryStamp>, String> {
    fn walk(root: &Path, folder: &Path, out: &mut Vec<EntryStamp>) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        for entry in fs::read_dir(folder).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if !meta.is_dir() && !meta.is_file() && !meta.file_type().is_symlink() {
                return Err(format!("Cannot move special file {}", path.display()));
            }
            out.push(EntryStamp {
                path: path.strip_prefix(root).unwrap().to_owned(),
                size: meta.len(),
                modified: meta.modified().map_err(|e| e.to_string())?,
                mode: meta.permissions().mode(),
                link: if meta.file_type().is_symlink() {
                    Some(fs::read_link(&path).map_err(|e| e.to_string())?)
                } else {
                    None
                },
            });
            if meta.is_dir() {
                walk(root, &path, out)?;
            }
        }
        Ok(())
    }
    let mut entries = vec![];
    walk(root, root, &mut entries)?;
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}
fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    for entry in fs::read_dir(source).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        let meta = fs::symlink_metadata(&from).map_err(|e| e.to_string())?;
        if meta.is_dir() {
            fs::create_dir(&to).map_err(|e| e.to_string())?;
            copy_tree(&from, &to)?;
        } else if meta.is_file() {
            fs::copy(&from, &to).map_err(|e| e.to_string())?;
            fs::File::open(&to)
                .and_then(|f| {
                    f.set_times(fs::FileTimes::new().set_modified(meta.modified()?))?;
                    f.sync_all()
                })
                .map_err(|e| e.to_string())?;
        } else if meta.file_type().is_symlink() {
            std::os::unix::fs::symlink(fs::read_link(&from).map_err(|e| e.to_string())?, &to)
                .map_err(|e| e.to_string())?;
        } else {
            return Err(format!("Cannot move special file {}", from.display()));
        }
    }
    fs::set_permissions(
        destination,
        fs::metadata(source)
            .map_err(|e| e.to_string())?
            .permissions(),
    )
    .map_err(|e| e.to_string())
}

fn rename_new(source: &Path, destination: &Path) -> Result<(), String> {
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        source,
        rustix::fs::CWD,
        destination,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(|e| format!("Could not move to {}: {e}", destination.display()))
}
fn stamp(path: &Path) -> Result<(u64, std::time::SystemTime), String> {
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("Choose a regular document file".into());
    }
    Ok((meta.len(), meta.modified().map_err(|e| e.to_string())?))
}
fn transfer_document(
    source: &Path,
    folder: &Path,
    copy: bool,
    suffix: &str,
) -> Result<PathBuf, String> {
    let original = stamp(source)?;
    let folder = folder.canonicalize().map_err(|e| e.to_string())?;
    if !copy && source.parent().and_then(|p| p.canonicalize().ok()).as_ref() == Some(&folder) {
        return Err("This document is already in that project".into());
    }
    let doc = crate::project::load_from(source)?;
    let stem = format!(
        "{}{suffix}",
        source.file_stem().unwrap_or_default().to_string_lossy()
    );
    let target = save_unique(&doc, &folder, &stem)?;
    if !copy {
        if stamp(source)? != original {
            return Err(format!(
                "Source changed during the move; both files kept. Copy: {}",
                target.display()
            ));
        }
        fs::remove_file(source).map_err(|e| {
            format!(
                "Copy kept at {}; original could not be removed: {e}",
                target.display()
            )
        })?;
    }
    Ok(target)
}
fn save_unique(doc: &Document, folder: &Path, stem: &str) -> Result<PathBuf, String> {
    // Save to a private staging file first, then publish without replace.
    // Font archives belong to the destination directory, so the staging file is
    // beside the final file and save_to archives the dependencies there.
    let temporary = folder.join(format!(
        ".oma-browser-{}-{}.oma",
        std::process::id(),
        crate::document::next_id()
    ));
    crate::project::save_to(doc, &temporary)?;
    let result = (|| {
        for index in 0..10000 {
            let name = if index == 0 {
                format!("{stem}.oma")
            } else {
                format!("{stem} {index}.oma")
            };
            let target = folder.join(name);
            match rustix::fs::renameat_with(
                rustix::fs::CWD,
                &temporary,
                rustix::fs::CWD,
                &target,
                rustix::fs::RenameFlags::NOREPLACE,
            ) {
                Ok(()) => return Ok(target),
                Err(rustix::io::Errno::EXIST) => continue,
                Err(error) => return Err(error.to_string()),
            }
        }
        Err("Too many files with that name".into())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copies_and_moves_preserve_documents_and_never_replace_existing_files() {
        let root = std::env::temp_dir().join(format!("oma-browser-{}", crate::document::next_id()));
        fs::create_dir_all(root.join("project")).unwrap();
        let doc = Document::new("Original", 80., 60., 72.);
        let source = root.join("art.oma");
        crate::project::save_to(&doc, &source).unwrap();
        let original = fs::read(&source).unwrap();
        let destination = root.join("project");
        fs::write(destination.join("art.oma"), b"existing").unwrap();
        let copy = transfer_document(&source, &destination, true, "").unwrap();
        assert_eq!(copy.file_name().unwrap(), "art 1.oma");
        assert_eq!(fs::read(&source).unwrap(), original);
        assert_eq!(fs::read(destination.join("art.oma")).unwrap(), b"existing");
        assert_eq!(crate::project::load_from(&copy).unwrap().name, "Original");
        let moved = transfer_document(&source, &destination, false, "").unwrap();
        assert!(!source.exists());
        assert_eq!(crate::project::load_from(&moved).unwrap().width, 80.);
        assert!(transfer_document(&moved, &destination, false, "").is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn new_documents_use_each_requested_mode_and_project_moves_relocate_every_file() {
        let root = std::env::temp_dir().join(format!("oma-modes-{}", crate::document::next_id()));
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        for mode in [Persona::Design, Persona::Pixel, Persona::Layout] {
            let done = execute(Action::New {
                project: source.clone(),
                mode,
            })
            .unwrap();
            let doc = crate::project::load_from(&done.open.unwrap()).unwrap();
            assert_eq!(doc.workspace, Some(mode));
        }
        let done = execute(Action::MoveProject {
            source: source.clone(),
            destination: destination.clone(),
        })
        .unwrap();
        assert!(!source.exists());
        assert_eq!(fs::read_dir(destination.join("source")).unwrap().count(), 3);
        assert_eq!(
            done.changes,
            vec![(source, Some(destination.join("source")))]
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn project_moves_reject_self_and_existing_destinations() {
        let root = std::env::temp_dir().join(format!("oma-project-{}", crate::document::next_id()));
        let source = root.join("source");
        let target = root.join("target");
        fs::create_dir_all(source.join("child")).unwrap();
        fs::create_dir_all(target.join("source")).unwrap();
        assert!(
            execute(Action::MoveProject {
                source: source.clone(),
                destination: source.join("child")
            })
            .is_err()
        );
        assert!(
            execute(Action::MoveProject {
                source: source.clone(),
                destination: target
            })
            .is_err()
        );
        assert!(source.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_move_through_symlink_relocates_lexical_and_canonical_tabs() {
        let root = std::env::temp_dir().join(format!("oma-project-alias-{}", crate::document::next_id()));
        let physical = root.join("physical");
        let alias = root.join("alias");
        fs::create_dir_all(physical.join("source")).unwrap();
        fs::create_dir_all(physical.join("destination")).unwrap();
        std::os::unix::fs::symlink(&physical, &alias).unwrap();
        let doc = Document::new("Open work", 80., 60., 72.);
        crate::project::save_to(&doc, &physical.join("source/art.oma")).unwrap();
        let mut studio = Studio::new();
        studio.open_document(doc.clone(), Some(alias.join("source/art.oma")));
        studio.open_document(doc, Some(physical.join("source/art.oma")));
        let done = execute(Action::MoveProject {
            source: alias.join("source"),
            destination: alias.join("destination"),
        }).unwrap();
        for (old, new) in done.changes {
            studio.relocate_browser_paths(&old, new.as_deref());
        }
        let target = alias.join("destination/source/art.oma");
        assert!(target.is_file());
        assert!(!physical.join("source").exists());
        assert_eq!(studio.tab_count(), 2);
        assert_eq!(studio.tab_path(0), Some(target.as_path()));
        assert_eq!(studio.tab_path(1), Some(target.as_path()));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn project_move_across_filesystems_preserves_hidden_assets_and_links() {
        use std::os::unix::fs::MetadataExt;
        let source =
            std::env::temp_dir().join(format!("oma-cross-source-{}", crate::document::next_id()));
        let target = PathBuf::from("/dev/shm")
            .join(format!("oma-cross-target-{}", crate::document::next_id()));
        fs::create_dir_all(source.join(".omabrand/fonts")).unwrap();
        fs::write(source.join(".omabrand/fonts/font.ttf"), b"font fixture").unwrap();
        std::os::unix::fs::symlink(".omabrand", source.join("brand-link")).unwrap();
        assert_ne!(
            fs::metadata(&source).unwrap().dev(),
            fs::metadata("/dev/shm").unwrap().dev()
        );
        assert!(move_project(&source, &target).unwrap().is_none());
        assert!(!source.exists());
        assert_eq!(
            fs::read(target.join(".omabrand/fonts/font.ttf")).unwrap(),
            b"font fixture"
        );
        assert_eq!(
            fs::read_link(target.join("brand-link")).unwrap(),
            PathBuf::from(".omabrand")
        );
        fs::remove_dir_all(target).unwrap();
    }
}
