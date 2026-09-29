//! Bounded, aging request counts for derived pixels. Exact keys matter here:
//! overestimating a cold key can evict the first resident of a sequential scene
//! and turn its next traversal into a cascade of misses.
use std::collections::HashMap;

const AGE_AFTER: usize = 1024;
const MAX_KEYS: usize = AGE_AFTER * 2;

#[derive(Default)]
pub(crate) struct Admission {
    counts: HashMap<u64, u8>,
    requests: usize,
}

impl Admission {
    pub(crate) fn reset(&mut self) {
        // Document edits/tab switches reset priorities, retaining allocation as
        // well as the caller's cached pixels for the next render.
        self.counts.clear();
        self.requests = 0;
    }

    pub(crate) fn record(&mut self, key: u64) {
        if self.requests == AGE_AFTER {
            self.counts.retain(|_, count| {
                *count /= 2;
                *count != 0
            });
            self.requests = 0;
        }
        self.requests += 1;
        let count = self.counts.entry(key).or_default();
        *count = count.saturating_add(1);
        // Every key contributes at least one to the sum of counts. Halving that
        // sum after each 1024 increments bounds both it and the key count below
        // 2048, regardless of input IDs, pointer reuse or hash collisions.
        // At most ~75 KiB of map metadata per cache, outside its pixel budget.
        debug_assert!(self.counts.len() <= MAX_KEYS);
    }

    pub(crate) fn frequency(&self, key: u64) -> u8 {
        self.counts.get(&key).copied().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_age_so_a_new_document_can_replace_old_pixels() {
        let mut admission = Admission::default();
        for _ in 0..20 {
            admission.record(1);
        }
        assert!(admission.frequency(1) > admission.frequency(2));
        for _ in 0..AGE_AFTER * 5 {
            admission.record(2);
        }
        assert!(admission.frequency(2) > admission.frequency(1));
    }

    #[test]
    fn colliding_pointer_keys_do_not_start_a_sequential_eviction_cascade() {
        // This fixed aligned-key sequence made the former four-row frequency
        // sketch miss 79 of 80 objects on the second traversal of a 63-entry
        // cache. No allocator addresses or test scheduling enter this fixture.
        let mut state = 156u64;
        let keys: Vec<_> = (0..80)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state & ((1 << 44) - 1)) * 16
            })
            .collect();
        let mut admission = Admission::default();
        let mut resident = std::collections::VecDeque::new();
        let mut visit = |key, admission: &mut Admission| {
            admission.record(key);
            if let Some(index) = resident.iter().position(|&old| old == key) {
                resident.remove(index);
                resident.push_back(key);
                return false;
            }
            if resident.len() == 63 {
                let oldest = *resident.front().unwrap();
                if admission.frequency(key) <= admission.frequency(oldest) {
                    return true;
                }
                resident.pop_front();
            }
            resident.push_back(key);
            true
        };
        for &key in &keys {
            visit(key, &mut admission);
        }
        for revision in 0..8 {
            if revision > 0 {
                admission.reset();
            }
            let misses = keys
                .iter()
                .filter(|&&key| visit(key, &mut admission))
                .count();
            assert_eq!(misses, 17, "revision {revision}");
        }
    }

    #[test]
    fn unique_requests_and_scene_resets_keep_counts_bounded() {
        let mut admission = Admission::default();
        for scene in 0..8 {
            for key in 0..AGE_AFTER * 12 {
                admission.record((scene * AGE_AFTER * 12 + key) as u64);
                if key % 3 == 0 {
                    admission.record(u64::MAX);
                }
                assert!(admission.counts.len() <= MAX_KEYS);
                assert!(
                    admission
                        .counts
                        .values()
                        .map(|&n| n as usize)
                        .sum::<usize>()
                        <= MAX_KEYS
                );
            }
            let allocated = admission.counts.capacity();
            admission.reset();
            assert!(admission.counts.is_empty());
            assert_eq!(admission.frequency(u64::MAX), 0);
            // Clearing also recovers deleted buckets, so usable capacity may
            // increase while the same table allocation is retained.
            assert!(admission.counts.capacity() >= allocated);
        }
    }
}
