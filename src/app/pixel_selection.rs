//! Edits to selection coverage, independent of the pixels being selected.
use super::{Studio, masking::SelectionSpace};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edit {
    Grow { radius: u32 },
    Shrink { radius: u32 },
    Feather { radius: u32 },
}

impl Edit {
    pub fn title(self) -> &'static str {
        match self {
            Self::Grow { .. } => "Grow selection",
            Self::Shrink { .. } => "Shrink selection",
            Self::Feather { .. } => "Feather selection",
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
        self.pixel_sel_cache.clear();
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
    crate::paint::selection_bounds(values, space.w, space.h)?;
    let (Edit::Grow { radius } | Edit::Shrink { radius } | Edit::Feather { radius }) = operation;
    Some((
        coverage(
            values,
            space.w as usize,
            space.h as usize,
            radius.min(1024) as usize,
            operation,
        ),
        space,
    ))
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
    use crate::paint;
    use tiny_skia::Transform;

    fn space() -> SelectionSpace {
        SelectionSpace {
            w: 24,
            h: 20,
            transform: Transform::identity(),
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
