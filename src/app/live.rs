use super::*;
use crate::cloud::{client::Client, client::Event};
use crate::collaboration::{
    self, Replica,
    transport::{Presence, Reply},
};
use serde_json::json;
use sha2::{Digest, Sha256};

pub(super) struct LiveRuntime {
    job: Option<std::sync::mpsc::Receiver<Result<Reply, String>>>,
    lease: Option<std::fs::File>,
    next: Instant,
    failures: u32,
    joined: bool,
    generation: Option<u64>,
    pub status: String,
    pub presence: Vec<Presence>,
    conflicts: usize,
}
impl Default for LiveRuntime {
    fn default() -> Self {
        Self {
            job: None,
            lease: None,
            next: Instant::now(),
            failures: 0,
            joined: false,
            generation: None,
            status: "Connecting…".into(),
            presence: Vec::new(),
            conflicts: 0,
        }
    }
}

fn fingerprint(identity: &crate::cloud::Identity) -> String {
    format!("{:x}", Sha256::digest(identity.token.as_bytes()))
}

fn lease(actor: &str) -> Result<std::fs::File, String> {
    if actor.len() != 64 || !actor.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Invalid live actor ID".into());
    }
    let root = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .ok_or("No home directory for live client ownership")?
        .join("omadesign/live-clients");
    lease_at(&root, actor)
}

fn lease_at(root: &std::path::Path, actor: &str) -> Result<std::fs::File, String> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(root.join(format!("{actor}.lock")))
        .map_err(|e| e.to_string())?;
    #[cfg(target_os = "linux")]
    rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive)
        .map_err(|e| e.to_string())?;
    Ok(file)
}

impl Studio {
    /// Create an opt-in live session using the cloud project's editor permissions.
    /// Args: none. Returns immediately; the worker publishes its result to this tab.
    pub fn start_live(&mut self) {
        if self.doc.collaboration.is_some() {
            self.cloud_modal = CloudModal::Live;
            return;
        }
        let Some(link) = self.doc.cloud.as_ref().filter(|link| link.enabled) else {
            self.status = "Push this project to cloud before starting live edits".into();
            return;
        };
        let project = link.project_id.clone();
        self.commit_type_edit();
        self.end_pixel_stroke(true);
        self.end_deform(true);
        let base = match collaboration::snapshot(&self.doc) {
            Ok(base) => base,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        let tab = self.swap_id.clone();
        let device = fingerprint(&self.cloud_identity);
        self.cloud_task(move |client| {
            let seed = serde_json::to_string(&base).map_err(|e| e.to_string())?;
            let session = client.create_live(&project, seed)?;
            let actor = crate::cloud::client::random_token()?;
            let mut replica = client.join_live(&session, &actor)?;
            replica.device = device;
            Ok(Event::LiveStarted(replica, tab))
        });
    }

    /// Open a live session in its own editable document tab.
    /// Args: session ID from another editor. Returns immediately while authorization runs.
    pub fn join_live(&mut self) {
        let session = self.live_join_id.trim().to_owned();
        if session.is_empty() {
            self.status = "Enter a live session ID".into();
            return;
        }
        let device = fingerprint(&self.cloud_identity);
        self.cloud_task(move |client| {
            let actor = crate::cloud::client::random_token()?;
            let mut replica = client.join_live(&session, &actor)?;
            replica.device = device;
            let local = Document::new("Live", 1., 1., 72.);
            let mut doc = replica.document(&local)?;
            doc.cloud = Some(crate::cloud::CloudLink {
                project_id: replica.project.clone(),
                enabled: true,
                ..Default::default()
            });
            doc.collaboration = Some(replica);
            Ok(Event::LiveJoined(doc))
        });
    }

    pub fn pause_live(&mut self, paused: bool) {
        if let Some(replica) = &mut self.doc.collaboration {
            replica.paused = paused;
            self.dirty = true;
            self.live_runtime.presence.clear();
            self.live_runtime.status = if paused {
                "Offline · edits stay on this computer"
            } else {
                "Reconnecting…"
            }
            .into();
            self.live_runtime.next = Instant::now();
            if paused {
                let client = Client::new(self.cloud_identity.clone());
                let session = replica.session.clone();
                let actor = replica.actor.clone();
                std::thread::spawn(move || {
                    let _ = client.call("live:leave", json!({"session":session,"actor":actor}));
                });
            } else {
                self.live_runtime.joined = false;
            }
        }
    }

    fn record_live(&mut self) -> Result<(), String> {
        let Some(mut replica) = self.doc.collaboration.take() else {
            return Ok(());
        };
        let result = replica.record(&self.doc);
        if result.is_ok() {
            self.live_runtime.conflicts = replica.conflicts().len();
        }
        self.doc.collaboration = Some(replica);
        if result? {
            self.dirty = true;
        }
        self.live_runtime.generation = Some(self.canvas_gen);
        Ok(())
    }

    fn apply_live(&mut self, replica: Replica) -> Result<(), String> {
        let result = replica.document(&self.doc);
        self.live_runtime.conflicts = replica.conflicts().len();
        let Ok(mut doc) = result else {
            return result.map(|_| ());
        };
        let selection: Vec<_> = self
            .selection
            .iter()
            .filter_map(|&(layer, id)| {
                if id == RASTER_ID {
                    let layer_id = self.doc.layers.get(layer)?.id;
                    doc.layers
                        .iter()
                        .position(|layer| layer.id == layer_id)
                        .map(|layer| (layer, id))
                } else {
                    doc.layers
                        .iter()
                        .enumerate()
                        .find(|(_, layer)| {
                            layer
                                .kind
                                .shapes()
                                .is_some_and(|shapes| shapes.iter().any(|shape| shape.id == id))
                        })
                        .map(|(layer, _)| (layer, id))
                }
            })
            .collect();
        let active = self
            .active_layer
            .and_then(|i| self.doc.layers.get(i))
            .map(|layer| layer.id);
        doc.collaboration = Some(replica);
        self.doc = doc;
        self.selection = selection;
        self.active_layer =
            active.and_then(|id| self.doc.layers.iter().position(|layer| layer.id == id));
        self.sanitize();
        self.mark();
        self.dirty = true;
        self.live_runtime.generation = Some(self.canvas_gen);
        Ok(())
    }

    pub(super) fn live_undo(&mut self, redo: bool) -> bool {
        if self.doc.collaboration.is_none() {
            return false;
        }
        self.commit_type_edit();
        if let Err(error) = self.record_live() {
            self.status = error;
            return true;
        }
        let mut replica = self.doc.collaboration.as_ref().unwrap().clone();
        if if redo { replica.redo() } else { replica.undo() } {
            if let Err(error) = self.apply_live(replica) {
                self.status = error;
            } else {
                self.status = if redo {
                    "Redid your live edit"
                } else {
                    "Undid your live edit"
                }
                .into();
            }
        } else {
            self.doc.collaboration = Some(replica);
        }
        true
    }

    pub fn can_undo(&self) -> bool {
        self.doc.collaboration.as_ref().map_or_else(
            || self.history.can_undo(),
            |replica| replica.can_undo() || self.live_runtime.generation != Some(self.canvas_gen),
        )
    }
    pub fn can_redo(&self) -> bool {
        self.doc
            .collaboration
            .as_ref()
            .map_or_else(|| self.history.can_redo(), Replica::can_redo)
    }
    pub fn live_conflict_count(&self) -> usize {
        self.live_runtime.conflicts
    }

    pub fn live_status(&self) -> &str {
        &self.live_runtime.status
    }
    pub fn live_presence(&self) -> &[Presence] {
        &self.live_runtime.presence
    }

    fn prepare_live_ownership(
        &mut self,
        claim: impl Fn(&str) -> Result<std::fs::File, String>,
    ) -> Result<(), String> {
        if !crate::cloud::connected(&self.cloud_identity) {
            return Ok(());
        }
        let Some(state) = &self.doc.collaboration else {
            return Ok(());
        };
        let device = fingerprint(&self.cloud_identity);
        if state.device == device && self.live_runtime.lease.is_some() {
            return Ok(());
        }
        let mut state = self.doc.collaboration.take().unwrap();
        self.live_runtime = Default::default();
        let lock = if state.device == device {
            claim(&state.actor).ok()
        } else {
            None
        };
        let result = if let Some(lock) = lock {
            self.live_runtime.lease = Some(lock);
            Ok(())
        } else {
            crate::cloud::client::random_token()
                .and_then(|actor| state.fork(actor, device, &self.doc))
                .and_then(|_| claim(&state.actor))
                .map(|lock| self.live_runtime.lease = Some(lock))
        };
        self.doc.collaboration = Some(state);
        self.dirty = true;
        result
    }

    pub(super) fn poll_live(&mut self, ctx: &egui::Context) {
        let Some(_) = &self.doc.collaboration else {
            return;
        };
        ctx.request_repaint_after(Duration::from_millis(150));
        if let Err(error) = self.prepare_live_ownership(lease) {
            self.live_runtime.status = error;
            return;
        }
        if !crate::cloud::connected(&self.cloud_identity) {
            self.live_runtime.job = None;
            self.live_runtime.joined = false;
            self.live_runtime.presence.clear();
            self.live_runtime.status = "Offline · sign in to resume live edits".into();
        }
        if ctx.input(|input| input.pointer.any_down())
            || self.op.is_some()
            || self.type_edit.is_some()
        {
            return;
        }
        if self.live_runtime.generation != Some(self.canvas_gen)
            && let Err(error) = self.record_live()
        {
            self.live_runtime.status = error;
            return;
        }
        let result = self
            .live_runtime
            .job
            .as_ref()
            .and_then(|job| match job.try_recv() {
                Ok(result) => Some(result),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some(Err("Live worker stopped; retrying with saved edits".into()))
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
            });
        if let Some(result) = result {
            self.live_runtime.job = None;
            match result {
                Ok(reply) => {
                    let mut state = self.doc.collaboration.as_ref().unwrap().clone();
                    let before = state.changes.len();
                    let old_cursor = state.cursor;
                    let old_pending = state.pending.len();
                    let received = state.receive(reply.changes);
                    if received.is_ok() {
                        for id in reply.acknowledged {
                            state.pending.remove(&id);
                        }
                        state.cursor = reply.cursor;
                    }
                    let changed = state.changes.len() != before;
                    let checkpoint =
                        state.cursor != old_cursor || state.pending.len() != old_pending;
                    if let Err(error) = received {
                        self.doc.collaboration = Some(state);
                        self.live_runtime.status = error;
                        return;
                    }
                    let result = {
                        if changed {
                            self.apply_live(state)
                        } else {
                            self.doc.collaboration = Some(state);
                            Ok(())
                        }
                    };
                    if let Err(error) = result {
                        self.live_runtime.status = error;
                        return;
                    }
                    self.live_runtime.presence = reply.presence;
                    if self
                        .doc
                        .collaboration
                        .as_ref()
                        .is_some_and(|state| state.paused)
                    {
                        self.live_runtime.presence.clear();
                    }
                    self.live_runtime.joined = true;
                    self.live_runtime.failures = 0;
                    let state = self.doc.collaboration.as_ref().unwrap();
                    self.live_runtime.status = if state.paused {
                        "Offline · edits stay on this computer".into()
                    } else {
                        format!(
                            "Live · {} other editors · {} pending · {} conflicts",
                            self.live_runtime.presence.len(),
                            state.pending.len(),
                            self.live_runtime.conflicts
                        )
                    };
                    self.live_runtime.next = Instant::now()
                        + Duration::from_millis(if reply.more || !state.pending.is_empty() {
                            0
                        } else {
                            500
                        });
                    self.dirty |= changed || checkpoint;
                }
                Err(error) => {
                    self.live_runtime.joined = false;
                    self.live_runtime.failures = (self.live_runtime.failures + 1).min(6);
                    self.live_runtime.next =
                        Instant::now() + Duration::from_secs(1 << self.live_runtime.failures);
                    self.live_runtime.presence.clear();
                    self.live_runtime.status =
                        format!("Offline · {error} · local edits retained; retrying");
                }
            }
        }
        let Some(state) = &self.doc.collaboration else {
            return;
        };
        if state.paused
            || !crate::cloud::connected(&self.cloud_identity)
            || self.live_runtime.job.is_some()
            || Instant::now() < self.live_runtime.next
        {
            return;
        }
        let state = state.clone();
        let client = Client::new(self.cloud_identity.clone());
        let cursor = self.cursor;
        let selection = self
            .selection
            .iter()
            .take(100)
            .map(|&(layer, id)| {
                if id == RASTER_ID {
                    self.doc.layers[layer].id.to_string()
                } else {
                    id.to_string()
                }
            })
            .collect();
        let reconnect = !self.live_runtime.joined;
        let (tx, rx) = std::sync::mpsc::channel();
        self.live_runtime.job = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(client.exchange_live(&state, cursor, selection, reconnect));
            ctx.request_repaint();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Shape, Style};
    use crate::geom::{Geom, Pt};

    fn add(studio: &mut Studio, name: &str) {
        let mut shape = Shape::new(
            Geom::Rect {
                origin: Pt::new(0.1, 0.3),
                size: Pt::new(10.25, 10.),
                radius: 0.,
            },
            Style::default(),
        );
        shape.name = name.into();
        studio.commit(Cmd::AddShape { layer: 1, shape });
    }
    fn count(studio: &Studio) -> usize {
        studio
            .doc
            .layers
            .iter()
            .flat_map(|l| l.kind.shapes().unwrap_or_default())
            .count()
    }
    fn studio_pair() -> (Studio, Studio) {
        let mut a = Studio::new();
        a.doc = crate::collaboration::native_tests::legacy_fixture("Acceptance", 64., 64., 72.);
        let mut b = Studio::new();
        b.doc = a.doc.clone();
        let seed = collaboration::snapshot(&a.doc).unwrap();
        a.doc.collaboration = Some(Replica::new("test".into(), "a".repeat(64), seed.clone()));
        b.doc.collaboration = Some(Replica::new("test".into(), "b".repeat(64), seed));
        (a, b)
    }
    fn exchange(a: &mut Studio, b: &mut Studio) {
        a.record_live().unwrap();
        b.record_live().unwrap();
        let mut left = a.doc.collaboration.take().unwrap();
        let mut right = b.doc.collaboration.take().unwrap();
        left.receive(right.changes.clone()).unwrap();
        right.receive(left.changes.clone()).unwrap();
        a.apply_live(left).unwrap();
        b.apply_live(right).unwrap();
    }

    #[test]
    fn live_collaboration_commands_undo_only_local_changes_after_remote_merge() {
        let (mut a, mut b) = studio_pair();
        add(&mut a, "Alice");
        add(&mut b, "Bob");
        exchange(&mut a, &mut b);
        assert_eq!(count(&a), 2);
        assert_eq!(count(&b), 2);
        a.undo();
        exchange(&mut a, &mut b);
        assert_eq!(count(&b), 1);
        assert_eq!(b.doc.layers[1].kind.shapes().unwrap()[0].name, "Bob");
        a.redo();
        exchange(&mut a, &mut b);
        assert_eq!(count(&b), 2);
        b.undo();
        exchange(&mut a, &mut b);
        assert_eq!(count(&a), 1);
        assert_eq!(a.doc.layers[1].kind.shapes().unwrap()[0].name, "Alice");
        assert!(a.doc.collaboration.is_some());
    }

    #[test]
    fn live_collaboration_failed_native_materialization_preserves_artwork_and_history() {
        let mut studio = Studio::new();
        studio.doc = crate::collaboration::native_tests::fixture();
        let seed = collaboration::snapshot(&studio.doc).unwrap();
        studio.doc.collaboration = Some(Replica::new("native".into(), "a".repeat(64), seed));
        let before = serde_json::to_vec(&studio.doc).unwrap();
        let mut candidate = studio.doc.collaboration.as_ref().unwrap().clone();
        let path = format!("/@entities/{}/kind/Raster/pixels", studio.doc.layers[2].id);
        if let Some(collaboration::Cell::Pixels {
            native: Some(native),
            ..
        }) = candidate.base.get_mut(&path)
        {
            native["intent"] = json!(4);
        }
        assert!(studio.apply_live(candidate).unwrap_err().contains("intent"));
        assert_eq!(before, serde_json::to_vec(&studio.doc).unwrap());
    }

    #[test]
    fn live_collaboration_restores_selection_by_identity_when_remote_layers_move() {
        let (mut a, mut b) = studio_pair();
        add(&mut a, "Selected");
        exchange(&mut a, &mut b);
        let id = a.doc.layers[1].kind.shapes().unwrap()[0].id;
        a.selection = vec![(1, id)];
        b.doc.layers.swap(0, 1);
        b.mark();
        exchange(&mut a, &mut b);
        assert_eq!(a.selection, [(0, id)]);
    }

    #[test]
    fn live_collaboration_account_changes_release_the_actor_and_preserve_pending_artwork() {
        let (mut studio, _) = studio_pair();
        studio.cloud_identity.token = "a".repeat(64);
        let device = fingerprint(&studio.cloud_identity);
        studio.doc.collaboration.as_mut().unwrap().device = device;
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".artifacts/live-tests")
            .join(format!("live-lease-{}", crate::document::next_id()));
        let claim = |actor: &str| lease_at(&root, actor);
        studio.prepare_live_ownership(claim).unwrap();
        let actor = studio.doc.collaboration.as_ref().unwrap().actor.clone();
        assert!(claim(&actor).is_err());
        add(&mut studio, "Unsent artwork");
        studio.record_live().unwrap();
        studio.cloud_identity.token = "b".repeat(64);
        studio.prepare_live_ownership(claim).unwrap();
        let state = studio.doc.collaboration.as_ref().unwrap();
        assert_ne!(state.actor, actor);
        assert!(
            state
                .changes
                .iter()
                .filter(|c| state.pending.contains(&c.id))
                .all(|c| c.id.actor == state.actor)
        );
        assert_eq!(count(&studio), 1);
        assert!(claim(&actor).is_ok());
        drop(studio);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn barrier(root: &std::path::Path, role: &str, step: &str) {
        std::fs::write(root.join(format!("{role}-{step}")), b"ready").unwrap();
        let other = if role == "Alice" { "Bob" } else { "Alice" };
        let until = Instant::now() + Duration::from_secs(30);
        while !root.join(format!("{other}-{step}")).exists() {
            assert!(Instant::now() < until, "peer did not reach {step}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn round(studio: &mut Studio, client: &Client) -> bool {
        studio.record_live().unwrap();
        let mut replica = studio.doc.collaboration.take().unwrap();
        let reply = client
            .exchange_live(&replica, Some(Pt::new(10., 20.)), vec![], true)
            .unwrap();
        let more = reply.more;
        replica.receive(reply.changes).unwrap();
        for id in reply.acknowledged {
            replica.pending.remove(&id);
        }
        replica.cursor = reply.cursor;
        studio.apply_live(replica).unwrap();
        more
    }
    fn converge(studio: &mut Studio, client: &Client, wanted: usize) {
        let until = Instant::now() + Duration::from_secs(30);
        loop {
            let more = round(studio, client);
            if count(studio) == wanted
                && !more
                && studio
                    .doc
                    .collaboration
                    .as_ref()
                    .unwrap()
                    .pending
                    .is_empty()
            {
                break;
            }
            assert!(
                Instant::now() < until,
                "client failed to converge to {wanted} objects"
            );
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    #[test]
    #[ignore = "launched as two independent clients by live_collaboration_http_acceptance"]
    fn live_collaboration_client() {
        let role = std::env::var("OMADESIGN_LIVE_ROLE").unwrap();
        let scenario = PathBuf::from(std::env::var("OMADESIGN_LIVE_SCENARIO").unwrap());
        let root = scenario.parent().unwrap();
        let data: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&scenario).unwrap()).unwrap();
        let index = if role == "Alice" { 0 } else { 1 };
        let actor = if index == 0 { "a" } else { "b" }.repeat(64);
        let client = Client::new(crate::cloud::Identity {
            token: data["tokens"][index].as_str().unwrap().into(),
            cloud_url: data["url"].as_str().unwrap().into(),
            email: "acceptance@example.com".into(),
            name: role.clone(),
        });
        let replica = client
            .join_live(data["session"].as_str().unwrap(), &actor)
            .unwrap();
        let mut studio = Studio::new();
        studio.cloud_identity = client.identity.clone();
        studio.doc = replica
            .document(&Document::new("local", 1., 1., 72.))
            .unwrap();
        studio.doc.collaboration = Some(replica);
        add(&mut studio, &format!("{role} first"));
        round(&mut studio, &client);
        barrier(root, &role, "first");
        converge(&mut studio, &client, 2);
        let replica = studio.doc.collaboration.as_ref().unwrap();
        client
            .call(
                "live:leave",
                json!({"session":replica.session,"actor":replica.actor}),
            )
            .unwrap();
        crate::collaboration::native_tests::edit(&mut studio.doc, &role);
        studio.mark();
        add(&mut studio, &format!("{role} offline"));
        studio.record_live().unwrap();
        let path = root.join(format!("{role}-offline.oma"));
        let encoded = crate::project::encode(&studio.doc).unwrap();
        std::fs::write(&path, &encoded).unwrap();
        let independent: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert!(
            !independent["doc"]["collaboration"]["pending"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        studio.doc = crate::project::decode(&std::fs::read_to_string(path).unwrap()).unwrap();
        barrier(root, &role, "offline");
        converge(&mut studio, &client, 4);
        barrier(root, &role, "reconnected");
        studio.undo();
        round(&mut studio, &client);
        barrier(root, &role, "undo");
        converge(&mut studio, &client, 2);
        barrier(root, &role, "undone");
        studio.redo();
        round(&mut studio, &client);
        barrier(root, &role, "redo");
        converge(&mut studio, &client, 4);
        barrier(root, &role, "redone");
        studio.doc.name = role.clone();
        studio.mark();
        studio.record_live().unwrap();
        barrier(root, &role, "conflict");
        round(&mut studio, &client);
        barrier(root, &role, "conflict-sent");
        while round(&mut studio, &client) {}
        assert_eq!(studio.doc.name, "Bob");
        assert!(
            !studio
                .doc
                .collaboration
                .as_ref()
                .unwrap()
                .conflicts()
                .is_empty()
        );
        let final_doc = crate::project::encode(&studio.doc).unwrap();
        std::fs::write(root.join(format!("{role}-final.oma")), &final_doc).unwrap();
        let reopened = crate::project::decode(&final_doc).unwrap();
        crate::collaboration::native_tests::assert_native_equal(&studio.doc, &reopened);
        let mut expected = crate::collaboration::native_tests::fixture();
        crate::collaboration::native_tests::edit(&mut expected, "Alice");
        crate::collaboration::native_tests::edit(&mut expected, "Bob");
        crate::collaboration::native_tests::assert_native_equal(&expected, &reopened);
        assert_eq!(
            collaboration::snapshot(&reopened).unwrap(),
            collaboration::snapshot(&studio.doc).unwrap()
        );
        std::fs::write(
            root.join(format!("{role}-snapshot.json")),
            serde_json::to_string(&collaboration::snapshot(&reopened).unwrap()).unwrap(),
        )
        .unwrap();
        let state = studio.doc.collaboration.as_ref().unwrap();
        client
            .call(
                "live:leave",
                json!({"session":state.session,"actor":state.actor}),
            )
            .unwrap();
    }

    #[test]
    #[ignore = "requires an isolated local service fixture in OMADESIGN_LIVE_ACCEPTANCE"]
    fn live_collaboration_http_acceptance() {
        let fixture = PathBuf::from(std::env::var("OMADESIGN_LIVE_ACCEPTANCE").unwrap());
        let mut data: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&fixture).unwrap()).unwrap();
        let client = Client::new(crate::cloud::Identity {
            token: data["tokens"][0].as_str().unwrap().into(),
            cloud_url: data["url"].as_str().unwrap().into(),
            ..Default::default()
        });
        let mut doc = crate::collaboration::native_tests::fixture();
        doc.width = 1920.;
        doc.height = 1080.;
        let seed = serde_json::to_string(&collaboration::snapshot(&doc).unwrap()).unwrap();
        assert!(
            client
                .call(
                    "live:create",
                    json!({"projectId":data["project"],"seed":seed})
                )
                .unwrap_err()
                .contains("native-preservation")
        );
        data["session"] = client
            .call(
                "live:create",
                json!({"projectId":data["project"],"seed":seed,"protocol":collaboration::supported_protocol()}),
            )
            .unwrap();
        let legacy_actor = "c".repeat(64);
        assert!(
            client
                .call(
                    "live:join",
                    json!({"session":data["session"],"actor":legacy_actor})
                )
                .unwrap_err()
                .contains("native-preservation")
        );
        assert!(client.call("live:exchange",json!({"session":data["session"],"actor":legacy_actor,"changes":[],"cursor":null,"selection":[],"after":0})).unwrap_err().contains("native-preservation"));
        let root = fixture
            .parent()
            .unwrap()
            .join(format!("clients-{}", crate::document::next_id()));
        std::fs::create_dir_all(&root).unwrap();
        let scenario = root.join("scenario.json");
        std::fs::write(&scenario, serde_json::to_string(&data).unwrap()).unwrap();
        let mut children = Vec::new();
        for role in ["Alice", "Bob"] {
            children.push(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "app::live::tests::live_collaboration_client",
                        "--ignored",
                        "--nocapture",
                    ])
                    .env("OMADESIGN_LIVE_ROLE", role)
                    .env("OMADESIGN_LIVE_SCENARIO", &scenario)
                    .spawn()
                    .unwrap(),
            );
        }
        for child in &mut children {
            assert!(
                child.wait().unwrap().success(),
                "independent native client failed; receipts at {}",
                root.display()
            );
        }
        let left: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("Alice-snapshot.json")).unwrap(),
        )
        .unwrap();
        let right: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("Bob-snapshot.json")).unwrap())
                .unwrap();
        assert_eq!(left, right);
        std::fs::remove_file(scenario).unwrap();
        println!(
            "Two independent native processes converged through HTTP, offline save/reopen, reconnect, own undo/redo and conflicts; receipts: {}",
            root.display()
        );
    }
}
