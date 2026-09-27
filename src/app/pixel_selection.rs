//! Edits to selection coverage, independent of the pixels being selected.
use super::{Studio, masking::SelectionSpace};
use std::collections::VecDeque;
use tiny_skia::Transform;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edit {
    Move {
        x: f32,
        y: f32,
    },
    Resize {
        width: f32,
        height: f32,
    },
    Grow {
        radius: u32,
    },
    Shrink {
        radius: u32,
    },
    Feather {
        radius: u32,
    },
    Reshape {
        angle: f32,
        skew_x: f32,
        skew_y: f32,
    },
}

impl Edit {
    pub fn title(self) -> &'static str {
        match self {
            Self::Move { .. } => "Move selection",
            Self::Resize { .. } => "Resize selection",
            Self::Grow { .. } => "Grow selection",
            Self::Shrink { .. } => "Shrink selection",
            Self::Feather { .. } => "Feather selection",
            Self::Reshape { .. } => "Reshape selection",
        }
    }
}

impl Studio {
    pub(crate) fn pixel_selection_space(&self) -> Option<SelectionSpace> {
        let mask = self.pixel_sel.as_ref()?;
        let space = self
            .pixel_sel_space
            .or_else(|| self.selection_target_space())?;
        (mask.len() == space.w as usize * space.h as usize).then_some(space)
    }

    pub(crate) fn select_all_pixels(&mut self) {
        self.end_pixel_stroke(false);
        if let Some(space) = self.selection_target_space() {
            self.replace_pixel_selection(vec![255; space.w as usize * space.h as usize], space);
        }
    }

    pub(crate) fn invert_pixel_selection(&mut self) {
        self.end_pixel_stroke(false);
        if let Some(space) = self.pixel_selection_space() {
            let values = self
                .pixel_sel
                .as_ref()
                .unwrap()
                .iter()
                .map(|v| 255 - v)
                .collect();
            self.replace_pixel_selection(values, space);
        } else {
            self.select_all_pixels();
        }
    }

    pub(crate) fn replace_pixel_selection(&mut self, values: Vec<u8>, space: SelectionSpace) {
        self.pixel_sel = Some(values);
        self.pixel_sel_space = Some(space);
        self.pixel_sel_gen = self.pixel_sel_gen.wrapping_add(1);
        self.status = format!(
            "{} pixels selected",
            crate::paint::selected_count(self.pixel_sel.as_ref().unwrap())
        );
    }
}

/// Preserve the native mask coordinate system, including placed/rotated rasters.
/// Resample once on Apply, always previewing from the original coverage.
pub fn edit(
    values: &[u8],
    space: SelectionSpace,
    operation: Edit,
) -> Option<(Vec<u8>, SelectionSpace)> {
    let (x0, y0, x1, y1) = crate::paint::selection_bounds(values, space.w, space.h)?;
    let mut output = space;
    let center = ((x0 + x1) as f32 * 0.5, (y0 + y1) as f32 * 0.5);
    let local = match operation {
        Edit::Move { x, y } if x.is_finite() && y.is_finite() => Transform::from_translate(x, y),
        Edit::Resize { width, height }
            if width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0 =>
        {
            Transform::from_translate(x0 as f32, y0 as f32)
                .pre_scale(width / (x1 - x0) as f32, height / (y1 - y0) as f32)
                .pre_translate(-(x0 as f32), -(y0 as f32))
        }
        Edit::Reshape {
            angle,
            skew_x,
            skew_y,
        } if angle.is_finite() && skew_x.is_finite() && skew_y.is_finite() => {
            // Compose two shears, which stays invertible even when both are 45°.
            let sx = skew_x.clamp(-80.0, 80.0).to_radians().tan();
            let sy = skew_y.clamp(-80.0, 80.0).to_radians().tan();
            Transform::from_translate(center.0, center.1)
                .pre_rotate(angle)
                .pre_concat(Transform::from_row(1.0, 0.0, sx, 1.0, 0.0, 0.0))
                .pre_concat(Transform::from_row(1.0, sy, 0.0, 1.0, 0.0, 0.0))
                .pre_translate(-center.0, -center.1)
        }
        Edit::Grow { radius } | Edit::Shrink { radius } | Edit::Feather { radius } => {
            let radius = radius.min(1024) as usize;
            let values = coverage(
                values,
                space.w as usize,
                space.h as usize,
                radius,
                operation,
            );
            return Some((values, output));
        }
        _ => return None,
    };
    output.transform = space.transform.pre_concat(local);
    output.transform.invert()?;
    Some((super::masking::resample(values, output, space)?, space))
}

/// Two sliding-window passes keep even large radii linear in the mask size.
/// Outside the mask is unselected, including for erosion and feathering.
fn coverage(input: &[u8], w: usize, h: usize, radius: usize, operation: Edit) -> Vec<u8> {
    if radius == 0 {
        return input.to_vec();
    }
    let mut horizontal = vec![0; input.len()];
    let mut result = vec![0; input.len()];
    for y in 0..h {
        let row = line((0..w).map(|x| input[y * w + x]), radius, operation);
        horizontal[y * w..(y + 1) * w].copy_from_slice(&row);
    }
    for x in 0..w {
        let column = line((0..h).map(|y| horizontal[y * w + x]), radius, operation);
        for (y, value) in column.into_iter().enumerate() {
            result[y * w + x] = value;
        }
    }
    result
}

fn line(input: impl Iterator<Item = u8>, radius: usize, operation: Edit) -> Vec<u8> {
    let values: Vec<_> = input.collect();
    let n = values.len();
    let window = radius * 2 + 1;
    let mut result = Vec::with_capacity(n);
    let mut queue: VecDeque<(usize, u8)> = VecDeque::new();
    let mut sum = 0u32;
    for i in 0..n + radius * 2 {
        let value = i
            .checked_sub(radius)
            .and_then(|j| values.get(j))
            .copied()
            .unwrap_or(0);
        if matches!(operation, Edit::Feather { .. }) {
            sum += value as u32;
            if i >= window {
                sum -= (i - window)
                    .checked_sub(radius)
                    .and_then(|j| values.get(j))
                    .copied()
                    .unwrap_or(0) as u32;
            }
        } else {
            while queue.back().is_some_and(|&(_, v)| {
                if matches!(operation, Edit::Grow { .. }) {
                    v <= value
                } else {
                    v >= value
                }
            }) {
                queue.pop_back();
            }
            queue.push_back((i, value));
            while queue.front().is_some_and(|&(j, _)| j + window <= i) {
                queue.pop_front();
            }
        }
        if i + 1 >= window {
            result.push(if matches!(operation, Edit::Feather { .. }) {
                ((sum + window as u32 / 2) / window as u32) as u8
            } else {
                queue.front().unwrap().1
            });
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{geom::Pt, paint};

    fn space() -> SelectionSpace {
        SelectionSpace {
            w: 24,
            h: 20,
            transform: Transform::identity(),
        }
    }

    #[test]
    fn all_selection_shapes_move_resize_and_reshape_without_touching_source() {
        let space = space();
        for source in [
            paint::fill_rect_mask(24, 20, 6.0, 6.0, 12.0, 12.0),
            paint::fill_ellipse_mask(24, 20, 6.0, 6.0, 12.0, 12.0),
            paint::fill_poly_mask(
                24,
                20,
                &[Pt::new(6.0, 6.0), Pt::new(12.0, 7.0), Pt::new(9.0, 12.0)],
            ),
        ] {
            let original = source.clone();
            let (moved, result_space) =
                edit(&source, space, Edit::Move { x: 2.0, y: -1.0 }).unwrap();
            assert_eq!(result_space, space);
            for y in 1..20 {
                for x in 0..22 {
                    assert_eq!(source[y * 24 + x], moved[(y - 1) * 24 + x + 2]);
                }
            }
            let (resized, _) = edit(
                &source,
                space,
                Edit::Resize {
                    width: 10.0,
                    height: 8.0,
                },
            )
            .unwrap();
            assert!(paint::selected_count(&resized) > paint::selected_count(&source));
            let (reshaped, _) = edit(
                &source,
                space,
                Edit::Reshape {
                    angle: 20.0,
                    skew_x: 15.0,
                    skew_y: -10.0,
                },
            )
            .unwrap();
            assert_ne!(reshaped, source);
            assert_eq!(source, original);
        }
    }

    #[test]
    fn morphology_and_feather_handle_soft_values_and_image_edges() {
        let space = space();
        let source = paint::fill_rect_mask(24, 20, 6.0, 6.0, 12.0, 12.0);
        let (grown, _) = edit(&source, space, Edit::Grow { radius: 2 }).unwrap();
        let (shrunk, _) = edit(&source, space, Edit::Shrink { radius: 2 }).unwrap();
        let (soft, _) = edit(&source, space, Edit::Feather { radius: 2 }).unwrap();
        assert_eq!(
            paint::selection_bounds(&grown, 24, 20),
            Some((4, 4, 14, 14))
        );
        assert_eq!(
            paint::selection_bounds(&shrunk, 24, 20),
            Some((8, 8, 10, 10))
        );
        assert_eq!(soft[8 * 24 + 8], 255);
        assert!(soft[6 * 24 + 6] > 0 && soft[6 * 24 + 6] < 255);
        assert!(soft[5 * 24 + 5] > 0);
        assert_eq!(soft[0], 0);
        let (empty, _) = edit(&source, space, Edit::Shrink { radius: 1024 }).unwrap();
        assert!(empty.iter().all(|v| *v == 0));
        for operation in [
            Edit::Grow { radius: 0 },
            Edit::Shrink { radius: 0 },
            Edit::Feather { radius: 0 },
        ] {
            assert_eq!(edit(&soft, space, operation).unwrap().0, soft);
        }
        assert_eq!(
            line([32, 200, 64].into_iter(), 1, Edit::Grow { radius: 1 }),
            [200, 200, 200]
        );
        assert_eq!(
            line([32, 200, 64].into_iter(), 1, Edit::Shrink { radius: 1 }),
            [0, 32, 0]
        );
        assert_eq!(
            line([255, 255, 255].into_iter(), 1, Edit::Feather { radius: 1 }),
            [170, 255, 170]
        );
    }

    #[test]
    fn edits_preserve_placed_raster_space_and_reject_invalid_transforms() {
        let mut space = space();
        space.transform = Transform::from_translate(40.0, 70.0)
            .pre_rotate(30.0)
            .pre_scale(2.0, 3.0);
        let source = paint::fill_rect_mask(24, 20, 6.0, 6.0, 12.0, 12.0);
        let (moved, output) = edit(&source, space, Edit::Move { x: 2.0, y: 1.0 }).unwrap();
        assert_eq!(output, space);
        assert!(moved[8 * 24 + 10] > 250);
        assert_eq!(moved[5 * 24 + 5], 0);
        assert!(
            edit(
                &source,
                space,
                Edit::Resize {
                    width: 0.0,
                    height: 2.0
                }
            )
            .is_none()
        );
        assert!(
            edit(
                &source,
                space,
                Edit::Move {
                    x: f32::NAN,
                    y: 0.0
                }
            )
            .is_none()
        );
        assert!(
            edit(
                &source,
                space,
                Edit::Reshape {
                    angle: 0.0,
                    skew_x: 45.0,
                    skew_y: 45.0
                }
            )
            .is_some()
        );
    }

    #[test]
    fn pixel_menu_actions_leave_artwork_unchanged_and_keep_empty_masks() {
        let mut studio = Studio::new();
        studio.doc = crate::document::Document::new("Selections", 24.0, 20.0, 96.0);
        studio.doc.layers = vec![crate::document::Layer::raster("Pixels", 24, 20)];
        studio.active_layer = Some(0);
        let original = studio.doc.layers[0].clone();
        studio.select_all_pixels();
        assert_eq!(
            paint::selected_count(studio.pixel_sel.as_ref().unwrap()),
            480
        );
        studio.invert_pixel_selection();
        assert!(studio.pixel_sel.as_ref().unwrap().iter().all(|v| *v == 0));
        studio.invert_pixel_selection();
        assert!(studio.pixel_sel.as_ref().unwrap().iter().all(|v| *v == 255));
        assert_eq!(
            studio.doc.layers[0].kind.pixels().unwrap().data,
            original.kind.pixels().unwrap().data
        );
    }
}
