//! Reuse exact transformed object masks without retaining their documents.
//! Source bytes are the premultiplied pixels used by the renderer. Comparing
//! them makes object-id and allocation reuse safe across replacement and tabs.
use super::*;
use std::{cell::RefCell, sync::Arc};

const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_ENTRIES: usize = 256;

struct Entry {
    key: [u32; 10],
    source: Vec<u8>,
    mask: Arc<tiny_skia::Mask>,
    bytes: usize,
    touched: u64,
}

#[derive(Default)]
struct Cache {
    entries: HashMap<u64, Entry>,
    bytes: usize,
    clock: u64,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

fn entry_bytes(source_len: usize, width: u32, height: u32) -> Option<usize> {
    let mask_len = (width as usize).checked_mul(height as usize)?;
    let bytes = source_len
        .checked_add(mask_len)?
        .checked_add(std::mem::size_of::<Entry>())?;
    // Comparing a large source on every hit can cost more than rebuilding a
    // small viewport mask. Reject it before retaining or cloning any pixels.
    (source_len <= mask_len && bytes <= MAX_BYTES).then_some(bytes)
}

impl Cache {
    fn remove(&mut self, id: u64) {
        if let Some(old) = self.entries.remove(&id) {
            self.bytes -= old.bytes;
        }
    }

    fn insert(
        &mut self,
        id: u64,
        key: [u32; 10],
        source: &Pixmap,
        mask: Arc<tiny_skia::Mask>,
        bytes: usize,
    ) {
        self.remove(id);
        while self.bytes + bytes > MAX_BYTES || self.entries.len() >= MAX_ENTRIES {
            let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.touched)
                .map(|(&id, _)| id)
            else {
                break;
            };
            self.remove(oldest);
        }
        self.entries.insert(
            id,
            Entry {
                key,
                source: source.data().to_vec(),
                mask,
                bytes,
                touched: self.clock,
            },
        );
        self.bytes += bytes;
    }
}

pub(super) fn render(
    id: u64,
    source: &Pixmap,
    width: u32,
    height: u32,
    transform: Transform,
) -> Option<Arc<tiny_skia::Mask>> {
    let Some(bytes) = entry_bytes(source.data().len(), width, height) else {
        CACHE.with_borrow_mut(|cache| cache.remove(id));
        return render_uncached(source, width, height, transform).map(Arc::new);
    };
    let key = [
        source.width(),
        source.height(),
        width,
        height,
        transform.sx.to_bits(),
        transform.kx.to_bits(),
        transform.ky.to_bits(),
        transform.sy.to_bits(),
        transform.tx.to_bits(),
        transform.ty.to_bits(),
    ];
    let hit = CACHE.with_borrow_mut(|cache| {
        cache.clock = cache.clock.wrapping_add(1);
        let entry = cache.entries.get_mut(&id)?;
        if entry.key == key && entry.source == source.data() {
            entry.touched = cache.clock;
            Some(entry.mask.clone())
        } else {
            None
        }
    });
    if hit.is_some() {
        return hit;
    }
    // Keep the original full-viewport transform and luminance conversion on a
    // miss. No cropped coordinate system can change bilinear rounding.
    let mask = Arc::new(render_uncached(source, width, height, transform)?);
    CACHE.with_borrow_mut(|cache| cache.insert(id, key, source, mask.clone(), bytes));
    Some(mask)
}

fn render_uncached(
    source: &Pixmap,
    width: u32,
    height: u32,
    transform: Transform,
) -> Option<tiny_skia::Mask> {
    let mut placed = Pixmap::new(width, height)?;
    placed.draw_pixmap(
        0,
        0,
        source.as_ref(),
        &PixmapPaint {
            quality: tiny_skia::FilterQuality::Bilinear,
            ..Default::default()
        },
        transform,
        None,
    );
    Some(tiny_skia::Mask::from_pixmap(
        placed.as_ref(),
        tiny_skia::MaskType::Luminance,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Pixels, Style};

    fn clear() {
        CACHE.with_borrow_mut(|cache| *cache = Cache::default());
    }

    fn shape() -> Shape {
        let mut shape = Shape::new(
            Geom::Rect {
                origin: Pt::new(11.25, 8.5),
                size: Pt::new(57.75, 43.5),
                radius: 4.0,
            },
            Style::default(),
        );
        shape.rotation = 0.23;
        let data = (0..8 * 6)
            .flat_map(|i| {
                [
                    (i * 37) as u8,
                    (i * 19) as u8,
                    (i * 71) as u8,
                    (i * 13) as u8,
                ]
            })
            .collect();
        shape.mask = Pixels::from_rgba(8, 6, data);
        shape
    }

    // Independent copy of the original object_mask algorithm, including the
    // current parent intersection. It must not call the cache or its miss path.
    fn reference(
        pm: &Pixmap,
        shape: &Shape,
        t: Transform,
        pose: Pose,
        parent: Option<&tiny_skia::Mask>,
    ) -> tiny_skia::Mask {
        let mut placed = Pixmap::new(pm.width(), pm.height()).unwrap();
        let transform = t
            .pre_concat(pose.to_skia(shape.world_bbox().center()))
            .pre_concat(shape_mask_transform(shape));
        shape
            .mask
            .as_ref()
            .unwrap()
            .with_pm(|source| {
                placed.draw_pixmap(
                    0,
                    0,
                    source.as_ref(),
                    &PixmapPaint {
                        quality: tiny_skia::FilterQuality::Bilinear,
                        ..Default::default()
                    },
                    transform,
                    None,
                );
            })
            .unwrap();
        let mut mask =
            tiny_skia::Mask::from_pixmap(placed.as_ref(), tiny_skia::MaskType::Luminance);
        if let Some(parent) = parent {
            for (a, b) in mask.data_mut().iter_mut().zip(parent.data()) {
                *a = ((u16::from(*a) * u16::from(*b) + 127) / 255) as u8;
            }
        }
        mask
    }

    #[test]
    fn cache_matches_original_mask_at_fractional_transforms_and_changed_viewports() {
        clear();
        let shape = shape();
        let mut previous = None;
        for (width, height) in [(128, 96), (141, 109)] {
            let pm = Pixmap::new(width, height).unwrap();
            for transform in [
                Transform::identity(),
                Transform::from_row(1.17, 0.13, -0.19, 0.83, -7.35, 13.65),
                Transform::from_row(-0.92, 0.2, 0.1, 1.43, 101.25, -19.5),
            ] {
                for pose in [
                    Pose::identity(),
                    Pose {
                        dx: 3.75,
                        dy: -8.25,
                        rotation: 0.32,
                        scale: 1.12,
                        width_scale: 0.81,
                        height_scale: 1.21,
                        ..Pose::identity()
                    },
                ] {
                    let expected = reference(&pm, &shape, transform, pose, None);
                    let actual = object_mask(&pm, &shape, transform, pose, None).unwrap();
                    assert_eq!(actual.data(), expected.data());
                    let hit = object_mask(&pm, &shape, transform, pose, None).unwrap();
                    assert!(
                        Arc::ptr_eq(&actual, &hit),
                        "a hit must share, not copy, the viewport mask"
                    );
                    if let Some(previous) = previous {
                        assert!(!Arc::ptr_eq(&actual, &previous));
                    }
                    previous = Some(actual);
                }
            }
        }
    }

    #[test]
    fn source_replacement_touch_and_actual_premultiplied_bytes_invalidate_safely() {
        clear();
        let mut shape = shape();
        let pm = Pixmap::new(128, 96).unwrap();
        let draw = |shape: &Shape| {
            object_mask(&pm, shape, Transform::identity(), Pose::identity(), None).unwrap()
        };
        let first = draw(&shape);
        let version = shape.mask.as_ref().unwrap().version;
        shape.mask = Pixels::from_rgba(8, 6, [240, 190, 60, 180].repeat(48));
        assert_eq!(shape.mask.as_ref().unwrap().version, version);
        let replaced = draw(&shape);
        assert!(!Arc::ptr_eq(&first, &replaced));
        assert_ne!(first.data(), replaced.data());
        // Two objects/documents can share a persisted id and version. Content
        // comparison must still distinguish them, without relying on addresses.
        let mut other = shape.clone();
        other.mask = Pixels::from_rgba(8, 6, [15, 25, 35, 255].repeat(48));
        let other_mask = draw(&other);
        assert_ne!(replaced.data(), other_mask.data());
        let px = shape.mask.as_mut().unwrap();
        px.data[..4].copy_from_slice(&[0, 0, 0, 255]);
        px.touch();
        let touched = draw(&shape);
        assert_eq!(
            touched.data(),
            reference(&pm, &shape, Transform::identity(), Pose::identity(), None).data()
        );
        // Validate the bytes actually supplied by with_pm, not only stored RGBA
        // or its version: retained premultiplied state is the render authority.
        let px = shape.mask.as_mut().unwrap();
        let raw = px.data.clone();
        let version = px.version;
        px.cached_pm
            .get_mut()
            .as_mut()
            .unwrap()
            .1
            .fill(tiny_skia::Color::WHITE);
        assert_eq!(px.data, raw);
        assert_eq!(px.version, version);
        let changed_pm = draw(&shape);
        assert!(!Arc::ptr_eq(&touched, &changed_pm));
        assert_ne!(touched.data(), changed_pm.data());
        assert_eq!(
            changed_pm.data(),
            reference(&pm, &shape, Transform::identity(), Pose::identity(), None).data()
        );
        // Identical byte lengths with different source dimensions are distinct.
        shape.mask = Pixels::from_rgba(12, 4, [255; 4].repeat(48));
        let resized = draw(&shape);
        assert!(!Arc::ptr_eq(&changed_pm, &resized));
        assert_eq!(
            resized.data(),
            reference(&pm, &shape, Transform::identity(), Pose::identity(), None).data()
        );
    }

    #[test]
    fn parent_changes_are_applied_without_modifying_cached_own_mask() {
        clear();
        let shape = shape();
        let pm = Pixmap::new(128, 96).unwrap();
        let transform = Transform::from_translate(0.35, -1.75);
        let own = object_mask(&pm, &shape, transform, Pose::identity(), None).unwrap();
        let original = own.data().to_vec();
        let mut parent = tiny_skia::Mask::new(128, 96).unwrap();
        for amount in [255u8, 87, 0, 193] {
            parent.data_mut().fill(amount);
            let combined =
                object_mask(&pm, &shape, transform, Pose::identity(), Some(&parent)).unwrap();
            assert_eq!(
                combined.data(),
                reference(&pm, &shape, transform, Pose::identity(), Some(&parent)).data()
            );
            assert_eq!(own.data(), original);
            assert!(Arc::ptr_eq(
                &own,
                &object_mask(&pm, &shape, transform, Pose::identity(), None).unwrap()
            ));
        }
    }

    #[test]
    fn cache_evicts_by_entry_and_byte_limits_and_rejects_large_sources() {
        clear();
        let source = Pixmap::new(2, 2).unwrap();
        let first = render(0, &source, 8, 8, Transform::identity()).unwrap();
        let expired = Arc::downgrade(&first);
        drop(first);
        for id in 1..=MAX_ENTRIES as u64 {
            render(id, &source, 8, 8, Transform::identity()).unwrap();
        }
        assert!(expired.upgrade().is_none());
        CACHE.with_borrow(|cache| {
            assert_eq!(cache.entries.len(), MAX_ENTRIES);
            assert_eq!(
                cache.bytes,
                cache
                    .entries
                    .values()
                    .map(|entry| entry.bytes)
                    .sum::<usize>()
            );
            assert!(cache.bytes <= MAX_BYTES);
        });
        let mut cache = Cache::default();
        let bytes = entry_bytes(source.data().len(), 2048, 2048).unwrap();
        let first = Arc::new(tiny_skia::Mask::new(2048, 2048).unwrap());
        let expired = Arc::downgrade(&first);
        cache.insert(0, [0; 10], &source, first, bytes);
        for id in 1..10 {
            cache.clock += 1;
            cache.insert(
                id,
                [0; 10],
                &source,
                Arc::new(tiny_skia::Mask::new(2048, 2048).unwrap()),
                bytes,
            );
        }
        assert!(expired.upgrade().is_none());
        assert!(cache.bytes <= MAX_BYTES);
        assert_eq!(
            cache.bytes,
            cache
                .entries
                .values()
                .map(|entry| entry.bytes)
                .sum::<usize>()
        );
        assert_eq!(
            entry_bytes(16, 8192, 4096),
            None,
            "oversized masks are never admitted"
        );
        let large_source = Pixmap::new(8, 8).unwrap();
        assert_eq!(entry_bytes(large_source.data().len(), 8, 8), None);
        let uncached = render(1, &large_source, 8, 8, Transform::identity()).unwrap();
        assert_eq!(
            uncached.data(),
            render_uncached(&large_source, 8, 8, Transform::identity())
                .unwrap()
                .data()
        );
        CACHE.with_borrow(|cache| assert!(!cache.entries.contains_key(&1)));
    }
}
