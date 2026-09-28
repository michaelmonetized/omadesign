//! Bounded, derived effect pixels shared by successive draws on one render thread.
//! Keys own the exact cached path and paint inputs, never a document or its masks.
use super::*;
use std::{cell::RefCell, sync::Arc};

// A native 1920px effect-heavy composition can exceed 32 MiB without a single
// oversized object. Keep that working set resident rather than evicting every
// entry during each sequential playback traversal.
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 256;

#[derive(PartialEq)]
struct RasterKey {
    transform: [u32; 6],
    size: [u32; 2],
    stroke_reveal: Option<f32>,
    fill_reveal: Option<f32>,
}

impl RasterKey {
    fn new(shape: &Shape, mut pose: Pose, bounds: crate::geom::Bounds) -> Self {
        pose.opacity = Some(1.0);
        // Match draw_shape_inner's actual floating-point arithmetic, including
        // cancellation of animated translation against the local image origin.
        // Only reuse when the resulting matrix bits really are identical.
        let local = Transform::from_translate(-bounds.min.x, -bounds.min.y);
        let transform = local.pre_concat(pose.to_skia(shape.world_bbox().center()));
        Self {
            transform: [
                transform.sx.to_bits(),
                transform.kx.to_bits(),
                transform.ky.to_bits(),
                transform.sy.to_bits(),
                transform.tx.to_bits(),
                transform.ty.to_bits(),
            ],
            size: [
                bounds.width().ceil().max(1.0) as u32,
                bounds.height().ceil().max(1.0) as u32,
            ],
            stroke_reveal: pose.stroke_reveal,
            fill_reveal: pose.fill_reveal,
        }
    }
}

struct Entry {
    path: Arc<tiny_skia::Path>,
    style: crate::document::Style,
    image: Option<crate::layout_images::ImageFill>,
    filters: crate::filter::FilterStack,
    raster: RasterKey,
    pixels: Arc<Pixmap>,
    bytes: usize,
    touched: u64,
}

#[derive(Default)]
struct Cache {
    entries: HashMap<usize, Entry>,
    bytes: usize,
    clock: u64,
    admission: super::cache_admission::Admission,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

pub(super) fn render(
    shape: &Shape,
    pose: Pose,
    bounds: crate::geom::Bounds,
    draw: impl FnOnce() -> Option<Pixmap>,
) -> Option<Arc<Pixmap>> {
    // Opacity and placement on the destination do not invalidate native pixels.
    // Paint changes are already materialized in shape.style by the caller.
    let raster = RasterKey::new(shape, pose, bounds);
    let Some(path) = shape.get_cached_path(96) else {
        return draw().map(Arc::new);
    };
    let key = Arc::as_ptr(&path) as usize;
    let hit = CACHE.with_borrow_mut(|cache| {
        cache.clock = cache.clock.wrapping_add(1);
        cache.admission.record(key as u64);
        let entry = cache.entries.get_mut(&key)?;
        if Arc::ptr_eq(&entry.path, &path)
            && entry.style == shape.style
            && entry.image == shape.layout.image
            && entry.filters == shape.filters
            && entry.raster == raster
        {
            entry.touched = cache.clock;
            Some(entry.pixels.clone())
        } else {
            None
        }
    });
    if hit.is_some() {
        return hit;
    }
    // Do not retain a RefCell borrow while drawing (future render paths can nest).
    let pixels = Arc::new(draw()?);
    let gradient_stops = match &shape.style.fill {
        Fill::Gradient(gradient) => gradient.stops.len(),
        _ => 0,
    } + shape
        .style
        .stroke
        .as_ref()
        .and_then(|stroke| stroke.gradient.as_ref())
        .map_or(0, |gradient| gradient.stops.len());
    let bytes = pixels.data().len()
        + path.points().len() * 2 * (std::mem::size_of::<Point>() + 1)
        + gradient_stops * std::mem::size_of::<crate::gradient::GradientStop>()
        + shape.filters.items.len() * std::mem::size_of::<crate::filter::Fx>()
        + std::mem::size_of::<Entry>()
        + shape
            .layout
            .image
            .as_ref()
            .map_or(0, |image| image.data.len());
    CACHE.with_borrow_mut(|cache| {
        if let Some(old) = cache.entries.remove(&key) {
            cache.bytes -= old.bytes;
        }
        if bytes > MAX_BYTES {
            return;
        }
        let mut victims = Vec::new();
        if cache.bytes + bytes > MAX_BYTES || cache.entries.len() >= MAX_ENTRIES {
            let frequency = cache.admission.frequency(key as u64);
            let mut candidates: Vec<_> = cache.entries.iter().collect();
            candidates.sort_unstable_by_key(|(_, entry)| entry.touched);
            let mut retained = cache.bytes;
            for (&oldest, entry) in candidates {
                // Preserve a useful working subset when a sequential scene is
                // larger than the budget, instead of missing on every object.
                if frequency <= cache.admission.frequency(oldest as u64) {
                    return;
                }
                victims.push(oldest);
                retained -= entry.bytes;
                if retained + bytes <= MAX_BYTES
                    && cache.entries.len() - victims.len() < MAX_ENTRIES
                {
                    break;
                }
            }
        }
        for oldest in victims {
            if let Some(old) = cache.entries.remove(&oldest) {
                cache.bytes -= old.bytes;
            }
        }
        cache.bytes += bytes;
        cache.entries.insert(
            key,
            Entry {
                path,
                style: shape.style.clone(),
                image: shape.layout.image.clone(),
                filters: shape.filters.clone(),
                raster,
                pixels: pixels.clone(),
                bytes,
                touched: cache.clock,
            },
        );
    });
    Some(pixels)
}

#[cfg(test)]
pub(super) fn clear() {
    CACHE.with_borrow_mut(|cache| *cache = Cache::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_local_raster_reuses_fractional_translation_and_opacity() {
        clear();
        let shape = Shape::new(
            Geom::Rect {
                // Keep the local origin nonzero: tiny-skia's identity fast
                // path can preserve -0 while later cancellation produces +0.
                origin: Pt::new(12., 16.),
                size: Pt::new(20., 16.),
                radius: 2.,
            },
            crate::document::Style::default(),
        );
        let mut original = None;
        let mut original_raster = None;
        for step in 0..16 {
            let pose = Pose {
                dx: step as f32 * 0.25,
                dy: -(step as f32) * 0.5,
                opacity: Some(0.1 + step as f32 * 0.05),
                ..Pose::identity()
            };
            let bounds = pose.map_bounds(shape.world_bbox()).inflate(8.);
            let raster = RasterKey::new(&shape, pose, bounds);
            if let Some(previous) = &original_raster {
                assert!(
                    previous == &raster,
                    "fixture must have bit-identical local matrices"
                );
            } else {
                original_raster = Some(raster);
            }
            let current = render(&shape, pose, bounds, || Pixmap::new(36, 32)).unwrap();
            if let Some(previous) = &original {
                assert!(
                    Arc::ptr_eq(previous, &current),
                    "identical native matrix must reuse pixels"
                );
            } else {
                original = Some(current);
            }
        }
        clear();
    }

    #[test]
    fn sequential_effect_scene_retains_its_native_working_set() {
        clear();
        let shapes: Vec<_> = (0..40)
            .map(|index| {
                Shape::new(
                    Geom::Rect {
                        origin: Pt::new(index as f32, 0.),
                        size: Pt::new(20., 16.),
                        radius: 2.,
                    },
                    crate::document::Style::default(),
                )
            })
            .collect();
        let mut previous = Vec::new();
        for shape in &shapes {
            let pixels = render(shape, Pose::identity(), shape.world_bbox(), || {
                Pixmap::new(512, 512)
            })
            .unwrap();
            previous.push(Arc::downgrade(&pixels));
        }
        for (shape, previous) in shapes.iter().zip(previous) {
            let previous = previous
                .upgrade()
                .expect("a 40 MiB scene must remain resident");
            let pixels = render(shape, Pose::identity(), shape.world_bbox(), || {
                panic!("sequential cache thrash")
            })
            .unwrap();
            assert!(Arc::ptr_eq(&previous, &pixels));
        }
        CACHE.with_borrow(|cache| assert!(cache.bytes <= MAX_BYTES));
        clear();
    }

    #[test]
    fn sequential_scene_above_budget_does_not_evict_every_useful_entry() {
        clear();
        let shapes: Vec<_> = (0..80)
            .map(|index| {
                Shape::new(
                    Geom::Rect {
                        origin: Pt::new(index as f32, 0.),
                        size: Pt::new(20., 16.),
                        radius: 2.,
                    },
                    crate::document::Style::default(),
                )
            })
            .collect();
        for shape in &shapes {
            render(shape, Pose::identity(), shape.world_bbox(), || {
                Pixmap::new(512, 512)
            })
            .unwrap();
        }
        let mut misses = 0;
        for shape in &shapes {
            render(shape, Pose::identity(), shape.world_bbox(), || {
                misses += 1;
                Pixmap::new(512, 512)
            })
            .unwrap();
        }
        assert!(
            misses < 40,
            "an over-budget scene must keep a useful working subset, got {misses} misses"
        );
        CACHE.with_borrow(|cache| {
            assert!(cache.bytes <= MAX_BYTES);
            assert!(cache.entries.len() <= MAX_ENTRIES);
        });
        clear();
    }

    #[test]
    fn cache_evicts_old_paths_instead_of_retaining_closed_documents() {
        clear();
        let mut shape = Shape::new(
            Geom::Rect {
                origin: Pt::ZERO,
                size: Pt::new(10.0, 10.0),
                radius: 0.0,
            },
            crate::document::Style::default(),
        );
        let bounds = shape.geom.bbox();
        let first = render(&shape, Pose::identity(), bounds, || Pixmap::new(8, 8)).unwrap();
        let recent = std::sync::Arc::downgrade(&first);
        drop(first);
        for _ in 0..MAX_ENTRIES * 8 {
            shape.geom.translate(Pt::new(1.0, 0.0));
            render(&shape, Pose::identity(), shape.geom.bbox(), || {
                Pixmap::new(8, 8)
            })
            .unwrap();
        }
        assert!(recent.upgrade().is_none());
        CACHE.with_borrow(|cache| {
            assert_eq!(cache.entries.len(), MAX_ENTRIES);
            assert!(cache.bytes <= MAX_BYTES);
            assert_eq!(
                cache.bytes,
                cache.entries.values().map(|e| e.bytes).sum::<usize>()
            );
        });
        clear();
        CACHE.with_borrow(|cache| assert_eq!(cache.bytes, 0));
    }
}
