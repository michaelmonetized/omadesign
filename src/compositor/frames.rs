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
) {
    if shapes
        .iter()
        .all(|shape| !shape.layout.frame && shape.layout.parent.is_none())
    {
        for shape in shapes.iter().filter(|s| s.visible && !s.guide) {
            draw_shape(
                pm,
                shape,
                transform,
                opacity,
                blend,
                pose_of(shape.id, motion, doc, overrides),
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
        .filter(|s| s.blend != crate::color::Blend::Normal)
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
    }
    .draw_children(pm, None, transform, opacity, blend, None, 0);
}

struct Context<'a> {
    children: HashMap<Option<u64>, Vec<&'a Shape>>,
    blended_descendants: HashSet<u64>,
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
        depth: usize,
    ) {
        if depth >= 64 {
            return;
        }
        for shape in self.children.get(&parent).into_iter().flatten() {
            if !shape.visible || shape.guide {
                continue;
            }
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
            let alpha = pose.opacity.unwrap_or(shape.opacity).clamp(0.0, 1.0);
            if alpha <= 0.0 {
                continue;
            }
            if alpha < 1.0
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
                    depth,
                );
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
                self.draw_frame(pm, shape, transform, opacity, blend, pose, mask, depth);
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
        depth: usize,
    ) {
        // The frame's opacity and effects belong to the whole subtree, once.
        pose.opacity = Some(1.0);
        draw_shape_inner(pm, shape, transform, opacity, blend, pose, mask);
        let motion_transform = transform.pre_concat(pose.to_skia(shape.world_bbox().center()));
        let centre = shape.geom.bbox().center();
        let child_transform = motion_transform.pre_concat(Transform::from_rotate_at(
            shape.rotation.to_degrees(),
            centre.x,
            centre.y,
        ));
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
            depth + 1,
        );
    }
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
}
