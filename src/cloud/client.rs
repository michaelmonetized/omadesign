//! Authenticated project transfer. Network and encoding work runs on workers.
use super::{CloudLink, Identity};
use crate::document::Document;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{io::Read, time::Duration};
#[derive(Clone)]
pub struct Client {
    pub identity: Identity,
    agent: ureq::Agent,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Project {
    #[serde(rename = "_id")]
    pub id: String,
    pub title: String,
    pub role: String,
}
#[derive(Clone, Debug, Deserialize)]
pub struct File {
    #[serde(rename = "_id")]
    pub id: String,
    pub name: String,
    pub kind: String,
    pub version: u64,
    #[serde(default)]
    pub width: Option<f32>,
    #[serde(default)]
    pub height: Option<f32>,
}
#[derive(Default)]
pub struct Panel {
    pub code: String,
    pub projects: Vec<Project>,
    pub files: Vec<File>,
    pub annotations: Vec<Value>,
    pub showcase: Vec<Value>,
    pub competitions: Vec<Value>,
    pub selected_snapshot: String,
    pub selected_work: String,
    pub reply: String,
    pub project_id: String,
    pub preview: Option<eframe::egui::TextureHandle>,
    pub preview_file: String,
}
pub enum Event {
    Disconnected,
    Preview(eframe::egui::ColorImage, String),
    DeviceCode(String, String),
    Connected(Identity),
    Projects(Vec<Project>),
    Uploaded(CloudLink, String),
    Pulled(Document),
    Refreshed {
        project_id: String,
        files: Vec<File>,
        annotations: Vec<Value>,
        showcase: Vec<Value>,
        competitions: Vec<Value>,
    },
    Notice(String),
}
impl Client {
    pub fn new(identity: Identity) -> Self {
        Self {
            identity,
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(60))
                .redirects(0)
                .build(),
        }
    }
    pub fn endpoint(&self) -> Result<String, String> {
        let base = if self.identity.cloud_url.is_empty() {
            std::env::var("OMADESIGN_CLOUD_URL").unwrap_or_else(|_| "https://omadesign.app".into())
        } else {
            self.identity.cloud_url.clone()
        };
        if !(base.starts_with("https://")
            || base.starts_with("http://localhost:")
            || base.starts_with("http://127.0.0.1:"))
        {
            return Err("Cloud requires HTTPS".into());
        }
        Ok(base
            .trim_end_matches('/')
            .trim_end_matches("/api/cloud")
            .to_string())
    }
    pub fn call(&self, operation: &str, mut args: Value) -> Result<Value, String> {
        if !self.identity.token.is_empty()
            && !matches!(operation, "devices:begin" | "showcase:competitions")
        {
            args["deviceToken"] = json!(self.identity.token);
        }
        let result = self
            .agent
            .post(&format!("{}/api/cloud", self.endpoint()?))
            .set("Content-Type", "application/json")
            .send_string(&json!({"operation":operation,"args":args}).to_string());
        let response = match result {
            Ok(r) => r,
            Err(ureq::Error::Status(_, r)) => r,
            Err(e) => return Err(format!("Cloud unavailable: {e}")),
        };
        let mut body = String::new();
        response
            .into_reader()
            .take(4 * 1024 * 1024)
            .read_to_string(&mut body)
            .map_err(|e| e.to_string())?;
        let data: Value = serde_json::from_str(&body).map_err(|_| "Invalid cloud response")?;
        if data["ok"] != true {
            return Err(data["error"]
                .as_str()
                .unwrap_or("Cloud request failed")
                .into());
        }
        Ok(data["value"].clone())
    }
    pub fn projects(&self) -> Result<Vec<Project>, String> {
        serde_json::from_value(self.call("projects:list", json!({}))?).map_err(|e| e.to_string())
    }
    pub fn upload(
        &self,
        project_id: &str,
        name: &str,
        kind: &str,
        content_type: &str,
        bytes: &[u8],
        dimensions: Option<(f32, f32)>,
    ) -> Result<String, String> {
        let max = if kind == "snapshot" { 20 } else { 100 } * 1024 * 1024;
        if bytes.len() > max {
            return Err(format!("{name} exceeds the cloud upload limit"));
        }
        let pending = self.call("files:begin", json!({"projectId":project_id,"kind":kind}))?;
        let url = pending["url"].as_str().ok_or("Missing upload URL")?;
        if !url.starts_with("https://") {
            return Err("Invalid upload URL".into());
        }
        let response = self
            .agent
            .post(url)
            .set("Content-Type", content_type)
            .send_bytes(bytes)
            .map_err(|e| format!("Upload failed: {e}"))?;
        let mut body = String::new();
        response
            .into_reader()
            .take(4096)
            .read_to_string(&mut body)
            .map_err(|e| e.to_string())?;
        let stored: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        let mut args =
            json!({"uploadId":pending["id"],"storageId":stored["storageId"],"name":name});
        if let Some((w, h)) = dimensions {
            args["width"] = json!(w);
            args["height"] = json!(h);
        }
        self.call("files:finish", args)?
            .as_str()
            .map(str::to_string)
            .ok_or("Missing file ID".into())
    }
    pub fn push(&self, doc: &Document) -> Result<CloudLink, String> {
        let mut link = doc.cloud.clone().unwrap_or_default();
        if !link.enabled || link.project_id.is_empty() {
            link.project_id = self
                .call("projects:create", json!({"title":doc.name}))?
                .as_str()
                .ok_or("Missing project ID")?
                .into();
        }
        link.enabled = true;
        let mut copy = doc.clone();
        copy.cloud = Some(link.clone());
        // Portable project fonts are explicit assets; raster pixels are embedded in .oma.
        let mut seen = std::collections::HashSet::new();
        for layer in &doc.layers {
            for shape in layer.kind.shapes().unwrap_or_default() {
                if let crate::geom::Geom::Text(run) = &shape.geom {
                    if run.font.starts_with("omatype:") && seen.insert(run.font.clone()) {
                        let bytes = crate::text::project_font_bytes(&run.font)
                            .ok_or("Project font is missing")?;
                        let extension = if bytes.starts_with(b"OTTO") {
                            "otf"
                        } else {
                            "ttf"
                        };
                        self.upload(
                            &link.project_id,
                            &format!(
                                "omatype-{}.{extension}",
                                run.font.trim_start_matches("omatype:")
                            ),
                            "asset",
                            "application/octet-stream",
                            &bytes,
                            None,
                        )?;
                    }
                }
            }
        }
        let encoded = crate::project::encode(&copy)?;
        self.upload(
            &link.project_id,
            &format!("{}.oma", doc.name),
            "source",
            "application/octet-stream",
            encoded.as_bytes(),
            None,
        )?;
        let png = crate::compositor::export_png(doc, 1)?;
        link.document_id = self.upload(
            &link.project_id,
            &format!("{}-review.png", doc.name),
            "snapshot",
            "image/png",
            &png,
            Some((doc.width, doc.height)),
        )?;
        Ok(link)
    }
    pub fn download(&self, id: &str) -> Result<Vec<u8>, String> {
        let response = self
            .agent
            .get(&format!("{}/api/cloud", self.endpoint()?))
            .call()
            .map_err(|e| e.to_string())?;
        let mut text = String::new();
        response
            .into_reader()
            .take(4096)
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        let config: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let base = config["convexSiteUrl"]
            .as_str()
            .ok_or("Cloud download service is not configured")?;
        if !base.starts_with("https://")
            || !base.ends_with(".convex.site")
            || !id.bytes().all(|c| c.is_ascii_alphanumeric())
        {
            return Err("Invalid download address".into());
        }
        let response = self
            .agent
            .get(&format!("{base}/file?id={id}"))
            .set("X-Omadesign-Device", &self.identity.token)
            .call()
            .map_err(|e| format!("Download failed: {e}"))?;
        let mut data = Vec::new();
        response
            .into_reader()
            .take(100 * 1024 * 1024 + 1)
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        if data.len() > 100 * 1024 * 1024 {
            return Err("File exceeds download limit".into());
        }
        Ok(data)
    }
    pub fn pull(&self, project_id: &str) -> Result<Document, String> {
        if project_id.is_empty() || !project_id.bytes().all(|c| c.is_ascii_alphanumeric()) {
            return Err("Invalid project id".into());
        }
        let p = self.call("projects:get", json!({"projectId":project_id}))?;
        let files: Vec<File> =
            serde_json::from_value(p["files"].clone()).map_err(|e| e.to_string())?;
        let source = files
            .iter()
            .find(|f| f.kind == "source")
            .ok_or("This project has no design file")?;
        let destination = crate::project::data_dir()
            .join("cloud-downloads")
            .join(project_id)
            .join(format!("pull-{}", crate::document::next_id()));
        std::fs::create_dir_all(destination.join(".omabrand/fonts")).map_err(|e| e.to_string())?;
        let mut total = 0usize;
        let mut downloaded = std::collections::HashSet::new();
        for asset in files.iter().filter(|f| f.kind == "asset") {
            // Only the newest version of each asset is copied, into a new private folder.
            if !downloaded.insert(asset.name.clone()) {
                continue;
            }
            let data = self.download(&asset.id)?;
            total += data.len();
            if total > 500 * 1024 * 1024 {
                return Err("Project assets exceed the 500 MB download limit".into());
            }
            let safe: String = asset
                .name
                .chars()
                .map(|c| {
                    if c == '/' || c == '\\' || c.is_control() {
                        '_'
                    } else {
                        c
                    }
                })
                .collect();
            if safe == "." || safe == ".." || safe.is_empty() {
                return Err("Invalid asset filename".into());
            }
            let path = if safe.starts_with("omatype-")
                && (safe.ends_with(".ttf") || safe.ends_with(".otf"))
            {
                destination.join(".omabrand/fonts").join(&safe)
            } else {
                destination.join(&safe)
            };
            std::fs::write(path, data).map_err(|e| e.to_string())?;
        }
        crate::typography::register_archive(&destination)?;
        let bytes = self.download(&source.id)?;
        let mut doc =
            crate::project::decode(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?)?;
        crate::typography::validate_document_fonts(&doc)?;
        doc.cloud = Some(CloudLink {
            project_id: project_id.into(),
            enabled: true,
            ..doc.cloud.unwrap_or_default()
        });
        Ok(doc)
    }
    pub fn refresh(&self, project_id: &str) -> Result<Event, String> {
        let p = self.call("projects:get", json!({"projectId":project_id}))?;
        Ok(Event::Refreshed {
            project_id: project_id.into(),
            files: serde_json::from_value(p["files"].clone()).map_err(|e| e.to_string())?,
            annotations: serde_json::from_value(
                self.call("review:list", json!({"projectId":project_id}))?,
            )
            .map_err(|e| e.to_string())?,
            showcase: serde_json::from_value(
                self.call("showcase:project", json!({"projectId":project_id}))?,
            )
            .map_err(|e| e.to_string())?,
            competitions: serde_json::from_value(self.call("showcase:competitions", json!({}))?)
                .map_err(|e| e.to_string())?,
        })
    }
}
pub fn random_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|e| e.to_string())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
