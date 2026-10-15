//! Original format data retained once, independently of editable artwork.

use base64::Engine as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::{path::Path, sync::Arc};

const MAX_BYTES: usize = 512 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceAttachment {
    pub id: String,
    pub name: String,
    pub format: String,
    pub sha256: String,
    #[serde(
        serialize_with = "serialize_bytes",
        deserialize_with = "deserialize_bytes"
    )]
    data: Arc<Vec<u8>>,
}

impl SourceAttachment {
    /// Retain original bytes under a content-derived UUID.
    /// Args: original filename, format label, and original bytes. Returns a bounded attachment.
    pub fn new(name: &str, format: &str, bytes: Vec<u8>) -> Result<Self, String> {
        if bytes.len() > MAX_BYTES {
            return Err("Original source exceeds 512 MiB".into());
        }
        let sha256 = format!("{:x}", Sha256::digest(&bytes));
        let name = name
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("source")
            .to_string();
        Ok(Self {
            id: uuid(&sha256),
            name,
            format: format.to_string(),
            sha256,
            data: Arc::new(bytes),
        })
    }

    /// Read immutable original data.
    /// Args: none. Returns the exact bytes supplied at import.
    pub fn bytes(&self) -> &[u8] {
        &self.data
    }

    /// Check original data integrity.
    /// Args: none. Returns an error for damaged bytes, identifiers or oversized data.
    pub fn validate(&self) -> Result<(), String> {
        if self.data.len() > MAX_BYTES
            || self.sha256 != format!("{:x}", Sha256::digest(&*self.data))
            || self.id != uuid(&self.sha256)
        {
            return Err(format!(
                "Retained source {} failed its SHA-256 integrity check",
                self.name
            ));
        }
        Ok(())
    }

    /// Recover the original file without converting it.
    /// Args: destination path. Returns success only after integrity checking and atomic writing.
    pub fn write_original(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        crate::formats::write_atomic(path, self.bytes())
    }
}

fn uuid(hash: &str) -> String {
    let mut h = if hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        hash.as_bytes()[..32].to_vec()
    } else {
        b"00000000000000000000000000000000".to_vec()
    };
    h[12] = b'8';
    h[16] = b"89ab"[(h[16] as usize) & 3];
    let h = String::from_utf8_lossy(&h);
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}

fn serialize_bytes<S: Serializer>(bytes: &Arc<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(&**bytes))
}

fn deserialize_bytes<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Arc<Vec<u8>>, D::Error> {
    let encoded = String::deserialize(deserializer)?;
    if encoded.len() > MAX_BYTES.div_ceil(3) * 4 {
        return Err(serde::de::Error::custom("Original source exceeds 512 MiB"));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(serde::de::Error::custom)?;
    if bytes.len() > MAX_BYTES {
        return Err(serde::de::Error::custom("Original source exceeds 512 MiB"));
    }
    Ok(Arc::new(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_attachment_survives_save_reopen_without_duplicate_object_bytes() {
        let source =
            SourceAttachment::new("/private/folder/art.af", "affinity", vec![0, 255, 7, 0, 42])
                .unwrap();
        let mut doc = crate::document::Document::new("retained", 8., 8., 72.);
        let id = doc.retain_source(source.clone());
        assert_eq!(id, doc.retain_source(source.clone()));
        doc.layers[0].source_attachment = Some(id.clone());
        doc.layers[1].source_attachment = Some(id.clone());
        let saved = crate::project::encode(&doc).unwrap();
        let reopened = crate::project::decode(&saved).unwrap();
        assert_eq!(reopened.sources, vec![source]);
        assert_eq!(reopened.source(&id).unwrap().name, "art.af");
        assert_eq!(reopened.source(&id).unwrap().bytes(), &[0, 255, 7, 0, 42]);
        assert_eq!(reopened.layers[0].source_attachment, Some(id));
        let mut damaged: serde_json::Value = serde_json::from_str(&saved).unwrap();
        damaged["doc"]["sources"][0]["data"] = "AA==".into();
        assert!(
            crate::project::decode(&damaged.to_string())
                .unwrap_err()
                .contains("integrity")
        );
    }

    #[test]
    fn old_projects_keep_empty_source_defaults_on_roundtrip() {
        let doc = crate::document::Document::new("old", 4., 4., 72.);
        let saved = crate::project::encode(&doc).unwrap();
        assert!(!saved.contains("source_attachment"));
        assert!(!saved.contains("\"sources\""));
        let reopened = crate::project::decode(&saved).unwrap();
        assert!(reopened.sources.is_empty());
        assert!(
            reopened
                .layers
                .iter()
                .all(|l| l.source_attachment.is_none())
        );
        assert_eq!(saved, crate::project::encode(&reopened).unwrap());
    }

    #[test]
    fn source_attachment_placement_keeps_references_through_undo_redo_and_reopen() {
        use crate::{
            document::{Document, Layer, Shape, Style},
            geom::Pt,
        };
        let mut source = Document::new("placed", 12., 12., 72.);
        source.layers = vec![Layer::vector("art")];
        source.layers[0].kind.shapes_mut().unwrap().push(Shape::new(
            crate::geom::Geom::Rect {
                origin: Pt::ZERO,
                size: Pt::splat(12.),
                radius: 0.,
            },
            Style::default(),
        ));
        let attachment =
            SourceAttachment::new("original.ai", "illustrator", vec![42, 0, 255]).unwrap();
        let id = source.retain_source(attachment.clone());
        source.layers[0].source_attachment = Some(id.clone());
        let mut studio = crate::app::Studio::default();
        studio
            .place_brand_imported(crate::import::Imported::Document(source), Pt::splat(100.))
            .unwrap();
        assert_eq!(studio.doc.source(&id).unwrap(), &attachment);
        assert!(
            studio
                .doc
                .layers
                .iter()
                .any(|l| l.source_attachment.as_deref() == Some(&id))
        );
        studio.undo();
        assert!(studio.doc.sources.is_empty());
        studio.redo();
        let reopened =
            crate::project::decode(&crate::project::encode(&studio.doc).unwrap()).unwrap();
        assert_eq!(reopened.source(&id).unwrap().bytes(), &[42, 0, 255]);
    }
}
