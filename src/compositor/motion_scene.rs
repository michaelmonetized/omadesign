//! Additional motion scene pass, isolated from vector group rendering.
use super::*;

pub(super) struct Budget {
    remaining: usize,
    pixels: u64,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            remaining: 128,
            pixels: 32_000_000,
        }
    }
}

/// Composite bounded nested documents after the parent artwork.
/// Args: destination, document transform/time, frame poses, cancellation and traversal budget.
/// Returns a named resource, expression or cancellation error.
pub(super) fn draw(
    pm: &mut Pixmap,
    doc: &Document,
    transform: Transform,
    time: f32,
    depth: usize,
    poses: &HashMap<u64, Pose>,
    keep: &mut dyn FnMut() -> bool,
    budget: &mut Budget,
) -> Result<(), String> {
    if depth > crate::motion::compositions::MAX_DEPTH {
        return Err("CompositionDepth: at most eight nested compositions".into());
    }
    if let Some(scene) = &doc.motion.scene3d {
        let view = crate::scene3d::render::View {
            width: pm.width(),
            height: pm.height(),
            document_width: doc.width as f64,
            document_height: doc.height as f64,
            affine: [
                transform.sx as f64,
                transform.ky as f64,
                transform.kx as f64,
                transform.sy as f64,
                transform.tx as f64,
                transform.ty as f64,
            ],
        };
        crate::scene3d::render::render(scene, view, time as f64, keep)?.composite(pm)?;
    }
    for comp in &doc.motion.compositions {
        if !keep() {
            return Err("Cancelled: render cancelled".into());
        }
        let Some(local_time) = comp.local_time(time) else {
            continue;
        };
        budget.remaining = budget
            .remaining
            .checked_sub(1)
            .ok_or("CompositionLimit: too many composition layers")?;
        let pose = poses
            .get(&comp.id)
            .copied()
            .unwrap_or_else(|| doc.motion.pose(comp.id, time));
        let local = Transform::from_translate(-comp.anchor.x, -comp.anchor.y)
            .post_concat(Transform::from_scale(
                comp.scale.x * pose.scale * pose.width_scale,
                comp.scale.y * pose.scale * pose.height_scale,
            ))
            .post_concat(Transform::from_rotate(
                (comp.rotation + pose.rotation).to_degrees(),
            ))
            .post_concat(Transform::from_translate(
                comp.position.x + pose.dx,
                comp.position.y + pose.dy,
            ));
        let placement = transform.pre_concat(local);
        let page = visible_polygon(
            &[
                Pt::ZERO,
                Pt::new(comp.document.width, 0.),
                comp.document.size(),
                Pt::new(0., comp.document.height),
            ],
            placement,
            pm.width(),
            pm.height(),
        )?;
        if page.is_empty() {
            continue;
        }
        let min_x = page
            .iter()
            .map(|p| p[0])
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.) as u32;
        let min_y = page
            .iter()
            .map(|p| p[1])
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.) as u32;
        let max_x = page
            .iter()
            .map(|p| p[0])
            .fold(0., f64::max)
            .ceil()
            .min(pm.width() as f64) as u32;
        let max_y = page
            .iter()
            .map(|p| p[1])
            .fold(0., f64::max)
            .ceil()
            .min(pm.height() as f64) as u32;
        let (w, h) = (max_x.saturating_sub(min_x), max_y.saturating_sub(min_y));
        if w == 0 || h == 0 {
            continue;
        }
        budget.pixels = budget
            .pixels
            .checked_sub(w as u64 * h as u64)
            .ok_or("CompositionPixels: visible nested surfaces exceed 32 million pixels")?;
        let mut child =
            Pixmap::new(w, h).ok_or("CompositionPixels: cannot allocate visible composition")?;
        let child_transform =
            placement.post_concat(Transform::from_translate(-(min_x as f32), -(min_y as f32)));
        let mut path = PathBuilder::new();
        for (i, p) in page.iter().enumerate() {
            let (x, y) = ((p[0] - min_x as f64) as f32, (p[1] - min_y as f64) as f32);
            if i == 0 {
                path.move_to(x, y);
            } else {
                path.line_to(x, y);
            }
        }
        path.close();
        let path = path
            .finish()
            .ok_or("CompositionTransform: invalid visible page")?;
        let mut mask =
            tiny_skia::Mask::new(w, h).ok_or("CompositionPixels: cannot allocate visible clip")?;
        mask.fill_path(&path, FillRule::Winding, true, Transform::identity());
        let child_poses = comp
            .document
            .motion
            .evaluate_frame(local_time, keep)
            .map_err(|e| e.to_string())?;
        if !comp.document.transparent {
            if comp.document.artboards.is_empty() {
                child.fill(tiny_skia::Color::WHITE);
            } else {
                for artboard in &comp.document.artboards {
                    let polygon = visible_polygon(&artboard.corners(), child_transform, w, h)?;
                    let mut path = PathBuilder::new();
                    for (i, p) in polygon.iter().enumerate() {
                        if i == 0 {
                            path.move_to(p[0] as f32, p[1] as f32);
                        } else {
                            path.line_to(p[0] as f32, p[1] as f32);
                        }
                    }
                    path.close();
                    if let Some(path) = path.finish() {
                        let mut paint = Paint::default();
                        paint.set_color_rgba8(255, 255, 255, 255);
                        child.fill_path(
                            &path,
                            &paint,
                            FillRule::Winding,
                            Transform::identity(),
                            None,
                        );
                    }
                }
            }
        }
        groups::draw(
            &mut child,
            &comp.document,
            child_transform,
            &Draft::none(),
            Some(local_time),
            Some(&child_poses),
        );
        draw(
            &mut child,
            &comp.document,
            child_transform,
            local_time,
            depth + 1,
            &child_poses,
            keep,
            budget,
        )?;
        child.apply_mask(&mask);
        pm.draw_pixmap(
            min_x as i32,
            min_y as i32,
            child.as_ref(),
            &PixmapPaint {
                opacity: (comp.opacity * pose.opacity.unwrap_or(1.)).clamp(0., 1.),
                quality: tiny_skia::FilterQuality::Bilinear,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
    Ok(())
}

/// Clip a transformed page to actual output pixels.
/// Args: finite page corners, placement and output dimensions. Returns a bounded visible polygon.
fn visible_polygon(
    points: &[Pt],
    transform: Transform,
    width: u32,
    height: u32,
) -> Result<Vec<[f64; 2]>, String> {
    if [
        transform.sx,
        transform.sy,
        transform.kx,
        transform.ky,
        transform.tx,
        transform.ty,
    ]
    .iter()
    .any(|v| !v.is_finite())
    {
        return Err("CompositionTransform: nonfinite placement".into());
    }
    let mut polygon: Vec<_> = points
        .iter()
        .map(|p| {
            [
                transform.sx as f64 * p.x as f64
                    + transform.kx as f64 * p.y as f64
                    + transform.tx as f64,
                transform.ky as f64 * p.x as f64
                    + transform.sy as f64 * p.y as f64
                    + transform.ty as f64,
            ]
        })
        .collect();
    if polygon.iter().flatten().any(|p| !p.is_finite()) {
        return Err("CompositionTransform: nonfinite page bounds".into());
    }
    for (axis, bound, lower) in [
        (0, 0., true),
        (0, width as f64, false),
        (1, 0., true),
        (1, height as f64, false),
    ] {
        let Some(mut a) = polygon.last().copied() else {
            break;
        };
        let mut output = Vec::with_capacity(polygon.len() + 1);
        for &b in &polygon {
            let inside = |p: [f64; 2]| {
                if lower {
                    p[axis] >= bound
                } else {
                    p[axis] <= bound
                }
            };
            if inside(a) != inside(b) {
                let t = (bound - a[axis]) / (b[axis] - a[axis]);
                let mut p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
                p[axis] = bound;
                output.push(p);
            }
            if inside(b) {
                output.push(b);
            }
            a = b;
        }
        polygon = output;
    }
    let area: f64 = polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .take(polygon.len())
        .map(|(a, b)| a[0] * b[1] - a[1] * b[0])
        .sum();
    if area == 0. {
        polygon.clear();
    }
    Ok(polygon)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn nested(size: f32) -> Document {
        let mut child = Document::new("Virtual", 16., 16., 96.);
        child.layers.clear();
        child.width = size;
        child.height = size;
        child.artboards.clear();
        child.artboardless = true;
        child.transparent = true;
        let mut layer = Layer::vector("Vector");
        let shape = Shape::new(
            Geom::Rect {
                origin: Pt::ZERO,
                size: Pt::splat(size),
                radius: 0.,
            },
            crate::document::Style {
                fill: Fill::Solid(Rgba::rgb(255, 0, 0)),
                stroke: None,
            },
        );
        layer.kind.shapes_mut().unwrap().push(shape);
        child.layers.push(layer);
        let mut doc = Document::new("Parent", 16., 16., 96.);
        doc.layers.clear();
        doc.width = size;
        doc.height = size;
        doc.artboards.clear();
        doc.artboardless = true;
        doc.transparent = true;
        doc.motion
            .compositions
            .push(crate::motion::compositions::Composition::new(child));
        doc
    }
    #[test]
    fn large_virtual_vector_composition_uses_visible_scale_without_a_floor() {
        let doc = nested(1e12);
        let scale = 64. / doc.width;
        assert!(scale < 0.01);
        let mut pm = Pixmap::new(64, 64).unwrap();
        let mut budget = Budget {
            remaining: 128,
            pixels: 64 * 64,
        };
        draw(
            &mut pm,
            &doc,
            Transform::from_scale(scale, scale),
            0.,
            0,
            &HashMap::new(),
            &mut || true,
            &mut budget,
        )
        .unwrap();
        assert_eq!(
            pm.pixel(32, 32).unwrap().demultiply(),
            tiny_skia::ColorU8::from_rgba(255, 0, 0, 255)
        );
        assert_eq!(budget.pixels, 0);
        let preview = render_view_posed(
            &doc,
            View {
                scale,
                offset: Pt::ZERO,
            },
            64,
            64,
            Draft::none(),
            Some(0.),
            None,
        )
        .unwrap();
        assert_eq!(preview.pixel(32, 32), pm.pixel(32, 32));
        assert_eq!(motion_error(), None);
    }
    #[test]
    fn offscreen_virtual_composition_does_not_allocate_or_charge_surface_pixels() {
        let doc = nested(1e12);
        let mut pm = Pixmap::new(64, 64).unwrap();
        let mut budget = Budget {
            remaining: 128,
            pixels: 0,
        };
        let transform = Transform::from_scale(1e-10, 1e-10)
            .post_concat(Transform::from_translate(1000., 1000.));
        draw(
            &mut pm,
            &doc,
            transform,
            0.,
            0,
            &HashMap::new(),
            &mut || true,
            &mut budget,
        )
        .unwrap();
        assert_eq!(budget.pixels, 0);
        assert!(pm.pixels().iter().all(|p| p.alpha() == 0));
    }
    #[test]
    fn visible_crop_keeps_rotated_page_clipping_and_pixel_safeguards() {
        let mut doc = nested(10.);
        let comp = &mut doc.motion.compositions[0];
        comp.position = Pt::new(16., 8.);
        comp.rotation = std::f32::consts::FRAC_PI_4;
        comp.document.layers[0].kind.shapes_mut().unwrap()[0].geom = Geom::Rect {
            origin: Pt::splat(-100.),
            size: Pt::splat(200.),
            radius: 0.,
        };
        let mut pm = Pixmap::new(32, 32).unwrap();
        draw(
            &mut pm,
            &doc,
            Transform::identity(),
            0.,
            0,
            &HashMap::new(),
            &mut || true,
            &mut Budget::default(),
        )
        .unwrap();
        assert_eq!(pm.pixel(9, 8).unwrap().alpha(), 0);
        assert_eq!(pm.pixel(16, 15).unwrap().alpha(), 255);
        assert!(
            draw(
                &mut pm,
                &doc,
                Transform::identity(),
                0.,
                0,
                &HashMap::new(),
                &mut || true,
                &mut Budget {
                    remaining: 128,
                    pixels: 0
                }
            )
            .unwrap_err()
            .contains("visible nested surfaces")
        );
    }
}
