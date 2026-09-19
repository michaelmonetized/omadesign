use super::*;
use crate::cloud::client::{Client, Event};
use crate::cloud::{self, CommentPin};
use serde_json::json;

impl Studio {
    pub fn drop_comment_pin(&mut self, pos: crate::geom::Pt) {
        let author = if self.cloud_identity.name.is_empty() {
            "You"
        } else {
            self.cloud_identity.name.as_str()
        };
        let body = self.comment_draft.trim();
        if body.is_empty() {
            self.status = "Type a comment first".into();
            return;
        }
        let frame_id = self.selected_frame().map(|(_, id)| id).or_else(|| {
            crate::layout::containing_frame(&self.doc, self.active_layer.unwrap_or(0), pos)
        });
        let pin = CommentPin::new(pos, author, body, frame_id);
        let mut after = self.doc.comments.clone();
        after.push(pin);
        self.commit(Cmd::SetComments {
            before: self.doc.comments.clone(),
            after,
        });
        self.comment_draft.clear();
        self.pinning_comment = false;
        self.status = "Comment pinned".into();
    }

    pub fn resolve_comment(&mut self, id: u64, resolved: bool) {
        let mut after = self.doc.comments.clone();
        if let Some(pin) = after.iter_mut().find(|p| p.id == id) {
            pin.resolved = resolved;
        }
        self.commit(Cmd::SetComments {
            before: self.doc.comments.clone(),
            after,
        });
    }

    pub fn reply_comment(&mut self, id: u64, body: &str) {
        let body = body.trim();
        if body.is_empty() {
            return;
        }
        let author = if self.cloud_identity.name.is_empty() {
            "You".into()
        } else {
            self.cloud_identity.name.clone()
        };
        let mut after = self.doc.comments.clone();
        if let Some(pin) = after.iter_mut().find(|p| p.id == id) {
            pin.thread.push(crate::cloud::CommentMsg {
                author,
                body: body.into(),
                created: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            });
            pin.resolved = false;
        }
        self.commit(Cmd::SetComments {
            before: self.doc.comments.clone(),
            after,
        });
    }
}

impl Studio {
    pub fn cloud_busy(&self) -> bool {
        self.cloud_job.is_some()
    }
    pub fn cloud_task(
        &mut self,
        work: impl FnOnce(Client) -> Result<Event, String> + Send + 'static,
    ) {
        if self.cloud_job.is_some() {
            return;
        }
        let client = Client::new(self.cloud_identity.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        self.cloud_job = Some(rx);
        self.status = "Connecting to cloud…".into();
        std::thread::spawn(move || {
            let result = work(client);
            let _ = tx.send(result);
        });
    }
    pub fn connect_cloud(&mut self) {
        if self.cloud_job.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        self.cloud_job = Some(rx);
        let endpoint =
            std::env::var("OMADESIGN_CLOUD_URL").unwrap_or_else(|_| "https://omadesign.app".into());
        std::thread::spawn(move || {
            let run = || -> Result<Event, String> {
                let token = cloud::client::random_token()?;
                let code = token[..12].to_uppercase();
                let mut identity = cloud::Identity {
                    token: token.clone(),
                    cloud_url: endpoint,
                    ..Default::default()
                };
                let client = Client::new(identity.clone());
                client.call(
                    "devices:begin",
                    json!({"token":token,"code":code,"label":"Omadesign on Linux"}),
                )?;
                let url = format!("{}/cloud?device={code}", client.endpoint()?);
                tx.send(Ok(Event::DeviceCode(code, url.clone())))
                    .map_err(|_| "Sign-in cancelled")?;
                std::process::Command::new("xdg-open")
                    .arg(url)
                    .spawn()
                    .map_err(|e| e.to_string())?;
                for _ in 0..300 {
                    if let Ok(user) = client.call("devices:me", json!({})) {
                        identity.email = user["email"].as_str().unwrap_or_default().into();
                        identity.name = user["name"].as_str().unwrap_or_default().into();
                        cloud::save_identity(&identity)?;
                        return Ok(Event::Connected(identity));
                    }
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }
                Err("Sign-in timed out. Start again to get a new device code.".into())
            };
            let _ = tx.send(run());
        });
    }
    pub fn enable_cloud_sync(&mut self) {
        if !cloud::signed_in(&self.cloud_identity) {
            self.cloud_modal = CloudModal::SignIn;
            return;
        }
        let doc = self.doc.clone();
        let tab = self.swap_id.clone();
        self.cloud_task(move |client| client.push(&doc).map(|link| Event::Uploaded(link, tab)));
    }
    pub fn show_cloud_projects(&mut self) {
        self.cloud_modal = CloudModal::Projects;
        self.cloud_task(|client| client.projects().map(Event::Projects));
    }
    pub fn refresh_cloud_review(&mut self) {
        let Some(link) = self.doc.cloud.as_ref().filter(|l| l.enabled) else {
            self.status = "Push this project to cloud first".into();
            return;
        };
        let id = link.project_id.clone();
        self.cloud_modal = CloudModal::Review;
        self.cloud_task(move |client| client.refresh(&id));
    }
    pub fn pull_cloud_project(&mut self, id: String) {
        self.cloud_task(move |client| client.pull(&id).map(Event::Pulled));
    }
    pub fn upload_cloud_asset(&mut self) {
        let Some(link) = self.doc.cloud.as_ref().filter(|l| l.enabled) else {
            self.status = "Push this project to cloud first".into();
            return;
        };
        let id = link.project_id.clone();
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Share project asset")
            .pick_file()
        {
            self.cloud_task(move |client| {
                let name = path
                    .file_name()
                    .ok_or("Invalid asset path")?
                    .to_string_lossy()
                    .into_owned();
                let size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
                if size > 100 * 1024 * 1024 {
                    return Err("Asset exceeds 100 MB".into());
                }
                let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
                client.upload(
                    &id,
                    &name,
                    "asset",
                    "application/octet-stream",
                    &bytes,
                    None,
                )?;
                Ok(Event::Notice("Asset saved to the project".into()))
            });
        }
    }
    pub fn invite_collaborator(&mut self) {
        let Some(link) = self.doc.cloud.as_ref().filter(|l| l.enabled) else {
            self.status = "Push this project to cloud first".into();
            return;
        };
        let id = link.project_id.clone();
        let email = self.invite_email.clone();
        self.cloud_task(move |client| {
            client.call(
                "projects:invite",
                json!({"projectId":id,"email":email,"role":"reviewer"}),
            )?;
            Ok(Event::Notice("Review invitation sent".into()))
        });
    }
    pub fn publish_showcase(&mut self) {
        let snapshot = self.cloud_panel.selected_snapshot.clone();
        if snapshot.is_empty() {
            self.status = "Select a flat review export first".into();
            return;
        }
        let title = self.publish_title.clone();
        let description = self.publish_summary.clone();
        self.cloud_task(move |client| {
            client.call(
                "showcase:publish",
                json!({"snapshotId":snapshot,"title":title,"description":description}),
            )?;
            Ok(Event::Notice(
                "Selected export published to showcase".into(),
            ))
        });
    }
    pub fn load_cloud_preview(&mut self) {
        let id = self.cloud_panel.selected_snapshot.clone();
        self.cloud_task(move |client| Ok(Event::Preview(client.download(&id)?, id)));
    }
    pub fn poll_cloud(&mut self, ctx: &egui::Context) {
        let result = self.cloud_job.as_ref().map(|rx| rx.try_recv());
        match result {
            Some(Ok(Ok(Event::DeviceCode(code, url)))) => {
                self.cloud_panel.code = code;
                self.status = format!("Approve this device in your browser: {url}");
            }
            Some(Ok(result)) => {
                self.cloud_job = None;
                match result {
                    Err(error) => self.status = error,
                    Ok(Event::Preview(bytes, id)) => match image::load_from_memory(&bytes) {
                        Ok(image) => {
                            let pixels = image.to_rgba8();
                            let color = egui::ColorImage::from_rgba_unmultiplied(
                                [pixels.width() as usize, pixels.height() as usize],
                                pixels.as_raw(),
                            );
                            self.cloud_panel.preview = Some(ctx.load_texture(
                                "cloud-review",
                                color,
                                egui::TextureOptions::LINEAR,
                            ));
                            self.cloud_panel.preview_file = id;
                            self.status = "Flat export loaded".into();
                        }
                        Err(error) => self.status = error.to_string(),
                    },
                    Ok(Event::Connected(identity)) => {
                        self.cloud_identity = identity;
                        self.cloud_panel.code.clear();
                        self.cloud_modal = CloudModal::None;
                        self.status = "Connected to Omadesign cloud".into();
                    }
                    Ok(Event::Projects(projects)) => {
                        self.cloud_panel.projects = projects;
                        self.status = "Cloud projects loaded".into();
                    }
                    Ok(Event::Uploaded(link, tab)) => {
                        self.link_cloud_tab(&tab, link);
                        self.status = "Project and flat review export saved to cloud".into();
                    }
                    Ok(Event::Pulled(doc)) => {
                        self.open_document(doc, None);
                        self.cloud_modal = CloudModal::None;
                        self.status = "Cloud project opened in a new document".into();
                    }
                    Ok(Event::Refreshed {
                        project_id,
                        files,
                        annotations,
                        showcase,
                        competitions,
                    }) => {
                        self.cloud_panel.project_id = project_id;
                        self.cloud_panel.files = files;
                        self.cloud_panel.annotations = annotations;
                        self.cloud_panel.showcase = showcase;
                        self.cloud_panel.competitions = competitions;
                        if !self
                            .cloud_panel
                            .files
                            .iter()
                            .any(|f| f.id == self.cloud_panel.selected_snapshot)
                        {
                            self.cloud_panel.selected_snapshot = self
                                .cloud_panel
                                .files
                                .iter()
                                .find(|f| f.kind == "snapshot")
                                .map(|f| f.id.clone())
                                .unwrap_or_default();
                        }
                        self.status = "Cloud review updated".into();
                    }
                    Ok(Event::Notice(note)) => self.status = note,
                    Ok(Event::DeviceCode(..)) => {}
                }
            }
            Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                self.cloud_job = None;
                self.status = "Cloud operation stopped before completing".into();
            }
            _ => {}
        }
        if self.cloud_job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
    }
}
