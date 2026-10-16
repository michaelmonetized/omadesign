//! `.oma` project files: JSON with PNG-packed rasters.

use crate::document::{Document, Pixels};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

mod features;

pub(crate) const VERSION: u32 = features::EXPORT_RESOLUTION;

/// Check whether this build implements a project version.
/// Args: envelope version. Returns false for unimplemented reserved formats.
pub(crate) fn supports_version(version: u32) -> bool {
    features::supports_version(version)
}

#[derive(Serialize, Deserialize)]
pub(crate) struct File {
    version: u32,
    doc: Document,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    svg_sources: std::collections::BTreeMap<String, std::sync::Arc<str>>,
}

pub fn encode(doc: &Document) -> Result<String, String> {
    let output = encode_admitted(doc)?;
    let allocation =
        std::sync::Arc::try_unwrap(output).map_err(|_| "Shared project encoded storage")?;
    let (bytes, _memory) = allocation.into_parts();
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

/// Encode a native project while retaining its admitted output storage.
/// Args: editable document. Returns the encoded allocation and its live resource owner.
pub fn encode_admitted(
    doc: &Document,
) -> Result<std::sync::Arc<crate::resources::Allocation<u8>>, String> {
    encode_envelope_admitted(doc, |project| project)
}

/// Encode one owned project inside a typed portable envelope.
/// Args: artwork and envelope constructor. Returns one admitted serialized allocation.
pub(crate) fn encode_envelope_admitted<T: Serialize>(
    doc: &Document,
    envelope: impl FnOnce(File) -> T,
) -> Result<std::sync::Arc<crate::resources::Allocation<u8>>, String> {
    crate::motion::compositions::validate(doc)?;
    for document in crate::motion::compositions::documents(doc)? {
        document.screen_export.validate()?;
        document.validate_sources()?;
        if let Some(replica) = &document.collaboration {
            crate::collaboration::resources::validate(replica)?;
        }
    }
    let version = features::required_version(doc);
    if version > VERSION {
        return Err("Unsupported omadesign project version".into());
    }
    let mut packed = doc.clone();
    pack_rasters(&mut packed)?;
    let svg_sources = crate::svg_source::pack_sources(&mut packed)?;
    let prepared = envelope(File {
        version,
        doc: packed,
        svg_sources,
    });
    encode_prepared(&prepared)
}

/// Admit a prepared portable value without retaining old and replacement output buffers.
/// Args: immutable envelope. Returns its one exact-sized encoded owner or a serialization error.
fn encode_prepared<T: Serialize>(
    value: &T,
) -> Result<std::sync::Arc<crate::resources::Allocation<u8>>, String> {
    use std::io::{self, Write};
    struct Size(usize);
    impl Write for Size {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .ok_or_else(|| io::Error::other("Project size overflows"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    struct Output<'a> {
        writer: &'a mut crate::deep_color::storage::Builder<u8>,
        remaining: usize,
    }
    impl Write for Output<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > self.remaining {
                return Err(io::Error::other("Project changed during encoding"));
            }
            self.writer.write_all(bytes)?;
            self.remaining -= bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut size = Size(0);
    serde_json::to_writer(&mut size, value).map_err(|e| e.to_string())?;
    let mut writer = crate::deep_color::storage::Builder::with_capacity(size.0)?;
    let mut output = Output {
        writer: &mut writer,
        remaining: size.0,
    };
    serde_json::to_writer(&mut output, value).map_err(|e| e.to_string())?;
    if output.remaining != 0 {
        return Err("Project changed during encoding".into());
    }
    writer.finish()

}

pub fn decode(s: &str) -> Result<Document, String> {
    let _input_memory = crate::resources::global()
        .reserve_now(s.len() as u64, &crate::deep_color::storage::check)?;
    decode_admitted(s)
}

fn decode_admitted(s: &str) -> Result<Document, String> {
    let _parser = admit_parser(s)?;
    let file: File = serde_json::from_str(s).map_err(|e| e.to_string())?;
    let mut doc = file.doc;
    restore_sources(&mut doc, file.version, &file.svg_sources)?;
    unpack_rasters(&mut doc)?;
    normalize_decoded_document(doc, file.version)
}

/// Keep parser scratch charged while excluding only direct borrowed native sample text.
/// Args: complete original file. Returns pending parser admission after the unchanged retained preflight.
fn admit_parser(text: &str) -> Result<std::sync::Arc<crate::resources::Lease>, String> {
    let parser = crate::resources::global()
        .reserve_now(text.len() as u64, &crate::deep_color::storage::check)?;
    validate_retained_json(text)?;
    parser.resize_now(
        text.len()
            .saturating_sub(borrowed_native_sample_bytes(text)) as u64,
        &crate::deep_color::storage::check,
    )?;
    Ok(parser)
}

/// Measure borrowed sample strings only at the native raster fields of the root document.
/// Args: preflighted file JSON. Returns zero on ambiguous structure; other retained data remains fully charged.
fn borrowed_native_sample_bytes(text: &str) -> usize {
    use serde::de::{DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};
    use std::{borrow::Cow, cell::Cell, fmt};

    struct Text<'a>(Cow<'a, str>);
    impl<'de> Deserialize<'de> for Text<'de> {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct Borrow;
            impl<'de> Visitor<'de> for Borrow {
                type Value = Text<'de>;
                fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                    f.write_str("sample text")
                }
                fn visit_borrowed_str<E: serde::de::Error>(
                    self,
                    value: &'de str,
                ) -> Result<Self::Value, E> {
                    Ok(Text(Cow::Borrowed(value)))
                }
                fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<Self::Value, E> {
                    Ok(Text(Cow::Owned(String::new())))
                }
            }
            deserializer.deserialize_str(Borrow)
        }
    }
    #[derive(Clone, Copy, PartialEq)]
    enum At {
        File,
        Doc,
        Layers,
        Layer,
        Kind,
        Raster,
        Pixels,
        Native,
    }
    struct Scan<'a> {
        at: At,
        bytes: &'a Cell<usize>,
        valid: &'a Cell<bool>,
    }
    impl<'de> DeserializeSeed<'de> for Scan<'_> {
        type Value = ();
        fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
            deserializer.deserialize_any(self)
        }
    }
    impl<'de> Visitor<'de> for Scan<'_> {
        type Value = ();
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("root raster structure")
        }
        fn visit_unit<E: serde::de::Error>(self) -> Result<(), E> {
            if self.at == At::Native {
                Ok(())
            } else {
                Err(E::custom("Unexpected root structure"))
            }
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<(), A::Error> {
            if self.at != At::Layers {
                return Err(serde::de::Error::custom("Unexpected root sequence"));
            }
            while sequence
                .next_element_seed(Scan {
                    at: At::Layer,
                    bytes: self.bytes,
                    valid: self.valid,
                })?
                .is_some()
            {}
            Ok(())
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
            if self.at == At::Layers {
                return Err(serde::de::Error::custom("Unexpected layer map"));
            }
            let mut seen = false;
            let mut fields = 0usize;
            while let Some(Text(key)) = map.next_key::<Text<'de>>()? {
                fields += 1;
                let key = match key {
                    Cow::Borrowed(key) => key,
                    Cow::Owned(_) => {
                        self.valid.set(false);
                        map.next_value::<IgnoredAny>()?;
                        continue;
                    }
                };
                let next = match (self.at, key) {
                    (At::File, "doc") => Some(At::Doc),
                    (At::Doc, "layers") => Some(At::Layers),
                    (At::Layer, "kind") => Some(At::Kind),
                    (At::Kind, "Raster") => Some(At::Raster),
                    (At::Raster, "pixels") => Some(At::Pixels),
                    (At::Pixels, "native") => Some(At::Native),
                    _ => None,
                };
                if let Some(at) = next {
                    if seen {
                        self.valid.set(false);
                    }
                    seen = true;
                    map.next_value_seed(Scan {
                        at,
                        bytes: self.bytes,
                        valid: self.valid,
                    })?;
                } else if self.at == At::Native && key == "samples" {
                    if seen {
                        self.valid.set(false);
                    }
                    seen = true;
                    if let Text(Cow::Borrowed(value)) = map.next_value::<Text<'de>>()? {
                        match self.bytes.get().checked_add(value.len()) {
                            Some(bytes) => self.bytes.set(bytes),
                            None => self.valid.set(false),
                        }
                    }
                } else {
                    map.next_value::<IgnoredAny>()?;
                }
            }
            if self.at == At::Kind && fields != 1 {
                self.valid.set(false);
            }
            Ok(())
        }
    }
    let bytes = Cell::new(0);
    let valid = Cell::new(true);
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let result = Scan {
        at: At::File,
        bytes: &bytes,
        valid: &valid,
    }
    .deserialize(&mut deserializer);
    if result.is_ok() && deserializer.end().is_ok() && valid.get() && bytes.get() <= text.len() {
        bytes.get()
    } else {
        0
    }
}

/// Inspect retained live/source properties without rebuilding pixels or color state.
/// Args: original JSON value. Returns its greatest required retained-feature version.
pub(crate) fn retained_feature_version(value: &serde_json::Value) -> Result<u32, String> {
    features::value_version(value)
}

/// Refuse unsupported retained state before a project or preview reader can discard fields.
/// Args: original JSON. Returns a version error before typed materialization.
pub(crate) fn validate_retained_json(text: &str) -> Result<(), String> {
    let (required, declared) = features::serialized_version(text)?;
    if required > VERSION || declared.is_some_and(|version| !supports_version(version)) {
        return Err("Unsupported omadesign project version".into());
    }
    if let Some(declared) = declared
        && required >= 18
        && declared < required
    {
        return Err(format!(
            "Project declares version {declared}, but its retained data requires version {required}"
        ));
    }
    Ok(())
}

/// Restore shared vector sources before materializing project pixels.
/// Args: document, declared version and source table. Returns a version or reference error.
pub(crate) fn restore_sources(
    doc: &mut Document,
    version: u32,
    sources: &std::collections::BTreeMap<String, std::sync::Arc<str>>,
) -> Result<(), String> {
    if !supports_version(version) {
        return Err("Unsupported omadesign project version".into());
    }
    features::validate_version(doc, version, !sources.is_empty())?;
    crate::svg_source::unpack_sources(doc, sources)
}

/// Shared normalization after pixels have been decoded under the caller's budgets.
pub(crate) fn normalize_decoded_document(
    mut doc: Document,
    version: u32,
) -> Result<Document, String> {
    if !supports_version(version) {
        return Err("Unsupported omadesign project version".into());
    }
    features::validate_version(&doc, version, false)?;
    crate::motion::compositions::visit_documents_mut(&mut doc, |doc| {
        crate::deep_color::cms::validate_profile(&doc.color.icc_profile, doc.color.model)?;
        if doc.color.intent > 3 {
            return Err("Invalid document color rendering intent".into());
        }
        doc.validate_sources()?;
        if let Some(replica) = &doc.collaboration {
            crate::collaboration::resources::validate(replica)?;
        }
        crate::layout_components::validate_properties(doc)?;
        for layer in &mut doc.layers {
            if version < 7 {
                layer.filters.migrate_legacy(layer.blend);
            }
            if let Some(shapes) = layer.kind.shapes_mut() {
                for shape in shapes {
                    if version < 7 {
                        shape.filters.migrate_legacy(shape.blend);
                    }
                    if version < 8 {
                        if let crate::geom::Geom::Text(t) = &mut shape.geom
                            && t.wrap_width.is_some()
                        {
                            t.update_paragraphs(0, t.content.chars().count(), |p| {
                                p.overflow_wrap = true
                            });
                        }
                    }
                    crate::text::fill_contours(&mut shape.geom);
                }
            }
        }
        crate::text_geometry::reflow(doc);
        for comp in &doc.motion.compositions {
            crate::document::bump_id(comp.id);
        }
        if let Some(scene) = &doc.motion.scene3d {
            for object in &scene.objects {
                crate::document::bump_id(object.id);
            }
        }
        doc.ensure_ids()?;
        doc.validate_hierarchy()?;
        Ok(())
    })?;
    crate::motion::compositions::validate(&doc)?;
    Ok(doc)
}

/// Versionless vector clipboard payloads predate independent effect appearance.
/// Newer writers include explicit per-effect blend/opacity fields; preserve them.
pub fn decode_clipboard_shapes(text: &str) -> Result<Vec<crate::document::Shape>, String> {
    let raw: Vec<serde_json::Value> = serde_json::from_str(text).map_err(|e| e.to_string())?;
    raw.into_iter()
        .map(|value| {
            let mut shape: crate::document::Shape =
                serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
            let filters = &value["filters"];
            let old_effects = filters.get("legacy_composite").is_none()
                && filters["items"].as_array().is_some_and(|items| {
                    items.iter().any(|item| {
                        [
                            "Shadow",
                            "InnerShadow",
                            "OuterGlow",
                            "InnerGlow",
                            "ColorOverlay",
                        ]
                        .iter()
                        .filter_map(|kind| item.get(*kind))
                        .any(|effect| {
                            effect.get("blend").is_none() || effect.get("opacity").is_none()
                        })
                    })
                });
            if old_effects {
                shape.filters.migrate_legacy(shape.blend);
            }
            Ok(shape)
        })
        .collect()
}

fn compress_pixels(px: &Pixels) -> Result<Pixels, String> {
    use image::ImageEncoder;
    let mut png = crate::deep_color::storage::Builder::<u8>::new()?;
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&px.data, px.w, px.h, image::ExtendedColorType::Rgba8)
        .map_err(|e| e.to_string())?;
    let png = png.finish()?;
    let encoded_memory = crate::resources::global().reserve_now(
        (png.len() as u64).div_ceil(3).saturating_mul(4),
        &crate::deep_color::storage::check,
    )?;
    let mut b64 = String::new();
    b64.try_reserve_exact(
        usize::try_from(encoded_memory.bytes()).map_err(|_| "Project encoded pixels overflow")?,
    )
    .map_err(|_| "Not enough memory to encode project pixels")?;
    base64::engine::general_purpose::STANDARD.encode_string(png.as_slice(), &mut b64);
    encoded_memory.resize_now(b64.capacity() as u64, &crate::deep_color::storage::check)?;
    Ok(Pixels {
        native: px.native.clone(),
        w: px.w,
        h: px.h,
        data: crate::resources::global().adopt(b64.into_bytes(), encoded_memory)?,
        version: px.version,
        cached_pm: std::cell::RefCell::new(None),
        cached_uniform: Default::default(),
    })
}

fn decompress_pixels(px: &Pixels) -> Result<Pixels, String> {
    // Already raw? Overflow is not a match: it panics in debug and wraps in release.
    if let Some(expected) = (px.w as usize)
        .checked_mul(px.h as usize)
        .and_then(|count| count.checked_mul(4))
        && px.data.len() == expected
    {
        px.validate_native()?;
        return Ok(px.clone());
    }
    use image::ImageDecoder;
    use std::io::Read;
    let b64 = std::str::from_utf8(&px.data).map_err(|e| e.to_string())?;
    let mut input = base64::read::DecoderReader::new(
        b64.as_bytes(),
        &base64::engine::general_purpose::STANDARD,
    );
    let mut packed = crate::deep_color::storage::Builder::<u8>::new()?;
    let mut chunk = [0u8; 16384];
    loop {
        let read = input.read(&mut chunk).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        packed.extend(&chunk[..read])?;
    }
    let packed = packed.finish()?;
    let expected = crate::deep_color::storage::count(px.w, px.h, 4)?;
    let _codec = crate::resources::global()
        .reserve_now(64 * 1024 * 1024, &crate::deep_color::storage::check)?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some((expected as u64).saturating_add(64 * 1024 * 1024));
    let decoder = image::codecs::png::PngDecoder::with_limits(
        std::io::Cursor::new(packed.as_slice()),
        limits,
    )
    .map_err(|e| e.to_string())?;
    if decoder.dimensions() != (px.w, px.h) || decoder.color_type() != image::ColorType::Rgba8 {
        return Err("Packed project raster must contain matching straight RGBA8 cache data".into());
    }
    let mut data = crate::resources::global()
        .allocate_now::<u8, _>(expected, &crate::deep_color::storage::check)?;
    decoder
        .read_image(data.as_mut_slice())
        .map_err(|e| e.to_string())?;
    let mut out = Pixels::from_byte_allocation(px.w, px.h, std::sync::Arc::new(data))?;
    out.native = px.native.clone();
    out.validate_native()?;
    if out
        .native
        .as_ref()
        .is_some_and(|n| n.samples.len() != out.data.len())
    {
        return Err("Native sample dimensions mismatch".into());
    }
    Ok(out)
}

pub fn save_to(doc: &Document, path: &Path) -> Result<(), String> {
    let output = encode_admitted(doc)?;
    save_encoded_to(doc, path, &output, &crate::ml::NoProgress)
}

/// Write an admitted project atomically without releasing its output reservation.
/// Args: document for font archival, destination, encoded owner and write checks. Returns after sync and rename or rollback.
pub(crate) fn save_encoded_to(
    doc: &Document,
    path: &Path,
    output: &std::sync::Arc<crate::resources::Allocation<u8>>,
    progress: &dyn crate::ml::Progress,
) -> Result<(), String> {
    use std::io::Write;
    progress.check()?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let copied_fonts = crate::typography::archive_document_fonts(doc, parent)?;
    let temporary = parent.join(format!(
        ".oma-save-{}-{}.tmp",
        std::process::id(),
        crate::document::next_id()
    ));
    let mut created = false;
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        created = true;
        if let Ok(metadata) = std::fs::metadata(path)
            && metadata.is_file()
        {
            file.set_permissions(metadata.permissions())
                .map_err(|e| e.to_string())?;
        }
        for chunk in output.as_slice().chunks(256 * 1024) {
            progress.check()?;
            file.write_all(chunk).map_err(|e| e.to_string())?;
        }
        progress.check()?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        std::fs::rename(&temporary, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        if created {
            let _ = std::fs::remove_file(temporary);
        }
        for font in copied_fonts {
            let _ = std::fs::remove_file(font);
        }
    }
    result
}

pub(crate) fn read_admitted(path: &Path) -> Result<crate::resources::Allocation<u8>, String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let size = usize::try_from(file.metadata().map_err(|e| e.to_string())?.len())
        .map_err(|_| "Project file length overflows")?;
    let mut bytes = crate::resources::global()
        .allocate_now::<u8, _>(size, &crate::deep_color::storage::check)?;
    file.read_exact(bytes.as_mut_slice())
        .map_err(|e| e.to_string())?;
    let mut extra = [0u8; 1];
    if file.read(&mut extra).map_err(|e| e.to_string())? != 0 {
        return Err("Project changed while reading; reopen it again".into());
    }
    Ok(bytes)
}

pub fn load_from(path: &Path) -> Result<Document, String> {
    let bytes = read_admitted(path)?;
    let s = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
    if s.contains("omatype:") {
        crate::typography::load_for_document(path)?;
    }
    let doc = decode_admitted(s)?;
    crate::typography::validate_document_fonts(&doc)?;
    Ok(doc)
}

pub fn dialog_save(name: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("omadesign", &["oma"])
        .set_file_name(if name.ends_with(".oma") {
            name.to_string()
        } else {
            format!("{name}.oma")
        })
        .save_file()
}

const PLACE_EXTS: &[&str] = &[
    "oma",
    "svg",
    "svgz",
    "png",
    "jpg",
    "jpeg",
    "webp",
    "gif",
    "bmp",
    "tif",
    "tiff",
    "pdf",
    "ai",
    "eps",
    "ps",
    "psd",
    "psb",
    "omamotion",
    "gltf",
    "glb",
    "obj",
    "stl",
    "fbx",
    "ora",
    "xcf",
    "af",
    "afdesign",
    "afphoto",
    "afpub",
    "aftemplate",
    "afpackage",
];

fn raw_extensions() -> Vec<String> {
    // Portal glob filters are case-sensitive; cameras normally write uppercase
    // suffixes while edited/downloaded files often use lowercase ones.
    crate::formats::raw::EXTENSIONS
        .iter()
        .flat_map(|extension| [(*extension).to_owned(), extension.to_ascii_uppercase()])
        .collect()
}

fn supported_extensions() -> Vec<String> {
    PLACE_EXTS
        .iter()
        .map(|extension| (*extension).to_owned())
        .chain(raw_extensions())
        .chain(["omaphoto".into(), "OMAPHOTO".into()])
        .collect()
}

pub fn dialog_open() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("All supported", &supported_extensions())
        .add_filter("omadesign", &["oma"])
        .add_filter("Photo settings", &["omaphoto", "OMAPHOTO"])
        .add_filter("Camera RAW", &raw_extensions())
        .add_filter(
            "Layered documents",
            &[
                "psd",
                "psb",
                "ora",
                "xcf",
                "af",
                "afdesign",
                "afphoto",
                "afpub",
                "aftemplate",
                "afpackage",
            ],
        )
        .add_filter(
            "Images",
            &[
                "png", "jpg", "jpeg", "webp", "gif", "tif", "tiff", "bmp", "psd", "xcf",
            ],
        )
        .add_filter("Vector", &["svg", "svgz", "pdf", "ai", "eps"])
        .pick_file()
}

pub fn dialog_photo() -> Option<PathBuf> {
    let extensions: Vec<String> = [
        "png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "omaphoto",
    ]
    .iter()
    .flat_map(|extension| [(*extension).to_owned(), extension.to_ascii_uppercase()])
    .chain(raw_extensions())
    .collect();
    rfd::FileDialog::new()
        .set_title("Open a photo or saved settings")
        .add_filter("Photos and saved settings", &extensions)
        .add_filter("Photo settings", &["omaphoto", "OMAPHOTO"])
        .add_filter("Camera RAW", &raw_extensions())
        .pick_file()
}

pub fn dialog_place() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Place", &supported_extensions())
        .add_filter("Camera RAW", &raw_extensions())
        .add_filter(
            "Images",
            &[
                "png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "psd",
            ],
        )
        .add_filter("Vector", &["svg", "svgz", "pdf", "ai", "eps"])
        .pick_file()
}

pub fn dialog_export(kind: &str, ext: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(kind, &[ext])
        .set_file_name(format!("export.{ext}"))
        .save_file()
}

pub fn dialog_folder() -> Option<PathBuf> {
    rfd::FileDialog::new().pick_folder()
}

fn recents_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return PathBuf::from(format!("{xdg}/omadesign/recent.json"));
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(format!("{home}/.config/omadesign/recent.json"))
}

pub fn load_recents() -> Vec<PathBuf> {
    load_recents_raw()
        .into_iter()
        .filter(|p| p.exists())
        .take(12)
        .collect()
}

pub fn load_recents_all() -> Vec<PathBuf> {
    load_recents_raw()
}

pub fn push_recent(path: &Path) {
    let mut v = load_recents_raw();
    v.retain(|p| p != path);
    v.insert(0, path.to_path_buf());
    v.truncate(24);
    write_recents(&v);
}

pub fn remove_recent(path: &Path) {
    let mut v = load_recents_raw();
    v.retain(|p| p != path);
    write_recents(&v);
}

fn load_recents_raw() -> Vec<PathBuf> {
    let Ok(s) = std::fs::read_to_string(recents_path()) else {
        return vec![];
    };
    serde_json::from_str::<Vec<String>>(&s)
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .collect()
}

fn write_recents(v: &[PathBuf]) {
    let strings: Vec<String> = v.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    if let Some(dir) = recents_path().parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(s) = serde_json::to_string_pretty(&strings) {
        let _ = std::fs::write(recents_path(), s);
    }
}

pub fn data_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME")
        && !xdg.is_empty()
    {
        return PathBuf::from(format!("{xdg}/omadesign"));
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(format!("{home}/.local/share/omadesign"))
}

pub fn swap_dir() -> PathBuf {
    data_dir()
}

pub fn new_swap_id() -> String {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let r = crate::document::next_id();
    format!("{t:016x}{r:016x}")
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SwapMeta {
    pub id: String,
    pub original: Option<PathBuf>,
    pub name: String,
    pub saved_at: u64,
    pub doc: crate::document::Document,
}

#[derive(Serialize, Deserialize)]
struct SwapArchive {
    #[serde(flatten)]
    meta: SwapMeta,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version: Option<u32>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    svg_sources: std::collections::BTreeMap<String, std::sync::Arc<str>>,
}

pub fn swap_path(id: &str) -> PathBuf {
    swap_dir().join(format!("{id}.oma.swp"))
}

fn pack_rasters(doc: &mut Document) -> Result<(), String> {
    crate::motion::compositions::visit_documents_mut(doc, |doc| {
        for layer in &mut doc.layers {
            if let Some(px) = layer.kind.pixels_mut() {
                *px = compress_pixels(px)?;
            }
            if let Some(mask) = layer.mask.as_mut() {
                *mask = compress_pixels(mask)?;
            }
            if let Some(shapes) = layer.kind.shapes_mut() {
                for shape in shapes {
                    if let Some(mask) = shape.mask.as_mut() {
                        *mask = compress_pixels(mask)?;
                    }
                }
            }
        }
        Ok(())
    })
}

fn unpack_rasters(doc: &mut Document) -> Result<(), String> {
    crate::motion::compositions::visit_documents_mut(doc, |doc| {
        for layer in &mut doc.layers {
            if let Some(px) = layer.kind.pixels_mut() {
                *px = decompress_pixels(px)?;
            }
            if let Some(mask) = layer.mask.as_mut() {
                *mask = decompress_pixels(mask)?;
            }
            for shape in layer.kind.shapes_mut().into_iter().flatten() {
                if let Some(mask) = shape.mask.as_mut() {
                    *mask = decompress_pixels(mask)?;
                }
            }
        }
        Ok(())
    })
}

/// A worker writes a temporary file; only the UI may publish it after checking
/// that the document is still open and unsaved. Dropping it cancels the write.
pub(crate) struct PreparedSwap {
    temporary: PathBuf,
    destination: PathBuf,
    font_copies: Vec<PathBuf>,
    font_ids: std::collections::HashSet<String>,
    committed: bool,
}

impl PreparedSwap {
    pub(crate) fn commit(mut self) -> Result<PathBuf, String> {
        std::fs::rename(&self.temporary, &self.destination).map_err(|e| e.to_string())?;
        self.committed = true;
        // JSON is now committed. Remove only obsolete font files from this
        // snapshot's private archive; a failed preparation never prunes them.
        let archive = swap_font_archive(&self.destination);
        if let Ok(files) = std::fs::read_dir(archive.join(".omabrand/fonts")) {
            for file in files.flatten() {
                let path = file.path();
                if file.file_type().is_ok_and(|kind| kind.is_file())
                    && path.file_stem().and_then(|s| s.to_str()).is_some_and(|id| {
                        id.len() == 32
                            && id.bytes().all(|b| b.is_ascii_hexdigit())
                            && !self.font_ids.contains(id)
                    })
                {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
        remove_empty_font_archive(&archive);
        Ok(self.destination.clone())
    }
}

impl Drop for PreparedSwap {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.temporary);
        if !self.committed {
            for path in &self.font_copies {
                let _ = std::fs::remove_file(path);
            }
            remove_empty_font_archive(&swap_font_archive(&self.destination));
        }
    }
}

pub(crate) fn prepare_swap(
    mut meta: SwapMeta,
    destination: PathBuf,
) -> Result<PreparedSwap, String> {
    use std::io::Write;

    crate::motion::compositions::validate(&meta.doc)?;
    for doc in crate::motion::compositions::documents(&meta.doc)? {
        doc.validate_sources()?;
        if let Some(replica) = &doc.collaboration {
            crate::collaboration::resources::validate(replica)?;
        }
    }
    if features::required_version(&meta.doc) > VERSION {
        return Err("Unsupported omadesign project version".into());
    }
    pack_rasters(&mut meta.doc)?;
    if let Some(dir) = destination.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let temporary = destination.with_extension(format!("{}.tmp", new_swap_id()));
    let file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    let mut prepared = PreparedSwap {
        temporary,
        destination,
        font_copies: vec![],
        font_ids: Default::default(),
        committed: false,
    };
    (prepared.font_copies, prepared.font_ids) = crate::typography::archive_recovery_fonts(
        &meta.doc,
        &swap_font_archive(&prepared.destination),
    )?;
    let version = Some(features::required_version(&meta.doc));
    let svg_sources = crate::svg_source::pack_sources(&mut meta.doc)?;
    let mut writer = std::io::BufWriter::new(file);
    serde_json::to_writer(
        &mut writer,
        &SwapArchive {
            meta,
            version,
            svg_sources,
        },
    )
    .map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;
    writer.get_ref().sync_all().map_err(|e| e.to_string())?;
    Ok(prepared)
}

/// Restore recovery resource references before recovery or previews use the document.
/// Args: original recovery JSON. Returns metadata with every current and retained SVG reference restored.
pub(crate) fn decode_swap_archive(s: &str) -> Result<SwapMeta, String> {
    validate_retained_json(s)?;
    let archive: SwapArchive = serde_json::from_str(s).map_err(|e| e.to_string())?;
    let mut meta = archive.meta;
    let version = archive
        .version
        .unwrap_or_else(|| features::required_version(&meta.doc));
    restore_sources(&mut meta.doc, version, &archive.svg_sources)?;
    Ok(meta)
}

pub fn load_swap(path: &Path) -> Result<SwapMeta, String> {
    let bytes = read_admitted(path)?;
    let s = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
    let _parser = crate::resources::global()
        .reserve_now(bytes.len() as u64, &crate::deep_color::storage::check)?;
    let mut meta = decode_swap_archive(s)?;
    let archive = swap_font_archive(path);
    if archive.is_dir() {
        crate::typography::register_archive(&archive)?;
    } else if let Some(original) = &meta.original {
        crate::typography::load_for_document(original)?;
    }
    crate::typography::validate_document_fonts(&meta.doc)?;
    unpack_rasters(&mut meta.doc)?;
    meta.doc = normalize_decoded_document(meta.doc, VERSION)?;
    Ok(meta)
}

pub fn delete_swap(id: &str) {
    let _ = delete_swap_at(&swap_path(id));
}

fn swap_font_archive(path: &Path) -> PathBuf {
    path.with_extension("swp-fonts")
}

fn remove_empty_font_archive(path: &Path) {
    for directory in [
        path.join(".omabrand/fonts"),
        path.join(".omabrand"),
        path.to_owned(),
    ] {
        let _ = std::fs::remove_dir(directory);
    }
}

pub fn delete_swap_at(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    match std::fs::remove_dir_all(swap_font_archive(path)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

pub fn list_swaps() -> Vec<(PathBuf, SwapMeta)> {
    let Ok(read) = std::fs::read_dir(swap_dir()) else {
        return vec![];
    };
    let mut out = vec![];
    for e in read.flatten() {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("swp") {
            continue;
        }
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !name.ends_with(".oma.swp") {
            continue;
        }
        // Listing needs metadata only. Keep older snapshots visible even when
        // restoring one needs a missing font; Recover reports that error.
        if let Some(meta) = std::fs::read_to_string(&p)
            .ok()
            .and_then(|json| decode_swap_archive(&json).ok())
        {
            out.push((p, meta));
        }
    }
    out.sort_by(|a, b| b.1.saved_at.cmp(&a.1.saved_at));
    out
}

fn font_recents_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return PathBuf::from(format!("{xdg}/omadesign/font-recents.json"));
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(format!("{home}/.config/omadesign/font-recents.json"))
}

pub fn load_font_recents() -> Vec<String> {
    let Ok(s) = std::fs::read_to_string(font_recents_path()) else {
        return vec![];
    };
    serde_json::from_str(&s).unwrap_or_default()
}

pub fn push_font_recent(path: &str) {
    if path.is_empty() {
        return;
    }
    let mut v = load_font_recents();
    v.retain(|p| p != path);
    v.insert(0, path.to_string());
    v.truncate(5);
    if let Some(dir) = font_recents_path().parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(s) = serde_json::to_string_pretty(&v) {
        let _ = std::fs::write(font_recents_path(), s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Cmd, Shape, Style, apply};
    use crate::geom::{Geom, Pt};

    #[test]
    fn native_parser_deducts_only_borrowed_root_raster_samples() {
        let raster =
            serde_json::json!({"kind":{"Raster":{"pixels":{"native":{"samples":"AAAA"}}}}});
        let text =
            serde_json::json!({"version":VERSION,"doc":{"layers":[raster.clone(),raster.clone()],
            "history":{"layers":[raster.clone()]},"opaque":{"native":{"samples":"opaque"}}},
            "other":{"doc":{"layers":[raster.clone()]}}})
            .to_string();
        assert_eq!(borrowed_native_sample_bytes(&text), 8);
        let pool = crate::resources::global();
        let before = (pool.usage().0, pool.pending_bytes());
        let parser = admit_parser(&text).unwrap();
        assert_eq!(parser.bytes(), text.len() as u64 - 8);
        assert_eq!(pool.pending_bytes() - before.1, parser.bytes());
        drop(parser);
        assert_eq!((pool.usage().0, pool.pending_bytes()), before);
        let escaped = text.replace("AAAA", "\\u0041AAA");
        assert_eq!(borrowed_native_sample_bytes(&escaped), 0);
        let parser = admit_parser(&escaped).unwrap();
        assert_eq!(parser.bytes(), escaped.len() as u64);
        drop(parser);
        assert_eq!((pool.usage().0, pool.pending_bytes()), before);
        let mut mixed: serde_json::Value = serde_json::from_str(&text).unwrap();
        mixed["doc"]["layers"][0]["kind"]["Raster"]["pixels"]["native"]["samples"] = "BBBB".into();
        let mixed = mixed.to_string().replacen("BBBB", "\\u0042BBB", 1);
        assert_eq!(borrowed_native_sample_bytes(&mixed), 4);
        let parser = admit_parser(&mixed).unwrap();
        assert_eq!(parser.bytes(), mixed.len() as u64 - 4);
        drop(parser);
        assert_eq!((pool.usage().0, pool.pending_bytes()), before);
    }

    #[test]
    fn native_parser_keeps_ambiguous_escaped_and_opaque_paths_fully_charged() {
        let original = r#"{"version":23,"doc":{"layers":[{"kind":{"Raster":{"pixels":{"native":{"samples":"AAAA"}}}}}]}}"#;
        let doc = serde_json::from_str::<serde_json::Value>(original).unwrap()["doc"].to_string();
        let cases = [
            original.replace("\"doc\"", "\"d\\u006fc\""),
            original.replace(
                "\"samples\":\"AAAA\"",
                "\"samples\":\"AAAA\",\"samples\":\"AAAA\"",
            ),
            original.replace("\"pixels\":", "\"wrong_pixels\":"),
            original.replace("\"samples\":\"AAAA\"", "\"samples\":42"),
            original.replace("\"Raster\":", "\"Vector\":{},\"Raster\":"),
            original.replace("\"layers\":[", "\"history\":["),
            format!(r#"{{"version":23,"doc":{doc},"doc":{doc}}}"#),
            format!(r#"{{"project":{original}}}"#),
        ];
        let pool = crate::resources::global();
        let before = (pool.usage().0, pool.pending_bytes());
        for text in cases {
            assert_eq!(borrowed_native_sample_bytes(&text), 0, "{text}");
            let parser = admit_parser(&text).unwrap();
            assert_eq!(parser.bytes(), text.len() as u64);
            drop(parser);
            assert_eq!((pool.usage().0, pool.pending_bytes()), before);
        }
        let future = original.replace("23", "4294967295");
        assert!(admit_parser(&future).unwrap_err().contains("Unsupported"));
        let understated = original.replace("23", "1");
        assert!(
            admit_parser(&understated)
                .unwrap_err()
                .contains("requires version")
        );
        assert_eq!((pool.usage().0, pool.pending_bytes()), before);
    }

    #[test]
    fn admitted_project_output_refuses_size_drift_and_releases_its_owner() {
        struct Changing {
            grow: bool,
            calls: std::cell::Cell<u8>,
        }
        impl Serialize for Changing {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let first = self.calls.replace(self.calls.get() + 1) == 0;
                serializer.serialize_str(if first == self.grow { "a" } else { "different" })
            }
        }
        let pool = crate::resources::global();
        let before = (pool.usage().0, pool.pending_bytes());
        for grow in [false, true] {
            let changing = Changing {
                grow,
                calls: std::cell::Cell::new(0),
            };
            let error = encode_prepared(&changing).unwrap_err();
            assert!(error.contains("Project changed during encoding"));
            assert_eq!(changing.calls.get(), 2);
            assert_eq!((pool.usage().0, pool.pending_bytes()), before);
        }
    }

    #[test]
    fn admitted_project_output_survives_roundtrip_write_failure_and_last_alias() {
        use crate::{deep_color::Depth, document::Layer};
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        struct FailWrite<'a> {
            owner: &'a Arc<crate::resources::Allocation<u8>>,
            checks: AtomicUsize,
        }
        impl crate::ml::Progress for FailWrite<'_> {
            fn report(&self, _: &str, _: usize, _: usize) {}
            fn cancelled(&self) -> bool {
                false
            }
            fn check(&self) -> Result<(), String> {
                assert!(crate::resources::global().usage().0 >= self.owner.bytes());
                if self.checks.fetch_add(1, Ordering::Relaxed) == 2 {
                    Err("injected project write failure".into())
                } else {
                    Ok(())
                }
            }
        }
        let before = crate::resources::global().usage().0;
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join(".qa-runs");
        std::fs::create_dir_all(&parent).unwrap();
        let directory = parent.join(format!(
            "admitted-project-{}-{}",
            std::process::id(),
            crate::document::next_id()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("image.oma");
        let samples: Vec<_> = (0..17 * 11)
            .flat_map(|n| [n as f32 / 193., 0.12345679, 0.8765432, 1.])
            .collect();
        let pixels = Pixels::from_samples(17, 11, Depth::F32, samples.clone()).unwrap();
        let mut doc = Document::new("admitted project", 17., 11., 72.);
        doc.transparent = true;
        doc.color.depth = Depth::F32;
        doc.color.intent = 1;
        doc.layers = vec![Layer::placed_raster(
            "RAW",
            pixels,
            Pt::ZERO,
            Pt::new(17., 11.),
        )];
        let owner = encode_admitted(&doc).unwrap();
        let weak = Arc::downgrade(owner.lease());
        let alias = owner.clone();
        let decoded = decode(std::str::from_utf8(owner.as_slice()).unwrap()).unwrap();
        assert_eq!(
            decoded.layers[0]
                .kind
                .pixels()
                .unwrap()
                .samples()
                .unwrap()
                .as_slice(),
            samples
        );
        drop(decoded);
        std::fs::write(&path, b"previous project").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        }
        let failure = FailWrite {
            owner: &owner,
            checks: AtomicUsize::new(0),
        };
        assert!(
            save_encoded_to(&doc, &path, &owner, &failure)
                .unwrap_err()
                .contains("injected project write")
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"previous project");
        assert_eq!(
            std::fs::read_dir(&directory).unwrap().count(),
            1,
            "failed temporary write leaked"
        );
        save_to(&doc, &path).unwrap();
        let reopened = load_from(&path).unwrap();
        assert_eq!(reopened.color, doc.color);
        assert_eq!(
            reopened.layers[0]
                .kind
                .pixels()
                .unwrap()
                .samples()
                .unwrap()
                .as_slice(),
            samples
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o640
            );
        }
        let blocked = directory.join("blocked.oma");
        std::fs::create_dir(&blocked).unwrap();
        std::fs::write(blocked.join("keep"), b"preserved").unwrap();
        assert!(save_encoded_to(&doc, &blocked, &owner, &crate::ml::NoProgress).is_err());
        assert_eq!(std::fs::read(blocked.join("keep")).unwrap(), b"preserved");
        drop(owner);
        assert!(weak.upgrade().is_some());
        drop(alias);
        assert!(weak.upgrade().is_none());
        drop(reopened);
        drop(doc);
        assert_eq!(crate::resources::global().usage().0, before);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn stacked_typography_versions_preserve_combined_appearance_and_ranges() {
        use crate::geom::{CharSpan, Leading, TypeRun};
        use crate::text_geometry::{TextFrame, TextOnPath};
        let mut doc = Document::new("Combined typography", 640., 480., 96.);
        let guide = Shape::new(
            Geom::Ellipse {
                center: Pt::new(160., 160.),
                radii: Pt::splat(100.),
            },
            Style::default(),
        );
        let guide_id = guide.id;
        let text = Shape::new(
            Geom::Text(TypeRun {
                content: "AV typography".into(),
                px: 24.,
                ..Default::default()
            }),
            Style::default(),
        );
        let id = text.id;
        doc.layers[1]
            .kind
            .shapes_mut()
            .unwrap()
            .extend([guide, text]);
        let check = |doc: &Document, expected: u32| {
            let encoded = encode(doc).unwrap();
            let file: serde_json::Value = serde_json::from_str(&encoded).unwrap();
            assert_eq!(file["version"], expected);
            decode(&encoded).expect("every emitted version must reopen");
        };
        check(&doc, 5);
        doc.find_shape_mut(1, id).unwrap().style.stroke = Some(crate::document::Stroke {
            alignment: crate::document::StrokeAlignment::Outside,
            ..Default::default()
        });
        check(&doc, 6);
        doc.find_shape_mut(1, id).unwrap().fill_opacity = 0.4;
        check(&doc, 7);
        fn run(doc: &mut Document, id: u64) -> &mut TypeRun {
            match &mut doc.find_shape_mut(1, id).unwrap().geom {
                Geom::Text(run) => run,
                _ => unreachable!(),
            }
        }
        run(&mut doc, id).wrap_width = Some(260.);
        check(&doc, 8);
        run(&mut doc, id).on_path = Some(TextOnPath {
            path_id: guide_id,
            ..Default::default()
        });
        check(&doc, 9);
        run(&mut doc, id).on_path = None;
        run(&mut doc, id).frame = Some(TextFrame::default());
        check(&doc, 10);
        run(&mut doc, id).features.push((*b"smcp", 1));
        check(&doc, 11);
        run(&mut doc, id).manual_kern.insert(1, -30.);
        run(&mut doc, id).spans.push(CharSpan {
            start: 0,
            end: 2,
            leading: Some(Leading::Fixed(45.)),
            tracking: Some(80.),
            baseline_shift: Some(3.),
            ..Default::default()
        });
        check(&doc, 12);
        let restored = decode(&encode(&doc).unwrap()).unwrap();
        let shape = restored.find_shape(1, id).unwrap();
        assert_eq!(shape.fill_opacity, 0.4);
        let Geom::Text(actual) = &shape.geom else {
            unreachable!()
        };
        let Geom::Text(expected) = &doc.find_shape(1, id).unwrap().geom else {
            unreachable!()
        };
        assert_eq!(actual.content, expected.content);
        assert_eq!(actual.frame, expected.frame);
        assert_eq!(actual.features, expected.features);
        assert_eq!(actual.spans, expected.spans);
        assert_eq!(actual.manual_kern, expected.manual_kern);
    }

    #[test]
    fn roundtrip_vector() {
        let mut doc = Document::new("logo", 320.0, 200.0, 72.0);
        apply(
            &mut doc,
            &Cmd::AddShape {
                layer: 1,
                shape: Shape::new(
                    Geom::Ellipse {
                        center: Pt::new(80.0, 80.0),
                        radii: Pt::new(40.0, 30.0),
                    },
                    Style::default(),
                ),
            },
        );
        let s = encode(&doc).unwrap();
        let back = decode(&s).unwrap();
        assert_eq!(back.width, 320.0);
        assert_eq!(back.layers[1].kind.shapes().unwrap().len(), 1);
    }

    #[test]
    fn layout_frames_round_trip_with_comments() {
        let mut doc = crate::layout_templates::build("layout-mobile", 390.0, 844.0, 72.0).unwrap();
        doc.comments.push(crate::cloud::CommentPin::new(
            Pt::new(40.0, 60.0),
            "Ada",
            "Tighten the stack",
            doc.layers[0]
                .kind
                .shapes()
                .unwrap()
                .iter()
                .find(|s| s.layout.frame)
                .map(|s| s.id),
        ));
        doc.cloud = Some(crate::cloud::CloudLink {
            project_id: "proj-1".into(),
            document_id: "doc-1".into(),
            enabled: true,
            collaborators: vec!["ada@example.com".into()],
            published: false,
            gallery_id: String::new(),
        });
        let encoded = encode(&doc).unwrap();
        // The template contains wrapped paragraphs. Version 8 preserves the new
        // Unicode wrapping defaults instead of applying the legacy migration.
        assert!(encoded.contains("\"version\":8"));
        let back = decode(&encoded).unwrap();
        let frames: Vec<_> = back.layers[0]
            .kind
            .shapes()
            .unwrap()
            .iter()
            .filter(|s| s.layout.frame)
            .collect();
        assert!(!frames.is_empty());
        assert!(
            back.layers[0]
                .kind
                .shapes()
                .unwrap()
                .iter()
                .any(|s| s.layout.parent.is_some())
        );
        assert_eq!(back.comments.len(), 1);
        assert!(!back.comments[0].resolved);
        assert_eq!(back.cloud.as_ref().unwrap().project_id, "proj-1");
        assert!(!back.cloud.as_ref().unwrap().published);
    }

    #[test]
    fn save_as_keeps_portable_fonts_and_failures_preserve_the_previous_file() {
        use crate::document::Layer;
        use crate::geom::TypeRun;
        use std::fs;
        const CHILD_PATH: &str = "OMADESIGN_SAVE_FONT_REOPEN_TEST";
        if let Some(path) = std::env::var_os(CHILD_PATH) {
            let path = PathBuf::from(path);
            let recovery = path.extension().is_some_and(|extension| extension == "swp");
            let mut doc = if recovery {
                let meta = load_swap(&path).unwrap();
                assert!(
                    meta.original.is_none(),
                    "unsaved recovery must carry its own fonts"
                );
                let legacy = path.parent().unwrap().join("legacy.oma.swp");
                assert!(load_swap(&legacy).is_err());
                assert!(
                    list_swaps()
                        .iter()
                        .any(|(path, meta)| path == &legacy && meta.name == "Legacy missing font"),
                    "missing legacy fonts hid the recoverable snapshot"
                );
                meta.doc
            } else {
                load_from(&path).unwrap()
            };
            let Geom::Text(run) = &mut doc.layers[0].kind.shapes_mut().unwrap()[0].geom else {
                panic!("saved artwork must remain live text")
            };
            assert!(crate::text::project_font_loaded(&run.font));
            let original = run.contours.clone();
            run.content.push_str(" Ω");
            run.contours = crate::text::shape(run);
            assert!(run.contours.len() > 1 && run.contours != original);
            if !recovery {
                save_to(&doc, &path).unwrap();
            }
            return;
        }
        struct Temp(PathBuf);
        impl Drop for Temp {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let temp = Temp(std::env::temp_dir().join(format!(
            "oma-save-fonts-{}-{}",
            std::process::id(),
            crate::document::next_id()
        )));
        fs::create_dir(&temp.0).unwrap();
        let mut bytes =
            fs::read(crate::text::default_path().expect("a desktop text font")).unwrap();
        bytes.extend_from_slice(format!("save-test-{}", crate::document::next_id()).as_bytes());
        let id = format!("omatype:{:032x}", crate::typography::fingerprint(&bytes));
        crate::text::register_memory_font(
            &id,
            "Imported project face",
            std::sync::Arc::new(bytes.clone()),
        )
        .unwrap();
        let mut doc = Document::new("Placed portable text", 1.0, 1.0, 72.0);
        doc.layers = vec![Layer::vector("Text")];
        doc.layers[0].kind.shapes_mut().unwrap().push(Shape::new(
            Geom::Text(TypeRun {
                content: "Save and travel".into(),
                font: id.clone(),
                px: 28.0,
                ..Default::default()
            }),
            Style::default(),
        ));
        let project = temp.0.join("Saved project");
        let drafts = project.join("drafts");
        fs::create_dir_all(&drafts).unwrap();
        fs::write(project.join(".omacolors"), "[]").unwrap();
        let path = drafts.join("artwork.oma");
        save_to(&doc, &path).unwrap();
        let extension = if bytes.starts_with(b"OTTO") {
            "otf"
        } else {
            "ttf"
        };
        let relative_font = format!(
            ".omabrand/fonts/{}.{extension}",
            id.trim_start_matches("omatype:")
        );
        assert_eq!(fs::read(project.join(&relative_font)).unwrap(), bytes);
        assert!(
            !drafts.join(".omabrand").exists(),
            "nested Save As split the existing project library"
        );
        assert!(
            !project.join(".omatype").exists(),
            "saving artwork must not invent or overwrite font roles"
        );
        let moved = temp.0.join("Moved project");
        fs::rename(&project, &moved).unwrap();
        let path = moved.join("drafts/artwork.oma");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "project::tests::save_as_keeps_portable_fonts_and_failures_preserve_the_previous_file", "--nocapture"])
            .env(CHILD_PATH, &path).output().unwrap();
        assert!(
            output.status.success(),
            "fresh-process edit failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let reopened = load_from(&path).unwrap();
        assert!(
            matches!(&reopened.layers[0].kind.shapes().unwrap()[0].geom, Geom::Text(run) if run.content == "Save and travel Ω")
        );

        let previous = fs::read(&path).unwrap();
        let mut missing = doc.clone();
        let Geom::Text(run) = &mut missing.layers[0].kind.shapes_mut().unwrap()[0].geom else {
            unreachable!()
        };
        run.font = format!("omatype:missing-{}", crate::document::next_id());
        assert!(
            save_to(&missing, &path)
                .unwrap_err()
                .contains("project font")
        );
        assert_eq!(
            fs::read(&path).unwrap(),
            previous,
            "missing font replaced the previous document"
        );

        let failed = temp.0.join("Failed write");
        let blocked = failed.join("blocked.oma");
        fs::create_dir_all(&blocked).unwrap();
        fs::write(blocked.join("keep.txt"), "untouched").unwrap();
        assert!(save_to(&doc, &blocked).is_err());
        assert!(
            !failed.join(&relative_font).exists(),
            "failed document write retained its new font copy"
        );
        assert_eq!(
            fs::read_to_string(blocked.join("keep.txt")).unwrap(),
            "untouched"
        );
        assert!(fs::read_dir(&failed).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".oma-save-")
        }));

        let recovery_home = temp.0.join("Recovery data");
        let recovery_dir = recovery_home.join("omadesign");
        let snapshot = recovery_dir.join("unsaved.oma.swp");
        let meta = SwapMeta {
            id: "unsaved".into(),
            original: None,
            name: doc.name.clone(),
            saved_at: 1,
            doc: doc.clone(),
        };
        prepare_swap(meta.clone(), snapshot.clone())
            .unwrap()
            .commit()
            .unwrap();
        let archive = swap_font_archive(&snapshot);
        let original_archive = archive.join(&relative_font);
        assert_eq!(fs::read(&original_archive).unwrap(), bytes);
        let snapshot_bytes = fs::read(&snapshot).unwrap();
        let mut legacy = meta.clone();
        legacy.name = "Legacy missing font".into();
        legacy.doc = missing.clone();
        let legacy_path = recovery_dir.join("legacy.oma.swp");
        fs::write(&legacy_path, serde_json::to_string(&legacy).unwrap()).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "project::tests::save_as_keeps_portable_fonts_and_failures_preserve_the_previous_file", "--nocapture"])
            .env(CHILD_PATH, &snapshot).env("XDG_DATA_HOME", &recovery_home).output().unwrap();
        assert!(
            output.status.success(),
            "fresh-process unsaved recovery failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let mut replacement_bytes = bytes.clone();
        replacement_bytes.extend_from_slice(b"replacement");
        let replacement_id = format!(
            "omatype:{:032x}",
            crate::typography::fingerprint(&replacement_bytes)
        );
        crate::text::register_memory_font(
            &replacement_id,
            "Replacement recovery face",
            std::sync::Arc::new(replacement_bytes.clone()),
        )
        .unwrap();
        let mut replacement = meta.clone();
        let Geom::Text(run) = &mut replacement.doc.layers[0].kind.shapes_mut().unwrap()[0].geom
        else {
            unreachable!()
        };
        run.font = replacement_id.clone();
        let replacement_archive = archive.join(format!(
            ".omabrand/fonts/{}.{extension}",
            replacement_id.trim_start_matches("omatype:")
        ));
        let pending = prepare_swap(replacement.clone(), snapshot.clone()).unwrap();
        assert!(replacement_archive.exists());
        drop(pending);
        assert!(
            !replacement_archive.exists(),
            "canceled recovery left its new font dependency"
        );
        assert_eq!(fs::read(&snapshot).unwrap(), snapshot_bytes);
        assert_eq!(fs::read(&original_archive).unwrap(), bytes);
        assert!(prepare_swap(legacy, snapshot.clone()).is_err());
        assert_eq!(
            fs::read(&snapshot).unwrap(),
            snapshot_bytes,
            "failed snapshot replaced working recovery"
        );
        assert!(original_archive.exists());
        prepare_swap(replacement, snapshot.clone())
            .unwrap()
            .commit()
            .unwrap();
        assert!(
            !original_archive.exists(),
            "per-swap archive kept obsolete fonts"
        );
        assert_eq!(fs::read(&replacement_archive).unwrap(), replacement_bytes);
        delete_swap_at(&snapshot).unwrap();
        assert!(!snapshot.exists() && !archive.exists());
        assert!(
            legacy_path.exists(),
            "deleting one recovery removed another"
        );
        delete_swap_at(&legacy_path).unwrap();
        assert_eq!(fs::read_dir(&recovery_dir).unwrap().count(), 0);

        // Removed role archives still count toward the cold loader's limit.
        // Appending a dependency must fail atomically, not save a document that
        // can only reopen while its font remains in the current process cache.
        let full = temp.0.join("Full archive");
        let full_fonts = full.join(".omabrand/fonts");
        fs::create_dir_all(&full_fonts).unwrap();
        let mut padded = bytes.clone();
        padded.resize(16 * 1024 * 1024, 0);
        let existing = full_fonts.join("archived-0.ttf");
        fs::write(&existing, &padded).unwrap();
        for index in 1..4 {
            fs::hard_link(&existing, full_fonts.join(format!("archived-{index}.ttf"))).unwrap();
        }
        let full_doc = full.join("artwork.oma");
        fs::write(&full_doc, &previous).unwrap();
        assert!(save_to(&doc, &full_doc).unwrap_err().contains("64 MB"));
        assert_eq!(fs::read(&full_doc).unwrap(), previous);
        assert_eq!(fs::read_dir(&full_fonts).unwrap().count(), 4);
        assert!(!full.join(&relative_font).exists());
    }

    #[test]
    fn overflowing_raster_dimensions_do_not_panic_on_load() {
        // Packed rasters are base64 PNGs. A buffer is raw only when its length
        // is exactly w*h*4. Dimensions whose product does not fit in usize must
        // take that same fallback, not abort the multiply.
        let mut doc = Document::new("overflow", 1.0, 1.0, 72.0);
        doc.layers[0] = crate::document::Layer::raster("Overflow raster", 1, 1).unwrap();
        let encoded = encode(&doc).unwrap();
        let restored = decode(&encoded).unwrap();
        assert_eq!(
            restored.layers[0].kind.pixels().unwrap().data.as_slice(),
            doc.layers[0].kind.pixels().unwrap().data.as_slice()
        );

        let mut file: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        file["doc"]["layers"][0]["kind"]["Raster"]["pixels"]["data"] =
            serde_json::json!([9, 8, 7, 6]);
        let raw = decode(&file.to_string()).unwrap();
        assert_eq!(
            raw.layers[0].kind.pixels().unwrap().data.as_slice(),
            [9, 8, 7, 6]
        );

        file["doc"]["layers"][0]["kind"]["Raster"]["pixels"]["w"] =
            serde_json::json!(2_147_483_648u64);
        file["doc"]["layers"][0]["kind"]["Raster"]["pixels"]["h"] =
            serde_json::json!(2_147_483_648u64);
        file["doc"]["layers"][0]["kind"]["Raster"]["pixels"]["data"] = serde_json::json!([255]);
        assert!(
            decode(&file.to_string()).is_err(),
            "w*h*4 past usize must not be treated as a raw buffer"
        );
    }

    #[test]
    fn decode_rejects_extreme_object_ids_instead_of_overflowing_the_allocator() {
        let mut doc = Document::new("Extreme ids", 64., 64., 72.);
        let mut shape = Shape::new(
            Geom::Rect {
                origin: Pt::new(0., 0.),
                size: Pt::new(10., 10.),
                radius: 0.,
            },
            Style::default(),
        );
        shape.id = u64::MAX;
        doc.layers[1].kind.shapes_mut().unwrap().push(shape);
        let encoded = encode(&doc).unwrap();
        assert!(decode(&encoded).is_err());
        // A swapped recovery document takes the same validation path.
        let root = std::env::temp_dir().join(new_swap_id());
        std::fs::create_dir_all(&root).unwrap();
        let swap = root.join("extreme.oma.swp");
        let meta = SwapMeta {
            id: "extreme".into(),
            original: None,
            name: "Extreme ids".into(),
            saved_at: 0,
            doc,
        };
        std::fs::write(&swap, serde_json::to_string(&meta).unwrap()).unwrap();
        assert!(load_swap(&swap).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod motion_archive_tests;
