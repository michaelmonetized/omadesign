//! Direct manipulation of selection coverage; image pixels are never transformed.
use super::{Studio, masking::SelectionSpace};
use crate::{
    deform::{Cage, Mode},
    geom::{Anchor, Bounds, Pt},
};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct SelectionPath {
    pub anchors: Vec<Anchor>,
    pub space: SelectionSpace,
    pub generation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ModeKind {
    Transform,
    Reshape(Mode),
    Bezier,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Geometry {
    Cage(Cage),
    Path { anchors: Vec<Anchor>, closed: bool },
}
#[derive(Clone)]
pub struct Drag {
    pub handle: usize,
    pub point: Pt,
    pub before: Geometry,
}
pub type Job = Arc<Mutex<Option<Option<Vec<u8>>>>>;
pub struct Editor {
    pub document: String,
    pub generation: u64,
    pub space: SelectionSpace,
    pub mode: ModeKind,
    pub geometry: Geometry,
    pub source: Arc<Vec<u8>>,
    pub had_selection: bool,
    pub source_path: Option<Vec<Anchor>>,
    pub drag: Option<Drag>,
    pub history: Vec<Geometry>,
    pub future: Vec<Geometry>,
    pub rendered: Option<Geometry>,
    pub job: Option<(Geometry, Job)>,
    pub finish: bool,
}
impl Editor {
    pub fn pending(&self) -> bool {
        self.rendered.as_ref() != Some(&self.geometry) && self.closed()
    }
    pub fn closed(&self) -> bool {
        !matches!(&self.geometry, Geometry::Path { closed: false, .. })
    }
    pub fn record(&mut self, before: Geometry) {
        if before != self.geometry {
            self.history.push(before);
            self.future.clear();
            if self.history.len() > 100 {
                self.history.remove(0);
            }
        }
    }
    pub fn path(&self) -> Option<Vec<Anchor>> {
        match &self.geometry {
            Geometry::Path {
                anchors,
                closed: true,
            } => Some(anchors.clone()),
            Geometry::Cage(cage) => mapped_path(self.source_path.as_deref()?, cage),
            _ => None,
        }
    }
}

impl Studio {
    /// Editable nodes for the current selection, if it was made with the Bézier lasso.
    pub fn pixel_selection_path(&self) -> Option<&[Anchor]> {
        self.pixel_path
            .as_ref()
            .filter(|p| p.generation == self.pixel_sel_gen)
            .map(|p| p.anchors.as_slice())
    }

    pub fn begin_new_bezier(&mut self) {
        self.begin_pixel_edit(ModeKind::Bezier);
        if let Some(edit) = &mut self.pixel_edit {
            edit.geometry = Geometry::Path {
                anchors: vec![],
                closed: false,
            };
            edit.rendered = Some(edit.geometry.clone());
        }
    }
    pub fn begin_pixel_edit(&mut self, mode: ModeKind) {
        self.end_pixel_stroke(false);
        let Some(space) = self
            .pixel_selection_space()
            .or_else(|| self.selection_target_space())
        else {
            return;
        };
        if u64::from(space.w) * u64::from(space.h) > 64_000_000 {
            self.status = "Selection is too large to edit".into();
            return;
        }
        let source = self
            .pixel_sel
            .clone()
            .unwrap_or_else(|| vec![0; space.w as usize * space.h as usize]);
        let path = self
            .pixel_path
            .as_ref()
            .filter(|p| p.generation == self.pixel_sel_gen && p.space == space)
            .map(|p| p.anchors.clone());
        let geometry = if mode == ModeKind::Bezier {
            Geometry::Path {
                closed: path.is_some(),
                anchors: path.clone().unwrap_or_default(),
            }
        } else {
            let Some((x0, y0, x1, y1)) = crate::paint::selection_bounds(&source, space.w, space.h)
            else {
                return;
            };
            let bounds = Bounds::from_min_size(
                Pt::new(x0 as f32, y0 as f32),
                Pt::new((x1 - x0) as f32, (y1 - y0) as f32),
            );
            let cage_mode = if let ModeKind::Reshape(m) = mode {
                m
            } else {
                Mode::Distort
            };
            let Some(cage) = Cage::new(cage_mode, bounds) else {
                return;
            };
            Geometry::Cage(cage)
        };
        self.pixel_edit = Some(Editor {
            document: self.swap_id.clone(),
            generation: self.pixel_sel_gen,
            space,
            mode,
            rendered: Some(geometry.clone()),
            geometry,
            source: Arc::new(source),
            had_selection: self.pixel_sel.is_some(),
            source_path: path,
            drag: None,
            history: vec![],
            future: vec![],
            job: None,
            finish: false,
        });
        self.status = "Drag selection handles · Enter finishes · Esc cancels".into();
    }

    pub fn pixel_edit_key(&mut self, key: eframe::egui::Key) -> bool {
        use eframe::egui::Key;
        let Some(mut edit) = self.pixel_edit.take() else {
            return false;
        };
        match key {
            Key::Escape => {
                if let Some(drag) = edit.drag.take() {
                    edit.geometry = drag.before;
                    self.pixel_edit = Some(edit);
                } else {
                    if self.tool == crate::tools::Tool::BezierLasso {
                        self.tool = crate::tools::Tool::Marquee;
                    }
                    if edit.had_selection {
                        self.replace_pixel_selection((*edit.source).clone(), edit.space);
                    } else {
                        self.set_pixel_sel(None);
                    }
                    self.pixel_path = edit.source_path.map(|anchors| SelectionPath {
                        anchors,
                        space: edit.space,
                        generation: self.pixel_sel_gen,
                    });
                }
                true
            }
            Key::Enter => {
                let before = edit.geometry.clone();
                if let Geometry::Path { anchors, closed } = &mut edit.geometry {
                    if !*closed && anchors.len() >= 3 {
                        *closed = true;
                    } else if *closed {
                        edit.finish = true;
                    }
                } else {
                    edit.finish = true;
                }
                edit.record(before);
                self.pixel_edit = Some(edit);
                true
            }
            _ => {
                self.pixel_edit = Some(edit);
                false
            }
        }
    }

    pub fn pixel_edit_history(&mut self, redo: bool) -> bool {
        let Some(edit) = &mut self.pixel_edit else {
            return false;
        };
        if let Some(drag) = edit.drag.take() {
            edit.geometry = drag.before;
            return true;
        }
        let from = if redo {
            &mut edit.future
        } else {
            &mut edit.history
        };
        if let Some(state) = from.pop() {
            let previous = std::mem::replace(&mut edit.geometry, state);
            if redo {
                edit.history.push(previous);
            } else {
                edit.future.push(previous);
            }
        }
        true
    }

    pub fn poll_pixel_edit(&mut self, ctx: &eframe::egui::Context) {
        let Some(mut edit) = self.pixel_edit.take() else {
            return;
        };
        if edit.document != self.swap_id
            || edit.generation != self.pixel_sel_gen
            || self.persona != crate::tools::Persona::Pixel
        {
            return;
        }
        if !edit.closed()
            && edit
                .rendered
                .as_ref()
                .is_some_and(|g| matches!(g, Geometry::Path { closed: true, .. }))
        {
            if edit.had_selection {
                self.replace_pixel_selection((*edit.source).clone(), edit.space);
            } else {
                self.set_pixel_sel(None);
            }
            edit.generation = self.pixel_sel_gen;
            self.pixel_path = edit.source_path.clone().map(|anchors| SelectionPath {
                anchors,
                space: edit.space,
                generation: self.pixel_sel_gen,
            });
            edit.rendered = Some(edit.geometry.clone());
        }
        let result = edit
            .job
            .as_ref()
            .and_then(|(_, job)| job.lock().ok()?.take());
        if let Some(result) = result {
            let (geometry, _) = edit.job.take().unwrap();
            if geometry == edit.geometry {
                if let Some(mask) = result {
                    self.replace_pixel_selection(mask, edit.space);
                    edit.generation = self.pixel_sel_gen;
                    self.pixel_path = edit.path().map(|anchors| SelectionPath {
                        anchors,
                        space: edit.space,
                        generation: self.pixel_sel_gen,
                    });
                    edit.rendered = Some(geometry);
                } else {
                    self.status = "Selection transform is invalid".into();
                    edit.finish = false;
                    edit.rendered = Some(geometry);
                }
            }
        }
        if edit.pending() && edit.job.is_none() {
            let source = edit.source.clone();
            let space = edit.space;
            let geometry = edit.geometry.clone();
            let path = edit.source_path.clone();
            let job = Arc::new(Mutex::new(None));
            let send = job.clone();
            let repaint = ctx.clone();
            edit.job = Some((geometry.clone(), job));
            std::thread::spawn(move || {
                let output = render(&source, space, &geometry, path.as_deref());
                *send.lock().unwrap() = Some(output);
                repaint.request_repaint();
            });
        }
        if edit.finish && !edit.pending() && edit.job.is_none() {
            if self.tool == crate::tools::Tool::BezierLasso {
                self.tool = crate::tools::Tool::Marquee;
            }
            return;
        }
        if edit.pending() || edit.job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        self.pixel_edit = Some(edit);
    }
}

fn mapped_path(anchors: &[Anchor], cage: &Cage) -> Option<Vec<Anchor>> {
    let mapper = cage.mapper()?;
    anchors
        .iter()
        .map(|a| {
            let mut b = a.clone();
            b.pt = mapper.map(a.pt)?;
            b.h_in = mapper.map(a.pt + a.h_in)? - b.pt;
            b.h_out = mapper.map(a.pt + a.h_out)? - b.pt;
            Some(b)
        })
        .collect()
}
pub fn path_mask(anchors: &[Anchor], space: SelectionSpace) -> Option<Vec<u8>> {
    if anchors.len() < 3 || anchors.len() > 4096 {
        return None;
    }
    let mut builder = tiny_skia::PathBuilder::new();
    builder.move_to(anchors[0].pt.x, anchors[0].pt.y);
    for i in 0..anchors.len() {
        let a = &anchors[i];
        let b = &anchors[(i + 1) % anchors.len()];
        let c1 = a.pt + a.h_out;
        let c2 = b.pt + b.h_in;
        builder.cubic_to(c1.x, c1.y, c2.x, c2.y, b.pt.x, b.pt.y);
    }
    builder.close();
    let path = builder.finish()?;
    let mut mask = tiny_skia::Mask::new(space.w, space.h)?;
    mask.fill_path(
        &path,
        tiny_skia::FillRule::EvenOdd,
        true,
        tiny_skia::Transform::identity(),
    );
    Some(mask.data().to_vec())
}
fn render(
    source: &[u8],
    space: SelectionSpace,
    geometry: &Geometry,
    path: Option<&[Anchor]>,
) -> Option<Vec<u8>> {
    match geometry {
        Geometry::Path {
            anchors,
            closed: true,
        } => path_mask(anchors, space),
        Geometry::Path { .. } => None,
        Geometry::Cage(cage) => {
            if let Some(path) = path {
                return path_mask(&mapped_path(path, cage)?, space);
            }
            let mapper = cage.mapper()?;
            let points = mapper
                .grid_lines(16, 16)
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            let first = *points.first()?;
            let mut bounds = Bounds::from_min_size(first, Pt::ZERO);
            for point in points {
                bounds.union_pt(point);
            }
            let mut output = vec![0; source.len()];
            for y in (bounds.min.y.floor().max(0.0) as u32)
                ..(bounds.max.y.ceil().max(0.0) as u32).min(space.h)
            {
                for x in (bounds.min.x.floor().max(0.0) as u32)
                    ..(bounds.max.x.ceil().max(0.0) as u32).min(space.w)
                {
                    let Some(p) = mapper.unmap(Pt::new(x as f32 + 0.5, y as f32 + 0.5), bounds)
                    else {
                        continue;
                    };
                    if p.x < cage.bounds.min.x
                        || p.x > cage.bounds.max.x
                        || p.y < cage.bounds.min.y
                        || p.y > cage.bounds.max.y
                    {
                        continue;
                    }
                    let px = p.x - 0.5;
                    let py = p.y - 0.5;
                    let ix = px.floor() as i32;
                    let iy = py.floor() as i32;
                    let mut coverage = 0.0;
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let sx = ix + dx;
                            let sy = iy + dy;
                            if sx >= 0 && sy >= 0 && sx < space.w as i32 && sy < space.h as i32 {
                                let wx = if dx == 0 {
                                    1.0 - (px - ix as f32)
                                } else {
                                    px - ix as f32
                                };
                                let wy = if dy == 0 {
                                    1.0 - (py - iy as f32)
                                } else {
                                    py - iy as f32
                                };
                                coverage += source[sy as usize * space.w as usize + sx as usize]
                                    as f32
                                    * wx
                                    * wy;
                            }
                        }
                    }
                    output[y as usize * space.w as usize + x as usize] = coverage.round() as u8;
                }
            }
            Some(output)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn space() -> SelectionSpace {
        SelectionSpace {
            w: 64,
            h: 48,
            transform: tiny_skia::Transform::identity(),
        }
    }
    fn fixture() -> Studio {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.doc = crate::document::Document::new("Selection", 64., 48., 96.);
        s.doc.layers = vec![crate::document::Layer::raster("Pixels", 64, 48)];
        s.active_layer = Some(0);
        s.persona = crate::tools::Persona::Pixel;
        s.replace_pixel_selection(
            crate::paint::fill_rect_mask(64, 48, 10., 10., 30., 30.),
            space(),
        );
        s
    }
    fn settle(s: &mut Studio) {
        let ctx = eframe::egui::Context::default();
        for _ in 0..200 {
            s.poll_pixel_edit(&ctx);
            if s.pixel_edit
                .as_ref()
                .is_none_or(|e| !e.pending() && e.job.is_none())
            {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        panic!("Mask worker timed out");
    }
    #[test]
    fn selection_handles_translate_resize_and_cancel_without_touching_pixels() {
        let mut s = fixture();
        let pixels = crate::project::encode(&s.doc).unwrap();
        let original = s.pixel_sel.clone();
        s.begin_pixel_edit(ModeKind::Transform);
        let Geometry::Cage(cage) = &mut s.pixel_edit.as_mut().unwrap().geometry else {
            panic!()
        };
        *cage = cage.translated(Pt::new(7., 3.)).unwrap();
        settle(&mut s);
        assert_eq!(
            crate::paint::selection_bounds(s.pixel_sel.as_ref().unwrap(), 64, 48),
            Some((17, 13, 37, 33))
        );
        let before = s.pixel_edit.as_ref().unwrap().geometry.clone();
        let Geometry::Cage(cage) = &mut s.pixel_edit.as_mut().unwrap().geometry else {
            panic!()
        };
        *cage = cage.resized(2, Pt::new(10., 5.), false).unwrap();
        s.pixel_edit.as_mut().unwrap().record(before);
        settle(&mut s);
        assert_eq!(
            crate::paint::selection_bounds(s.pixel_sel.as_ref().unwrap(), 64, 48),
            Some((17, 13, 47, 38))
        );
        s.pixel_edit_history(false);
        settle(&mut s);
        assert_eq!(
            crate::paint::selection_bounds(s.pixel_sel.as_ref().unwrap(), 64, 48),
            Some((17, 13, 37, 33))
        );
        s.pixel_edit_key(eframe::egui::Key::Escape);
        assert_eq!(s.pixel_sel, original);
        assert_eq!(crate::project::encode(&s.doc).unwrap(), pixels);
    }
    #[test]
    fn every_reshape_mode_inverse_maps_and_preserves_holes() {
        let mut mask = crate::paint::fill_rect_mask(64, 48, 10., 10., 40., 38.);
        for y in 18..27 {
            for x in 18..27 {
                mask[y * 64 + x] = 0;
            }
        }
        let bounds = Bounds::from_min_size(Pt::new(10., 10.), Pt::new(30., 28.));
        for mode in Mode::ALL {
            let cage = Cage::new(mode, bounds)
                .unwrap()
                .dragged(0, Pt::new(4., 2.))
                .unwrap();
            let mapper = cage.mapper().unwrap();
            let p = Pt::new(24., 24.);
            let mapped = mapper.map(p).unwrap();
            assert!(
                (mapper.unmap(mapped, bounds).unwrap() - p).length() < 0.02,
                "{mode:?}"
            );
            let result = render(&mask, space(), &Geometry::Cage(cage), None).unwrap();
            assert_eq!(
                result[mapped.y as usize * 64 + mapped.x as usize],
                0,
                "hole in {mode:?}"
            );
            assert_ne!(result, mask, "{mode:?}");
        }
    }
    #[test]
    fn bezier_finish_survives_tool_and_tab_switches_and_stale_workers_cannot_publish() {
        let mut s = fixture();
        s.tool = crate::tools::Tool::BezierLasso;
        s.begin_new_bezier();
        s.pixel_edit.as_mut().unwrap().geometry = Geometry::Path {
            anchors: vec![
                Anchor::corner(Pt::new(8., 8.)),
                Anchor::corner(Pt::new(40., 8.)),
                Anchor::corner(Pt::new(24., 36.)),
            ],
            closed: true,
        };
        settle(&mut s);
        assert!(s.pixel_path.is_some());
        s.pixel_edit_key(eframe::egui::Key::Enter);
        settle(&mut s);
        assert!(s.pixel_edit.is_none());
        assert_eq!(s.tool, crate::tools::Tool::Marquee);
        s.begin_pixel_edit(ModeKind::Bezier);
        assert!(
            matches!(&s.pixel_edit.as_ref().unwrap().geometry,Geometry::Path{anchors,closed:true} if anchors.len()==3)
        );
        s.pixel_edit.as_mut().unwrap().geometry = Geometry::Path {
            anchors: vec![Anchor::corner(Pt::new(1., 1.)); 3],
            closed: true,
        };
        s.poll_pixel_edit(&eframe::egui::Context::default());
        s.set_pixel_sel(None);
        settle(&mut s);
        assert!(s.pixel_sel.is_none());
        assert!(s.pixel_edit.is_none());
    }
    #[test]
    fn cancelled_empty_lasso_and_tab_roundtrip_preserve_selection_semantics() {
        let mut s = fixture();
        s.set_pixel_sel(None);
        s.tool = crate::tools::Tool::BezierLasso;
        s.begin_new_bezier();
        s.pixel_edit_key(eframe::egui::Key::Escape);
        assert!(
            s.pixel_sel.is_none(),
            "Cancelling must not replace unrestricted painting with an empty mask"
        );
        s.tool = crate::tools::Tool::BezierLasso;
        s.begin_new_bezier();
        s.pixel_edit.as_mut().unwrap().geometry = Geometry::Path {
            anchors: vec![
                Anchor::corner(Pt::new(8., 8.)),
                Anchor::corner(Pt::new(40., 8.)),
                Anchor::corner(Pt::new(24., 36.)),
            ],
            closed: true,
        };
        settle(&mut s);
        let mask = s.pixel_sel.clone();
        s.ensure_tabs();
        s.new_tab();
        assert!(s.pixel_edit.is_none());
        s.switch_tab(0);
        assert_eq!(s.pixel_sel, mask);
        assert_eq!(s.pixel_path.as_ref().unwrap().anchors.len(), 3);
        assert_eq!(s.pixel_edit.as_ref().unwrap().generation, s.pixel_sel_gen);
        s.pixel_edit_key(eframe::egui::Key::Enter);
        settle(&mut s);
        assert!(s.pixel_edit.is_none());
    }
}
