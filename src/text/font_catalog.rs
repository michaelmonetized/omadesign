//! Family and style choices share the renderer's installed and document font bytes.
use super::*;
use usvg::fontdb::{self, Database, Source};

#[derive(Clone)]
pub(crate) struct Catalog {
    pub db: Arc<Database>,
}
#[derive(Clone, Debug)]
pub(crate) struct Choice {
    pub id: fontdb::ID,
    pub family: String,
    pub style: String,
    pub weight: u16,
    pub slant: u8,
}
impl Catalog {
    /// Collect installed faces and actual document fonts into one read-only resolver.
    /// Args: authored font references. Returns a database with original family aliases intact.
    pub fn new(references: impl IntoIterator<Item = String>) -> Self {
        let mut db = svg_type::options().fontdb;
        let references: std::collections::BTreeSet<_> = references.into_iter().collect();
        for reference in references {
            let probe = TypeRun {
                font: reference,
                ..Default::default()
            };
            if let Some(bytes) = font_bytes_for(&probe) {
                Arc::make_mut(&mut db).load_font_source(Source::Binary(bytes));
            }
        }
        Self { db }
    }
    /// List canonical family names while retaining all aliases for resolution.
    /// Args: none. Returns unique names in case-insensitive display order.
    pub fn families(&self) -> Vec<String> {
        let mut families: Vec<_> = self
            .db
            .faces()
            .filter_map(|f| f.families.first().map(|f| f.0.clone()))
            .collect();
        families.sort_by_cached_key(|s| s.to_lowercase());
        families.dedup();
        families
    }
    /// List real OpenType styles belonging to a named family or localized alias.
    /// Args: family name. Returns available face choices, sorted by weight and style.
    pub fn styles(&self, family: &str) -> Vec<Choice> {
        let mut choices: Vec<_> = self
            .db
            .faces()
            .filter(|f| {
                f.families
                    .iter()
                    .any(|(n, _)| n.eq_ignore_ascii_case(family))
            })
            .filter_map(|f| {
                let style = self
                    .db
                    .with_face_data(f.id, |data, index| {
                        let face = rustybuzz::ttf_parser::Face::parse(data, index).ok()?;
                        font_inventory::name(&face, &[17, 2])
                    })
                    .flatten()?;
                Some(Choice {
                    id: f.id,
                    family: f.families.first()?.0.clone(),
                    style,
                    weight: f.weight.0,
                    slant: match f.style {
                        fontdb::Style::Italic => 1,
                        fontdb::Style::Oblique => 2,
                        _ => 0,
                    },
                })
            })
            .collect();
        choices.sort_by_key(|f| (f.weight, f.slant, f.style.clone()));
        choices.dedup_by(|a, b| a.style == b.style);
        choices
    }
    /// Resolve an explicit style or the closest source weight and slant in another family.
    /// Args: family, optional exact style and source request. Returns a real available face.
    pub fn choice(
        &self,
        family: &str,
        style: Option<&str>,
        from: &font_inventory::Request,
    ) -> Result<Choice, String> {
        let choices = self.styles(family);
        if let Some(style) = style {
            return choices
                .into_iter()
                .find(|f| f.style == style)
                .ok_or_else(|| {
                    "This font style is no longer available. Refresh the font list".into()
                });
        }
        let (weight, slant) = match from {
            font_inventory::Request::Named { weight, slant, .. } => (*weight, *slant),
            font_inventory::Request::Stored(reference) => font_bytes_for(&TypeRun {
                font: reference.clone(),
                ..Default::default()
            })
            .and_then(|b| {
                rustybuzz::ttf_parser::Face::parse(&b, 0)
                    .ok()
                    .map(|f| (f.weight().to_number(), u8::from(f.is_italic())))
            })
            .unwrap_or((400, 0)),
        };
        let id = self.db.query(&fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            weight: fontdb::Weight(weight),
            style: match slant {
                1 => fontdb::Style::Italic,
                2 => fontdb::Style::Oblique,
                _ => fontdb::Style::Normal,
            },
            ..Default::default()
        });
        choices
            .into_iter()
            .find(|f| {
                Some(f.id) == id
                    || self.db.face(id.unwrap_or(f.id)).is_some_and(|face| {
                        face.weight.0 == f.weight
                            && match face.style {
                                fontdb::Style::Italic => f.slant == 1,
                                fontdb::Style::Oblique => f.slant == 2,
                                _ => f.slant == 0,
                            }
                    })
            })
            .ok_or_else(|| "Choose an available font family".into())
    }
    /// Bind a selected face to the existing portable font loader.
    /// Args: actual catalog choice. Returns its content-addressed native reference after parser validation.
    pub fn reference(&self, choice: &Choice) -> Result<String, String> {
        let bytes = self
            .db
            .with_face_data(choice.id, face_bytes)
            .ok_or("The selected font is no longer available")??;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err("This face exceeds the portable project font limit of 16 MiB".into());
        }
        let id = format!("omatype:{:032x}", crate::typography::fingerprint(&bytes));
        register_memory_font(
            &id,
            &format!("{} {}", choice.family, choice.style),
            Arc::new(bytes),
        )?;
        Ok(id)
    }
}
fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, b| {
        let mut word = [0; 4];
        word[..b.len()].copy_from_slice(b);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}
/// Extract a collection face as a standalone SFNT without changing any outline or naming tables.
/// Args: shared font bytes and actual collection face index. Returns checked native font data.
fn face_bytes(data: &[u8], index: u32) -> Result<Vec<u8>, String> {
    let invalid = || "The selected font has an invalid table directory".to_owned();
    let read = |at: usize| {
        data.get(at..at.checked_add(4)?)
            .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
    };
    if data.get(..4) != Some(b"ttcf") {
        if index != 0 {
            return Err(invalid());
        }
        return Ok(data.to_vec());
    }
    let count = read(8).ok_or_else(invalid)?;
    if index >= count {
        return Err(invalid());
    }
    let offset = read(
        12usize
            .checked_add((index as usize).checked_mul(4).ok_or_else(invalid)?)
            .ok_or_else(invalid)?,
    )
    .ok_or_else(invalid)? as usize;
    let header = data
        .get(offset..offset.checked_add(12).ok_or_else(invalid)?)
        .ok_or_else(invalid)?;
    let tables = u16::from_be_bytes(header[4..6].try_into().unwrap()) as usize;
    let directory_end = offset.checked_add(12 + tables * 16).ok_or_else(invalid)?;
    let directory = data.get(offset + 12..directory_end).ok_or_else(invalid)?;
    let mut result = header.to_vec();
    result.extend_from_slice(directory);
    let mut head = None;
    for (i, table) in directory.chunks_exact(16).enumerate() {
        let start = u32::from_be_bytes(table[8..12].try_into().unwrap()) as usize;
        let len = u32::from_be_bytes(table[12..16].try_into().unwrap()) as usize;
        let bytes = data
            .get(start..start.checked_add(len).ok_or_else(invalid)?)
            .ok_or_else(invalid)?;
        if result
            .len()
            .checked_add(len + 3)
            .is_none_or(|n| n > 16 * 1024 * 1024)
        {
            return Err("This face exceeds the portable project font limit of 16 MiB".into());
        }
        let at = result.len();
        result.extend_from_slice(bytes);
        result.resize((result.len() + 3) & !3, 0);
        let row = 12 + i * 16;
        result[row + 8..row + 12].copy_from_slice(&(at as u32).to_be_bytes());
        if &table[..4] == b"head" {
            if len < 12 {
                return Err(invalid());
            }
            result[at + 8..at + 12].fill(0);
            head = Some(at);
            let sum = checksum(&result[at..at + len]);
            result[row + 4..row + 8].copy_from_slice(&sum.to_be_bytes());
        }
    }
    let head = head.ok_or_else(invalid)?;
    let adjustment = 0xb1b0afbau32.wrapping_sub(checksum(&result));
    result[head + 8..head + 12].copy_from_slice(&adjustment.to_be_bytes());
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    const FONT: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/assets/fonts/EBGaramond.ttf"
    );
    fn font() -> Vec<u8> {
        std::fs::read(FONT).unwrap()
    }
    #[test]
    fn craft_find_font_catalog_document_alias_closest_and_portable_face_share_real_bytes() {
        let bytes = font();
        let id = format!("omatype:{:032x}", crate::typography::fingerprint(&bytes));
        register_memory_font(&id, "Document face", Arc::new(bytes.clone())).unwrap();
        let catalog = Catalog::new([id]);
        let family = font_inventory::metadata(&bytes, 0, &Default::default())
            .unwrap()
            .0;
        assert!(catalog.families().contains(&family));
        let choices = catalog.styles(&family);
        assert!(!choices.is_empty());
        let choice = catalog
            .choice(&family, None, &font_inventory::Request::Stored(FONT.into()))
            .unwrap();
        let reference = catalog.reference(&choice).unwrap();
        assert_eq!(*project_font_bytes(&reference).unwrap(), bytes);
        let request = font_inventory::Request::Named {
            family,
            weight: 400,
            slant: 0,
        };
        let info =
            font_inventory::describe_with(&catalog, &request, FONT, &['A', '\n', ' '].into());
        assert_eq!(info.status, font_inventory::Status::Exact);
        assert_eq!(info.missing_glyphs, 0);
        assert!(
            catalog
                .choice("Not an installed family", None, &request)
                .is_err()
        );
        assert!(
            catalog
                .choice(&choice.family, Some("Unavailable style"), &request)
                .is_err()
        );
    }
    #[test]
    fn craft_find_font_catalog_collection_extract_keeps_outline_tables_and_checksum() {
        let bytes = font();
        let table_count = u16::from_be_bytes(bytes[4..6].try_into().unwrap()) as usize;
        let offset = 20usize;
        let mut collection = b"ttcf".to_vec();
        collection.extend_from_slice(&0x10000u32.to_be_bytes());
        collection.extend_from_slice(&2u32.to_be_bytes());
        collection.extend_from_slice(&(offset as u32).to_be_bytes());
        collection.extend_from_slice(&(offset as u32).to_be_bytes());
        collection.extend_from_slice(&bytes);
        for row in 0..table_count {
            let at = offset + 12 + row * 16 + 8;
            let original = u32::from_be_bytes(collection[at..at + 4].try_into().unwrap());
            collection[at..at + 4].copy_from_slice(&(original + offset as u32).to_be_bytes());
        }
        let extracted = face_bytes(&collection, 1).unwrap();
        assert_eq!(checksum(&extracted), 0xb1b0afba);
        let a = rustybuzz::ttf_parser::Face::parse(&bytes, 0).unwrap();
        let b = rustybuzz::ttf_parser::Face::parse(&extracted, 0).unwrap();
        for tag in [*b"glyf", *b"cmap", *b"hmtx", *b"name", *b"GSUB"] {
            let tag = rustybuzz::ttf_parser::Tag::from_bytes(&tag);
            assert_eq!(a.raw_face().table(tag), b.raw_face().table(tag));
        }
        assert!(ab_glyph::FontRef::try_from_slice(&extracted).is_ok());
        assert!(face_bytes(&collection, 2).is_err());
        assert!(face_bytes(&collection[..22], 0).is_err());
        let mut bad = collection;
        bad[offset + 20..offset + 24].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(face_bytes(&bad, 0).is_err());
    }
}

#[cfg(test)]
mod refresh_tests {
    use super::*;
    #[test]
    fn craft_find_font_refresh_reloads_replaced_native_file_and_preserves_portable_bytes() {
        let source = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/assets/fonts/EBGaramond.ttf"
        ))
        .unwrap();
        let alternate = PREFERRED
            .iter()
            .find_map(|p| std::fs::read(p).ok().filter(|b| b != &source))
            .expect("Installed native fallback font");
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".artifacts/font-recovery-tests")
            .join(format!("refresh-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("replace.ttf");
        std::fs::write(&path, &source).unwrap();
        let probe = TypeRun {
            font: path.to_string_lossy().into_owned(),
            ..Default::default()
        };
        assert_eq!(*font_bytes_for(&probe).unwrap(), source);
        let managed = format!("omatype:{:032x}", crate::typography::fingerprint(&source));
        register_memory_font(&managed, "Portable face", Arc::new(source.clone())).unwrap();
        std::fs::write(&path, &alternate).unwrap();
        assert_eq!(*font_bytes_for(&probe).unwrap(), source);
        refresh_fonts();
        assert_eq!(*font_bytes_for(&probe).unwrap(), alternate);
        assert_eq!(*project_font_bytes(&managed).unwrap(), source);
        std::fs::remove_file(path).unwrap();
    }
}
