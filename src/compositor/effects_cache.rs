//! Bounded, derived effect pixels shared by successive draws on one render thread.
//! Keys own the exact cached path and paint inputs, never a document or its masks.
use super::*;
use std::{cell::RefCell, sync::Arc};

const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_ENTRIES: usize = 256;

struct Entry {
    path: Arc<tiny_skia::Path>,
    style: crate::document::Style,
    image: Option<crate::layout_images::ImageFill>,
    filters: crate::filter::FilterStack,
    pose: Pose,
    bounds: crate::geom::Bounds,
    pixels: Arc<Pixmap>,
    bytes: usize,
    touched: u64,
}

#[derive(Default)]
struct Cache {
    entries: HashMap<usize, Entry>,
    bytes: usize,
    clock: u64,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

pub(super) fn render(
    shape: &Shape,
    mut pose: Pose,
    bounds: crate::geom::Bounds,
    draw: impl FnOnce() -> Option<Pixmap>,
) -> Option<Arc<Pixmap>> {
    // The native effect image is always opaque-pose content; object opacity is
    // applied during the final canvas composite and cannot invalidate its pixels.
    pose.opacity = Some(1.0);
    let Some(path) = shape.get_cached_path(96) else {
        return draw().map(Arc::new);
    };
    let key = Arc::as_ptr(&path) as usize;
    let hit = CACHE.with_borrow_mut(|cache| {
        cache.clock = cache.clock.wrapping_add(1);
        let entry = cache.entries.get_mut(&key)?;
        if Arc::ptr_eq(&entry.path, &path)
            && entry.style == shape.style
            && entry.image == shape.layout.image
            && entry.filters == shape.filters
            && entry.pose == pose
            && entry.bounds == bounds
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
        while cache.bytes + bytes > MAX_BYTES || cache.entries.len() >= MAX_ENTRIES {
            let Some(oldest) = cache
                .entries
                .iter()
                .min_by_key(|(_, e)| e.touched)
                .map(|(key, _)| *key)
            else {
                break;
            };
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
                pose,
                bounds,
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
        for _ in 0..MAX_ENTRIES {
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
