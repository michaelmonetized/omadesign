//! Channel adjustments keep the immutable document and selection captured at open.
use super::{
    Studio,
    channels::Target,
    masking::{self, SelectionSpace},
};
use crate::{
    channels::{
        Plane,
        curve::{self, Curve},
    },
    deep_color::storage,
    resources::{Allocation, CancellationCheck},
};
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct Captured {
    target: Target,
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) source: Plane,
    selection: Option<(Arc<Allocation<f32>>, SelectionSpace)>,
}
impl Captured {
    /// Capture one editable saved channel and its actual selection before preview work.
    /// Args: current editor. Returns an immutable owner or a busy, damaged or unsupported-source error.
    pub(crate) fn new(studio: &mut Studio) -> Result<Self, String> {
        studio.end_pixel_stroke(false);
        let target = studio.capture_channels()?;
        if studio.doc.collaboration.is_some() {
            return Err("Saved channel editing is unavailable in Live".into());
        }
        let channel = studio
            .channel_paint_target()
            .ok_or("Select a saved alpha channel to adjust")?;
        channel.plane.validate()?;
        let selection = match studio.pixel_sel.as_ref() {
            Some(values) => Some((
                values.clone(),
                studio
                    .pixel_selection_space()
                    .ok_or("Clear the damaged pixel selection before adjusting this channel")?,
            )),
            None => None,
        };
        Ok(Self {
            target,
            id: channel.id,
            name: channel.name.clone(),
            source: channel.plane.clone(),
            selection,
        })
    }
    /// Test the captured source immediately before publishing a preview or edit.
    /// Args: current editor. Returns true only for the same document, generation, selection and scalar allocation.
    pub(crate) fn current(&self, studio: &Studio) -> bool {
        studio.channels_current(&self.target)
            && studio.channel_paint_target().is_some_and(|c| {
                c.id == self.id && Arc::ptr_eq(&c.plane.samples, &self.source.samples)
            })
    }
    /// Render a bounded proxy or full native adjustment without changing the document.
    /// Args: validated curve, optional preview edge and cancellation. Returns a plane retaining its same document placement.
    pub(crate) fn render(
        &self,
        curve: &Curve,
        edge: Option<u32>,
        cancel: &dyn CancellationCheck,
    ) -> Result<Plane, String> {
        cancel.check()?;
        let (source, selection) = if let Some(edge) = edge {
            let scale = (f64::from(edge.max(1))
                / f64::from(self.source.width.max(self.source.height)))
            .min(1.);
            let width = (f64::from(self.source.width) * scale).round().max(1.) as u32;
            let height = (f64::from(self.source.height) * scale).round().max(1.) as u32;
            let n = storage::count(width, height, 1)?;
            let pool = crate::resources::global();
            let mut values = pool.allocate_now::<f32, _>(n, cancel)?;
            let mut mask = self
                .selection
                .as_ref()
                .map(|_| pool.allocate_now::<f32, _>(n, cancel))
                .transpose()?;
            let inverse = self
                .selection
                .as_ref()
                .map(|(_, space)| {
                    space
                        .transform
                        .invert()
                        .ok_or("Selection placement is singular")
                })
                .transpose()?;
            for y in 0..height {
                cancel.check()?;
                for x in 0..width {
                    let sx = u64::from(x) * u64::from(self.source.width) / u64::from(width);
                    let sy = u64::from(y) * u64::from(self.source.height) / u64::from(height);
                    let i = (y * width + x) as usize;
                    values.as_mut_slice()[i] =
                        self.source.samples[sy as usize * self.source.width as usize + sx as usize];
                    if let Some(mask) = mask.as_mut() {
                        let (src, space) = self.selection.as_ref().unwrap();
                        let mut point = tiny_skia::Point::from_xy(sx as f32 + 0.5, sy as f32 + 0.5);
                        self.source.placement().map_point(&mut point);
                        inverse.unwrap().map_point(&mut point);
                        mask.as_mut_slice()[i] = if point.x >= 0.
                            && point.y >= 0.
                            && point.x < space.w as f32
                            && point.y < space.h as f32
                        {
                            src[point.y.floor() as usize * space.w as usize
                                + point.x.floor() as usize]
                        } else {
                            0.
                        };
                    }
                }
            }
            let placement = self.source.placement().pre_scale(
                self.source.width as f32 / width as f32,
                self.source.height as f32 / height as f32,
            );
            (
                Plane {
                    width,
                    height,
                    transform: [
                        placement.sx,
                        placement.ky,
                        placement.kx,
                        placement.sy,
                        placement.tx,
                        placement.ty,
                    ],
                    samples: Arc::new(values),
                },
                mask.map(Arc::new),
            )
        } else {
            let target = SelectionSpace {
                w: self.source.width,
                h: self.source.height,
                transform: self.source.placement(),
            };
            let selection = self
                .selection
                .as_ref()
                .map(|(values, space)| {
                    if *space == target {
                        Ok(values.clone())
                    } else {
                        masking::resample_admitted(values, *space, target)
                    }
                })
                .transpose()?;
            cancel.check()?;
            (self.source.clone(), selection)
        };
        curve::apply(
            &source,
            curve,
            selection.as_ref().map(|m| m.as_slice()),
            cancel,
        )
    }
    /// Commit a completed full-resolution result through the sole channel command.
    /// Args: current editor and produced plane. Returns before mutation if its original owner changed.
    pub(crate) fn commit(&self, studio: &mut Studio, plane: Plane) -> Result<bool, String> {
        if !self.current(studio) {
            return Err("The channel or selection changed; reopen Curves".into());
        }
        if plane.width != self.source.width
            || plane.height != self.source.height
            || plane.transform != self.source.transform
        {
            return Err("Channel curve output placement changed".into());
        }
        studio.replace_scalar_plane(self.id, plane, "Channel curves applied")
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn fixture() -> Studio {
        let mut s = Studio::new();
        s.show_welcome = false;
        s.doc = crate::channels::tests::fixture();
        let plane = Plane {
            width: 8,
            height: 4,
            transform: [1., 0., 0., 1., 10., 20.],
            samples: storage::collect((0..32).map(|i| if i == 0 { -0. } else { i as f32 / 32. }))
                .unwrap(),
        };
        let target = s.capture_channels().unwrap();
        let id = s.save_channel(&target, plane, "Curve alpha").unwrap();
        s.select_channel_target(Some(id)).unwrap();
        s
    }
    #[test]
    fn craft_channel_curves_preview_apply_one_undo_and_full_native_reopen() {
        let mut s = fixture();
        let before = crate::project::encode(&s.doc).unwrap();
        let n = s.history.len();
        let owner = Captured::new(&mut s).unwrap();
        let curve = Curve::Points(vec![[0., 0.], [0.5, 1.], [1., 0.]]);
        let proxy = owner.render(&curve, Some(2), &storage::check).unwrap();
        assert_eq!((proxy.width, proxy.height), (2, 1));
        assert_eq!(proxy.transform, [4., 0., 0., 4., 10., 20.]);
        assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
        assert_eq!(s.history.len(), n);
        let result = owner.render(&curve, None, &storage::check).unwrap();
        for (i, value) in result.samples.iter().enumerate() {
            let x = i as f32 / 32.;
            let z = if x <= 0.5 { x } else { 1. - x };
            assert!((*value - (3. * z - 4. * z * z * z)).abs() < 2e-7);
        }
        owner.commit(&mut s, result).unwrap();
        assert_eq!(s.history.len(), n + 1);
        let after = crate::project::encode(&s.doc).unwrap();
        assert_eq!(
            crate::project::encode(&crate::project::decode(&after).unwrap()).unwrap(),
            after
        );
        s.undo();
        assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
        s.redo();
        assert_eq!(crate::project::encode(&s.doc).unwrap(), after);
        if let Some(root) = std::env::var_os("OMADESIGN_CHANNEL_CURVES_QA") {
            let root = std::path::PathBuf::from(root);
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(root.join("before.oma"), before).unwrap();
            std::fs::write(root.join("curves.oma"), after).unwrap();
        }
    }
    #[test]
    fn craft_channel_curves_captured_selection_placement_stale_target_and_cancel_are_atomic() {
        let mut s = fixture();
        s.replace_pixel_selection(
            vec![128; 32],
            SelectionSpace {
                w: 8,
                h: 4,
                transform: tiny_skia::Transform::from_translate(10., 20.),
            },
        );
        let owner = Captured::new(&mut s).unwrap();
        let curve = Curve::Points(vec![[0., 1.], [1., 0.]]);
        let output = owner.render(&curve, None, &storage::check).unwrap();
        for (before, value) in owner.source.samples.iter().zip(output.samples.iter()) {
            let expected = *before + (1. - *before - *before) * (128. / 255.);
            assert_eq!(value.to_bits(), expected.to_bits());
        }
        let original = crate::project::encode(&s.doc).unwrap();
        s.pixel_sel_gen += 1;
        assert!(owner.commit(&mut s, output).is_err());
        assert_eq!(crate::project::encode(&s.doc).unwrap(), original);
        assert!(
            owner
                .render(&curve, None, &|| Err("Cancelled".into()))
                .is_err()
        );
        let next = Captured::new(&mut s).unwrap();
        s.select_channel_target(None).unwrap();
        assert!(!next.current(&s));
        s.select_channel_target(Some(next.id)).unwrap();
        s.swap_id.push_str("-next");
        assert!(!next.current(&s));
        assert_eq!(crate::project::encode(&s.doc).unwrap(), original);
    }
}
