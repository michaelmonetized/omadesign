//! OpenRaster 0.0.6 layered pixel interchange, without extracting ZIP entries.

use crate::color::Blend;
use crate::document::{Artboard, Document, Layer, LayerKind, Pixels};
use crate::geom::Pt;
use image::ImageEncoder;
use std::collections::HashSet;
use std::io::{Cursor, Read, Seek, Write};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const LIMIT: u64 = 512 * 1024 * 1024;
const MAX_PIXELS: u64 = 64 * 1024 * 1024;
const LOCK_NS: &str = "https://omadesign.app/ns/openraster";

pub fn read(bytes: &[u8], name: &str) -> Result<(Document, Vec<String>), String> {
    if bytes.len() as u64 > LIMIT {
        return Err("OpenRaster archive exceeds 512 MiB.".into());
    }
    let mut zip = ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("Invalid OpenRaster archive: {e}"))?;
    if zip.is_empty() || zip.len() > 10_000 {
        return Err("OpenRaster archive must contain 1–10,000 entries.".into());
    }
    let mut names = HashSet::new();
    let mut expanded = 0u64;
    for i in 0..zip.len() {
        let file = zip.by_index(i).map_err(|e| e.to_string())?;
        valid_path(file.name())?;
        if !names.insert(file.name().to_string()) {
            return Err("OpenRaster archive contains duplicate entry names.".into());
        }
        expanded = expanded
            .checked_add(file.size())
            .ok_or("OpenRaster archive is too large")?;
        if expanded > LIMIT {
            return Err("OpenRaster expanded data exceeds 512 MiB.".into());
        }
        if !matches!(
            file.compression(),
            CompressionMethod::Stored | CompressionMethod::Deflated
        ) {
            return Err("OpenRaster entries must use stored or deflated ZIP compression.".into());
        }
        if i == 0 && (file.name() != "mimetype" || file.compression() != CompressionMethod::Stored)
        {
            return Err("OpenRaster must start with an uncompressed mimetype entry.".into());
        }
    }
    if entry(&mut zip, "mimetype", 64)? != b"image/openraster" {
        return Err("Archive is not an OpenRaster document.".into());
    }
    let stack = String::from_utf8(entry(&mut zip, "stack.xml", 4 * 1024 * 1024)?)
        .map_err(|_| "OpenRaster stack.xml is not UTF-8")?;
    let xml = roxmltree::Document::parse(&stack)
        .map_err(|e| format!("Invalid OpenRaster layer stack: {e}"))?;
    let root = xml.root_element();
    if root.tag_name().name() != "image" {
        return Err("OpenRaster stack.xml has no image root.".into());
    }
    let w = positive(root, "w", None)?;
    let h = positive(root, "h", None)?;
    dimensions(w, h)?;
    let dpi = positive(root, "xres", Some(72))?;
    let ydpi = positive(root, "yres", Some(72))?;
    let mut doc = Document::new(name, 1., 1., dpi as f32);
    doc.width = w as f32;
    doc.height = h as f32;
    doc.layers.clear();
    doc.transparent = true;
    doc.artboards = vec![Artboard::new(0, Pt::ZERO, Pt::new(w as f32, h as f32))];
    let mut warnings = Vec::new();
    if dpi != ydpi {
        warnings.push("OpenRaster uses different horizontal and vertical resolutions; Omadesign uses the horizontal resolution for both.".into());
    }
    let children: Vec<_> = root.children().filter(|n| n.is_element()).collect();
    if children.len() != 1 || children[0].tag_name().name() != "stack" {
        return Err("OpenRaster must contain one root layer stack.".into());
    }
    let mut reader = Reader {
        zip,
        doc,
        warnings,
        decoded: 0,
        nodes: 0,
    };
    reader.stack(children[0], None, 0)?;
    if reader.doc.layers.is_empty() {
        reader.doc.layers.push(Layer::vector("Layer 1"));
    }
    reader.doc.validate_hierarchy()?;
    Ok((reader.doc, reader.warnings))
}

struct Reader<'a> {
    zip: ZipArchive<Cursor<&'a [u8]>>,
    doc: Document,
    warnings: Vec<String>,
    decoded: u64,
    nodes: usize,
}

impl Reader<'_> {
    fn stack(
        &mut self,
        stack: roxmltree::Node<'_, '_>,
        parent: Option<u64>,
        depth: usize,
    ) -> Result<(), String> {
        if depth > 64 {
            return Err("OpenRaster group nesting exceeds 64 levels.".into());
        }
        // OpenRaster lists the topmost item first; Omadesign uses painter order.
        for node in stack
            .children()
            .filter(|n| n.is_element())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            self.nodes += 1;
            if self.nodes > 10_000 {
                return Err("OpenRaster document exceeds 10,000 layers.".into());
            }
            let mut layer = match node.tag_name().name() {
                "stack" => {
                    let mut group = Layer::group(node.attribute("name").unwrap_or("Group"));
                    group.pass_through = match node.attribute("isolation").unwrap_or("isolate") {
                        "auto" => true,
                        "isolate" => false,
                        value => {
                            return Err(format!("Unsupported OpenRaster group isolation: {value}"));
                        }
                    };
                    self.stack(node, Some(group.id), depth + 1)?;
                    group
                }
                "layer" => {
                    if node.children().any(|child| child.is_element()) {
                        return Err("OpenRaster layers with filters or other nested extensions are not supported.".into());
                    }
                    let source = node
                        .attribute("src")
                        .ok_or("OpenRaster layer has no image source")?;
                    valid_path(source)?;
                    let bytes = entry(&mut self.zip, source, 256 * 1024 * 1024)?;
                    if bytes.len() < 25 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                        return Err(format!(
                            "OpenRaster layer '{source}' is not a PNG. Vector and other extended layer sources are not supported."
                        ));
                    }
                    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
                    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
                    dimensions(width, height)?;
                    let allocation = u64::from(width) * u64::from(height) * 4;
                    if self.decoded + allocation > LIMIT {
                        return Err("OpenRaster layer pixels exceed 512 MiB.".into());
                    }
                    if bytes[24] == 16 {
                        let warning = "OpenRaster 16-bit pixel channels are converted to 8-bit RGBA in Omadesign.";
                        if !self.warnings.iter().any(|w| w == warning) {
                            self.warnings.push(warning.into());
                        }
                    }
                    let pixels = super::decode_image(&bytes)?;
                    self.decoded = self
                        .decoded
                        .checked_add(pixels.data.len() as u64)
                        .ok_or("OpenRaster pixels exceed memory limit")?;
                    if self.decoded > LIMIT {
                        return Err("OpenRaster layer pixels exceed 512 MiB.".into());
                    }
                    let size = Pt::new(pixels.w as f32, pixels.h as f32);
                    Layer::placed_raster(
                        node.attribute("name").unwrap_or("Layer"),
                        pixels,
                        Pt::new(offset(node, "x")? as f32, offset(node, "y")? as f32),
                        size,
                    )
                }
                tag => {
                    return Err(format!(
                        "OpenRaster element '{tag}' cannot be imported without losing its content."
                    ));
                }
            };
            layer.parent = parent;
            layer.opacity = node
                .attribute("opacity")
                .unwrap_or("1")
                .parse::<f32>()
                .map_err(|_| "Invalid OpenRaster opacity")?;
            if !layer.opacity.is_finite() || !(0.0..=1.0).contains(&layer.opacity) {
                return Err("OpenRaster opacity must be between 0 and 1.".into());
            }
            layer.visible = match node.attribute("visibility").unwrap_or("visible") {
                "visible" => true,
                "hidden" => false,
                value => return Err(format!("Invalid OpenRaster visibility: {value}")),
            };
            layer.locked = node.attribute((LOCK_NS, "locked")) == Some("true");
            layer.blend = if layer.is_group && layer.pass_through {
                // The OpenRaster specification ignores the blend on auto groups.
                Blend::Normal
            } else {
                parse_blend(node.attribute("composite-op").unwrap_or("svg:src-over"))?
            };
            if layer.is_group && layer.pass_through && layer.opacity < 1.0 {
                // ORA combines auto-stack opacity with each child before blending.
                // The native renderer applies group opacity to its finished result.
                let mut parents = vec![layer.id];
                while let Some(parent) = parents.pop() {
                    for child in self
                        .doc
                        .layers
                        .iter_mut()
                        .filter(|l| l.parent == Some(parent))
                    {
                        if child.is_group && child.pass_through {
                            parents.push(child.id);
                        } else {
                            child.opacity *= layer.opacity;
                        }
                    }
                }
                layer.opacity = 1.0;
                let note = "OpenRaster pass-through group opacity was applied to its child layers to preserve compositing.";
                if !self.warnings.iter().any(|w| w == note) {
                    self.warnings.push(note.into());
                }
            }
            self.doc.layers.push(layer);
        }
        Ok(())
    }
}

fn valid_path(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.starts_with('/')
        || name.contains('\\')
        || name.contains(':')
        || name.chars().any(char::is_control)
        || name.split('/').any(|part| part == ".." || part == ".")
    {
        return Err("OpenRaster contains an unsafe archive path.".into());
    }
    Ok(())
}

fn entry<R: Read + Seek>(zip: &mut ZipArchive<R>, name: &str, max: u64) -> Result<Vec<u8>, String> {
    let file = zip
        .by_name(name)
        .map_err(|e| format!("OpenRaster entry '{name}' is unavailable: {e}"))?;
    if file.size() > max {
        return Err(format!("OpenRaster entry '{name}' is too large."));
    }
    let mut data = Vec::new();
    file.take(max + 1)
        .read_to_end(&mut data)
        .map_err(|e| format!("Could not read OpenRaster entry '{name}': {e}"))?;
    if data.len() as u64 > max {
        return Err(format!(
            "OpenRaster entry '{name}' exceeded its size limit."
        ));
    }
    Ok(data)
}

fn positive(node: roxmltree::Node<'_, '_>, key: &str, default: Option<u32>) -> Result<u32, String> {
    let number = match node.attribute(key) {
        Some(s) => s
            .parse::<u32>()
            .map_err(|_| format!("Invalid OpenRaster {key}"))?,
        None => default.ok_or_else(|| format!("Missing OpenRaster {key}"))?,
    };
    if number == 0 {
        return Err(format!("OpenRaster {key} must be positive."));
    }
    Ok(number)
}

fn offset(node: roxmltree::Node<'_, '_>, key: &str) -> Result<i32, String> {
    node.attribute(key)
        .unwrap_or("0")
        .parse()
        .map_err(|_| format!("Invalid OpenRaster {key} offset"))
}

fn dimensions(w: u32, h: u32) -> Result<(), String> {
    if w == 0 || h == 0 || w > 32768 || h > 32768 || u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err("OpenRaster canvas exceeds 32,768 pixels per side or 64 megapixels.".into());
    }
    Ok(())
}

fn parse_blend(value: &str) -> Result<Blend, String> {
    if value == "svg:src-over" {
        return Ok(Blend::Normal);
    }
    Blend::ALL.into_iter().find(|b| value.strip_prefix("svg:") == Some(b.css()))
        .ok_or_else(|| format!("OpenRaster composite operation '{value}' is not supported. Its appearance cannot be preserved as editable layers."))
}

fn xml_escape(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
        .collect::<String>()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn png(pixels: &Pixels) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            &pixels.data,
            pixels.w,
            pixels.h,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}

pub fn write(doc: &Document) -> Result<(Vec<u8>, Vec<String>), String> {
    if !doc.width.is_finite() || !doc.height.is_finite() || doc.width < 1. || doc.height < 1. {
        return Err("OpenRaster needs a finite, positive canvas size.".into());
    }
    dimensions(doc.width.round() as u32, doc.height.round() as u32)?;
    doc.validate_hierarchy()?;
    if doc.layers.len() > 10_000 {
        return Err("OpenRaster export exceeds 10,000 layers.".into());
    }
    let mut writer = Writer {
        zip: ZipWriter::new(Cursor::new(Vec::new())),
        doc,
        warnings: Vec::new(),
        bytes: 0,
        pixels: 0,
        serial: 0,
    };
    writer.add("mimetype", b"image/openraster")?;
    let mut stack = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<image version=\"0.0.6\" w=\"{}\" h=\"{}\" xres=\"{}\" yres=\"{}\" xmlns:oma=\"{LOCK_NS}\"><stack>",
        doc.width.round() as u32,
        doc.height.round() as u32,
        doc.dpi.round().max(1.) as u32,
        doc.dpi.round().max(1.) as u32
    );
    writer.stack(None, &mut stack, 0)?;
    if !doc.transparent {
        let mut background = Document::new("Canvas background", 1., 1., doc.dpi);
        background.width = doc.width;
        background.height = doc.height;
        background.artboards = doc.artboards.clone();
        background.layers.clear();
        let bytes = crate::compositor::export_png(&background, 1)?;
        writer.add("data/background.png", &bytes)?;
        stack.push_str("<layer name=\"Canvas background\" src=\"data/background.png\"/>");
    }
    stack.push_str("</stack></image>\n");
    writer.add("stack.xml", stack.as_bytes())?;
    let merged = crate::compositor::export_png(doc, 1)?;
    writer.add("mergedimage.png", &merged)?;
    let thumbnail = image::load_from_memory(&merged)
        .map_err(|e| e.to_string())?
        .thumbnail(256, 256);
    let mut thumb = Cursor::new(Vec::new());
    thumbnail
        .write_to(&mut thumb, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    writer.add("Thumbnails/thumbnail.png", thumb.get_ref())?;
    let bytes = writer.zip.finish().map_err(|e| e.to_string())?.into_inner();
    if bytes.len() as u64 > LIMIT {
        return Err("OpenRaster export exceeds 512 MiB.".into());
    }
    Ok((bytes, writer.warnings))
}

struct Writer<'a> {
    zip: ZipWriter<Cursor<Vec<u8>>>,
    doc: &'a Document,
    warnings: Vec<String>,
    bytes: u64,
    pixels: u64,
    serial: usize,
}

impl Writer<'_> {
    fn add(&mut self, name: &str, data: &[u8]) -> Result<(), String> {
        self.bytes = self
            .bytes
            .checked_add(data.len() as u64)
            .ok_or("OpenRaster export too large")?;
        if self.bytes > LIMIT {
            return Err("OpenRaster export exceeds 512 MiB.".into());
        }
        self.zip
            .start_file(
                name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .map_err(|e| e.to_string())?;
        self.zip.write_all(data).map_err(|e| e.to_string())
    }

    fn warn(&mut self, text: &str) {
        if !self.warnings.iter().any(|w| w == text) {
            self.warnings.push(text.into());
        }
    }

    fn stack(&mut self, parent: Option<u64>, out: &mut String, depth: usize) -> Result<(), String> {
        if depth > 64 {
            return Err("OpenRaster group nesting exceeds 64 levels.".into());
        }
        let indices: Vec<_> = self
            .doc
            .layers
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, l)| l.parent == parent)
            .map(|(i, _)| i)
            .collect();
        for index in indices {
            let layer = &self.doc.layers[index];
            let blend = if layer.blend == Blend::Normal {
                "svg:src-over".into()
            } else {
                format!("svg:{}", layer.blend.css())
            };
            if layer.blend == Blend::Exclusion {
                self.warn("Exclusion blending uses an OpenRaster extension and may differ in applications limited to baseline compositing.");
            }
            let attributes = format!(
                "name=\"{}\" opacity=\"{}\" visibility=\"{}\" composite-op=\"{blend}\" oma:locked=\"{}\"",
                xml_escape(&layer.name),
                layer.opacity.clamp(0., 1.),
                if layer.visible { "visible" } else { "hidden" },
                layer.locked
            );
            if layer.is_group && layer.mask.is_none() && !layer.filters.active() {
                if layer.pass_through && layer.opacity < 1.0 {
                    self.warn("OpenRaster applies pass-through group opacity to each child before blending. Overlapping layers in a translucent pass-through group can look different from Omadesign; use an isolated group for consistent interchange.");
                }
                out.push_str(&format!(
                    "<stack {attributes} isolation=\"{}\">",
                    if layer.pass_through {
                        "auto"
                    } else {
                        "isolate"
                    }
                ));
                self.stack(Some(layer.id), out, depth + 1)?;
                out.push_str("</stack>");
                continue;
            }
            let (bytes, x, y) = self.layer_image(index)?;
            let path = format!("data/layer{}.png", self.serial);
            self.serial += 1;
            self.add(&path, &bytes)?;
            out.push_str(&format!(
                "<layer {attributes} src=\"{path}\" x=\"{x}\" y=\"{y}\"/>"
            ));
        }
        Ok(())
    }

    fn layer_image(&mut self, index: usize) -> Result<(Vec<u8>, i32, i32), String> {
        let layer = &self.doc.layers[index];
        if let LayerKind::Raster {
            pixels,
            origin,
            size,
            rotation,
        } = &layer.kind
            && layer.mask.is_none()
            && !layer.filters.active()
            && *rotation == 0.
            && origin.x.is_finite()
            && origin.y.is_finite()
            && origin.x.fract() == 0.
            && origin.y.fract() == 0.
            && origin.x >= i32::MIN as f32
            && origin.x < i32::MAX as f32
            && origin.y >= i32::MIN as f32
            && origin.y < i32::MAX as f32
            && (*size == Pt::ZERO || *size == Pt::new(pixels.w as f32, pixels.h as f32))
        {
            dimensions(pixels.w, pixels.h)?;
            self.pixels += pixels.data.len() as u64;
            if self.pixels > LIMIT {
                return Err("OpenRaster layer pixels exceed 512 MiB.".into());
            }
            return Ok((png(pixels)?, origin.x as i32, origin.y as i32));
        }
        self.pixels += self.doc.width.round() as u64 * self.doc.height.round() as u64 * 4;
        if self.pixels > LIMIT {
            return Err("OpenRaster layer pixels exceed 512 MiB.".into());
        }
        if layer.is_group {
            self.warn("Groups with masks or live effects are rendered as a single pixel layer in OpenRaster. Save .oma to retain their editable children and effects.");
        } else if matches!(layer.kind, LayerKind::Vector { .. }) {
            self.warn("OpenRaster stores pixel layers. Vector artwork and text are rendered separately for each layer and clipped to the canvas; save .oma to retain vector editing.");
        } else {
            self.warn("Transformed pixels, masks and live effects are rendered into their OpenRaster layer and clipped to the canvas.");
        }
        let mut single = Document::new("OpenRaster layer", 1., 1., self.doc.dpi);
        single.width = self.doc.width;
        single.height = self.doc.height;
        single.artboards = self.doc.artboards.clone();
        single.transparent = true;
        single.layers = self
            .doc
            .layers
            .iter()
            .enumerate()
            .filter(|(i, _)| *i == index || self.doc.layer_ancestors(*i).contains(&index))
            .map(|(_, l)| l.clone())
            .collect();
        let root = single
            .layers
            .iter_mut()
            .find(|l| l.id == layer.id)
            .ok_or("Missing OpenRaster export layer")?;
        root.parent = None;
        root.visible = true;
        root.opacity = 1.;
        root.blend = Blend::Normal;
        Ok((crate::compositor::export_png(&single, 1)?, 0, 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_independent_deflated_stack_fixture_in_painter_order() {
        let (doc, warnings) = read(
            include_bytes!("openraster/fixtures/independent.ora"),
            "Independent",
        )
        .unwrap();
        assert!(warnings.is_empty());
        assert_eq!((doc.width, doc.height, doc.dpi), (8., 6., 144.));
        assert_eq!(
            doc.layers
                .iter()
                .map(|l| l.name.as_str())
                .collect::<Vec<_>>(),
            ["Bottom blue", "Hidden red", "Top group"]
        );
        assert_eq!(doc.layers[1].parent, Some(doc.layers[2].id));
        assert!(!doc.layers[1].visible);
        assert_eq!(
            doc.layers[1].kind.raster_xform().unwrap().0,
            Pt::new(-2., 3.)
        );
        assert_eq!(
            doc.layers[1].kind.pixels().unwrap().data,
            [250, 20, 10, 128]
        );
        let rendered =
            super::super::decode_image(&crate::compositor::export_png(&doc, 1).unwrap()).unwrap();
        let offset = (2 * 8 + 1) * 4;
        assert_eq!(&rendered.data[offset..offset + 4], &[10, 20, 250, 255]);
        assert_eq!(&rendered.data[0..4], &[0, 0, 0, 0]);
    }

    #[test]
    fn archive_preserves_nested_pixel_layers_and_required_preview_files() {
        let mut doc = Document::new("ORA", 6., 4., 144.);
        doc.transparent = true;
        let mut group = Layer::group("G & \"one\"");
        group.pass_through = false;
        group.opacity = 0.5;
        let pixels = Pixels::from_rgba(1, 1, vec![255, 0, 0, 128]).unwrap();
        let mut child = Layer::placed_raster("Red", pixels, Pt::new(-2., 3.), Pt::new(1., 1.));
        child.parent = Some(group.id);
        child.blend = Blend::Multiply;
        child.locked = true;
        child.visible = false;
        doc.layers = vec![child, group];
        let (bytes, warnings) = write(&doc).unwrap();
        assert!(warnings.is_empty());
        let mut archive = ZipArchive::new(Cursor::new(&bytes)).unwrap();
        let first = archive.by_index(0).unwrap();
        assert_eq!(first.name(), "mimetype");
        assert_eq!(first.compression(), CompressionMethod::Stored);
        drop(first);
        let stack = entry(&mut archive, "stack.xml", 4096).unwrap();
        assert!(roxmltree::Document::parse(std::str::from_utf8(&stack).unwrap()).is_ok());
        assert!(
            entry(&mut archive, "mergedimage.png", 4096)
                .unwrap()
                .starts_with(b"\x89PNG")
        );
        let thumb = super::super::decode_image(
            &entry(&mut archive, "Thumbnails/thumbnail.png", 4096).unwrap(),
        )
        .unwrap();
        assert!(thumb.w <= 256 && thumb.h <= 256);
        let (opened, _) = read(&bytes, "Reopened").unwrap();
        assert_eq!(opened.layers.len(), 2);
        assert!(opened.layers[1].is_group);
        assert!(!opened.layers[1].pass_through);
        assert_eq!(opened.layers[1].name, "G & \"one\"");
        assert_eq!(opened.layers[0].parent, Some(opened.layers[1].id));
        assert_eq!(opened.layers[0].blend, Blend::Multiply);
        assert!(opened.layers[0].locked);
        assert!(!opened.layers[0].visible);
        assert_eq!(
            opened.layers[0].kind.raster_xform().unwrap().0,
            Pt::new(-2., 3.)
        );
        assert_eq!(
            opened.layers[0].kind.pixels().unwrap().data,
            vec![255, 0, 0, 128]
        );
    }

    #[test]
    fn rejects_path_traversal_and_unsupported_compositing() {
        assert!(valid_path("../secret.png").is_err());
        assert!(valid_path("data/../../secret.png").is_err());
        assert!(valid_path("C:\\secret.png").is_err());
        assert!(parse_blend("svg:dst-out").is_err());
    }

    #[test]
    fn cropped_group_mask_is_baked_at_its_document_offset() {
        let mut doc = Document::new("Mask", 3., 1., 72.);
        doc.transparent = true;
        let mut group = Layer::group("Masked group");
        group.pass_through = false;
        group.mask = Some(Pixels::from_rgba(1, 1, vec![255; 4]).unwrap());
        group.mask_origin = Pt::new(1., 0.);
        group.mask_size = Pt::new(1., 1.);
        let mut red = Layer::placed_raster(
            "Red",
            Pixels::from_rgba(3, 1, [255, 0, 0, 255].repeat(3)).unwrap(),
            Pt::ZERO,
            Pt::new(3., 1.),
        );
        red.parent = Some(group.id);
        doc.layers = vec![red, group];
        let (bytes, notes) = write(&doc).unwrap();
        assert!(notes.iter().any(|n| n.contains("Groups with masks")));
        let (opened, _) = read(&bytes, "Mask opened").unwrap();
        assert_eq!(opened.layers.len(), 1);
        let pixels = opened.layers[0].kind.pixels().unwrap();
        assert_eq!(pixels.data, [0, 0, 0, 0, 255, 0, 0, 255, 0, 0, 0, 0]);
    }

    #[test]
    fn normalizes_openraster_auto_stack_opacity_to_children() {
        let bytes = include_bytes!("openraster/fixtures/independent.ora");
        let mut source = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        for i in 0..source.len() {
            let mut file = source.by_index(i).unwrap();
            let name = file.name().to_owned();
            let mut data = Vec::new();
            file.read_to_end(&mut data).unwrap();
            if name == "stack.xml" {
                data = String::from_utf8(data)
                    .unwrap()
                    .replace("isolation=\"isolate\"", "isolation=\"auto\"")
                    .into_bytes();
            }
            zip.start_file(
                name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(&data).unwrap();
        }
        let bytes = zip.finish().unwrap().into_inner();
        let (doc, notes) = read(&bytes, "Auto").unwrap();
        assert!(doc.layers[2].pass_through);
        assert_eq!(doc.layers[2].opacity, 1.);
        assert_eq!(doc.layers[1].opacity, 0.5);
        assert!(
            notes
                .iter()
                .any(|n| n.contains("applied to its child layers"))
        );
    }
}
