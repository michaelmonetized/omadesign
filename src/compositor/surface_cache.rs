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
    coverage: Option<Option<tiny_skia::IntRect>>,
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

pub(super) fn reset_admission() {
    CACHE.with_borrow_mut(|cache| cache.admission = Default::default());
}

/// Coverage belongs to these exact filtered pixels, including after ID reuse.
/// Unadmitted results are scanned without retaining another pixmap or cache.
pub(super) fn bounds(id: u64, pixels: &Arc<Pixmap>) -> Option<tiny_skia::IntRect> {
    let cached = CACHE.with_borrow_mut(|cache| {
        let entry = cache.entries.get_mut(&id)?;
        if !Arc::ptr_eq(&entry.pixels, pixels) {
            return None;
        }
        Some(
            *entry
                .coverage
                .get_or_insert_with(|| super::bounded_blit::bounds(pixels.as_ref().as_ref())),
        )
    });
    cached.unwrap_or_else(|| super::bounded_blit::bounds(pixels.as_ref().as_ref()))
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
                coverage: None,
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
    fn switching_tabs_admits_new_surfaces_after_a_saturated_large_group() {
        assert_fresh_working_set(false);
    }

    #[test]
    fn committed_replacement_admits_new_surfaces_after_a_saturated_large_group() {
        assert_fresh_working_set(true);
    }

    fn assert_fresh_working_set(replace: bool) {
        let mut studio = crate::app::Studio::new();
        studio.show_welcome = false;
        studio.new_tab();
        clear();
        // Warm the same group at a small viewport before making its retained
        // input/result total 63 MiB; this avoids copying gigabytes in the test.
        for _ in 0..256 {
            render(7, source(8, 8), &FilterStack::default());
        }
        let old = render(7, source(4096, 2016), &FilterStack::default());
        let previous = Arc::downgrade(&old);
        drop(old);
        CACHE.with_borrow(|cache| assert_eq!(cache.admission.frequency(7), 255));
        let layers: Vec<_> = (1000..1050)
            .map(|id| {
                let mut layer = crate::document::Layer::group("new group");
                layer.id = id;
                layer
            })
            .collect();
        if replace {
            let mut old = crate::document::Layer::group("old group");
            old.id = 7;
            studio.doc.layers = vec![old.clone()];
            let mut commands = vec![crate::document::Cmd::RemoveLayer {
                index: 0,
                layer: old,
            }];
            commands.extend(
                layers
                    .into_iter()
                    .enumerate()
                    .map(|(index, layer)| crate::document::Cmd::AddLayer { index, layer }),
            );
            studio.commit(crate::document::Cmd::Batch(commands));
            assert!(studio.dirty);
        } else {
            studio.switch_tab(0);
            assert_eq!(studio.active_tab, 0);
            studio.doc.layers = layers;
        }
        assert!(
            previous.upgrade().is_some(),
            "activating or editing a document must preserve cached pixels"
        );
        let mut cached = Vec::new();
        for id in 1000..1050 {
            cached.push(Arc::downgrade(&render(
                id,
                source(512, 256),
                &FilterStack::default(),
            )));
        }
        assert!(
            previous.upgrade().is_none(),
            "the old large working set must yield immediately"
        );
        for (index, previous) in cached.iter().enumerate() {
            let previous = previous
                .upgrade()
                .expect("new tab must be cached after its first traversal");
            let current = render(
                1000 + index as u64,
                source(512, 256),
                &FilterStack::default(),
            );
            assert!(Arc::ptr_eq(&previous, &current));
        }
        CACHE.with_borrow(|cache| assert!(cache.bytes <= MAX_BYTES));
        clear();
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
    fn output_coverage_follows_exact_pixels_through_hits_and_reused_ids() {
        clear();
        let mut sparse = Pixmap::new(29, 23).unwrap();
        let offset = (7 * 29 + 11) * 4;
        sparse.data_mut()[offset..offset + 4].copy_from_slice(&[19, 31, 7, 63]);
        let filters = FilterStack::default();
        let first = render(8, sparse.clone(), &filters);
        CACHE.with_borrow(|cache| assert!(cache.entries[&8].coverage.is_none()));
        let expected = tiny_skia::IntRect::from_xywh(11, 7, 1, 1);
        assert_eq!(bounds(8, &first), expected);
        CACHE.with_borrow(|cache| assert_eq!(cache.entries[&8].coverage, Some(expected)));
        let repeated = render(8, sparse, &filters);
        assert!(Arc::ptr_eq(&first, &repeated));
        assert_eq!(bounds(8, &repeated), expected);

        let replacement = render(8, Pixmap::new(29, 23).unwrap(), &filters);
        assert_eq!(bounds(8, &replacement), None);
        assert_eq!(
            bounds(8, &first),
            expected,
            "old shared pixels must not use the new ID's bounds"
        );
        CACHE.with_borrow(|cache| assert_eq!(cache.entries[&8].coverage, Some(None)));
        let foreign = Arc::new(source(13, 17));
        assert_eq!(
            bounds(8, &foreign),
            super::super::bounded_blit::bounds(foreign.as_ref().as_ref())
        );
        CACHE.with_borrow(|cache| {
            assert_eq!(
                cache.entries[&8].coverage,
                Some(None),
                "an unadmitted surface must not replace cached coverage"
            )
        });
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
        let mut studio = crate::app::Studio::new();
        studio.doc.layers = (0..40)
            .map(|id| {
                let mut layer = crate::document::Layer::group("edited group");
                layer.id = id;
                layer
            })
            .collect();
        clear();
        let mut previous = Vec::new();
        for id in 0..40 {
            previous.push(Arc::downgrade(&render(
                id,
                source(512, 512),
                &FilterStack::default(),
            )));
        }
        for revision in 0..4 {
            if revision > 0 {
                let layer = &studio.doc.layers[0];
                studio.commit(crate::document::Cmd::SetLayerMeta {
                    index: 0,
                    name: layer.name.clone(),
                    visible: layer.visible,
                    locked: layer.locked,
                    opacity: 0.9 - revision as f32 * 0.1,
                    blend: layer.blend,
                    before: (
                        layer.name.clone(),
                        layer.visible,
                        layer.locked,
                        layer.opacity,
                        layer.blend,
                    ),
                });
            }
            let mut hits = 0;
            for (id, old) in previous.iter_mut().enumerate() {
                let previous = old.upgrade();
                let pixels = render(id as u64, source(512, 512), &FilterStack::default());
                hits +=
                    usize::from(previous.is_some_and(|previous| Arc::ptr_eq(&previous, &pixels)));
                *old = Arc::downgrade(&pixels);
            }
            assert!(
                hits >= 20,
                "revision {revision}: sequential over-budget traversal must retain useful filtered surfaces, got {hits} hits"
            );
        }
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
