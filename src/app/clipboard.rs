//! Native paste is asynchronous and tied to the destination tab and viewport.
use super::*;
use crate::clipboard::ClipboardContent;
use std::sync::mpsc::{self, Receiver, TryRecvError};

pub(super) struct PasteJob {
    receiver: Receiver<Result<PasteContents, String>>,
    owner: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn copy(studio: &mut Studio) -> String {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| studio.copy_selection(ui.ctx()));
        output.textures_delta.clear();
        output
            .platform_output
            .commands
            .into_iter()
            .find_map(|command| match command {
                egui::OutputCommand::CopyText(text) => Some(text),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn cross_window_rasters_and_shapes_keep_positions_with_one_undo() {
        let mut source = Studio::new();
        source.place_text(Pt::new(76., 125.));
        source.commit_type_edit();
        let shape = source.selection[0];
        let bounds = source.doc.find_shape(shape.0, shape.1).unwrap().geom.bbox();
        let raster = Layer::placed_raster(
            "Image",
            crate::document::Pixels::new(3, 2),
            Pt::new(98., 142.),
            Pt::new(60., 40.),
        );
        let index = source.doc.layers.len();
        source.doc.layers.push(raster);
        source.selection.push((index, RASTER_ID));
        let payload = copy(&mut source);
        let mut target = Studio::new();
        let history = target.history.len();
        let layers = target.doc.layers.len();
        target.paste_clipboard(Some(&payload));
        assert_eq!(target.history.len(), history + 1);
        assert_eq!(target.selection.len(), 2);
        let pasted = target.selection[0];
        assert_eq!(
            target
                .doc
                .find_shape(pasted.0, pasted.1)
                .unwrap()
                .geom
                .bbox(),
            bounds
        );
        assert_ne!(pasted.1, shape.1);
        let raster = &target.doc.layers[target.selection[1].0];
        assert_eq!(raster.kind.raster_xform().unwrap().0, Pt::new(98., 142.));
        assert_eq!(raster.kind.pixels().unwrap().w, 3);
        target.undo();
        assert_eq!(target.doc.layers.len(), layers);
        assert!(target.doc.find_shape(pasted.0, pasted.1).is_none());
        target.redo();
        assert!(target.doc.find_shape(pasted.0, pasted.1).is_some());
    }

    #[test]
    fn external_text_replaces_stale_internal_clipboard_and_centers_after_pan_zoom() {
        let mut studio = Studio::new();
        studio.place_text(Pt::new(12., 24.));
        studio.commit_type_edit();
        copy(&mut studio);
        studio.canvas_rect = Some(Rect::from_min_size(
            Pos2::new(80., 40.),
            egui::vec2(1000., 600.),
        ));
        studio.view.scale = 2.;
        studio.view.offset = Pt::new(-150., 70.);
        let center = studio.clipboard_center();
        assert_eq!(center, Pt::new(325., 115.));
        let count = studio.doc.layers.len();
        studio.paste_clipboard(Some("Outside text\nsecond line"));
        assert_eq!(studio.doc.layers.len(), count + 1);
        let (li, id) = studio.selection[0];
        let shape = studio.doc.find_shape(li, id).unwrap();
        assert!((shape.geom.bbox().center() - center).length() < 0.01);
        let Geom::Text(text) = &shape.geom else {
            panic!("editable text expected")
        };
        assert_eq!(text.content, "Outside text\nsecond line");
    }

    #[test]
    fn invalid_internal_payload_never_reuses_stale_objects() {
        let mut studio = Studio::new();
        studio.place_text(Pt::ZERO);
        studio.commit_type_edit();
        copy(&mut studio);
        let history = studio.history.len();
        studio.paste_clipboard(Some("omadesign-shapes:broken"));
        assert_eq!(studio.history.len(), history);
        assert!(studio.status.starts_with("Invalid copied objects:"));
    }

    #[test]
    fn delayed_paste_does_not_modify_another_tab() {
        let mut studio = Studio::new();
        let (tx, receiver) = mpsc::channel();
        studio.clipboard_jobs.push(PasteJob {
            receiver,
            owner: "different-tab".into(),
        });
        tx.send(Ok(PasteContents::Internal("omadesign-shapes:[]".into())))
            .unwrap();
        let history = studio.history.len();
        studio.poll_clipboard_jobs(&egui::Context::default());
        assert_eq!(studio.history.len(), history);
        assert!(studio.status.contains("destination tab changed"));
    }

    #[test]
    fn copied_frame_preserves_descendants_positions_and_remaps_parent_ids() {
        let mut source = Studio::new();
        let mut frame = crate::layout::make_frame(Pt::new(42.0, 64.0), Pt::new(220.0, 180.0));
        frame.name = "Outer frame".into();
        let mut nested = crate::layout::make_frame(Pt::new(66.0, 90.0), Pt::new(110.0, 80.0));
        nested.name = "Inner frame".into();
        nested.layout.parent = Some(frame.id);
        let mut child = Shape::new(
            Geom::Rect {
                origin: Pt::new(75.0, 104.0),
                size: Pt::new(32.0, 20.0),
                radius: 0.0,
            },
            Style::default(),
        );
        child.name = "Nested artwork".into();
        child.layout.parent = Some(nested.id);
        let original_shapes = vec![frame.clone(), nested, child];
        source.doc.layers[1]
            .kind
            .shapes_mut()
            .unwrap()
            .extend(original_shapes.clone());
        source.selection = vec![(1, frame.id)];
        let payload = copy(&mut source);

        let mut target = Studio::new();
        let original = crate::project::encode(&target.doc).unwrap();
        target.paste_clipboard(Some(&payload));
        assert_eq!(target.history.len(), 1);
        assert_eq!(target.selection.len(), 1);
        let (layer, frame_id) = target.selection[0];
        let pasted = target.doc.layers[layer].kind.shapes().unwrap();
        assert_eq!(pasted.len(), 3);
        for (pasted, original) in pasted.iter().zip(&original_shapes) {
            assert_ne!(pasted.id, original.id);
            assert_eq!(pasted.name, original.name);
            assert_eq!(pasted.world_bbox(), original.world_bbox());
        }
        assert_eq!(pasted[0].id, frame_id);
        assert_eq!(pasted[0].layout.parent, None);
        assert_eq!(pasted[1].layout.parent, Some(frame_id));
        assert_eq!(pasted[2].layout.parent, Some(pasted[1].id));
        assert_eq!(
            crate::layout::descendants(&target.doc, layer, frame_id).len(),
            2
        );
        let encoded = crate::project::encode(&target.doc).unwrap();
        target.undo();
        assert_eq!(crate::project::encode(&target.doc).unwrap(), original);
        target.redo();
        assert_eq!(crate::project::encode(&target.doc).unwrap(), encoded);
    }

    #[test]
    fn internal_vector_payload_does_not_repeat_a_previously_copied_bitmap() {
        let mut source = Studio::new();
        source.place_text(Pt::new(90.0, 130.0));
        source.commit_type_edit();
        let payload = copy(&mut source);
        let mut target = Studio::new();
        target
            .insert_clipboard_content(
                ClipboardContent::Image {
                    name: "External screenshot".into(),
                    image: RgbaImage {
                        w: 3,
                        h: 2,
                        data: [80, 100, 120, 255].repeat(6),
                    },
                },
                Pt::new(400.0, 250.0),
            )
            .unwrap();
        copy(&mut target);
        assert_eq!(target.clipboard_rasters.len(), 1);
        let layer_count = target.doc.layers.len();
        let history = target.history.len();
        target.paste_clipboard(Some(&payload));
        assert_eq!(target.doc.layers.len(), layer_count);
        assert_eq!(target.history.len(), history + 1);
        assert_eq!(target.selection.len(), 1);
        let (layer, id) = target.selection[0];
        assert_ne!(id, RASTER_ID);
        assert!(matches!(
            target.doc.find_shape(layer, id).unwrap().geom,
            Geom::Text(_)
        ));
        target.undo();
        assert!(target.doc.find_shape(layer, id).is_none());
        assert_eq!(target.doc.layers.len(), layer_count);
    }
}

enum PasteContents {
    Internal(String),
    External(super::clipboard_insert::PreparedClipboard),
}

impl Studio {
    pub(super) const CLIP_PREFIX: &'static str = "omadesign-shapes:";
    const OBJECT_CLIP_PREFIX: &'static str = "omadesign-objects:";

    pub fn copy_selection(&mut self, ctx: &egui::Context) {
        let mut ids: HashSet<_> = self.selection.iter().copied().collect();
        for &(li, id) in &self.selection {
            ids.extend(
                crate::layout::descendants(&self.doc, li, id)
                    .into_iter()
                    .map(|id| (li, id)),
            );
        }
        let mut shapes: Vec<Shape> = self
            .doc
            .layers
            .iter()
            .enumerate()
            .flat_map(|(li, layer)| {
                let ids = &ids;
                layer
                    .kind
                    .shapes()
                    .into_iter()
                    .flatten()
                    .filter(move |s| ids.contains(&(li, s.id)))
                    .cloned()
            })
            .collect();
        let copied: HashSet<_> = shapes.iter().map(|s| s.id).collect();
        for shape in &mut shapes {
            shape.layout.parent = shape.layout.parent.filter(|id| copied.contains(id));
        }
        let rasters: Vec<_> = self
            .selection
            .iter()
            .filter_map(|(li, id)| {
                (*id == RASTER_ID)
                    .then(|| self.doc.layers.get(*li).cloned())
                    .flatten()
            })
            .collect();
        if shapes.is_empty() && rasters.is_empty() {
            self.status = "nothing to copy".into();
            return;
        }
        let payload = if rasters.is_empty() {
            serde_json::to_string(&shapes)
                .map(|s| format!("{}{s}", Self::CLIP_PREFIX))
                .map_err(|e| e.to_string())
        } else {
            let mut doc = Document::new("Clipboard", self.doc.width, self.doc.height, self.doc.dpi);
            doc.layers.clear();
            if !shapes.is_empty() {
                let mut layer = Layer::vector("Copied objects");
                layer.kind = LayerKind::Vector {
                    shapes: shapes.clone(),
                };
                doc.layers.push(layer);
            }
            doc.layers.extend(rasters.iter().cloned().map(|mut l| {
                l.parent = None;
                l
            }));
            crate::project::encode(&doc).map(|s| format!("{}{s}", Self::OBJECT_CLIP_PREFIX))
        };
        match payload {
            Ok(payload) => {
                self.clipboard = shapes;
                self.clipboard_rasters = rasters;
                ctx.copy_text(payload);
                let n = self.clipboard.len() + self.clipboard_rasters.len();
                self.status = format!("copied {n} {}", if n == 1 { "object" } else { "objects" });
            }
            Err(error) => self.status = format!("Could not copy: {error}"),
        }
    }

    pub fn cut_selection(&mut self, ctx: &egui::Context) {
        self.copy_selection(ctx);
        if self.status.starts_with("copied ") {
            if !self.delete_focused_edit() {
                self.delete_objects();
            }
            self.status = "cut".into();
        }
    }

    fn internal_clipboard(text: &str) -> bool {
        text.starts_with(Self::CLIP_PREFIX) || text.starts_with(Self::OBJECT_CLIP_PREFIX)
    }

    pub fn clipboard_center(&self) -> Pt {
        self.canvas_rect
            .filter(|r| r.is_positive())
            .map_or(self.doc.size() * 0.5, |r| {
                self.view
                    .to_world(Pt::new(r.width() * 0.5, r.height() * 0.5))
            })
    }

    pub fn request_clipboard_paste(&mut self, ctx: &egui::Context, payload: Option<&str>) {
        if let Some(text) = payload.filter(|p| Self::internal_clipboard(p)) {
            self.paste_clipboard(Some(text));
            return;
        }
        if self.clipboard_jobs.len() >= 4 {
            self.status = "Wait for the current paste to finish".into();
            return;
        }
        let (tx, receiver) = mpsc::channel();
        let center = self.clipboard_center();
        let text = self.type_defaults();
        let style = self.style.clone();
        let fallback = payload.map(str::to_owned);
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = crate::clipboard::read()
                .or_else(|error| {
                    fallback
                        .as_deref()
                        .map(crate::clipboard::parse_text)
                        .unwrap_or(Err(error))
                })
                .and_then(|content| {
                    let content = match (content, fallback.as_deref()) {
                        (ClipboardContent::Empty, Some(text)) => {
                            crate::clipboard::parse_text(text)?
                        }
                        (content, _) => content,
                    };
                    if let ClipboardContent::Text(ref value) = content
                        && Self::internal_clipboard(value)
                    {
                        return Ok(PasteContents::Internal(value.clone()));
                    }
                    super::clipboard_insert::prepare_clipboard_content(content, text, style, center)
                        .map(PasteContents::External)
                });
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        self.clipboard_jobs.push(PasteJob {
            receiver,
            owner: self.swap_id.clone(),
        });
        self.status = "Pasting…".into();
        ctx.request_repaint();
    }

    pub(super) fn poll_clipboard_jobs(&mut self, ctx: &egui::Context) {
        while let Some(job) = self.clipboard_jobs.first() {
            let result = match job.receiver.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    Err("The clipboard worker stopped unexpectedly".into())
                }
            };
            let job = self.clipboard_jobs.remove(0);
            if job.owner != self.swap_id {
                self.status =
                    "The destination tab changed. Paste again in the intended tab.".into();
                continue;
            }
            match result {
                Ok(PasteContents::Internal(text)) => self.paste_clipboard(Some(&text)),
                Ok(PasteContents::External(prepared)) => {
                    if let Err(error) = self.insert_prepared_clipboard(prepared) {
                        self.status = error;
                    }
                }
                Err(error) => self.status = error,
            }
        }
        if !self.clipboard_jobs.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    /// Explicit payload entry point also keeps older Omadesign clipboard data compatible.
    pub fn paste_clipboard(&mut self, payload: Option<&str>) {
        let parsed = match payload {
            Some(text) if text.starts_with(Self::CLIP_PREFIX) => {
                serde_json::from_str::<Vec<Shape>>(&text[Self::CLIP_PREFIX.len()..])
                    .map(|shapes| (shapes, vec![]))
                    .map_err(|e| format!("Invalid copied objects: {e}"))
            }
            Some(text) if text.starts_with(Self::OBJECT_CLIP_PREFIX) => {
                crate::project::decode(&text[Self::OBJECT_CLIP_PREFIX.len()..]).map(|doc| {
                    let mut shapes = Vec::new();
                    let mut rasters = Vec::new();
                    for layer in doc.layers {
                        if let Some(items) = layer.kind.shapes() {
                            shapes.extend(items.iter().cloned());
                        } else {
                            rasters.push(layer);
                        }
                    }
                    (shapes, rasters)
                })
            }
            Some(text) => {
                let result = crate::clipboard::parse_text(text).and_then(|content| {
                    self.insert_clipboard_content(content, self.clipboard_center())
                });
                if let Err(error) = result {
                    self.status = error;
                }
                return;
            }
            None => Ok((self.clipboard.clone(), self.clipboard_rasters.clone())),
        };
        let (shapes, rasters) = match parsed {
            Ok(value) => value,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        if shapes.is_empty() && rasters.is_empty() {
            self.status = "clipboard is empty".into();
            return;
        }
        let mut commands = Vec::new();
        let mut selection = Vec::new();
        let mut next_layer = self.doc.layers.len();
        if !shapes.is_empty() {
            let li = self.vector_target().unwrap_or_else(|| {
                let index = next_layer;
                next_layer += 1;
                commands.push(Cmd::AddLayer {
                    index,
                    layer: Layer::vector("Copied objects"),
                });
                index
            });
            let remap: HashMap<_, _> = shapes
                .iter()
                .map(|s| (s.id, crate::document::next_id()))
                .collect();
            for mut shape in shapes {
                shape.id = remap[&shape.id];
                shape.layout.parent = shape.layout.parent.and_then(|id| remap.get(&id).copied());
                crate::text::fill_contours(&mut shape.geom);
                if shape.layout.parent.is_none() {
                    selection.push((li, shape.id));
                }
                commands.push(Cmd::AddShape { layer: li, shape });
            }
        }
        for mut layer in rasters {
            layer.id = crate::document::next_id();
            layer.parent = None;
            selection.push((next_layer, RASTER_ID));
            commands.push(Cmd::AddLayer {
                index: next_layer,
                layer,
            });
            next_layer += 1;
        }
        self.commit_type_edit();
        self.commit(Cmd::Batch(commands));
        self.active_layer = selection.first().map(|s| s.0);
        self.selected_layer = None;
        self.selection = selection;
        self.pending_place = None;
        self.op = None;
        self.key_drag = None;
        self.tool = Tool::Select;
        self.show_welcome = false;
        self.status = format!("pasted {}", self.selection.len());
    }
}
