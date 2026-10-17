//! Frame traversal shares the document's flat shape storage without making draw order flat.
//! O(n) indexing per layer; ordinary vector artwork retains its direct draw path.
use super::*;
use std::collections::HashSet;

pub(super) fn draw(
    pm: &mut Pixmap,
    shapes: &[Shape],
    transform: Transform,
    opacity: f32,
    blend: tiny_skia::BlendMode,
    motion: Option<f32>,
    doc: &Document,
    overrides: Option<&HashMap<u64, Pose>>,
    sources: &HashSet<crate::artwork_mask::MaskSource>,
) {
    draw_with_source(
        pm, shapes, transform, opacity, blend, motion, doc, overrides, None, transform, sources,
    );
}

fn draw_with_source(
    pm: &mut Pixmap,
    shapes: &[Shape],
    transform: Transform,
    opacity: f32,
    blend: tiny_skia::BlendMode,
    motion: Option<f32>,
    doc: &Document,
    overrides: Option<&HashMap<u64, Pose>>,
    source: Option<u64>,
    view: Transform,
    sources: &HashSet<crate::artwork_mask::MaskSource>,
) {
    let sources: HashSet<_> = sources
        .iter()
        .filter_map(|s| s.shape)
        .filter(|id| Some(*id) != source)
        .collect();
    if shapes
        .iter()
        .all(|shape| !shape.layout.frame && shape.layout.parent.is_none())
    {
        for shape in shapes
            .iter()
            .filter(|s| s.visible && !s.guide && !sources.contains(&s.id))
        {
            let live = artwork_masks::render(
                &shape.artwork_masks,
                doc,
                view,
                pm.width(),
                pm.height(),
                motion,
                overrides,
            );
            draw_shape_masked(
                pm,
                shape,
                transform,
                opacity,
                blend,
                pose_of(shape.id, motion, doc, overrides),
                live.as_ref(),
            );
        }
        return;
    }
    let ids: HashSet<_> = shapes
        .iter()
        .filter(|s| s.layout.frame)
        .map(|s| s.id)
        .collect();
    let parents: HashMap<_, _> = shapes
        .iter()
        .filter(|s| s.layout.frame)
        .map(|s| (s.id, s.layout.parent.filter(|id| ids.contains(id))))
        .collect();
    let mut blended_descendants = HashSet::new();
    for shape in shapes
        .iter()
        .filter(|s| s.blend != crate::color::Blend::Normal || s.filters.blends_backdrop())
    {
        let mut parent = shape.layout.parent;
        for _ in 0..64 {
            let Some(id) = parent.filter(|id| ids.contains(id)) else {
                break;
            };
            if !blended_descendants.insert(id) {
                break;
            }
            parent = parents.get(&id).copied().flatten();
        }
    }
    let mut children: HashMap<Option<u64>, Vec<&Shape>> = HashMap::new();
    for shape in shapes {
        let parent = shape.layout.parent.filter(|id| ids.contains(id));
        children.entry(parent).or_default().push(shape);
    }
    Context {
        children,
        blended_descendants,
        motion,
        doc,
        overrides,
        sources,
    }
    .draw_children(pm, None, transform, opacity, blend, None, view, 0);
}

struct Context<'a> {
    children: HashMap<Option<u64>, Vec<&'a Shape>>,
    blended_descendants: HashSet<u64>,
    sources: HashSet<u64>,
    motion: Option<f32>,
    doc: &'a Document,
    overrides: Option<&'a HashMap<u64, Pose>>,
}

impl Context<'_> {
    fn draw_children(
        &self,
        pm: &mut Pixmap,
        parent: Option<u64>,
        transform: Transform,
        opacity: f32,
        blend: tiny_skia::BlendMode,
        mask: Option<&tiny_skia::Mask>,
        view: Transform,
        depth: usize,
    ) {
        if depth >= 64 {
            return;
        }
        for shape in self.children.get(&parent).into_iter().flatten() {
            if !shape.visible || shape.guide || self.sources.contains(&shape.id) {
                continue;
            }
            let mut live = artwork_masks::render(
                &shape.artwork_masks,
                self.doc,
                view,
                pm.width(),
                pm.height(),
                self.motion,
                self.overrides,
            );
            if let (Some(live), Some(parent)) = (&mut live, mask) {
                artwork_masks::multiply(live, parent);
            }
            let mask = live.as_ref().or(mask);
            let pose = pose_of(shape.id, self.motion, self.doc, self.overrides);
            if !shape.layout.frame {
                draw_shape_masked(pm, shape, transform, opacity, blend, pose, mask);
                continue;
            }
            if shape.layout.clip && !shape.filters.active() {
                let b = shape.world_bbox();
                let mut corners = [
                    Point::from_xy(b.min.x, b.min.y),
                    Point::from_xy(b.max.x, b.min.y),
                    Point::from_xy(b.max.x, b.max.y),
                    Point::from_xy(b.min.x, b.max.y),
                ];
                transform
                    .pre_concat(pose.to_skia(b.center()))
                    .map_points(&mut corners);
                if corners.iter().all(|p| p.x < -2.)
                    || corners.iter().all(|p| p.y < -2.)
                    || corners.iter().all(|p| p.x > pm.width() as f32 + 2.)
                    || corners.iter().all(|p| p.y > pm.height() as f32 + 2.)
                {
                    continue;
                }
            }
            let own_mask = object_mask(pm, shape, transform, pose, mask);
            let mask = own_mask.as_deref().or(mask);
            let alpha = pose.opacity.unwrap_or(shape.opacity).clamp(0.0, 1.0);
            if alpha <= 0.0 {
                continue;
            }
            if shape.filters.active()
                && self.draw_local_filter(
                    pm, shape, transform, opacity, blend, pose, mask, view, depth,
                )
            {
                continue;
            }
            if mask.is_some()
                || shape.fill_opacity < 1.
                || alpha < 1.0
                || shape.filters.active()
                || shape.blend != crate::color::Blend::Normal
                || self.blended_descendants.contains(&shape.id)
            {
                let Some(mut group) = Pixmap::new(pm.width(), pm.height()) else {
                    continue;
                };
                self.draw_frame(
                    &mut group,
                    shape,
                    transform,
                    1.0,
                    tiny_skia::BlendMode::SourceOver,
                    pose,
                    None,
                    view,
                    depth,
                );
                if shape.filters.independent() || shape.fill_opacity < 1. || shape.blend_interior {
                    crate::filter::composite(
                        pm,
                        group,
                        &shape.filters,
                        Transform::identity(),
                        if shape.blend == crate::color::Blend::Normal {
                            blend
                        } else {
                            shape.blend.to_skia()
                        },
                        opacity * alpha,
                        shape.fill_opacity,
                        shape.blend_interior,
                        mask,
                    );
                    continue;
                }
                if shape.filters.active() {
                    crate::filter::apply(&mut group, &shape.filters);
                }
                pm.draw_pixmap(
                    0,
                    0,
                    group.as_ref(),
                    &PixmapPaint {
                        opacity: (opacity * alpha).clamp(0.0, 1.0),
                        blend_mode: if shape.blend == crate::color::Blend::Normal {
                            blend
                        } else {
                            shape.blend.to_skia()
                        },
                        ..Default::default()
                    },
                    Transform::identity(),
                    mask,
                );
            } else {
                self.draw_frame(
                    pm, shape, transform, opacity, blend, pose, mask, view, depth,
                );
            }
        }
    }

    fn draw_frame(
        &self,
        pm: &mut Pixmap,
        shape: &Shape,
        transform: Transform,
        opacity: f32,
        blend: tiny_skia::BlendMode,
        mut pose: Pose,
        mask: Option<&tiny_skia::Mask>,
        view: Transform,
        depth: usize,
    ) {
        // The frame's opacity and effects belong to the whole subtree, once.
        pose.opacity = Some(1.0);
        draw_shape_inner(pm, shape, transform, opacity, blend, pose, mask);
        let motion_transform = transform.pre_concat(pose.to_skia(shape.world_bbox().center()));
        let child_transform = frame_transform(shape, transform, pose);
        let child_mask = if shape.layout.clip {
            let Some(path) = shape.get_cached_path(96) else {
                return;
            };
            let Some(mut clip) = mask
                .cloned()
                .or_else(|| tiny_skia::Mask::new(pm.width(), pm.height()))
            else {
                return;
            };
            if mask.is_some() {
                clip.intersect_path(&path, FillRule::Winding, true, motion_transform);
            } else {
                clip.fill_path(&path, FillRule::Winding, true, motion_transform);
            }
            Some(clip)
        } else {
            None
        };
        self.draw_children(
            pm,
            Some(shape.id),
            child_transform,
            opacity,
            blend,
            child_mask.as_ref().or(mask),
            view,
            depth + 1,
        );
    }

    /// Bound the actual subtree in the caller's coordinate system.
    /// Args: root, inherited transform, current pose and depth. Returns conservative geometry/effect bounds.
    fn subtree_bounds(
        &self,
        shape: &Shape,
        transform: Transform,
        pose: Pose,
        depth: usize,
    ) -> Option<crate::geom::Bounds> {
        if depth >= 64 {
            return None;
        }
        let placed = transform.pre_concat(pose.to_skia(shape.world_bbox().center()));
        let mut bounds = super::transformed_bounds(shape.world_bbox(), placed)?;
        if shape.layout.frame && !shape.layout.clip {
            let center = shape.geom.bbox().center();
            let child_transform = placed.pre_concat(Transform::from_rotate_at(
                shape.rotation.to_degrees(),
                center.x,
                center.y,
            ));
            for child in self.children.get(&Some(shape.id)).into_iter().flatten() {
                if !child.visible || child.guide || self.sources.contains(&child.id) {
                    continue;
                }
                let child_pose = pose_of(child.id, self.motion, self.doc, self.overrides);
                if child_pose.opacity.unwrap_or(child.opacity) <= 0. {
                    continue;
                }
                let mut next =
                    self.subtree_bounds(child, child_transform, child_pose, depth + 1)?;
                if child.filters.active() {
                    let placed =
                        child_transform.pre_concat(child_pose.to_skia(child.world_bbox().center()));
                    let scale = (placed.sx * placed.sx
                        + placed.sy * placed.sy
                        + placed.kx * placed.kx
                        + placed.ky * placed.ky)
                        .sqrt();
                    next =
                        next.inflate(crate::filter::svg_pad(&child.filters).ceil().max(8.) * scale);
                }
                bounds = bounds.union(next);
            }
        }
        Some(bounds)
    }

    /// Filter an appearance group in the same stable object space as an ordinary object.
    /// Args: destination, frame, placement, appearance, mask/view and depth. Returns false to retain the viewport-bounded path.
    fn draw_local_filter(
        &self,
        pm: &mut Pixmap,
        shape: &Shape,
        transform: Transform,
        opacity: f32,
        blend: tiny_skia::BlendMode,
        pose: Pose,
        mask: Option<&tiny_skia::Mask>,
        view: Transform,
        depth: usize,
    ) -> bool {
        let translation = Pt::new(pose.dx, pose.dy);
        let local_pose = Pose {
            dx: 0.,
            dy: 0.,
            opacity: Some(1.),
            ..pose
        };
        let Some(bounds) = self.subtree_bounds(shape, Transform::identity(), local_pose, depth)
        else {
            return false;
        };
        let bounds = bounds.inflate(crate::filter::svg_pad(&shape.filters).ceil().max(8.));
        let width = bounds.width().ceil().max(1.);
        let height = bounds.height().ceil().max(1.);
        if !width.is_finite()
            || !height.is_finite()
            || width * height * 4. > effects_cache::MAX_BYTES as f32
        {
            return false;
        }
        let local = Transform::from_translate(-bounds.min.x, -bounds.min.y);
        let Some(inverse) = transform.invert() else {
            return false;
        };
        let local_view = local
            .pre_translate(-translation.x, -translation.y)
            .pre_concat(inverse)
            .pre_concat(view);
        let bytes = (width as usize)
            .checked_mul(height as usize)
            .and_then(|count| count.checked_mul(4));
        let Some(bytes) = bytes else {
            return false;
        };
        let Ok(_memory) = crate::resources::global()
            .reserve_now(bytes as u64, &crate::deep_color::storage::check)
        else {
            return false;
        };
        let Some(mut pixels) = Pixmap::new(width as u32, height as u32) else {
            return false;
        };
        self.draw_frame(
            &mut pixels,
            shape,
            local,
            1.,
            tiny_skia::BlendMode::SourceOver,
            local_pose,
            None,
            local_view,
            depth,
        );
        let placement = transform.pre_concat(Transform::from_translate(
            bounds.min.x + translation.x,
            bounds.min.y + translation.y,
        ));
        crate::filter::composite(
            pm,
            pixels,
            &shape.filters,
            placement,
            if shape.blend == crate::color::Blend::Normal {
                blend
            } else {
                shape.blend.to_skia()
            },
            opacity * pose.opacity.unwrap_or(shape.opacity).clamp(0., 1.),
            shape.fill_opacity,
            shape.blend_interior,
            mask,
        );
        true
    }
}

pub(super) fn draw_item(
    pm: &mut Pixmap,
    doc: &Document,
    index: usize,
    id: u64,
    transform: Transform,
    motion: Option<f32>,
    overrides: Option<&HashMap<u64, Pose>>,
) {
    let Some(root) = doc.find_shape(index, id) else {
        return;
    };
    let descendants = crate::layout::descendants(doc, index, id);
    let shapes: Vec<_> = doc.layers[index]
        .kind
        .shapes()
        .unwrap_or(&[])
        .iter()
        .filter(|s| s.id == id || descendants.contains(&s.id))
        .map(|s| {
            let mut s = s.clone();
            if s.id == id {
                s.layout.parent = None;
            }
            s
        })
        .collect();
    draw_with_source(
        pm,
        &shapes,
        transform.pre_concat(shape_parent_transform(doc, index, id)),
        1.,
        tiny_skia::BlendMode::SourceOver,
        motion,
        doc,
        overrides,
        Some(root.id),
        transform,
        &doc.mask_sources(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Style;

    fn rect(x: f32, y: f32, w: f32, h: f32, color: Rgba) -> Shape {
        Shape::new(
            Geom::Rect {
                origin: Pt::new(x, y),
                size: Pt::new(w, h),
                radius: 0.0,
            },
            Style {
                fill: Fill::Solid(color),
                stroke: None,
            },
        )
    }

    #[test]
    fn nested_clips_intersect_and_hidden_frames_hide_their_whole_subtree() {
        let mut doc = Document::new("clipped", 100., 100., 72.);
        doc.transparent = true;
        let mut outer = rect(10., 10., 50., 50., Rgba::rgb(255, 255, 255));
        outer.layout = crate::layout::FrameLayout::frame();
        let mut inner = rect(40., 20., 40., 30., Rgba::rgb(0, 255, 0));
        inner.layout = crate::layout::FrameLayout::frame();
        inner.layout.parent = Some(outer.id);
        let mut child = rect(0., 0., 100., 100., Rgba::rgb(255, 0, 0));
        child.layout.parent = Some(inner.id);
        doc.layers = vec![Layer::vector("Frames")];
        // Storage order must not let children draw below their parent background.
        *doc.layers[0].kind.shapes_mut().unwrap() = vec![child, inner, outer];
        let pm = render_export(&doc, 1).unwrap();
        assert_eq!(pm.pixel(45, 25).unwrap().red(), 255);
        assert_eq!(pm.pixel(65, 25).unwrap().alpha(), 0);
        assert_eq!(pm.pixel(20, 25).unwrap().green(), 255);
        assert_eq!(pm.pixel(45, 55).unwrap().green(), 255);
        doc.layers[0].kind.shapes_mut().unwrap()[2].visible = false;
        assert!(
            render_export(&doc, 1)
                .unwrap()
                .data()
                .iter()
                .all(|b| *b == 0)
        );
    }

    #[test]
    fn frame_opacity_composites_once_and_clip_toggle_permits_overflow() {
        let mut doc = Document::new("opacity", 60., 60., 72.);
        doc.transparent = true;
        let mut frame = rect(10., 10., 20., 20., Rgba::rgb(255, 255, 255));
        frame.layout = crate::layout::FrameLayout::frame();
        frame.opacity = 0.5;
        let mut child = rect(15., 15., 30., 30., Rgba::rgb(0, 0, 255));
        child.layout.parent = Some(frame.id);
        doc.layers = vec![Layer::vector("Frames")];
        *doc.layers[0].kind.shapes_mut().unwrap() = vec![frame, child];
        let pm = render_export(&doc, 1).unwrap();
        assert_eq!(pm.pixel(20, 20).unwrap().alpha(), 128);
        assert_eq!(pm.pixel(40, 20).unwrap().alpha(), 0);
        doc.layers[0].kind.shapes_mut().unwrap()[0].layout.clip = false;
        assert_eq!(
            render_export(&doc, 1)
                .unwrap()
                .pixel(40, 20)
                .unwrap()
                .blue(),
            128
        );
    }
    #[test]
    fn filtered_frame_matches_a_single_object_at_native_and_double_resolution() {
        let mut doc = Document::new("Filtered frame", 120., 90., 72.);
        doc.transparent = true;
        doc.layers = vec![Layer::vector("Frame")];
        let mut root = rect(20.25, 18.5, 45., 38., Rgba::rgb(220, 30, 80));
        root.filters
            .items
            .push(crate::filter::Fx::Blur { std: 1.25 });
        root.opacity = 0.6;
        root.fill_opacity = 0.8;
        doc.layers[0].kind.shapes_mut().unwrap().push(root.clone());
        let before = [
            render_export(&doc, 1).unwrap(),
            render_export(&doc, 2).unwrap(),
        ];
        let mut child = root.clone();
        child.id = crate::document::next_id();
        child.opacity = 1.;
        child.fill_opacity = 1.;
        child.filters = Default::default();
        child.layout.parent = Some(root.id);
        root.style.fill = Fill::None;
        root.layout = crate::layout::FrameLayout::frame();
        root.layout.clip = false;
        *doc.layers[0].kind.shapes_mut().unwrap() = vec![root, child];
        for (index, scale) in [1, 2].into_iter().enumerate() {
            let actual = render_export(&doc, scale).unwrap();
            assert!(
                actual.data() == before[index].data(),
                "filtered frame changed at {scale}x"
            );
        }
    }

    #[test]
    fn filtered_frame_keeps_overflow_and_live_masks_in_document_space_during_translation() {
        let mut doc = Document::new("Moving masked frame", 120., 90., 72.);
        doc.transparent = true;
        doc.layers = vec![Layer::vector("Frame")];
        let mut root = rect(20., 20., 30., 30., Rgba::BLACK);
        root.style.fill = Fill::None;
        root.layout = crate::layout::FrameLayout::frame();
        root.layout.clip = false;
        root.filters.items.push(crate::filter::Fx::Blur { std: 1. });
        let source = rect(25., 20., 10., 30., Rgba::WHITE);
        let mut child = rect(20., 20., 30., 30., Rgba::rgb(240, 30, 70));
        child.layout.parent = Some(root.id);
        child
            .artwork_masks
            .push(crate::artwork_mask::ArtworkMask::new(
                crate::artwork_mask::MaskSource {
                    layer: doc.layers[0].id,
                    shape: Some(source.id),
                },
            ));
        doc.motion.set_key(
            root.id,
            crate::motion::Prop::X,
            0.,
            0.,
            crate::motion::Ease::Linear,
        );
        doc.motion.set_key(
            root.id,
            crate::motion::Prop::X,
            1.,
            25.,
            crate::motion::Ease::Linear,
        );
        *doc.layers[0].kind.shapes_mut().unwrap() = vec![root, child, source];
        let before = render_export_at(&doc, 1., 0., true).unwrap();
        assert!(before.pixels().iter().any(|p| p.alpha() > 0));
        assert!(
            render_export_at(&doc, 1., 1., true)
                .unwrap()
                .pixels()
                .iter()
                .all(|p| p.alpha() == 0)
        );
        let mut overflow = doc.clone();
        let root = &mut overflow.layers[0].kind.shapes_mut().unwrap()[0];
        root.geom = Geom::Rect {
            origin: Pt::new(0., 0.),
            size: Pt::new(5., 5.),
            radius: 0.,
        };
        assert!(
            render_export_at(&overflow, 1., 0., true).unwrap().data() == before.data(),
            "overflow child was clipped by frame bounds"
        );
    }
}
