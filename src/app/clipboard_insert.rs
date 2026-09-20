//! Clipboard artwork is prepared off the frame thread, then inserted in one edit.
use super::*;
use crate::clipboard::ClipboardContent;
use crate::document::{Pixels, next_id};

pub(super) struct PreparedClipboard {
    layers: Vec<Layer>,
    notes: Vec<String>,
}

/// All expensive file decoding, SVG parsing, and text shaping can run in the
/// clipboard worker. Capture the text defaults and world-space center at Paste.
pub(super) fn prepare_clipboard_content(
    content: ClipboardContent,
    text: TypeRun,
    style: Style,
    center: Pt,
) -> Result<PreparedClipboard, String> {
    if !center.x.is_finite() || !center.y.is_finite() {
        return Err("The clipboard placement point is invalid".into());
    }
    let mut prepared = PreparedClipboard {
        layers: vec![],
        notes: vec![],
    };
    prepare(content, &text, &style, center, &mut prepared)?;
    if prepared.layers.is_empty() {
        return Err("The clipboard is empty".into());
    }
    Ok(prepared)
}

fn prepare(
    content: ClipboardContent,
    text: &TypeRun,
    style: &Style,
    center: Pt,
    prepared: &mut PreparedClipboard,
) -> Result<(), String> {
    match content {
        ClipboardContent::Empty => {}
        ClipboardContent::Files(paths) => {
            for path in paths {
                let mut item = prepare_clipboard_content(
                    crate::clipboard::read_file(&path)?,
                    text.clone(),
                    style.clone(),
                    center,
                )?;
                // Keep the file's recognizable name on its top-level layers.
                if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                    for layer in &mut item.layers {
                        if layer.parent.is_none() {
                            layer.name = name.to_owned();
                        }
                    }
                }
                prepared.layers.extend(item.layers);
                prepared.notes.extend(item.notes);
            }
        }
        ClipboardContent::Image { name, image } => {
            let count = u64::from(image.w) * u64::from(image.h);
            if count == 0 || count > crate::formats::raw::MAX_PIXELS {
                return Err("The clipboard image has invalid dimensions".into());
            }
            let size = Pt::new(image.w as f32, image.h as f32);
            let pixels = Pixels::from_rgba(image.w, image.h, image.data)
                .ok_or("The clipboard image has invalid pixels")?;
            prepared.layers.push(Layer::placed_raster(
                name,
                pixels,
                center - size * 0.5,
                size,
            ));
        }
        ClipboardContent::Text(content) => {
            if content.is_empty() {
                return Ok(());
            }
            let name = content
                .lines()
                .next()
                .unwrap_or("Text")
                .chars()
                .take(40)
                .collect::<String>();
            let mut run = text.clone();
            run.origin = Pt::ZERO;
            run.content = content;
            let mut geom = Geom::Text(run);
            crate::text::fill_contours(&mut geom);
            geom.translate(center - geom.bbox().center());
            let mut style = style.clone();
            if style.fill.is_none() && style.stroke.is_none() {
                style.fill = Fill::Solid(Rgba::BLACK);
            }
            let mut layer = Layer::vector(if name.is_empty() { "Text".into() } else { name });
            let mut shape = Shape::new(geom, style);
            shape.name = "Text".into();
            layer.kind.shapes_mut().unwrap().push(shape);
            prepared.layers.push(layer);
        }
        ClipboardContent::Svg(svg) => {
            let (mut doc, notes) = crate::formats::svg::read(&svg, "Pasted SVG")?;
            let mut bounds: Option<Bounds> = None;
            for (index, layer) in doc.layers.iter().enumerate() {
                if !doc.layer_visible(index) || layer.is_group {
                    continue;
                }
                match &layer.kind {
                    LayerKind::Vector { shapes } => {
                        for shape in shapes.iter().filter(|shape| shape.visible && !shape.guide) {
                            let b = shape.world_bbox();
                            bounds = Some(bounds.map_or(b, |bounds| bounds.union(b)));
                        }
                    }
                    LayerKind::Raster { .. } => {
                        if let Some(b) = layer.kind.raster_bounds() {
                            bounds = Some(bounds.map_or(b, |bounds| bounds.union(b)));
                        }
                    }
                }
            }
            let bounds = bounds.ok_or("The SVG contains no visible artwork")?;
            if ![bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y]
                .into_iter()
                .all(f32::is_finite)
            {
                return Err("The clipboard SVG has invalid geometry".into());
            }
            let delta = center - bounds.center();
            let ids: HashMap<_, _> = doc
                .layers
                .iter()
                .map(|layer| (layer.id, next_id()))
                .collect();
            for layer in &mut doc.layers {
                layer.id = ids[&layer.id];
                layer.parent = layer.parent.and_then(|id| ids.get(&id).copied());
                match &mut layer.kind {
                    LayerKind::Vector { shapes } => {
                        // SVG clips are canvas-sized masks. Give them a placed
                        // transform instead of resampling/clipping to our artboard.
                        if let Some(mask) = &layer.mask {
                            if layer.mask_size.x <= 0.0 || layer.mask_size.y <= 0.0 {
                                layer.mask_origin = Pt::ZERO;
                                layer.mask_size = Pt::new(mask.w as f32, mask.h as f32);
                            }
                            layer.mask_origin += delta;
                        }
                        for shape in shapes {
                            shape.id = next_id();
                            shape.geom.translate(delta);
                        }
                    }
                    LayerKind::Raster { origin, .. } => *origin += delta,
                }
            }
            doc.validate_hierarchy()?;
            prepared.layers.extend(doc.layers);
            prepared.notes.extend(notes);
        }
    }
    let pixels: u64 = prepared
        .layers
        .iter()
        .flat_map(|layer| {
            [layer.kind.pixels(), layer.mask.as_ref()]
                .into_iter()
                .flatten()
        })
        .map(|pixels| u64::from(pixels.w) * u64::from(pixels.h))
        .sum();
    if pixels > crate::formats::raw::MAX_PIXELS || prepared.layers.len() > 100_000 {
        return Err("The clipboard artwork is too large to paste".into());
    }
    Ok(())
}

impl Studio {
    /// Synchronous entry point for tests and callers outside the frame loop.
    pub fn insert_clipboard_content(
        &mut self,
        content: ClipboardContent,
        center: Pt,
    ) -> Result<(), String> {
        let prepared =
            prepare_clipboard_content(content, self.type_defaults(), self.style.clone(), center)?;
        self.insert_prepared_clipboard(prepared)
    }

    pub(super) fn insert_prepared_clipboard(
        &mut self,
        prepared: PreparedClipboard,
    ) -> Result<(), String> {
        if self.pending_nav.is_some() {
            return Err("Finish the open dialog before pasting".into());
        }
        if prepared.layers.is_empty() {
            return Err("The clipboard is empty".into());
        }
        let first = self.doc.layers.len();
        let mut selection = vec![];
        let mut commands = vec![];
        for (position, layer) in prepared.layers.into_iter().enumerate() {
            let index = first + position;
            match &layer.kind {
                LayerKind::Vector { shapes } => {
                    selection.extend(
                        shapes
                            .iter()
                            .filter(|shape| shape.visible && !shape.locked && !shape.guide)
                            .map(|shape| (index, shape.id)),
                    );
                }
                LayerKind::Raster { .. } => selection.push((index, RASTER_ID)),
            }
            commands.push(Cmd::AddLayer { index, layer });
        }
        let before = self.doc.import_notes.clone();
        let mut after = before.clone();
        for note in prepared.notes {
            if !after.contains(&note) {
                after.push(note);
            }
        }
        if before != after {
            commands.push(Cmd::SetImportNotes { before, after });
        }
        self.commit_type_edit();
        self.commit(Cmd::Batch(commands));
        selection.retain(|(index, _)| self.doc.layer_editable(*index));
        self.selection = selection;
        self.selected_layer = None;
        self.active_layer = self
            .selection
            .last()
            .map(|(layer, _)| *layer)
            .or(Some(first));
        self.pending_place = None;
        self.op = None;
        self.key_drag = None;
        self.paint_mask = false;
        self.tool = Tool::Select;
        if matches!(self.persona, Persona::Photo | Persona::Pixel) {
            self.persona = Persona::Design;
        }
        self.show_welcome = false;
        self.status = format!("Pasted {} · one Undo", self.selection.len());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn studio() -> Studio {
        let mut studio = Studio::new();
        studio.doc = Document::new("Clipboard destination", 120.0, 80.0, 96.0);
        studio.doc.layers.clear();
        studio.doc.layers.push(Layer::vector("Existing artwork"));
        studio.selection.clear();
        studio.history = History::default();
        studio.active_layer = Some(0);
        studio.view = View {
            scale: 2.25,
            offset: Pt::new(-700.0, 135.0),
        };
        studio.path = Some(PathBuf::from("/tmp/clipboard-destination.oma"));
        studio.need_fit = false;
        studio.show_welcome = false;
        studio
    }

    fn assert_center(bounds: Bounds, center: Pt) {
        let actual = bounds.center();
        assert!(
            (actual.x - center.x).abs() < 0.001,
            "{actual:?} != {center:?}"
        );
        assert!(
            (actual.y - center.y).abs() < 0.001,
            "{actual:?} != {center:?}"
        );
    }

    fn image(w: u32, h: u32) -> ClipboardContent {
        ClipboardContent::Image {
            name: "Pasted image".into(),
            image: RgbaImage {
                w,
                h,
                data: [231, 42, 80, 255].repeat(w as usize * h as usize),
            },
        }
    }

    #[test]
    fn image_paste_preserves_native_size_center_camera_document_and_atomic_history() {
        let mut studio = studio();
        let original = crate::project::encode(&studio.doc).unwrap();
        let center = Pt::new(740.0, -135.0);
        studio
            .insert_clipboard_content(image(300, 160), center)
            .unwrap();
        assert_eq!(studio.doc.layers.len(), 2);
        let layer = &studio.doc.layers[1];
        assert_center(layer.kind.raster_bounds().unwrap(), center);
        let (_, size, rotation) = layer.kind.raster_xform().unwrap();
        assert_eq!(size, Pt::new(300.0, 160.0));
        assert_eq!(rotation, 0.0);
        assert_eq!(studio.doc.size(), Pt::new(120.0, 80.0));
        assert_eq!(studio.view.scale, 2.25);
        assert_eq!(studio.view.offset, Pt::new(-700.0, 135.0));
        assert_eq!(
            studio.path.as_deref(),
            Some(std::path::Path::new("/tmp/clipboard-destination.oma"))
        );
        assert!(!studio.need_fit);
        assert_eq!(studio.selection, vec![(1, RASTER_ID)]);
        assert_eq!(studio.active_layer, Some(1));
        assert_eq!(studio.history.len(), 1);
        let encoded = crate::project::encode(&studio.doc).unwrap();
        let restored = crate::project::decode(&encoded).unwrap();
        assert_center(restored.layers[1].kind.raster_bounds().unwrap(), center);
        assert_eq!(
            restored.layers[1].kind.pixels().unwrap().data,
            [231, 42, 80, 255].repeat(300 * 160)
        );
        studio.undo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), original);
        studio.redo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), encoded);
    }

    #[test]
    fn text_paste_is_a_new_editable_layer_centered_on_its_shaped_glyphs() {
        let mut studio = studio();
        studio.text_px = 28.0;
        let content = "Pasted text\nA second line — café";
        let center = Pt::new(-218.5, 349.25);
        studio
            .insert_clipboard_content(ClipboardContent::Text(content.into()), center)
            .unwrap();
        assert_eq!(studio.doc.layers.len(), 2);
        let shape = &studio.doc.layers[1].kind.shapes().unwrap()[0];
        let Geom::Text(run) = &shape.geom else {
            panic!("Paste must remain editable text");
        };
        assert_eq!(run.content, content);
        assert_eq!(run.px, 28.0);
        assert!(!run.contours.is_empty());
        assert_center(shape.world_bbox(), center);
        assert_eq!(studio.selection, vec![(1, shape.id)]);
        let restored =
            crate::project::decode(&crate::project::encode(&studio.doc).unwrap()).unwrap();
        let restored_shape = &restored.layers[1].kind.shapes().unwrap()[0];
        assert_center(restored_shape.world_bbox(), center);
        assert!(matches!(&restored_shape.geom, Geom::Text(run) if run.content == content));
        studio.undo();
        assert_eq!(studio.doc.layers.len(), 1);
    }

    #[test]
    fn svg_doctype_css_hierarchy_and_clips_survive_centered_vector_paste() {
        let source = r##"<?xml version="1.0"?>
<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">
<svg xmlns="http://www.w3.org/2000/svg" width="100" height="80">
<style>.artwork { fill: #ff0000; }</style>
<defs><clipPath id="clip"><rect x="10" y="20" width="40" height="20"/></clipPath></defs>
<g id="art" clip-path="url(#clip)"><rect class="artwork" x="10" y="20" width="40" height="20"/></g>
<g style="display:none"><rect x="800" y="600" width="100" height="100"/></g>
</svg>"##;
        let mut studio = studio();
        let center = Pt::new(520.0, -120.0);
        studio
            .insert_clipboard_content(ClipboardContent::Svg(source.into()), center)
            .unwrap();
        studio.doc.validate_hierarchy().unwrap();
        let visible = studio.selection.clone();
        assert_eq!(visible.len(), 1);
        let shape = studio.doc.find_shape(visible[0].0, visible[0].1).unwrap();
        assert!(matches!(shape.geom, Geom::Path { .. }));
        assert!(matches!(shape.style.fill, Fill::Solid(color) if color == Rgba::rgb(255, 0, 0)));
        assert_center(shape.world_bbox(), center);
        let group = studio
            .doc
            .layers
            .iter()
            .find(|layer| layer.mask.is_some())
            .unwrap();
        assert_eq!(group.mask_size, Pt::new(100.0, 80.0));
        assert_eq!(group.mask_origin, center - Pt::new(30.0, 30.0));
        let transform = crate::compositor::layer_pixel_transform(group);
        let mut pt = [tiny_skia::Point::from_xy(30.0, 30.0)];
        transform.map_points(&mut pt);
        assert_eq!(Pt::new(pt[0].x, pt[0].y), center);
        let ids: HashSet<_> = studio.doc.layers.iter().map(|layer| layer.id).collect();
        assert_eq!(ids.len(), studio.doc.layers.len());
        let count = studio.doc.layers.len();
        let original = crate::project::encode(&studio.doc).unwrap();
        studio
            .insert_clipboard_content(ClipboardContent::Svg(source.into()), center)
            .unwrap();
        let ids: HashSet<_> = studio.doc.layers.iter().map(|layer| layer.id).collect();
        assert_eq!(ids.len(), studio.doc.layers.len());
        studio.doc.validate_hierarchy().unwrap();
        studio.undo();
        assert_eq!(studio.doc.layers.len(), count);
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), original);
        let restored = crate::project::decode(&original).unwrap();
        restored.validate_hierarchy().unwrap();
        assert_eq!(
            restored
                .layers
                .iter()
                .filter(|layer| layer.mask.is_some())
                .count(),
            1
        );
    }

    struct TestFolder(PathBuf);
    impl TestFolder {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "omadesign-clipboard-files-{}-{}",
                std::process::id(),
                next_id()
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestFolder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn copied_image_and_svg_files_are_new_layers_in_one_undo() {
        let folder = TestFolder::new();
        let png = folder.0.join("Copied image.png");
        image::RgbaImage::from_pixel(22, 14, image::Rgba([22, 40, 200, 255]))
            .save(&png)
            .unwrap();
        let svg = folder.0.join("Copied vector.svg");
        std::fs::write(&svg, r#"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40"><rect x="10" y="5" width="60" height="30" fill="blue"/></svg>"#).unwrap();
        let mut studio = studio();
        let original = crate::project::encode(&studio.doc).unwrap();
        let center = Pt::new(340.0, -60.0);
        studio
            .insert_clipboard_content(ClipboardContent::Files(vec![png, svg]), center)
            .unwrap();
        assert_eq!(studio.history.len(), 1);
        assert_eq!(studio.doc.layers.len(), 3);
        assert_eq!(studio.selection.len(), 2);
        assert_center(studio.doc.layers[1].kind.raster_bounds().unwrap(), center);
        assert_center(
            studio.doc.layers[2].kind.shapes().unwrap()[0].world_bbox(),
            center,
        );
        assert_eq!(studio.doc.layers[1].name, "Copied image.png");
        assert_eq!(studio.doc.layers[2].name, "Copied vector.svg");
        studio.undo();
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), original);
    }

    #[test]
    fn invalid_file_in_multi_paste_and_invalid_pixels_leave_document_unchanged() {
        let folder = TestFolder::new();
        let good = folder.0.join("good.png");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([255; 4]))
            .save(&good)
            .unwrap();
        let bad = folder.0.join("broken.svg");
        std::fs::write(&bad, "<svg invalid").unwrap();
        let mut studio = studio();
        let original = crate::project::encode(&studio.doc).unwrap();
        assert!(
            studio
                .insert_clipboard_content(ClipboardContent::Files(vec![good, bad]), Pt::ZERO)
                .is_err()
        );
        assert!(
            studio
                .insert_clipboard_content(
                    ClipboardContent::Image {
                        name: "Broken".into(),
                        image: RgbaImage {
                            w: 4,
                            h: 4,
                            data: vec![0; 8]
                        }
                    },
                    Pt::ZERO
                )
                .is_err()
        );
        assert_eq!(crate::project::encode(&studio.doc).unwrap(), original);
        assert_eq!(studio.history.len(), 0);
    }
}
