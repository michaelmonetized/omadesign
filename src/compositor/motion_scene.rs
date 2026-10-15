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
        let sampling = (placement
            .sx
            .hypot(placement.ky)
            .max(placement.kx.hypot(placement.sy)))
        .clamp(0.01, 8.);
        let w = (comp.document.width * sampling).ceil().max(1.) as u32;
        let h = (comp.document.height * sampling).ceil().max(1.) as u32;
        budget.pixels = budget
            .pixels
            .checked_sub(w as u64 * h as u64)
            .ok_or("CompositionPixels: nested surfaces exceed 32 million pixels")?;
        let mut child =
            Pixmap::new(w, h).ok_or("CompositionPixels: cannot allocate composition")?;
        let child_poses = comp
            .document
            .motion
            .evaluate_frame(local_time, keep)
            .map_err(|e| e.to_string())?;
        let child_transform = Transform::from_scale(sampling, sampling);
        if !comp.document.transparent {
            draw_export_plates(&mut child, &comp.document, sampling);
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
        pm.draw_pixmap(
            0,
            0,
            child.as_ref(),
            &PixmapPaint {
                opacity: (comp.opacity * pose.opacity.unwrap_or(1.)).clamp(0., 1.),
                quality: tiny_skia::FilterQuality::Bilinear,
                ..Default::default()
            },
            placement.pre_concat(Transform::from_scale(1. / sampling, 1. / sampling)),
            None,
        );
    }
    Ok(())
}
