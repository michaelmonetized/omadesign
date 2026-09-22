use super::*;
use semver::Version;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub tag: String,
}
enum ResultMessage {
    Checked(Option<Release>),
    Installed(PathBuf),
    Snapshot(PathBuf),
}
pub struct Updates {
    pub status: String,
    pub latest: Option<Release>,
    pub busy: bool,
    pub freezing: bool,
    checked: bool,
    job: Option<Receiver<Result<ResultMessage, String>>>,
    installed: Option<PathBuf>,
}
impl Default for Updates {
    fn default() -> Self {
        Self {
            status: "Check for a newer release.".into(),
            latest: None,
            busy: false,
            freezing: false,
            checked: false,
            job: None,
            installed: None,
        }
    }
}
impl Updates {
    fn start(
        &mut self,
        ctx: &egui::Context,
        work: impl FnOnce() -> Result<ResultMessage, String> + Send + 'static,
    ) {
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        self.busy = true;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(work());
            ctx.request_repaint();
        });
    }
    pub fn check(&mut self, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        self.checked = true;
        self.status = "Checking releases…".into();
        self.start(ctx, || check_release().map(ResultMessage::Checked));
    }
    pub fn install(&mut self, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        let Some(release) = self.latest.clone() else {
            return;
        };
        self.status = format!(
            "Installing {}… You can keep working while it downloads.",
            release.version
        );
        crate::telemetry::count("feature.update");
        self.start(ctx, move || {
            install_release(&release).map(ResultMessage::Installed)
        });
    }
}
fn compatible_release(value: &serde_json::Value, current: &Version) -> Option<(Version, Release)> {
    if value["draft"].as_bool().unwrap_or(true) {
        return None;
    }
    let tag = value["tag_name"].as_str()?;
    let version = Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()?;
    if version <= *current || (current.pre.is_empty() && !version.pre.is_empty()) {
        return None;
    }
    let triple = match std::env::consts::ARCH {
        "x86_64" => "x86_64-unknown-linux-gnu",
        "aarch64" => "aarch64-unknown-linux-gnu",
        _ => return None,
    };
    let archive = format!("omadesign-{version}-{triple}.tar.gz");
    let names = value["assets"].as_array()?;
    if !names.iter().any(|a| a["name"] == archive)
        || !names
            .iter()
            .any(|a| a["name"] == format!("{archive}.sha256"))
    {
        return None;
    }
    Some((
        version.clone(),
        Release {
            version: version.to_string(),
            tag: tag.into(),
        },
    ))
}
fn check_release() -> Result<Option<Release>, String> {
    let response =
        ureq::get("https://api.github.com/repos/michaelmonetized/omadesign/releases?per_page=100")
            .set("User-Agent", "Omadesign update check")
            .set("Accept", "application/vnd.github+json")
            .timeout(Duration::from_secs(15))
            .call()
            .map_err(|e| format!("Could not check releases: {e}"))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(4 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let releases: Vec<serde_json::Value> =
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let current = Version::parse(env!("CARGO_PKG_VERSION")).map_err(|e| e.to_string())?;
    Ok(releases
        .iter()
        .filter_map(|v| compatible_release(v, &current))
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, r)| r))
}
fn install_release(release: &Release) -> Result<PathBuf, String> {
    let version = Version::parse(&release.version).map_err(|e| e.to_string())?;
    if release.tag != format!("v{version}") && release.tag != version.to_string() {
        return Err("Invalid update version".into());
    }
    // Fetch with curl first so a failed/truncated download never becomes a
    // successful empty shell pipeline. Only the fixed HTTPS installer is run.
    let output = Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--max-time",
            "60",
            "--max-filesize",
            "262144",
            "https://omadesign.app/install",
        ])
        .output()
        .map_err(|e| format!("Could not run curl: {e}"))?;
    if !output.status.success() {
        return Err("Could not download the installer. Your workspace is still open.".into());
    }
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("Home directory unavailable")?);
    let current = std::env::current_exe().map_err(|e| e.to_string())?;
    let nightly_root = home.join(".local/share/omadesign-nightly/releases");
    let nightly = current.starts_with(&nightly_root);
    let prefix = nightly.then(|| nightly_root.join(&release.version));
    let executable = prefix.as_ref().map_or_else(
        || home.join(".local/bin/omadesign"),
        |p| p.join("bin/omadesign"),
    );
    let mut installer = Command::new("sh");
    installer
        .env("OMADESIGN_TAG", &release.tag)
        .env_remove("OMADESIGN_INSTALL_PREFIX");
    if let Some(prefix) = &prefix {
        installer.env("OMADESIGN_INSTALL_PREFIX", prefix);
    }
    let mut child = installer
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    child
        .stdin
        .take()
        .ok_or("Installer stdin unavailable")?
        .write_all(&output.stdout)
        .map_err(|e| e.to_string())?;
    let result = child.wait_with_output().map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err(format!(
            "Installation failed. Your workspace is still open. {}",
            String::from_utf8_lossy(&result.stderr)
                .chars()
                .rev()
                .take(500)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        ));
    }
    let installed = Command::new(&executable)
        .arg("--version")
        .output()
        .map_err(|e| e.to_string())?;
    if !installed.status.success()
        || String::from_utf8_lossy(&installed.stdout).trim() != format!("omadesign {version}")
    {
        return Err(
            "The installed version could not be verified. Your workspace is still open.".into(),
        );
    }
    if nightly {
        let launcher = home.join(".local/bin/omadesign-nightly");
        let old = std::fs::read_to_string(&launcher)
            .map_err(|e| format!("Could not update nightly launcher: {e}"))?;
        let old_target = format!("releases/{}/bin/omadesign", env!("CARGO_PKG_VERSION"));
        if !old.contains(&old_target) {
            return Err("The custom nightly launcher could not be updated automatically. Your workspace remains open.".into());
        }
        let updated = old.replace(
            &old_target,
            &format!("releases/{}/bin/omadesign", release.version),
        );
        crate::formats::write_atomic(&launcher, updated.as_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(launcher, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(executable)
}
impl Studio {
    pub fn poll_updates(&mut self, ctx: &egui::Context, background_busy: bool) {
        // Tests and scripted screenshots don't opt into disk config or networking.
        if self.startup_preferences_path.is_some()
            && self.startup_preferences.check_updates
            && !self.updates.checked
        {
            self.updates.check(ctx);
        }
        if let Some(receiver) = &self.updates.job {
            let result = match receiver.try_recv() {
                Ok(v) => Some(v),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(_) => Some(Err(
                    "Update worker stopped. Your workspace is still open.".into()
                )),
            };
            if let Some(result) = result {
                self.updates.job = None;
                self.updates.busy = false;
                match result {
                    Ok(ResultMessage::Checked(release)) => {
                        self.updates.status = release.as_ref().map_or_else(
                            || "No newer compatible release is available.".into(),
                            |r| format!("Version {} is available.", r.version),
                        );
                        self.updates.latest = release;
                    }
                    Ok(ResultMessage::Installed(executable)) => {
                        self.updates.installed = Some(executable);
                        self.updates.busy = true;
                        self.updates.status="Update installed. Waiting for pending work before saving the workspace…".into();
                    }
                    Ok(ResultMessage::Snapshot(manifest)) => {
                        let executable = self.updates.installed.take().unwrap();
                        crate::telemetry::flush(true);
                        #[cfg(unix)]
                        {
                            use std::os::unix::process::CommandExt;
                            let error = Command::new(executable)
                                .arg("--desktop-child")
                                .arg("--restore-session")
                                .arg(&manifest)
                                .exec();
                            self.updates.status = format!(
                                "Could not restart: {error}. Recovery is saved at {}.",
                                manifest.display()
                            );
                        }
                        self.updates.freezing = false;
                    }
                    Err(error) => {
                        crate::telemetry::count("error.update");
                        self.updates.status = error;
                        self.updates.freezing = false;
                        self.updates.installed = None;
                    }
                }
            }
        }
        if self.updates.installed.is_some() && self.updates.job.is_none() {
            if background_busy
                || self.file_dialog_pending()
                || !self.file_jobs.is_empty()
                || !self.pending_open_files.is_empty()
                || !self.clipboard_jobs.is_empty()
                || self.photo.is_loading()
                || self.photo.is_saving()
                || self.photo.is_batching()
                || self.photo.is_saving_presets()
                || self.photo.crop_drag.is_some()
                || self.deformation.is_some()
                || ctx.text_edit_focused()
                || ctx.input(|i| i.pointer.any_down())
                || self.op.is_some()
                || self.pending_place.is_some()
                || self.pending_nav.is_some()
                || self.libraries.close_requested
            {
                ctx.request_repaint_after(Duration::from_millis(100));
                return;
            }
            self.recovery_job = None;
            let snapshot = self.restart_snapshot();
            let root = crate::project::swap_dir();
            self.updates.freezing = true;
            self.show_preferences = true;
            self.settings_page = 1;
            self.updates.status =
                "Saving all open artwork, original photos and unsaved edits for restart…".into();
            self.updates.start(ctx, move || {
                snapshot.write(&root).map(ResultMessage::Snapshot)
            });
        }
        if self.updates.busy {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn release(tag: &str) -> serde_json::Value {
        serde_json::json!({"tag_name":tag,"draft":false,"assets":[{"name":format!("omadesign-{}-{}-unknown-linux-gnu.tar.gz",tag.trim_start_matches('v'),std::env::consts::ARCH)},{"name":format!("omadesign-{}-{}-unknown-linux-gnu.tar.gz.sha256",tag.trim_start_matches('v'),std::env::consts::ARCH)}]})
    }
    #[test]
    fn semantic_versions_respect_channels_and_complete_packages() {
        let nightly = Version::parse("0.5.5-nightly.1").unwrap();
        let stable = Version::parse("0.5.5").unwrap();
        assert!(compatible_release(&release("v0.5.5-nightly.10"), &nightly).is_some());
        assert!(compatible_release(&release("v0.5.6-nightly.1"), &stable).is_none());
        assert!(compatible_release(&release("v0.5.5"), &nightly).is_some());
        assert!(compatible_release(&release("v0.5.4"), &nightly).is_none());
        let mut missing = release("v0.6.0");
        missing["assets"] = serde_json::json!([]);
        assert!(compatible_release(&missing, &stable).is_none());
    }
}
