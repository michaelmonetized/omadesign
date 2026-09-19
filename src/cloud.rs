//! Opt-in cloud sync, comment pins, and showcase publish.

use crate::document::Document;
use crate::geom::Pt;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const IDENTITY_FILE: &str = "cloud-identity.json";
pub mod client;

/// Browser-authorized identity used by the desktop client.
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
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let path = identity_path();
        let temporary = path.with_extension(format!("{}.tmp", crate::document::next_id()));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        file.write_all(text.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        std::fs::rename(temporary, path).map_err(|e| e.to_string())
    }
}

pub fn signed_in(identity: &Identity) -> bool {
    !identity.email.trim().is_empty() && identity.token.len() == 64
}

pub fn unresolved_count(doc: &Document, frame_id: Option<u64>) -> usize {
    doc.comments
        .iter()
        .filter(|pin| !pin.resolved && (frame_id.is_none() || pin.frame_id == frame_id))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_email_is_not_authentication() {
        let identity = Identity {
            email: "person@example.com".into(),
            ..Identity::default()
        };
        assert!(!signed_in(&identity));
    }
    #[test]
    fn annotation_resolution_counts() {
        let mut doc = Document::new("Review", 100., 100., 72.);
        doc.comments.push(CommentPin::new(
            Pt::new(10., 20.),
            "Reviewer",
            "Align this",
            None,
        ));
        assert_eq!(unresolved_count(&doc, None), 1);
        doc.comments[0].resolved = true;
        assert_eq!(unresolved_count(&doc, None), 0);
    }
}
