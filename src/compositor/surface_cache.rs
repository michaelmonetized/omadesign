//! Reuse exact legacy layer/group filters after their children have been drawn.
//! Keeping both the input bytes and result avoids depending on document IDs,
//! mutation counters, motion tracks, hierarchy, or a particular view transform.
use crate::filter::FilterStack;
use std::{cell::RefCell, collections::HashMap, sync::Arc};
use tiny_skia::Pixmap;

// Includes retained source and output pixels, effect parameters, and entries.
// Independent appearance compositing still runs against its current backdrop.
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 256;

struct Entry {
    source: Vec<u8>,
    filters: FilterStack,
    pixels: Arc<Pixmap>,
    bytes: usize,
    touched: u64,
}

#[derive(Default)]
struct Cache {
    entries: HashMap<u64, Entry>,
    bytes: usize,
    clock: u64,
    admission: super::cache_admission::Admission,
}

impl Cache {
    fn make_room(&mut self, id: u64, bytes: usize) -> bool {
        if let Some(old) = self.entries.remove(&id) {
            self.bytes -= old.bytes;
        }
        if bytes > MAX_BYTES {
            return false;
        }
        let mut victims = Vec::new();
        if self.bytes + bytes > MAX_BYTES || self.entries.len() >= MAX_ENTRIES {
            let frequency = self.admission.frequency(id);
            let mut candidates: Vec<_> = self.entries.iter().collect();
            candidates.sort_unstable_by_key(|(_, entry)| entry.touched);
            let mut retained = self.bytes;
            for (&oldest, entry) in candidates {
                if frequency <= self.admission.frequency(oldest) {
                    return false;
                }
                victims.push(oldest);
                retained -= entry.bytes;
                if retained + bytes <= MAX_BYTES && self.entries.len() - victims.len() < MAX_ENTRIES
                {
                    break;
                }
            }
        }
        for oldest in victims {
            if let Some(old) = self.entries.remove(&oldest) {
                self.bytes -= old.bytes;
            }
        }
        true
    }
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

pub(super) fn render(id: u64, mut source: Pixmap, filters: &FilterStack) -> Arc<Pixmap> {
    let hit = CACHE.with_borrow_mut(|cache| {
        cache.clock = cache.clock.wrapping_add(1);
        cache.admission.record(id);
        let entry = cache.entries.get_mut(&id)?;
        if entry.pixels.width() == source.width()
            && entry.pixels.height() == source.height()
            && entry.filters == *filters
            && entry.source == source.data()
        {
            entry.touched = cache.clock;
            Some(entry.pixels.clone())
        } else {
            None
        }
    });
    if let Some(hit) = hit {
        return hit;
    }
    let bytes = source.data().len() * 2
        + filters.items.len() * std::mem::size_of::<crate::filter::Fx>()
        + std::mem::size_of::<Entry>();
    // Decide admission before copying the full source. Oversized viewports and
    // scan misses rejected by the working set need no second unfiltered image.
    let keep = CACHE.with_borrow_mut(|cache| cache.make_room(id, bytes));
    let input = keep.then(|| source.data().to_vec());
    crate::filter::apply(&mut source, filters);
    let pixels = Arc::new(source);
    CACHE.with_borrow_mut(|cache| {
        let Some(source) = input else {
            return;
        };
        // No borrow spans filtering. Recheck the bound in case a future filter
        // implementation renders nested surfaces while generating this result.
        if !cache.make_room(id, bytes) {
            return;
        }
        cache.bytes += bytes;
        cache.entries.insert(
            id,
            Entry {
                source,
                filters: filters.clone(),
                pixels: pixels.clone(),
                bytes,
                touched: cache.clock,
            },
        );
    });
    pixels
}

#[cfg(test)]
pub(super) fn clear() {
    CACHE.with_borrow_mut(|cache| *cache = Cache::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{color::Rgba, filter::Fx};

    fn filters() -> FilterStack {
        FilterStack {
            enabled: true,
            legacy_composite: true,
            items: vec![Fx::Blur { std: 1.7 }],
        }
    }

    fn source(width: u32, height: u32) -> Pixmap {
        let mut pixels = Pixmap::new(width, height).unwrap();
        pixels.fill(Rgba::new(120, 52, 217, 199).to_skia());
        pixels.data_mut()[0..4].copy_from_slice(&[0, 0, 0, 0]);
        pixels
    }

    fn assert_fresh(id: u64, source: Pixmap, filters: &FilterStack) -> Arc<Pixmap> {
        let mut expected = source.clone();
        crate::filter::apply(&mut expected, filters);
        let actual = render(id, source, filters);
        assert_eq!(actual.data(), expected.data());
        actual
    }

    #[test]
    fn exact_source_reuses_pixels_and_all_filter_inputs_invalidate() {
        clear();
        let filters = filters();
        let first = assert_fresh(7, source(33, 24), &filters);
        let repeated = render(7, source(33, 24), &filters);
        assert!(Arc::ptr_eq(&first, &repeated));
        let mut changed = source(33, 24);
        changed.data_mut()[4..8].copy_from_slice(&[0, 0, 0, 0]);
        let changed = assert_fresh(7, changed, &filters);
        assert!(!Arc::ptr_eq(&first, &changed));
        let resized = assert_fresh(7, source(24, 33), &filters);
        assert!(!Arc::ptr_eq(&changed, &resized));
        let mut changed_filters = filters.clone();
        changed_filters.items[0] = Fx::Blur { std: 2.8 };
        let restyled = assert_fresh(7, source(24, 33), &changed_filters);
        assert!(!Arc::ptr_eq(&resized, &restyled));
        // Reusing a layer/group ID in another tab still validates exact pixels.
        assert_fresh(7, source(33, 24), &filters);
        clear();
    }

    #[test]
    fn oversized_source_is_exact_and_is_not_retained() {
        clear();
        let old = render(3, source(8, 8), &FilterStack::default());
        let previous = Arc::downgrade(&old);
        drop(old);
        let large = render(3, source(4096, 2048), &FilterStack::default());
        assert_eq!(large.data(), source(4096, 2048).data());
        assert!(previous.upgrade().is_none());
        CACHE.with_borrow(|cache| {
            assert!(cache.entries.is_empty());
            assert_eq!(cache.bytes, 0);
        });
    }

    #[test]
    fn above_budget_sequential_scene_keeps_a_useful_subset() {
        clear();
        let mut previous = Vec::new();
        for id in 0..40 {
            previous.push(Arc::downgrade(&render(
                id,
                source(512, 512),
                &FilterStack::default(),
            )));
        }
        let mut hits = 0;
        for (id, old) in previous.iter().enumerate() {
            let previous = old.upgrade();
            let pixels = render(id as u64, source(512, 512), &FilterStack::default());
            hits += usize::from(previous.is_some_and(|previous| Arc::ptr_eq(&previous, &pixels)));
        }
        assert!(
            hits >= 20,
            "a sequential over-budget traversal must retain useful filtered surfaces"
        );
        CACHE.with_borrow(|cache| {
            assert!(cache.bytes <= MAX_BYTES);
            assert!(cache.entries.len() <= MAX_ENTRIES);
            assert_eq!(
                cache.bytes,
                cache
                    .entries
                    .values()
                    .map(|entry| entry.bytes)
                    .sum::<usize>()
            );
        });
        clear();
    }
}
