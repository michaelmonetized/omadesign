//! Edits to selection coverage, independent of the pixels being selected.
use crate::selection_coverage::Sample;
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
        (mask.len() == space.w as usize * space.h as usize && crate::channels::valid_placement(space.transform)).then_some(space)
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
                .map(|v| 1. - v)
                .collect();
            self.replace_pixel_selection_float(values, space);
        } else {
            self.select_all_pixels();
        }
    }

    pub(crate) fn replace_pixel_selection(&mut self, values: Vec<u8>, space: SelectionSpace) {
        self.try_replace_pixel_selection(values,space);
    }

    /// Admit replacement coverage before changing the current selection.
    /// Args: owned mask and coordinate system. Returns false while preserving the current owner on failure.
    pub(crate) fn try_replace_pixel_selection(&mut self, values: Vec<u8>, space: SelectionSpace) -> bool {
        match crate::selection_coverage::from_bytes(&values,&crate::deep_color::storage::check) {
            Ok(values) => self.replace_pixel_selection_admitted(values, space),
            Err(error) => {self.status = error;false},
        }
    }

    /// Install authored float coverage without an intermediate byte mask.
    /// Args: coverage and placement. Returns false without mutation on invalid data or admission failure.
    pub(crate) fn replace_pixel_selection_float(&mut self,values:Vec<f32>,space:SelectionSpace)->bool{
        if let Err(error)=crate::selection_coverage::validate(&values){self.status=error;return false;}
        match crate::deep_color::storage::adopt(values){Ok(values)=>self.replace_pixel_selection_admitted(values,space),Err(error)=>{self.status=error;false}}
    }
    /// Retain a complete admitted selection without copying or releasing its owner.
    /// Args: coverage and its coordinate system. Returns whether dimensions were accepted.
    pub(crate) fn replace_pixel_selection_admitted(&mut self, values: std::sync::Arc<crate::resources::Allocation<f32>>, space: SelectionSpace) -> bool {
        if crate::deep_color::storage::count(space.w,space.h,1).ok()!=Some(values.len()) || !crate::channels::valid_placement(space.transform) {
            self.status="Selection dimensions or placement changed".into();return false;
        }
        if let Err(error)=crate::selection_coverage::validate(&values){self.status=error;return false;}
        self.pixel_sel = Some(values);
        self.pixel_sel_space = Some(space);
        self.pixel_sel_gen = self.pixel_sel_gen.wrapping_add(1);
        self.pixel_sel_cache.clear();
        self.status = format!(
            "{} pixels selected",
            crate::paint::selected_count(self.pixel_sel.as_ref().unwrap())
        );
        true
    }
}

/// Preserve the native mask coordinate system, including placed/rotated rasters.
/// Resample once on Apply, always previewing from the original coverage.
pub fn edit<T:Sample>(
    values: &[T],
    space: SelectionSpace,
    operation: Edit,
) -> Option<(Vec<T>, SelectionSpace)> {
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
fn coverage<T:Sample>(input: &[T], w: usize, h: usize, radius: usize, operation: Edit) -> Vec<T> {
    if radius == 0 {
        return input.to_vec();
    }
    let mut horizontal = vec![T::default(); input.len()];
    let mut result = vec![T::default(); input.len()];
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

fn line<T:Sample>(input: impl Iterator<Item = T>, radius: usize, operation: Edit) -> Vec<T> {
    let values: Vec<_> = input.collect();
    let n = values.len();
    let window = radius * 2 + 1;
    let mut result = Vec::with_capacity(n);
    let mut queue: VecDeque<(usize, T)> = VecDeque::new();
    let mut sum = 0f64;
    for i in 0..n + radius * 2 {
        let value = i
            .checked_sub(radius)
            .and_then(|j| values.get(j))
            .copied()
            .unwrap_or_default();
        if matches!(operation, Edit::Feather { .. }) {
            sum += value.raw();
            if i >= window {
                sum -= (i - window)
                    .checked_sub(radius)
                    .and_then(|j| values.get(j))
                    .copied()
                    .unwrap_or_default().raw();
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
                T::average((sum/window as f64).max(0.))
            } else {
                queue.front().unwrap().1
            });
        }
    }
    result
}

/// Modify full-depth selection coverage with admitted buffers and bounded line scratch.
/// Args: original coverage, placement and operation. Returns its unchanged grid or no nonempty selection.
pub fn edit_admitted(values:&[f32],space:SelectionSpace,operation:Edit)->Result<Option<(std::sync::Arc<crate::resources::Allocation<f32>>,SelectionSpace)>,String>{
    use crate::{deep_color::storage,resources::{self,CancellationCheck}};
    if values.len()!=storage::count(space.w,space.h,1)?{return Err("Selection dimensions changed".into());}crate::selection_coverage::validate(values)?;
    if crate::paint::selection_bounds(values,space.w,space.h).is_none(){return Ok(None);}
    let (Edit::Grow{radius}|Edit::Shrink{radius}|Edit::Feather{radius})=operation;let radius=radius.min(1024)as usize;
    let pool=resources::global();if radius==0{return pool.copy_now(values,&storage::check).map(|v|Some((std::sync::Arc::new(v),space)));}
    let mut horizontal=pool.allocate_now::<f32,_>(values.len(),&storage::check)?;
    let mut output=pool.allocate_now::<f32,_>(values.len(),&storage::check)?;
    let scratch=(space.w.max(space.h)as usize).checked_mul(8).and_then(|v|v.checked_add(65536)).ok_or("Selection scratch is too large")?;
    let _scratch=pool.reserve_now(scratch as u64,&storage::check)?;
    let (w,h)=(space.w as usize,space.h as usize);
    for y in 0..h{storage::check.check()?;let row=line((0..w).map(|x|values[y*w+x]),radius,operation);horizontal.as_mut_slice()[y*w..(y+1)*w].copy_from_slice(&row);}
    for x in 0..w{storage::check.check()?;let column=line((0..h).map(|y|horizontal[y*w+x]),radius,operation);for(y,v)in column.into_iter().enumerate(){output.as_mut_slice()[y*w+x]=v;}}
    Ok(Some((std::sync::Arc::new(output),space)))
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
            line([32u8, 200, 64].into_iter(), 1, Edit::Grow { radius: 1 }),
            [200, 200, 200]
        );
        assert_eq!(
            line([32, 200, 64].into_iter(), 1, Edit::Shrink { radius: 1 }),
            [0, 32, 0]
        );
        assert_eq!(
            line([255u8, 255, 255].into_iter(), 1, Edit::Feather { radius: 1 }),
            [170, 255, 170]
        );
    }

    #[test]
    fn pixel_menu_actions_leave_artwork_unchanged_and_keep_empty_masks() {
        let mut studio = Studio::new();
        studio.doc = crate::document::Document::new("Selections", 24.0, 20.0, 96.0);
        studio.doc.layers = vec![crate::document::Layer::raster("Pixels", 24, 20).unwrap()];
        studio.active_layer = Some(0);
        let original = studio.doc.layers[0].clone();
        studio.select_all_pixels();
        assert_eq!(
            paint::selected_count(studio.pixel_sel.as_ref().unwrap()),
            480
        );
        studio.invert_pixel_selection();
        assert!(studio.pixel_sel.as_ref().unwrap().iter().all(|v| *v == 0.));
        studio.invert_pixel_selection();
        assert!(studio.pixel_sel.as_ref().unwrap().iter().all(|v| *v == 1.));
        assert_eq!(
            studio.doc.layers[0].kind.pixels().unwrap().data.as_slice(),
            original.kind.pixels().unwrap().data.as_slice()
        );
    }
}
