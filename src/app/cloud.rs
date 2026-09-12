use super::*;
use crate::cloud::{self, CommentPin};

impl Studio {
    pub fn sign_in_cloud(&mut self, email: &str, name: &str) {
        let email = email.trim().to_string();
        let name = name.trim().to_string();
        if email.is_empty() || !email.contains('@') {
            self.status = "Enter a real email".into();
            return;
        }
        self.cloud_identity.email = email;
        self.cloud_identity.name = if name.is_empty() {
            self.cloud_identity.email.clone()
        } else {
            name
        };
        if let Err(error) = cloud::save_identity(&self.cloud_identity) {
            self.status = error;
            return;
        }
        self.cloud_modal = CloudModal::None;
        self.status = format!("Signed in as {}", self.cloud_identity.email);
    }

    pub fn enable_cloud_sync(&mut self) {
        match cloud::enable_sync(&mut self.doc, &self.cloud_identity) {
            Ok(link) => {
                let before = self.doc.cloud.clone();
                self.commit(Cmd::SetCloud {
                    before,
                    after: Some(link),
                });
                self.status = "Cloud sync is on for this document".into();
            }
            Err(error) => {
                self.status = error;
                self.cloud_modal = CloudModal::SignIn;
            }
        }
    }

    pub fn invite_collaborator(&mut self) {
        let mut link = self.doc.cloud.clone().unwrap_or_default();
        match cloud::invite(&mut link, &self.invite_email) {
            Ok(()) => {
                let before = self.doc.cloud.clone();
                self.commit(Cmd::SetCloud {
                    before,
                    after: Some(link),
                });
                self.invite_email.clear();
                self.cloud_modal = CloudModal::None;
                self.status = "Collaborator invited".into();
            }
            Err(error) => self.status = error,
        }
    }

    pub fn publish_showcase(&mut self) {
        let tags: Vec<String> = self
            .publish_tags
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        match cloud::publish(
            &self.doc,
            &self.cloud_identity,
            &self.publish_title,
            &tags,
            &self.publish_summary,
        ) {
            Ok(item) => {
                let mut link = self.doc.cloud.clone().unwrap_or_default();
                link.published = true;
                link.gallery_id = item.id;
                let before = self.doc.cloud.clone();
                self.commit(Cmd::SetCloud {
                    before,
                    after: Some(link),
                });
                self.cloud_modal = CloudModal::None;
                self.status = "Published to the showcase".into();
            }
            Err(error) => self.status = error,
        }
    }

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
