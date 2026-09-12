//! Opt-in cloud sync, comment pins, and showcase publish.

use crate::document::Document;
use crate::geom::Pt;
use crate::project::data_dir;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const IDENTITY_FILE: &str = "cloud-identity.json";
const LOCAL_STORE: &str = "cloud-store.json";

/// Clerk-or-local identity used by the desktop client.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Identity {
    pub email: String,
    pub name: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub cloud_url: String,
}

/// Link stored in the `.oma` document. Tokens stay in the identity file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CloudLink {
    pub project_id: String,
    pub document_id: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collaborators: Vec<String>,
    #[serde(default)]
    pub published: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gallery_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommentMsg {
    pub author: String,
    pub body: String,
    pub created: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommentPin {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_id: Option<u64>,
    pub pos: Pt,
    pub author: String,
    pub created: u64,
    #[serde(default)]
    pub resolved: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub thread: Vec<CommentMsg>,
}

impl CommentPin {
    pub fn new(pos: Pt, author: &str, body: &str, frame_id: Option<u64>) -> Self {
        let now = now_secs();
        Self {
            id: crate::document::next_id(),
            frame_id,
            pos,
            author: author.to_string(),
            created: now,
            resolved: false,
            thread: vec![CommentMsg {
                author: author.to_string(),
                body: body.to_string(),
                created: now,
            }],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GalleryItem {
    pub id: String,
    pub title: String,
    pub author: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub cover: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub published: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct LocalStore {
    #[serde(default)]
    gallery: Vec<GalleryItem>,
    #[serde(default)]
    waitlist: Vec<WaitlistEntry>,
    #[serde(default)]
    documents: Vec<SyncedDocument>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WaitlistEntry {
    pub email: String,
    pub name: String,
    pub created: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SyncedDocument {
    pub id: String,
    pub owner: String,
    pub snapshot: String,
    #[serde(default)]
    pub collaborators: Vec<String>,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn identity_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return PathBuf::from(format!("{xdg}/omadesign/{IDENTITY_FILE}"));
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(format!("{home}/.config/omadesign/{IDENTITY_FILE}"))
}

fn store_path() -> PathBuf {
    data_dir().join(LOCAL_STORE)
}

pub fn load_identity() -> Identity {
    let Ok(text) = std::fs::read_to_string(identity_path()) else {
        return Identity::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save_identity(identity: &Identity) -> Result<(), String> {
    if let Some(dir) = identity_path().parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(identity).map_err(|e| e.to_string())?;
    std::fs::write(identity_path(), text).map_err(|e| e.to_string())
}

fn load_store() -> LocalStore {
    let Ok(text) = std::fs::read_to_string(store_path()) else {
        return LocalStore::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn save_store(store: &LocalStore) -> Result<(), String> {
    std::fs::create_dir_all(data_dir()).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    std::fs::write(store_path(), text).map_err(|e| e.to_string())
}

pub fn signed_in(identity: &Identity) -> bool {
    !identity.email.trim().is_empty()
}

pub fn enable_sync(doc: &mut Document, identity: &Identity) -> Result<CloudLink, String> {
    if !signed_in(identity) {
        return Err("Sign in before enabling cloud sync".into());
    }
    let mut link = doc.cloud.clone().unwrap_or_default();
    if link.project_id.is_empty() {
        link.project_id = format!("proj-{}", crate::document::next_id());
    }
    if link.document_id.is_empty() {
        link.document_id = format!("doc-{}", crate::document::next_id());
    }
    link.enabled = true;
    if !link
        .collaborators
        .iter()
        .any(|e| e.eq_ignore_ascii_case(&identity.email))
    {
        link.collaborators.push(identity.email.clone());
    }
    push_snapshot(doc, identity, &link)?;
    Ok(link)
}

pub fn invite(link: &mut CloudLink, email: &str) -> Result<(), String> {
    let email = email.trim().to_string();
    if email.is_empty() || !email.contains('@') {
        return Err("Enter an email address".into());
    }
    if !link.enabled {
        return Err("Enable cloud sync first".into());
    }
    if !link
        .collaborators
        .iter()
        .any(|e| e.eq_ignore_ascii_case(&email))
    {
        link.collaborators.push(email);
    }
    Ok(())
}

fn push_snapshot(doc: &Document, identity: &Identity, link: &CloudLink) -> Result<(), String> {
    let snapshot = crate::project::encode(doc)?;
    if let Some(url) = http_base(identity) {
        let body = serde_json::json!({
            "projectId": link.project_id,
            "documentId": link.document_id,
            "owner": identity.email,
            "collaborators": link.collaborators,
            "snapshot": snapshot,
        });
        post_json(&format!("{url}/sync"), identity, &body)?;
        return Ok(());
    }
    let mut store = load_store();
    if let Some(existing) = store
        .documents
        .iter_mut()
        .find(|d| d.id == link.document_id)
    {
        existing.snapshot = snapshot;
        existing.collaborators = link.collaborators.clone();
    } else {
        store.documents.push(SyncedDocument {
            id: link.document_id.clone(),
            owner: identity.email.clone(),
            snapshot,
            collaborators: link.collaborators.clone(),
        });
    }
    save_store(&store)
}

pub fn publish(
    doc: &Document,
    identity: &Identity,
    title: &str,
    tags: &[String],
    summary: &str,
) -> Result<GalleryItem, String> {
    if !signed_in(identity) {
        return Err("Sign in before publishing".into());
    }
    let item = GalleryItem {
        id: format!("gal-{}", crate::document::next_id()),
        title: title.trim().to_string(),
        author: identity.name.clone(),
        tags: tags.to_vec(),
        cover: String::new(),
        summary: summary.trim().to_string(),
        published: true,
    };
    if item.title.is_empty() {
        return Err("Give the showcase item a title".into());
    }
    if let Some(url) = http_base(identity) {
        let body = serde_json::json!({
            "item": item,
            "documentId": doc.cloud.as_ref().map(|c| c.document_id.clone()),
        });
        post_json(&format!("{url}/publish"), identity, &body)?;
        return Ok(item);
    }
    let mut store = load_store();
    store.gallery.push(item.clone());
    save_store(&store)?;
    Ok(item)
}

pub fn join_waitlist(identity: &Identity) -> Result<(), String> {
    if !signed_in(identity) {
        return Err("Sign in with an email first".into());
    }
    if let Some(url) = http_base(identity) {
        let body = serde_json::json!({
            "email": identity.email,
            "name": identity.name,
        });
        post_json(&format!("{url}/waitlist"), identity, &body)?;
        return Ok(());
    }
    let mut store = load_store();
    if !store
        .waitlist
        .iter()
        .any(|e| e.email.eq_ignore_ascii_case(&identity.email))
    {
        store.waitlist.push(WaitlistEntry {
            email: identity.email.clone(),
            name: identity.name.clone(),
            created: now_secs(),
        });
    }
    save_store(&store)
}

pub fn local_gallery() -> Vec<GalleryItem> {
    load_store().gallery
}

fn http_base(identity: &Identity) -> Option<String> {
    let url = if !identity.cloud_url.trim().is_empty() {
        identity.cloud_url.trim().to_string()
    } else {
        std::env::var("OMADESIGN_CLOUD_URL")
            .ok()
            .unwrap_or_default()
    };
    if url.is_empty() {
        None
    } else {
        Some(url.trim_end_matches('/').to_string())
    }
}

fn post_json(url: &str, identity: &Identity, body: &serde_json::Value) -> Result<(), String> {
    let mut request = ureq::post(url).set("Content-Type", "application/json");
    if !identity.token.is_empty() {
        request = request.set("Authorization", &format!("Bearer {}", identity.token));
    }
    let response = request
        .send_string(&body.to_string())
        .map_err(|e| e.to_string())?;
    if response.status() >= 300 {
        return Err(format!("Cloud request failed ({})", response.status()));
    }
    Ok(())
}

pub fn unresolved_count(doc: &Document, frame_id: Option<u64>) -> usize {
    doc.comments
        .iter()
        .filter(|pin| !pin.resolved && (frame_id.is_none() || pin.frame_id == frame_id))
        .count()
}

pub fn seed_gallery() -> Vec<GalleryItem> {
    vec![
        GalleryItem {
            id: "seed-mobile".into(),
            title: "Night ferry boarding".into(),
            author: "omadesign".into(),
            tags: vec!["layout".into(), "mobile".into()],
            cover: String::new(),
            summary: "A nested mobile frame with an auto-stacked boarding pass.".into(),
            published: true,
        },
        GalleryItem {
            id: "seed-hero".into(),
            title: "Harbor landing".into(),
            author: "omadesign".into(),
            tags: vec!["layout".into(), "web".into()],
            cover: String::new(),
            summary: "A landing hero with constraints on the call-to-action row.".into(),
            published: true,
        },
        GalleryItem {
            id: "seed-dash".into(),
            title: "Studio dashboard".into(),
            author: "omadesign".into(),
            tags: vec!["layout".into(), "dashboard".into()],
            cover: String::new(),
            summary: "Sidebar plus a stretching content column.".into(),
            published: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;

    fn isolated_identity() -> Identity {
        let dir = std::env::temp_dir().join(format!("oma-cloud-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        unsafe {
            std::env::set_var("XDG_DATA_HOME", &dir);
            std::env::set_var("XDG_CONFIG_HOME", &dir);
        }
        Identity {
            email: "ada@example.com".into(),
            name: "Ada".into(),
            token: String::new(),
            cloud_url: String::new(),
        }
    }

    #[test]
    fn opt_in_sync_and_invite_round_trip() {
        let identity = isolated_identity();
        save_identity(&identity).unwrap();
        let mut doc = Document::new("Cloud", 400.0, 300.0, 72.0);
        let link = enable_sync(&mut doc, &identity).unwrap();
        assert!(link.enabled);
        assert!(link.collaborators.iter().any(|e| e == "ada@example.com"));
        let mut link = link;
        invite(&mut link, "guest@example.com").unwrap();
        assert_eq!(link.collaborators.len(), 2);
        let pin = CommentPin::new(Pt::new(12.0, 20.0), "Ada", "Tighten this stack", Some(1));
        assert!(!pin.resolved);
        assert_eq!(pin.thread.len(), 1);
        doc.comments.push(pin);
        assert_eq!(unresolved_count(&doc, Some(1)), 1);
        doc.comments[0].resolved = true;
        assert_eq!(unresolved_count(&doc, Some(1)), 0);
    }

    #[test]
    fn unpublished_items_stay_off_the_public_list() {
        let identity = isolated_identity();
        let mut doc = Document::new("Private", 100.0, 100.0, 72.0);
        let item = publish(&doc, &identity, "Harbor mark", &["logo".into()], "A mark").unwrap();
        assert!(item.published);
        let listed = local_gallery();
        assert!(listed.iter().any(|g| g.id == item.id && g.published));
        doc.cloud = Some(CloudLink {
            published: false,
            ..CloudLink::default()
        });
        assert!(!doc.cloud.as_ref().unwrap().published);
    }
}
