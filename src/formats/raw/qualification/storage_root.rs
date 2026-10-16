//! Bind the one scheduled external case to its authorized mount and directory handles.
use super::*;
use std::{
    os::unix::{fs::MetadataExt, io::AsRawFd},
    sync::Arc,
};

const MAC_ROOT: &str = "/mnt/mac/omadesign-craft-gap-stack";
const ROUTE_SHA256: &str = "80b47af7ce581da23b0ad157560f6a4f965f8f1c7121ea4f8b7ed0b27b06f75a";

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Destination {
    route_packet: PathBuf,
    route_sha256: String,
    mount_id: String,
    device: String,
    root_device: String,
    root_inode: String,
}

#[derive(Deserialize)]
struct Route {
    mount: String,
    filesystem: String,
    remote_source: String,
    project_root: PathBuf,
}

pub(super) struct Mounted {
    expected: Destination,
    route: Route,
    root: Arc<File>,
}

/// Match the exact observed mounted volume, including its kernel mount identity.
/// Args: mountinfo bytes and pinned destination/route. Returns refusal for disappearance, replacement or a different source/type.
fn match_mount(text: &str, expected: &Destination, route: &Route) -> Result<(), String> {
    let found = text.lines().find_map(|line| {
        let (before, after) = line.split_once(" - ")?;
        let left: Vec<_> = before.split_whitespace().collect();
        let right: Vec<_> = after.split_whitespace().collect();
        (left.len() >= 6 && right.len() >= 3 && left[4] == route.mount).then_some((left, right))
    });
    if let Some((left, right)) = found
        && left[0] == expected.mount_id
        && left[2] == expected.device
        && left[3] == "/"
        && right[0] == route.filesystem
        && right[1] == route.remote_source
    {
        return Ok(());
    }
    Err("Scheduled destination mount disappeared or changed; refusing filesystem fallback".into())
}

/// Address a retained directory handle without resolving a possibly replaced mount path.
/// Args: live directory file. Returns its process-local path; the handle must outlive every use.
pub(super) fn fd_path(directory: &File) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()))
}

impl Mounted {
    /// Open only the SHA-bound route selected by Michael for the X2D case.
    /// Args: scheduled identity. Returns retained mount-root ownership after checking packet, mount and inode.
    pub(super) fn open(expected: &Destination) -> Result<Self, String> {
        if expected.route_sha256 != ROUTE_SHA256 || !expected.route_packet.starts_with("/home") {
            return Err("External case needs the exact approved route packet".into());
        }
        let packet = fs::read(&expected.route_packet).map_err(|e| e.to_string())?;
        if format!("{:x}", Sha256::digest(&packet)) != ROUTE_SHA256 {
            return Err("Scheduled route packet changed".into());
        }
        let route: Route = serde_json::from_slice(&packet).map_err(|e| e.to_string())?;
        if route.project_root != Path::new(MAC_ROOT)
            || route.mount != "/mnt/mac"
            || route.filesystem != "fuse.sshfs"
        {
            return Err("External route is outside the approved Mac project".into());
        }
        match_mount(
            &fs::read_to_string("/proc/self/mountinfo").map_err(|e| e.to_string())?,
            expected,
            &route,
        )?;
        let root = Arc::new(File::open(&route.project_root).map_err(|e| e.to_string())?);
        let mounted = Self {
            expected: expected.clone(),
            route,
            root,
        };
        mounted.validate()?;
        Ok(mounted)
    }

    /// Recheck pathname and kernel mount identity before every guarded write.
    /// Args: none. Returns an error before publishing to any replacement or underlying filesystem.
    pub(super) fn validate(&self) -> Result<(), String> {
        match_mount(
            &fs::read_to_string("/proc/self/mountinfo").map_err(|e| e.to_string())?,
            &self.expected,
            &self.route,
        )?;
        let current = fs::symlink_metadata(&self.route.project_root).map_err(|e| e.to_string())?;
        let held = self.root.metadata().map_err(|e| e.to_string())?;
        if !current.is_dir()
            || current.dev().to_string() != self.expected.root_device
            || current.ino().to_string() != self.expected.root_inode
            || current.dev() != held.dev()
            || current.ino() != held.ino()
            || self
                .route
                .project_root
                .canonicalize()
                .map_err(|e| e.to_string())?
                != self.route.project_root
        {
            return Err("Scheduled destination directory identity changed".into());
        }
        Ok(())
    }

    /// Retain an actual case directory on the approved filesystem.
    /// Args: requested directory or its live FD alias. Returns another owned handle after ancestry/device validation.
    pub(super) fn pin_directory(&self, path: &Path) -> Result<File, String> {
        self.validate()?;
        let directory = File::open(path).map_err(|e| e.to_string())?;
        let metadata = directory.metadata().map_err(|e| e.to_string())?;
        let resolved = fd_path(&directory)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if !metadata.is_dir()
            || metadata.dev().to_string() != self.expected.root_device
            || resolved.parent() != Some(self.route.project_root.as_path())
        {
            return Err("Case directory is outside the pinned destination".into());
        }
        self.validate()?;
        Ok(directory)
    }

    pub(super) fn report(&self) -> Value {
        json!({"kind":"approved_mac_sshfs","route_packet_sha256":ROUTE_SHA256,"fd_anchored":true,
            "mount_identity_sha256":format!("{:x}",Sha256::digest(format!("{}:{}:{}:{}",self.expected.mount_id,self.expected.device,self.expected.root_device,self.expected.root_inode).as_bytes())),
            "boundary":"API fsync/rename checks; no physical Mac durability claim"})
    }
}

/// Keep an owned case directory anchored across every write, cleanup and rename.
/// Args: fresh home path or the explicit scheduled Mac child. Returns live directory ownership; no recursive cleanup is performed.
pub(in crate::formats::raw) struct Scratch {
    path: PathBuf,
    _directory: Option<File>,
    mount: Option<Mounted>,
    name: Option<std::ffi::OsString>,
}
impl Scratch {
    pub(in crate::formats::raw) fn create(path: &Path) -> Result<Self, String> {
        if path.starts_with("/home") {
            return Ok(Self {
                path: super::super::create_scratch(path)?,
                _directory: None,
                mount: None,
                name: None,
            });
        }
        let name = path
            .file_name()
            .filter(|name| !name.is_empty())
            .ok_or("Missing scheduled case directory name")?;
        if path.parent() != Some(Path::new(MAC_ROOT))
            || path
                .components()
                .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
        {
            return Err(
                "Case directory must be a new direct child of the approved Mac project".into(),
            );
        }
        let (plan, _) = scheduled()?.ok_or("External case has no schedule")?;
        if !approved(&plan) {
            return Err("External destination is only approved for the pinned X2D case".into());
        }
        let mounted = Mounted::open(
            plan.destination
                .as_ref()
                .ok_or("No scheduled Mac destination")?,
        )?;
        let anchored = fd_path(&mounted.root).join(name);
        fs::create_dir(&anchored).map_err(|e| e.to_string())?;
        let created = fs::symlink_metadata(&anchored).map_err(|e| e.to_string())?;
        let directory = match mounted.pin_directory(&anchored) {
            Ok(directory) => directory,
            Err(error) => {
                let _ = fs::remove_dir(&anchored);
                return Err(error);
            }
        };
        let pinned = directory.metadata().map_err(|e| e.to_string())?;
        if !created.is_dir() || created.dev() != pinned.dev() || created.ino() != pinned.ino() {
            return Err("New case directory was replaced before it could be pinned".into());
        }
        Ok(Self {
            path: fd_path(&directory),
            _directory: Some(directory),
            mount: Some(mounted),
            name: Some(name.to_owned()),
        })
    }
    pub(in crate::formats::raw) fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if let (Some(mount), Some(name)) = (&self.mount, &self.name) {
            let _ = fs::remove_dir(fd_path(&mount.root).join(name));
        }
    }
}

/// Choose a fresh current-case directory without moving other tests or global temporary storage.
/// Args: operation label. Returns the scheduled destination guard, or the existing home QA route when no external destination was selected.
pub(in crate::formats::raw) fn scratch(label: &str) -> Result<Scratch, String> {
    let remote = scheduled()?.is_some_and(|(plan, _)| plan.destination.is_some());
    let parent = if remote {
        PathBuf::from(MAC_ROOT)
    } else {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join(".qa-runs");
        fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
        parent
    };
    Scratch::create(&parent.join(format!(
        "{label}-{}-{}",
        std::process::id(),
        crate::document::next_id()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vanished_replaced_or_different_mounts_are_refused() {
        let expected = Destination {
            route_packet: PathBuf::new(),
            route_sha256: ROUTE_SHA256.into(),
            mount_id: "257".into(),
            device: "0:86".into(),
            root_device: "1".into(),
            root_inode: "2".into(),
        };
        let route = Route {
            mount: "/mnt/mac".into(),
            filesystem: "fuse.sshfs".into(),
            remote_source: "fixture:/volume".into(),
            project_root: MAC_ROOT.into(),
        };
        let line = "257 33 0:86 / /mnt/mac rw - fuse.sshfs fixture:/volume rw";
        assert!(match_mount(line, &expected, &route).is_ok());
        for changed in [
            String::new(),
            line.replacen("257 ", "258 ", 1),
            line.replace("0:86", "0:87"),
            line.replace("fuse.sshfs", "ext4"),
            line.replace("fixture:/volume", "other:/volume"),
        ] {
            assert!(match_mount(&changed, &expected, &route).is_err());
        }
        assert!(Scratch::create(Path::new("/mnt/mac/other/case")).is_err());
        assert!(Scratch::create(Path::new("/mnt/mac/omadesign-craft-gap-stack/../case")).is_err());
    }
    #[test]
    fn retained_directory_writes_never_fall_through_a_replaced_path() {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join(".qa-runs");
        fs::create_dir_all(&parent).unwrap();
        let scratch = super::super::super::create_scratch(&parent.join(format!(
            "raw-anchor-{}-{}",
            std::process::id(),
            crate::document::next_id()
        )))
        .unwrap();
        let root = scratch.join("root");
        fs::create_dir(&root).unwrap();
        let held = File::open(&root).unwrap();
        let renamed = scratch.join("held");
        fs::rename(&root, &renamed).unwrap();
        fs::create_dir(&root).unwrap();
        fs::write(fd_path(&held).join("owned"), b"kept on original handle").unwrap();
        assert_eq!(
            fs::read(renamed.join("owned")).unwrap(),
            b"kept on original handle"
        );
        assert!(!root.join("owned").exists());
        fs::remove_file(renamed.join("owned")).unwrap();
        fs::remove_dir(renamed).unwrap();
        fs::remove_dir(root).unwrap();
        fs::remove_dir(scratch).unwrap();
    }
}
